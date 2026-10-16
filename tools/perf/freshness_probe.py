#!/usr/bin/env python3
"""M1 operator driver: persist only validated numeric counts from the read-only probe.

The operator chooses the foreground window by hand. No engine, input request, screenshot,
window activation or config modification exists in this driver. See S4_SESIONES_PENDIENTES D.
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


def validate_report(report, args):
    """Reject unexpected keys/types rather than persisting raw child stderr."""
    keys = {"pairs", "interval_ms", "margin_px", "outside_pairs", "descriptive"}
    if not args.descriptive:
        keys |= {"min_outside_pairs", "criterion_met"}
    if not isinstance(report, dict) or set(report) != keys:
        raise ValueError("invalid measurement report")
    if type(report["descriptive"]) is not bool or report["descriptive"] != args.descriptive:
        raise ValueError("invalid measurement mode")
    counts = ("total_tiles", "group_tiles", "group_window_count")
    fields = {*counts, "touches_target", "group_frame_fraction", "group_covers_whole_frame", "capture_gap_ms"}
    if not isinstance(report["pairs"], list) or len(report["pairs"]) != args.pairs:
        raise ValueError("incomplete measurement")
    for pair in report["pairs"]:
        if not isinstance(pair, dict) or set(pair) != fields:
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
    if not args.descriptive:
        if (type(report["min_outside_pairs"]) is not int
                or report["min_outside_pairs"] != args.min_outside_pairs
                or type(report["criterion_met"]) is not bool
                or report["criterion_met"] != (outside >= args.min_outside_pairs)):
            raise ValueError("inconsistent criterion")
    return report


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
    with output.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
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
