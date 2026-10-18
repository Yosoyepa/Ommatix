#!/usr/bin/env python3
"""M1 operator driver: persist only validated numeric counts from the read-only probe.

The operator chooses the foreground window by hand. No engine, input request, screenshot,
window activation or config modification exists in this driver. See S4_SESIONES_PENDIENTES D.

A report may also carry, per pair, the amended group of ADR-0035 under `occluder_group` (the
ADR-0032 fields are unchanged). Old reports without it still parse. Everything here is
descriptive: there is no threshold and no verdict.
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
from pathlib import Path


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--pairs", type=int, required=True)
    p.add_argument("--interval-ms", type=int, default=150)
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--descriptive", action="store_true", help="M1: counts only, no pass criterion")
    mode.add_argument("--min-outside-pairs", type=int, help="optional owner-set pass criterion; no default")
    p.add_argument("--window-class", required=True)
    p.add_argument("--output", type=int, default=0)
    p.add_argument("--target-x", type=int, required=True)
    p.add_argument("--target-y", type=int, required=True)
    p.add_argument("--target-width", type=int, required=True)
    p.add_argument("--target-height", type=int, required=True)
    args = p.parse_args(argv)
    if not (1 <= args.pairs <= 100_000 and 1 <= args.interval_ms <= 60_000
            and (args.min_outside_pairs is None or 0 <= args.min_outside_pairs <= args.pairs)
            and args.output >= 0
            and 0 <= args.target_x <= 2**32 - 1 and 0 <= args.target_y <= 2**32 - 1
            and 1 <= args.target_width <= 2**32 - 1 and 1 <= args.target_height <= 2**32 - 1):
        p.error("invalid pair count, interval, criterion, output or target box")
    return args


def probe_argv(args):
    """A single fixed read-only subcommand; there is no live-server/input alternative."""
    command = [str(args.exe), "freshness-probe", "--config", str(args.config)]
    for key in ("pairs", "interval_ms", "window_class", "output",
                "target_x", "target_y", "target_width", "target_height"):
        command.extend(["--" + key.replace("_", "-"), str(getattr(args, key))])
    if args.descriptive:
        command.append("--descriptive")
    else:
        command.extend(["--min-outside-pairs", str(args.min_outside_pairs)])
    return command


# Why the amended scope would compare the whole frame (closed set; no free text).
OCCLUDER_FALLBACKS = ("walk_failed", "unattested_rect", "reads_differ",
                      "box_outside_target", "no_opacity", "corner")
# These two leave no group to count.
OCCLUDER_NO_COUNTS = ("walk_failed", "unattested_rect")
OCCLUDER_FIELDS = {"group_tiles", "group_window_count", "group_frame_fraction",
                   "group_covers_whole_frame", "opacity_attested", "scoped", "fallback"}


def validate_occluder(record, total_tiles):
    """One pair's ADR-0035 group: numbers, flags and one enum string, mutually consistent."""
    if not isinstance(record, dict) or set(record) != OCCLUDER_FIELDS:
        raise ValueError("invalid occluder fields")
    tiles, windows = record["group_tiles"], record["group_window_count"]
    fraction, covers = record["group_frame_fraction"], record["group_covers_whole_frame"]
    fallback = record["fallback"]
    for key in ("opacity_attested", "scoped"):
        if type(record[key]) is not bool:
            raise ValueError("invalid occluder flags")
    if fallback is not None and fallback not in OCCLUDER_FALLBACKS:
        raise ValueError("invalid occluder fallback")
    if record["scoped"] != (fallback is None):
        raise ValueError("inconsistent occluder scope")
    counts = (tiles, windows, fraction, covers)
    if fallback in OCCLUDER_NO_COUNTS:
        if any(v is not None for v in counts):
            raise ValueError("inconsistent occluder counts")
    else:
        if (type(tiles) is not int or type(windows) is not int or tiles < 0 or windows < 1
                or tiles > total_tiles):
            raise ValueError("invalid occluder counts")
        if type(fraction) not in (int, float) or not math.isfinite(fraction) or not 0 <= fraction <= 1:
            raise ValueError("invalid occluder coverage")
        if type(covers) is not bool or covers != (fraction == 1):
            raise ValueError("inconsistent occluder coverage")
    if record["scoped"] and not record["opacity_attested"]:
        raise ValueError("inconsistent occluder opacity")
    if fallback == "no_opacity" and record["opacity_attested"]:
        raise ValueError("inconsistent occluder opacity")
    if fallback in ("corner", "box_outside_target") and not record["opacity_attested"]:
        raise ValueError("inconsistent occluder opacity")


def validate_report(report, args):
    """Reject unexpected keys/types rather than persisting raw child stderr."""
    keys = {"pairs", "interval_ms", "margin_px", "outside_pairs", "descriptive"}
    if not args.descriptive:
        keys |= {"min_outside_pairs", "criterion_met"}
    # A paired report (ADR-0035) adds two summary counts and one record per pair; all or none.
    paired = isinstance(report, dict) and "occluder_scoped_pairs" in report
    if paired:
        keys |= {"occluder_scoped_pairs", "occluder_outside_pairs"}
    if not isinstance(report, dict) or set(report) != keys:
        raise ValueError("invalid measurement report")
    if type(report["descriptive"]) is not bool or report["descriptive"] != args.descriptive:
        raise ValueError("invalid measurement mode")
    counts = ("total_tiles", "group_tiles", "group_window_count")
    fields = {*counts, "touches_target", "group_frame_fraction", "group_covers_whole_frame", "capture_gap_ms"}
    if not isinstance(report["pairs"], list) or len(report["pairs"]) != args.pairs:
        raise ValueError("incomplete measurement")
    for pair in report["pairs"]:
        if not isinstance(pair, dict) or set(pair) != (fields | ({"occluder_group"} if paired else set())):
            raise ValueError("invalid pair fields")
        if any(type(pair[k]) is not int or pair[k] < 0 for k in counts):
            raise ValueError("invalid counts")
        if pair["group_tiles"] > pair["total_tiles"] or pair["group_window_count"] == 0:
            raise ValueError("inconsistent group counts")
        for key in ("group_frame_fraction", "capture_gap_ms"):
            if type(pair[key]) not in (int, float) or not math.isfinite(pair[key]) or pair[key] < 0:
                raise ValueError("invalid measurement")
        if not 0 <= pair["group_frame_fraction"] <= 1:
            raise ValueError("invalid coverage")
        for key in ("touches_target", "group_covers_whole_frame"):
            if type(pair[key]) is not bool:
                raise ValueError("invalid flags")
        if pair["group_covers_whole_frame"] != (pair["group_frame_fraction"] == 1):
            raise ValueError("inconsistent coverage")
    outside = sum(p["total_tiles"] > p["group_tiles"] for p in report["pairs"])
    for key in ("interval_ms", "margin_px", "outside_pairs"):
        if type(report[key]) is not int or report[key] < 0:
            raise ValueError("invalid summary count")
    if report["interval_ms"] != args.interval_ms or report["outside_pairs"] != outside:
        raise ValueError("inconsistent summary")
    if paired:
        for pair in report["pairs"]:
            validate_occluder(pair["occluder_group"], pair["total_tiles"])
        scoped = sum(p["occluder_group"]["scoped"] for p in report["pairs"])
        reduced = sum(p["occluder_group"]["scoped"]
                      and p["total_tiles"] > p["occluder_group"]["group_tiles"]
                      for p in report["pairs"])
        for key, expected in (("occluder_scoped_pairs", scoped), ("occluder_outside_pairs", reduced)):
            if type(report[key]) is not int or report[key] != expected:
                raise ValueError("inconsistent occluder summary")
    if not args.descriptive:
        if (type(report["min_outside_pairs"]) is not int
                or report["min_outside_pairs"] != args.min_outside_pairs
                or type(report["criterion_met"]) is not bool
                or report["criterion_met"] != (outside >= args.min_outside_pairs)):
            raise ValueError("inconsistent criterion")
    return report


def summarise(report):
    """Descriptive counts of a validated report, for the old group and, when present, the
    amended one. No threshold, criterion or verdict; numbers and enum names only."""
    pairs = report["pairs"]
    n = len(pairs)

    def mean(values):
        return round(sum(values) / len(values), 6) if values else None

    summary = {"pairs": n, "adr_0032": {
        "outside_pairs": report["outside_pairs"],
        "covers_whole_frame_pairs": sum(p["group_covers_whole_frame"] for p in pairs),
        "mean_group_frame_fraction": mean([p["group_frame_fraction"] for p in pairs]),
        "mean_group_window_count": mean([p["group_window_count"] for p in pairs])}}
    if "occluder_scoped_pairs" in report:
        groups = [p["occluder_group"] for p in pairs]
        counted = [g for g in groups if g["group_window_count"] is not None]
        fallbacks = {}
        for g in groups:
            if g["fallback"] is not None:
                fallbacks[g["fallback"]] = fallbacks.get(g["fallback"], 0) + 1
        summary["adr_0035"] = {
            "scoped_pairs": report["occluder_scoped_pairs"],
            "outside_pairs": report["occluder_outside_pairs"],
            "covers_whole_frame_pairs": sum(bool(g["group_covers_whole_frame"]) for g in groups),
            "opacity_attested_pairs": sum(g["opacity_attested"] for g in groups),
            "mean_group_frame_fraction": mean([g["group_frame_fraction"] for g in counted]),
            "mean_group_window_count": mean([g["group_window_count"] for g in counted]),
            "fallbacks": dict(sorted(fallbacks.items()))}
    return summary


def run(args, launch=subprocess.run):
    output = args.out.resolve()
    if any((p / ".git").exists() for p in (output.parent, *output.parent.parents)):
        raise ValueError("measurement output must be outside a repository")
    if not output.parent.is_dir() or output.exists():
        raise ValueError("output folder must exist and output file must be new")
    child = launch(probe_argv(args), capture_output=True, text=True, encoding="utf-8",
                   timeout=args.pairs * (args.interval_ms / 1000 + 10) + 30, check=False)
    if child.returncode != 0 or child.stdout:
        raise ValueError("read-only probe failed; no report was saved")
    report = validate_report(json.loads(child.stderr), args)
    # A paired report is saved with its descriptive summary; an old report exactly as it was.
    saved = {**report, "summary": summarise(report)} if "occluder_scoped_pairs" in report else report
    with output.open("x", encoding="utf-8") as stream:
        json.dump(saved, stream, indent=2, allow_nan=False)
        stream.write("\n")
    return 0


def main(argv=None):
    args = parse_args(argv)
    try:
        return run(args)
    except (OSError, ValueError, subprocess.SubprocessError):
        print("M1 failed; no valid report was saved (check policy, target, group and output).", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
