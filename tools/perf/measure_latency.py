#!/usr/bin/env python3
"""RNF-05 measurement: per-stage latency of `ommatix demo --source dxgi --timings`.

Sends no input of any kind. It starts our own test application (``tools/s4/testapp``) with
its hidden ``--animate`` flag, so that incremental regions appear by themselves, runs the
demo against it and reads the per-stage timings the demo writes to stderr. The only process
it ever ends is the test application it started. It checks, before and after each run,
that the foreground window is that process; a run where it was not is discarded, because
the screen would then show something the operator did not authorise.

Nothing is saved but the timings: no frame, no element text.

Several variants (a binary and a configuration each) can be compared in one go. Their runs
are interleaved, process by process and in a rotating order, so that whatever else the
machine is doing (and the desktop's other windows are many: UI Automation has to ask each
one) lands on every variant alike. Never compare numbers taken in different sessions.

Cases (each is one test application launch)::

    full         observations 3.. with a veil that comes and goes over the whole window -> every
                 observation is a full refresh, warm (the session has analysed before)
    incr1        observations 3.. with one animated label  -> one dirty region
    incr2        observations 3.. with two animated labels -> two dirty regions

The first observation of a session is a full refresh too, and is also reported (``first``):
it is the one that pays for whatever the detector has not done yet.

A single call of a long-running tool may not give the test application the foreground (the
window is then not in front and its runs are discarded): run it in chunks of a few minutes,
each with ``--out chunk-N.json``, and join them with ``--combine chunk-*.json --out all.json``.

Example (from the repository root, after ``cargo build --release -p ommatix --features
dxgi,directml,winrt-ocr,uia`` and ``cargo build --release`` in ``tools/s4/testapp``)::

    python tools/perf/measure_latency.py --out perf.json ^
        --variant before=old.exe,%LOCALAPPDATA%\\Ommatix\\before.toml ^
        --variant after=target/release/ommatix.exe,%LOCALAPPDATA%\\Ommatix\\after.toml
"""
from __future__ import annotations

import argparse
import ctypes
import json
import math
import re
import statistics
import subprocess
import sys
import time
from ctypes import wintypes
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TESTAPP = REPO / "tools" / "s4" / "testapp" / "target" / "release" / "ommatix-testapp.exe"
STAGES = ["capture", "plan", "detect", "recognize", "accessibility", "fuse", "track", "total"]
LINE = re.compile(r"^timings (\d+) \(ms\): (.*)$")
ELEMENTS = re.compile(r"^observation (\d+): snapshot \S+ \| frame \d+ \| (\d+) elements")

user32 = ctypes.windll.user32
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]


def foreground_pid() -> int:
    pid = wintypes.DWORD(0)
    user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(pid))
    return int(pid.value)


def percentile(values: list[float], q: float) -> float:
    """Nearest rank, as the S0 report does."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q * len(ordered)) - 1)]


def summary(values: list[float]) -> dict:
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "median": round(statistics.median(values), 2),
        "p95": round(percentile(values, 0.95), 2),
    }


def parse(stderr: str, stdout: str) -> dict[int, dict]:
    """observation number -> {stage: ms, 'elements': n}."""
    out: dict[int, dict] = {}
    for line in stderr.splitlines():
        m = LINE.match(line.strip())
        if m:
            fields = dict(re.findall(r"(\w+) ([0-9.]+)", m.group(2)))
            out[int(m.group(1))] = {k: float(v) for k, v in fields.items()}
    for line in stdout.splitlines():
        m = ELEMENTS.match(line)
        if m and int(m.group(1)) in out:
            out[int(m.group(1))]["elements"] = int(m.group(2))
    return out


CASES = {
    "incr1": ["--animate-labels", "1"],
    "incr2": ["--animate-labels", "2"],
    "full": ["--animate-flash"],
}


def run_case(args, variants: list[tuple[str, str, str]], name: str) -> dict:
    """One test application launch; every variant runs `--processes` times against it."""
    animate_ms = args.flash_ms if name == "full" else args.animate_ms
    app = subprocess.Popen([str(TESTAPP), "--animate", str(animate_ms), *CASES[name]])
    raw = {v: {"first": [], "incremental": [], "idle": 0, "discarded": 0} for v, _, _ in variants}
    try:
        time.sleep(4.0)  # the window opens maximised and draws
        for p in range(args.processes):
            # Rotate the order so that no variant is always first or always last.
            order = variants[p % len(variants):] + variants[: p % len(variants)]
            for label, exe, config in order:
                slot = raw[label]
                if foreground_pid() != app.pid:
                    slot["discarded"] += 1
                    print(f"[{name}/{label}] run {p}: the test application is not in the "
                          "foreground; discarded", file=sys.stderr)
                    time.sleep(1.0)
                    continue
                done = subprocess.run(
                    [exe, "demo", "--source", "dxgi", "--config", config,
                     "--session-id", f"perf-{name}-{label}-{p}",
                     "--observations", str(args.observations), "--timings"],
                    capture_output=True, text=True, cwd=REPO, timeout=300, check=False,
                )
                if foreground_pid() != app.pid or done.returncode != 0:
                    slot["discarded"] += 1
                    print(f"[{name}/{label}] run {p}: foreground changed or exit "
                          f"{done.returncode}; discarded", file=sys.stderr)
                    continue
                by_obs = parse(done.stderr, done.stdout)
                if 1 in by_obs:
                    slot["first"].append(by_obs[1])
                for k, sample in sorted(by_obs.items()):
                    if k < 3:
                        continue
                    # An observation with nothing planned (the label did not tick between
                    # two captures) is not an incremental analysis: counted apart.
                    if sample["detect"] > 0:
                        slot["incremental"].append(sample)
                    else:
                        slot["idle"] += 1
    finally:
        app.terminate()  # only the process we started
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.kill()
    return raw


def table(samples: list[dict]) -> dict:
    result = {stage: summary([s[stage] for s in samples if stage in s]) for stage in STAGES}
    result["elements"] = summary([float(s["elements"]) for s in samples if "elements" in s])
    return result


def report_from(raw: dict, meta: dict) -> dict:
    """Tables from raw samples: ``raw[case][variant] = {first, incremental, idle, discarded}``."""
    report = {**meta, "cases": {}, "raw": raw}
    first: dict[str, list[dict]] = {}
    for case, per_variant in raw.items():
        report["cases"][case] = {}
        for label, data in per_variant.items():
            first.setdefault(label, []).extend(data["first"])
            report["cases"][case][label] = {
                "stages": table(data["incremental"]),
                "idle_observations": data["idle"],
                "discarded_runs": data["discarded"],
            }
    report["cases"]["first observation"] = {
        label: {"stages": table(samples)} for label, samples in first.items()
    }
    return report


def combine(paths: list[str]) -> dict:
    """Joins the raw samples of several chunks of the same measurement."""
    merged: dict = {}
    meta: dict = {}
    for path in paths:
        chunk = json.loads(Path(path).read_text(encoding="utf-8"))
        meta = {k: v for k, v in chunk.items() if k not in ("cases", "raw")}
        for case, per_variant in chunk["raw"].items():
            for label, data in per_variant.items():
                slot = merged.setdefault(case, {}).setdefault(
                    label, {"first": [], "incremental": [], "idle": 0, "discarded": 0})
                slot["first"] += data.get("first", [])
                slot["incremental"] += data["incremental"]
                slot["idle"] += data["idle"]
                slot["discarded"] += data["discarded"]
    return report_from(merged, meta)


def show(report: dict) -> None:
    """The headline numbers as plain text: median / p95 of total, then of each stage."""
    for case, per_variant in report["cases"].items():
        print(f"\n== {case}")
        for label, entry in per_variant.items():
            t = entry["stages"]
            n = t["total"].get("n", 0)
            row = " ".join(
                f"{s} {t[s]['median']}/{t[s]['p95']}" for s in STAGES if t[s].get("n")
            )
            print(f"{label:>10}  n={n:<4} {row}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--variant", action="append", metavar="NAME=EXE,CONFIG")
    ap.add_argument("--combine", nargs="+", metavar="CHUNK.json",
                    help="join the raw samples of earlier chunks instead of measuring")
    ap.add_argument("--out", required=True)
    ap.add_argument("--processes", type=int, default=30)
    ap.add_argument("--observations", type=int, default=20)
    ap.add_argument("--animate-ms", type=int, default=30)
    ap.add_argument("--flash-ms", type=int, default=250,
                    help="period of the veil of the `full` case")
    ap.add_argument("--cases", default="full,incr1,incr2",
                    help="comma list of full, incr1, incr2")
    args = ap.parse_args()
    if args.combine:
        report = combine(args.combine)
        Path(args.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
        show(report)
        return 0
    if not args.variant:
        ap.error("--variant is required unless --combine is given")
    variants = []
    for spec in args.variant:
        label, rest = spec.split("=", 1)
        exe, config = rest.rsplit(",", 1)
        variants.append((label, exe, config))

    meta = {"animate_ms": args.animate_ms, "processes": args.processes,
            "observations": args.observations}
    raw: dict = {}
    for name in args.cases.split(","):
        raw[name] = run_case(args, variants, name)
    report = report_from(raw, meta)
    Path(args.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    show(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
