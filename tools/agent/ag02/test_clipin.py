"""The pinned agent CLI and the tool-search switch (decisions of 2026-10-20; docs/reviews/2026-10-20-agent-cli-pin.md).

Two facts, measured the same day with the real CLI against the loopback stand-in:

* Claude Code puts a ``DeferredToolPlaceholder`` entry in the tool list of EVERY request (2.1.304 and 2.1.305 alike, not
  tied to the version: it follows the CLI's own configuration), which the "exactly the three tools" verdicts refuse.
  ``ENABLE_TOOL_SEARCH=false`` in the agent's environment removes it, in both harnesses (AG-02R runs on the same CLI).
* The CLI is not the operator's own: the experiments run on ONE pinned binary (2.1.304, SHA-256 below) in a private
  directory, named by an absolute ``--claude`` path outside the repository; ``claude`` from PATH is refused, the hash and
  ``--version`` are checked before every launch, the proof records ``cli_sha256`` and is valid for at most 24 hours.

Fakes only: no real CLI, no provider, no screen, no real input."""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import agentcli  # noqa: E402
import preflight  # noqa: E402
import run_goal as rg  # noqa: E402
from pin_support import PIN_SHA, launch_refused, pinned  # noqa: E402

SWITCH = "ENABLE_TOOL_SEARCH"
NOT_GIVEN = object()




def fresh_date(offset=0.0):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + offset))


class CheckTests(unittest.TestCase):
    """``clipin.check``: what a ``--claude`` value must be before any launch (injected hasher and version reader)."""

    def setUp(self):
        import clipin
        self.clipin = clipin
        scratch = tempfile.TemporaryDirectory(prefix="ag-pin-")
        self.addCleanup(scratch.cleanup)
        self.exe = Path(scratch.name) / "claude.exe"
        self.exe.write_bytes(b"MZ fake")

    def check(self, claude=NOT_GIVEN, *, sha=PIN_SHA, version="2.1.304 (Claude Code)", **kwargs):
        def hasher(path):
            return sha

        def runner(exe):
            if isinstance(version, Exception):
                raise version
            return version
        return self.clipin.check(str(self.exe) if claude is NOT_GIVEN else claude, hasher=hasher, version_runner=runner,
                                 **kwargs)

    def test_the_pin_is_the_binary_of_the_owners_npm_verified_download(self):
        self.assertEqual(self.clipin.CLAUDE_CODE_PIN, {"version": "2.1.304", "sha256": PIN_SHA})

    def test_the_pinned_binary_passes_and_its_hash_is_returned(self):
        self.assertEqual(self.check(), ([], PIN_SHA))

    def test_claude_from_the_path_or_a_relative_name_is_refused(self):
        for name in ("claude", "claude.exe", r"bin\claude.exe", "./claude"):
            with self.subTest(name=name):
                self.assertIn("claude_not_an_absolute_path", self.check(name)[0])

    def test_a_command_that_is_not_a_single_executable_is_refused(self):
        for command in ([sys.executable, "fake_claude.py"], [], None, 5):
            with self.subTest(command=command):
                self.assertEqual(self.check(command)[0], ["claude_not_a_single_executable"])
        self.assertEqual(self.check([str(self.exe)])[0], [])  # a one-element list is that executable

    def test_a_missing_file_a_directory_and_a_binary_inside_the_repository_are_refused(self):
        self.assertIn("claude_missing", self.check(str(self.exe.parent / "absent.exe"))[0])
        self.assertIn("claude_missing", self.check(str(self.exe.parent))[0])
        self.assertIn("claude_inside_repository", self.check(repo_root=self.exe.parent)[0])

    def test_another_hash_or_another_version_is_refused(self):
        self.assertEqual(self.check(sha="0" * 64)[0], ["claude_hash_differs"])
        self.assertEqual(self.check(version="2.1.305 (Claude Code)")[0], ["claude_version_differs"])
        for version in ("", "garbage", subprocess.TimeoutExpired("claude", 30), OSError("no")):
            with self.subTest(version=repr(version)):
                self.assertEqual(self.check(version=version)[0], ["claude_version_unreadable"])

    def test_an_unreadable_file_is_refused(self):
        def hasher(path):
            raise PermissionError("denied")
        problems, sha = self.clipin.check(str(self.exe), hasher=hasher, version_runner=lambda exe: "2.1.304 (Claude Code)")
        self.assertEqual((problems, sha), (["claude_unreadable"], None))

    def test_the_real_hasher_streams_the_file(self):
        self.exe.write_bytes(b"abc")
        self.assertEqual(self.clipin.sha256_file(self.exe),
                         "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")


class AgeAndLaunchTests(unittest.TestCase):
    def setUp(self):
        import clipin
        self.clipin = clipin
        scratch = tempfile.TemporaryDirectory(prefix="ag-pin-age-")
        self.addCleanup(scratch.cleanup)
        self.exe = Path(scratch.name) / "claude.exe"
        self.exe.write_bytes(b"MZ fake")

    def launch(self, proof, *, sha=PIN_SHA, **kwargs):
        return self.clipin.launch_problems(str(self.exe), proof, hasher=lambda path: sha,
                                           version_runner=lambda exe: "2.1.304 (Claude Code)", **kwargs)

    def test_a_proof_is_valid_for_at_most_twenty_four_hours(self):
        self.assertEqual(self.clipin.PROOF_MAX_AGE_S, 24 * 3600)
        self.assertEqual(self.clipin.age_problems(fresh_date()), [])
        self.assertEqual(self.clipin.age_problems(fresh_date(-23 * 3600)), [])
        self.assertEqual(self.clipin.age_problems(fresh_date(-25 * 3600)), ["proof_expired"])
        self.assertEqual(self.clipin.age_problems(fresh_date(+3600)), ["proof_from_the_future"])
        for bad in (None, "", "yesterday", 5, "2026-10-20 10:00:00"):
            self.assertEqual(self.clipin.age_problems(bad), ["proof_date_unreadable"])

    def test_the_launch_check_joins_the_cli_the_hash_of_the_proof_and_the_age(self):
        proof = {"cli_sha256": PIN_SHA, "date_utc": fresh_date()}
        self.assertEqual(self.launch(proof), [])
        self.assertEqual(self.launch(proof | {"cli_sha256": "1" * 64}), ["cli_differs_from_proof"])
        self.assertEqual(self.launch(proof | {"date_utc": fresh_date(-90000)}), ["proof_expired"])
        self.assertEqual(self.launch(proof, sha="2" * 64), ["claude_hash_differs"])


class AgentEnvironmentTests(unittest.TestCase):
    """AG-02's agent environment carries the same switches the preflight child had (O11), and the tool-search switch."""

    def test_the_switches_are_the_ones_the_preflight_child_gets(self):
        self.assertEqual(rg.QUIET_ENV[SWITCH], "false")
        self.assertEqual(rg.QUIET_ENV["DISABLE_AUTOUPDATER"], "1")
        self.assertEqual(rg.scrubbed_env({})[SWITCH], "false")

    def test_the_native_production_environment_of_ag02_has_them(self):
        args = argparse.Namespace(route=None, model="claude-haiku-5-5", claude="claude", claude_config_dir=Path("cfg"))
        env = agentcli.ClaudeCLI(args).env({"CLAUDE_CONFIG_DIR": "cfg"})
        self.assertEqual((env[SWITCH], env["DISABLE_AUTOUPDATER"]), ("false", "1"))
        self.assertEqual(rg.env_problems(env), [])
        self.assertEqual(rg.env_problems({k: v for k, v in env.items() if k != SWITCH}),
                         ["agent_environment_lacks_switches"])

    def test_a_route_environment_has_the_tool_search_switch_without_a_route_extra_variable_problem(self):
        spec = agentcli.CLAUDE_ROUTES["deepseek"]
        env = agentcli.route_env("deepseek", spec["base_urls"][0], "sk-test-0000000000000000", spec["model"], "cfg")
        self.assertEqual(env[SWITCH], "false")
        self.assertEqual(agentcli.route_env_problems(env, "deepseek", spec["base_urls"]), [])


class Ag02ProofTests(unittest.TestCase):
    def good(self):
        import clipin
        from test_run_goal import make_proof
        return make_proof(cli_sha256=clipin.CLAUDE_CODE_PIN["sha256"], agent_env=dict(rg.QUIET_ENV),
                          claude_code_version=clipin.CLAUDE_CODE_PIN["version"])

    def load(self, proof):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            return rg.load_isolation_proof(path)

    def test_the_verifier_revision_was_raised_and_the_loader_needs_the_pin_facts(self):
        self.assertGreaterEqual(rg.VERIFIER_REVISION, 4)
        good = self.good()
        self.assertEqual(self.load(good)["cli_sha256"], PIN_SHA)
        for name, broken in {
                "no hash": {k: v for k, v in good.items() if k != "cli_sha256"},
                "other hash": good | {"cli_sha256": "0" * 64},
                "no env": {k: v for k, v in good.items() if k != "agent_env"},
                "other env": good | {"agent_env": {"DISABLE_AUTOUPDATER": "1"}}}.items():
            with self.subTest(broken=name), self.assertRaises(SystemExit):
                self.load(broken)


class Ag02PreflightTests(unittest.TestCase):
    def test_the_preflight_records_the_pin_and_the_environment_and_refuses_an_unpinned_claude(self):
        from test_preflight import PreflightBase

        class Run(PreflightBase):
            def runTest(self):
                pass
        run = Run()
        run.setUp()
        run.addCleanup(run.doCleanups)
        run.pin = False  # the harness would otherwise answer for the pin itself
        with pinned():
            proof = run.run_preflight()
        self.assertEqual(proof["cli_sha256"], PIN_SHA)
        self.assertEqual(proof["agent_env"], dict(rg.QUIET_ENV))
        self.assertTrue(proof["passed"], proof["problems"])
        launched = mock.Mock(side_effect=AssertionError("a session was launched with an unpinned claude"))
        with pinned(None, ["claude_not_an_absolute_path"]), mock.patch.object(rg, "run_claude", launched):
            with self.assertRaises(preflight.PreflightRefused) as refused:
                run.run_preflight()
        self.assertIn("claude_not_an_absolute_path", refused.exception.reasons)

    def test_a_route_preflight_passes_only_because_the_tool_search_switch_removes_the_placeholder(self):
        from test_prreview93 import RoutePreflightTests

        class Run(RoutePreflightTests):
            def runTest(self):
                pass
        run = Run()
        run.setUp()
        run.addCleanup(run.doCleanups)
        with pinned():
            proof = run.run_route(mode="deferred_placeholder")
        self.assertTrue(proof["passed"], proof["problems"])
        from test_prreview93 import THREE
        self.assertEqual(proof["request_tools"], THREE)


class Ag02LaunchTests(unittest.TestCase):
    def test_a_goal_is_not_launched_when_the_cli_is_not_the_pin(self):
        from test_run_goal import Harness
        test = self
        harness = Harness(test, "G01", 1, final_state={})
        harness.patch_pin = False  # this test is about the real check
        launched = []
        real = harness.deps.run_claude
        harness.deps.run_claude = staticmethod(lambda *a, **k: launched.append(1) or real(*a, **k))
        with launch_refused("claude_hash_differs"):
            result = harness.run()
        self.assertEqual(launched, [], "a session was launched with a CLI that is not the pin")
        self.assertIn("isolation proof does not cover this run", result.get("aborted", ""))
        self.assertIn("claude_hash_differs", result["aborted"])


if __name__ == "__main__":
    unittest.main()
