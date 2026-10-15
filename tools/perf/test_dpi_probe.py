"""Tests of dpi_probe.py with fakes: no process is started, no screen is read and no input
exists. Run: python -m unittest discover -s tools/perf"""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import dpi_probe as probe  # noqa: E402
import perf_common as common  # noqa: E402

APP = 100
CAPTURE_OUTPUT = (
    "running 1 test\n"
    "dxgi output 0: 1920x1080 stride 7680 epoch 1 mapping DisplayMapping { origin_x: 0, "
    "origin_y: 0, frame_w: 1920, frame_h: 1080, os_w: 1920, os_h: 1080 } DxgiDiagnostics { "
    "frames_acquired: 1 }\ntest result: ok.\n")


def entry(percent, epoch, observed=True):
    if not observed:
        return {"observed": False, "attempted": False, "percent": percent, "skipped": "foreground"}
    labels = {label: 1 for label in (*probe.BUTTON_LABELS, probe.FIELD_LABEL)}
    return {"observed": True, "attempted": True, "percent": percent, "epoch": epoch,
            "labels": labels}


class FakeProc:
    pid = APP
    ended = None

    def terminate(self):
        self.ended = "terminated"

    def wait(self, timeout=None):
        return 0

    def kill(self):
        self.ended = "killed"


class FakeEngine:
    pid = 200

    def __init__(self, epochs=(1,), error_code=None, labels=None):
        self.epochs = list(epochs)
        self.error_code = error_code
        self.labels = probe.BUTTON_LABELS if labels is None else labels
        self.calls = []
        self.closed = False

    def call(self, name, arguments, timeout=0):
        self.calls.append(name)
        if self.error_code:
            return ({"code": self.error_code}, True, {}, 1.0)
        epoch = self.epochs.pop(0) if len(self.epochs) > 1 else self.epochs[0]
        elements = [{"label": label, "target_id": "t"} for label in self.labels]
        elements.append({"label": "the owner's own text", "target_id": "x"})
        return ({"geometry_epoch": epoch, "elements": elements, "total_elements": len(elements),
                 "degraded": False}, False, {}, 12.0)

    def close(self):
        self.closed = True


class FakeHost(probe.Host):
    def __init__(self, percents=(100,), front=lambda n: APP, engine=None, capture=CAPTURE_OUTPUT,
                 monitor=(1920, 1080), running=()):
        self.t = 0.0
        self.percents = list(percents)
        self.front = front
        self.engine = engine or FakeEngine()
        self.capture = capture
        self.monitor = monitor
        self.already = set(running)
        self.app = FakeProc()
        self.reads = 0
        self.capture_runs = 0

    def now(self):
        return self.t

    def sleep(self, seconds):
        self.t += seconds

    def foreground_pid(self):
        self.reads += 1
        return self.front(self.reads)

    def running(self, image):
        return image in self.already

    def start_testapp(self, args):
        return self.app

    def start_engine(self, args):
        return self.engine

    def read_display(self):
        percent = self.percents.pop(0) if len(self.percents) > 1 else self.percents[0]
        return {"dpi": round(percent * 96 / 100), "percent": percent,
                "monitor_px": list(self.monitor), "monitor_origin": [0, 0]}

    def run_capture_test(self):
        self.capture_runs += 1
        return 0, self.capture


class Case(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.config = Path(self.dir.name, "ok.toml")
        self.config.write_text('[engine]\nmode = "dry_run"\n', encoding="utf-8")

    def args(self, **extra):
        values = dict(config=self.config, exe=Path("ommatix.exe"), testapp=Path("t.exe"),
                      observations=3, interval=2.0, watch_minutes=0.0, skip_capture_test=False,
                      foreground_wait=3.0, call_timeout=10.0)
        values.update(extra)
        return SimpleNamespace(**values)

    def run_probe(self, host, **extra):
        return probe.Probe(self.args(**extra), host).run()


class Scaling(unittest.TestCase):
    def test_the_effective_dpi_becomes_the_percentage_of_the_dossier(self):
        self.assertEqual([probe.scaling_percent(d) for d in (96, 120, 144, 168)],
                         [100, 125, 150, 175])
        self.assertEqual(probe.PROFILE, (100, 125, 150))


class Capture(unittest.TestCase):
    def test_the_shape_line_of_the_manual_test_is_parsed(self):
        shape = probe.parse_capture(CAPTURE_OUTPUT)
        self.assertEqual((shape["width"], shape["height"], shape["epoch"]), (1920, 1080, 1))
        self.assertEqual(shape["mapping"]["os"], [1920, 1080])
        self.assertIsNone(probe.parse_capture("nothing here"))

    def test_the_frame_must_have_the_monitors_physical_size_and_an_identity_mapping(self):
        shape = probe.parse_capture(CAPTURE_OUTPUT)
        ok = probe.capture_checks(shape, (1920, 1080))
        self.assertEqual(ok, {"frame_matches_monitor": True, "mapping_is_identity": True})
        scaled = probe.capture_checks(shape, (1280, 720))  # a virtualised monitor size
        self.assertFalse(scaled["frame_matches_monitor"])
        shape["mapping"]["os"] = [1280, 720]
        self.assertFalse(probe.capture_checks(shape, (1920, 1080))["mapping_is_identity"])
        self.assertEqual(probe.capture_checks(None, (1, 1))["frame_matches_monitor"], None)


class Epoch(unittest.TestCase):
    def test_a_change_of_scaling_must_raise_the_epoch(self):
        timeline = [entry(100, 1), entry(100, 1), entry(125, 2), entry(125, 2), entry(125, 2)]
        result = probe.check_epoch(timeline)
        self.assertEqual((result["changes_answered"], result["met"]), (1, True))

    def test_an_epoch_that_does_not_move_after_a_change_fails(self):
        timeline = [entry(100, 1)] + [entry(125, 1) for _ in range(6)]
        result = probe.check_epoch(timeline)
        self.assertEqual((result["changes_unanswered"], result["met"]), (1, False))

    def test_a_change_at_the_very_end_is_pending_not_failed(self):
        timeline = [entry(100, 1), entry(100, 1), entry(125, 1)]
        result = probe.check_epoch(timeline)
        self.assertEqual((result["changes_pending_at_end"], result["met"]), (1, None))

    def test_a_rise_without_a_change_of_scaling_is_unexplained_and_a_fall_fails(self):
        self.assertFalse(probe.check_epoch([entry(100, 1), entry(100, 2)])["met"])
        self.assertEqual(
            probe.check_epoch([entry(100, 1), entry(100, 2)])["unexplained_increases"], 1)
        self.assertEqual(probe.check_epoch([entry(100, 2), entry(100, 1)])["decreases"], 1)

    def test_the_engine_noticing_a_moment_before_the_scaling_is_read_still_counts(self):
        timeline = [entry(100, 1), entry(100, 2), entry(125, 2), entry(125, 2)]
        self.assertTrue(probe.check_epoch(timeline)["met"])

    def test_skipped_observations_are_left_out(self):
        timeline = [entry(100, 1), entry(100, 0, observed=False), entry(125, 2)]
        self.assertTrue(probe.check_epoch(timeline)["met"])

    def test_without_any_change_there_is_nothing_to_say(self):
        self.assertIsNone(probe.check_epoch([entry(100, 1), entry(100, 1)])["met"])


class OutOfProfile(unittest.TestCase):
    def test_inside_the_profile_the_criterion_does_not_apply(self):
        self.assertEqual(probe.out_of_profile_result(125, []), "not_applicable")

    def test_outside_it_the_dossier_asks_for_an_explicit_refusal(self):
        refused = [{"attempted": True, "observed": False, "error_code": "UNSUPPORTED_GEOMETRY"}]
        self.assertEqual(probe.out_of_profile_result(175, refused), "flagged")
        served = [{"attempted": True, "observed": True}]
        self.assertEqual(probe.out_of_profile_result(175, served), "served_unflagged")
        other = [{"attempted": True, "observed": False, "error_code": "CAPTURE_UNAVAILABLE"}]
        self.assertEqual(probe.out_of_profile_result(175, other), "refused_with_another_code")
        self.assertEqual(probe.out_of_profile_result(175, []), "no_observation")


class Session(Case):
    def test_a_session_inside_the_profile_meets_every_criterion(self):
        host = FakeHost(percents=(125,))
        record, code = self.run_probe(host)
        self.assertEqual((code, record["ended"]), (0, "completed"))
        self.assertEqual(record["scaling"]["percent"], 125)
        self.assertTrue(record["criteria"]["met_all"])
        self.assertEqual(record["criteria"]["out_of_profile"], "not_applicable")
        self.assertTrue(record["criteria"]["buttons_found"])
        self.assertEqual(set(host.engine.calls), {"observe_screen"})
        self.assertTrue(host.engine.closed)
        self.assertEqual(host.app.ended, "terminated")

    def test_the_record_keeps_counts_of_the_test_applications_labels_and_nothing_else(self):
        record, _ = self.run_probe(FakeHost())
        text = json.dumps(record)
        self.assertNotIn("the owner's own text", text)
        self.assertNotIn("target_id", text)
        self.assertIn('"Sumar": 1', text)

    def test_a_frame_that_is_not_the_monitors_size_fails_the_mapping_criterion(self):
        record, code = self.run_probe(FakeHost(percents=(150,), monitor=(1280, 720)))
        self.assertEqual(code, 1)
        self.assertFalse(record["criteria"]["frame_matches_monitor"])
        self.assertFalse(record["criteria"]["met_all"])

    def test_a_moving_epoch_without_a_change_fails_the_stability_criterion(self):
        record, _ = self.run_probe(FakeHost(engine=FakeEngine(epochs=(1, 2, 2))))
        self.assertFalse(record["criteria"]["epoch_stable_without_change"])

    def test_a_missing_button_fails_the_elements_criterion(self):
        engine = FakeEngine(labels=probe.BUTTON_LABELS[:-1])
        record, _ = self.run_probe(FakeHost(engine=engine))
        self.assertFalse(record["criteria"]["buttons_found"])

    def test_outside_the_profile_a_served_observation_is_reported_unflagged(self):
        record, code = self.run_probe(FakeHost(percents=(175,)))
        self.assertEqual(record["criteria"]["out_of_profile"], "served_unflagged")
        self.assertFalse(record["criteria"]["met_all"])
        self.assertEqual(code, 1)

    def test_outside_the_profile_an_explicit_refusal_is_flagged(self):
        engine = FakeEngine(error_code="UNSUPPORTED_GEOMETRY")
        record, code = self.run_probe(FakeHost(percents=(175,), engine=engine))
        self.assertEqual(record["criteria"]["out_of_profile"], "flagged")
        self.assertEqual(code, 0)

    def test_the_capture_test_can_be_skipped(self):
        host = FakeHost()
        record, _ = self.run_probe(host, skip_capture_test=True)
        self.assertEqual(host.capture_runs, 0)
        self.assertIsNone(record["criteria"]["frame_matches_monitor"])

    def test_a_watch_records_the_epoch_following_a_change_made_by_hand(self):
        # 3 initial observations at 100 %, then the owner sets 125 % during the watch.
        percents = [100] * 4 + [125] * 40
        engine = FakeEngine(epochs=[1] * 5 + [2])
        host = FakeHost(percents=percents, engine=engine)
        record, _ = self.run_probe(host, watch_minutes=0.5)
        check = record["epoch_timeline"]["check"]
        self.assertEqual((check["changes_answered"], check["met"]), (1, True))
        self.assertTrue(record["criteria"]["epoch_follows_scaling"])
        self.assertNotIn("labels", json.dumps(record["epoch_timeline"]))

    def test_while_the_owner_is_in_settings_nothing_is_observed(self):
        host = FakeHost(front=lambda n: APP if n < 12 else 999)
        record, _ = self.run_probe(host, watch_minutes=0.5)
        skipped = record["epoch_timeline"]["skipped_foreground"]
        self.assertGreater(skipped, 0)
        self.assertEqual(host.engine.calls.count("observe_screen"),
                         sum(1 for e in record["observations"] if e["attempted"])
                         + sum(1 for e in record["epoch_timeline"]["entries"] if e["attempted"]))


class Refusals(Case):
    def test_it_refuses_to_start_when_the_foreground_cannot_be_held(self):
        host = FakeHost(front=lambda n: 999)
        record, code = self.run_probe(host)
        self.assertEqual((code, record["ended"]), (2, "refused"))
        self.assertEqual(host.engine.calls, [])
        self.assertEqual(host.capture_runs, 0)
        self.assertEqual(host.app.ended, "terminated")

    def test_it_refuses_a_running_engine_and_a_configuration_with_input(self):
        record, code = self.run_probe(FakeHost(running=["ommatix.exe"]))
        self.assertEqual((code, record["ended"]), (2, "refused"))
        self.config.write_text('[safety]\nactuation_enabled = true\n', encoding="utf-8")
        record, code = self.run_probe(FakeHost())
        self.assertEqual((code, record["ended"]), (2, "refused"))


class Campaign(unittest.TestCase):
    def test_a_campaign_is_complete_when_each_scaling_of_the_profile_has_a_passing_session(self):
        def session(percent, met):
            return {"scaling": {"percent": percent},
                    "criteria": {"scaling_in_profile": percent in probe.PROFILE, "met_all": met,
                                 "out_of_profile": "not_applicable"}}
        joined = probe.combine([session(100, True), session(125, True), session(150, False)])
        self.assertEqual(joined["profile_scalings_complete"], [100, 125])
        self.assertEqual(joined["profile_scalings_missing"], [150])
        self.assertFalse(joined["epoch_transition_checked"])


class Command(unittest.TestCase):
    def test_the_tool_never_changes_the_scaling_or_sends_input(self):
        source = Path(probe.__file__).read_text(encoding="utf-8")
        for forbidden in ("execute_action", "ChangeDisplaySettings", "SendInput", "SetDpi",
                          "SendMessage", "mouse_event", "keybd_event"):
            self.assertNotIn(forbidden, source)

    def test_combine_needs_no_configuration_but_probing_does(self):
        probe.parse_args(["--combine", "a.json", "--out", "o.json"])
        with self.assertRaises(SystemExit):
            probe.parse_args(["--out", "o.json"])


if __name__ == "__main__":
    unittest.main()
