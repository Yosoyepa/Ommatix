"""The pinned CLI and the tool-search switch in the AG-03 harness (see ``ag02/test_clipin.py`` for the pure checks).

The pixel agent's request tool list must be exactly ``screenshot``, ``click`` and ``type_text``: Claude Code adds a
``DeferredToolPlaceholder`` entry to every request unless ``ENABLE_TOOL_SEARCH=false`` (measured 2026-10-20 on 2.1.304
and 2.1.305). The environment of the pixel agent carries the switch (with AG-02's ``QUIET_ENV``), the proof records it
with ``cli_sha256`` and its date, and the runner launches nothing with another CLI, another environment or an expired
proof. Fakes only."""

import argparse
import contextlib
import calendar
import json
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import run_goal as rg  # noqa: E402
import clipin  # noqa: E402
from pin_support import PIN_SHA, launch_refused, pinned  # noqa: E402
from test_disk_evidence import PreflightHarness  # noqa: E402

SWITCH = "ENABLE_TOOL_SEARCH"


class PixelEnvironmentTests(unittest.TestCase):
    def test_the_agent_environment_is_ag02s_switches_plus_the_survey_switch(self):
        self.assertEqual(pa.AGENT_ENV, {**rg.QUIET_ENV, **pa.PIXEL_ENV})
        self.assertEqual(pa.AGENT_ENV[SWITCH], "false")
        self.assertEqual(pa.env_problems(pa.AGENT_ENV), [])
        self.assertEqual(pa.env_problems({k: v for k, v in pa.AGENT_ENV.items() if k != SWITCH}),
                         ["agent_environment_lacks_pixel_switches"])

    def test_the_native_and_route_environments_of_the_pixel_agent_carry_all_of_it(self):
        import pixel_runner as pr
        args = argparse.Namespace(route=None, model="claude-haiku-5-5", claude="claude", claude_config_dir=Path("cfg"))
        env = pr.PixelCLI(args).env({"CLAUDE_CONFIG_DIR": "cfg"})
        self.assertEqual(pa.env_problems(env), [])
        spec = agentcli.CLAUDE_ROUTES["deepseek"]
        key = Path(tempfile.mkdtemp()) / "key.txt"
        key.write_text("sk-test-0000000000000000", encoding="utf-8")
        self.addCleanup(key.unlink)
        route_args = argparse.Namespace(route="deepseek", route_base_url=spec["base_urls"][0], model=spec["model"],
                                        claude="claude", claude_config_dir=Path("cfg"), key_file=key)
        self.assertEqual(pa.env_problems(pr.PixelCLI(route_args).env({"CLAUDE_CONFIG_DIR": "cfg"})), [])


class PixelPreflightTests(PreflightHarness):
    def preflight(self, mode="normal", **kwargs):
        with pinned():
            return super().preflight(mode, **kwargs)

    def test_the_verifier_revisions_were_raised(self):
        self.assertGreaterEqual(pa.VERIFIER_REVISION, 3)
        self.assertGreaterEqual(pf.ROUTE_VERIFIER_REVISION, 2)

    def test_a_native_preflight_passes_only_because_the_switch_removes_the_placeholder(self):
        proof = self.preflight("deferred_placeholder")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["pixel"]["request_tools"], sorted(pa.ALLOWED_TOOL_NAMES))

    def test_a_route_preflight_too(self):
        proof = self.preflight("deferred_placeholder", route="deepseek")
        self.assertTrue(proof["passed"], proof["problems"])

    def test_the_proof_records_the_pin_the_environment_and_the_date(self):
        proof = self.preflight()
        self.assertEqual(proof["cli_sha256"], PIN_SHA)
        self.assertEqual(proof["agent_env"], pa.AGENT_ENV)

    def test_the_preflight_refuses_a_claude_that_is_not_the_pin_before_launching_anything(self):
        launched = mock.Mock(side_effect=AssertionError("a session was launched with an unpinned claude"))
        with pinned(None, ["claude_not_an_absolute_path"]), mock.patch.object(rg, "run_claude", launched):
            with self.assertRaises(pf.PixelRefused) as refused:
                super().preflight(pin=False)
        self.assertIn("claude_not_an_absolute_path", refused.exception.reasons)

    def test_the_loader_needs_the_pin_facts_and_proof_problems_compare_them(self):
        proof = self.preflight()
        with tempfile.TemporaryDirectory() as tmp:
            for name, broken in {"no_hash": {k: v for k, v in proof.items() if k != "cli_sha256"},
                                 "other_hash": proof | {"cli_sha256": "0" * 64},
                                 "other_env": proof | {"agent_env": dict(pa.PIXEL_ENV)}}.items():
                with self.subTest(broken=name):
                    path = Path(tmp) / f"{name}.json"
                    path.write_text(json.dumps(broken), encoding="utf-8")
                    with self.assertRaises(SystemExit):
                        pf.load_isolation_proof(path)
        args = argparse.Namespace(model="claude-haiku-5-5")
        self.assertIn("agent_env_differs_from_proof",
                      pf.proof_problems(proof | {"agent_env": dict(pa.PIXEL_ENV)}, args, self.production, None))


class CopyDirectoryEvidenceTests(PreflightHarness):
    """Re-review of PR 107 (R107-4, R107-5) on the AG-03 preflight with the REAL guard: the private copy lives inside the child's own
    monitored temp directory, so whatever the CLI writes beside its executable is scanned before the copy is removed; a copy that
    could not be removed fails the proof."""

    BYTES = b"MZ synthetic claude " * 64

    def run_real_guard(self, *, png=False, read_only=False, fail_cleanup=False):
        import hashlib
        import os
        import shutil
        import pixel_capture as cap
        scratch = tempfile.TemporaryDirectory(prefix="ag03-realguard-")
        self.addCleanup(scratch.cleanup)
        exe = Path(scratch.name) / "agent-cli" / "claude.exe"
        exe.parent.mkdir()
        exe.write_bytes(self.BYTES)
        pin = {"version": "9.9.9", "sha256": hashlib.sha256(self.BYTES).hexdigest()}
        real = rg.run_claude
        fake_cli = [sys.executable, str(HERE / "fake_claude_pixel.py")]

        def launch(argv, *a, **k):  # the "CLI" writes beside its executable, then the scripted stand-in runs
            if png:
                target = Path(argv[0]).parent / "synthetic.png"
                target.write_bytes(cap.synthetic_png(0, 64, 48)[0])
                if read_only:
                    os.chmod(target, 0o400)
            return real(fake_cli + argv[1:], *a, **k)
        leftovers = []
        patches = [mock.patch.object(clipin, "CLAUDE_CODE_PIN", pin),
                   mock.patch.object(clipin, "run_version", lambda path: "9.9.9 (Claude Code)"),
                   mock.patch.object(rg, "run_claude", launch)]
        if fail_cleanup:
            patches.append(mock.patch.object(clipin.shutil, "rmtree", lambda path, *a, **k: leftovers.append(path)))
        with contextlib.ExitStack() as stack:
            for patch in patches:
                stack.enter_context(patch)
            proof = self.preflight(pin=False, claude=str(exe))
        for path in leftovers:  # the test's own housekeeping of what the simulated failure left
            for root, _dirs, files in os.walk(path):
                for name in files:
                    os.chmod(Path(root) / name, 0o700)
            shutil.rmtree(path, ignore_errors=True)
        return proof, leftovers

    def test_a_clean_session_passes_and_leaves_no_copy(self):
        proof, _ = self.run_real_guard()
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(list(Path(tempfile.gettempdir()).glob("ommatix-cli-*")), [])

    def test_an_image_written_beside_the_launched_copy_refuses_the_proof_in_every_session(self):
        for read_only in (False, True):
            with self.subTest(read_only=read_only):
                proof, _ = self.run_real_guard(png=True, read_only=read_only)
                self.assertFalse(proof["passed"], "a PNG beside the launched executable went unnoticed")
                for name in ("canary", "control", "production_dir"):
                    self.assertIn(f"{name}_disk_temp_image_data", proof["problems"])

    def test_a_copy_that_could_not_be_removed_fails_the_proof(self):
        proof, leftovers = self.run_real_guard(fail_cleanup=True)
        self.assertTrue(leftovers, "the simulated cleanup failure did not happen")
        self.assertFalse(proof["passed"])
        self.assertIn("claude_copy_not_removed", proof["problems"])


class PixelLaunchTests(unittest.TestCase):
    def test_nothing_is_launched_with_a_cli_that_is_not_the_pin(self):
        from test_campaign import Series
        series = Series(self, patch_pin=False)
        with launch_refused("claude_hash_differs"):
            series.run()
        self.assertEqual(series.calls, [], "a session was launched with a CLI that is not the pin")

    def test_nothing_is_launched_with_an_expired_proof(self):
        from test_campaign import Series
        series = Series(self, patch_pin=False)
        two_days_later = calendar.timegm(time.strptime("2026-10-22T10:00:00Z", "%Y-%m-%dT%H:%M:%SZ"))

        @contextlib.contextmanager
        def aging(claude, proof=None, **kwargs):  # the guard of a pinned CLI, with the clock two days after the stand-in proof
            problems = clipin.age_problems(proof.get("date_utc"), two_days_later) if proof else []
            yield clipin.Verified(problems, PIN_SHA, None if problems else claude)
        with mock.patch.object(clipin, "verified", aging):
            series.run()  # the stand-in proof is dated 2026-10-20T10:00:00Z: more than 24 h before "now"
        self.assertEqual(series.calls, [])


if __name__ == "__main__":
    unittest.main()
