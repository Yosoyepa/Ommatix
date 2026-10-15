"""Tests of the pure parts of perf_common.py. Run: python -m unittest discover -s tools/perf"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as c  # noqa: E402


class Statistics(unittest.TestCase):
    def test_percentiles_use_the_nearest_rank_like_the_s0_report(self):
        self.assertEqual(c.percentile(list(range(1, 101)), 0.95), 95)
        self.assertEqual(c.percentile([7], 0.99), 7)

    def test_summary_of_nothing_is_just_n_zero(self):
        self.assertEqual(c.summary([]), {"n": 0})
        s = c.summary([1.0, 2.0, 3.0, 100.0])
        self.assertEqual((s["n"], s["median"], s["max"]), (4, 2.5, 100.0))

    def test_stage_timings_are_converted_from_microseconds(self):
        meta = {c.TIMINGS_KEY: {"detect_us": 80_000, "total_us": 90_500}}
        self.assertEqual(c.stage_ms(meta), {"detect": 80.0, "total": 90.5})
        self.assertEqual(c.stage_ms({}), {})


class InputFreeConfiguration(unittest.TestCase):
    def test_the_shipped_operator_configurations_are_input_free(self):
        paths = sorted((c.REPO / "tools" / "s0").glob("*.toml"))
        self.assertNotEqual(paths, [])
        for path in paths:
            self.assertIsNone(c.config_problem(path.read_text(encoding="utf-8")), path.name)

    def test_a_configuration_that_could_deliver_input_is_refused(self):
        live = '[safety]\nactuation_enabled = true\nallowed_applications = ["notepad.exe"]\n'
        self.assertIn("actuation_enabled", c.config_problem(live))
        allowed = '[safety]\nallowed_applications = ["notepad.exe"]\n'
        self.assertIn("allowed_applications", c.config_problem(allowed))
        self.assertIn("mode", c.config_problem('[engine]\nmode = "native"\n'))
        self.assertIn("TOML", c.config_problem("not [toml"))
        self.assertIsNone(c.config_problem(""))


class Files(unittest.TestCase):
    def test_a_report_is_written_whole_and_replaces_the_previous_one(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d, "r.json")
            c.write_json(path, {"a": 1})
            c.write_json(path, {"a": 2})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")), {"a": 2})
            self.assertEqual([p.name for p in Path(d).iterdir()], ["r.json"])


class ReadOnlyEngine(unittest.TestCase):
    def test_the_engine_wrapper_can_only_call_the_two_read_only_tools(self):
        engine = c.Engine.__new__(c.Engine)  # no process: the check comes first
        for tool in ("execute_action", "anything_else"):
            with self.assertRaises(ValueError):
                engine.call(tool, {})
        self.assertEqual(c.Engine.ALLOWED, ("observe_screen", "get_engine_metrics"))


if __name__ == "__main__":
    unittest.main()
