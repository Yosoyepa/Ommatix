"""Regressions for the sixth Codex review of PR 101 at 70ad4fc (J1; docs/reviews/2026-10-20-pr101-ag03-harness.md).

Real Python signals (``signal.raise_signal``) are delivered from a trace hook at every line the mask helper executes,
so an interruption lands exactly at each transition of installing and restoring the mask. A tiny Python child under the
owned-process-tree runner and fakes only: no screen, window, input or provider.

The design these tests pin: establishing and lifting the signal mask is part of the guarded cleanup. An interruption
while the mask is being installed does not skip the body (the session cleanup still runs); every handler the helper
changed is restored before a saved interruption propagates, an interruption during the restore is retried, and a
handler that could not be restored is an error, never a silent success."""

import contextlib
import inspect
import io
import signal
import subprocess
import sys
import tempfile
import threading
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


def helper_frame(frame):
    return frame.f_code.co_filename == rg.__file__ and frame.f_code.co_name in (
        "_interrupts_ignored", "attempt", "<lambda>")


def first_guarded_line():
    """The last line of the helper at which a signal may still skip the body: the call of the first guarded step (the
    same as a signal arriving just before the ``with`` statement, which ``run_claude`` repeats the cleanup for)."""
    source, first = inspect.getsourcelines(rg._interrupts_ignored.__wrapped__)  # noqa: SLF001
    return first + next(i for i, text in enumerate(source) if "attempt(setup)" in text)  # the first guarded step


def run_once(fire_at, sig):
    """Runs the helper with a body; delivers ``sig`` at the ``fire_at``-th traced line (None: only counts lines)."""
    seen = {"lines": 0, "ran": False, "raised": None, "where": None}

    def trace(frame, event, arg):
        if event == "call" and not helper_frame(frame):
            return None
        if event == "line" and helper_frame(frame):
            seen["lines"] += 1
            if seen["lines"] == fire_at:
                seen["where"] = frame.f_lineno
                signal.raise_signal(sig)
        return trace

    try:
        sys.settrace(trace)
        try:
            with rg._interrupts_ignored():  # noqa: SLF001
                seen["ran"] = True
        finally:
            sys.settrace(None)
    except KeyboardInterrupt as error:
        seen["raised"] = error
    return seen


# ---- J1: the mask transitions are inside the guarded cleanup ----------------------------------------------

class J1MaskTransitionTests(unittest.TestCase):
    def test_a_real_signal_at_every_line_of_the_mask_helper_restores_every_handler_and_runs_the_body(self):
        with interrupting_handlers() as before:
            counted = run_once(None, signal.SIGTERM)
            self.assertGreater(counted["lines"], 10, "the helper was not traced")
            self.assertTrue(counted["ran"] and counted["raised"] is None)
            self.assertEqual({n: signal.getsignal(n) for n in before}, before)
            for sig in (signal.SIGINT, signal.SIGTERM):
                for line in range(1, counted["lines"] + 1):
                    with self.subTest(signal=sig.name, line=line):
                        seen = run_once(line, sig)
                        if not seen["ran"]:  # only a signal before the first guarded step may skip the body
                            self.assertLessEqual(seen["where"], first_guarded_line(), "the body was skipped inside the mask")
                            self.assertIsInstance(seen["raised"], KeyboardInterrupt)
                        self.assertEqual({n: signal.getsignal(n) for n in before}, before,
                                         "a handler was left changed")

    def test_an_interruption_during_the_install_is_raised_after_the_body_and_the_restore(self):
        with interrupting_handlers() as before:
            order = []
            real = signal.signal
            fired = []

            def interrupt_once(number, handler):
                # the SIGTERM mask is the second change: a second termination arrives just before it
                if handler is signal.SIG_IGN and number == signal.SIGTERM and not fired:
                    fired.append(number)
                    raise KeyboardInterrupt("arrived before the SIGTERM mask")
                return real(number, handler)
            with mock.patch.object(signal, "signal", interrupt_once):
                with self.assertRaises(KeyboardInterrupt):
                    with rg._interrupts_ignored():  # noqa: SLF001
                        order.append("body")
            self.assertEqual(order, ["body"])
            self.assertEqual({n: signal.getsignal(n) for n in before}, before)

    def test_a_handler_that_cannot_be_restored_is_an_error_not_a_silent_success(self):
        with interrupting_handlers() as before:
            real = signal.signal

            def fail_restore(number, handler):
                if handler is not signal.SIG_IGN and number == signal.SIGTERM:
                    raise KeyboardInterrupt("keeps interrupting")
                return real(number, handler)
            try:
                with mock.patch.object(signal, "signal", fail_restore):
                    with self.assertRaises(RuntimeError):
                        with rg._interrupts_ignored():  # noqa: SLF001
                            pass
            finally:
                signal.signal(signal.SIGTERM, before[signal.SIGTERM])

    def test_run_claude_still_ends_and_verifies_the_tree_when_the_mask_install_is_interrupted(self):
        owned = {"unverified": 0}
        with interrupting_handlers() as before, tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            fired = []

            def trace(frame, event, arg):
                if event == "call" and not helper_frame(frame):
                    return None
                if (event == "line" and helper_frame(frame) and not fired
                        and signal.getsignal(signal.SIGINT) is signal.SIG_IGN
                        and signal.getsignal(signal.SIGTERM) is not signal.SIG_IGN):
                    fired.append(1)
                    signal.raise_signal(signal.SIGTERM)  # between the SIGINT mask and the SIGTERM mask
                return trace
            real = subprocess.Popen.wait
            try:
                with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                    sys.settrace(trace)
                    try:
                        with self.assertRaises(KeyboardInterrupt):
                            rg.run_claude(child(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0,
                                          ownership=owned)
                    finally:
                        sys.settrace(None)
                alive = beating(tmp)
            finally:
                end_child(tmp)
            self.assertTrue(fired, "the signal was never delivered at the transition")
            self.assertFalse(alive, "the owned CLI outlived a failed mask install")
            self.assertEqual(owned["unverified"], 0)
            self.assertEqual({n: signal.getsignal(n) for n in before}, before, "a handler was left masked")

    def test_an_interruption_at_the_mask_after_a_normal_session_propagates_once_the_session_is_verified(self):
        owned = {"unverified": 0}
        with interrupting_handlers() as before, tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "short.py").write_text("print('{}', flush=True)\n", encoding="utf-8")
            fired = []

            def trace(frame, event, arg):
                if event == "call" and not helper_frame(frame):
                    return None
                if (event == "line" and helper_frame(frame) and not fired
                        and signal.getsignal(signal.SIGINT) is signal.SIG_IGN
                        and signal.getsignal(signal.SIGTERM) is not signal.SIG_IGN):
                    fired.append(1)
                    signal.raise_signal(signal.SIGTERM)
                return trace
            try:
                sys.settrace(trace)
                with self.assertRaises(KeyboardInterrupt):
                    rg.run_claude([sys.executable, str(tmp / "short.py")], "", tmp,
                                  rg.StreamMetrics([], None, None), 60.0, ownership=owned)
            finally:
                sys.settrace(None)
            self.assertTrue(fired)
            self.assertEqual(owned["unverified"], 0)
            self.assertEqual({n: signal.getsignal(n) for n in before}, before)

    def test_a_real_signal_at_every_mask_line_never_leaves_the_session_unverified_or_a_handler_masked(self):
        with interrupting_handlers() as before, tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "short.py").write_text("print('{}', flush=True)\n", encoding="utf-8")

            def session(fire_at):
                seen, owned = {"lines": 0}, {"unverified": 0}

                def trace(frame, event, arg):
                    if event == "call" and not helper_frame(frame):
                        return None
                    if event == "line" and helper_frame(frame):
                        seen["lines"] += 1
                        if seen["lines"] == fire_at:
                            signal.raise_signal(signal.SIGTERM)
                    return trace
                outcome = None
                try:
                    sys.settrace(trace)
                    try:
                        rg.run_claude([sys.executable, str(tmp / "short.py")], "", tmp,
                                      rg.StreamMetrics([], None, None), 60.0, ownership=owned)
                    finally:
                        sys.settrace(None)
                except KeyboardInterrupt as error:
                    outcome = error
                return seen["lines"], owned, outcome
            total, owned, outcome = session(None)
            self.assertGreater(total, 10)
            self.assertEqual((owned["unverified"], outcome), (0, None))
            for line in range(1, total + 1):
                with self.subTest(line=line):
                    _, owned, outcome = session(line)
                    self.assertEqual(owned["unverified"], 0, "a session was left unverified")
                    self.assertTrue(outcome is None or isinstance(outcome, KeyboardInterrupt))
                    self.assertEqual({n: signal.getsignal(n) for n in before}, before, "a handler was left masked")

    def test_a_mask_failure_with_an_unverifiable_tree_keeps_the_lock_and_a_second_main_starts_nothing(self):
        s = Series(self)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            before = {getattr(signal, n): signal.getsignal(getattr(signal, n)) for n in NAMES}
            fired = []

            def trace(frame, event, arg):
                if event == "call" and not helper_frame(frame):
                    return None
                if (event == "line" and helper_frame(frame) and not fired
                        and signal.getsignal(signal.SIGINT) is signal.SIG_IGN
                        and signal.getsignal(signal.SIGTERM) is not signal.SIG_IGN):
                    fired.append(1)
                    signal.raise_signal(signal.SIGTERM)
                return trace

            class Deps(s.deps):
                @staticmethod
                def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                    real = subprocess.Popen.wait
                    with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)), \
                            mock.patch.object(rg.OwnedTree, "_terminate", side_effect=OSError("cannot")), \
                            mock.patch.object(rg.OwnedTree, "shutdown", side_effect=RuntimeError("cannot")):
                        sys.settrace(trace)
                        try:
                            return rg.run_claude(child(tmp), "", tmp, metrics, 60.0, ownership=k.get("ownership"))
                        finally:
                            sys.settrace(None)
            (s.root / "folders").mkdir(exist_ok=True)
            stderr = io.StringIO()
            try:
                with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                        contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
                    with self.assertRaises(rg.OwnedSessionNotQuiescent):
                        pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
                self.assertTrue(fired)
                self.assertTrue((s.root / "campaign" / "campaign.lock").exists())
                self.assertIn("by hand", stderr.getvalue())
                self.assertNotEqual(signal.getsignal(signal.SIGINT), signal.SIG_IGN, "Ctrl+C is still ignored")
                self.assertEqual({n: signal.getsignal(n) for n in before}, before)
                second = Series(self, root=s.root)
                with self.assertRaises(SystemExit):
                    second.run(extra=["--out", str(s.root / "out2")])
                self.assertEqual((second.calls, second.app_calls), ([], {}))
            finally:
                sys.settrace(None)
                end_child(tmp)


if __name__ == "__main__":
    unittest.main()
