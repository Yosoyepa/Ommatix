"""Regression tests for the five findings of the third review of PR 86 (N1-N5). See
docs/reviews/2026-10-17-pr86-ag02-harness.md. No Claude, no network beyond 127.0.0.1, no screen."""

import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import preflight  # noqa: E402
import run_goal as rg  # noqa: E402
from test_preflight import PreflightBase  # noqa: E402
from test_run_goal import GOALS, Harness, final, make_proof, observation  # noqa: E402


class ManagedPolicyDiscoveryTests(unittest.TestCase):
    """N1: the documented drop-in directory and the other managed artefacts are found."""

    def system_root(self, tmp, artefact):
        root = Path(tmp) / "ClaudeCode"
        (root).mkdir()
        if artefact == "managed-settings.d":
            (root / artefact).mkdir()
            (root / artefact / "10-provider.json").write_text(
                json.dumps({"env": {"CLAUDE_CODE_USE_BEDROCK": "1"}}), encoding="utf-8")
        else:
            (root / artefact).write_text("{}", encoding="utf-8")
        return root

    def default_discovery(self, tmp):
        """Default discovery pointed at a temporary system root (no other root, no registry)."""
        return mock.patch.dict(os.environ, {"PROGRAMFILES": tmp, "PROGRAMDATA": ""}), \
            mock.patch.object(rg, "POSIX_POLICY_ROOTS", ())

    def test_a_drop_in_alone_is_found_by_the_default_discovery_and_the_preflights_refusals(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.system_root(tmp, "managed-settings.d")
            env, roots = self.default_discovery(tmp)
            with env, roots:
                self.assertEqual(rg.managed_config_found(), ["managed_root_0_managed-settings.d"])
                self.assertIn("managed_root_0_managed-settings.d", preflight.refusals())

    def test_every_documented_managed_artefact_is_found(self):
        for artefact in rg.POLICY_ARTEFACTS:
            with tempfile.TemporaryDirectory() as tmp:
                root = self.system_root(tmp, artefact)
                self.assertEqual(rg.managed_config_found([root]), [f"managed_root_0_{artefact}"], artefact)

    def test_an_empty_drop_in_directory_and_an_absent_root_are_not_policy(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "ClaudeCode"
            (root / "managed-settings.d").mkdir(parents=True)
            self.assertEqual(rg.managed_config_found([root, Path(tmp) / "absent"]), [])

    def test_a_root_that_cannot_be_listed_is_a_refusal_not_a_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "ClaudeCode"
            root.mkdir()
            with mock.patch.object(Path, "exists", side_effect=PermissionError("denied")):
                self.assertEqual(rg.managed_config_found([root]), ["managed_root_0_unreadable"])
            real_iterdir = Path.iterdir
            (root / "managed-settings.d").mkdir()

            def deny(self_):
                if self_.name == "managed-settings.d":
                    raise PermissionError("denied")
                return real_iterdir(self_)
            with mock.patch.object(Path, "iterdir", deny):
                self.assertEqual(rg.managed_config_found([root]),
                                 ["managed_root_0_managed-settings.d_unreadable"])

    def fake_winreg(self, behaviour):
        def open_key(hive, path):
            outcome = behaviour[hive]
            if outcome == "present":
                return object()
            raise outcome
        return types.SimpleNamespace(HKEY_LOCAL_MACHINE="machine", HKEY_CURRENT_USER="user",
                                     OpenKey=open_key, CloseKey=lambda key: None)

    def test_the_policy_registry_key_distinguishes_absent_from_unreadable(self):
        cases = [({"machine": FileNotFoundError(), "user": FileNotFoundError()}, []),
                 ({"machine": "present", "user": FileNotFoundError()}, ["managed_policy_registry_key_machine"]),
                 ({"machine": PermissionError("denied"), "user": FileNotFoundError()},
                  ["managed_policy_registry_machine_unreadable"]),
                 ({"machine": FileNotFoundError(), "user": OSError("io")},
                  ["managed_policy_registry_user_unreadable"])]
        for behaviour, expected in cases:
            with mock.patch.dict(sys.modules, {"winreg": self.fake_winreg(behaviour)}), \
                    mock.patch.object(rg, "policy_roots", return_value=[]), \
                    mock.patch.object(rg.os, "name", "nt"):
                self.assertEqual(rg.managed_config_found(), expected, behaviour)


class DropInRefusesBeforeAnySessionTests(PreflightBase):
    def test_no_session_starts_when_only_a_drop_in_exists(self):
        import argparse
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "ClaudeCode" / "managed-settings.d"
            root.mkdir(parents=True)
            (root / "10-provider.json").write_text("{}", encoding="utf-8")
            args = argparse.Namespace(claude_config_dir=Path(tmp))
            with mock.patch.dict(os.environ, {"PROGRAMFILES": tmp, "PROGRAMDATA": ""}), \
                    mock.patch.object(rg, "POSIX_POLICY_ROOTS", ()), \
                    mock.patch.object(preflight, "one_session") as session:
                with self.assertRaises(preflight.PreflightRefused) as caught:
                    preflight.run(args)
            session.assert_not_called()
            self.assertIn("managed_root_0_managed-settings.d", caught.exception.reasons)


class DecodedRequestTests(PreflightBase):
    """N2: stage B reads what the recipient reads, not the serialisation's bytes."""

    LINE = 'Instrucción privada número uno: dice "hola" y usa C:\\ruta\\secreta.'

    def test_the_fixture_is_blind_to_a_raw_byte_search(self):
        body = json.dumps({"messages": [{"role": "user", "content": self.LINE}]}).encode()
        self.assertNotIn(self.LINE.encode("utf-8"), body)  # escapes defeat the old comparison
        self.assertIn(self.LINE, preflight.decoded_text(body))

    def test_a_stage_b_only_leak_with_escaped_text_fails_the_proof_while_stage_a_stays_valid(self):
        proof = self.run_preflight("leak_dir", files={"CLAUDE.md": self.LINE + "\n"})
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_context_in_request", proof["problems"])
        self.assertNotIn("sentinels_in_production_request", proof["problems"])  # stage A is clean
        self.assertTrue(proof["control_leaked_kinds"])  # and its control still sees leaks
        self.assertGreaterEqual(proof["production_dir_context_lines_in_request"], 1)
        self.assertNotIn(self.LINE, json.dumps(proof))

    def test_the_same_directory_without_the_leak_passes(self):
        proof = self.run_preflight("normal", files={"CLAUDE.md": self.LINE + "\n"})
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["production_dir_context_lines_checked"], 1)

    def test_quotes_and_backslashes_alone_are_still_found(self):
        proof = self.run_preflight("leak_dir", files={"rules.md": "x", "CLAUDE.md": 'say "yes" to C:\\a\\b\\c\\d\n'})
        self.assertIn("production_dir_context_in_request", proof["problems"])

    def test_a_short_file_is_tested_by_its_whole_text(self):
        proof = self.run_preflight("leak_dir", files={"CLAUDE.md": "short one\n"})
        self.assertIn("production_dir_context_in_request", proof["problems"])

    def test_a_nonempty_file_with_nothing_testable_is_reported_not_counted_as_clean(self):
        proof = self.run_preflight("normal", files={"CLAUDE.md": "ok\n"})
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_context_file_untestable", proof["problems"])
        self.assertEqual(proof["production_dir_context_files_untestable"], 1)

    def test_an_unreadable_request_cannot_be_certified(self):
        proof = self.run_preflight("garbled")
        self.assertFalse(proof["passed"])
        self.assertIn("request_unreadable", proof["problems"])

    def test_decoded_text_covers_keys_lists_and_depth_and_fails_closed_on_non_json(self):
        body = json.dumps({"ключ": ["a", {"b": "c\u00e9"}], "n": 1}).encode()
        text = preflight.decoded_text(body)
        for needle in ("ключ", "a", "b", "cé"):
            self.assertIn(needle, text)
        self.assertIsNone(preflight.decoded_text(b"not json"))
        self.assertIsNone(preflight.decoded_text(b"\xff\xfe"))


class ManifestCapTests(unittest.TestCase):
    """N3: a cap is a refusal, never a complete-looking manifest."""

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-n3-")
        self.addCleanup(scratch.cleanup)
        self.cfg = Path(scratch.name)
        (self.cfg / "rules").mkdir()

    def files(self, n, prefix="r"):
        for i in range(n):
            (self.cfg / "rules" / f"{prefix}{i:04d}.md").write_text(f"rule {i}\n", encoding="utf-8")

    def test_editing_the_last_file_of_a_tree_at_the_cap_changes_the_manifest(self):
        with mock.patch.object(rg, "MANIFEST_FILE_CAP", 3):
            self.files(3)
            before = rg.config_manifest(self.cfg)
            (self.cfg / "rules" / "r0002.md").write_text("changed\n", encoding="utf-8")
            self.assertNotEqual(rg.config_manifest(self.cfg), before)

    def test_a_tree_over_the_cap_is_refused_whether_a_file_is_added_or_edited(self):
        with mock.patch.object(rg, "MANIFEST_FILE_CAP", 3):
            self.files(4)
            with self.assertRaises(rg.ManifestRefused) as caught:
                rg.config_manifest(self.cfg)
            self.assertEqual(str(caught.exception), "too_many_files")
            (self.cfg / "rules" / "r0003.md").write_text("edited beyond the cap\n", encoding="utf-8")
            with self.assertRaises(rg.ManifestRefused):
                rg.config_manifest(self.cfg)
            self.files(1, prefix="z")
            with self.assertRaises(rg.ManifestRefused):
                rg.config_manifest(self.cfg)

    def test_the_proof_check_refuses_instead_of_matching_an_over_limit_tree(self):
        args = rg.parse_args(["--plan", "--model", "claude-haiku-5-5"])
        with mock.patch.object(rg, "MANIFEST_FILE_CAP", 3):
            self.files(3)
            proof = make_proof(config_identity=rg.config_identity(self.cfg),
                               context_manifest=rg.config_manifest(self.cfg))
            self.assertEqual(rg.proof_problems(proof, args, self.cfg), [])
            self.files(1, prefix="z")  # now over the cap
            self.assertEqual(rg.proof_problems(proof, args, self.cfg),
                             ["configuration_manifest_refused_too_many_files"])

    def test_the_default_cap_refuses_the_reviewers_5002_files(self):
        self.files(5002)
        proof = make_proof(config_identity=rg.config_identity(self.cfg), context_manifest={})
        args = rg.parse_args(["--plan", "--model", "claude-haiku-5-5"])
        self.assertEqual(rg.proof_problems(proof, args, self.cfg),
                         ["configuration_manifest_refused_too_many_files"])

    def test_an_unreadable_file_is_a_refusal_too(self):
        self.files(1)
        with mock.patch.object(Path, "read_bytes", side_effect=PermissionError("denied")):
            with self.assertRaises(rg.ManifestRefused) as caught:
                rg.config_manifest(self.cfg)
        self.assertEqual(str(caught.exception), "unreadable")

    def test_the_preflight_fails_and_records_no_manifest_for_an_over_limit_directory(self):
        class Base(PreflightBase):
            def runTest(self):  # pragma: no cover - only to instantiate the helper
                pass
        helper = Base()
        helper.setUp = lambda: None
        helper.addCleanup = self.addCleanup
        with mock.patch.object(rg, "MANIFEST_FILE_CAP", 2):
            proof = helper.run_preflight("normal", files={})
            # the helper made an empty production directory; add the over-limit tree and rerun
        production = helper.production
        (production / "rules").mkdir()
        for i in range(3):
            (production / "rules" / f"r{i}.md").write_text(f"a distinctive rule number {i} here\n", encoding="utf-8")
        import argparse
        args = argparse.Namespace(exe=Path(sys.executable), model="claude-haiku-5-5",
                                  claude=[sys.executable, str(HERE / "fake_claude.py")], timeout=60,
                                  claude_config_dir=production, extra_env={})
        with mock.patch.object(rg, "MANIFEST_FILE_CAP", 2), mock.patch.object(preflight, "refusals",
                                                                              return_value=[]):
            proof = preflight.run(args)
        self.assertFalse(proof["passed"])
        self.assertIn("context_manifest_refused_too_many_files", proof["problems"])
        self.assertEqual(proof["context_manifest"], {})

    def test_credentials_and_volatile_counters_stay_excluded(self):
        (self.cfg / ".credentials.json").write_text('{"t": "secret"}', encoding="utf-8")
        (self.cfg / ".claude.json").write_text(json.dumps({"numStartups": 3}), encoding="utf-8")
        self.assertEqual(set(rg.config_manifest(self.cfg)), {"rules"})  # the empty rules folder only


class ClosedStreamVocabularyTests(unittest.TestCase):
    """N4: event kinds and subtypes are enumerated; everything else is one fixed marker."""

    CANARIES = ("private_screen_canary", "private_observation", "private_result_canary", "PRIVATE-SCREEN-CANARY")

    def feed(self, events):
        metrics = rg.StreamMetrics([], None, make_proof())
        for event in events:
            metrics.feed(json.dumps(event))
        return metrics

    def test_lowercase_and_underscore_canaries_are_not_retained(self):
        metrics = self.feed([
            {"type": "private_screen_canary", "subtype": "private_observation"},
            {"type": "system", "subtype": "private_observation"},
            {"type": "result", "subtype": "private_result_canary", "is_error": False, "result": "VERDICT: DONE"},
            {"type": "PRIVATE-SCREEN-CANARY"}])
        text = json.dumps(metrics.summary())
        for canary in self.CANARIES:
            self.assertNotIn(canary, text)
        self.assertEqual(metrics.summary()["cli_subtype"], "other")
        self.assertEqual(metrics.summary()["event_types"],
                         {"other": 1, "other/other": 1, "result/other": 1, "system/other": 1})

    def test_reviewed_kinds_and_subtypes_keep_their_names(self):
        metrics = self.feed([{"type": "system", "subtype": "init"}, {"type": "assistant"}, {"type": "user"},
                             {"type": "rate_limit_event"},
                             {"type": "result", "subtype": "success", "result": "VERDICT: DONE"}])
        self.assertEqual(metrics.summary()["event_types"], {"assistant": 1, "rate_limit_event": 1,
                                                            "result/success": 1, "system/init": 1, "user": 1})
        self.assertEqual(metrics.summary()["cli_subtype"], "success")

    def test_every_key_is_in_the_closed_set(self):
        allowed = {"other", "other/other"}
        for kind, subtypes in rg.EVENT_KINDS.items():
            allowed |= {kind, f"{kind}/other", *(f"{kind}/{sub}" for sub in subtypes)}
        metrics = self.feed([{"type": t, "subtype": s} for t in ("system", "result", "zzz", None, 3)
                             for s in ("init", "success", "zzz", None, 4)])
        self.assertLessEqual(set(metrics.summary()["event_types"]), allowed)

    def test_the_whole_serialised_goal_result_is_free_of_lowercase_canaries(self):
        init = GOALS["initial_states"]["testapp"]
        elements = [{"target_id": "a", "revision": 1, "label": "Sumar"}]
        events = [*observation("o1", elements),
                  {"type": "private_screen_canary", "subtype": "private_observation"},
                  {"type": "result", "subtype": "private_result_canary", "is_error": False,
                   "result": "VERDICT: DONE"}]
        h = Harness(self, "G01", 1, final_state=dict(init), events=events,
                    proxy_records=[{"dir": "proxy", "kind": "end", "engine_exit": 0}])
        text = json.dumps(h.run())
        for canary in self.CANARIES:
            self.assertNotIn(canary, text)


class ControlCharacterTests(unittest.TestCase):
    """N5: no control character in any safety path, in either phase."""

    CONTROLS = ("\n", "\r", "\t", "\x1f", "\x07", "\x00", "\x01", "\x7f")

    def test_controls_are_refused_in_absolute_and_relative_forms(self):
        for ch in self.CONTROLS:
            for form in (f"C:\\ops\\stop{ch}.flag", f"C:\\ops{ch}\\stop.flag", f"C:\\ops\\stop.flag{ch}",
                         f"stop{ch}.flag", f"sub{ch}/stop.flag", f"{ch}stop.flag"):
                self.assertEqual(rg.raw_path_problem(form), "control characters are not accepted", repr(form))
        self.assertIsNone(rg.raw_path_problem("C:\\ops\\stop.flag"))

    def test_both_safety_paths_are_checked_in_both_phases(self):
        base = Path(tempfile.gettempdir())
        for phase in (1, 2):
            for field in ("stop_file", "purge_file"):
                safety = {"stop_file": str(base / "stop.flag"), "purge_file": str(base / "purge.flag")}
                if phase == 2:
                    safety |= {"actuation_enabled": True,
                               "allowed_applications": ["ommatix-testapp.exe", "ommatix-testeditor.exe"]}
                safety[field] = "C:\\ops\\bad\nname.flag"
                config = {"engine": {"max_snapshot_age_ms": 10000}, "models": {"accessibility": "uia"},
                          "safety": safety}
                problems = rg.config_problems(phase, config, base)
                self.assertTrue(any(field in p and "control characters" in p for p in problems), (phase, field))

    def test_main_rejects_a_newline_in_a_stop_path_before_a_proof_an_application_or_an_agent(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "cfg").mkdir()
            config = tmp / "cfg" / "engine.toml"
            config.write_text('[engine]\nmax_snapshot_age_ms = 10000\n[models]\naccessibility = "uia"\n'
                              '[safety]\nactuation_enabled = true\n'
                              'allowed_applications = ["ommatix-testapp.exe", "ommatix-testeditor.exe"]\n'
                              'stop_file = "' + str(tmp).replace("\\", "\\\\") + '\\\\stop\\n.flag"\n'
                              f'purge_file = {json.dumps(str(tmp / "purge.flag"))}\n', encoding="utf-8")
            argv = ["--phase", "2", "--live", "--max-live-session", "3", "--ledger", str(tmp / "led.json"),
                    "--model", "m", "--exe", str(tmp / "ommatix.exe"), "--config", str(config),
                    "--isolation-proof", str(tmp / "proof.json"), "--claude-config-dir", str(tmp / "home"),
                    "--out", str(tmp / "out")]
            with mock.patch.object(rg, "windows_driver") as driver, mock.patch.object(rg, "run_series") as series, \
                    mock.patch.object(rg, "load_isolation_proof") as proof:
                with self.assertRaises(SystemExit) as caught:
                    rg.main(argv)
            self.assertIn("control characters", str(caught.exception))
            for started in (driver, series, proof):
                started.assert_not_called()
            self.assertFalse((tmp / "out").exists())

    def test_the_stop_path_resolver_refuses_a_control_character_too(self):
        with self.assertRaises(ValueError):
            rg.stop_path_of({"safety": {"stop_file": "C:\\ops\\stop\n.flag"}}, Path(tempfile.gettempdir()))


if __name__ == "__main__":
    unittest.main()
