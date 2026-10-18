"""Regression tests for the six findings of the second review of PR 86 (R1-R6). See
docs/reviews/2026-10-17-pr86-ag02-harness.md. No Claude, no network, no screen."""

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import run_goal as rg  # noqa: E402
from test_run_goal import (BY_ID, GOALS, Harness, act, action_result, clean_init, final,  # noqa: E402
                           make_proof, observation)

CANARY = "PRIVATE-SCREEN-CANARY"
REFUSED_FORMS = ["C:STOP", "c:stop.flag", "\\STOP", "\\\\srv\\share\\stop", "//srv/share/stop",
                 "\\\\?\\C:\\stop", "C:\\x\\..\\stop", "C:\\x\\\\stop", "STOP.", "NUL", "stop:stream", "a b",
                 "", "  ", "./stop", "../stop", "sub/../stop", "C:\\x\\NUL.txt"]
if os.name == "nt":
    REFUSED_FORMS.append("/STOP")  # root-relative on Windows; an absolute path elsewhere
ACCEPTED_FORMS = ["STOP", "stop.flag", "sub/stop.flag", "sub\\stop.flag", "C:\\STOP", "C:/ops/stop",
                  "C:\\Users\\a b\\stop.flag"]


class DriveRelativePathTests(unittest.TestCase):
    """R1: a written form the engine and Python would resolve differently is refused, not emulated."""

    def test_ambiguous_forms_are_refused_and_plain_ones_accepted(self):
        for form in REFUSED_FORMS:
            self.assertIsNotNone(rg.raw_path_problem(form), form)
            with self.assertRaises(ValueError, msg=form):
                rg.engine_path(Path("C:/operator/config"), form)
        for form in ACCEPTED_FORMS:
            self.assertIsNone(rg.raw_path_problem(form), form)

    def test_the_rules_do_not_depend_on_the_platform_the_check_runs_on(self):
        for name in ("nt", "posix"):
            with mock.patch.object(rg.os, "name", name):
                for form in ("C:STOP", "\\STOP", "\\\\srv\\s", "//srv/s", "..", "a b", "stop:x"):
                    self.assertIsNotNone(rg.raw_path_problem(form), (name, form))
                self.assertIsNone(rg.raw_path_problem("STOP"), name)
                self.assertIsNone(rg.raw_path_problem("C:\\STOP"), name)
        with mock.patch.object(rg.os, "name", "nt"):
            self.assertIsNotNone(rg.raw_path_problem("/STOP"))
        with mock.patch.object(rg.os, "name", "posix"):
            self.assertIsNone(rg.raw_path_problem("/var/ops/stop"))

    def test_a_drive_relative_stop_file_is_refused_in_every_phase_before_anything_starts(self):
        base = Path(tempfile.gettempdir())
        for phase, extra in ((1, {}), (2, {"actuation_enabled": True,
                                           "allowed_applications": ["ommatix-testapp.exe", "ommatix-testeditor.exe"],
                                           "purge_file": str(base / "purge.flag")})):
            config = {"engine": {"max_snapshot_age_ms": 10000}, "models": {"accessibility": "uia"},
                      "safety": {"stop_file": "C:STOP", **extra}}
            problems = rg.config_problems(phase, config, base)
            self.assertTrue(any("stop_file" in p and "drive-relative" in p for p in problems), (phase, problems))
        with self.assertRaises(ValueError):
            rg.stop_path_of({"safety": {"stop_file": "C:STOP"}}, base)

    def test_main_refuses_the_configuration_before_a_proof_an_application_or_an_agent(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "cfg").mkdir()
            config = tmp / "cfg" / "engine.toml"
            config.write_text('[engine]\nmax_snapshot_age_ms = 10000\n[models]\naccessibility = "uia"\n'
                              '[safety]\nactuation_enabled = true\n'
                              'allowed_applications = ["ommatix-testapp.exe", "ommatix-testeditor.exe"]\n'
                              f'stop_file = \'C:STOP\'\npurge_file = {json.dumps(str(tmp / "purge.flag"))}\n',
                              encoding="utf-8")
            argv = ["--phase", "2", "--live", "--max-live-session", "3", "--ledger", str(tmp / "led.json"),
                    "--model", "m", "--exe", str(tmp / "ommatix.exe"), "--config", str(config),
                    "--isolation-proof", str(tmp / "proof.json"), "--claude-config-dir", str(tmp / "home"),
                    "--out", str(tmp / "out")]
            with mock.patch.object(rg, "windows_driver") as driver, mock.patch.object(rg, "run_series") as series, \
                    mock.patch.object(rg, "load_isolation_proof") as proof:
                with self.assertRaises(SystemExit) as caught:
                    rg.main(argv)
            self.assertIn("drive-relative", str(caught.exception))
            for started in (driver, series, proof):
                started.assert_not_called()
            self.assertFalse((tmp / "out").exists())

    def test_ordinary_relative_and_absolute_stop_paths_still_resolve_like_the_engine(self):
        base = Path(tempfile.gettempdir()) / "cfg"
        self.assertEqual(rg.stop_path_of({"safety": {"stop_file": "STOP"}}, base), base / "STOP")
        self.assertEqual(rg.stop_path_of({"safety": {"stop_file": "C:\\STOP" if os.name == "nt" else "/STOP"}},
                                         base), Path("C:\\STOP" if os.name == "nt" else "/STOP"))


@unittest.skipUnless(sys.platform == "win32" and shutil.which("rustc"), "needs Windows and rustc")
class AgainstTheEnginesRustResolverTests(unittest.TestCase):
    """The accepted forms resolve exactly as ``Path::is_relative``/``join`` (the engine's
    ``Config::resolve_path``) resolves them; the refused ones are the ones that would not."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="ag02-rust-")
        source = Path(cls.tmp.name) / "resolve.rs"
        source.write_text(
            'use std::path::Path;\nfn main() {\n let mut a = std::env::args().skip(1);\n'
            ' let base = a.next().unwrap();\n for p in a {\n  let pp = Path::new(&p);\n'
            '  let r = if pp.is_relative() { Path::new(&base).join(pp) } else { pp.to_path_buf() };\n'
            '  println!("{}", r.display());\n }\n}\n', encoding="utf-8")
        cls.exe = Path(cls.tmp.name) / "resolve.exe"
        done = subprocess.run(["rustc", "--edition", "2021", "-o", str(cls.exe), str(source)],
                              capture_output=True, text=True, check=False)
        if done.returncode != 0:
            cls.tmp.cleanup()
            raise unittest.SkipTest("rustc could not build the resolver probe")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def rust(self, base, forms):
        done = subprocess.run([str(self.exe), str(base), *forms], capture_output=True, text=True, check=True)
        return done.stdout.splitlines()

    def test_accepted_forms_resolve_identically_and_the_drive_relative_one_would_not(self):
        base = Path(self.tmp.name) / "config"
        norm = lambda text: os.path.normcase(os.path.normpath(str(text)))  # noqa: E731
        forms = ["C:STOP", *ACCEPTED_FORMS]
        for form, theirs in zip(forms, self.rust(base, forms)):
            if rg.raw_path_problem(form) is None:
                self.assertEqual(norm(rg.engine_path(base, form)), norm(theirs), form)
        # the review's case: Python would have said <config>\STOP, Rust says C:STOP
        theirs = self.rust(base, ["C:STOP"])[0]
        self.assertEqual(theirs, "C:STOP")
        self.assertNotEqual(norm(base / "STOP"), norm(theirs))
        self.assertIsNotNone(rg.raw_path_problem("C:STOP"))


class ProviderRoutingTests(unittest.TestCase):
    """R2, the pure parts (the preflight's own tests are in test_preflight.py)."""

    def test_routing_problems_names_the_variables_and_never_their_values(self):
        env = {"CLAUDE_CODE_USE_BEDROCK": "1", "ANTHROPIC_BEDROCK_BASE_URL": "https://secret.invalid",
               "ANTHROPIC_BASE_URL": "http://127.0.0.1:1", "ANTHROPIC_API_KEY": "sk", "PATH": "x",
               "CLAUDE_CODE_USE_VERTEX": "", "ANTHROPIC_FOUNDRY_RESOURCE": "r"}
        found = rg.routing_problems(env)
        self.assertEqual(found, ["ANTHROPIC_BASE_URL", "ANTHROPIC_BEDROCK_BASE_URL",
                                 "ANTHROPIC_FOUNDRY_RESOURCE", "CLAUDE_CODE_USE_BEDROCK"])
        self.assertNotIn("secret", json.dumps(found))

    def test_scrubbed_env_keeps_the_basics_and_drops_everything_that_routes_or_authenticates(self):
        parent = {"PATH": "p", "SYSTEMROOT": "s", "TEMP": "t", "CLAUDE_CODE_USE_BEDROCK": "1",
                  "ANTHROPIC_API_KEY": "real", "ANTHROPIC_AUTH_TOKEN": "real", "AWS_ACCESS_KEY_ID": "a",
                  "AWS_PROFILE": "p", "GOOGLE_APPLICATION_CREDENTIALS": "g", "HTTPS_PROXY": "h",
                  "OTEL_EXPORTER_OTLP_ENDPOINT": "o", "CLAUDE_CODE_OAUTH_TOKEN": "t", "DISABLE_TELEMETRY": "1",
                  "ANTHROPIC_MODEL": "m", "SOMETHING_ELSE": "x"}
        with mock.patch.dict(os.environ, parent, clear=True):
            env = rg.scrubbed_env({"ANTHROPIC_BASE_URL": "http://127.0.0.1:5", "ANTHROPIC_API_KEY": "dummy"})
        self.assertEqual({k: env[k] for k in ("PATH", "SYSTEMROOT", "TEMP")},
                         {"PATH": "p", "SYSTEMROOT": "s", "TEMP": "t"})
        self.assertEqual((env["ANTHROPIC_BASE_URL"], env["ANTHROPIC_API_KEY"]), ("http://127.0.0.1:5", "dummy"))
        for name in ("CLAUDE_CODE_USE_BEDROCK", "ANTHROPIC_AUTH_TOKEN", "AWS_ACCESS_KEY_ID", "AWS_PROFILE",
                     "GOOGLE_APPLICATION_CREDENTIALS", "HTTPS_PROXY", "OTEL_EXPORTER_OTLP_ENDPOINT",
                     "CLAUDE_CODE_OAUTH_TOKEN", "DISABLE_TELEMETRY", "ANTHROPIC_MODEL", "SOMETHING_ELSE"):
            self.assertNotIn(name, env, name)
        self.assertEqual(env["CLAUDE_CODE_DISABLE_CLAUDE_MDS"], "1")

    def test_a_goal_run_refuses_provider_routing_variables_in_its_own_environment(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "home").mkdir()
            argv = ["--smoke", "--model", "m", "--exe", str(tmp / "o.exe"), "--out", str(tmp / "out")]
            with mock.patch.dict(os.environ, {"CLAUDE_CODE_USE_BEDROCK": "1"}):
                with self.assertRaises(SystemExit) as caught:
                    rg.main(argv)
            self.assertIn("CLAUDE_CODE_USE_BEDROCK", str(caught.exception))
            self.assertFalse((tmp / "out").exists())

    def test_managed_configuration_files_are_reported_with_closed_tokens(self):
        with tempfile.TemporaryDirectory() as tmp:
            present = Path(tmp) / "managed-settings.json"
            present.write_text("{}", encoding="utf-8")
            self.assertEqual(rg.managed_config_found([present, Path(tmp) / "absent.json"]),
                             ["managed_settings_file_0"])
            self.assertEqual(rg.managed_config_found([Path(tmp) / "absent.json"]), [])


class ProofIsBoundToTheProductionContextTests(unittest.TestCase):
    """R3: the proof says which configuration, model and context it covers."""

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-r3-")
        self.addCleanup(scratch.cleanup)
        self.tmp = Path(scratch.name)
        self.cfg = self.tmp / "cfg"
        self.cfg.mkdir()

    def problems(self, init=None, proof=None, config_dir=None):
        att = rg.init_attestation(init or clean_init(memory_paths={"auto": str(self.cfg / "projects" / "s" / "memory")}))
        return rg.init_problems(att, proof or make_proof(), config_dir if config_dir is not None else self.cfg)

    def test_the_memory_value_is_checked_not_just_the_familiar_key(self):
        self.assertEqual(self.problems(), [])
        self.assertIn("memory_paths_not_a_path", self.problems(clean_init(memory_paths={"auto": [CANARY]})))
        self.assertIn("memory_paths_outside_the_configuration",
                      self.problems(clean_init(memory_paths={"auto": str(self.tmp / "elsewhere")})))
        self.assertIn("memory_paths_unreadable", self.problems(clean_init(memory_paths="unexpected")))
        loaded = self.cfg / "projects" / "s" / "memory"
        loaded.mkdir(parents=True)
        (loaded / "MEMORY.md").write_text("- a remembered thing\n", encoding="utf-8")
        self.assertIn("memory_loaded", self.problems(clean_init(memory_paths={"auto": str(loaded)})))
        self.assertNotIn(CANARY, json.dumps(self.problems(clean_init(memory_paths={"auto": [CANARY]}))))

    def test_a_different_valid_model_is_refused_and_so_is_a_different_requested_model(self):
        self.assertIn("model_differs_from_proof", self.problems(
            clean_init(model="claude-sonnet-5-5", memory_paths={"auto": str(self.cfg / "p" / "memory")})))
        args = rg.parse_args(["--plan", "--model", "claude-sonnet-5-5"])
        proof = make_proof(config_identity=rg.config_identity(self.cfg), context_manifest=rg.config_manifest(self.cfg))
        self.assertIn("requested_model_differs_from_proof", rg.proof_problems(proof, args, self.cfg))

    def test_a_same_named_plugin_or_agent_with_a_different_entry_is_a_different_fact(self):
        mem = {"auto": str(self.cfg / "p" / "memory")}
        other = clean_init(memory_paths=mem, plugins=[{"name": "p1", "path": "/elsewhere", "version": "2"}])
        self.assertIn("plugins_differ", self.problems(other))
        self.assertIn("agents_differ", self.problems(clean_init(memory_paths=mem, agents=[{"name": "a1", "x": 1}])))

    def test_an_init_field_nobody_reviewed_is_a_change(self):
        mem = {"auto": str(self.cfg / "p" / "memory")}
        self.assertIn("init_fields_differ", self.problems(clean_init(memory_paths=mem, brand_new_field=[])))

    def test_the_configuration_identity_and_manifest_must_match_the_proof(self):
        args = rg.parse_args(["--plan", "--model", "claude-haiku-5-5"])
        proof = make_proof(config_identity=rg.config_identity(self.cfg), context_manifest=rg.config_manifest(self.cfg))
        self.assertEqual(rg.proof_problems(proof, args, self.cfg), [])
        other = self.tmp / "other"
        other.mkdir()
        self.assertIn("configuration_directory_differs_from_proof", rg.proof_problems(proof, args, other))
        self.assertEqual(rg.proof_problems(proof, args, None), ["no_configuration_directory"])
        (self.cfg / "CLAUDE.md").write_text("added after the proof\n", encoding="utf-8")
        self.assertIn("configuration_manifest_differs_from_proof", rg.proof_problems(proof, args, self.cfg))

    def test_the_manifest_sees_context_changes_and_ignores_credentials_and_session_noise(self):
        empty = rg.config_manifest(self.cfg)
        self.assertEqual(empty, {})
        (self.cfg / ".credentials.json").write_text('{"token": "secret"}', encoding="utf-8")
        (self.cfg / "history.jsonl").write_text("noise\n", encoding="utf-8")
        (self.cfg / ".claude.json").write_text(json.dumps({"numStartups": 7}), encoding="utf-8")
        before = rg.config_manifest(self.cfg)
        (self.cfg / ".claude.json").write_text(json.dumps({"numStartups": 8}), encoding="utf-8")
        self.assertEqual(rg.config_manifest(self.cfg), before)  # volatile counters are not context
        (self.cfg / ".claude.json").write_text(json.dumps({"numStartups": 8, "mcpServers": {"x": {}}}),
                                               encoding="utf-8")
        self.assertNotEqual(rg.config_manifest(self.cfg), before)
        (self.cfg / "rules").mkdir()
        (self.cfg / "rules" / "r.md").write_text("one\n", encoding="utf-8")
        first = rg.config_manifest(self.cfg)["rules"]
        (self.cfg / "rules" / "r.md").write_text("two\n", encoding="utf-8")
        self.assertNotEqual(rg.config_manifest(self.cfg)["rules"], first)
        memory = self.cfg / "projects" / "p" / "memory"
        memory.mkdir(parents=True)
        (memory / "MEMORY.md").write_text("- x\n", encoding="utf-8")
        self.assertIn("projects/*/memory", rg.config_manifest(self.cfg))
        self.assertNotIn("secret", json.dumps(rg.config_manifest(self.cfg)))

    def test_context_files_in_the_session_directory_or_its_ancestors_are_found(self):
        scratch = self.tmp / "a" / "b" / "scratch"
        scratch.mkdir(parents=True)
        self.assertEqual(rg.ancestor_context(scratch, self.cfg), [])
        (self.tmp / "a" / "CLAUDE.md").write_text("x\n", encoding="utf-8")
        (scratch / ".mcp.json").write_text("{}", encoding="utf-8")
        (scratch / ".claude").mkdir()
        (scratch / ".claude" / "settings.json").write_text("{}", encoding="utf-8")
        found = rg.ancestor_context(scratch, self.cfg)
        self.assertIn("ancestor_2_claude_md", found)
        self.assertIn("project_mcp_json", found)
        self.assertIn("project_settings_json", found)

    def test_a_run_whose_configuration_differs_from_the_proof_is_aborted_before_the_agent_starts(self):
        init = GOALS["initial_states"]["testapp"]
        h = Harness(self, "G01", 1, final_state=dict(init), events=final("DONE"))
        (h.cfg / "CLAUDE.md").write_text("drift\n", encoding="utf-8")  # after the proof was made
        result = h.run()
        self.assertIn("configuration_manifest_differs_from_proof", result["aborted"])
        self.assertFalse(h.captured)
        h2 = Harness(self, "G01", 1, final_state=dict(init), events=final("DONE"))
        self.assertIn("ancestor_3_claude_md", h2.run(ancestors=["ancestor_3_claude_md"])["aborted"])
        self.assertFalse(h2.captured)

    def test_the_session_is_started_in_the_proven_configuration_directory(self):
        init = GOALS["initial_states"]["testapp"]
        h = Harness(self, "G01", 1, final_state=dict(init), events=final("DONE"))
        seen = {}
        original = h.deps.run_claude

        def spy(argv, prompt, cwd, metrics, timeout, *a, **k):
            seen["env"] = k.get("env")
            return original(argv, prompt, cwd, metrics, timeout, *a, **k)
        h.deps.run_claude = staticmethod(spy)
        h.run()
        self.assertEqual(seen["env"]["CLAUDE_CONFIG_DIR"], str(h.cfg))
        self.assertEqual(seen["env"]["CLAUDE_CODE_DISABLE_CLAUDE_MDS"], "1")


class FailedObservationTests(unittest.TestCase):
    """R4: a failed or unreadable newest observation leaves the agent with no current evidence."""

    ELEMENTS = [{"target_id": "a", "revision": 3, "label": "Sumar"}]

    def metrics(self, events):
        m = rg.StreamMetrics(["Sumar"], None)
        for event in events:
            m.feed(json.dumps(event))
        return m

    def unparsed(self, tool_id):
        return [
            {"type": "assistant", "message": {"id": f"m{tool_id}", "content": [
                {"type": "tool_use", "id": tool_id, "name": "mcp__ommatix__observe_screen", "input": {}}]}},
            {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tool_id,
                                                      "content": "this is not json at all"}]}}]

    def test_good_then_failed_then_action_has_no_evidence(self):
        events = (observation("o1", self.ELEMENTS) + observation("o2", self.ELEMENTS, is_error=True)
                  + [act("a1", "a", 3), action_result("a1", "simulated", False)])
        action = self.metrics(events).actions[0]
        self.assertFalse(action["observed_before"])
        self.assertFalse(action["revision_current"])
        self.assertEqual(action["label"], "unknown")

    def test_good_then_unparsed_then_action_has_no_evidence(self):
        events = (observation("o1", self.ELEMENTS) + self.unparsed("o2")
                  + [act("a1", "a", 3), action_result("a1", "simulated", False)])
        action = self.metrics(events).actions[0]
        self.assertEqual((action["observed_before"], action["revision_current"]), (False, False))

    def test_a_successful_refresh_after_the_failure_restores_the_evidence(self):
        events = (observation("o1", self.ELEMENTS) + observation("o2", self.ELEMENTS, is_error=True)
                  + observation("o3", self.ELEMENTS) + [act("a1", "a", 3), action_result("a1", "simulated", False)])
        action = self.metrics(events).actions[0]
        self.assertEqual((action["observed_before"], action["revision_current"], action["label"]),
                         (True, True, "Sumar"))

    def test_a_failed_observation_after_an_action_does_not_satisfy_the_observe_again_rule(self):
        events = (observation("o1", self.ELEMENTS) + [act("a1", "a", 3), action_result("a1", "simulated", False)]
                  + observation("o2", self.ELEMENTS, is_error=True))
        metrics = self.metrics(events)
        self.assertEqual(metrics.observed_after, 0)
        self.assertTrue(metrics.awaiting_observation)
        refreshed = self.metrics(events + observation("o3", self.ELEMENTS))
        self.assertEqual(refreshed.observed_after, 1)


class AbbreviationTests(unittest.TestCase):
    """R5: the removed flag is an unrecognised argument, whatever the working directory is."""

    @contextmanager
    def cwd(self, path):
        before = os.getcwd()
        os.chdir(path)
        try:
            yield
        finally:
            os.chdir(before)

    def reject(self, argv):
        stderr = io.StringIO()
        with redirect_stderr(stderr), self.assertRaises(SystemExit):
            rg.parse_args(argv)
        return stderr.getvalue()

    def test_the_obsolete_flag_is_unrecognised_from_inside_and_outside_a_checkout(self):
        with tempfile.TemporaryDirectory() as outside, tempfile.TemporaryDirectory() as repo:
            (Path(repo) / ".git").mkdir()
            proof = str(Path(outside) / "proof.json")  # absolute and outside every checkout
            for where in (outside, repo):
                with self.cwd(where):
                    for argv in (["--plan", "--isolation", "none", "--isolation-proof", proof],
                                 ["--plan", "--isolation=none", "--isolation-proof", proof]):
                        message = self.reject(argv)
                        self.assertIn("unrecognized arguments", message)
                        self.assertIn("--isolation", message.split("unrecognized arguments")[1])
                        self.assertNotIn("outside the repository", message)  # not the checkout rule

    def test_no_flag_can_be_abbreviated_in_any_tool_and_for_the_right_reason(self):
        import preflight
        import proxy
        import summarise
        with tempfile.TemporaryDirectory() as outside:
            absolute = str(Path(outside) / "x.jsonl")
            (Path(outside) / "G01.json").write_text(json.dumps(
                {"goal": "G01", "phase": 1, "expected": "done", "score": {"success": True}, "metrics": {}}),
                encoding="utf-8")
            summary = str(Path(outside) / "s.json")
            probes = {
                "runner": lambda: rg.parse_args(["--plan", "--isol", absolute]),
                "runner_prefix": lambda: rg.parse_args(["--plan", "--max-live-s", "3"]),
                # every other argument is valid and absolute: only the abbreviation can be the reason
                "proxy": lambda: proxy.parse_args(["--dry-run-only", "--log", absolute, "--live-act", "click",
                                                   "--", "engine"]),
                "summary": lambda: summarise.main([outside, "--ou", summary]),
                "preflight": lambda: preflight.main(["--exe", sys.executable, "--model", "m",
                                                     "--claude-config-dir", outside,
                                                     "--out", str(Path(outside) / "p.json"), "--time", "5"]),
            }
            for name, probe in probes.items():
                stderr = io.StringIO()
                # if an abbreviation were accepted the preflight would go on to run a session: forbid it
                with redirect_stderr(stderr), redirect_stdout(io.StringIO()), self.assertRaises(SystemExit, msg=name),                         mock.patch.object(sys.modules["preflight"], "run", side_effect=AssertionError("parsed")):
                    probe()
                self.assertIn("unrecognized arguments", stderr.getvalue(), name)
            self.assertFalse(Path(summary).exists())

    def test_the_real_flags_still_parse(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = rg.parse_args(["--plan", "--isolation-proof", str(Path(tmp) / "p.json"),
                                  "--claude-config-dir", str(Path(tmp) / "h")])
            self.assertTrue(args.isolation_proof.is_absolute() and args.claude_config_dir.is_absolute())


class ToolNameSanitisationTests(unittest.TestCase):
    """R6: nothing the model or the session chose is persisted as a name or a code."""

    def test_an_invented_tool_name_never_reaches_the_summary(self):
        elements = [{"target_id": "a", "revision": 1, "label": "Sumar"}]
        invented = {"type": "assistant", "message": {"id": "mx", "content": [
            {"type": "tool_use", "id": "tx", "name": CANARY, "input": {"q": CANARY}}]}}
        result = {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "tx",
                                                           "is_error": True, "content": CANARY}]}}
        metrics = rg.StreamMetrics(["Sumar"], None, make_proof())
        for event in [clean_init(), *observation("o1", elements), invented, result,
                      *observation("o2", elements)]:
            metrics.feed(json.dumps(event))
        summary = metrics.summary()
        self.assertNotIn(CANARY, json.dumps(summary))
        self.assertEqual(summary["tool_calls"], {"observe_screen": 2, "unknown": 1})
        self.assertIn("unknown", [step["tool"] for step in summary["step_ms"]])
        self.assertTrue(set(summary["tool_calls"]) <= {"observe_screen", "execute_action",
                                                       "get_engine_metrics", "unknown"})

    def test_the_whole_serialised_goal_result_is_free_of_session_and_model_chosen_strings(self):
        init = GOALS["initial_states"]["testapp"]
        elements = [{"target_id": "a", "revision": 1, "label": "Sumar"}]
        invented = {"type": "assistant", "message": {"id": "mx", "content": [
            {"type": "tool_use", "id": "tx", "name": CANARY, "input": {}}]}}
        bad_result = {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "tx",
                                                               "content": json.dumps({"structuredContent": {
                                                                   "status": CANARY, "error": {"code": CANARY}}})}]}}
        weird = {"type": "result", "subtype": CANARY, "is_error": False, "result": "VERDICT: DONE"}
        h = Harness(self, "G01", 1, final_state=dict(init),
                    events=[*observation("o1", elements), invented, bad_result, weird], proxy_records=[
                        {"dir": "proxy", "kind": "end", "engine_exit": 0}])
        self.assertNotIn(CANARY, json.dumps(h.run()))

    def test_a_server_name_and_status_chosen_by_the_session_are_closed_before_they_are_stored(self):
        metrics = rg.StreamMetrics([], None, make_proof())
        metrics.feed(json.dumps(clean_init(
            mcp_servers=[{"name": CANARY, "status": CANARY}, {"name": "ommatix", "status": "connected"}],
            tools=list(rg.ALLOWED_TOOL_NAMES) + [CANARY])))
        summary = metrics.summary()
        self.assertNotIn(CANARY, json.dumps(summary))
        self.assertEqual(summary["mcp_status"], {"unknown": "unknown", "ommatix": "connected"})
        self.assertEqual(summary["unexpected_tool_count"], 1)
        self.assertEqual(sorted(summary["tools_listed"]), sorted(["observe_screen", "execute_action",
                                                                  "get_engine_metrics", "unknown"]))
        self.assertIn("tools_differ", summary["init_problems"])
        self.assertIn("mcp_servers_differ", summary["init_problems"])

    def test_only_the_three_exact_qualified_names_get_a_short_name(self):
        for name, short in zip(rg.ALLOWED_TOOL_NAMES, rg.TOOLS):
            self.assertEqual(rg.tool_short_name(name), short)
        for name in ("observe_screen", "mcp__ommatix__observe_screen_x", "mcp__other__observe_screen",
                     "mcp__ommatix__", CANARY, ""):
            self.assertEqual(rg.tool_short_name(name), "unknown", name)


if __name__ == "__main__":
    unittest.main()
