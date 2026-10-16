"""M1 driver doubles: never start a process or read/capture the desktop."""

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("m1_probe", HERE / "freshness_probe.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class FreshnessProbeTest(unittest.TestCase):
    def args(self, root, descriptive=False):
        return probe.parse_args(["--exe", "ommatix.exe", "--config", "operator.toml",
            "--out", str(root / "m1.json"), "--pairs", "1",
            "--window-class", "TestClass", "--target-x", "10", "--target-y", "10",
            "--target-width", "4", "--target-height", "4"] +
            (["--descriptive"] if descriptive else ["--min-outside-pairs", "1"]))

    def test_descriptive_mode_neither_supplies_nor_accepts_a_pass_threshold(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = self.args(Path(tmp), descriptive=True)
            self.assertIsNone(args.min_outside_pairs)
            self.assertIn("--descriptive", probe.probe_argv(args))
            self.assertNotIn("--min-outside-pairs", probe.probe_argv(args))
            report = self.report()
            report.pop("min_outside_pairs")
            report.pop("criterion_met")
            report["descriptive"] = True
            child = SimpleNamespace(returncode=0, stdout="", stderr=json.dumps(report))
            self.assertEqual(probe.run(args, lambda *a, **kw: child), 0)
            self.assertNotIn("criterion_met", json.loads(args.out.read_text()))
            report["min_outside_pairs"] = 0
            report["criterion_met"] = True
            with self.assertRaises(ValueError):
                probe.validate_report(report, args)

    def report(self):
        return {"pairs": [{"capture_gap_ms": 152.2, "total_tiles": 3, "group_tiles": 0,
                "touches_target": False, "group_window_count": 2, "group_frame_fraction": 0.4,
                "group_covers_whole_frame": False}], "interval_ms": 150, "margin_px": 0,
                "outside_pairs": 1, "min_outside_pairs": 1, "criterion_met": True, "descriptive": False}

    def test_numeric_report_is_saved_and_the_command_can_only_select_the_read_only_probe(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = self.args(Path(tmp))
            seen = []

            def launch(argv, **kwargs):
                seen.append(argv)
                return SimpleNamespace(returncode=0, stdout="", stderr=json.dumps(self.report()))

            self.assertEqual(probe.run(args, launch), 0)
            self.assertEqual(json.loads(args.out.read_text()), self.report())
            self.assertEqual(seen[0][1], "freshness-probe")
            self.assertNotIn("serve", seen[0])
            self.assertNotIn("--input", seen[0])
            with self.assertRaises(ValueError):
                probe.run(args, launch)
            self.assertEqual(len(seen), 1, "existing files refuse before any process starts")

    def test_refusal_malformed_output_or_screen_data_never_get_persisted(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = self.args(Path(tmp))
            secret = self.report()
            secret["pairs"][0]["label"] = "private screen text"
            for child in [SimpleNamespace(returncode=1, stdout="", stderr="private error"),
                          SimpleNamespace(returncode=0, stdout="private", stderr=""),
                          SimpleNamespace(returncode=0, stdout="", stderr="invalid json"),
                          SimpleNamespace(returncode=0, stdout="", stderr=json.dumps(secret))]:
                with self.assertRaises(ValueError):
                    probe.run(args, lambda *a, **k: child)
                self.assertFalse(args.out.exists())

    def test_invalid_types_missing_evidence_and_inconsistent_criteria_are_refused(self):
        args = self.args(Path(tempfile.gettempdir()))
        changes = [("total_tiles", True), ("group_tiles", 4), ("group_window_count", 0),
                   ("touches_target", "false"), ("capture_gap_ms", float("nan")),
                   ("group_frame_fraction", 1.5), ("group_covers_whole_frame", True)]
        for key, value in changes:
            report = copy.deepcopy(self.report())
            report["pairs"][0][key] = value
            with self.assertRaises(ValueError):
                probe.validate_report(report, args)
        for key, value in [("pairs", []), ("criterion_met", False), ("outside_pairs", 0),
                           ("min_outside_pairs", 0), ("interval_ms", 151)]:
            report = self.report()
            report[key] = value
            with self.assertRaises(ValueError):
                probe.validate_report(report, args)

    def test_repository_output_is_refused_before_launch(self):
        args = self.args(HERE)

        def forbidden(*a, **k):
            self.fail("the probe must not start")

        with self.assertRaises(ValueError):
            probe.run(args, forbidden)


if __name__ == "__main__":
    unittest.main()
