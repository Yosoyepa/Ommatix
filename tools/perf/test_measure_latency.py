"""Tests of measure_latency.py. Fakes only: no test application, no demo, no input, no screen.

Run: python -m unittest discover -s tools/perf -p "test_*.py"
"""
import contextlib
import copy
import io
import json
import re
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure_latency as m  # noqa: E402

COMMIT = "a" * 40
SESSION = re.compile(r"(?P<case>[a-z0-9]+)-(?P<label>[A-Za-z0-9_-]+)-p(?P<p>\d{3})")
STAGES = ("capture", "plan", "detect", "recognize", "accessibility", "fuse", "track",
          "commit", "response_build", "total")


# ---- builders ---------------------------------------------------------------------------------

def plan(work="regions", reason=None, regions=1, area=4096.0, inferences=None, shapes=None,
         truncated=False):
    return {"work": work, "full_refresh_reason": reason, "region_count": regions,
            "region_area_px": area, "inference_count": inferences, "input_shapes": shapes,
            "accessibility_truncated": truncated, "controls_n": 10, "words_n": 20,
            "supports_n": 30, "relations_n": 5}


def ok(number, work="regions", total_us=100_000, signatures=("0000000000000001",), reason=None,
       detect_us=10_000, **plan_fields):
    timings = {"capture": 3000, "plan": 100, "detect": detect_us, "recognize": 4000,
               "accessibility": 0, "fuse": 50, "track": 40, "commit": 20,
               "response_build": 60, "total": total_us}
    return {"schema": m.RECORD_SCHEMA, "observation_id": number, "outcome": "ok",
            "error_code": None, "snapshot_id": f"s{number}", "geometry_epoch": 1,
            "timings_us": timings,
            "plan": plan(work, reason, **plan_fields),
            "signatures": list(signatures)}


def failed(number, code="INFERENCE_FAILED"):
    return {"schema": m.RECORD_SCHEMA, "observation_id": number, "outcome": "error",
            "error_code": code, "snapshot_id": None, "geometry_epoch": None,
            "timings_us": None, "plan": None, "signatures": []}


def lines(*records):
    return "".join(f"{m.RECORD_PREFIX}{json.dumps(r)}\n" for r in records)


def sample(run, seq, label, number, record, case="incr1", pid=None):
    return {"experiment_id": "E", "run_id": run, "process_id": pid or f"{seq}-{label}",
            "sequence_id": seq, "observation_id": number, "variant": label, "case": case,
            "observation": {k: v for k, v in record.items() if k != "observation_id"}}


def process(run, seq, label, case="incr1", status="ok"):
    return {"experiment_id": "E", "run_id": run, "process_id": f"{seq}-{label}",
            "sequence_id": seq, "case": case, "variant": label, "order": [label],
            "status": status, "reason": None}


SIG_A = ("aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb", "cccccccccccccccc")


def run_demo_stderr(label, index):
    """The records of one demo run of the fake demo: first, second, warm, idle and an error
    (the demo stops at its first error). B0 is slower than B1 by 10 ms in every process."""
    base = 100_000 if label == "B0" else 90_000
    base += 10_000 * index
    b1_sig = SIG_A[:2] if label == "B1" else SIG_A
    return lines(
        ok(1, "full", total_us=base + 5_000, signatures=SIG_A, reason="no_committed_base"),
        ok(2, "regions", total_us=base, signatures=SIG_A),
        ok(3, "regions", total_us=base, signatures=b1_sig),
        ok(4, "idle", total_us=base, signatures=()),
        failed(5),
    )


class FakeApp:
    """Stands in for the test application the tool starts; it is never run."""
    pid = 4242

    def __init__(self, argv):
        self.argv = argv

    def terminate(self):
        return None

    def wait(self, timeout=None):
        return 0

    def kill(self):
        return None


class FakeDemo:
    """Stands in for `ommatix demo`: answers from `stderr_for(label, index)`."""

    def __init__(self, stderr_for):
        self.stderr_for = stderr_for
        self.calls = []

    def __call__(self, argv, **kwargs):
        session = argv[argv.index("--session-id") + 1]
        self.calls.append(session)
        match = SESSION.fullmatch(session)
        return SimpleNamespace(returncode=0, stdout="",
                               stderr=self.stderr_for(match["label"], int(match["p"])))


def measured(stderr_for=run_demo_stderr):
    """Patches every host call of the tool: the application, the demo, the foreground window,
    the clock, git and the run identifier. Returns the context managers and the fake demo."""
    demo = FakeDemo(stderr_for)
    return demo, [
        mock.patch.object(m.subprocess, "Popen", FakeApp),
        mock.patch.object(m.subprocess, "run", demo),
        mock.patch.object(m, "foreground_pid", lambda: FakeApp.pid),
        mock.patch.object(m.time, "sleep", lambda seconds: None),
        mock.patch.object(m, "git_commit", lambda: COMMIT),
    ]


class Files(unittest.TestCase):
    """Executables, configurations and a test application in a temporary folder."""

    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.dir = Path(self._dir.name)
        self.addCleanup(self._dir.cleanup)
        self.exe = self.write("ommatix.exe", b"one executable")
        self.other_exe = self.write("ommatix-other.exe", b"another executable")
        self.testapp = self.write("ommatix-testapp.exe", b"test application")
        self.b0 = self.write("B0.toml", "[engine]\nexperimental_full_frame = true\n")
        self.b1 = self.write("B1.toml", "[engine]\nexperimental_full_frame = false\n")

    def write(self, name, content):
        path = self.dir / name
        path.write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))
        return path

    def args(self, *extra, declare=True):
        """Command line of a B0/B1 measurement of one case, two processes, five observations.
        The one configuration difference (the experimental switch) is declared unless asked
        not to."""
        declared = ["--allow-difference", "engine.experimental_full_frame"] if declare else []
        return ["--experiment-id", "EXP", "--variant", f"B0={self.exe},{self.b0}",
                "--variant", f"B1={self.exe},{self.b1}", "--cases", "incr1",
                "--processes", "2", "--observations", "5", *declared, *extra]


# ---- records ----------------------------------------------------------------------------------

class Records(unittest.TestCase):
    def test_a_record_line_is_parsed_and_other_lines_are_ignored(self):
        stderr = "timings 1 (ms): capture 1.0\nsome note\n" + lines(ok(1))
        self.assertEqual([r["observation_id"] for r in m.parse_records(stderr)], [1])

    def test_an_error_record_keeps_its_code_and_carries_no_timings_or_plan(self):
        (record,) = m.parse_records(lines(failed(3, "CAPTURE_UNAVAILABLE")))
        self.assertEqual((record["outcome"], record["error_code"]),
                         ("error", "CAPTURE_UNAVAILABLE"))
        self.assertIsNone(record["timings_us"])
        self.assertIsNone(record["plan"])

    def test_malformed_json_on_a_record_line_is_a_hard_error(self):
        with self.assertRaisesRegex(m.RecordError, "malformed JSON"):
            m.parse_records(m.RECORD_PREFIX + "{not json\n")

    def test_an_unknown_record_schema_is_a_hard_error(self):
        record = ok(1)
        record["schema"] = "ommatix.observation/0"
        with self.assertRaisesRegex(m.RecordError, "unknown observation record schema"):
            m.parse_records(lines(record))

    def test_a_record_without_response_build_is_a_hard_error_not_a_zero(self):
        record = ok(1)
        del record["timings_us"]["response_build"]
        with self.assertRaisesRegex(m.RecordError, "response_build"):
            m.parse_records(lines(record))

    def test_a_null_stage_of_an_ok_record_is_refused_because_null_is_not_a_measurement(self):
        record = ok(1)
        record["timings_us"]["commit"] = None
        with self.assertRaisesRegex(m.RecordError, "commit"):
            m.parse_records(lines(record))

    def test_an_observation_recorded_twice_in_one_run_is_refused(self):
        with self.assertRaisesRegex(m.RecordError, "recorded twice"):
            m.parse_records(lines(ok(1), ok(1)))

    def test_a_full_refresh_names_a_reason_and_no_other_plan_does(self):
        bad = ok(1, "regions", reason="area_budget")
        with self.assertRaisesRegex(m.RecordError, "only a full refresh"):
            m.validate_record(bad)
        missing = ok(1, "full")
        with self.assertRaisesRegex(m.RecordError, "known full_refresh_reason"):
            m.validate_record(missing)

    def test_inference_count_and_input_shapes_are_null_together_and_agree(self):
        ok_count = ok(1, inferences=1, shapes=[[1, 3, 1280, 1280]])
        m.validate_record(ok_count)
        with self.assertRaisesRegex(m.RecordError, "together or not at all"):
            m.validate_record(ok(1, inferences=1, shapes=None))
        with self.assertRaisesRegex(m.RecordError, "does not match"):
            m.validate_record(ok(1, inferences=2, shapes=[[1, 3, 1280, 1280]]))


# ---- classes and statistics -------------------------------------------------------------------

class Classes(unittest.TestCase):
    def sample_of(self, record):
        return sample("R", "incr1-p000", "B0", record["observation_id"], record)

    def test_the_plan_decides_the_class_not_a_rounded_detection_time(self):
        regions_with_zero_detect = ok(3, "regions", detect_us=0)
        idle_with_detection_time = ok(4, "idle", detect_us=9_000)
        self.assertEqual(m.classify(self.sample_of(regions_with_zero_detect)), "warm_regions")
        self.assertEqual(m.classify(self.sample_of(idle_with_detection_time)), "idle")

    def test_the_first_and_second_observations_are_never_warm(self):
        self.assertEqual(m.classify(self.sample_of(ok(1, "full", reason="no_committed_base"))),
                         "first")
        self.assertEqual(m.classify(self.sample_of(ok(2, "regions"))), "second")

    def test_an_error_is_an_error_whatever_its_observation_number(self):
        self.assertEqual(m.classify(self.sample_of(failed(3))), "error")


class Nulls(unittest.TestCase):
    def test_null_inference_counts_are_not_counted_as_zero(self):
        warm = [{"plan": plan("regions", inferences=None, shapes=None), "signatures": []},
                {"plan": plan("regions", inferences=4, shapes=[[1, 3, 640, 640]] * 4),
                 "signatures": []},
                {"plan": plan("regions", inferences=2, shapes=[[1, 3, 640, 640]] * 2),
                 "signatures": []}]
        summary = m.plan_summary(warm, warm)
        self.assertEqual(summary["warm_inference_count"]["null"], 1)
        self.assertEqual(summary["warm_inference_count"]["n"], 2)
        self.assertEqual(summary["warm_inference_count"]["median"], 3.0)
        self.assertEqual(summary["warm_input_shapes"]["null"], 1)
        self.assertEqual(summary["warm_input_shapes"]["shapes"], {"1x3x640x640": 6})

    def test_nothing_measured_is_n_zero_and_never_a_zero_time(self):
        self.assertEqual(m.stage_table([])["total"], {"n": 0})
        self.assertEqual(m.common.summary([]), {"n": 0})

    def test_a_stage_table_names_commit_and_response_build(self):
        table = m.stage_table([ok(3)])
        self.assertEqual(set(table), set(STAGES))
        self.assertEqual(table["commit"]["median"], 0.02)
        self.assertEqual(table["response_build"]["median"], 0.06)


class Agreement(unittest.TestCase):
    def test_dice_of_two_multisets_counts_repeats_and_both_empty_is_not_measured(self):
        self.assertAlmostEqual(m.dice(Counter("aab"), Counter("abc")), 4 / 6)
        self.assertIsNone(m.dice(Counter(), Counter()))
        self.assertEqual(m.dice(Counter("a"), Counter()), 0.0)

    def test_agreement_excludes_observations_with_no_element_on_either_side(self):
        base = [sample("R", "incr1-p000", "B0", 1, ok(1, signatures=("a", "a", "b")), pid="x"),
                sample("R", "incr1-p000", "B0", 2, ok(2, signatures=()), pid="x")]
        treat = [sample("R", "incr1-p000", "B1", 1, ok(1, signatures=("a", "b", "c")), pid="y"),
                 sample("R", "incr1-p000", "B1", 2, ok(2, signatures=()), pid="y")]
        result = m.agreement_dice(base + treat, "B0", "B1", "incr1")
        self.assertEqual(result["paired_observations"], 1)
        self.assertEqual(result["not_measured_observations"], 1)
        self.assertEqual(result["mean_dice"], 0.6667)
        self.assertEqual(result["identical_observations"], 0)
        self.assertFalse(any("accuracy" in key for key in result),
                         "agreement is never named accuracy")


class Bootstrap(unittest.TestCase):
    def test_the_interval_resamples_processes_not_frames(self):
        # Process 1: 100 frames of 100 ms and one of 900 ms (median 100); the treatment is
        # 90 ms, so the process difference is -10. Process 2 differs by -20 the same way.
        run, samples = "R", []
        for seq, base_ms, treat_ms in (("incr1-p000", 100, 90), ("incr1-p001", 200, 180)):
            for number in range(3, 103):
                total = base_ms if number < 102 else 900
                samples.append(sample(run, seq, "B0", number, ok(number, total_us=total * 1000),
                                      pid=seq + "-B0"))
            for number in range(3, 103):
                total = treat_ms if number < 102 else 900
                samples.append(sample(run, seq, "B1", number, ok(number, total_us=total * 1000),
                                      pid=seq + "-B1"))
        result = m.paired_total(samples, "B0", "B1", "incr1")
        self.assertEqual(result["processes"], 2)
        self.assertEqual(sorted(p["delta_ms"] for p in result["per_process"]), [-20.0, -10.0])
        # Two processes give exactly three possible means (-10, -15, -20): the interval is over
        # processes. Resampling the 200 frames would give values between those.
        self.assertEqual(result["delta_ms"]["ci95_mean"], [-20.0, -10.0])
        self.assertEqual(result["delta_ms"]["mean"], -15.0)

    def test_the_same_inputs_give_the_same_intervals_every_time(self):
        samples = [sample("R", "incr1-p000", "B0", 3, ok(3, total_us=100_000), pid="a"),
                   sample("R", "incr1-p000", "B1", 3, ok(3, total_us=95_000), pid="b"),
                   sample("R", "incr1-p001", "B0", 3, ok(3, total_us=110_000), pid="c"),
                   sample("R", "incr1-p001", "B1", 3, ok(3, total_us=120_000), pid="d")]
        self.assertEqual(m.paired_total(samples, "B0", "B1", "incr1"),
                         m.paired_total(samples, "B0", "B1", "incr1"))

    def test_fewer_than_two_processes_give_no_interval(self):
        samples = [sample("R", "incr1-p000", "B0", 3, ok(3), pid="a"),
                   sample("R", "incr1-p000", "B1", 3, ok(3), pid="b")]
        result = m.paired_total(samples, "B0", "B1", "incr1")
        self.assertIsNone(result["delta_ms"]["ci95_mean"])
        self.assertEqual(result["processes"], 1)

    def test_a_process_without_warm_observations_is_excluded_and_counted(self):
        samples = [sample("R", "incr1-p000", "B0", 3, ok(3), pid="a"),
                   sample("R", "incr1-p000", "B1", 1, ok(1, "full", reason="no_committed_base"),
                          pid="b")]
        result = m.paired_total(samples, "B0", "B1", "incr1")
        self.assertEqual((result["processes"], result["excluded_processes"]), (0, 1))


# ---- preflight --------------------------------------------------------------------------------

class Preflight(Files):
    def test_a_configuration_difference_must_be_declared_before_anything_starts(self):
        demo, patches = measured()
        with contextlib.ExitStack() as stack, contextlib.redirect_stderr(io.StringIO()) as err:
            for patch in patches:
                stack.enter_context(patch)
            launched = stack.enter_context(mock.patch.object(m.subprocess, "Popen"))
            stack.enter_context(mock.patch.object(m, "TESTAPP", self.testapp))
            code = m.main(self.args("--out", str(self.dir / "out.json"), declare=False))
        self.assertEqual(code, 2)
        self.assertIn("engine.experimental_full_frame", err.getvalue())
        self.assertEqual(demo.calls, [], "no demo may run when a difference is undeclared")
        self.assertFalse(launched.called, "the test application must not start either")

    def test_a_declared_configuration_difference_is_accepted(self):
        result = m.preflight(
            [m.Variant("B0", str(self.exe), str(self.b0)),
             m.Variant("B1", str(self.exe), str(self.b1))],
            {"engine.experimental_full_frame"}, self.testapp)
        self.assertEqual([d["key"] for d in result["differences_found"]],
                         ["engine.experimental_full_frame"])

    def test_a_different_executable_needs_the_binary_declared(self):
        variants = [m.Variant("before", str(self.exe), str(self.b1)),
                    m.Variant("after", str(self.other_exe), str(self.b1))]
        with self.assertRaisesRegex(m.Refused, "binary"):
            m.preflight(variants, set(), self.testapp)
        result = m.preflight(variants, {"binary"}, self.testapp)
        self.assertEqual(result["differences_found"][0]["key"], "binary")

    def test_a_configuration_that_could_deliver_input_refuses_the_run(self):
        live = self.write("live.toml", "[safety]\nactuation_enabled = true\n")
        with self.assertRaisesRegex(m.Refused, "could deliver input"):
            m.preflight([m.Variant("B1", str(self.exe), str(live))], set(), self.testapp)

    def test_the_test_application_must_be_built(self):
        with self.assertRaisesRegex(m.Refused, "not built"):
            m.preflight([m.Variant("B1", str(self.exe), str(self.b1))], set(),
                        self.dir / "missing.exe")


# ---- running the collector against fakes ------------------------------------------------------

class Collector(Files):
    def run_main(self, *argv, runs=("20261015T100000Z",), stderr_for=run_demo_stderr):
        demo, patches = measured(stderr_for)
        with contextlib.ExitStack() as stack, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            for patch in patches:
                stack.enter_context(patch)
            stack.enter_context(mock.patch.object(m, "TESTAPP", self.testapp))
            stack.enter_context(mock.patch.object(m, "new_run_id", side_effect=list(runs)))
            code = m.main(list(argv))
        return code, demo

    def read(self, name):
        return json.loads((self.dir / name).read_text(encoding="utf-8"))

    def test_an_end_to_end_run_keeps_identities_errors_and_idle_observations(self):
        out = self.dir / "chunk1.json"
        code, demo = self.run_main(*self.args("--paired", "B0:B1", "--out", str(out)))
        self.assertEqual(code, 0)
        self.assertEqual(len(demo.calls), 4)  # two variants x two processes
        report = self.read("chunk1.json")
        self.assertEqual(report["schema"], m.REPORT_SCHEMA)
        self.assertEqual(report["variant_order"], ["B0", "B1"])
        self.assertEqual(report["allowed_differences"], ["engine.experimental_full_frame"])
        self.assertEqual([d["key"] for d in report["differences_found"]],
                         ["engine.experimental_full_frame"])
        self.assertEqual(report["run_ids"], ["20261015T100000Z"])
        self.assertEqual(report["code_commit"], COMMIT)
        self.assertEqual([p["status"] for p in report["processes"]], ["ok"] * 4)
        self.assertEqual(len(report["samples"]), 4 * 5)
        first = report["samples"][0]
        self.assertEqual({k: first[k] for k in ("experiment_id", "run_id", "sequence_id",
                                                 "observation_id", "variant")},
                         {"experiment_id": "EXP", "run_id": "20261015T100000Z",
                          "sequence_id": "incr1-p000", "observation_id": 1, "variant": "B0"})
        b0 = report["summary"]["cases"]["incr1"]["B0"]
        self.assertEqual(b0["classes"]["error"], 2)
        self.assertEqual(b0["classes"]["error_codes"], {"INFERENCE_FAILED": 2})
        self.assertEqual(b0["classes"]["idle"], 2)
        self.assertEqual(b0["classes"]["warm_regions"], 2)
        self.assertEqual(b0["stages_ms"]["regions"]["total"]["n"], 2)
        self.assertEqual(b0["stages_ms"]["full"]["total"], {"n": 0})
        self.assertEqual(b0["plan"]["work"], {"full": 2, "regions": 4, "idle": 2})

    def test_the_paired_section_reports_the_difference_and_the_agreement(self):
        out = self.dir / "chunk1.json"
        self.run_main(*self.args("--paired", "B0:B1", "--out", str(out)))
        pair = self.read("chunk1.json")["summary"]["paired"]["incr1"]["B0:B1"]
        self.assertEqual(pair["total"]["processes"], 2)
        self.assertEqual(pair["total"]["delta_ms"]["mean"], -10.0)
        dices = pair["agreement_dice"]
        self.assertEqual((dices["paired_observations"], dices["not_measured_observations"]),
                         (6, 2))
        self.assertEqual(dices["identical_observations"], 4)
        self.assertEqual(dices["mean_dice"], 0.9333)
        self.assertFalse(any("accuracy" in key for key in dices),
                         "agreement is never named accuracy")

    def test_a_malformed_record_fails_its_process_and_keeps_no_sample_of_it(self):
        def stderr_for(label, index):
            if (label, index) == ("B1", 1):
                return m.RECORD_PREFIX + "{broken\n"
            return run_demo_stderr(label, index)

        out = self.dir / "chunk1.json"
        code, _ = self.run_main(*self.args("--out", str(out)), stderr_for=stderr_for)
        self.assertEqual(code, 0)
        report = self.read("chunk1.json")
        failed_process = [p for p in report["processes"] if p["status"] == "failed"]
        self.assertEqual([p["process_id"] for p in failed_process], ["incr1-B1-p001"])
        self.assertIn("malformed JSON", failed_process[0]["reason"])
        self.assertFalse(any(s["process_id"] == "incr1-B1-p001" for s in report["samples"]))

    def test_a_process_whose_foreground_changed_is_discarded_without_samples(self):
        app_pid = FakeApp.pid
        calls = iter([app_pid, app_pid, app_pid, 999, app_pid, app_pid] + [app_pid] * 20)

        def foreground():
            return next(calls)

        demo, patches = measured()
        with contextlib.ExitStack() as stack, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            for patch in patches:
                stack.enter_context(patch)
            stack.enter_context(mock.patch.object(m, "foreground_pid", foreground))
            stack.enter_context(mock.patch.object(m, "TESTAPP", self.testapp))
            stack.enter_context(mock.patch.object(m, "new_run_id", return_value="R1"))
            out = self.dir / "chunk1.json"
            self.assertEqual(m.main(self.args("--out", str(out))), 0)
        report = self.read("chunk1.json")
        statuses = Counter(p["status"] for p in report["processes"])
        self.assertEqual(statuses["discarded"], 1)
        self.assertIn("foreground", next(p for p in report["processes"]
                                         if p["status"] == "discarded")["reason"])
        self.assertEqual(len(report["samples"]), 3 * 5)

    def test_combining_chunks_of_two_runs_joins_them_and_recomputes_every_summary(self):
        first, second = self.dir / "a.json", self.dir / "b.json"
        self.run_main(*self.args("--out", str(first)), runs=("20261015T100000Z",))
        self.run_main(*self.args("--out", str(second)), runs=("20261015T110000Z",))
        joined = self.dir / "all.json"
        code, _ = self.run_main("--combine", str(first), str(second), "--paired", "B0:B1",
                                "--out", str(joined))
        self.assertEqual(code, 0)
        report = self.read("all.json")
        self.assertEqual(report["run_ids"], ["20261015T100000Z", "20261015T110000Z"])
        self.assertEqual(len(report["processes"]), 8)
        self.assertEqual(len(report["samples"]), 40)
        self.assertEqual(report["summary"]["cases"]["incr1"]["B0"]["classes"]["warm_regions"], 4)
        self.assertEqual(report["summary"]["paired"]["incr1"]["B0:B1"]["total"]["processes"], 4)

    def test_combining_the_same_chunk_twice_is_refused(self):
        chunk = self.dir / "a.json"
        self.run_main(*self.args("--out", str(chunk)))
        code, _ = self.run_main("--combine", str(chunk), str(chunk), "--out",
                                str(self.dir / "all.json"))
        self.assertEqual(code, 2)
        self.assertFalse((self.dir / "all.json").exists())

    def test_a_run_that_appears_in_two_chunks_is_refused_as_an_overlap(self):
        chunk = self.dir / "a.json"
        self.run_main(*self.args("--out", str(chunk)))
        copy_path = self.dir / "b.json"
        report = self.read("a.json")
        report["samples"][0]["observation"]["timings_us"]["total"] += 1
        copy_path.write_text(json.dumps(report), encoding="utf-8")
        code, _ = self.run_main("--combine", str(chunk), str(copy_path), "--out",
                                str(self.dir / "all.json"))
        self.assertEqual(code, 2)

    def test_a_duplicate_observation_inside_one_report_is_refused(self):
        chunk = self.dir / "a.json"
        self.run_main(*self.args("--out", str(chunk)))
        report = self.read("a.json")
        report["samples"].append(copy.deepcopy(report["samples"][0]))
        (self.dir / "dup.json").write_text(json.dumps(report), encoding="utf-8")
        code, _ = self.run_main("--combine", str(self.dir / "dup.json"), "--out",
                                str(self.dir / "all.json"))
        self.assertEqual(code, 2)

    def test_combining_refuses_an_unknown_report_schema(self):
        chunk = self.dir / "a.json"
        self.run_main(*self.args("--out", str(chunk)))
        report = self.read("a.json")
        report["schema"] = "ommatix.perf/1"
        (self.dir / "old.json").write_text(json.dumps(report), encoding="utf-8")
        code, _ = self.run_main("--combine", str(self.dir / "old.json"), "--out",
                                str(self.dir / "all.json"))
        self.assertEqual(code, 2)

    def test_combining_refuses_a_different_experiment_or_code_commit_or_binary(self):
        base = self.dir / "a.json"
        self.run_main(*self.args("--out", str(base)))
        for field, value in (("experiment_id", "OTHER"), ("code_commit", "b" * 40)):
            report = self.read("a.json")
            report[field] = value
            path = self.dir / f"{field}.json"
            path.write_text(json.dumps(report), encoding="utf-8")
            code, _ = self.run_main("--combine", str(base), str(path), "--out",
                                    str(self.dir / "all.json"))
            self.assertEqual(code, 2, field)
        report = self.read("a.json")
        report["variants"]["B1"]["exe_sha256"] = "f" * 64
        path = self.dir / "binary.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        code, _ = self.run_main("--combine", str(base), str(path), "--out",
                                str(self.dir / "all.json"))
        self.assertEqual(code, 2)


    def test_a_record_of_an_unknown_schema_fails_its_process(self):
        def stderr_for(label, index):
            if (label, index) == ("B0", 0):
                record = ok(1)
                record["schema"] = "ommatix.observation/0"
                return lines(record)
            return run_demo_stderr(label, index)

        out = self.dir / "chunk1.json"
        self.run_main(*self.args("--out", str(out)), stderr_for=stderr_for)
        failed_process = [p for p in self.read("chunk1.json")["processes"]
                          if p["status"] == "failed"]
        self.assertEqual([p["process_id"] for p in failed_process], ["incr1-B0-p000"])
        self.assertIn("unknown observation record schema", failed_process[0]["reason"])

    def test_a_pair_naming_a_variant_that_was_not_measured_is_refused(self):
        code, demo = self.run_main(*self.args("--paired", "B0:B9",
                                              "--out", str(self.dir / "out.json")))
        self.assertEqual(code, 2)
        self.assertEqual(demo.calls, [])


if __name__ == "__main__":
    unittest.main()
