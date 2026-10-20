"""Regressions for the fourth Codex review of PR 101 at 4d2ece8 (H1-H3; docs/reviews/2026-10-20-pr101-ag03-harness.md).

Written and run RED against 4d2ece8 before each fix (red output per class kept). Fakes, tiny Python children under
the owned-process-tree runner and Python stand-ins for the DPI DLL functions: no screen, window, input or provider.

The simple designs these tests pin: on EVERY exit path (normal, exception, KeyboardInterrupt, signal) the owned
job/tree is killed first and verified gone, and only then is the campaign lock released - if termination cannot be
verified the lock is KEPT and the operator must check and clear it by hand; the watcher's side effects (stop file,
kill) go through one guarded gate whose closed flag is set under a lock before ``run_claude`` returns, and a
monitor that cannot be shown to have finished is a failed cleanup, never a normal return; the numeric DPI read
accepts only a verified process-level per-monitor v2 context - no thread-level fallback."""

import contextlib
import io
import json
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

import pixel_plan as plan  # noqa: E402
import pixel_runner as pr  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import run_goal as rg  # noqa: E402
from test_campaign import Series  # noqa: E402

HEARTBEAT = textwrap.dedent('''
    import pathlib, sys, time
    beat = pathlib.Path(sys.argv[1])
    print("{}", flush=True)
    while True:
        beat.write_text(str(time.monotonic()))
        time.sleep(0.1)
''')


def spawn_args(tmp: Path, name="child.py"):
    (tmp / name).write_text(HEARTBEAT, encoding="utf-8")
    return [sys.executable, str(tmp / name), str(tmp / "beat.txt")]


def beating(tmp: Path) -> bool:
    """Whether the heartbeat file is still being written (the child lives)."""
    beat = tmp / "beat.txt"
    if not beat.exists():
        return False
    before = beat.read_text(encoding="utf-8")
    time.sleep(0.6)
    return beat.read_text(encoding="utf-8") != before


def interrupting_wait(real_wait):
    """``Popen.wait`` that raises KeyboardInterrupt on its first call (the user's Ctrl+C in the main wait)."""
    state = {"n": 0}

    def wait(self, *a, **k):
        state["n"] += 1
        if state["n"] == 1:
            raise KeyboardInterrupt()
        return real_wait(self, *a, **k)
    return wait


# ---- H1: the lock outlives the owned session -------------------------------------------------------------

class H1InterruptionTests(unittest.TestCase):
    def test_an_interruption_in_the_wait_kills_and_verifies_the_whole_tree_before_it_propagates(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            real = subprocess.Popen.wait
            with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                with self.assertRaises(KeyboardInterrupt):
                    rg.run_claude(spawn_args(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0)
            self.assertFalse(beating(tmp), "the owned CLI was still alive when the interruption propagated")

    def test_an_exception_while_the_session_is_being_prepared_also_ends_the_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            with mock.patch.object(rg.ContextGuard, "__init__", side_effect=RuntimeError("setup failed")):
                with self.assertRaises(RuntimeError):
                    rg.run_claude(spawn_args(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0,
                                  max_context_tokens=5)
            self.assertFalse(beating(tmp))

    def test_an_exception_inside_the_monitoring_ends_the_tree_and_the_watcher(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            calls = {"n": 0}

            def observe():
                calls["n"] += 1
                return None
            real = subprocess.Popen.wait
            with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                with self.assertRaises(KeyboardInterrupt):
                    rg.run_claude(spawn_args(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0,
                                  should_stop=observe, poll_s=0.05)
            seen = calls["n"]
            time.sleep(0.5)
            self.assertEqual(calls["n"], seen, "the watcher kept observing after the exception propagated")
            self.assertFalse(beating(tmp))

    def test_a_tree_that_cannot_be_verified_gone_is_a_failed_cleanup_not_a_normal_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            real, real_shutdown = subprocess.Popen.wait, rg.OwnedTree.shutdown

            def unverified(self, *a, **k):
                real_shutdown(self, *a, **k)   # really end it, so the test leaves nothing behind ...
                return 1                       # ... but report that the verification did not succeed
            with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)),                     mock.patch.object(rg.OwnedTree, "shutdown", unverified):
                with self.assertRaises(rg.OwnedSessionNotQuiescent) as raised:
                    rg.run_claude(spawn_args(tmp), "", tmp, rg.StreamMetrics([], None, None), 60.0)
            self.assertIsInstance(raised.exception.__cause__, KeyboardInterrupt)  # the original is chained

    def test_the_lock_is_released_only_after_the_interrupted_session_is_gone(self):
        s = Series(self)
        order = plan.goal_order(1)
        observed = {}
        real_release = pr.release_lock

        def spy(path, token):
            observed["beating_at_release"] = beating(Path(spy.tmp))
            real_release(path, token)

        class Deps(s.deps):
            @staticmethod
            def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                real = subprocess.Popen.wait
                with mock.patch.object(subprocess.Popen, "wait", interrupting_wait(real)):
                    return rg.run_claude(spawn_args(Path(spy.tmp)), "", Path(spy.tmp), metrics, 60.0)
        with tempfile.TemporaryDirectory() as tmp:
            spy.tmp = tmp
            with mock.patch.object(pr, "release_lock", spy):
                (s.root / "folders").mkdir(exist_ok=True)
                with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                        contextlib.redirect_stdout(io.StringIO()):
                    with self.assertRaises(KeyboardInterrupt):
                        pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
            self.assertFalse(observed["beating_at_release"], "the lock was released while the CLI was alive")
        self.assertFalse((s.root / "campaign" / "campaign.lock").exists())
        # and another executor may now run, because nothing of the first is left
        second = Series(self, root=s.root)
        self.assertEqual(second.run(extra=["--out", str(s.root / "out2")]), 0)
        self.assertEqual([g for g, _ in second.calls], order[1:])  # the interrupted attempt is consumed, not rerun

    def test_a_failed_cleanup_keeps_the_lock_and_tells_the_operator(self):
        s = Series(self)

        class Deps(s.deps):
            @staticmethod
            def run_claude(*a, **k):
                raise rg.OwnedSessionNotQuiescent("the owned tree could not be verified gone") from KeyboardInterrupt()
        stderr = io.StringIO()
        (s.root / "folders").mkdir(exist_ok=True)
        with mock.patch.object(rg, "ancestor_context", return_value=[]), contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(stderr):
            with self.assertRaises(rg.OwnedSessionNotQuiescent):
                pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
        lock = s.root / "campaign" / "campaign.lock"
        self.assertTrue(lock.exists())                      # kept
        self.assertIn("by hand", stderr.getvalue())         # and the operator is told to check and clear it
        other = Series(self, root=s.root)
        with self.assertRaises(SystemExit):
            other.run(extra=["--out", str(s.root / "out2")])
        self.assertEqual(other.calls, [])

    def test_an_unverified_tree_reported_by_a_normal_return_also_keeps_the_lock(self):
        order = plan.goal_order(1)
        s = Series(self)
        base = s.deps

        class Deps(base):
            @staticmethod
            def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                base.run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k)
                return {"exit_code": 0, "killed": None, "tree_left": 2}
        stderr = io.StringIO()
        (s.root / "folders").mkdir(exist_ok=True)
        with mock.patch.object(rg, "ancestor_context", return_value=[]), contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(stderr):
            code = pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
        self.assertEqual(code, 2)                 # the series ended at the first goal
        self.assertTrue((s.root / "campaign" / "campaign.lock").exists())
        self.assertIn("by hand", stderr.getvalue())
        self.assertEqual(len(s.calls), 1)
        self.assertEqual(s.calls[0][0], order[0])

    def test_a_pre_launch_failure_releases_the_lock_cleanly(self):
        s = Series(self, gate_status="rejected")
        with self.assertRaises(SystemExit):
            s.run()
        self.assertFalse((s.root / "campaign" / "campaign.lock").exists())

    def test_a_termination_signal_is_turned_into_the_same_orderly_exit(self):
        s = Series(self)
        before = signal.getsignal(signal.SIGTERM)
        base = s.deps

        class Deps(base):
            @staticmethod
            def run_claude(*a, **k):
                signal.raise_signal(signal.SIGTERM)
                raise AssertionError("the handler should have raised KeyboardInterrupt")
        (s.root / "folders").mkdir(exist_ok=True)
        with mock.patch.object(rg, "ancestor_context", return_value=[]), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(KeyboardInterrupt):
                pr.main(s.argv, deps=Deps, driver=object(), display=s.display)
        self.assertEqual(signal.getsignal(signal.SIGTERM), before)  # the previous handler is restored
        self.assertFalse((s.root / "campaign" / "campaign.lock").exists())


# ---- H2: the monitor cannot act after run_claude returns -------------------------------------------------

class Monitor:
    """An observation (``__call__``) with its guarded effect (``on_stop``), recording what happened."""

    def __init__(self, observe):
        self.observe, self.effects = observe, []

    def __call__(self):
        return self.observe()

    def on_stop(self, reason):
        self.effects.append(reason)


class H2WatcherGateTests(unittest.TestCase):
    def test_a_watcher_stuck_in_its_observation_makes_run_claude_fail_not_return(self):
        release = threading.Event()
        entered = threading.Event()

        def observe():
            entered.set()
            release.wait(30)
            return "terminal"
        monitor = Monitor(observe)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(0.3)\n", encoding="utf-8")
            with mock.patch.object(rg, "WATCHER_JOIN_S", 1.0):
                with self.assertRaises(rg.OwnedSessionNotQuiescent):
                    rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                  60.0, should_stop=monitor, poll_s=0.05)
            self.assertTrue(entered.is_set())
            release.set()           # the stuck observation finally answers "terminal" - after the session is over
            time.sleep(0.6)
            self.assertEqual(monitor.effects, [], "a late observation applied a side effect to an ended session")

    def test_no_late_effect_after_an_ordinary_fast_shutdown(self):
        state = {"late": False}

        def observe():
            return "terminal" if state["late"] else None
        monitor = Monitor(observe)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(0.3)\n", encoding="utf-8")
            ran = rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                60.0, should_stop=monitor, poll_s=0.05)
        self.assertIsNone(ran["killed"])
        state["late"] = True
        time.sleep(0.5)
        self.assertEqual(monitor.effects, [])

    def test_concurrent_requests_from_the_pump_the_watcher_and_the_timeout_produce_one_effect(self):
        for round_ in range(4):
            started = time.monotonic()
            monitor = Monitor(lambda: "terminal" if time.monotonic() - started > 0.25 else None)
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                (tmp / "child.py").write_text(
                    "import time\nfor _ in range(200):\n    print('{}', flush=True)\n    time.sleep(0.05)\n",
                    encoding="utf-8")
                ran = rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                    0.3, should_stop=monitor, poll_s=0.05)
            self.assertIn(ran["killed"], ("terminal", "timeout"), round_)
            self.assertLessEqual(len(monitor.effects), 1, round_)
            if ran["killed"] == "terminal":
                self.assertEqual(monitor.effects, ["terminal"])

    def test_the_effect_runs_before_the_kill_and_only_while_the_gate_is_open(self):
        order = []
        monitor = Monitor(lambda: "terminal")
        monitor.on_stop = lambda reason: order.append(("effect", reason))
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(60)\n", encoding="utf-8")
            ran = rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                60.0, should_stop=monitor, poll_s=0.05)
        self.assertEqual(ran["killed"], "terminal")
        self.assertEqual(order, [("effect", "terminal")])

    def test_the_runners_stop_file_effect_goes_through_the_gate(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            log, stop = tmp / "pixel.jsonl", tmp / "stop.flag"
            monitor = pr.session_stop_hook(log, stop)
            self.assertTrue(callable(monitor) and callable(getattr(monitor, "on_stop", None)))
            log.write_text(json.dumps({"tool": "click", "ok": False, "terminal": "safety"}) + "\n", encoding="utf-8")
            self.assertEqual(monitor(), "terminal")
            self.assertFalse(stop.exists(), "observing must have no side effect: the gate applies it")
            monitor.on_stop("terminal")
            self.assertTrue(stop.exists())


# ---- H3: the numeric DPI read needs a verified process-level context --------------------------------------

class FakeFn:
    def __init__(self, behaviour):
        self.behaviour, self.argtypes, self.restype, self.calls = behaviour, None, None, []

    def __call__(self, *args):
        self.calls.append(args)
        return self.behaviour(*args) if callable(self.behaviour) else self.behaviour


class FakeDll:
    """A Python stand-in for a DLL: every function is a ``FakeFn`` that records calls."""

    def __init__(self, **functions):
        self._fns = {name: FakeFn(b) for name, b in functions.items()}

    def __getattr__(self, name):
        try:
            return self.__dict__["_fns"][name]
        except KeyError:
            raise AttributeError(name) from None


V2 = -4


class DpiWorld:
    """Process and thread DPI contexts, modelled apart. ``process_settable`` is whether the process default can
    still be set; ``thread_override`` is a context set on the calling thread."""

    def __init__(self, process="unaware", process_settable=False, thread=None):
        self.process, self.process_settable, self.thread = process, process_settable, thread

    def effective_thread(self):
        return self.thread or self.process

    def set_process(self, ctx):
        if self.process_settable and ctx == V2:
            self.process = "per_monitor_v2"
            return 1
        return 0

    def set_thread(self, ctx):
        self.thread = "per_monitor_v2" if ctx == V2 else self.thread
        return 1

    @staticmethod
    def signed(ctx):
        value = getattr(ctx, "value", ctx)
        return value - 2 ** 64 if value > 2 ** 63 else value

    def dlls(self):
        names = {"unaware": 0, "system": 1, "per_monitor": 2, "per_monitor_v2": 2}

        def equals(a, b):
            return 1 if (a == "per_monitor_v2" and self.signed(b) == V2) else 0
        user32 = FakeDll(
            SetProcessDpiAwarenessContext=lambda ctx: self.set_process(self.signed(ctx)),
            SetThreadDpiAwarenessContext=lambda ctx: self.set_thread(self.signed(ctx)),
            GetThreadDpiAwarenessContext=lambda: self.effective_thread(),
            GetDpiAwarenessContextForProcess=lambda handle: self.process,
            AreDpiAwarenessContextsEqual=equals,
            GetAwarenessFromDpiAwarenessContext=lambda ctx: names.get(ctx, -1))
        kernel32 = FakeDll(GetCurrentProcess=lambda: 1)
        return user32, kernel32, FakeDll(), FakeDll()


class H3ProcessContextTests(unittest.TestCase):
    def ensure(self, world):
        with mock.patch.object(w32, "_dlls", return_value=world.dlls()):
            return w32.RealOs(None).ensure_dpi_aware()

    def test_a_process_fixed_as_unaware_is_refused_even_when_a_thread_override_succeeds(self):
        world = DpiWorld(process="unaware", process_settable=False)
        self.assertFalse(self.ensure(world))

    def test_a_thread_override_to_v2_over_an_unaware_process_is_refused(self):
        # the review's case: the process default is fixed as unaware, the thread was overridden to v2
        self.assertFalse(self.ensure(DpiWorld(process="unaware", process_settable=False, thread="per_monitor_v2")))

    def test_the_thread_setter_is_never_called(self):
        world = DpiWorld(process="unaware")
        with mock.patch.object(w32, "_dlls", return_value=(dlls := world.dlls())):
            w32.RealOs(None).ensure_dpi_aware()
        self.assertEqual(dlls[0].SetThreadDpiAwarenessContext.calls if "SetThreadDpiAwarenessContext" in dlls[0]._fns
                         else [], [])
        self.assertIsNone(world.thread)

    def test_a_process_that_can_still_be_set_to_per_monitor_v2_is_accepted(self):
        self.assertTrue(self.ensure(DpiWorld(process="unaware", process_settable=True)))

    def test_a_process_already_per_monitor_v2_is_accepted_whatever_the_setter_says(self):
        self.assertTrue(self.ensure(DpiWorld(process="per_monitor_v2", process_settable=False)))

    def test_system_aware_and_per_monitor_v1_processes_are_refused(self):
        for process in ("system", "per_monitor"):
            self.assertFalse(self.ensure(DpiWorld(process=process)), process)

    def test_a_thread_overridden_away_from_v2_under_a_v2_process_is_refused_too(self):
        # the rectangle read follows the thread: both levels must be per-monitor v2
        self.assertFalse(self.ensure(DpiWorld(process="per_monitor_v2", thread="unaware")))

    def test_an_unreadable_context_is_refused(self):
        world = DpiWorld(process="per_monitor_v2")
        dlls = world.dlls()
        dlls[0]._fns["GetDpiAwarenessContextForProcess"].behaviour = lambda handle: 0  # NULL: could not be read
        with mock.patch.object(w32, "_dlls", return_value=dlls):
            self.assertFalse(w32.RealOs(None).ensure_dpi_aware())

    def test_the_orchestration_has_no_thread_fallback_in_the_source(self):
        source = (HERE / "pixel_win32.py").read_text(encoding="utf-8")
        self.assertNotIn("SetThreadDpiAwarenessContext", source)

    def test_failed_process_set_with_a_successful_thread_set_yields_unknown_never_an_asserted_100(self):
        world = DpiWorld(process="unaware", process_settable=False)

        class Api(w32.RealOs):
            def primary_monitor_rect(self):
                return (0, 0, 2560, 1440)

            def primary_effective_dpi(self):
                return 96  # what the process contract returns for an unaware process, whatever the real scaling
        with mock.patch.object(w32, "_dlls", return_value=world.dlls()):
            self.assertIsNone(w32.measure_primary_display(Api(None)))


if __name__ == "__main__":
    unittest.main()
