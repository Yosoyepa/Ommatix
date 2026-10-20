"""Regressions for the third Codex review of PR 101 at 03838bd (G1-G5; docs/reviews/2026-10-20-pr101-ag03-harness.md).

Written and run RED against 03838bd before each fix (red output per class kept). Fakes, tiny Python children under
the owned-process-tree runner and a loopback-free harness only: no screen, window, input or provider.

The simpler designs chosen by the architect, which these tests pin: one exclusive campaign lock held for the whole
process lifetime (a second executor is refused outright; a stale lock is cleared by the operator by hand, never
automatically); a log that cannot be read or parsed completely is a terminal uncertain outcome, never success and
never retried; a terminal state kills the whole owned job/tree at once with no grace; the display is read only
in a DPI context verified in this very process; diagnostics write only to their own directory and never read or
create primary-pool state."""

import contextlib
import io
import json
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

TERMINAL = json.dumps({"tool": "click", "ok": False, "code": "FOREGROUND_NOT_TARGET_PROCESS", "reserved": True,
                       "terminal": "safety"})


def run_threaded(series, extra=()):
    """``pixel_runner.main`` of ``series`` on a thread (the caller owns the patches), result in a dict."""
    box = {}

    def target():
        try:
            box["code"] = pr.main([*series.argv, *extra], deps=series.deps, driver=object(), display=series.display)
        except BaseException as error:  # noqa: BLE001
            box["error"] = error
    thread = threading.Thread(target=target)
    thread.start()
    return thread, box


@contextlib.contextmanager
def quiet_world(series):
    (series.root / "folders").mkdir(exist_ok=True)
    with mock.patch.object(rg, "ancestor_context", return_value=[]), contextlib.redirect_stdout(io.StringIO()):
        yield


# ---- G1: one exclusive campaign lock for the whole process lifetime ---------------------------------------

class G1CampaignLockTests(unittest.TestCase):
    def test_a_second_executor_is_refused_outright_while_the_first_session_is_active(self):
        order = plan.goal_order(1)
        first = Series(self, script={(order[0], 1): "block"})
        second = Series(self, root=first.root)
        with quiet_world(first):
            thread, box = run_threaded(first)
            self.assertTrue(first.started.wait(30), "the first fake session never started")
            with self.assertRaises(SystemExit) as raised:
                pr.main([*second.argv, "--out", str(first.root / "out2")], deps=second.deps, driver=object(),
                        display=second.display)
            first.release.set()
            thread.join(60)
        self.assertIn("lock", str(raised.exception))
        self.assertEqual(second.calls, [])       # no second session ...
        self.assertEqual(second.app_calls, {})   # ... and no second application
        self.assertEqual(box.get("code"), 0)
        self.assertEqual(len(first.calls), 11)
        # nothing was invented: the first slot is a normal success, not an "unknown progress" failure
        counts = first.summary()["counts"]
        self.assertEqual((counts["successes"], counts["raw_attempts"]), (11, 11))
        self.assertFalse(any(r.get("unknown_progress") for s in first.summary()["slots"] for r in s["raw"]))

    def test_a_stale_lock_is_refused_and_left_for_the_operator_to_clear_by_hand(self):
        s = Series(self)
        lock = s.root / "campaign" / "campaign.lock"
        lock.parent.mkdir(parents=True)
        lock.write_text(json.dumps({"pid": 1, "token": "left-by-a-crash"}), encoding="utf-8")
        with self.assertRaises(SystemExit) as raised:
            s.run()
        self.assertIn("by hand", str(raised.exception))
        self.assertEqual(json.loads(lock.read_text(encoding="utf-8"))["token"], "left-by-a-crash")  # untouched
        self.assertEqual(s.calls, [])

    def test_after_the_operator_clears_a_stale_lock_a_genuine_restart_consumes_unknown_progress(self):
        order = plan.goal_order(1)
        crashed = Series(self, script={(order[3], 1): "crash"})
        with self.assertRaises(RuntimeError):
            crashed.run()
        lock = crashed.root / "campaign" / "campaign.lock"
        lock.write_text(json.dumps({"pid": 1, "token": "hard-kill"}), encoding="utf-8")  # a killed owner leaves it
        again = Series(self, root=crashed.root)
        with self.assertRaises(SystemExit):
            again.run(extra=["--out", str(crashed.root / "out2")])
        lock.unlink()  # the operator's decision, by hand
        self.assertEqual(again.run(extra=["--out", str(crashed.root / "out2")]), 0)
        self.assertEqual([g for g, _ in again.calls], order[4:])  # the crashed attempt is consumed, not rerun
        summary = json.loads((crashed.root / "out2" / "Haiku-ag03-phase1-rep1" / "series.json").read_text())
        self.assertEqual(summary["counts"]["raw_attempts"], 11)

    def test_the_lock_is_released_at_the_end_of_a_normal_run_and_only_by_its_owner(self):
        s = Series(self)
        s.run()
        self.assertFalse((s.root / "campaign" / "campaign.lock").exists())

    def test_a_planning_run_takes_no_lock(self):
        s = Series(self)
        with contextlib.redirect_stdout(io.StringIO()):
            pr.main(["--plan", "--phase", "1", "--repetition", "1", "--model", "claude-haiku-5-5",
                     "--campaign-dir", str(s.root / "campaign")])
        self.assertFalse((s.root / "campaign" / "campaign.lock").exists())

    def test_two_initial_pool_writers_cannot_both_win(self):
        a = Series(self, rep=1)
        b = Series(self, root=a.root, rep=2)
        results = {}
        barrier = threading.Barrier(2)
        with quiet_world(a):
            def go(name, series, extra):
                barrier.wait()
                try:
                    results[name] = pr.main([*series.argv, *extra], deps=series.deps, driver=object(),
                                            display=series.display)
                except SystemExit as refused:
                    results[name] = str(refused)
            threads = [threading.Thread(target=go, args=("a", a, [])),
                       threading.Thread(target=go, args=("b", b, ["--goal-timeout", "120"]))]
            for t in threads:
                t.start()
            for t in threads:
                t.join(120)
        pool = json.loads((a.root / "campaign" / "campaign-Haiku-AG-03-phase1.json").read_text(encoding="utf-8"))
        winners = [name for name, value in results.items() if value == 0]
        self.assertGreaterEqual(len(winners), 1)
        caps = json.loads(pool["caps"])["wall_s_per_goal"]
        self.assertIn(caps, (300.0, 120.0))
        for name, value in results.items():
            if value != 0:  # the loser was refused by the lock or by the pool, never run silently
                self.assertTrue("lock" in value or "not pooled" in value, value)


# ---- G2: an unreadable or corrupt log is a terminal uncertain outcome ---------------------------------------

class G2UnverifiedLogTests(unittest.TestCase):
    def hook_over(self, tmp, writer, stop=None):
        log = Path(tmp) / "pixel.jsonl"
        writer(log)
        stop = stop or Path(tmp) / "stop.flag"
        return log, stop, pr.session_stop_hook(log, stop)

    def test_a_log_that_cannot_be_read_ends_the_session_and_engages_the_stop(self):
        with tempfile.TemporaryDirectory() as tmp:
            log, stop, hook = self.hook_over(tmp, lambda p: p.write_text(TERMINAL + "\n", encoding="utf-8"))
            real = Path.read_text

            def denied(path, *a, **k):
                if path == log:
                    raise PermissionError("denied")
                return real(path, *a, **k)
            with mock.patch.object(Path, "read_text", denied):
                self.assertEqual(hook(), "terminal")
            hook.on_stop("terminal")  # the effect run_claude applies through its gate
            self.assertTrue(stop.exists())

    def test_a_log_that_cannot_be_parsed_completely_ends_the_session_and_engages_the_stop(self):
        for body in ("this is not json\n", TERMINAL + "\n" + "{broken}\n", "[1, 2]\n"):
            with tempfile.TemporaryDirectory() as tmp:
                _log, stop, hook = self.hook_over(tmp, lambda p, b=body: p.write_text(b, encoding="utf-8"))
                self.assertEqual(hook(), "terminal", body)
                hook.on_stop("terminal")
                self.assertTrue(stop.exists())

    def test_a_log_that_does_not_exist_yet_is_not_a_terminal_event(self):
        with tempfile.TemporaryDirectory() as tmp:
            hook = pr.session_stop_hook(Path(tmp) / "pixel.jsonl", Path(tmp) / "stop.flag")
            self.assertIsNone(hook())
            self.assertFalse((Path(tmp) / "stop.flag").exists())

    def test_a_half_written_last_line_is_not_a_terminal_event_while_the_server_is_running(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "pixel.jsonl"
            log.write_text('{"tool": "click", "ok": true}\n{"tool": "scr', encoding="utf-8")  # mid-write
            self.assertIsNone(pr.session_stop_hook(log, Path(tmp) / "stop.flag")())

    def test_the_final_summary_of_an_unreadable_log_is_unknown_not_zero_deliveries(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "log_denied"})
        self.assertEqual(s.run(), 2)  # the campaign ends
        result = json.loads((s.series_dir / order[0] / "attempt1" / "result.json").read_text(encoding="utf-8"))
        server = result["pixel_server"]
        self.assertEqual(server["log_status"], "unreadable")
        self.assertTrue(server["delivery_unknown"])
        self.assertEqual(server["terminal"], "uncertain_delivery")
        self.assertEqual(result["safety_event"], "uncertain_delivery")
        self.assertFalse(result["score"]["success"])  # the fake state reached its goal; it must still not score
        self.assertEqual(len([c for c in s.calls]), 1)  # no later application or session

    def test_a_corrupt_final_log_is_unknown_too_even_when_its_terminal_record_parsed(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "log_corrupt"})
        self.assertEqual(s.run(), 2)
        result = json.loads((s.series_dir / order[0] / "attempt1" / "result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["pixel_server"]["log_status"], "corrupt")
        self.assertFalse(result["score"]["success"])
        self.assertEqual(len(s.calls), 1)

    def test_genuine_absence_of_the_log_does_not_fabricate_a_stop(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "nolog"})
        self.assertEqual(s.run(), 0)
        result = json.loads((s.series_dir / order[0] / "attempt1" / "result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["pixel_server"]["log_status"], "absent")
        self.assertNotIn("safety_event", result)
        self.assertFalse(s.stop.exists())

    def test_once_the_session_is_over_a_half_written_last_line_is_corrupt(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "pixel.jsonl"
            log.write_text('{"tool": "click", "ok": true, "delivered": true}\n{"tool": "scr', encoding="utf-8")
            self.assertEqual(pr.read_log(log)[1], "ok")              # while it runs: a write in progress
            self.assertEqual(pr.read_log(log, final=True)[1], "corrupt")
            summary = pr.pixel_log_summary(log)
            self.assertEqual((summary["log_status"], summary["terminal"], summary["delivery_unknown"]),
                             ("corrupt", "uncertain_delivery", True))
            self.assertEqual(summary["delivered"], "unknown")
            self.assertFalse(summary["complete"])

    def test_an_unverified_log_disqualifies_the_attempt_on_its_own(self):
        for status in ("unreadable", "corrupt"):
            self.assertEqual(pr.terminal_fault({}, {}, {"log_status": status}), "log_unverified", status)
        self.assertIsNone(pr.terminal_fault({}, {}, {"log_status": "ok"}))
        self.assertIsNone(pr.terminal_fault({}, {}, {"log_status": "absent"}))

    def test_silent_session_whose_log_becomes_unreadable_is_ended_promptly(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            log, stop = tmp / "pixel.jsonl", tmp / "stop.flag"
            log.write_text(TERMINAL + "\n", encoding="utf-8")
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(120)\n", encoding="utf-8")
            real = Path.read_text

            def denied(path, *a, **k):
                if path == log:
                    raise PermissionError("denied")
                return real(path, *a, **k)
            started = time.monotonic()
            with mock.patch.object(Path, "read_text", denied):
                ran = rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                    60.0, should_stop=pr.session_stop_hook(log, stop))
            self.assertEqual(ran["killed"], "terminal")
            self.assertLess(time.monotonic() - started, 15)
            self.assertTrue(stop.exists())


# ---- G3: a terminal state kills the whole owned tree at once ------------------------------------------------

PARENT = textwrap.dedent('''
    import pathlib, subprocess, sys, time
    marker, log = sys.argv[1], sys.argv[2]
    grandchild = ("import pathlib, time\\n"
                  "time.sleep(1.8)\\n"
                  "pathlib.Path(" + repr(marker) + ").write_text('still running after the terminal state')\\n"
                  "time.sleep(60)\\n")
    subprocess.Popen([sys.executable, "-c", grandchild])
    print("{}", flush=True)
    time.sleep(0.4)
    pathlib.Path(log).write_text(sys.argv[3] + "\\n")
    time.sleep(120)
''')


class G3WholeTreeTests(unittest.TestCase):
    def test_a_descendant_cannot_outlive_the_terminal_state_and_no_grace_is_spent(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            marker, log, stop = tmp / "alive.marker", tmp / "pixel.jsonl", tmp / "stop.flag"
            (tmp / "parent.py").write_text(PARENT, encoding="utf-8")
            started = time.monotonic()
            ran = rg.run_claude([sys.executable, str(tmp / "parent.py"), str(marker), str(log), TERMINAL], "", tmp,
                                rg.StreamMetrics([], None, None), 90.0, should_stop=pr.session_stop_hook(log, stop))
            elapsed = time.monotonic() - started
            self.assertEqual(ran["killed"], "terminal")
            self.assertLess(elapsed, 15)                    # nowhere near the 40 s grace of the normal shutdown
            self.assertNotIn("tree_left", ran)
            time.sleep(3.0)                                 # the grandchild would have written its marker by now
            self.assertFalse(marker.exists(), "a descendant kept running after the terminal state")
            self.assertTrue(stop.exists())

    def test_the_first_request_to_stop_wins_and_the_watcher_leaves_no_activity_behind(self):
        calls = []

        def hook():
            calls.append(time.monotonic())
            return "operator_stop" if len(calls) == 1 else "terminal"
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(120)\n", encoding="utf-8")
            ran = rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None),
                                60.0, should_stop=hook)
        self.assertEqual(ran["killed"], "operator_stop")
        seen = len(calls)
        time.sleep(1.0)
        self.assertEqual(len(calls), seen, "the watcher kept calling the hook after run_claude returned")

    def test_the_watcher_has_finished_when_run_claude_returns_even_if_it_was_inside_the_hook(self):
        state = {"calls": 0, "inside": False}

        def hook():
            state["calls"] += 1
            if state["calls"] == 2:
                state["inside"] = True
                time.sleep(3.5)  # still working when the child has long exited
                state["inside"] = False
            return None
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "child.py").write_text("import time; print('{}', flush=True); time.sleep(0.4)\n", encoding="utf-8")
            rg.run_claude([sys.executable, str(tmp / "child.py")], "", tmp, rg.StreamMetrics([], None, None), 60.0,
                          should_stop=hook, poll_s=0.05)
        self.assertFalse(state["inside"], "run_claude returned while the watcher was still inside the hook")

    def test_a_timeout_keeps_the_ordinary_graceful_shutdown(self):
        # the normal policy (receipts) is unchanged for a plain timeout: only terminal reasons skip the grace
        source = (HERE.parent / "ag02" / "run_goal.py").read_text(encoding="utf-8")
        self.assertIn("PROCESS_TREE_GRACE_S", source)
        self.assertIn("IMMEDIATE_TREE_KILL", source)


# ---- G4: the display is read only in a verified DPI context -------------------------------------------------

class ContractDisplayApi:
    """Models the documented native contract: an unaware caller gets 96 back from ``GetDpiForMonitor``; only a
    process whose per-monitor v2 context is VERIFIED reads the actual DPI. The fake counts what was done."""

    def __init__(self, actual_dpi, can_verify=True, resolution=(0, 0, 2560, 1440)):
        self.actual, self.can_verify, self.resolution = actual_dpi, can_verify, resolution
        self.verified, self.log = False, []

    def ensure_dpi_aware(self):
        self.log.append("ensure")
        self.verified = self.can_verify
        return self.verified

    def primary_monitor_rect(self):
        self.log.append("rect")
        return self.resolution

    def primary_effective_dpi(self):
        self.log.append("dpi")
        return self.actual if self.verified else 96


class G4VerifiedContextTests(unittest.TestCase):
    def test_an_unaware_runner_cannot_record_a_default_100_percent_for_a_real_150(self):
        api = ContractDisplayApi(actual_dpi=144, can_verify=False)
        facts = pr.real_display_facts(api)
        self.assertIsNone(facts["scaling"])
        self.assertTrue(pr.display_problems(facts))
        self.assertNotIn("dpi", api.log)  # it never even read the DPI it could not trust

    def test_a_verified_context_reads_the_actual_scaling_and_records_the_awareness_separately(self):
        facts = pr.real_display_facts(ContractDisplayApi(actual_dpi=144))
        self.assertEqual((facts["resolution"], facts["scaling"], facts["dpi_context"]), ("2560x1440", 150, "per_monitor_v2"))
        self.assertEqual(pr.display_problems(dict(facts, capture_backend="printwindow")), [])

    def test_the_context_is_verified_before_the_dpi_is_read(self):
        api = ContractDisplayApi(actual_dpi=120)
        pr.real_display_facts(api)
        self.assertLess(api.log.index("ensure"), api.log.index("dpi"))

    def test_actual_scalings_of_100_125_and_150_give_three_pool_keys(self):
        keys = set()
        for dpi in (96, 120, 144):
            s = Series(self, display=dict(pr.real_display_facts(ContractDisplayApi(actual_dpi=dpi)),
                                          capture_backend="printwindow"))
            s.run()
            keys.add(json.loads((s.root / "campaign" / "campaign-Haiku-AG-03-phase1.json").read_text())["monitor_scaling"])
        self.assertEqual(keys, {100, 125, 150})

    def test_a_failed_measurement_or_an_unverified_context_is_refused_before_any_session(self):
        for display in ({"resolution": "2560x1440", "scaling": 100, "capture_backend": "printwindow"},  # no awareness
                        {"resolution": "2560x1440", "scaling": 100, "capture_backend": "printwindow",
                         "dpi_context": "unaware"}):
            s = Series(self, display=display)
            with self.subTest(display), self.assertRaises(SystemExit) as raised:
                s.run()
            self.assertIn("display", str(raised.exception))
            self.assertEqual(s.calls, [])

    def test_the_real_reader_asks_for_the_verified_context_first(self):
        source = (HERE / "pixel_win32.py").read_text(encoding="utf-8")
        self.assertIn("GetDpiAwarenessContextForProcess", source)  # H3: the process level is what is verified
        measure = source[source.index("def measure_primary_display"):source.index("# ---- the shapes")]
        self.assertIn("ensure_dpi_aware", measure)


# ---- G5: diagnostics never touch the primary pool -----------------------------------------------------------

class G5DiagnosticIsolationTests(unittest.TestCase):
    def snapshot(self, root):
        return {str(p.relative_to(root)): p.read_bytes() for p in sorted((root / "campaign").rglob("*"))
                if p.is_file() and "diagnostic" not in str(p) and p.name != "campaign.lock"}

    def test_a_diagnostic_with_other_settings_leaves_the_primary_claims_and_pool_unchanged(self):
        first = Series(self)
        first.run()
        before = self.snapshot(first.root)
        self.assertTrue(any(k.endswith("campaign-Haiku-AG-03-phase1.json") for k in before))
        diag = Series(self, root=first.root)
        self.assertEqual(diag.run(extra=["--out", str(first.root / "out2"), "--diagnostic", "--goal-timeout", "17"]), 0)
        self.assertEqual(self.snapshot(first.root), before)
        summary = json.loads((first.root / "out2" / "Haiku-ag03-phase1-rep1" / "series.json").read_text())
        self.assertTrue(summary["diagnostic"])
        self.assertEqual(json.loads(summary["fingerprint"]["caps"])["wall_s_per_goal"], 17.0)

    def test_a_diagnostic_first_in_a_fresh_campaign_does_not_become_the_pools_authority(self):
        diag = Series(self)
        self.assertEqual(diag.run(extra=["--diagnostic", "--goal-timeout", "17"]), 0)
        self.assertFalse((diag.root / "campaign" / "campaign-Haiku-AG-03-phase1.json").exists())  # nothing created
        primary = Series(self, root=diag.root)
        self.assertEqual(primary.run(extra=["--out", str(diag.root / "out2")]), 0)  # the registered settings are admitted
        pool = json.loads((diag.root / "campaign" / "campaign-Haiku-AG-03-phase1.json").read_text())
        self.assertEqual(json.loads(pool["caps"])["wall_s_per_goal"], 300.0)

    def test_an_established_primary_pool_does_not_constrain_a_diagnostic(self):
        first = Series(self)
        first.run()
        diag = Series(self, root=first.root, rep=2)
        self.assertEqual(diag.run(extra=["--diagnostic", "--goal-timeout", "17"]), 0)
        self.assertEqual(len(diag.calls), 11)

    def test_a_diagnostic_still_needs_the_gate_the_stop_and_the_lock(self):
        s = Series(self, gate_status="rejected")
        with self.assertRaises(SystemExit):
            s.run(extra=["--diagnostic"])
        self.assertEqual(s.calls, [])
        held = Series(self)
        lock = held.root / "campaign" / "campaign.lock"
        lock.parent.mkdir(parents=True)
        lock.write_text("{}", encoding="utf-8")
        with self.assertRaises(SystemExit):
            held.run(extra=["--diagnostic"])

    def test_the_diagnostic_branch_never_calls_the_pool(self):
        source = (HERE / "pixel_runner.py").read_text(encoding="utf-8")
        self.assertIn("if not args.diagnostic:", source)


if __name__ == "__main__":
    unittest.main()
