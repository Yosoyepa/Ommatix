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

    # ---- ADR-0035: the amended group beside the ADR-0032 one (paired M1 repeat) ----

    def occluder(self, **changes):
        record = {"group_tiles": 0, "group_window_count": 2, "group_frame_fraction": 0.2,
                  "group_covers_whole_frame": False, "opacity_attested": True,
                  "scoped": True, "fallback": None}
        record.update(changes)
        return record

    def paired_report(self, records=None):
        """Two pairs: one scoped with a reduction, one that falls back (no opacity)."""
        pairs = []
        for record in records or [self.occluder(),
                                  self.occluder(opacity_attested=False, scoped=False,
                                                fallback="no_opacity")]:
            pair = dict(self.report()["pairs"][0], occluder_group=record)
            pairs.append(pair)
        report = self.report()
        report["pairs"] = pairs
        report["outside_pairs"] = len(pairs)
        report["occluder_scoped_pairs"] = sum(p["occluder_group"]["scoped"] for p in pairs)
        report["occluder_outside_pairs"] = sum(
            p["occluder_group"]["scoped"] and p["total_tiles"] > p["occluder_group"]["group_tiles"]
            for p in pairs)
        return report

    def pair_args(self, pairs=2):
        args = self.args(Path(tempfile.gettempdir()))
        args.pairs = pairs
        return args

    def test_the_old_report_still_parses_and_the_new_one_carries_the_amended_group(self):
        args = self.pair_args(1)
        probe.validate_report(self.report(), args)
        args = self.pair_args(2)
        report = probe.validate_report(self.paired_report(), args)
        self.assertEqual(report["occluder_scoped_pairs"], 1)
        # The old fields are unchanged and still validated in the new format.
        report = self.paired_report()
        report["pairs"][0]["group_tiles"] = 99
        with self.assertRaises(ValueError):
            probe.validate_report(report, args)

    def test_a_partial_or_inconsistent_amended_group_is_refused(self):
        args = self.pair_args(2)
        bad_records = [
            {"scoped": False},                       # scoped must equal 'no fallback'
            {"fallback": "bogus", "scoped": False},   # closed set of reasons
            {"group_tiles": -1},
            {"group_tiles": True},
            {"group_window_count": 0},
            {"group_frame_fraction": 1.5},
            {"group_covers_whole_frame": True},       # fraction 0.2
            {"opacity_attested": "yes"},
            {"fallback": "no_opacity", "scoped": False},   # opacity is attested here
            {"fallback": "corner", "scoped": False, "opacity_attested": False},
            {"group_tiles": None},                    # counts are all-or-none
            {"fallback": "unattested_rect", "scoped": False},  # that reason has no counts
            {"scoped": True, "opacity_attested": False},
            {"title": "private screen text"},
        ]
        for change in bad_records:
            report = self.paired_report()
            report["pairs"][0]["occluder_group"].update(change)
            with self.assertRaises(ValueError, msg=str(change)):
                probe.validate_report(report, args)
        # Missing on one pair only, or a summary count that disagrees.
        report = self.paired_report()
        del report["pairs"][1]["occluder_group"]
        with self.assertRaises(ValueError):
            probe.validate_report(report, args)
        for key, value in [("occluder_scoped_pairs", 2), ("occluder_outside_pairs", 0)]:
            report = self.paired_report()
            report[key] = value
            with self.assertRaises(ValueError, msg=key):
                probe.validate_report(report, args)
        report = self.paired_report()
        del report["occluder_outside_pairs"]
        with self.assertRaises(ValueError):
            probe.validate_report(report, args)

    def test_a_group_that_is_not_attested_reports_no_counts_and_a_reason(self):
        args = self.pair_args(2)
        records = [self.occluder(),
                   self.occluder(group_tiles=None, group_window_count=None,
                                 group_frame_fraction=None, group_covers_whole_frame=None,
                                 opacity_attested=False, scoped=False,
                                 fallback="unattested_rect")]
        probe.validate_report(self.paired_report(records), args)

    def test_the_summary_is_descriptive_for_both_groups_and_old_reports(self):
        args = self.pair_args(2)
        summary = probe.summarise(probe.validate_report(self.paired_report(), args))
        self.assertEqual(summary["pairs"], 2)
        self.assertEqual(summary["adr_0032"]["outside_pairs"], 2)
        self.assertEqual(summary["adr_0035"]["scoped_pairs"], 1)
        self.assertEqual(summary["adr_0035"]["fallbacks"], {"no_opacity": 1})
        self.assertEqual(summary["adr_0035"]["opacity_attested_pairs"], 1)
        # No threshold, verdict or criterion anywhere in it.
        text = json.dumps(summary)
        for word in ("criterion", "verdict", "pass", "threshold"):
            self.assertNotIn(word, text)
        old = probe.summarise(self.report())
        self.assertEqual(old["pairs"], 1)
        self.assertNotIn("adr_0035", old)

    def test_a_paired_report_is_saved_with_its_summary_and_an_old_one_as_it_was(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = self.args(Path(tmp), descriptive=True)
            args.pairs = 2
            report = self.paired_report()
            report.pop("min_outside_pairs")
            report.pop("criterion_met")
            report["descriptive"] = True
            child = SimpleNamespace(returncode=0, stdout="", stderr=json.dumps(report))
            self.assertEqual(probe.run(args, lambda *a, **kw: child), 0)
            saved = json.loads(args.out.read_text())
            self.assertEqual(saved["summary"]["adr_0035"]["scoped_pairs"], 1)
            del saved["summary"]
            self.assertEqual(saved, report)

    def test_repository_output_is_refused_before_launch(self):
        args = self.args(HERE)

        def forbidden(*a, **k):
            self.fail("the probe must not start")

        with self.assertRaises(ValueError):
            probe.run(args, forbidden)


if __name__ == "__main__":
    unittest.main()
