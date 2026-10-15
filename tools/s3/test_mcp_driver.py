"""Tests of the S3 driver (no engine, no screen, no input): the engine command and the
live-round steps, run against a fake engine that follows the gate's order."""
import argparse
import contextlib
import io
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import mcp_driver  # noqa: E402


class ServeArgvTest(unittest.TestCase):
    def test_a_live_run_passes_no_fixed_session_id(self):
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", True, None)
        self.assertNotIn("--session-id", argv)
        self.assertEqual(argv[-2:], ["--input", "sendinput"])

    def test_a_live_run_with_a_session_id_is_refused_before_the_engine_starts(self):
        with self.assertRaises(ValueError):
            mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", True, "s3-supervised")

    def test_observe_and_dry_runs_keep_an_identifier(self):
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", False, None)
        self.assertEqual(argv[-2:], ["--session-id", mcp_driver.PROBE_SESSION_ID])
        self.assertNotIn("--input", argv)
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", False, "mine")
        self.assertEqual(argv[-2:], ["--session-id", "mine"])


# ---- doubles: an engine that follows the gate's order, a clock and the foreground -----------


class FakeClock:
    """``sleep`` advances ``monotonic``; nothing really waits."""

    def __init__(self):
        self.now = 1000.0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakeEngine:
    """The parts of ``ommatix serve`` the live-round steps rely on: request_id replay, the
    purge file (honoured once per instance, at a request boundary), the snapshot's age
    (ADR-0025), the operator's stop and the allowlisted foreground. ``bug`` names a defect
    to inject so that a step can be shown to notice it."""

    max_age_s = 10.0

    def __init__(self, clock, foreground_of, stop_file, purge_file, bug=None):
        self.clock = clock
        self.foreground_of = foreground_of
        self.stop_file = stop_file
        self.purge_file = purge_file
        self.bug = bug
        self.next_element = 1
        self.next_snapshot = 1
        self.published = None
        self.spent = False
        self.journal = {}
        self.delivered = 0
        self.rejected = 0
        self.honoured = None
        self.calls = []

    def call(self, name, arguments):
        self.calls.append(name)
        self._boundary()
        if name == "get_engine_metrics":
            return ({"mode": "live", "observations": 0, "rejected_actions": self.rejected,
                     "delivered_actions": self.delivered}, False, {}, 0.1)
        if name == "observe_screen":
            return self._observe(), False, {}, 1.0
        return self._act(arguments), False, {}, 1.0

    def _boundary(self):
        if self.purge_file is None or not self.purge_file.exists():
            self.honoured = None
            return
        st = self.purge_file.stat()
        instance = (st.st_mtime_ns, st.st_size)
        if instance != self.honoured and self.bug != "purge_ignored":
            self.honoured = instance
            self.published = None

    def _observe(self):
        sid = f"s{self.next_snapshot}"
        self.next_snapshot += 1
        elements = []
        for label in ("Editar", "", "Otro"):
            elements.append({"target_id": f"e{self.next_element}", "revision": 1,
                             "role": "unknown", "label": label,
                             "actionability": "confirmed", "label_source": "ocr"})
            self.next_element += 1
        if self.bug == "ids_reused":
            elements[0]["target_id"] = "e1"
        self.published = {"id": sid, "at": self.clock.now}
        self.spent = False
        return {"session_id": "s", "snapshot_id": sid, "geometry_epoch": 1, "frame_seq": 1,
                "age_ms": 0, "degraded": False, "total_elements": len(elements),
                "elements": elements}

    def _refuse(self, code, message):
        self.rejected += 1
        return {"status": "rejected", "input_delivered": False,
                "error": {"code": code, "message": message}}

    def _act(self, a):
        stored = self.journal.get(a["request_id"])
        if stored is not None:
            return stored
        out = self._decide(a)
        self.journal[a["request_id"]] = out
        return out

    def _decide(self, a):
        if self.published is None or self.spent or a["snapshot_id"] != self.published["id"]:
            return self._refuse("STALE_SNAPSHOT",
                                "the action does not reference the published snapshot")
        if self.bug != "ttl_ignored" and self.clock.now - self.published["at"] > self.max_age_s:
            return self._refuse("STALE_SNAPSHOT",
                                "the snapshot is older than [engine] max_snapshot_age_ms")
        if a["dry_run"]:
            return {"status": "simulated", "input_delivered": False}
        stopped = self.stop_file is not None and self.stop_file.exists()
        if stopped and self.bug != "stop_ignored":
            return self._refuse("POLICY_DENIED", "the operator stopped live input")
        if self.foreground_of() != "notepad.exe" and self.bug != "foreground_ignored":
            return self._refuse("POLICY_DENIED", "foreground not allowlisted")
        self.delivered += 1
        self.spent = True
        return {"status": "effect_observed", "input_delivered": True, "verified": True,
                "observed_effect": "visual_change"}


class RoundCase(unittest.TestCase):
    """A ``Run`` over the fake engine, a fake clock and a scripted foreground."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)
        self.clock = FakeClock()
        self.fg = "notepad.exe"
        self.fg_script = None
        patches = [
            mock.patch.object(mcp_driver.time, "sleep", self.clock.sleep),
            mock.patch.object(mcp_driver.time, "monotonic", self.clock.monotonic),
            mock.patch.object(mcp_driver.subprocess, "Popen",
                              side_effect=AssertionError("a step must start no process")),
            mock.patch.object(mcp_driver.subprocess, "run",
                              side_effect=AssertionError("a step must start no process")),
            mock.patch.object(mcp_driver, "foreground", self.read_foreground),
        ]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        self.stop = self.dir / "stop.flag"
        self.purge = self.dir / "purge.flag"

    def read_foreground(self):
        if self.fg_script is not None:
            self.fg = self.fg_script(self.clock.now)
        return self.fg, True

    def make(self, bug=None, max_live=10, labels="Editar,Edit", max_age_ms=10_000):
        self.engine = FakeEngine(self.clock, lambda: self.fg, self.stop, self.purge, bug)
        self.engine.max_age_s = max_age_ms / 1000
        args = argparse.Namespace(
            session_id=None, menu_labels=labels, max_live=max_live, observations=0,
            ttl_margin_ms=2000, owner_wait_s=20, switch_to="calculatorapp.exe")
        self.run = mcp_driver.Run(args, self.engine, self.stop, purge_file=self.purge,
                                  max_age_ms=max_age_ms, allowed_apps=("notepad.exe",))
        return self.run

    def go(self, step):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                getattr(self.run, step)()
                aborted = None
            except mcp_driver.Abort as e:
                aborted = str(e)
        self.out, self.err = out.getvalue(), err.getvalue()
        events = [json.loads(x) for x in self.out.splitlines() if x.startswith("{")]
        return aborted, events

    @staticmethod
    def failed_checks(events):
        return [e["name"] for e in events if e["event"] == "check" and not e["ok"]]


class Sec02ReplayUnderStopTest(RoundCase):
    def test_sec02r_replay_returns_the_stored_result_and_a_new_request_is_denied(self):
        self.make()
        aborted, events = self.go("sec02r")
        self.assertIsNone(aborted)
        self.assertEqual(self.failed_checks(events), [])
        # the opening click and the closing click are the only deliveries
        self.assertEqual(self.engine.delivered, 2)
        self.assertEqual(self.engine.rejected, 1)

    def test_sec02r_removes_only_the_stop_file_it_created_and_says_so(self):
        self.make()
        self.go("sec02r")
        self.assertFalse(self.stop.exists())
        self.assertIn("stop file", self.err)

    def test_sec02r_never_prints_a_label(self):
        self.make()
        self.go("sec02r")
        self.assertNotIn("Otro", self.out + self.err)  # screen text; the menu names are fixed constants

    def test_sec02r_a_delivery_under_the_stop_leaves_the_stop_engaged_and_aborts(self):
        self.make(bug="stop_ignored")
        aborted, _ = self.go("sec02r")
        self.assertIsNotNone(aborted)
        self.assertTrue(self.stop.exists())

    def test_sec02r_refuses_labels_other_than_editar(self):
        self.make(labels="Archivo,Editar")
        aborted, _ = self.go("sec02r")
        self.assertIn("Editar", aborted)
        self.assertEqual(self.engine.calls, [])

    def test_sec02r_respects_the_live_budget(self):
        self.make(max_live=2)
        aborted, _ = self.go("sec02r")
        self.assertIn("budget", aborted)


class PurgeTest(RoundCase):
    def test_purge_refuses_the_action_on_the_old_snapshot_and_issues_new_ids(self):
        self.make()
        aborted, events = self.go("purge")
        self.assertIsNone(aborted)
        self.assertEqual(self.failed_checks(events), [])
        self.assertEqual(self.engine.delivered, 0)

    def test_purge_removes_only_the_purge_file_it_created_and_says_so(self):
        self.make()
        self.go("purge")
        self.assertFalse(self.purge.exists())
        self.assertIn("purge file", self.err)

    def test_purge_refuses_to_start_when_the_purge_file_already_exists(self):
        self.purge.write_text("operator's")
        self.make()
        aborted, _ = self.go("purge")
        self.assertIn("exists", aborted)
        self.assertTrue(self.purge.exists())
        self.assertEqual(self.engine.calls, [])

    def test_purge_needs_a_configured_purge_file(self):
        self.make()
        self.run.purge_file = None
        aborted, _ = self.go("purge")
        self.assertIn("purge_file", aborted)

    def test_purge_notices_an_engine_that_ignores_the_file_and_sends_no_live_click(self):
        self.make(bug="purge_ignored")
        _, events = self.go("purge")
        self.assertIn("purge.dry_refused", self.failed_checks(events))
        self.assertEqual(self.engine.delivered, 0)

    def test_purge_notices_a_reused_identifier(self):
        self.make(bug="ids_reused")
        _, events = self.go("purge")
        self.assertIn("purge.ids_fresh", self.failed_checks(events))


class TtlTest(RoundCase):
    def test_ttl_waits_the_window_plus_the_margin_then_both_refusals_hold(self):
        self.make()
        t0 = self.clock.now
        aborted, events = self.go("ttl")
        self.assertIsNone(aborted)
        self.assertEqual(self.failed_checks(events), [])
        self.assertGreaterEqual(self.clock.now - t0, 12.0)
        self.assertEqual(self.engine.delivered, 0)

    def test_ttl_reads_the_window_from_the_configuration(self):
        self.make(max_age_ms=4000)
        t0 = self.clock.now
        self.go("ttl")
        self.assertGreaterEqual(self.clock.now - t0, 6.0)
        self.assertLess(self.clock.now - t0, 12.0)

    def test_ttl_sends_no_live_click_when_the_engine_does_not_enforce_the_window(self):
        self.make(bug="ttl_ignored")
        _, events = self.go("ttl")
        self.assertIn("ttl.dry_refused", self.failed_checks(events))
        self.assertEqual(self.engine.delivered, 0)

    def test_ttl_refuses_an_unreasonably_long_window(self):
        self.make(max_age_ms=600_000)
        aborted, _ = self.go("ttl")
        self.assertIn("window", aborted)


class Sec05ManualTest(RoundCase):
    def owner_switches_at(self, seconds, to="calculatorapp.exe", back_at=None):
        start = self.clock.now

        def script(now):
            t = now - start
            if t >= seconds and (back_at is None or t < back_at):
                return to
            return "notepad.exe"

        self.fg_script = script

    def test_sec05m_denies_the_click_after_the_owner_switches_and_waits_for_the_return(self):
        self.make(max_age_ms=120_000)
        self.owner_switches_at(5, back_at=12)
        aborted, events = self.go("sec05m")
        self.assertIsNone(aborted)
        self.assertEqual(self.failed_checks(events), [])
        self.assertEqual(self.engine.delivered, 0)
        self.assertEqual(self.engine.rejected, 1)
        self.assertIn("OWNER", self.err)
        self.assertIn("calculatorapp.exe", self.err)

    def test_sec05m_without_a_switch_sends_nothing(self):
        self.make(max_age_ms=120_000)
        aborted, events = self.go("sec05m")
        self.assertIsNone(aborted)
        self.assertTrue(any(e["event"] == "sec05m" and "skipped" in e for e in events))
        self.assertNotIn("execute_action", self.engine.calls)

    def test_sec05m_a_switch_to_an_unexpected_application_sends_nothing(self):
        self.make(max_age_ms=120_000)
        self.owner_switches_at(3, to="somethingelse.exe")
        _, events = self.go("sec05m")
        self.assertNotIn("execute_action", self.engine.calls)
        self.assertTrue(any(e["event"] == "sec05m" and "skipped" in e for e in events))

    def test_sec05m_refuses_a_window_shorter_than_the_owner_wait(self):
        self.make(max_age_ms=10_000)
        aborted, _ = self.go("sec05m")
        self.assertIn("max_snapshot_age_ms", aborted)
        self.assertEqual(self.engine.calls, [])

    def test_sec05m_refuses_a_switch_target_that_is_allowlisted(self):
        self.make(max_age_ms=120_000)
        self.run.args.switch_to = "notepad.exe"
        aborted, _ = self.go("sec05m")
        self.assertIn("allowlist", aborted)

    def test_sec05m_notices_a_click_that_was_delivered_and_engages_the_stop(self):
        self.make(bug="foreground_ignored", max_age_ms=120_000)
        self.owner_switches_at(2)
        aborted, _ = self.go("sec05m")
        self.assertIsNotNone(aborted)
        self.assertTrue(self.stop.exists())

    def test_sec05m_starts_no_process(self):
        # setUp makes Popen and run raise: a step that touched them would fail here.
        self.make(max_age_ms=120_000)
        self.owner_switches_at(2, back_at=8)
        aborted, _ = self.go("sec05m")
        self.assertIsNone(aborted)


class LiveStepsDispatchTest(unittest.TestCase):
    def test_the_new_steps_are_known_to_the_live_scenario(self):
        for name in ("sec02r", "purge", "ttl", "sec05m"):
            self.assertIn(name, mcp_driver.LIVE_STEPS)


if __name__ == "__main__":
    unittest.main()
