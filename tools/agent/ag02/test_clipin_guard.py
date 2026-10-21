"""Review of PR 107 at 2bfbd9c (R107-1, R107-2, R107-3; docs/reviews/2026-10-20-agent-cli-pin.md).

R107-1 nothing is executed before the scope and hash checks pass: a rejected executable never reaches ``--version``.
R107-2 the verified bytes are the bytes that run: the harness verifies a PRIVATE COPY of the pinned binary (made in a directory it
       creates, read-only, held against writes where the platform allows it), asks the version of that copy and launches that
       copy; replacing the configured file at any moment changes nothing that runs. Every launch (each preflight session, each
       probe session, each goal, the smoke) is verified on its own.
R107-3 the native smoke needs the pin too, whether or not it needs an isolation proof.

Synthetic bytes and a temporary pin only; the version query is a spy: nothing is executed."""

import argparse
import contextlib
import hashlib
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import clipin  # noqa: E402
import run_goal as rg  # noqa: E402

BYTES = b"MZ synthetic claude " * 64
SHA = hashlib.sha256(BYTES).hexdigest()
VERSION = "9.9.9"


def temp_pin():
    return mock.patch.object(clipin, "CLAUDE_CODE_PIN", {"version": VERSION, "sha256": SHA})


class Base(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag-guard-")
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.exe = self.root / "agent-cli" / "claude.exe"
        self.exe.parent.mkdir()
        self.exe.write_bytes(BYTES)
        self.enterContext(temp_pin())
        self.versions = []  # the paths the version query was asked about

    def version(self, path):
        self.versions.append(str(path))
        return f"{VERSION} (Claude Code)"

    def sha_of(self, path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class NothingRunsBeforeTheChecksPassTests(Base):
    """R107-1."""

    def check(self, claude, **kw):
        return clipin.check(claude, version_runner=self.version, **kw)

    def test_a_wrong_hash_never_reaches_the_version_query(self):
        self.exe.write_bytes(BYTES + b"tampered")
        problems, _sha = self.check(str(self.exe))
        self.assertEqual(problems, ["claude_hash_differs"])
        self.assertEqual(self.versions, [], "a binary with another hash was executed")

    def test_a_binary_inside_the_repository_never_reaches_the_version_query(self):
        problems, _sha = self.check(str(self.exe), repo_root=self.root)
        self.assertIn("claude_inside_repository", problems)
        self.assertEqual(self.versions, [], "a binary inside the repository was executed")

    def test_a_path_a_missing_file_and_an_unreadable_file_never_reach_it(self):
        def unreadable(path):
            raise PermissionError("denied")
        for claude, kw in (("claude", {}), (str(self.root / "absent.exe"), {}), ([sys.executable, "x"], {}),
                           (str(self.exe), {"hasher": unreadable})):
            with self.subTest(claude=claude):
                self.assertTrue(self.check(claude, **kw)[0])
        self.assertEqual(self.versions, [])

    def test_the_pinned_binary_reaches_the_version_query_once_and_not_through_the_configured_path(self):
        problems, sha = self.check(str(self.exe))
        self.assertEqual((problems, sha), ([], SHA))
        self.assertEqual(len(self.versions), 1)
        self.assertNotEqual(os.path.normcase(self.versions[0]), os.path.normcase(str(self.exe)),
                            "the version was asked of the configured file, not of the verified copy")


class VerifiedBytesAreTheBytesThatRunTests(Base):
    """R107-2."""

    def test_the_guard_yields_a_private_copy_with_the_pinned_bytes(self):
        with clipin.verified(str(self.exe), version_runner=self.version) as v:
            self.assertEqual(v.problems, [])
            self.assertEqual(v.sha, SHA)
            self.assertNotEqual(os.path.normcase(v.path), os.path.normcase(str(self.exe)))
            self.assertEqual(self.sha_of(v.path), SHA)
            copy_dir = Path(v.path).parent
            self.assertTrue(copy_dir.is_dir())
            self.assertFalse(str(copy_dir).startswith(str(self.exe.parent)), "the copy lives beside the original")
        self.assertFalse(copy_dir.exists(), "the private copy was left behind")

    def test_replacing_the_configured_file_while_the_version_is_asked_changes_nothing_that_runs(self):
        def replacing(path):
            self.exe.write_bytes(b"MZ something else entirely")
            return f"{VERSION} (Claude Code)"
        with clipin.verified(str(self.exe), version_runner=replacing) as v:
            self.assertEqual(v.problems, [])
            self.assertNotEqual(self.sha_of(self.exe), SHA, "the test did not replace the configured file")
            self.assertEqual(self.sha_of(v.path), SHA, "what runs is no longer the verified bytes")

    def test_replacing_the_configured_file_after_the_guard_was_entered_changes_nothing_that_runs(self):
        with clipin.verified(str(self.exe), version_runner=self.version) as v:
            self.exe.write_bytes(b"MZ something else entirely")
            self.assertEqual(self.sha_of(v.path), SHA)

    def test_a_copy_that_does_not_match_the_original_is_refused_and_never_run(self):
        def bad_copier(source, target):
            Path(target).write_bytes(b"MZ not the same")
        with clipin.verified(str(self.exe), version_runner=self.version, copier=bad_copier) as v:
            self.assertEqual(v.problems, ["claude_copy_differs"])
            self.assertIsNone(v.path)
        self.assertEqual(self.versions, [], "an unverified copy was executed")

    def test_a_refusal_leaves_no_copy_and_names_no_path(self):
        before = set(Path(tempfile.gettempdir()).glob("ommatix-cli-*"))
        with clipin.verified("claude", version_runner=self.version) as v:
            self.assertEqual(v.problems, ["claude_not_an_absolute_path"])
            self.assertIsNone(v.path)
        self.assertEqual(set(Path(tempfile.gettempdir()).glob("ommatix-cli-*")), before)

    def test_the_proof_must_name_these_bytes_and_be_fresh(self):
        proof = {"cli_sha256": SHA, "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        with clipin.verified(str(self.exe), proof, version_runner=self.version) as v:
            self.assertEqual(v.problems, [])
        with clipin.verified(str(self.exe), proof | {"cli_sha256": "1" * 64}, version_runner=self.version) as v:
            self.assertEqual(v.problems, ["cli_differs_from_proof"])
            self.assertIsNone(v.path)
        with clipin.verified(str(self.exe), proof | {"date_utc": "2020-01-01T00:00:00Z"},
                             version_runner=self.version) as v:
            self.assertEqual(v.problems, ["proof_expired"])

    @unittest.skipUnless(os.name == "nt", "the deny-write hold uses the Win32 sharing mode")
    def test_on_windows_the_copy_cannot_be_written_renamed_or_deleted_while_the_guard_is_held(self):
        with clipin.verified(str(self.exe), version_runner=self.version) as v:
            for name, act in (("write", lambda: open(v.path, "ab").close()), ("delete", lambda: os.remove(v.path)),
                              ("rename", lambda: os.rename(v.path, v.path + ".x"))):
                with self.subTest(act=name), self.assertRaises(OSError):
                    act()


class EachLaunchIsVerifiedOnItsOwnTests(Base):
    """R107-2 for the preflights, the probe, the goals and R107-3 for the smoke."""

    def test_every_session_of_the_ag02_preflight_runs_inside_its_own_guard(self):
        import preflight
        from test_preflight import PreflightBase
        log, launches, real = [], [], rg.run_claude

        @contextlib.contextmanager
        def guard(claude, proof=None, **kw):
            log.append("enter")
            yield clipin.Verified([], SHA, claude)
            log.append("exit")

        def spy(argv, *a, **k):
            launches.append((list(log), argv[0]))
            return real(argv, *a, **k)

        class Run(PreflightBase):
            pin = False

            def runTest(self):
                pass
        run = Run()
        run.setUp()
        run.addCleanup(run.doCleanups)
        with mock.patch.object(clipin, "verified", guard), mock.patch.object(rg, "run_claude", spy):
            proof = run.run_preflight()
        self.assertEqual(len(launches), 3, "the AG-02 preflight runs three sessions")
        for number, (seen, _argv0) in enumerate(launches, 1):
            self.assertEqual(seen.count("enter") - seen.count("exit"), 1, f"session {number} ran outside a guard")
        self.assertEqual(log.count("enter"), 3)
        self.assertEqual(proof["cli_sha256"], SHA)

    def test_a_session_whose_guard_refuses_is_never_launched(self):
        import preflight
        from test_preflight import PreflightBase

        @contextlib.contextmanager
        def refusing(claude, proof=None, **kw):
            yield clipin.Verified(["claude_hash_differs"], None, None)
        launched = mock.Mock(side_effect=AssertionError("launched with a refused CLI"))

        class Run(PreflightBase):
            pin = False

            def runTest(self):
                pass
        run = Run()
        run.setUp()
        run.addCleanup(run.doCleanups)
        with mock.patch.object(clipin, "verified", refusing), mock.patch.object(rg, "run_claude", launched):
            with self.assertRaises(preflight.PreflightRefused) as refused:
                run.run_preflight()
        self.assertIn("claude_hash_differs", refused.exception.reasons)

    def test_a_goal_launches_the_verified_copy_even_if_the_configured_file_is_replaced_before_the_child(self):
        from test_run_goal import Harness, make_proof
        harness = Harness(self, "G01", 1, final_state={})
        harness.patch_pin = False
        harness.args.claude = str(self.exe)
        harness.proof = make_proof(cli_sha256=SHA, agent_env=dict(rg.QUIET_ENV),
                                   date_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                                   config_identity=rg.config_identity(harness.cfg),
                                   context_manifest=rg.config_manifest(harness.cfg))
        seen = []
        real = harness.deps.run_claude

        def boundary(argv, *a, **k):
            seen.append((argv[0], self.sha_of(argv[0])))
            return real(argv, *a, **k)
        harness.deps.run_claude = staticmethod(boundary)
        import agentcli
        real_env = agentcli.ClaudeCLI.env

        def replacing_env(self_, extra=None):
            self.exe.write_bytes(b"MZ replaced after the launch check")  # between the check and the child
            return real_env(self_, extra)
        with temp_pin(), mock.patch.object(clipin, "run_version", self.version), \
                mock.patch.object(agentcli.ClaudeCLI, "env", replacing_env):
            result = harness.run()
        self.assertEqual(len(seen), 1, result.get("aborted"))
        launched, launched_sha = seen[0]
        self.assertEqual(launched_sha, SHA, "the child was started with bytes that are not the verified ones")
        self.assertNotEqual(os.path.normcase(launched), os.path.normcase(str(self.exe)))
        self.assertNotIn("aborted", result)


class CopyCleanupTests(Base):
    """Re-review of PR 107 (R107-5): the guard's private tree is removed even with read-only companions, and a cleanup that fails is
    reported to the caller (closed token), never ignored."""

    def test_a_read_only_companion_file_does_not_keep_the_copy_directory(self):
        with clipin.verified(str(self.exe), version_runner=self.version) as v:
            companion = Path(v.path).parent / "readonly-state.bin"
            companion.write_bytes(b"state")
            os.chmod(companion, 0o400)  # the read-only attribute on Windows
            copy_dir = Path(v.path).parent
        self.assertFalse(copy_dir.exists(), "a read-only companion kept the private directory")
        self.assertEqual(v.cleanup, [])

    def test_a_cleanup_that_fails_is_reported_and_names_no_path(self):
        import shutil
        with mock.patch.object(clipin.shutil, "rmtree", lambda *a, **k: None):
            with clipin.verified(str(self.exe), version_runner=self.version) as v:
                copy_dir = Path(v.path).parent
        try:
            self.assertEqual(v.cleanup, ["claude_copy_not_removed"])
            self.assertTrue(copy_dir.exists())
        finally:
            os.chmod(copy_dir / Path(v.path).name, 0o700)
            shutil.rmtree(copy_dir, ignore_errors=True)

    def test_a_goal_result_records_the_cleanup_failure_and_keeps_its_outcome(self):
        from pin_support import pinned
        from test_run_goal import Harness
        harness = Harness(self, "G01", 1, final_state={})
        harness.patch_pin = False
        with pinned(cleanup=["claude_copy_not_removed"]):
            result = harness.run()
        self.assertEqual(result.get("cli_copy_cleanup"), ["claude_copy_not_removed"])
        self.assertNotIn("claude_copy_not_removed", result.get("aborted", ""), "a cleanup failure must not rewrite the outcome")


class SmokeNeedsThePinTests(Base):
    """R107-3: the native smoke has no isolation proof and still needs the pinned executable."""

    def run_main(self, claude, extra=()):
        launches = []

        class Deps(rg.RealDeps):
            @staticmethod
            def run_claude(argv, prompt, cwd, metrics, timeout, *a, **k):
                launches.append(argv[0])
                return {"exit_code": 0, "killed": None}
        out = self.root / "out"
        argv = ["--smoke", "--model", "claude-haiku-5-5", "--exe", str(self.root / "ommatix.exe"), "--claude", claude,
                "--out", str(out), *extra]
        error = None
        try:
            with contextlib.redirect_stdout(open(os.devnull, "w")):
                code = rg.main(argv, deps=Deps)
        except SystemExit as exit_:
            error, code = exit_, None
        return launches, code, error

    def test_the_default_claude_from_the_path_is_refused(self):
        launches, _code, error = self.run_main("claude")
        self.assertEqual(launches, [], "the native smoke launched the claude found on PATH")
        self.assertIsNotNone(error)

    def test_a_binary_with_another_hash_is_refused(self):
        self.exe.write_bytes(BYTES + b"x")
        launches, _code, error = self.run_main(str(self.exe))
        self.assertEqual(launches, [])
        self.assertIsNotNone(error)

    def test_the_pinned_absolute_path_launches_the_verified_copy(self):
        with mock.patch.object(clipin, "run_version", self.version):
            launches, code, error = self.run_main(str(self.exe))
        self.assertIsNone(error, error)
        self.assertEqual(len(launches), 1)
        self.assertNotEqual(os.path.normcase(launches[0]), os.path.normcase(str(self.exe)))


if __name__ == "__main__":
    unittest.main()
