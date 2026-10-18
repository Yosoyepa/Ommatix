"""Tests of the AG-02 isolation preflight, against a scripted stand-in for the claude CLI and a
local stand-in for the API (no real model, no network beyond 127.0.0.1, no screen)."""

import argparse
import io
import json
import os
import sys
import tempfile
import unittest
import urllib.request
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import fake_api  # noqa: E402
import preflight  # noqa: E402
import run_goal as rg  # noqa: E402


class PreflightTests(unittest.TestCase):
    def run_preflight(self, mode="normal", **overrides):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-pf-")
        self.addCleanup(scratch.cleanup)
        self.argv_file = Path(scratch.name) / "argv.jsonl"
        args = argparse.Namespace(exe=Path(sys.executable), model="claude-haiku-5-5",
                                  claude=[sys.executable, str(HERE / "fake_claude.py")], timeout=60,
                                  **overrides)
        env = {"FAKE_CLAUDE_MODE": mode, "FAKE_CLAUDE_ARGV_FILE": str(self.argv_file)}
        with mock.patch.dict(os.environ, env):
            return preflight.run(args)

    def test_a_confined_session_passes_and_the_control_proves_the_canary_can_see_a_leak(self):
        proof = self.run_preflight()
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["production_leaked_kinds"], [])
        self.assertIn("user_claude_md", proof["control_leaked_kinds"])
        self.assertEqual(proof["bodies"], 1)
        self.assertEqual((proof["claude_code_version"], proof["model"]), ("2.1.302", "claude-haiku-5-5"))
        self.assertEqual(proof["tools"], sorted(rg.ALLOWED_TOOL_NAMES))
        self.assertEqual(proof["markers_after_production"], [])
        self.assertIn("no real API call was made", proof["differences_from_production"])

    def test_the_production_run_uses_the_runners_exact_flags_and_the_control_widens_only_the_sources(self):
        self.run_preflight()
        production, control = [json.loads(l) for l in self.argv_file.read_text().splitlines()]
        self.assertEqual(production[production.index("--setting-sources") + 1], "")
        self.assertEqual(production[production.index("--tools") + 1], "")
        self.assertIn("--strict-mcp-config", production)
        self.assertEqual(control[control.index("--setting-sources") + 1], "user,project,local")
        # Apart from the settings sources and the MCP config's own file name, the commands are equal.
        for argv, sources in ((production, ""), (control, "user,project,local")):
            argv[argv.index("--setting-sources") + 1] = "X"
            argv[argv.index("--mcp-config") + 1] = "M"
        self.assertEqual(production, control)

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
        self.assertIn("no_request_captured", proof["problems"])

    def test_a_passing_proof_is_accepted_by_the_runner_and_a_failing_one_is_not(self):
        good, bad = self.run_preflight(), self.run_preflight("leak")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "proof.json"
            path.write_text(json.dumps(good), encoding="utf-8")
            self.assertEqual(rg.load_isolation_proof(path)["claude_code_version"], "2.1.302")
            path.write_text(json.dumps(bad), encoding="utf-8")
            with self.assertRaises(SystemExit):
                rg.load_isolation_proof(path)

    def test_the_proof_must_be_written_outside_the_repository(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / ".git").mkdir()
            with redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    preflight.main(["--exe", sys.executable, "--model", "m",
                                    "--out", str(Path(tmp) / "proof.json")])

    def test_every_sentinel_kind_is_planted_and_distinct(self):
        with tempfile.TemporaryDirectory() as tmp:
            world = preflight.build_world(Path(tmp))
            self.assertEqual(set(world["sentinels"]), set(preflight.KINDS))
            self.assertEqual(len(set(world["sentinels"].values())), len(preflight.KINDS))
            planted = b"".join(p.read_bytes() for p in Path(tmp).rglob("*") if p.is_file())
            for kind, text in world["sentinels"].items():
                self.assertIn(text.encode(), planted, kind)
            self.assertTrue(all(not m.exists() for m in world["markers"].values()))


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
