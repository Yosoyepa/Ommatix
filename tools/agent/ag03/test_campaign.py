"""R7 of the review of PR 101: the production entry point drives the slot, plan, fingerprint and gate machinery.

``pixel_runner.main`` is called with fake dependencies (a fake application, sampler and Claude, a scripted
pixel-server log); the assertions are on the order of execution AND on the durable artefacts and counts
(`series.json`, one directory per attempt, the campaign pool, the not-pooled record). No screen, no input,
no provider."""

import contextlib
import io
import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_agent as pa  # noqa: E402
import pixel_gate as gate  # noqa: E402
import pixel_plan as plan  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import pixel_runner as pr  # noqa: E402
import run_goal as rg  # noqa: E402
from test_runner import (END, GOALS, BY_ID, FakeApp, FakeEditorRun, FakeSampler, click, final, init_event,  # noqa: E402
                         refused, session, shot)

PRE = {"privacy_reading_recorded": True, "oi1_recorded": True, "owner_go_ahead": True}
GOOD_ANSWER = "\n".join(f"IMAGE {n + 1}: WORD={w} SHAPE={s}" for n, (w, s) in enumerate(gate.expected_answers()))


class Series:
    """One call of ``pixel_runner.main`` with fakes. ``script`` maps ``(goal, attempt)`` to a behaviour:
    ``ok`` (default), ``no_front`` (the application never reaches the front: a pre-call harness fault),
    ``timeout`` (the session is killed after DONE), ``safety``, ``operator_stop``, ``capture_refused``."""

    def __init__(self, test, *, root=None, phase=1, rep=1, model="Haiku", route=None, script=None, gate_status="passed",
                 extra=(), model_id=None, gate_requested=None, gate_resolved=None, gate_base_url=None, display=None,
                 resolved_id=None):
        if root is None:
            scratch = tempfile.TemporaryDirectory(prefix="ag03-campaign-")
            test.addCleanup(scratch.cleanup)
            root = Path(scratch.name)
        self.test, self.root, self.phase, self.rep, self.script = test, root, phase, rep, script or {}
        self.name = {"anthropic": "Haiku", "deepseek": "DeepSeek", "mimo": "MiMo"}[route or "anthropic"]
        self.calls: list[tuple[str, int]] = []   # (goal, attempt) in the order the sessions ran
        self.app_calls: dict[str, int] = {}
        (root / "cfg").mkdir(exist_ok=True)
        (root / "operator.toml").write_text('[safety]\nstop_file = "stop.flag"\n', encoding="utf-8")
        self.stop = root / "stop.flag"
        import agentcli
        model_id = model_id or (agentcli.CLAUDE_ROUTES[route]["model"] if route else "claude-haiku-5-5")
        proof_model = resolved_id or model_id  # the attested (resolved) identity
        self.model_id = model_id
        self.display = display if display is not None else {"resolution": "2560x1440", "scaling": 100,
                                                          "capture_backend": "printwindow", "dpi_context": "per_monitor_v2"}
        route_url = agentcli.CLAUDE_ROUTES[route]["base_urls"][0] if route else None
        gate_file = root / f"gate-{self.name}.json"
        provider = route or "anthropic"
        gate_file.write_text(json.dumps(
            gate.gate_record(self.name, provider, preflight_passed=gate_status != "failed_preflight", preconditions=PRE,
                             answer=GOOD_ANSWER if gate_status == "passed" else "no idea",
                             provider_error="provider_rejected_image" if gate_status == "rejected" else None,
                             requested_model=gate_requested or model_id, resolved_model=gate_resolved or proof_model,
                             route_base_url=gate_base_url if gate_base_url is not None else route_url)),
            encoding="utf-8")
        self.init = init_event(model=proof_model, memory={"auto": str(root / "cfg" / "projects" / "s" / "memory")})
        proof = {
            "schema": pf.PROOF_SCHEMA, "verifier_revision": pf.VERIFIER_REVISION, "passed": True,
            "claude_code_version": "2.1.304", "model": proof_model, "requested_model": model_id,
            "tools": sorted(pa.ALLOWED_TOOL_NAMES), "plugins": [], "agents": [], "plugin_hashes": [],
            "agent_hashes": [], "init_keys_sha256": rg.init_attestation(self.init)["init_keys_sha256"],
            "config_identity": rg.config_identity(root / "cfg"), "context_manifest": rg.config_manifest(root / "cfg"),
            "canary_kinds": ["x"], "control_leaked_kinds": ["x"], "date_utc": "2026-10-20T10:00:00Z",
            "pixel": {"server_sha256": pf.file_sha256(pf.SERVER_PATH), "instruction_sha256": pa.instruction_sha256(),
                      "control_detected": ["extra_tool"], "control_blind": [],
                      "images_only_in_screenshot_result": True, "screenshot_result_images": 1},
            "real_capture_in_this_proof": False}
        if route:
            proof |= {"route": {"provider": route, "base_url": route_url, "model": model_id, "effort": "max",
                                "auth": "bearer"}, "route_verifier_revision": pf.ROUTE_VERIFIER_REVISION}
            (root / "key.txt").write_text("sk-test-0000000000000000", encoding="utf-8")
        (root / "proof.json").write_text(json.dumps(proof), encoding="utf-8")
        for exe in ("t.exe", "e.exe"):
            (root / exe).write_bytes(b"fake binary " + exe.encode())
        self.argv = ["--phase", str(phase), "--repetition", str(rep), "--model", model_id,
                     "--testapp-exe", str(root / "t.exe"), "--editor-exe", str(root / "e.exe"),
                     "--config", str(root / "operator.toml"), "--isolation-proof", str(root / "proof.json"),
                     "--claude-config-dir", str(root / "cfg"), "--admission", str(root / "admission.json"),
                     "--gate-record", str(gate_file), "--campaign-dir", str(root / "campaign"),
                     "--out", str(root / "out")]
        if phase == 2:
            self.argv += ["--live", "--max-live-session", "40", "--ledger", str(root / f"ledger-{rep}.json")]
        if route:
            self.argv += ["--route", route, "--route-base-url", route_url, "--key-file", str(root / "key.txt")]
        self.argv += list(extra)
        harness = self

        class Deps:
            announce = staticmethod(lambda text: None)
            sleep = staticmethod(lambda seconds: None)

            @staticmethod
            def make_app(goal, exe, folder, driver, temp_root=None):
                n = harness.app_calls[goal["id"]] = harness.app_calls.get(goal["id"], 0) + 1
                behaviour = harness.script.get((goal["id"], n), "ok")
                app = FakeApp(goal, dict(GOALS["initial_states"][goal["app"]]), foreground=behaviour != "no_front")
                app.attempt = n
                harness.current = app
                return app

            @staticmethod
            def make_sampler(read):
                return FakeSampler([])

            @staticmethod
            def make_folder(goal_id):
                return FakeEditorRun(harness.root / "folders", goal_id)

            @staticmethod
            def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                mcp = Path(argv_[argv_.index("--mcp-config") + 1])
                attempt_dir, goal_id = mcp.parent, mcp.name.split(".")[0]
                n = int(attempt_dir.name.removeprefix("attempt"))
                harness.calls.append((goal_id, n))
                behaviour = harness.script.get((goal_id, n), "ok")
                goal = BY_ID[goal_id]
                if behaviour == "crash":
                    raise RuntimeError("the runner process died")
                events, records = harness.events_for(goal, behaviour)
                lines = events if behaviour.startswith("noinit") else [harness.init, *events]
                if behaviour == "bad_init":
                    lines = [harness.init | {"tools": [*harness.init["tools"], "Bash"]}, *events]
                for line in lines:
                    metrics.feed(json.dumps(line))
                    if metrics.init_problems:  # the real runner ends the session at once
                        break
                log_path = attempt_dir / f"{goal_id}.pixel.jsonl"
                if behaviour != "nolog":
                    log_path.write_text("\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8")
                if behaviour == "log_corrupt":
                    log_path.write_text(log_path.read_text(encoding="utf-8") + "this is not json\n", encoding="utf-8")
                if behaviour == "log_denied":
                    harness.deny_path = log_path  # the final summary cannot read it (see Series.run)
                if behaviour == "block":
                    harness.started.set()
                    harness.release.wait(60)
                if behaviour == "safety_race":
                    harness.stop.write_bytes(b"operator-marker")
                hook = k.get("should_stop")
                if hook is not None and ("safety" in behaviour or "uncertain" in behaviour or "operator" in behaviour):
                    reason = hook()
                    if reason:
                        if getattr(hook, "on_stop", None) is not None:  # the gate applies the effect, as run_claude does
                            hook.on_stop(reason)
                        return {"exit_code": None, "killed": reason}
                if behaviour == "timeout":
                    return {"exit_code": -1, "killed": "timeout"}
                return {"exit_code": 0, "killed": None}
        self.deps = Deps
        self.deny_path = None
        self.started, self.release = threading.Event(), threading.Event()

    def events_for(self, goal, behaviour):
        labels, text = goal["choice"]["labels"], goal["choice"]["type_text"]
        events = session(*labels, text=text)
        if goal.get("phase1_verdict") == "impossible":
            events = events[:-1] + [final("VERDICT: IMPOSSIBLE")]
        records = [shot(), *[click(label) for label in labels]]
        if behaviour in ("log_denied", "log_corrupt"):
            records.append(refused("click", "FOREGROUND_NOT_TARGET_PROCESS", reserved=True, terminal="safety"))
        if behaviour in ("safety", "safety_race", "noinit_safety"):
            records.append(refused("click", "FOREGROUND_NOT_TARGET_PROCESS", reserved=True, terminal="safety"))
        elif behaviour in ("operator_stop", "noinit_operator_stop"):
            records.append(refused("click", "STOP_ENGAGED", terminal="operator_stop"))
        elif behaviour == "noinit_uncertain":
            records.append(refused("click", "INPUT_FAILED", reserved=True, delivered="uncertain", events_sent=1,
                                   terminal="uncertain_delivery"))
        elif behaviour == "capture_refused":
            records.append(refused("screenshot", "CAPTURE_REFUSED", reason="BACKEND_FAILED"))
        return events, [*records, END]

    def denying_read_text(self):
        """``Path.read_text`` that refuses the one log path the scenario denies (permission denied)."""
        real = Path.read_text
        harness = self

        def read_text(path, *a, **k):
            if harness.deny_path is not None and path == harness.deny_path:
                raise PermissionError("denied")
            return real(path, *a, **k)
        return mock.patch.object(Path, "read_text", read_text)

    def run(self, extra=()):
        stdout = io.StringIO()
        with mock.patch.object(rg, "ancestor_context", return_value=[]), contextlib.redirect_stdout(stdout), \
                self.denying_read_text():
            (self.root / "folders").mkdir(exist_ok=True)
            code = pr.main([*self.argv, *extra], deps=self.deps, driver=object(), display=self.display)
        return code

    @property
    def series_dir(self) -> Path:
        return self.root / "out" / f"{self.name}-ag03-phase{self.phase}-rep{self.rep}"

    def summary(self) -> dict:
        return json.loads((self.series_dir / "series.json").read_text(encoding="utf-8"))


class CampaignEntryPointTests(unittest.TestCase):
    def test_repetition_one_runs_the_eleven_goals_in_the_preregistered_order_and_records_the_counts(self):
        s = Series(self, rep=1)
        self.assertEqual(s.run(), 0)
        self.assertEqual([goal for goal, _ in s.calls], plan.goal_order(1))
        self.assertEqual(plan.goal_order(1)[0], "G11")
        counts = s.summary()["counts"]
        self.assertEqual(counts, {"slots": 11, "primary_n": 11, "successes": 11, "raw_attempts": 11,
                                  "technical_failures": 0, "interrupted": 0, "not_attempted": 0})
        self.assertTrue(s.summary()["series_complete"])
        self.assertEqual(s.summary()["series_score"], 11)
        for goal in plan.GOAL_IDS:
            self.assertTrue((s.series_dir / goal / "attempt1" / "result.json").is_file(), goal)

    def test_repetition_six_uses_its_own_order_and_directory(self):
        s = Series(self, rep=6)
        self.assertEqual(s.run(), 0)
        self.assertEqual([goal for goal, _ in s.calls], plan.goal_order(6))
        self.assertEqual(plan.goal_order(6)[0], "G04")
        self.assertTrue(s.series_dir.name.endswith("rep6"))
        self.assertEqual(s.summary()["repetition"], 6)

    def test_a_precall_failure_then_success_is_one_slot_with_two_raw_attempts_and_two_artifacts(self):
        s = Series(self, script={("G03", 1): "no_front"})
        self.assertEqual(s.run(), 0)
        self.assertEqual([c for c in s.calls if c[0] == "G03"], [("G03", 2)])  # the first never reached the model
        self.assertEqual(s.app_calls["G03"], 2)
        counts = s.summary()["counts"]
        self.assertEqual((counts["primary_n"], counts["successes"], counts["raw_attempts"],
                          counts["technical_failures"]), (11, 11, 12, 0))
        slot = next(x for x in s.summary()["slots"] if x["goal"] == "G03")
        self.assertEqual([r["attempt"] for r in slot["raw"]], [1, 2])
        self.assertEqual(slot["raw"][0]["model_called"], False)
        self.assertTrue(slot["raw"][0]["aborted"])
        for n in (1, 2):
            self.assertTrue((s.series_dir / "G03" / f"attempt{n}" / "result.json").is_file())

    def test_two_precall_failures_are_a_technical_failure_a_non_success_in_the_denominator(self):
        s = Series(self, script={("G03", 1): "no_front", ("G03", 2): "no_front"})
        self.assertEqual(s.run(), 0)  # a technical failure does not end the series
        self.assertEqual(s.app_calls["G03"], 2)  # never a third attempt
        counts = s.summary()["counts"]
        self.assertEqual((counts["primary_n"], counts["successes"], counts["raw_attempts"],
                          counts["technical_failures"]), (11, 10, 12, 1))
        self.assertEqual(s.summary()["series_score"], 10)
        tally = s.summary()["tally"]["G03"]
        self.assertEqual((tally["k"], tally["n"], tally["technical_failures"], tally["raw_attempts"]), (0, 1, 1, 2))

    def test_a_postcall_error_is_the_slots_outcome_and_is_not_retried(self):
        s = Series(self, script={("G05", 1): "timeout"})
        self.assertEqual(s.run(), 0)
        self.assertEqual([c for c in s.calls if c[0] == "G05"], [("G05", 1)])
        counts = s.summary()["counts"]
        self.assertEqual((counts["successes"], counts["raw_attempts"], counts["technical_failures"]), (10, 11, 0))
        slot = next(x for x in s.summary()["slots"] if x["goal"] == "G05")
        self.assertEqual(slot["raw"][0]["fault"], "timeout")

    def test_a_capture_refusal_is_an_outcome_not_a_fault_to_retry_around(self):
        s = Series(self, script={("G02", 1): "capture_refused"})
        self.assertEqual(s.run(), 0)
        self.assertEqual(s.app_calls["G02"], 1)
        slot = next(x for x in s.summary()["slots"] if x["goal"] == "G02")
        self.assertFalse(slot["primary_success"])
        self.assertTrue(slot["raw"][0]["capture_refused"])

    def test_a_safety_event_stops_the_campaign_creates_the_stop_file_and_leaves_the_rest_not_attempted(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[3], 1): "safety"})
        self.assertEqual(s.run(), 2)
        self.assertEqual([goal for goal, _ in s.calls], order[:4])
        self.assertTrue(s.stop.exists())
        counts = s.summary()["counts"]
        self.assertEqual((counts["primary_n"], counts["not_attempted"], counts["successes"]), (4, 7, 3))
        self.assertFalse(s.summary()["series_complete"])
        self.assertIsNone(s.summary()["series_score"])

    def test_an_operator_stop_ends_the_campaign_without_the_runner_touching_the_operators_file(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[2], 1): "operator_stop"})
        self.assertEqual(s.run(), 2)
        self.assertEqual(len(s.calls), 3)
        self.assertFalse(s.stop.exists())  # the runner neither creates nor deletes the operator's file
        self.assertEqual(s.summary()["counts"]["not_attempted"], 8)

    def test_a_precall_failure_with_the_operators_stop_present_does_not_retry(self):
        order = plan.goal_order(1)
        s = Series(self)
        s.stop.write_text("", encoding="utf-8")  # the operator engaged the stop before the series
        self.assertEqual(s.run(), 2)
        self.assertEqual(s.calls, [])  # no session at all, one aborted attempt, the rest not attempted
        counts = s.summary()["counts"]
        self.assertEqual((counts["raw_attempts"], counts["not_attempted"], counts["interrupted"]), (1, 10, 1))
        self.assertEqual(order[0], s.summary()["slots"][0]["goal"])

    def test_a_series_with_a_changed_effective_setting_is_not_pooled_and_never_starts(self):
        first = Series(self, rep=1)
        self.assertEqual(first.run(), 0)
        second = Series(self, root=first.root, rep=2)
        self.assertEqual(second.run(), 0)  # the same settings: pooled
        third = Series(self, root=first.root, rep=3)
        with self.assertRaises(SystemExit) as raised:
            third.run(extra=["--goal-timeout", "120"])
        self.assertIn("not pooled", str(raised.exception))
        self.assertEqual(third.calls, [])
        self.assertFalse(third.series_dir.exists())
        record = json.loads((first.root / "campaign" / "not_pooled.jsonl").read_text().splitlines()[-1])
        self.assertEqual(record["differs_in"], ["caps"])

    def test_a_changed_claude_code_version_in_the_proof_is_not_pooled(self):
        first = Series(self, rep=1)
        first.run()
        second = Series(self, root=first.root, rep=2)
        proof = json.loads((first.root / "proof.json").read_text())
        (first.root / "proof.json").write_text(json.dumps(proof | {"claude_code_version": "2.1.305"}))
        with self.assertRaises(SystemExit):
            second.run()
        self.assertEqual(second.calls, [])

    def test_a_model_that_did_not_pass_its_gate_is_refused_before_any_session(self):
        for status in ("rejected", "failed_smoke", "failed_preflight"):
            s = Series(self, gate_status={"rejected": "rejected", "failed_smoke": "smoke", "failed_preflight":
                                          "failed_preflight"}[status])
            with self.subTest(status), self.assertRaises(SystemExit) as raised:
                s.run()
            self.assertIn("did not pass its gate", str(raised.exception))
            self.assertEqual(s.calls, [])
            self.assertFalse(s.series_dir.exists())

    def test_a_gate_record_of_another_model_or_none_is_refused(self):
        s = Series(self)
        other = Path(s.argv[s.argv.index("--gate-record") + 1])
        other.write_text(json.dumps(gate.gate_record("MiMo", "mimo", preflight_passed=True, preconditions=PRE,
                                                     answer=GOOD_ANSWER)), encoding="utf-8")
        with self.assertRaises(SystemExit):
            s.run()
        other.unlink()
        with self.assertRaises(SystemExit):
            s.run()
        self.assertEqual(s.calls, [])

    def test_a_series_never_overwrites_an_existing_directory(self):
        s = Series(self)
        s.run()
        again = Series(self, root=s.root)
        with self.assertRaises(SystemExit):  # the primary claims are settled (F5)
            again.run()
        diagnostic = Series(self, root=s.root)
        with self.assertRaises(FileExistsError):  # and a diagnostic run still never reuses an output directory
            diagnostic.run(extra=["--diagnostic"])

    def test_a_series_is_always_the_eleven_goals(self):
        s = Series(self)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            s.run(extra=["--goal", "G01"])
        self.assertEqual(s.calls, [])

    def test_the_plan_lists_the_preregistered_order_of_the_repetition_and_starts_nothing(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = pr.main(["--plan", "--phase", "1", "--repetition", "6", "--model", "claude-haiku-5-5"])
        self.assertEqual(code, 0)
        goals = [json.loads(line)["goal"] for line in out.getvalue().splitlines()]
        self.assertEqual(goals, plan.goal_order(6))

    def test_the_production_entry_point_uses_the_slot_machinery_and_not_a_goal_loop(self):
        source = (HERE / "pixel_runner.py").read_text(encoding="utf-8")
        self.assertIn("sl.run_campaign(", source)
        self.assertIn("pool_check(", source)
        self.assertIn("read_gate(", source)


if __name__ == "__main__":
    unittest.main()
