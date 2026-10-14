"""Tests of the pure parts of measure_latency.py (Windows only: the module reads the
foreground window through user32). Run: python -m unittest tools/perf/test_measure_latency.py
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

if sys.platform != "win32":
    raise unittest.SkipTest("measure_latency.py needs Windows")

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure_latency as m  # noqa: E402

STDERR = (
    "timings 1 (ms): capture 5.000 plan 0.040 detect 80.000 recognize 40.000 "
    "accessibility 70.000 fuse 0.100 track 0.050 total 90.000\n"
    "timings 3 (ms): capture 3.000 plan 2.000 detect 0.000 recognize 0.000 "
    "accessibility 0.000 fuse 0.060 track 0.400 total 16.000\n"
)
STDOUT = "observation 1: snapshot s1 | frame 1 | 76 elements\nobservation 3: snapshot s3 | frame 3 | 75 elements\n"


class Parsing(unittest.TestCase):
    def test_stage_timings_and_element_counts_are_read_by_observation(self):
        by_obs = m.parse(STDERR, STDOUT)
        self.assertEqual(sorted(by_obs), [1, 3])
        self.assertEqual(by_obs[1]["detect"], 80.0)
        self.assertEqual(by_obs[1]["elements"], 76)
        self.assertEqual(by_obs[3]["total"], 16.0)

    def test_percentiles_use_the_nearest_rank(self):
        values = list(range(1, 101))
        self.assertEqual(m.percentile(values, 0.95), 95)
        self.assertEqual(m.percentile([7], 0.95), 7)
        self.assertEqual(m.summary([1.0, 2.0, 3.0])["median"], 2.0)
        self.assertEqual(m.summary([]), {"n": 0})


class Combining(unittest.TestCase):
    def chunk(self, path, detect):
        raw = {"incr1": {"after": {
            "first": [{"total": 100.0}],
            "incremental": [{"detect": detect, "total": detect + 10, "elements": 75}],
            "idle": 1, "discarded": 0}}}
        report = m.report_from(raw, {"processes": 1})
        Path(path).write_text(json.dumps(report), encoding="utf-8")

    def test_chunks_are_joined_by_raw_samples_not_by_their_summaries(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d, "a.json"), Path(d, "b.json")
            self.chunk(a, 70.0)
            self.chunk(b, 90.0)
            joined = m.combine([str(a), str(b)])
        stage = joined["cases"]["incr1"]["after"]["stages"]
        self.assertEqual(stage["detect"]["n"], 2)
        self.assertEqual(stage["detect"]["median"], 80.0)
        self.assertEqual(joined["cases"]["incr1"]["after"]["idle_observations"], 2)
        self.assertEqual(joined["cases"]["first observation"]["after"]["stages"]["total"]["n"], 2)


if __name__ == "__main__":
    unittest.main()
