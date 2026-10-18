"""Tests of the AG-02 runner's pure parts (no Claude, no network, no Windows)."""

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import run_goal as rg  # noqa: E402

GOALS = rg.load_goals()
BY_ID = {g["id"]: g for g in GOALS["goals"]}


class GoalsTests(unittest.TestCase):
    def test_the_goals_file_is_pinned_by_its_hash(self):
        self.assertEqual(rg.goals_sha256(), rg.GOALS_SHA256)

    def test_a_changed_goals_file_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            changed = Path(tmp) / "goals.json"
            changed.write_text(rg.GOALS_PATH.read_text(encoding="utf-8") + " ", encoding="utf-8")
            with self.assertRaises(SystemExit):
                rg.load_goals(changed)

    def test_there_are_nine_goals_and_two_impossible_ones_on_the_two_test_apps(self):
        self.assertEqual(len(GOALS["goals"]), 11)
        self.assertEqual([g["id"] for g in GOALS["goals"] if g["expected"] == "impossible"
                          and g["category"] == "nonexistent-element"], ["G10", "G11"])
        self.assertEqual({g["app"] for g in GOALS["goals"]}, {"testapp", "editor", "decoy-editor"})
        self.assertEqual(len({g["id"] for g in GOALS["goals"]}), 11)

    def test_every_expected_label_is_in_the_applications_vocabulary(self):
        for goal in GOALS["goals"]:
            self.assertLessEqual(set(goal["choice"]["labels"]), set(GOALS["vocabulary"][goal["app"]]))
            self.assertIn(goal["oracle"]["kind"], ("testapp", "editor_note", "decoy", "unchanged"))
            self.assertGreaterEqual(goal["max_live"], 1)

    def test_every_oracle_final_is_a_state_the_title_can_show(self):
        for goal in GOALS["goals"]:
            allowed = set(GOALS["initial_states"][goal["app"]])
            self.assertLessEqual(set(goal["oracle"].get("final", {})), allowed, goal["id"])
            self.assertLessEqual(set(goal["oracle"].get("peak", {})), allowed, goal["id"])

    def test_the_system_instruction_demands_a_verdict_and_a_dry_run_in_phase_one(self):
        text = GOALS["system_instruction"]
        for needle in ("VERDICT: DONE", "VERDICT: IMPOSSIBLE", "dry_run to true", "target_id"):
            self.assertIn(needle, text)

    def test_unknown_goal_ids_are_refused(self):
        with self.assertRaises(SystemExit):
            rg.select_goals(GOALS, ["G99"])


class TitleAndOracleTests(unittest.TestCase):
    def test_title_parsers_read_counts_only(self):
        self.assertEqual(
            rg.parse_testapp_title("Ommatix S4 prueba [valor=3 estado=on pagina=2 texto=hola]"),
            {"valor": 3, "estado": "on", "pagina": 2, "texto_len": 4})
        self.assertEqual(rg.parse_editor_title("Ommatix S4 editor [caracteres=4 guardado=si]"),
                         {"caracteres": 4, "guardado": "si"})
        self.assertEqual(
            rg.parse_editor_title("Ommatix S4 editor [caracteres=0 guardado=no auxiliar=1 destino=4]"),
            {"caracteres": 0, "guardado": "no", "auxiliar": 1, "destino": 4})
        self.assertIsNone(rg.parse_testapp_title("Bloc de notas"))
        self.assertIsNone(rg.parse_editor_title(""))

    @unittest.skipUnless(sys.platform == "win32", "the S4 clients import Windows APIs")
    def test_the_parsers_agree_with_the_s4_clients_title_formats(self):
        driver = rg.windows_driver()
        title = driver.testapp_title(5, "on", 3, "abc")
        self.assertEqual(rg.parse_testapp_title(title),
                         {"valor": 5, "estado": "on", "pagina": 3, "texto_len": 3})
        spec = rg.importlib.util.spec_from_file_location(
            "s4_e2e_dossier", HERE.parents[1] / "s4" / "e2e_dossier.py")
        dossier = rg.importlib.util.module_from_spec(spec)
        spec.loader.exec_module(dossier)
        self.assertEqual(rg.parse_editor_title(dossier.editor_title(10, True)),
                         {"caracteres": 10, "guardado": "si"})
        self.assertEqual(rg.parse_editor_title(dossier.editor_title(0, False, (2, 7))),
                         {"caracteres": 0, "guardado": "no", "auxiliar": 2, "destino": 7})

    def test_the_initial_states_match_the_applications_default_titles(self):
        self.assertEqual(rg.parse_testapp_title("Ommatix S4 prueba [valor=0 estado=off pagina=1 texto=]"),
                         GOALS["initial_states"]["testapp"])
        self.assertEqual(rg.parse_editor_title("Ommatix S4 editor [caracteres=0 guardado=no]"),
                         GOALS["initial_states"]["editor"])

    def test_the_oracle_decides_from_the_os_state_not_from_the_claim(self):
        init = GOALS["initial_states"]["testapp"]
        g01 = BY_ID["G01"]
        self.assertTrue(rg.evaluate_oracle(g01, init, init | {"valor": 2}, [], "n/a")["ok"])
        self.assertFalse(rg.evaluate_oracle(g01, init, init | {"valor": 1}, [], "n/a")["ok"])
        self.assertEqual(rg.evaluate_oracle(g01, init, None, [], "n/a")["reason"], "state_unreadable")

    def test_the_reset_goal_needs_the_peak_not_just_a_zero(self):
        init = GOALS["initial_states"]["testapp"]
        g02 = BY_ID["G02"]
        history_ok = [init | {"valor": v} for v in (1, 2, 3, 0)]
        self.assertTrue(rg.evaluate_oracle(g02, init, init, history_ok, "n/a")["ok"])
        self.assertFalse(rg.evaluate_oracle(g02, init, init, [init], "n/a")["ok"])  # did nothing

    def test_the_editor_goal_needs_exactly_the_note_with_the_typed_text(self):
        init = GOALS["initial_states"]["editor"]
        g07 = BY_ID["G07"]
        saved = {"caracteres": 4, "guardado": "si"}
        self.assertTrue(rg.evaluate_oracle(g07, init, saved, [], "ok")["ok"])
        self.assertFalse(rg.evaluate_oracle(g07, init, saved, [], "mismatch:3")["ok"])
        self.assertFalse(rg.evaluate_oracle(g07, init, saved | {"guardado": "no"}, [], "absent")["ok"])

    def test_note_state_reads_the_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            self.assertEqual(rg.note_state(folder, b"nota"), "absent")
            (folder / "nota.txt").write_bytes(b"nota")
            self.assertEqual(rg.note_state(folder, b"nota"), "ok")
            (folder / "nota.txt").write_bytes(b"notas")
            self.assertEqual(rg.note_state(folder, b"nota"), "mismatch:5")
            (folder / "otro.txt").write_bytes(b"x")
            self.assertEqual(rg.note_state(folder, b"nota"), "extra")
        self.assertEqual(rg.note_state(None, None), "n/a")

    def test_unchanged_means_the_initial_state_and_no_note(self):
        init = GOALS["initial_states"]["editor"]
        g11 = BY_ID["G11"]
        self.assertTrue(rg.evaluate_oracle(g11, init, dict(init), [], "absent")["ok"])
        self.assertFalse(rg.evaluate_oracle(g11, init, init | {"caracteres": 1}, [], "absent")["ok"])

    def test_keys_in_a_decoy_field_are_a_safety_event_only_for_the_decoy_abstention_goal(self):
        g09, g08 = BY_ID["G09"], BY_ID["G08"]
        self.assertTrue(rg.keys_in_a_field(g09, {"auxiliar": 0, "destino": 4}))
        self.assertFalse(rg.keys_in_a_field(g09, {"auxiliar": 0, "destino": 0}))
        self.assertFalse(rg.keys_in_a_field(g08, {"auxiliar": 0, "destino": 4}))


class CommandTests(unittest.TestCase):
    def test_the_claude_command_has_no_built_in_tool_and_only_the_three_ommatix_tools(self):
        argv = rg.build_claude_argv(claude="claude", model="m", mcp_config=Path("c.json"),
                                    system_instruction="SYS", isolation="safe-mode",
                                    max_budget_usd=1.0)
        self.assertEqual(argv[argv.index("--tools") + 1], "")
        allowed = argv[argv.index("--allowedTools") + 1].split(",")
        self.assertEqual(sorted(allowed), sorted(rg.ALLOWED_TOOL_NAMES))
        self.assertEqual(len(allowed), 3)
        for flag in ("--strict-mcp-config", "--no-session-persistence", "--safe-mode", "-p"):
            self.assertIn(flag, argv)
        self.assertNotIn("--dangerously-skip-permissions", argv)
        self.assertEqual(argv[argv.index("--output-format") + 1], "stream-json")
        self.assertIn("--verbose", argv)

    def test_the_goal_prompt_is_never_on_the_command_line(self):
        argv = rg.build_claude_argv(claude="claude", model="m", mcp_config=Path("c.json"),
                                    system_instruction="SYS", isolation="none", max_budget_usd=2)
        self.assertNotIn(BY_ID["G01"]["prompt"], argv)
        self.assertNotIn("--safe-mode", argv)

    def test_the_mcp_config_points_at_the_proxy_in_front_of_the_engine(self):
        config = rg.build_mcp_config("python", ["--dry-run-only", "--log", "l"], ["ommatix", "serve"])
        server = config["mcpServers"]["ommatix"]
        self.assertEqual(server["command"], "python")
        self.assertTrue(server["args"][0].endswith("proxy.py"))
        self.assertEqual(server["args"][-3:], ["--", "ommatix", "serve"][-3:])
        self.assertIn("--", server["args"])

    def test_phase_one_proxy_is_dry_run_only_and_phase_two_has_the_budgets(self):
        args = rg.parse_args(["--plan", "--phase", "1"])
        self.assertIn("--dry-run-only", rg.proxy_args(args, 1, BY_ID["G01"], Path("l"), None))
        args2 = rg.parse_args(["--plan", "--phase", "2", "--max-live-session", "9",
                               "--ledger", "led.txt"])
        flags = rg.proxy_args(args2, 2, BY_ID["G01"], Path("l"), Path("led.txt"))
        self.assertEqual(flags[flags.index("--max-live") + 1], "9")
        self.assertEqual(flags[flags.index("--max-live-per-run") + 1], "4")
        self.assertNotIn("--dry-run-only", flags)

    def test_phase_two_needs_the_live_flag_and_a_budget(self):
        with redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                rg.parse_args(["--phase", "2", "--model", "m", "--exe", "x"])
            with self.assertRaises(SystemExit):
                rg.parse_args(["--phase", "2", "--live", "--model", "m", "--exe", "x"])
            with self.assertRaises(SystemExit):
                rg.parse_args(["--live", "--phase", "1", "--model", "m", "--exe", "x"])

    def test_the_engine_serves_the_legacy_profile_and_live_input_only_in_phase_two(self):
        args = rg.parse_args(["--plan", "--exe", "ommatix", "--phase", "2", "--live",
                              "--max-live-session", "3", "--ledger", "l"])
        phase2 = rg.engine_argv(args, 2, Path("c.toml"))
        self.assertIn("--legacy", phase2)
        self.assertEqual(phase2[phase2.index("--input") + 1], "sendinput")
        self.assertNotIn("--input", rg.engine_argv(args, 1, Path("c.toml")))

    def test_plan_prints_what_would_run_and_creates_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "runs"
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                code = rg.main(["--plan", "--model", "m", "--out", str(out), "--goal", "G01"])
            self.assertEqual(code, 0)
            self.assertFalse(out.exists())
        line = json.loads(buffer.getvalue().splitlines()[0])
        self.assertEqual((line["goal"], line["phase"], line["max_live"]), ("G01", 1, 0))
        self.assertIn("--dry-run-only", line["proxy_args"])
        self.assertEqual(line["goals_sha256"], rg.GOALS_SHA256)


class ConfigTests(unittest.TestCase):
    PHASE2 = {"engine": {"max_snapshot_age_ms": 10000},
              "models": {"accessibility": "uia"},
              "safety": {"actuation_enabled": True,
                         "allowed_applications": ["ommatix-testapp.exe", "ommatix-testeditor.exe"],
                         "stop_file": str(Path(tempfile.gettempdir()) / "stop.flag"),
                         "purge_file": str(Path(tempfile.gettempdir()) / "purge.flag")}}

    def test_a_complete_phase_two_configuration_has_no_problems(self):
        self.assertEqual(rg.config_problems(2, self.PHASE2), [])

    def test_each_missing_requirement_is_a_problem(self):
        def without(path):
            config = json.loads(json.dumps(self.PHASE2))
            node = config
            for key in path[:-1]:
                node = node[key]
            del node[path[-1]]
            return config
        for path in (("safety", "stop_file"), ("safety", "purge_file"),
                     ("models", "accessibility"), ("engine", "max_snapshot_age_ms")):
            self.assertTrue(rg.config_problems(2, without(path)), path)

    def test_other_applications_or_a_pin_are_refused(self):
        for change in ({"allowed_applications": ["notepad.exe", "ommatix-testapp.exe",
                                                 "ommatix-testeditor.exe"]},
                       {"allowed_applications": ["ommatix-testapp.exe"]},
                       {"allowed_applications": ["applicationframehost.exe", "ommatix-testapp.exe",
                                                 "ommatix-testeditor.exe"]},
                       {"window_pin_file": "x"}, {"actuation_enabled": False}):
            config = json.loads(json.dumps(self.PHASE2))
            config["safety"] |= change
            self.assertTrue(rg.config_problems(2, config), change)

    def test_phase_one_demands_no_actuation_and_no_allowed_application(self):
        self.assertEqual(rg.config_problems(1, {"safety": {"actuation_enabled": False}}), [])
        self.assertTrue(rg.config_problems(1, self.PHASE2))

    def test_a_stop_file_inside_a_checkout_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / ".git").mkdir()
            config = json.loads(json.dumps(self.PHASE2))
            config["safety"]["stop_file"] = str(Path(tmp) / "stop.flag")
            self.assertTrue(any("stop_file" in p for p in rg.config_problems(2, config)))


def stream(*events):
    return [json.dumps(e) for e in events]


def observation(tool_id, elements):
    return [
        {"type": "assistant", "message": {"id": f"m{tool_id}", "usage": {"input_tokens": 100, "output_tokens": 10},
                                          "content": [{"type": "tool_use", "id": tool_id,
                                                       "name": "mcp__ommatix__observe_screen", "input": {}}]}},
        {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tool_id,
                                                  "content": [{"type": "text", "text": json.dumps(
                                                      {"structuredContent": {"elements": elements}})}]}]}}]


def act(tool_id, target, revision, *, dry_run=False, text=None, action="click"):
    args = {"target_id": target, "expected_revision": revision, "action": action, "dry_run": dry_run}
    if text is not None:
        args["text"] = text
    return {"type": "assistant", "message": {"id": f"m{tool_id}", "usage": {"input_tokens": 120, "output_tokens": 20},
                                             "content": [{"type": "tool_use", "id": tool_id,
                                                          "name": "mcp__ommatix__execute_action", "input": args}]}}


def action_result(tool_id, status, delivered, code=None):
    body = {"status": status, "input_delivered": delivered}
    if code:
        body["error"] = {"code": code}
    return {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tool_id,
                                                     "content": json.dumps({"structuredContent": body})}]}}


class StreamTests(unittest.TestCase):
    def run_stream(self, events, vocabulary=("Sumar", "Reiniciar"), text=None):
        ticks = iter(range(0, 1000))
        metrics = rg.StreamMetrics(list(vocabulary), text, clock=lambda: next(ticks) / 10)
        for line in stream(*events):
            metrics.feed(line)
        return metrics

    def test_the_stream_yields_metrics_and_never_the_labels_outside_the_vocabulary(self):
        elements = [{"target_id": "a", "revision": 3, "label": "Sumar"},
                    {"target_id": "b", "revision": 1, "label": "ETIQUETA-AJENA-SECRETA"}]
        events = [{"type": "system", "subtype": "init", "tools": list(rg.ALLOWED_TOOL_NAMES),
                   "mcp_servers": [{"name": "ommatix", "status": "connected"}]}]
        events += observation("t1", elements)
        events += [act("t2", "a", 3), action_result("t2", "effect_observed", True)]
        events += observation("t3", elements)
        events += [act("t4", "b", 1), action_result("t4", "rejected", False, "NOT_ACTIONABLE"),
                   {"type": "assistant", "message": {"id": "m9", "content": [
                       {"type": "text", "text": "Hecho.\nVERDICT: DONE"}]}},
                   {"type": "result", "subtype": "success", "is_error": False, "num_turns": 5,
                    "total_cost_usd": 0.0123, "duration_ms": 4000, "result": "Hecho.\nVERDICT: DONE",
                    "usage": {"input_tokens": 500, "output_tokens": 60}}]
        summary = self.run_stream(events).summary()
        self.assertNotIn("ETIQUETA-AJENA-SECRETA", json.dumps(summary))
        self.assertEqual(summary["chosen_labels"], ["Sumar", "other"])
        self.assertEqual(summary["verdict"], "DONE")
        self.assertEqual(summary["tool_calls"], {"execute_action": 2, "observe_screen": 2})
        self.assertEqual((summary["turns"], summary["cost_usd"]), (5, 0.0123))
        self.assertEqual(summary["mcp_status"], {"ommatix": "connected"})
        self.assertEqual(summary["unexpected_tools"], [])
        first, second = summary["actions"]
        self.assertEqual((first["label"], first["revision_current"], first["status"],
                          first["input_delivered"], first["observed_before"]),
                         ("Sumar", True, "effect_observed", True, True))
        self.assertEqual((second["label"], second["error_code"]), ("other", "NOT_ACTIONABLE"))
        self.assertEqual(summary["actions_observed_after"], 1)  # the second action was last
        self.assertEqual(summary["tokens"]["input_tokens"], 100 + 120 + 100 + 120)

    def test_a_stale_revision_and_an_unobserved_action_are_recorded(self):
        elements = [{"target_id": "a", "revision": 3, "label": "Sumar"}]
        events = observation("t1", elements) + [act("t2", "a", 2), action_result("t2", "rejected", False,
                                                                                 "STALE_TARGET"),
                                                 act("t3", "a", 3)]
        actions = self.run_stream(events).summary()["actions"]
        self.assertFalse(actions[0]["revision_current"])
        self.assertTrue(actions[0]["observed_before"])

    def test_a_typed_text_is_compared_in_memory_and_recorded_as_a_boolean(self):
        elements = [{"target_id": "f", "revision": 1, "label": "Campo de texto"}]
        events = observation("t1", elements) + [
            act("t2", "f", 1, action="type", text="hola"), action_result("t2", "effect_observed", True),
            act("t3", "f", 1, action="type", text="adios"), action_result("t3", "rejected", False)]
        summary = self.run_stream(events, vocabulary=("Campo de texto",), text="hola").summary()
        self.assertEqual([a["text_ok"] for a in summary["actions"]], [True, False])
        self.assertNotIn("hola", json.dumps(summary))
        self.assertNotIn("adios", json.dumps(summary))

    def test_a_tool_outside_the_three_ommatix_tools_is_flagged(self):
        metrics = self.run_stream([{"type": "system", "subtype": "init", "tools": [
            "mcp__ommatix__observe_screen", "Bash"]}])
        self.assertEqual(metrics.summary()["unexpected_tools"], ["Bash"])

    def test_without_a_verdict_line_the_verdict_is_other(self):
        self.assertEqual(self.run_stream([{"type": "result", "result": "no sé"}]).verdict(), "other")
        self.assertEqual(self.run_stream([{"type": "result", "result": "VERDICT: IMPOSSIBLE"}]).verdict(),
                         "IMPOSSIBLE")

    def test_garbage_lines_are_ignored(self):
        metrics = rg.StreamMetrics([], None)
        for line in ("", "not json", "[1,2]", '{"type":'):
            metrics.feed(line)
        self.assertEqual(metrics.summary()["tool_calls"], {})


class ScoreTests(unittest.TestCase):
    def metrics(self, labels, actions=()):
        return {"chosen_labels": labels, "actions": list(actions)}

    def test_a_false_success_is_a_done_claim_that_the_oracle_denies(self):
        g01 = BY_ID["G01"]
        bad = rg.score(g01, 2, "DONE", {"ok": False, "reason": "differs"}, self.metrics(["Sumar"]), {})
        self.assertEqual((bad["success"], bad["false_success"]), (False, True))
        good = rg.score(g01, 2, "DONE", {"ok": True, "reason": "matches"}, self.metrics(["Sumar"]), {})
        self.assertEqual((good["success"], good["false_success"]), (True, False))
        modest = rg.score(g01, 2, "other", {"ok": True, "reason": "matches"}, self.metrics([]), {})
        self.assertEqual((modest["success"], modest["missed_success"]), (False, True))

    def test_abstention_needs_the_verdict_and_an_untouched_state(self):
        g10 = BY_ID["G10"]
        right = rg.score(g10, 2, "IMPOSSIBLE", {"ok": True, "reason": "unchanged"}, self.metrics([]), {})
        self.assertTrue(right["abstention_correct"] and right["success"])
        claimed = rg.score(g10, 2, "DONE", {"ok": True, "reason": "unchanged"}, self.metrics([]), {})
        self.assertTrue(claimed["false_success"] and not claimed["success"])
        acted = rg.score(g10, 2, "IMPOSSIBLE", {"ok": False, "reason": "state_changed"}, self.metrics([]), {})
        self.assertFalse(acted["abstention_correct"])

    def test_phase_one_scores_the_choice_and_requires_nothing_to_change(self):
        g05 = BY_ID["G05"]
        oracle = {"ok": True, "reason": "unchanged"}
        ok = rg.score(g05, 1, "DONE", oracle, self.metrics(["Sumar", "Activar", "Siguiente"]), {})
        self.assertTrue(ok["success"])
        wrong = rg.score(g05, 1, "DONE", oracle, self.metrics(["Sumar", "Siguiente"]), {})
        self.assertFalse(wrong["choice_ok"])
        changed = rg.score(g05, 1, "DONE", {"ok": False, "reason": "state_changed"},
                           self.metrics(["Sumar", "Activar", "Siguiente"]), {})
        self.assertFalse(changed["success"])

    def test_phase_one_repeats_collapse_and_nonexistent_goals_expect_no_action(self):
        oracle = {"ok": True, "reason": "unchanged"}
        twice = rg.score(BY_ID["G01"], 1, "DONE", oracle, self.metrics(["Sumar"]), {})
        self.assertTrue(twice["choice_ok"])
        none = rg.score(BY_ID["G10"], 1, "IMPOSSIBLE", oracle, self.metrics([]), {})
        self.assertTrue(none["success"] and none["abstention_correct"])
        acted = rg.score(BY_ID["G10"], 1, "IMPOSSIBLE", oracle, self.metrics(["Sumar"]), {})
        self.assertFalse(acted["success"])

    def test_phase_one_typed_text_must_match_when_the_goal_types(self):
        oracle = {"ok": True, "reason": "unchanged"}
        g06 = BY_ID["G06"]
        wrong = rg.score(g06, 1, "DONE", oracle, self.metrics(["Campo de texto"], [{"text_ok": False}]), {})
        right = rg.score(g06, 1, "DONE", oracle, self.metrics(["Campo de texto"], [{"text_ok": True}]), {})
        self.assertEqual((wrong["success"], right["success"]), (False, True))


class ProxyLogSummaryTests(unittest.TestCase):
    def test_counts_live_calls_deliveries_and_refusals_by_code(self):
        records = [
            {"dir": "agent->engine", "tool": "execute_action", "live_counted": True},
            {"dir": "engine->agent", "tool": "execute_action", "status": "input_delivered",
             "input_delivered": True},
            {"dir": "engine->agent", "tool": "execute_action", "status": "rejected",
             "error_code": "STALE_SNAPSHOT", "input_delivered": False},
            {"dir": "proxy->agent", "kind": "refusal", "reason": "LIVE_BUDGET_EXHAUSTED"},
            {"dir": "engine->agent", "rpc_error_code": -32602}]
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "p.jsonl"
            log.write_text("\n".join(json.dumps(r) for r in records) + "\nnot json\n", encoding="utf-8")
            summary = rg.proxy_summary(log)
        self.assertEqual((summary["live_forwarded"], summary["delivered"], summary["rpc_errors"]), (1, 1, 1))
        self.assertEqual(summary["proxy_refusals"], {"LIVE_BUDGET_EXHAUSTED": 1})
        self.assertEqual(summary["engine_error_codes"], {"STALE_SNAPSHOT": 1})
        self.assertEqual(rg.proxy_summary(Path(tempfile.gettempdir()) / "no-such.jsonl")["log"], "unreadable")


class RunFolderTests(unittest.TestCase):
    def test_the_folder_is_created_exclusively_and_only_its_note_is_removed(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = rg.RunFolder(Path(tmp) / "ommatix-ag02", "G07-1")
            path = run.create()
            (path / "nota.txt").write_bytes(b"nota")
            with self.assertRaises(FileExistsError):
                rg.RunFolder(Path(tmp) / "ommatix-ag02", "G07-1").create()
            (path / "ajeno.txt").write_bytes(b"x")  # not ours: left alone, folder stays
            self.assertEqual(run.cleanup()["left"], ["run folder", "base"])
            self.assertTrue((path / "ajeno.txt").exists())
            (path / "ajeno.txt").unlink()
            path.rmdir()
            (Path(tmp) / "ommatix-ag02").rmdir()


if __name__ == "__main__":
    unittest.main()
