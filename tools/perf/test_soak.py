"""Tests of soak.py with fakes: no process is started, no screen is read and no input exists.
Run: python -m unittest discover -s tools/perf"""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as common  # noqa: E402
import soak  # noqa: E402

APP, ENGINE = 100, 200


class FakeProc:
    def __init__(self, pid):
        self.pid = pid
        self.ended = None

    def terminate(self):
        self.ended = "terminated"

    def wait(self, timeout=None):
        return 0

    def kill(self):
        self.ended = "killed"


class FakeEngine:
    def __init__(self, pid=ENGINE, error_code=None, timeout_after=None):
        self.pid = pid
        self.calls = []
        self.closed = False
        self.error_code = error_code
        self.timeout_after = timeout_after

    def alive(self):
        return not self.closed

    def call(self, name, arguments, timeout=0):
        self.calls.append(name)
        if self.timeout_after is not None and len(self.calls) > self.timeout_after:
            raise common.McpTimeout("late")
        if name == "get_engine_metrics":
            n = self.calls.count("get_engine_metrics")
            return ({"observations": n, "pending_regions": n % 3, "rejected_actions": 0,
                     "delivered_actions": 0}, False, {}, 1.0)
        if self.error_code:
            # The contract's shape: the tool error is under `error` (chapter 07 §7.7).
            return ({"error": {"code": self.error_code, "retryable": True}}, True, {}, 1.0)
        return ({"geometry_epoch": 1, "degraded": False},
                False, {common.TIMINGS_KEY: {"total_us": 20_000}}, 30.0)

    def close(self):
        self.closed = True


class FakeGpu:
    reason = None

    def read(self):
        return 512 * soak.MIB

    def close(self):
        pass


class FakeHost(soak.Host):
    def __init__(self, front=lambda t: APP, running=(), engine=None):
        self.t = 0.0
        self.front = front
        self.already = set(running)
        self.engine = engine or FakeEngine()
        self.app = FakeProc(APP)
        self.started_engine = False
        self.foreground_reads = 0

    def now(self):
        return self.t

    def sleep(self, seconds):
        self.t += seconds

    def foreground_pid(self):
        self.foreground_reads += 1
        return self.front(self.t)

    def running(self, image):
        return image in self.already

    def counters(self, pid):
        assert pid == ENGINE
        return {"rss": (300 + self.t / 60) * soak.MIB, "private": (250 + self.t / 30) * soak.MIB,
                "handles": 500}

    def gpu(self, pid):
        return FakeGpu()

    def start_testapp(self, args):
        return self.app

    def start_engine(self, args):
        self.started_engine = True
        return self.engine


class FlatWorkingSet(FakeHost):
    """The working set does not move while the private bytes grow 120 MiB per hour."""

    def counters(self, pid):
        return {"rss": 300 * soak.MIB, "private": (250 + self.t / 30) * soak.MIB, "handles": 500}


class GrowingHandles(FakeHost):
    """One handle more every minute."""

    def counters(self, pid):
        return {"rss": 300 * soak.MIB, "private": 250 * soak.MIB,
                "handles": 500 + int(self.t // 60)}


class Case(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.config = Path(self.dir.name, "ok.toml")
        self.config.write_text('[engine]\nmode = "dry_run"\n', encoding="utf-8")
        self.out = Path(self.dir.name, "soak.json")
        self.written = []
        original = common.write_json
        common.write_json = lambda path, doc: self.written.append(doc["ended"])
        self.addCleanup(setattr, common, "write_json", original)

    def args(self, **extra):
        values = dict(
            config=self.config, exe=Path("ommatix.exe"), testapp=Path("testapp.exe"),
            out=self.out, minutes=2.0, interval=5.0, animate_ms=100, animate_labels=2,
            window_minutes=1.0, warmup_minutes=10.0, foreground_wait=3.0,
            max_foreground_losses=3, max_observe_errors=3, call_timeout=10.0,
            max_rss_growth_mib_per_hour=10.0, max_handle_growth_per_hour=0.0,
            max_pending_regions=None)
        values.update(extra)
        return SimpleNamespace(**values)

    def run_soak(self, host, **extra):
        return soak.Soak(self.args(**extra), host).run()


class Completed(Case):
    def test_a_soak_samples_everything_and_ends_only_its_own_processes(self):
        host = FakeHost()
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (0, "completed"))
        self.assertEqual(report["counts"]["ticks"], 24)  # 2 minutes at 5 s
        self.assertEqual(report["counts"]["observed"], 24)
        self.assertEqual(set(host.engine.calls), {"observe_screen", "get_engine_metrics"})
        self.assertTrue(host.engine.closed)
        self.assertEqual(host.app.ended, "terminated")
        s = report["samples"][-1]
        self.assertEqual((s["handles"], s["gpu_dedicated"]), (500, 512 * soak.MIB))
        self.assertEqual(s["total_ms"], 20.0)
        self.assertEqual(report["delivered_actions_max"], 0)
        self.assertEqual(report["pending_regions"]["max"], 2)
        self.assertEqual(report["latency_ms"]["rtt"]["n"], 24)
        self.assertEqual(len(report["latency_ms"]["rtt"]["windows"]), 2)

    def test_the_report_holds_no_screen_data(self):
        report, _ = self.run_soak(FakeHost())
        text = json.dumps(report)
        for forbidden in ("label", "bbox", "pixels", "coordinates", "elements"):
            self.assertNotIn(f'"{forbidden}', text)

    def test_the_growth_of_a_resource_is_reported_per_hour_after_the_warm_up(self):
        report, _ = self.run_soak(FakeHost(), minutes=20.0, warmup_minutes=2.0, interval=10.0)
        private = report["resources"]["private_mib"]
        self.assertAlmostEqual(private["slope_per_hour"], 120.0, places=1)  # 1 MiB per 30 s
        self.assertGreater(private["last_median"], private["first_median"])
        self.assertEqual(report["resources"]["handles"]["slope_per_hour"], 0.0)  # flat

    def test_a_partial_report_is_written_while_the_soak_runs(self):
        self.run_soak(FakeHost(), minutes=10.0)
        self.assertEqual(self.written[0], "running")

    def test_the_working_set_and_the_handles_are_always_judged(self):
        report, _ = self.run_soak(FakeHost())
        self.assertEqual(set(report["criteria"]),
                         {"working_set_growth_mib_per_hour", "handle_growth_per_hour"})

    def test_the_queue_is_judged_only_when_it_has_a_bound(self):
        unbounded, _ = self.run_soak(FakeHost())
        self.assertNotIn("pending_regions_max", unbounded["criteria"])
        bounded, _ = self.run_soak(FakeHost(), max_pending_regions=1)
        self.assertFalse(bounded["criteria"]["pending_regions_max"]["met"])  # max 2 > 1

    def test_a_working_set_growing_faster_than_ten_mib_per_hour_is_not_met(self):
        report, _ = self.run_soak(FakeHost(), minutes=20.0, warmup_minutes=2.0, interval=10.0)
        criterion = report["criteria"]["working_set_growth_mib_per_hour"]
        self.assertFalse(criterion["met"])
        self.assertAlmostEqual(criterion["measured"], 60.0, places=1)  # 1 MiB per minute
        self.assertEqual(criterion["limit"], 10.0)

    def test_the_rss_threshold_judges_the_working_set_and_not_the_private_bytes(self):
        report, _ = self.run_soak(FlatWorkingSet(), minutes=20.0, warmup_minutes=2.0,
                                  interval=10.0)
        self.assertTrue(report["criteria"]["working_set_growth_mib_per_hour"]["met"])
        self.assertNotIn("private_growth_mib_per_hour", report["criteria"])
        self.assertGreater(report["resources"]["private_mib"]["slope_per_hour"], 100)

    def test_a_growing_handle_count_is_not_met_and_a_flat_one_is(self):
        grows, _ = self.run_soak(GrowingHandles(), minutes=20.0, warmup_minutes=2.0,
                                 interval=10.0)
        self.assertFalse(grows["criteria"]["handle_growth_per_hour"]["met"])
        flat, _ = self.run_soak(FakeHost(), minutes=20.0, warmup_minutes=2.0, interval=10.0)
        self.assertTrue(flat["criteria"]["handle_growth_per_hour"]["met"])
        self.assertEqual(flat["criteria"]["handle_growth_per_hour"]["measured"], 0.0)


class Refusals(Case):
    def test_it_refuses_to_start_when_the_foreground_cannot_be_held(self):
        host = FakeHost(front=lambda t: 999)
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (2, "refused"))
        self.assertFalse(host.started_engine)
        self.assertEqual(host.app.ended, "terminated")

    def test_it_refuses_an_engine_or_a_test_application_already_running(self):
        for image in ("ommatix.exe", "ommatix-testapp.exe"):
            host = FakeHost(running=[image])
            report, code = self.run_soak(host)
            self.assertEqual((code, report["ended"]), (2, "refused"))
            self.assertIn(image, report["reason"])
            self.assertIsNone(host.app.ended)  # nothing was even started

    def test_it_refuses_a_configuration_that_could_deliver_input(self):
        self.config.write_text('[safety]\nactuation_enabled = true\n', encoding="utf-8")
        report, code = self.run_soak(FakeHost())
        self.assertEqual((code, report["ended"]), (2, "refused"))

    def test_it_refuses_a_missing_configuration(self):
        report, code = self.run_soak(FakeHost(), config=Path(self.dir.name, "absent.toml"))
        self.assertEqual(code, 2)


class EndingEarly(Case):
    def test_no_observation_is_taken_while_the_test_application_is_not_in_front(self):
        host = FakeHost(front=lambda t: APP if t < 20 else 999)
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (1, "foreground_lost"))
        self.assertEqual(report["counts"]["foreground_lost"], 3)
        observations = host.engine.calls.count("observe_screen")
        self.assertEqual(observations, report["counts"]["observed"])
        self.assertLessEqual(observations, 5)  # only the ticks before the loss

    def test_a_short_loss_is_counted_and_the_soak_goes_on(self):
        host = FakeHost(front=lambda t: 999 if 20 <= t < 30 else APP)
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (0, "completed"))
        self.assertEqual(report["counts"]["foreground_lost"], 2)
        self.assertEqual(report["counts"]["observed"], 22)

    def test_an_observation_the_screen_changed_under_is_not_counted_in_the_latencies(self):
        reads = {"n": 0}

        def front(t):
            reads["n"] += 1
            # Reads: 1 to hold the foreground, then 2 per tick (before and after the
            # observation); the 3rd read is the check after the first tick's observation.
            return 999 if reads["n"] == 3 else APP
        report, _ = self.run_soak(FakeHost(front=front), minutes=0.5)
        self.assertEqual(report["counts"]["discarded_latencies"], 1)
        self.assertEqual(report["latency_ms"]["rtt"]["n"], report["counts"]["observed"])

    def test_consecutive_refusals_by_the_engine_end_the_soak(self):
        host = FakeHost(engine=FakeEngine(error_code="CAPTURE_UNAVAILABLE"))
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (1, "observe_errors"))
        self.assertEqual(report["samples"][0]["error_code"], "CAPTURE_UNAVAILABLE")

    def test_a_server_that_stops_answering_ends_the_soak_and_the_processes_with_it(self):
        host = FakeHost(engine=FakeEngine(timeout_after=4))
        report, code = self.run_soak(host)
        self.assertEqual((code, report["ended"]), (1, "engine_failed"))
        self.assertEqual(report["reason"], "McpTimeout")
        self.assertTrue(host.engine.closed)
        self.assertEqual(host.app.ended, "terminated")

    def test_an_interrupt_still_cleans_up(self):
        host = FakeHost()

        def interrupt(pid):
            raise KeyboardInterrupt

        host.counters = interrupt
        report, _ = self.run_soak(host)
        self.assertEqual(report["ended"], "interrupted")
        self.assertTrue(host.engine.closed)
        self.assertEqual(host.app.ended, "terminated")


class Analysis(unittest.TestCase):
    def test_slope_is_per_hour_and_needs_a_trend(self):
        self.assertAlmostEqual(soak.slope_per_hour([(0, 0), (1800, 5)]), 10.0)
        self.assertIsNone(soak.slope_per_hour([(0, 1)]))
        self.assertIsNone(soak.slope_per_hour([(5, 1), (5, 2)]))

    def test_latency_windows_group_by_time(self):
        rows = soak.latency_windows([(1, 10.0), (2, 20.0), (61, 30.0)], 60)
        self.assertEqual([(r["from_s"], r["n"]) for r in rows], [(0, 2), (60, 1)])
        self.assertEqual(rows[0]["p95"], 20.0)

    def test_growth_leaves_the_warm_up_out(self):
        samples = [{"t_s": t, "rss": 100 if t < 10 else 5} for t in range(0, 20)]
        g = soak.growth(samples, "rss", warmup_s=10)
        self.assertEqual((g["n"], g["max"]), (10, 5))
        self.assertEqual(soak.growth([], "rss", 0), {"n": 0})


class Verdict(unittest.TestCase):
    def test_the_owners_thresholds_judge_the_section_12_numbers_as_they_were_recorded(self):
        """S0 section 12 (1.0.55): working set +1.6 MiB/h; handles 947 in every sample."""
        resources = {"working_set_mib": {"n": 1320, "slope_per_hour": 1.6},
                     "handles": {"n": 1320, "slope_per_hour": 0.0}}
        args = SimpleNamespace(max_rss_growth_mib_per_hour=10.0,
                               max_handle_growth_per_hour=0.0, max_pending_regions=None)
        result = soak.criteria(resources, {"n": 1440, "max": 0}, args)
        self.assertTrue(result["working_set_growth_mib_per_hour"]["met"])
        self.assertTrue(result["handle_growth_per_hour"]["met"])
        self.assertNotIn("pending_regions_max", result)  # no bound given: no queue criterion

    def test_any_positive_handle_slope_is_not_met_and_a_missing_trend_is_not_met(self):
        args = SimpleNamespace(max_rss_growth_mib_per_hour=10.0,
                               max_handle_growth_per_hour=0.0, max_pending_regions=None)
        grows = soak.criteria({"working_set_mib": {"slope_per_hour": 0.0},
                               "handles": {"slope_per_hour": 0.5}}, {"n": 0}, args)
        self.assertFalse(grows["handle_growth_per_hour"]["met"])
        none = soak.criteria({"working_set_mib": {"n": 1}, "handles": {"slope_per_hour": 0.0}},
                             {"n": 0}, args)
        self.assertFalse(none["working_set_growth_mib_per_hour"]["met"])


class Command(unittest.TestCase):
    def test_the_defaults_are_the_two_hours_of_rnf_06_and_the_owners_thresholds(self):
        args = soak.parse_args(["--config", "c.toml", "--out", "o.json"])
        self.assertEqual(args.minutes, 120.0)
        self.assertEqual((args.max_rss_growth_mib_per_hour, args.max_handle_growth_per_hour),
                         (10.0, 0.0))
        self.assertIsNone(args.max_pending_regions)  # the frame sets the bound: no default

    def test_the_tool_has_no_way_to_send_an_action(self):
        source = Path(soak.__file__).read_text(encoding="utf-8")
        self.assertNotIn("execute_action", source)
        self.assertNotIn("SendInput", source)


if __name__ == "__main__":
    unittest.main()
