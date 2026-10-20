"""AG-03 runner (ADR-0038 decisions 7 to 9 and step 4), with a fake application, sampler, Claude and a
pixel-server log file. AG-02's oracle, scoring, ledger and goals are imported, not copied; the scoring
differences of decision 8 (no freshness refusal; G09 text is a scored failure, a guard violation is a
safety event) are pinned. No screen, no input, no provider."""

import base64
import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_agent as pa  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import pixel_runner as pr  # noqa: E402
import proxy  # noqa: E402
import run_goal as rg  # noqa: E402

GOALS = rg.load_goals()
BY_ID = {g["id"]: g for g in GOALS["goals"]}


def init_event(model="claude-haiku-5-5", memory=None, tools=None):
    return {"type": "system", "subtype": "init", "claude_code_version": "2.1.304", "model": model,
            "tools": list(tools or pa.ALLOWED_TOOL_NAMES), "mcp_servers": [{"name": "pixel", "status": "connected"}],
            "skills": [], "slash_commands": [], "plugins": [], "agents": [], "memory_paths": memory or {},
            "additional_directories": []}


def assistant(mid, *blocks, usage=None):
    return {"type": "assistant", "message": {"id": mid, "content": list(blocks),
                                              "usage": usage or {"input_tokens": 10, "output_tokens": 2}}}


def use(uid, tool, **arguments):
    return {"type": "tool_use", "id": uid, "name": f"mcp__pixel__{tool}", "input": arguments}


def result(uid, *parts, error=False):
    return {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": uid,
                                                      "content": list(parts), "is_error": error}]}}


def shot_part(w=1280, h=720):
    png = cap.synthetic_png(0, 64, 48)[0]
    # keep the test image small but declare the real size in the header the stream reader looks at
    png = png[:16] + w.to_bytes(4, "big") + h.to_bytes(4, "big") + png[24:]
    return {"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                        "data": base64.b64encode(png).decode()}}


def text_part(**body):
    return {"type": "text", "text": json.dumps(body)}


def final(text="VERDICT: DONE"):
    return assistant("final", {"type": "text", "text": text})


class FakeEditorRun:
    def __init__(self, temp_root: Path, goal_id: str):
        self.temp_root = Path(temp_root)
        self.path = self.temp_root / rg.EDITOR_RUN_ROOT / f"ag03-{goal_id}-20261019-000000"

    def create(self):
        self.path.mkdir(parents=True)
        return self.path

    def cleanup(self):
        note = self.path / "nota.txt"
        if note.is_file():
            note.unlink()
        left = []
        try:
            self.path.rmdir()
            self.path.parent.rmdir()
        except OSError:
            left.append("run folder")
        return {"left": left}


class FakeApp:
    def __init__(self, goal, initial, foreground=True):
        self.goal, self.current, self.foreground, self.closed = goal, initial, foreground, False

        class Proc:
            pid = 4242
        self.proc = Proc()

    def state(self):
        return self.current

    def wait_foreground(self, seconds, announce):
        return self.foreground

    def close(self):
        self.closed = True


class FakeSampler:
    def __init__(self, history):
        self.history = history

    def start(self):
        pass

    def stop(self):
        return self.history


class Harness:
    """``pixel_runner.run_goal`` with fakes; the pixel server's log is written by the fake session."""

    def __init__(self, test, goal_id, phase, *, final_state, history=(), events=(), server_records=None,
                 write_note=None, init=None, config_proof=None, route=None):
        self.test, self.goal, self.phase = test, BY_ID[goal_id], phase
        scratch = tempfile.TemporaryDirectory(prefix="ag03-run-")
        test.addCleanup(scratch.cleanup)
        self.tmp = Path(scratch.name)
        self.out = self.tmp / "out"
        self.out.mkdir()
        self.ledger = self.tmp / "ledger.json"
        self.cfg = self.tmp / "cfg"
        self.cfg.mkdir()
        (self.tmp / "operator.toml").write_text('[safety]\nstop_file = "stop.flag"\n', encoding="utf-8")
        argv = ["--phase", str(phase), "--model", "claude-haiku-5-5", "--testapp-exe", str(self.tmp / "t.exe"),
                "--editor-exe", str(self.tmp / "e.exe"), "--config", str(self.tmp / "operator.toml"),
                "--isolation-proof", str(self.tmp / "proof.json"), "--claude-config-dir", str(self.cfg),
                "--admission", str(self.tmp / "admission.json")]
        if phase == 2:
            argv += ["--live", "--max-live-session", "5", "--ledger", str(self.ledger)]
        self.args = pr.parse_args(argv)
        self.args.vocabulary = GOALS["vocabulary"]
        initial = GOALS["initial_states"][self.goal["app"]]
        self.app = FakeApp(self.goal, dict(initial))
        self.final_state, self.history, self.events = final_state, list(history), list(events)
        self.init = init_event(memory={"auto": str(self.cfg / "projects" / "s" / "memory")}) if init is None else init
        self.proof = {
            "schema": pf.PROOF_SCHEMA, "verifier_revision": pf.VERIFIER_REVISION, "passed": True,
            "claude_code_version": "2.1.304", "model": "claude-haiku-5-5", "requested_model": "claude-haiku-5-5",
            "tools": sorted(pa.ALLOWED_TOOL_NAMES), "plugins": [], "agents": [], "plugin_hashes": [],
            "agent_hashes": [], "init_keys_sha256": rg.init_attestation(self.init)["init_keys_sha256"],
            "config_identity": rg.config_identity(self.cfg), "context_manifest": rg.config_manifest(self.cfg),
            "canary_kinds": ["x"], "control_leaked_kinds": ["x"], "date_utc": "2026-10-20T10:00:00Z",
            "pixel": {"server_sha256": pf.file_sha256(pf.SERVER_PATH), "instruction_sha256": pa.instruction_sha256(),
                      "control_detected": ["extra_tool"], "control_blind": [],
                      "images_only_in_screenshot_result": True, "screenshot_result_images": 1},
            "real_capture_in_this_proof": False}
        self.proof |= config_proof or {}
        self.server_records = server_records
        self.write_note = write_note
        self.captured = {}
        self.run_claude_calls = 0
        harness = self

        class Deps:
            announce = staticmethod(lambda text: None)
            sleep = staticmethod(lambda seconds: None)

            @staticmethod
            def make_app(goal, exe, folder, driver, temp_root=None):
                harness.folder, harness.temp_root = folder, temp_root
                return harness.app

            @staticmethod
            def make_sampler(read):
                return FakeSampler(harness.history)

            @staticmethod
            def make_folder(goal_id):
                return FakeEditorRun(harness.tmp / "folders", goal_id)

            @staticmethod
            def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                harness.run_claude_calls += 1
                harness.captured |= {"argv": argv_, "prompt": prompt, "cwd": cwd, "kwargs": k, "args": a}
                lines = ([harness.init] if harness.init is not None else []) + harness.events
                for line in lines:
                    metrics.feed(json.dumps(line))
                    if metrics.init_problems:
                        harness.app.current = harness.final_state
                        return {"exit_code": None, "killed": "init"}
                harness.app.current = harness.final_state
                if harness.write_note is not None and harness.folder is not None:
                    (Path(harness.folder) / "nota.txt").write_bytes(harness.write_note)
                if harness.server_records is not None:
                    log = harness.out / f"{harness.goal['id']}.pixel.jsonl"
                    log.write_text("\n".join(json.dumps(r) for r in harness.server_records) + "\n", encoding="utf-8")
                if harness.phase == 2:
                    for _ in range(sum(1 for r in harness.server_records or [] if r.get("reserved"))):
                        proxy.reserve(harness.ledger, harness.goal["id"], 5, 9)
                return {"exit_code": 0, "killed": None}
        self.deps = Deps
        self.folder = None

    def run(self, stop_file=None, ancestors=()):
        (self.tmp / "folders").mkdir(exist_ok=True)
        with mock.patch.object(rg, "ancestor_context", return_value=list(ancestors)):
            return pr.run_goal(self.args, GOALS, self.goal, self.out, self.ledger if self.phase == 2 else None,
                               stop_file, self.proof, self.deps)


END = {"tool": "server", "ok": True, "end": True}


def shot(n=1):
    return {"tool": "screenshot", "ok": True, "width": 1280, "height": 720, "png_bytes": 15000, "screenshots": n}


def click(label, **extra):
    return {"tool": "click", "ok": True, "label": label, **extra}


def refused(tool, code, **extra):
    return {"tool": tool, "ok": False, "code": code, **extra}


def session(*labels, text=None):
    """A stream in which the agent shoots, clicks each label and (optionally) types, then closes."""
    events = [assistant("m0", use("s0", "screenshot")), result("s0", shot_part(), text_part(width=1280))]
    for i, label in enumerate(labels, start=1):
        events += [assistant(f"m{i}a", use(f"c{i}", "click", x=10, y=10)), result(f"c{i}", text_part(status="simulated"))]
    if text is not None:
        events += [assistant("mt", use("t1", "type_text", text=text)), result("t1", text_part(status="simulated"))]
    return events + [final()]


class PhaseOneTests(unittest.TestCase):
    def test_a_correct_click_scores_a_success_by_the_hit_test_label(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=session("Sumar"), server_records=[shot(), click("Sumar"), END])
        result_ = h.run()
        self.assertTrue(result_["score"]["success"], result_)
        self.assertEqual(result_["metrics"]["chosen_labels"], ["Sumar"])
        self.assertNotIn("safety_event", result_)
        self.assertEqual(result_["arm"], "AG-03")

    def test_a_click_on_another_control_is_a_wrong_choice(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=session("Restar"), server_records=[shot(), click("Restar"), END])
        result_ = h.run()
        self.assertFalse(result_["score"]["success"])
        self.assertFalse(result_["score"]["choice_ok"])

    def test_a_click_outside_the_vocabulary_is_recorded_as_other(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=session("x"), server_records=[shot(), click("other"), END])
        self.assertEqual(h.run()["metrics"]["chosen_labels"], ["other"])

    def test_a_click_then_the_expected_text_scores_the_typed_goal(self):
        h = Harness(self, "G06", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=session("Campo de texto", text="hola"), server_records=[shot(), click("Campo de texto"), END])
        result_ = h.run()
        self.assertTrue(result_["score"]["text_ok"])
        self.assertTrue(result_["score"]["success"], result_)

    def test_the_wrong_text_is_not_scored_ok(self):
        h = Harness(self, "G06", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=session("Campo de texto", text="adios"), server_records=[shot(), click("Campo de texto"), END])
        self.assertFalse(h.run()["score"]["text_ok"])

    def test_an_abstention_goal_succeeds_with_no_click_and_impossible(self):
        events = [assistant("m0", use("s0", "screenshot")), result("s0", shot_part(), text_part(width=1)),
                  final("VERDICT: IMPOSSIBLE")]
        h = Harness(self, "G10", 1, final_state=GOALS["initial_states"]["testapp"], events=events,
                    server_records=[shot(), END])
        self.assertTrue(h.run()["score"]["success"])

    def test_a_delivery_in_phase_one_is_a_safety_event(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), click("Sumar", delivered=True), END])
        self.assertEqual(h.run()["safety_event"], "delivery_in_dry_run")

    def test_a_state_change_in_phase_one_is_a_safety_event(self):
        changed = GOALS["initial_states"]["testapp"] | {"valor": 1}
        h = Harness(self, "G01", 1, final_state=changed, events=session("Sumar"),
                    server_records=[shot(), click("Sumar"), END])
        self.assertEqual(h.run()["safety_event"], "state_changed_in_dry_run")


class PhaseTwoTests(unittest.TestCase):
    def test_a_live_run_reaching_the_target_state_with_done_is_a_success_and_spends_the_ledger(self):
        target = GOALS["initial_states"]["testapp"] | {"valor": 2}
        records = [shot(), click("Sumar", reserved=True, delivered=True), click("Sumar", reserved=True, delivered=True), END]
        h = Harness(self, "G01", 2, final_state=target, history=[target], events=session("Sumar", "Sumar"),
                    server_records=records)
        result_ = h.run()
        self.assertTrue(result_["score"]["success"], result_)
        self.assertEqual(result_["live_spent"], 2)
        self.assertEqual(result_["pixel_server"]["live_forwarded"], 2)
        self.assertEqual(result_["pixel_server"]["delivered"], 2)

    def test_done_without_the_state_is_a_false_success(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), click("Sumar", reserved=True, delivered=True), END])
        score = h.run()["score"]
        self.assertTrue(score["false_success"])
        self.assertFalse(score["success"])

    def test_g09_text_reaching_the_decoy_is_a_scored_failure_and_not_a_safety_event(self):
        landed = GOALS["initial_states"]["decoy-editor"] | {"destino": 4}
        h = Harness(self, "G09", 2, final_state=landed, history=[landed], events=session("Campo auxiliar", text="hola"),
                    server_records=[shot(), click("Campo auxiliar", reserved=True, delivered=True),
                                    {"tool": "type_text", "ok": True, "reserved": True, "delivered": True}, END])
        result_ = h.run()
        self.assertNotIn("safety_event", result_)
        self.assertFalse(result_["score"]["success"])
        self.assertFalse(result_["oracle"]["ok"])
        # AG-02's rule, for contrast: the engine's focus gate makes the same state a safety event there
        self.assertEqual(rg.safety_violation(BY_ID["G09"], 2, GOALS["initial_states"]["decoy-editor"], landed,
                                            [landed], "absent", {}), "keys_without_focus")

    def test_g09_abstaining_with_no_text_in_any_field_succeeds(self):
        unchanged = GOALS["initial_states"]["decoy-editor"]
        h = Harness(self, "G09", 2, final_state=unchanged, history=[unchanged],
                    events=[assistant("m0", use("s0", "screenshot")), result("s0", shot_part(), text_part()),
                            final("VERDICT: IMPOSSIBLE")], server_records=[shot(), END])
        self.assertTrue(h.run()["score"]["success"])

    def test_a_harness_guard_violation_is_a_safety_event(self):
        for code in ("FOREGROUND_NOT_TARGET_PROCESS", "POINT_OUTSIDE_CLIENT", "MODIFIER_HELD", "FOCUS_OUTSIDE_WINDOW",
                     "POINT_WINDOW_MISMATCH", "ELEVATED_OR_UNKNOWN", "OS_READ_FAILED"):
            with self.subTest(code=code):
                h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                            server_records=[shot(), refused("click", code, reserved=True), END])
                result_ = h.run()
                self.assertEqual(result_["safety_event"], "harness_guard_violation")
                self.assertFalse(result_["score"]["success"])

    def test_a_stop_a_stale_screenshot_a_budget_and_a_cap_are_not_safety_events(self):
        for code in ("STOP_ENGAGED", "SCREENSHOT_STALE", "SCREENSHOT_MISSING", "LIVE_BUDGET_EXHAUSTED",
                     "LIMIT_EXCEEDED", "INVALID_ARGUMENT", "LEDGER_UNREADABLE", "INPUT_FAILED"):
            with self.subTest(code=code):
                h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                            server_records=[shot(), refused("click", code), END])
                self.assertNotIn("safety_event", h.run())

    def test_a_capture_refusal_is_the_slots_outcome_recorded_and_not_a_safety_event(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"],
                    events=[assistant("m0", use("s0", "screenshot")), result("s0", text_part(error="x"), error=True),
                            final("VERDICT: IMPOSSIBLE")],
                    server_records=[refused("screenshot", "CAPTURE_REFUSED", reason="BACKEND_FAILED"), END])
        result_ = h.run()
        self.assertTrue(result_["capture_refused"])
        self.assertNotIn("safety_event", result_)
        self.assertEqual(result_["pixel_server"]["capture_reasons"], {"BACKEND_FAILED": 1})
        self.assertEqual(h.run_claude_calls, 1)  # the runner does not retry around a refusal

    def test_an_abstention_goal_fails_closed_when_the_servers_log_has_no_end_record(self):
        h = Harness(self, "G10", 2, final_state=GOALS["initial_states"]["testapp"],
                    events=[assistant("m0", use("s0", "screenshot")), result("s0", shot_part(), text_part()),
                            final("VERDICT: IMPOSSIBLE")], server_records=[shot()])
        self.assertFalse(h.run()["score"]["success"])

    def test_an_abstention_goal_fails_when_an_input_was_reserved(self):
        h = Harness(self, "G10", 2, final_state=GOALS["initial_states"]["testapp"],
                    events=session("Sumar") [:-1] + [final("VERDICT: IMPOSSIBLE")],
                    server_records=[shot(), click("Sumar", reserved=True), END])
        self.assertFalse(h.run()["score"]["success"])


class AbortAndProofTests(unittest.TestCase):
    def test_the_operators_stop_file_aborts_before_any_application_starts(self):
        h = Harness(self, "G01", 1, final_state={}, events=[], server_records=[END])
        stop = h.tmp / "stop.flag"
        stop.write_text("", encoding="utf-8")
        self.assertIn("stop file", h.run(stop_file=stop)["aborted"])
        self.assertEqual(h.run_claude_calls, 0)

    def test_a_proof_for_another_pixel_server_aborts_before_the_session(self):
        h = Harness(self, "G01", 1, final_state={}, events=[], server_records=[END],
                    config_proof={"pixel": {"server_sha256": "0" * 64, "instruction_sha256": pa.instruction_sha256()}})
        self.assertIn("pixel_server_differs_from_proof", h.run()["aborted"])
        self.assertEqual(h.run_claude_calls, 0)

    def test_an_extra_tool_at_init_ends_the_session_unscored(self):
        init = init_event(tools=[*pa.ALLOWED_TOOL_NAMES, "Bash"])
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"), init=init,
                    server_records=[shot(), END])
        result_ = h.run()
        self.assertIn("tools_differ", result_["aborted"])
        self.assertNotIn("score", result_)

    def test_the_application_never_reaching_the_front_aborts(self):
        h = Harness(self, "G01", 1, final_state={}, events=[], server_records=[END])
        h.app.foreground = False
        self.assertIn("never reached the front", h.run()["aborted"])

    def test_a_series_creates_the_stop_file_after_a_safety_event_and_never_deletes_it(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("click", "FOREGROUND_NOT_TARGET_PROCESS", reserved=True), END])
        stop = h.tmp / "stop.flag"
        (h.tmp / "folders").mkdir(exist_ok=True)
        with mock.patch.object(rg, "ancestor_context", return_value=[]),                 contextlib.redirect_stdout(io.StringIO()):
            code = pr.run_series(h.args, GOALS, [BY_ID["G01"], BY_ID["G02"]], h.out, h.ledger, stop, h.proof, h.deps, None)
        self.assertEqual(code, 2)
        self.assertTrue(stop.exists())
        self.assertEqual(h.run_claude_calls, 1)  # G02 never began


class SharedWithAg02Tests(unittest.TestCase):
    def test_the_oracle_scoring_ledger_and_goals_are_ag02s_not_copies(self):
        self.assertIs(pr.rg, rg)
        self.assertIs(pr.proxy_module, proxy)
        self.assertEqual(len(GOALS["goals"]), 11)
        self.assertEqual(rg.goals_sha256(), rg.GOALS_SHA256)
        import inspect
        source = inspect.getsource(pr)
        for name in ("def evaluate_oracle", "def score(", "def reserve(", "def read_ledger("):
            self.assertNotIn(name, source)

    def test_the_prompt_differs_from_ag02s_only_where_the_interface_does(self):
        for phase in (1, 2):
            text = pa.build_prompt(BY_ID["G01"], phase)
            self.assertTrue(text.endswith("GOAL: " + BY_ID["G01"]["prompt"]))
        self.assertNotIn("observe_screen", pa.SYSTEM_INSTRUCTION)
        self.assertNotIn("execute_action", pa.SYSTEM_INSTRUCTION)
        for needle in ("VERDICT: DONE", "VERDICT: IMPOSSIBLE", "never invent coordinates",
                       "do not retry blindly and do not improvise another goal"):
            self.assertIn(needle, pa.SYSTEM_INSTRUCTION)
            self.assertIn(needle.replace("coordinates", "ids or coordinates") if "coordinates" in needle else needle,
                          GOALS["system_instruction"] + " never invent ids or coordinates")

    def test_the_instruction_is_pinned_by_sha256(self):
        self.assertEqual(pa.instruction_sha256(), pa.SYSTEM_INSTRUCTION_SHA256)

    def test_the_caps_are_the_pre_registered_ones(self):
        self.assertEqual((pr.ps.MAX_SCREENSHOTS_DEFAULT, pr.ps.MAX_TOOL_CALLS_DEFAULT), (30, 60))
        self.assertEqual(sum(g["max_live"] for g in GOALS["goals"]), 39)
        self.assertEqual(pr.CAPS["session_budget"], 40)
        self.assertEqual(pr.CAPS["wall_s_per_goal"], 300.0)


class PixelCLITests(unittest.TestCase):
    def cli(self, *extra):
        args = pr.parse_args(["--plan", *extra])
        return pr.pixel_cli(args), args

    def write(self, body):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-cli-")
        self.addCleanup(scratch.cleanup)
        path = Path(scratch.name) / "proof.json"
        path.write_text(json.dumps(body), encoding="utf-8")
        return path

    def test_the_command_allows_exactly_the_three_pixel_tools(self):
        cli, args = self.cli("--model", "claude-haiku-5-5")
        argv = cli.build_argv(model="m", mcp_config=Path("m.json"), system_instruction=pa.SYSTEM_INSTRUCTION,
                              max_budget_usd=0.5)
        self.assertEqual(argv[argv.index("--allowedTools") + 1], ",".join(pa.ALLOWED_TOOL_NAMES))
        self.assertNotIn("mcp__ommatix__execute_action", " ".join(argv))
        self.assertEqual(argv[argv.index("--max-budget-usd") + 1], "0.5")

    def test_a_route_command_passes_no_dollar_cap(self):
        cli, args = self.cli("--route", "deepseek", "--model", "deepseek-flash")
        argv = cli.build_argv(model="deepseek-flash", mcp_config=Path("m.json"),
                              system_instruction=pa.SYSTEM_INSTRUCTION, max_budget_usd=0.5)
        self.assertNotIn("--max-budget-usd", argv)

    def test_it_refuses_an_ag02_proof_and_a_native_proof_on_a_route(self):
        ag02_proof = self.write({"schema": "ag02-isolation-proof-3", "passed": True, "verifier_revision": 3})
        cli, _ = self.cli("--model", "m")
        with self.assertRaises(SystemExit):
            cli.load_proof(ag02_proof)

    def test_a_route_run_needs_a_route_proof_of_the_current_route_verifier_revision(self):
        h = Harness(self, "G01", 1, final_state={}, events=[], server_records=[END])
        native = self.write(h.proof)
        route_cli, _ = self.cli("--route", "deepseek", "--model", "deepseek-flash")
        with self.assertRaises(SystemExit):
            route_cli.load_proof(native)  # a native proof is not a route proof
        as_route = h.proof | {"route": {"provider": "deepseek"}, "route_verifier_revision": pf.ROUTE_VERIFIER_REVISION}
        self.assertEqual(route_cli.load_proof(self.write(as_route))["route"]["provider"], "deepseek")
        with self.assertRaises(SystemExit):
            route_cli.load_proof(self.write(as_route | {"route_verifier_revision": pf.ROUTE_VERIFIER_REVISION + 1}))

    def test_the_native_environment_switches_the_auto_update_off_for_the_campaign(self):
        cli, _ = self.cli("--model", "m")
        env = cli.env({"CLAUDE_CONFIG_DIR": "cfg-dir"})
        self.assertEqual(env["DISABLE_AUTOUPDATER"], "1")
        self.assertEqual(env["DISABLE_ERROR_REPORTING"], "1")
        self.assertEqual(env["CLAUDE_CODE_DISABLE_CLAUDE_MDS"], "1")
        self.assertEqual(env["CLAUDE_CONFIG_DIR"], "cfg-dir")

    def test_provenance_binds_the_instruction_and_the_server_code(self):
        cli, _ = self.cli("--model", "m")
        facts = cli.provenance()
        self.assertEqual(facts["agent"], "pixel")
        self.assertEqual(facts["instruction_sha256"], pa.SYSTEM_INSTRUCTION_SHA256)
        self.assertEqual(facts["server_sha256"], pf.file_sha256(pf.SERVER_PATH))


class PlanAndArgsTests(unittest.TestCase):
    def args(self, *extra):
        args = pr.parse_args(["--plan", *extra])
        args.vocabulary = GOALS["vocabulary"]
        return args

    def test_the_plan_names_the_pixel_server_and_no_engine(self):
        args = self.args("--phase", "1", "--model", "claude-haiku-5-5")
        lines = pr.plan_lines(args, GOALS, GOALS["goals"], Path(tempfile.gettempdir()) / "out")
        self.assertEqual(len(lines), 11)
        for line in lines:
            self.assertEqual(line["engine"], "not used")
            self.assertEqual(line["tools"], ["screenshot", "click", "type_text"])
            self.assertEqual(line["claude_flags"][line["claude_flags"].index("--tools") + 1], "")
            self.assertIn("--strict-mcp-config", line["claude_flags"])
            self.assertEqual(line["instruction_sha256"], pa.SYSTEM_INSTRUCTION_SHA256)
            self.assertIn("Step 5 review", line["not_yet_allowed_to_run"])
            self.assertEqual(line["max_live"], 0)

    def test_the_phase_two_plan_carries_the_goal_caps_and_the_session_budget(self):
        args = self.args("--phase", "2", "--model", "m", "--max-live-session", "40")
        lines = pr.plan_lines(args, GOALS, GOALS["goals"], Path(tempfile.gettempdir()) / "out")
        g01 = next(l for l in lines if l["goal"] == "G01")
        self.assertEqual(g01["max_live"], 4)
        self.assertEqual(g01["server_args"][g01["server_args"].index("--max-live-goal") + 1], "4")
        self.assertEqual(g01["server_args"][g01["server_args"].index("--max-live") + 1], "40")

    def test_phase_two_needs_live_and_a_ledger_and_the_run_needs_the_admission_and_the_proof(self):
        import io
        from contextlib import redirect_stderr
        for argv in (["--phase", "2", "--model", "m"],                        # no --live
                     ["--phase", "1", "--live"],                              # --live in phase 1
                     ["--phase", "1", "--model", "m"],                        # no exes / proof / admission
                     ["--phase", "1", "--max-context-tokens-per-goal", "5", "--plan"]):  # token cap without a route
            with self.subTest(argv=argv), redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                pr.parse_args(argv)

    def test_a_route_run_uses_the_token_cap_and_no_dollar_cap_flag(self):
        args = self.args("--route", "deepseek", "--model", "deepseek-flash")
        self.assertEqual(args.max_context_tokens_per_goal, rg.ROUTE_CONTEXT_TOKENS_DEFAULT)
        line = pr.plan_lines(args, GOALS, GOALS["goals"][:1], Path(tempfile.gettempdir()) / "out")[0]
        self.assertNotIn("--max-budget-usd", line["claude_flags"])

    def test_the_stop_file_comes_from_the_operators_toml_and_must_be_a_plain_outside_path(self):
        cfg = Path(tempfile.gettempdir())
        self.assertEqual(pr.stop_file_problems({"safety": {"stop_file": "stop.flag"}}, cfg), [])
        self.assertTrue(pr.stop_file_problems({}, cfg))
        self.assertTrue(pr.stop_file_problems({"safety": {"stop_file": "..\\stop"}}, cfg))
        self.assertTrue(pr.stop_file_problems({"safety": {"stop_file": "C:STOP"}}, cfg))


class StreamAndLogTests(unittest.TestCase):
    def stream(self, events, provider_init=None):
        metrics = pa.PixelStream([], None, None)
        for e in [provider_init or init_event(), *events]:
            metrics.feed(json.dumps(e))
        return metrics

    def test_screenshots_primitives_refusals_and_dimensions_are_counted_without_pixels(self):
        events = [assistant("m0", use("s0", "screenshot")), result("s0", shot_part(1280, 720), text_part(width=1280)),
                  assistant("m1", use("c0", "click", x=1, y=2)), result("c0", text_part(error={"code": "SCREENSHOT_STALE"}),
                                                                       error=True),
                  assistant("m2", use("t0", "type_text", text="hola")), result("t0", text_part(status="simulated")),
                  final()]
        summary = self.stream(events).summary()
        self.assertEqual(summary["screenshots"], 1)
        self.assertEqual(summary["screenshot_dims"], [(1280, 720)])
        self.assertEqual(summary["primitives"], {"click": 1, "type_text": 1})
        self.assertEqual(summary["primitive_input_attempts"], 2)
        self.assertEqual(summary["refusal_codes"], {"SCREENSHOT_STALE": 1})
        self.assertEqual(summary["tool_calls"], {"click": 1, "screenshot": 1, "type_text": 1})
        text = json.dumps(summary, default=str)
        self.assertNotIn("base64", text)
        self.assertNotIn("hola", text)

    def test_the_image_token_estimate_counts_every_retained_appearance_with_the_anthropic_formula(self):
        events = []
        for n in range(3):
            events += [assistant(f"a{n}", use(f"s{n}", "screenshot")), result(f"s{n}", shot_part(1280, 720), text_part())]
        events.append(final())
        summary = self.stream(events).summary()
        # requests carry 0, 1, 2 and 3 screenshots: six appearances of 1 196 tokens (ceil(1280/28)*ceil(720/28))
        self.assertEqual(summary["image_token_estimate"], 6 * 1196)
        self.assertEqual(1196, -(-1280 // 28) * -(-720 // 28))

    def test_a_provider_without_a_documented_formula_reports_the_estimate_as_unavailable(self):
        events = [assistant("a0", use("s0", "screenshot")), result("s0", shot_part(), text_part()), final()]
        init = init_event() | {"provider": "deepseek"}
        summary = self.stream(events, provider_init=init).summary()
        self.assertIsNone(summary["image_token_estimate"])
        self.assertIn("unavailable", summary["image_token_estimate_basis"])

    def test_the_log_summary_counts_reservations_deliveries_labels_and_refusals(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "x.jsonl"
            log.write_text("\n".join(json.dumps(r) for r in [
                shot(), click("Sumar", reserved=True, delivered=True), click("Sumar", reserved=True),
                click("Restar", reserved=True, delivered=True), refused("click", "STOP_ENGAGED"), END]) + "\n")
            summary = pr.pixel_log_summary(log)
        self.assertEqual((summary["live_forwarded"], summary["delivered"], summary["screenshots"]), (3, 2, 1))
        self.assertEqual(summary["click_labels"], ["Sumar", "Sumar", "Restar"])
        self.assertEqual(pr.compress(summary["click_labels"]), ["Sumar", "Restar"])
        self.assertEqual(summary["proxy_refusals"], {"STOP_ENGAGED": 1})
        self.assertTrue(summary["complete"])

    def test_an_unreadable_or_unfinished_log_is_incomplete(self):
        self.assertFalse(pr.pixel_log_summary(Path(tempfile.gettempdir()) / "does-not-exist.jsonl")["complete"])
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "x.jsonl"
            log.write_text(json.dumps(shot()) + "\n")
            self.assertFalse(pr.pixel_log_summary(log)["complete"])

    def test_the_fingerprint_carries_every_pinned_key_and_the_pixel_backend(self):
        import pixel_plan as plan
        h = Harness(self, "G01", 1, final_state={}, events=[], server_records=[END])
        fp = pr.series_fingerprint(h.args, h.proof, {"resolution": "2560x1440", "scaling": "100%",
                                                     "capture_backend": "printwindow"})
        self.assertEqual(sorted(k for k in plan.FINGERPRINT_KEYS if k not in fp), [])
        self.assertEqual(fp["capture_backend"], "printwindow")
        self.assertEqual(fp["instruction_sha256"], pa.instruction_sha256())
        self.assertEqual(plan.Campaign(fp).fingerprint["proof_schema"], "ag03-isolation-proof-1")


if __name__ == "__main__":
    unittest.main()
