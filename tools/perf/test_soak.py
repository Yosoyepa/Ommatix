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
                "handles": 500, "threads": 40}

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


class ThreadsAndHandles(FakeHost):
    """At minute 30 one handle and one thread appear, and nothing else moves."""

    def counters(self, pid):
        stepped = self.t >= 30 * 60
        return {"rss": 300 * soak.MIB, "private": 250 * soak.MIB,
                "handles": 501 if stepped else 500, "threads": 41 if stepped else 40}


class ThreadsUnreadable(FakeHost):
    """The thread count cannot be read on this machine: the soak must go on anyway."""

    def counters(self, pid):
        return {"rss": 300 * soak.MIB, "private": 250 * soak.MIB, "handles": 500, "threads": None}


class HandleSchedule(FakeHost):
    """The handle count is `base` plus the deltas of `changes`, (second, delta) pairs; nothing
    else moves."""

    def __init__(self, changes, base=998):
        super().__init__()
        self.changes = changes
        self.base = base

    def counters(self, pid):
        handles = self.base + sum(d for at, d in self.changes if self.t >= at)
        return {"rss": 300 * soak.MIB, "private": 250 * soak.MIB, "handles": handles,
                "threads": 40}


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
            max_rss_growth_mib_per_hour=10.0, max_handle_growth_per_hour=None,
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

    def test_the_working_set_and_the_handle_count_are_always_judged(self):
        report, _ = self.run_soak(FakeHost())
        self.assertEqual(set(report["criteria"]),
                         {"working_set_growth_mib_per_hour", "handle_count"})

    def test_the_handle_slope_is_judged_only_when_its_flag_is_given(self):
        unjudged, _ = self.run_soak(FakeHost())
        self.assertNotIn("handle_growth_per_hour", unjudged["criteria"])
        judged, _ = self.run_soak(FakeHost(), max_handle_growth_per_hour=0.0)
        self.assertTrue(judged["criteria"]["handle_growth_per_hour"]["met"])  # flat

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

    def test_a_count_that_keeps_growing_is_not_met_and_a_flat_one_is(self):
        grows, _ = self.run_soak(GrowingHandles(), minutes=75.0, interval=30.0)
        self.assertFalse(grows["criteria"]["handle_count"]["met"])
        flat, _ = self.run_soak(FakeHost(), minutes=75.0, interval=30.0)
        self.assertTrue(flat["criteria"]["handle_count"]["met"])
        self.assertEqual(flat["criteria"]["handle_count"]["handle_step_deltas"], [])


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
    RULE_NOT_UNDER_TEST = {"met": True}  # the step rule needs samples; these tests do not use it

    def test_the_owners_thresholds_judge_the_section_12_numbers_as_they_were_recorded(self):
        """S0 section 12 (1.0.55): working set +1.6 MiB/h; handles 947 in every sample. The
        no-growth rule of 15 October 2026 is the optional handle flag, given explicitly."""
        resources = {"working_set_mib": {"n": 1320, "slope_per_hour": 1.6},
                     "handles": {"n": 1320, "slope_per_hour": 0.0}}
        args = SimpleNamespace(max_rss_growth_mib_per_hour=10.0,
                               max_handle_growth_per_hour=0.0, max_pending_regions=None)
        result = soak.criteria(resources, {"n": 1440, "max": 0}, self.RULE_NOT_UNDER_TEST, args)
        self.assertTrue(result["working_set_growth_mib_per_hour"]["met"])
        self.assertTrue(result["handle_growth_per_hour"]["met"])
        self.assertNotIn("pending_regions_max", result)  # no bound given: no queue criterion

    def test_any_positive_handle_slope_is_not_met_and_a_missing_trend_is_not_met(self):
        args = SimpleNamespace(max_rss_growth_mib_per_hour=10.0,
                               max_handle_growth_per_hour=0.0, max_pending_regions=None)
        grows = soak.criteria({"working_set_mib": {"slope_per_hour": 0.0},
                               "handles": {"slope_per_hour": 0.5}}, {"n": 0},
                              self.RULE_NOT_UNDER_TEST, args)
        self.assertFalse(grows["handle_growth_per_hour"]["met"])
        none = soak.criteria({"working_set_mib": {"n": 1}, "handles": {"slope_per_hour": 0.0}},
                             {"n": 0}, self.RULE_NOT_UNDER_TEST, args)
        self.assertFalse(none["working_set_growth_mib_per_hour"]["met"])


class Command(unittest.TestCase):
    def test_the_defaults_are_the_two_hours_of_rnf_06_and_the_owners_thresholds(self):
        args = soak.parse_args(["--config", "c.toml", "--out", "o.json"])
        self.assertEqual(args.minutes, 120.0)
        self.assertEqual(args.max_rss_growth_mib_per_hour, 10.0)
        self.assertIsNone(args.max_handle_growth_per_hour)  # the handle rule needs no flag
        self.assertIsNone(args.max_pending_regions)  # the frame sets the bound: no default

    def test_the_handle_rule_of_17_october_is_cited_in_the_docstring_and_the_help(self):
        self.assertIn("17 October 2026", soak.__doc__)
        self.assertIn("17 October 2026", soak.build_parser().format_help())

    def test_the_tool_has_no_way_to_send_an_action(self):
        source = Path(soak.__file__).read_text(encoding="utf-8")
        self.assertNotIn("execute_action", source)
        self.assertNotIn("SendInput", source)


class Diagnostics(Case):
    def test_the_threads_are_sampled_and_summarised_like_the_handles(self):
        report, _ = self.run_soak(ThreadsAndHandles(), minutes=60.0)
        self.assertEqual(report["samples"][0]["threads"], 40)
        threads = report["resources"]["threads"]
        self.assertEqual((threads["min"], threads["max"]), (40, 41))
        self.assertEqual(set(threads), set(report["resources"]["handles"]))

    def test_a_handle_step_after_the_warm_up_is_listed_with_the_threads_around_it(self):
        report, _ = self.run_soak(ThreadsAndHandles(), minutes=60.0)
        self.assertEqual(report["handle_steps"], [
            {"t_s": 1800.0, "handles_before": 500, "handles_after": 501,
             "threads_before": 40, "threads_after": 41}])

    def test_the_threads_and_the_steps_add_no_criterion(self):
        report, _ = self.run_soak(ThreadsAndHandles(), minutes=60.0)
        self.assertEqual(set(report["criteria"]),
                         {"working_set_growth_mib_per_hour", "handle_count"})

    def test_an_unreadable_thread_count_is_none_and_never_stops_the_soak(self):
        report, code = self.run_soak(ThreadsUnreadable(), minutes=2.0)
        self.assertEqual((code, report["ended"]), (0, "completed"))
        self.assertIsNone(report["samples"][0]["threads"])
        self.assertEqual(report["resources"]["threads"], {"n": 0})
        self.assertEqual(report["handle_steps"], [])


class HandleSteps(unittest.TestCase):
    @staticmethod
    def sample(t, handles, threads=40):
        return {"t_s": t, "handles": handles, "threads": threads}

    def test_a_flat_count_has_no_steps(self):
        samples = [self.sample(t, 500) for t in range(0, 100, 10)]
        self.assertEqual(soak.handle_steps(samples, warmup_s=0), [])

    def test_a_change_before_the_warm_up_is_not_a_step(self):
        samples = [self.sample(0, 499), self.sample(10, 500), self.sample(20, 500)]
        self.assertEqual(soak.handle_steps(samples, warmup_s=15), [])

    def test_each_change_after_the_warm_up_is_listed_with_the_threads_around_it(self):
        samples = [self.sample(20, 500, 40), self.sample(30, 501, 41), self.sample(40, 501, 41),
                   self.sample(50, 500, 40)]
        self.assertEqual(soak.handle_steps(samples, warmup_s=15), [
            {"t_s": 30, "handles_before": 500, "handles_after": 501,
             "threads_before": 40, "threads_after": 41},
            {"t_s": 50, "handles_before": 501, "handles_after": 500,
             "threads_before": 41, "threads_after": 40}])

    def test_a_sample_without_a_handle_count_is_skipped_and_the_last_known_count_is_kept(self):
        samples = [self.sample(20, 500), {"t_s": 25, "handles": None, "threads": None},
                   self.sample(30, 501)]
        steps = soak.handle_steps(samples, warmup_s=15)
        self.assertEqual([(s["t_s"], s["handles_before"]) for s in steps], [(30, 500)])

    def test_an_unreadable_thread_count_is_none_on_its_side_of_the_step(self):
        samples = [self.sample(20, 500, None), self.sample(30, 501, 41)]
        step = soak.handle_steps(samples, warmup_s=15)[0]
        self.assertEqual((step["threads_before"], step["threads_after"]), (None, 41))


class HandleRule(unittest.TestCase):
    """The owner's handle rule of 17 October 2026 on synthetic samples, five seconds apart: the
    handle count is 998 plus the deltas of ``changes``."""

    @staticmethod
    def series(end_s, changes=(), gaps=(), base=998):
        return [{"t_s": float(t), "threads": 40,
                 "handles": None if t in gaps else base + sum(d for at, d in changes if t >= at)}
                for t in range(0, end_s + 1, 5)]

    @staticmethod
    def rule(samples, completed=True, warmup_s=600.0):
        return soak.handle_rule(samples, warmup_s, completed)

    def test_the_example_of_section_18_one_step_of_plus_one_at_second_785_is_met(self):
        rule = self.rule(self.series(7195, changes=[(785, 1)]))
        self.assertEqual((rule["met"], rule["reason"]), (True, None))
        self.assertEqual((rule["handle_steps_after_warmup"], rule["handle_step_deltas"]), (1, [1]))
        self.assertEqual(rule["flat_final_s"], 6410.0)

    def test_the_example_of_section_15_a_step_at_minute_69_is_not_met_with_51_flat_minutes(self):
        rule = self.rule(self.series(7195, changes=[(69 * 60, 1)]))
        self.assertFalse(rule["met"])
        self.assertEqual(rule["flat_final_s"], 3055.0)
        self.assertIn("last 60 minutes", rule["reason"])

    def test_two_changes_after_the_warm_up_are_not_met_even_if_both_are_plus_one(self):
        rule = self.rule(self.series(7195, changes=[(785, 1), (3000, 1)]))
        self.assertFalse(rule["met"])
        self.assertEqual(rule["handle_step_deltas"], [1, 1])
        self.assertIn("changes", rule["reason"])

    def test_a_step_that_is_not_plus_one_is_not_met(self):
        for delta in (-1, 2):
            rule = self.rule(self.series(7195, changes=[(785, delta)]))
            self.assertFalse(rule["met"], delta)
            self.assertIn("+1", rule["reason"])

    def test_a_change_inside_the_warm_up_is_not_a_step_and_the_flat_time_starts_after_it(self):
        rule = self.rule(self.series(7195, changes=[(300, 1)]))
        self.assertEqual((rule["met"], rule["handle_steps_after_warmup"]), (True, 0))
        self.assertEqual(rule["flat_final_s"], 6595.0)

    def test_a_run_without_changes_is_met(self):
        rule = self.rule(self.series(7195))
        self.assertTrue(rule["met"])
        self.assertEqual(rule["handle_step_deltas"], [])

    def test_a_run_shorter_than_the_warm_up_plus_an_hour_cannot_meet_it(self):
        for end_s in (3595, 4195):  # an hour; seventy minutes less one sample
            rule = self.rule(self.series(end_s))
            self.assertFalse(rule["met"], end_s)
            self.assertIn("shorter", rule["reason"])

    def test_a_run_that_did_not_complete_cannot_meet_it(self):
        rule = self.rule(self.series(7195), completed=False)
        self.assertFalse(rule["met"])
        self.assertIn("complete", rule["reason"])

    def test_a_missing_handle_count_is_skipped_and_the_step_is_seen_after_the_gap(self):
        samples = self.series(7195, changes=[(850, 1)], gaps=set(range(800, 901, 5)))
        rule = self.rule(samples)
        self.assertEqual(rule["handle_step_deltas"], [1])  # 998 before the gap, 999 at 905 s
        self.assertTrue(rule["met"])
        self.assertEqual(rule["flat_final_s"], 6290.0)

    def test_without_handle_counts_after_the_warm_up_the_rule_is_not_met(self):
        rule = self.rule(self.series(7195, gaps=set(range(0, 7196, 5))))
        self.assertFalse(rule["met"])
        self.assertIn("handle count", rule["reason"])

    def test_a_last_sample_without_a_handle_count_is_not_met(self):
        rule = self.rule(self.series(7195, gaps={7195}))
        self.assertFalse(rule["met"])
        self.assertIn("last sample", rule["reason"])

    def test_the_warm_up_is_the_one_given(self):
        samples = self.series(7195, changes=[(500, 1)])
        self.assertEqual(self.rule(samples)["handle_steps_after_warmup"], 0)  # 600 s
        self.assertEqual(self.rule(samples, warmup_s=400.0)["handle_steps_after_warmup"], 1)

    def test_an_empty_series_is_not_met(self):
        self.assertFalse(self.rule([])["met"])

    @staticmethod
    def points(*pairs):
        """Samples at the given (second, handle count) pairs: the sampler records milliseconds,
        so the timestamps need not be five seconds apart."""
        return [{"t_s": t, "threads": 40, "handles": handles} for t, handles in pairs]

    def test_a_change_3599_96_seconds_before_the_end_is_not_in_the_final_hour(self):
        rule = self.rule(self.points((600.0, 998), (3595.04, 999), (7195.0, 999)))
        self.assertFalse(rule["met"])
        self.assertEqual(rule["flat_final_s"], 3599.96)
        self.assertIn("last 60 minutes", rule["reason"])

    def test_a_count_flat_for_exactly_3600_seconds_is_met(self):
        rule = self.rule(self.points((600.0, 998), (3595.0, 999), (7195.0, 999)))
        self.assertTrue(rule["met"])
        self.assertEqual(rule["flat_final_s"], 3600.0)


class HandleRuleInTheReport(Case):
    def test_a_120_minute_run_with_the_section_18_step_is_met_in_the_report(self):
        report, code = self.run_soak(HandleSchedule([(785, 1)]), minutes=120.0)
        self.assertEqual((code, report["ended"]), (0, "completed"))
        rule = report["criteria"]["handle_count"]
        self.assertTrue(rule["met"])
        self.assertEqual(rule["flat_final_s"], 6410.0)
        self.assertEqual(report["handle_steps"][0]["handles_before"], 998)

    def test_a_120_minute_run_with_the_section_15_step_is_not_met_in_the_report(self):
        report, code = self.run_soak(HandleSchedule([(69 * 60, 1)]), minutes=120.0)
        self.assertEqual((code, report["ended"]), (0, "completed"))
        rule = report["criteria"]["handle_count"]
        self.assertFalse(rule["met"])
        self.assertEqual(rule["flat_final_s"], 3055.0)


if __name__ == "__main__":
    unittest.main()
