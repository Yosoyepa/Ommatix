"""Tests of the AG-02 isolation preflight, against a scripted stand-in for the claude CLI and a
local stand-in for the API (no real model, no network beyond 127.0.0.1, no screen)."""

import argparse
import io
import json
import os
import sys
import tempfile
import unittest
import urllib.error
import urllib.request
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import fake_api  # noqa: E402
import preflight  # noqa: E402
import run_goal as rg  # noqa: E402

PROVIDER_ENV = {
    "CLAUDE_CODE_USE_BEDROCK": "1", "CLAUDE_CODE_USE_VERTEX": "1", "CLAUDE_CODE_USE_FOUNDRY": "1",
    "ANTHROPIC_BEDROCK_BASE_URL": "https://bedrock.example.invalid", "ANTHROPIC_VERTEX_BASE_URL": "https://v.invalid",
    "ANTHROPIC_FOUNDRY_RESOURCE": "r", "ANTHROPIC_AUTH_TOKEN": "tok-real", "ANTHROPIC_CUSTOM_HEADERS": "x: y",
    "AWS_ACCESS_KEY_ID": "AKIA-real", "AWS_PROFILE": "prod", "GOOGLE_APPLICATION_CREDENTIALS": "g.json",
    "AZURE_CLIENT_SECRET": "az", "CLAUDE_CODE_OAUTH_TOKEN": "oauth-real", "HTTPS_PROXY": "http://corp:3128",
    "OTEL_EXPORTER_OTLP_ENDPOINT": "https://otel.invalid", "ANTHROPIC_API_KEY": "sk-ant-real",
}


class PreflightBase(unittest.TestCase):
    def run_preflight(self, mode="normal", *, files=None, refusals=(), parent_env=None):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-pf-")
        self.addCleanup(scratch.cleanup)
        tmp = Path(scratch.name)
        self.argv_file, self.env_file = tmp / "argv.jsonl", tmp / "env.json"
        self.provider_file = tmp / "provider.txt"
        self.production = tmp / "production"
        self.production.mkdir()
        for name, text in (files or {}).items():
            (self.production / name).write_text(text, encoding="utf-8")
        extra = {"FAKE_CLAUDE_MODE": mode, "FAKE_CLAUDE_ARGV_FILE": str(self.argv_file),
                 "FAKE_CLAUDE_ENV_FILE": str(self.env_file), "FAKE_CLAUDE_PROVIDER_FILE": str(self.provider_file),
                 "FAKE_CLAUDE_LEAK_DIR": str(self.production)}
        args = argparse.Namespace(exe=Path(sys.executable), model="claude-haiku-5-5",
                                  claude=[sys.executable, str(HERE / "fake_claude.py")], timeout=60,
                                  claude_config_dir=self.production, extra_env=extra)
        with mock.patch.dict(os.environ, parent_env or {}), \
                mock.patch.object(preflight, "refusals", return_value=list(refusals)):
            return preflight.run(args)


class PreflightTests(PreflightBase):
    def test_a_confined_session_passes_and_the_control_proves_the_canary_can_see_a_leak(self):
        proof = self.run_preflight()
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["production_leaked_kinds"], [])
        self.assertIn("user_claude_md", proof["control_leaked_kinds"])
        self.assertEqual(proof["bodies"], 1)
        self.assertEqual((proof["claude_code_version"], proof["model"]), ("2.1.302", "claude-haiku-5-5"))
        self.assertEqual(proof["tools"], sorted(rg.ALLOWED_TOOL_NAMES))
        self.assertEqual(proof["markers_after_production"], [])
        self.assertEqual(proof["requested_model"], "claude-haiku-5-5")
        self.assertEqual(proof["config_identity"], rg.config_identity(self.production))
        self.assertEqual(proof["context_manifest"], {})
        self.assertTrue(proof["egress_controls"])

    def test_the_proof_of_a_passing_run_is_accepted_by_the_runners_loader(self):
        proof = self.run_preflight()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            self.assertEqual(rg.load_isolation_proof(path)["config_identity"], proof["config_identity"])

    def test_the_production_run_uses_the_runners_exact_flags_and_the_control_widens_only_the_sources(self):
        self.run_preflight()
        runs = [json.loads(l) for l in self.argv_file.read_text().splitlines()]
        production, control = runs[0], runs[1]
        self.assertEqual(production[production.index("--setting-sources") + 1], "")
        self.assertEqual(production[production.index("--tools") + 1], "")
        self.assertIn("--strict-mcp-config", production)
        self.assertEqual(control[control.index("--setting-sources") + 1], "user,project,local")
        for argv in (production, control):
            argv[argv.index("--setting-sources") + 1] = "X"
            argv[argv.index("--mcp-config") + 1] = "M"
        self.assertEqual(production, control)
        # the third session is the production configuration directory, with the production flags
        self.assertEqual(runs[2][runs[2].index("--setting-sources") + 1], "")

    def test_a_leak_in_the_production_request_fails_the_preflight(self):
        proof = self.run_preflight("leak")
        self.assertFalse(proof["passed"])
        self.assertIn("sentinels_in_production_request", proof["problems"])
        self.assertIn("user_claude_md", proof["production_leaked_kinds"])

    def test_a_canary_that_cannot_see_a_leak_even_in_the_control_fails(self):
        proof = self.run_preflight("blind")
        self.assertFalse(proof["passed"])
        self.assertIn("control_blind", proof["problems"])

    def test_a_hook_that_runs_in_production_fails_the_preflight(self):
        proof = self.run_preflight("hooks")
        self.assertFalse(proof["passed"])
        self.assertIn("hook_or_extra_server_ran_in_production", proof["problems"])
        self.assertIn("hook", proof["markers_after_production"])

    def test_an_extra_tool_at_init_fails_the_preflight(self):
        proof = self.run_preflight("extra_tool")
        self.assertFalse(proof["passed"])
        self.assertIn("init_tools_differ", proof["problems"])

    def test_no_captured_request_means_no_evidence(self):
        proof = self.run_preflight("silent")
        self.assertFalse(proof["passed"])
        self.assertIn("canary_session_no_request_captured", proof["problems"])

    def test_the_production_directorys_own_context_in_the_request_fails_the_preflight(self):
        line = "The operator's private instruction line number one."
        proof = self.run_preflight("leak", files={"CLAUDE.md": line + "\n"})
        self.assertIn("production_dir_context_in_request", proof["problems"])
        self.assertGreaterEqual(proof["production_dir_context_lines_in_request"], 1)
        self.assertIn("CLAUDE.md", proof["context_manifest"])
        clean = self.run_preflight(files={"CLAUDE.md": line + "\n"})
        self.assertTrue(clean["passed"], clean["problems"])
        self.assertEqual(clean["production_dir_context_lines_checked"], 1)
        self.assertEqual(clean["production_dir_context_lines_in_request"], 0)
        self.assertNotIn(line, json.dumps(clean))

    def test_a_context_file_in_an_ancestor_of_the_production_scratch_fails_the_preflight(self):
        with mock.patch.object(rg, "ancestor_context", return_value=["ancestor_3_claude_md"]):
            proof = self.run_preflight()
        self.assertIn("ancestor_context_present", proof["problems"])

    def test_the_proof_must_be_written_outside_the_repository_and_the_directory_must_exist(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / ".git").mkdir()
            existing = Path(tmp) / "elsewhere"
            existing.mkdir()
            with redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    preflight.main(["--exe", sys.executable, "--model", "m", "--claude-config-dir",
                                    str(existing), "--out", str(Path(tmp) / "proof.json")])
        with tempfile.TemporaryDirectory() as tmp, redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                preflight.main(["--exe", sys.executable, "--model", "m", "--claude-config-dir",
                                str(Path(tmp) / "missing"), "--out", str(Path(tmp) / "proof.json")])

    def test_every_sentinel_kind_is_planted_and_distinct(self):
        with tempfile.TemporaryDirectory() as tmp:
            world = preflight.build_world(Path(tmp))
            self.assertEqual(set(world["sentinels"]), set(preflight.KINDS))
            self.assertEqual(len(set(world["sentinels"].values())), len(preflight.KINDS))
            planted = b"".join(p.read_bytes() for p in Path(tmp).rglob("*") if p.is_file())
            for kind, text in world["sentinels"].items():
                self.assertIn(text.encode(), planted, kind)
            self.assertTrue(all(not m.exists() for m in world["markers"].values()))


class NoRealProviderTests(PreflightBase):
    """R2: the preflight must not be able to reach a real provider."""

    def test_the_child_environment_carries_no_provider_endpoint_credential_proxy_or_telemetry_variable(self):
        # the refusal is bypassed on purpose: this is the second line of defence
        proof = self.run_preflight(parent_env=PROVIDER_ENV)
        seen = json.loads(self.env_file.read_text(encoding="utf-8"))
        for name in PROVIDER_ENV:
            if name not in ("ANTHROPIC_API_KEY",):
                self.assertNotIn(name, seen["names"], name)
        self.assertTrue(seen["base_url"].startswith("http://127.0.0.1:"))
        self.assertEqual(seen["api_key"], "sk-ant-canary-0000000000")  # a dummy, never the real key
        self.assertFalse(self.provider_file.exists(), "the CLI would have routed to a provider")
        self.assertTrue(proof["passed"], proof["problems"])

    def test_the_cli_that_honours_provider_selectors_would_be_routed_elsewhere_without_the_scrub(self):
        # The control for the test above: with the operator's environment passed through, the same
        # stand-in CLI does select a provider - so the scrub is what prevents it.
        env = {**os.environ, **PROVIDER_ENV, "FAKE_CLAUDE_PROVIDER_FILE": str(Path(tempfile.gettempdir()) /
                                                                              f"ag02-prov-{os.getpid()}")}
        marker = Path(env["FAKE_CLAUDE_PROVIDER_FILE"])
        self.addCleanup(lambda: marker.unlink(missing_ok=True))
        import subprocess
        subprocess.run([sys.executable, str(HERE / "fake_claude.py")], input="", env={
            **env, "ANTHROPIC_BASE_URL": "http://127.0.0.1:9", "FAKE_CLAUDE_MODE": "silent"},
            capture_output=True, text=True, check=False)
        self.assertTrue(marker.exists())

    def test_the_preflight_refuses_to_run_when_routing_variables_are_set(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_USE_BEDROCK": "1",
                                          "ANTHROPIC_BEDROCK_BASE_URL": "https://b.invalid"}):
            with self.assertRaises(preflight.PreflightRefused) as caught:
                preflight.run(argparse.Namespace(claude_config_dir=Path(tempfile.gettempdir())))
        self.assertIn("routing_variable:CLAUDE_CODE_USE_BEDROCK", caught.exception.reasons)
        self.assertIn("routing_variable:ANTHROPIC_BEDROCK_BASE_URL", caught.exception.reasons)
        self.assertNotIn("b.invalid", str(caught.exception))  # names only, never values

    def test_the_preflight_refuses_to_run_under_a_managed_configuration(self):
        with mock.patch.object(rg, "managed_config_found", return_value=["managed_settings_file_0"]), \
                mock.patch.dict(os.environ, {}, clear=False):
            with self.assertRaises(preflight.PreflightRefused) as caught:
                preflight.run(argparse.Namespace(claude_config_dir=Path(tempfile.gettempdir())))
        self.assertIn("managed_settings_file_0", caught.exception.reasons)

    def test_main_exits_without_starting_any_session_when_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            production = Path(tmp) / "p"
            production.mkdir()
            buffer = io.StringIO()
            with mock.patch.object(preflight, "refusals", return_value=["routing_variable:X"]), \
                    mock.patch.object(preflight, "one_session") as session, redirect_stdout(buffer):
                code = preflight.main(["--exe", sys.executable, "--model", "m",
                                       "--claude-config-dir", str(production), "--out", str(Path(tmp) / "o.json")])
            self.assertEqual(code, 2)
            session.assert_not_called()
            self.assertIn("routing_variable:X", buffer.getvalue())
            self.assertFalse((Path(tmp) / "o.json").exists())


class FakeApiTests(unittest.TestCase):
    def post(self, api, path, payload):
        request = urllib.request.Request(api.url + path, data=json.dumps(payload).encode(),
                                         headers={"content-type": "application/json"})
        return urllib.request.urlopen(request, timeout=10)

    def test_it_records_message_bodies_and_answers_both_modes_on_loopback_only(self):
        with fake_api.FakeApi() as api:
            self.assertTrue(api.url.startswith("http://127.0.0.1:"))
            plain = json.loads(self.post(api, "/v1/messages", {"model": "m", "stream": False}).read())
            self.assertEqual(plain["content"][0]["text"], "VERDICT: DONE")
            streamed = self.post(api, "/v1/messages", {"model": "m", "stream": True}).read().decode()
            self.assertIn("message_stop", streamed)
            try:
                self.post(api, "/other", {})
            except urllib.error.HTTPError as error:
                self.assertEqual(error.code, 404)
            self.assertEqual(len(api.bodies), 2)


if __name__ == "__main__":
    unittest.main()
