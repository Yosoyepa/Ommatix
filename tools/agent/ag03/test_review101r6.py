"""Regressions for the sixth and seventh Codex reviews of PR 101 (J1; docs/reviews/2026-10-20-pr101-ag03-harness.md).

The signal mask (``_interrupts_ignored``) of the sixth round could itself leak ignored handlers. The architect decided to
remove it rather than patch it again, so these tests pin the behaviour that needs no mask:

1. No signal is ignored or remapped by the cleanup of an owned session (nothing to leak).
2. An interruption anywhere inside the cleanup propagates, the session stays counted as unverified and the campaign
   lock stays in place - a second run starts no session (fail closed; the operator clears the lock by hand).
3. The owned CLI tree lives in a job object created with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, so the kernel ends the tree
   if the harness process dies or is interrupted for good (asserted with a fake kernel32: no job is created).

A tiny Python child under the owned-process-tree runner and fakes only: no screen, window, input or provider."""

import contextlib
import ctypes
import io
import os
import re
import signal
import subprocess
import sys
import tempfile
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
from test_review101r5 import child, end_child  # noqa: E402

NAMES = [n for n in ("SIGINT", "SIGTERM", "SIGBREAK") if hasattr(signal, n)]


def raising_handler(*_):
    raise KeyboardInterrupt("a real second signal")


@contextlib.contextmanager
def interrupting_handlers():
    """SIGINT and SIGTERM raise like the campaign's own handlers do; everything is restored afterwards."""
    original = {getattr(signal, n): signal.getsignal(getattr(signal, n)) for n in NAMES}
    try:
        for n in NAMES:
            signal.signal(getattr(signal, n), raising_handler)
        yield {number: signal.getsignal(number) for number in original}
    finally:
        sys.settrace(None)
        for number, handler in original.items():
            signal.signal(number, handler)


def after_cleanup_started(fire_at, sig, state):
    """A trace hook: from the call of ``close_gate`` (the first cleanup step) on, delivers a real signal ``sig`` at the
    ``fire_at``-th line executed in run_goal.py; ``fire_at`` None only counts the lines."""
    def trace(frame, event, arg):
        if frame.f_code.co_filename != rg.__file__:
            return None
        if event == "call" and frame.f_code.co_name == "close_gate":
            state["started"] = True
        if event == "line" and state.get("started"):
            state["lines"] = state.get("lines", 0) + 1
            if state["lines"] == fire_at:
                state["fired_in"] = frame.f_code.co_name
                signal.raise_signal(sig)
        return trace
    return trace


class J1NoMaskTests(unittest.TestCase):
    def test_no_signal_mask_is_left_in_the_harness(self):
        self.assertFalse(hasattr(rg, "_interrupts_ignored"))
        for module in (rg, pr):
            source = Path(module.__file__).read_text(encoding="utf-8")
            self.assertIsNone(re.search(r"SIG_IGN", source), f"{Path(module.__file__).name} ignores a signal")

    def test_an_interruption_anywhere_in_the_cleanup_leaves_nothing_unverified_that_is_alive(self):
        with interrupting_handlers() as before, tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            real_wait = subprocess.Popen.wait

            def session(fire_at):
                state, owned, outcome = {}, {"unverified": 0}, None
                with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real_wait)):
                    try:
                        sys.settrace(after_cleanup_started(fire_at, signal.SIGTERM, state))
                        try:
                            rg.run_claude(child(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0,
                                          ownership=owned)
                        finally:
                            sys.settrace(None)
                    except (KeyboardInterrupt, rg.OwnedSessionNotQuiescent) as error:
                        outcome = error
                return state, owned, outcome

            (tmp / "exit.flag").unlink(missing_ok=True)
            counted, owned, outcome = session(None)
            self.assertIsInstance(outcome, KeyboardInterrupt)
            self.assertEqual(owned["unverified"], 0)
            self.assertGreater(counted["lines"], 20, "the cleanup was not traced")
            end_child(tmp)
            stride = max(1, counted["lines"] // 40)
            for line in range(1, counted["lines"] + 1, stride):
                with self.subTest(line=line):
                    (tmp / "exit.flag").unlink(missing_ok=True)
                    try:
                        state, owned, outcome = session(line)
                        self.assertIsInstance(outcome, (KeyboardInterrupt, rg.OwnedSessionNotQuiescent))
                        if owned["unverified"] == 0:  # the only thing that lets a caller release its lock
                            self.assertFalse(beating(tmp), "the session was declared verified with a live CLI")
                        self.assertIn(owned["unverified"], (0, 1))
                    finally:
                        end_child(tmp)
            self.assertEqual({n: signal.getsignal(n) for n in before}, before)

    def test_an_interrupted_cleanup_keeps_the_lock_and_a_second_main_starts_nothing(self):
        s = Series(self)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            state = {}

            class Deps(s.deps):
                @staticmethod
                def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                    real = subprocess.Popen.wait
                    with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                        sys.settrace(after_cleanup_started(2, signal.SIGTERM, state))  # before the tree is ended
                        try:
                            return rg.run_claude(child(tmp), "", tmp, metrics, 60.0, ownership=k.get("ownership"))
                        finally:
                            sys.settrace(None)
            (s.root / "folders").mkdir(exist_ok=True)
            stderr = io.StringIO()
            handlers = {getattr(signal, n): signal.getsignal(getattr(signal, n)) for n in NAMES}
            try:
                with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                        contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
                    with self.assertRaises(KeyboardInterrupt):
                        pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
                self.assertIn("fired_in", state, "the signal was never delivered inside the cleanup")
                self.assertTrue((s.root / "campaign" / "campaign.lock").exists(), "the lock was released")
                self.assertIn("by hand", stderr.getvalue())
                self.assertEqual({n: signal.getsignal(n) for n in handlers}, handlers)
                second = Series(self, root=s.root)
                with self.assertRaises(SystemExit):
                    second.run(extra=["--out", str(s.root / "out2")])
                self.assertEqual((second.calls, second.app_calls), ([], {}))
            finally:
                sys.settrace(None)
                end_child(tmp)


@unittest.skipUnless(os.name == "nt", "the job object is a Windows mechanism")
class OwnedTreeJobFlagTests(unittest.TestCase):
    """The tree is killed by the kernel when the harness dies: checked on a fake kernel32, so no job is created."""

    def fake_kernel32(self, set_information_result=1):
        calls = {"set": [], "closed": []}

        def set_information(job, info_class, info_ref, size):
            info = info_ref._obj  # the ctypes structure behind byref()
            calls["set"].append((job, info_class, info.Basic.LimitFlags, size))
            return set_information_result

        fake = mock.MagicMock()  # attributes such as ``argtypes`` can be set on its members
        fake.CreateJobObjectW.return_value = 4242
        fake.CloseHandle.side_effect = lambda handle: calls["closed"].append(handle)
        fake.SetInformationJobObject.side_effect = set_information
        return fake, calls

    def open_job(self, fake):
        tree = object.__new__(rg.OwnedTree)
        tree.job = None
        with mock.patch.object(ctypes, "WinDLL", return_value=fake):
            tree._open_job()  # noqa: SLF001
        return tree

    def test_the_job_is_created_with_kill_on_job_close(self):
        self.assertEqual(rg.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, 0x2000)
        fake, calls = self.fake_kernel32()
        tree = self.open_job(fake)
        self.assertEqual(tree.job, 4242)
        self.assertEqual(len(calls["set"]), 1)
        job, info_class, flags, size = calls["set"][0]
        self.assertEqual((job, info_class), (4242, 9))  # JobObjectExtendedLimitInformation
        self.assertTrue(flags & rg.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, "the job would outlive the harness")
        self.assertGreater(size, 0)

    def test_a_job_whose_limit_could_not_be_set_is_closed_and_not_used(self):
        fake, calls = self.fake_kernel32(set_information_result=0)
        tree = self.open_job(fake)
        self.assertIsNone(tree.job)
        self.assertEqual(calls["closed"], [4242])


if __name__ == "__main__":
    unittest.main()
