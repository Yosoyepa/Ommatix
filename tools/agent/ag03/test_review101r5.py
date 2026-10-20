"""Regressions for the fifth Codex review of PR 101 at 380a756 (I1; docs/reviews/2026-10-20-pr101-ag03-harness.md).

Written and run RED against 380a756 before the fix (output kept). A tiny Python child under the owned-process-tree
runner and fakes only: no screen, window, input or provider.

The design these tests pin: the cleanup after an owned session is fail closed from the first statement to the last.
Every step of it is attempted whatever an earlier step did (a failed fast termination must not skip the fallback
shutdown and its verification); further SIGINT/SIGTERM/SIGBREAK are ignored while it runs; the campaign lock is
released ONLY on positive proof - the tree was killed and verified gone and the monitor was shown finished - and every
release path checks that proof, so any exception, second Ctrl+C or signal during cleanup leaves the lock in place and
the operator is told to check and clear it by hand."""

import contextlib
import io
import signal
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_runner as pr  # noqa: E402
import run_goal as rg  # noqa: E402
from test_campaign import Series  # noqa: E402
from test_review101r4 import beating, interrupting_wait  # noqa: E402

LIVELY = textwrap.dedent('''
    import pathlib, sys, time
    beat = pathlib.Path(sys.argv[1])
    stop = beat.with_name("exit.flag")      # the tests' way to end a child that the runner failed to end
    print("{}", flush=True)
    while not stop.exists():
        beat.write_text(str(time.monotonic()))
        time.sleep(0.1)
''')

MARKER = textwrap.dedent('''
    import pathlib, sys, time
    print("{}", flush=True)
    time.sleep(2.0)
    pathlib.Path(sys.argv[1]).write_text("still running after the interruption")
    time.sleep(60)
''')


def child(tmp: Path):
    (tmp / "lively.py").write_text(LIVELY, encoding="utf-8")
    return [sys.executable, str(tmp / "lively.py"), str(tmp / "beat.txt")]


def end_child(tmp: Path):
    """Ends a child the runner (deliberately broken in a test) did not end."""
    (tmp / "exit.flag").write_text("x", encoding="utf-8")
    time.sleep(0.5)


def first_call_raises(error):
    real = rg.OwnedTree._terminate  # noqa: SLF001
    state = {"n": 0}

    def terminate(self):
        state["n"] += 1
        if state["n"] == 1:
            raise error
        return real(self)
    return terminate


# ---- I1: a failure inside the cleanup never hands the campaign over --------------------------------------

class I1CleanupFailClosedTests(unittest.TestCase):
    def interrupted(self, tmp, **kw):
        real = subprocess.Popen.wait
        with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
            return rg.run_claude(child(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0, **kw)

    def test_a_failed_fast_termination_does_not_skip_the_fallback_shutdown_and_its_verification(self):
        for error in (OSError("simulated termination failure"), KeyboardInterrupt()):
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                with mock.patch.object(rg.OwnedTree, "_terminate", first_call_raises(error)):
                    with self.assertRaises((KeyboardInterrupt, rg.OwnedSessionNotQuiescent)):
                        self.interrupted(tmp)
                alive = beating(tmp)
                end_child(tmp)
                self.assertFalse(alive, f"the owned CLI outlived a failed fast termination ({error!r})")

    def test_when_nothing_can_end_the_tree_the_failure_is_typed_and_keeps_its_causes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            with mock.patch.object(rg.OwnedTree, "_terminate", side_effect=OSError("cannot terminate")), \
                    mock.patch.object(rg.OwnedTree, "shutdown", side_effect=RuntimeError("cannot verify")):
                with self.assertRaises(rg.OwnedSessionNotQuiescent) as raised:
                    self.interrupted(tmp)
            end_child(tmp)
        self.assertIsInstance(raised.exception.__cause__, KeyboardInterrupt)

    def test_a_cleanup_that_cannot_verify_keeps_the_lock_and_a_second_main_starts_nothing(self):
        s = Series(self)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)

            class Deps(s.deps):
                @staticmethod
                def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                    real = subprocess.Popen.wait
                    with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)), \
                            mock.patch.object(rg.OwnedTree, "_terminate", side_effect=OSError("cannot")), \
                            mock.patch.object(rg.OwnedTree, "shutdown", side_effect=RuntimeError("cannot")):
                        return rg.run_claude(child(tmp), "", tmp, metrics, 60.0, ownership=k.get("ownership"))
            (s.root / "folders").mkdir(exist_ok=True)
            stderr = io.StringIO()
            try:
                with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                        contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
                    with self.assertRaises(rg.OwnedSessionNotQuiescent):
                        pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
                self.assertTrue((s.root / "campaign" / "campaign.lock").exists(), "the lock was released with a live CLI")
                self.assertIn("by hand", stderr.getvalue())
                second = Series(self, root=s.root)
                with self.assertRaises(SystemExit):
                    second.run(extra=["--out", str(s.root / "out2")])
                self.assertEqual((second.calls, second.app_calls), ([], {}))
            finally:
                end_child(tmp)

    def test_an_exception_while_the_monitor_is_being_closed_or_joined_is_not_a_normal_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            real_join = threading.Thread.join

            def join(thread, *a, **k):
                if getattr(thread, "_target", None) is not None and thread._target.__name__ == "watch":  # noqa: SLF001
                    raise OSError("join failed")
                return real_join(thread, *a, **k)
            with mock.patch.object(threading.Thread, "join", join):
                with self.assertRaises(rg.OwnedSessionNotQuiescent):
                    self.interrupted(tmp, should_stop=lambda: None, poll_s=0.05)
            alive = beating(tmp)
            end_child(tmp)
            self.assertFalse(alive, "the tree was left alive although the monitor join failed")

    def test_further_interrupt_signals_are_ignored_while_the_cleanup_runs_and_restored_after(self):
        before = {n: signal.getsignal(n) for n in (signal.SIGINT, signal.SIGTERM)}
        seen = {}
        real_shutdown = rg.OwnedTree.shutdown

        def shutdown(self, *a, **k):
            seen.update({n: signal.getsignal(n) for n in before})
            return real_shutdown(self, *a, **k)
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(rg.OwnedTree, "shutdown", shutdown):
            with self.assertRaises(KeyboardInterrupt):
                self.interrupted(Path(tmp))
        self.assertEqual(seen, {signal.SIGINT: signal.SIG_IGN, signal.SIGTERM: signal.SIG_IGN})
        self.assertEqual({n: signal.getsignal(n) for n in before}, before)

    def test_the_lock_is_released_only_on_positive_proof_and_an_unproven_session_keeps_it(self):
        for proven, expected_lock in ((True, False), (False, True)):
            s = Series(self)

            class Deps(s.deps):
                @staticmethod
                def run_claude(*a, **k):
                    owned = k["ownership"]
                    owned["unverified"] += 1          # a session was opened ...
                    if proven:
                        owned["unverified"] -= 1      # ... and positively shown gone
                    raise RuntimeError("an unexpected failure escaped")
            (s.root / "folders").mkdir(exist_ok=True)
            stderr = io.StringIO()
            with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                    contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
                with self.assertRaises(RuntimeError):
                    pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
            self.assertEqual((s.root / "campaign" / "campaign.lock").exists(), expected_lock, proven)
            if expected_lock:
                self.assertIn("by hand", stderr.getvalue())

    def test_a_successful_session_leaves_nothing_unverified(self):
        owned = {"unverified": 0}
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "short.py").write_text("print('{}', flush=True)\n", encoding="utf-8")
            rg.run_claude([sys.executable, str(tmp / "short.py")], "", tmp, rg.StreamMetrics([], None, None), 60.0,
                          ownership=owned)
        self.assertEqual(owned["unverified"], 0)

    def test_an_interrupted_session_that_was_ended_and_verified_also_leaves_nothing_unverified(self):
        owned = {"unverified": 0}
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(KeyboardInterrupt):
                self.interrupted(Path(tmp), ownership=owned)
        self.assertEqual(owned["unverified"], 0)

    def test_a_session_whose_tree_is_reported_left_stays_unverified(self):
        owned = {"unverified": 0}
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "short.py").write_text("print('{}', flush=True)\n", encoding="utf-8")
            with mock.patch.object(rg.OwnedTree, "shutdown", return_value=1):
                result = rg.run_claude([sys.executable, str(tmp / "short.py")], "", tmp,
                                       rg.StreamMetrics([], None, None), 60.0, ownership=owned)
        self.assertEqual(result["tree_left"], 1)
        self.assertEqual(owned["unverified"], 1, "a session with a tree left was counted as verified gone")

    def test_an_output_reader_that_cannot_be_shown_finished_is_not_a_normal_exit(self):
        owned = {"unverified": 0}
        real_alive, real_start = threading.Thread.is_alive, threading.Thread.start
        pumps = []

        def start(thread):
            if getattr(thread, "_target", None) is not None and thread._target.__name__ == "pump":  # noqa: SLF001
                pumps.append(thread)  # a finished thread forgets its target, so remember it now
            return real_start(thread)

        def alive(thread):
            return True if thread in pumps else real_alive(thread)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "short.py").write_text("print('{}', flush=True)\n", encoding="utf-8")
            with mock.patch.object(threading.Thread, "start", start), \
                    mock.patch.object(threading.Thread, "is_alive", alive), \
                    mock.patch.object(threading.Thread, "join", lambda *a, **k: None):
                with self.assertRaises(rg.OwnedSessionNotQuiescent):
                    rg.run_claude([sys.executable, str(tmp / "short.py")], "", tmp,
                                  rg.StreamMetrics([], None, None), 60.0, ownership=owned)
        self.assertEqual(owned["unverified"], 1)

    def test_interruption_ends_the_tree_promptly_and_no_delayed_marker_is_written(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "marker.py").write_text(MARKER, encoding="utf-8")
            real = subprocess.Popen.wait
            started = time.monotonic()
            with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                with self.assertRaises(KeyboardInterrupt):
                    rg.run_claude([sys.executable, str(tmp / "marker.py"), str(tmp / "marker.txt")], "", tmp,
                                  rg.StreamMetrics([], None, None), 60.0)
            self.assertLess(time.monotonic() - started, 5.0, "the tree took the ordinary fallback wait to end")
            time.sleep(2.5)
            self.assertFalse((tmp / "marker.txt").exists())


if __name__ == "__main__":
    unittest.main()
