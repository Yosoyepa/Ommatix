#!/usr/bin/env python3
"""Per-observation latency of the perception engine, and the B0/B1 comparison (RNF-05, INC-02).

It starts our own test application (``tools/s4/testapp``) with its hidden ``--animate`` flag,
so that incremental regions appear by themselves, runs ``ommatix demo --source dxgi --timings``
against it and reads one JSON record per observation from the demo's stderr. Nothing is sent
to the screen beyond what the test application and the demo already do: no input of any kind
is delivered (the demo only observes and performs a dry-run action). Before and after each run
the tool checks that the foreground window belongs to the test application; a run where it did
not is discarded, because the screen would then show something the operator did not authorise.

Nothing is saved but the records and the manifest. Each observation keeps numbers,
enumerations and, for detection agreement only, one 16-digit signature per element (computed by
the demo from its role, label and actionability). The manifest keeps the paths and the hashes
of the executables and configurations. No frame and no element text is written.

Variants: a binary and a configuration each (``--variant NAME=EXE,CONFIG``). Their runs are
interleaved, process by process and in a rotating order, so that whatever else the machine is
doing lands on every variant alike. Before anything starts, every difference between the
variants must be declared with ``--allow-difference``: ``binary`` for another executable, or
the dotted key of a configuration leaf (``engine.experimental_full_frame``). An undeclared
difference, or a configuration that could deliver input, refuses the run and nothing starts.
B0 is the experimental full-frame baseline of INC-02 (``[engine] experimental_full_frame``);
it is never a production mode.

Cases (each is one test application launch)::

    full         observations 3.. with a veil that comes and goes over the whole window
    incr1        observations 3.. with one animated label  -> one dirty region
    incr2        observations 3.. with two animated labels -> two dirty regions

Classes of an observation, decided by its record and never by a rounded time: ``first``
(observation 1), ``second`` (observation 2), ``warm_full`` and ``warm_regions`` (observations
3.. that were analysed), ``idle`` (observations 3.. with nothing planned: counted, not timed),
and ``error`` (kept in the samples with its error code, never timed). Timing statistics are
over the warm observations only, in milliseconds, split by the work the plan did.

Pairing: ``--paired BASE:TREAT`` pairs the processes of two variants that share a sequence
(same run, same case and process index). The estimand is the difference of the per-process
medians of ``total`` (TREAT minus BASE, in ms) and their ratio; the 95 % intervals are
percentile bootstrap intervals over processes (never over frames), with a fixed seed. Detection
agreement is the Dice coefficient of the element signatures of two paired observations: it
measures how much two variants agree, not how accurate either is.

Chunks: a long measurement runs in chunks of a few minutes, each with its own ``--out``; join
them with ``--combine`` (only reports of the same measurement join; a chunk given twice, a run
or an observation that appears twice, or a report of another schema is refused).

Example, from the repository root, after ``cargo build --release -p ommatix --features
dxgi,sendinput,directml,winrt-ocr,uia`` and ``cargo build --release`` in ``tools/s4/testapp``
(two binaries, the RNF-05 comparison)::

    python tools/perf/measure_latency.py --experiment-id rnf05-before-after ^
        --variant before=old.exe,%LOCALAPPDATA%\\Ommatix\\before.toml ^
        --variant after=target\\release\\ommatix.exe,%LOCALAPPDATA%\\Ommatix\\after.toml ^
        --allow-difference binary --out perf.json

B0 and B1 (the same executable, a configuration that differs only in the experimental switch)::

    python tools/perf/measure_latency.py --experiment-id EXPERIMENT ^
        --variant B0=target\\release\\ommatix.exe,CONFIG_B0.toml ^
        --variant B1=target\\release\\ommatix.exe,CONFIG_B1.toml ^
        --allow-difference engine.experimental_full_frame --paired B0:B1 --out chunk-N.json

A third variant for the adaptive cost rule of INC-07 (ADR-0030) differs from B1 only in
``[engine] adaptive_min_regions``; declare it too and pair as needed::

    python tools/perf/measure_latency.py --experiment-id EXPERIMENT ^
        --variant B0=EXE,B0.toml --variant B1=EXE,B1.toml --variant AD=EXE,AD.toml ^
        --allow-difference engine.experimental_full_frame ^
        --allow-difference engine.adaptive_min_regions ^
        --paired B0:B1 --paired B0:AD --paired B1:AD --out chunk-N.json

Join chunks and print the headline table again::

    python tools/perf/measure_latency.py --combine chunk-1.json chunk-2.json ^
        --paired B0:B1 --out all.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import re
import statistics
import subprocess
import sys
import time
import tomllib
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as common  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
TESTAPP = REPO / "tools" / "s4" / "testapp" / "target" / "release" / "ommatix-testapp.exe"
PROTOCOL = "inc01-measurement-1"
REPORT_SCHEMA = "ommatix.perf/2"
RECORD_SCHEMA = "ommatix.observation/1"
RECORD_PREFIX = "ommatix-observation "
STAGES = ("capture", "plan", "detect", "recognize", "accessibility", "fuse", "track",
          "commit", "response_build", "total")
WORK_KINDS = ("full", "regions", "idle")
REFRESH_REASONS = ("no_committed_base", "geometry_changed", "closure_budget",
                   "too_many_regions", "area_budget", "experimental_baseline",
                   "adaptive_cost")
PROCESS_STATUSES = ("ok", "discarded", "failed")
WARM = ("warm_full", "warm_regions")
CASES = {
    "incr1": ["--animate-labels", "1"],
    "incr2": ["--animate-labels", "2"],
    "full": ["--animate-flash"],
}
BOOTSTRAP_SEED = 20261015
BOOTSTRAP_RESAMPLES = 10000
REQUIRED_RECORD_KEYS = ("schema", "observation_id", "outcome", "error_code", "snapshot_id",
                        "geometry_epoch", "timings_us", "plan", "signatures")
REPORT_KEYS = ("protocol", "experiment_id", "run_ids", "code_commit", "test_app_sha256",
               "animate_ms", "flash_ms", "observations", "cases", "variant_order", "variants",
               "allowed_differences", "differences_found", "processes", "samples")
PROCESS_KEYS = ("experiment_id", "run_id", "process_id", "sequence_id", "case", "variant",
                "order", "status", "reason")
SAMPLE_KEYS = ("experiment_id", "run_id", "process_id", "sequence_id", "observation_id",
               "variant", "case", "observation")
# Manifest values that every chunk of one measurement must share (see combine).
IDENTITY_KEYS = ("protocol", "experiment_id", "code_commit", "test_app_sha256", "animate_ms",
                 "flash_ms", "observations", "variant_order", "allowed_differences")
SIGNATURE = re.compile(r"[0-9a-f]{16}")
LABEL = re.compile(r"[A-Za-z0-9_-]+")


class Refused(Exception):
    """The run cannot start, or the reports cannot be joined. Nothing is written."""


class RecordError(ValueError):
    """An observation record the contract does not allow."""


@dataclass(frozen=True)
class Variant:
    label: str
    exe: str
    config: str


# ---- records ----------------------------------------------------------------------------------

def _whole(value, minimum: int = 0) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= minimum


def _measure(value) -> bool:
    return (isinstance(value, (int, float)) and not isinstance(value, bool)
            and math.isfinite(value) and value >= 0)


def validate_plan(plan) -> None:
    """The effective plan of an analysed observation (every field is measured: no null
    except the inference fields, which are null together when the detector does not count)."""
    if not isinstance(plan, dict):
        raise RecordError("an ok record has no plan")
    work = plan.get("work")
    if work not in WORK_KINDS:
        raise RecordError(f"unknown plan.work {work!r}")
    reason = plan.get("full_refresh_reason")
    if work == "full" and reason not in REFRESH_REASONS:
        raise RecordError("a full refresh names no known full_refresh_reason")
    if work != "full" and reason is not None:
        raise RecordError("only a full refresh carries a full_refresh_reason")
    if not _whole(plan.get("region_count")):
        raise RecordError("plan.region_count is not a whole number")
    if not _measure(plan.get("region_area_px")):
        raise RecordError("plan.region_area_px is not a non-negative number")
    counts, shapes = plan.get("inference_count"), plan.get("input_shapes")
    if (counts is None) != (shapes is None):
        raise RecordError("inference_count and input_shapes are measured together or not at all")
    if counts is not None:
        valid_shapes = isinstance(shapes, list) and all(
            isinstance(shape, list) and len(shape) == 4
            and all(_whole(dim, 1) for dim in shape) for shape in shapes)
        if not _whole(counts) or not valid_shapes or len(shapes) != counts:
            raise RecordError("input_shapes does not match inference_count")
    if not isinstance(plan.get("accessibility_truncated"), bool):
        raise RecordError("plan.accessibility_truncated is not a boolean")
    for key in ("controls_n", "words_n", "supports_n", "relations_n"):
        if not _whole(plan.get(key)):
            raise RecordError(f"plan.{key} is not a whole number")


def validate_record(record) -> dict:
    """Checks one observation record against ``ommatix.observation/1``; returns it."""
    if not isinstance(record, dict):
        raise RecordError("the observation record is not an object")
    missing = [key for key in REQUIRED_RECORD_KEYS if key not in record]
    if missing:
        raise RecordError("the observation record lacks " + ", ".join(missing))
    if record["schema"] != RECORD_SCHEMA:
        raise RecordError(f"unknown observation record schema {record['schema']!r}")
    if not _whole(record["observation_id"], 1):
        raise RecordError("observation_id is not a whole number from 1")
    signatures = record["signatures"]
    if not isinstance(signatures, list) or not all(
            isinstance(s, str) and SIGNATURE.fullmatch(s) for s in signatures):
        raise RecordError("signatures is not a list of 16 lowercase hex digits per element")
    outcome = record["outcome"]
    if outcome == "error":
        if not isinstance(record["error_code"], str) or not record["error_code"]:
            raise RecordError("an error record names no error_code")
        if record["timings_us"] is not None or record["plan"] is not None:
            raise RecordError("an error record carries timings or a plan")
        if signatures:
            raise RecordError("an error record carries signatures")
        return record
    if outcome != "ok":
        raise RecordError(f"unknown outcome {outcome!r}")
    if record["error_code"] is not None:
        raise RecordError("an ok record names an error_code")
    if not isinstance(record["snapshot_id"], str):
        raise RecordError("an ok record has no snapshot_id")
    if not _whole(record["geometry_epoch"]):
        raise RecordError("geometry_epoch is not a whole number")
    timings = record["timings_us"]
    if not isinstance(timings, dict):
        raise RecordError("an ok record has no timings_us object")
    for stage in STAGES:
        if stage not in timings:
            raise RecordError(f"timings_us lacks {stage}")
        if not _whole(timings[stage]):
            raise RecordError(f"timings_us.{stage} is not a whole number of microseconds")
    validate_plan(record["plan"])
    return record


def parse_records(stderr: str) -> list[dict]:
    """The observation records of one demo run, in order. Other lines are ignored; a line
    that starts with the record prefix must be a valid record, or the whole run is refused."""
    records, seen = [], set()
    for number, line in enumerate(stderr.splitlines(), start=1):
        if not line.startswith(RECORD_PREFIX):
            continue
        try:
            record = json.loads(line[len(RECORD_PREFIX):])
        except json.JSONDecodeError as error:
            raise RecordError(f"line {number}: malformed JSON ({error.msg})") from error
        try:
            validate_record(record)
        except RecordError as error:
            raise RecordError(f"line {number}: {error}") from error
        if record["observation_id"] in seen:
            raise RecordError(f"line {number}: observation {record['observation_id']} "
                              "is recorded twice")
        seen.add(record["observation_id"])
        records.append(record)
    return records


def classify(sample: dict) -> str:
    """The class of one sample, from its record (never from a rounded time)."""
    observation = sample["observation"]
    if observation["outcome"] == "error":
        return "error"
    number = sample["observation_id"]
    if number == 1:
        return "first"
    if number == 2:
        return "second"
    if observation["plan"]["work"] == "idle":
        return "idle"
    return "warm_" + observation["plan"]["work"]


# ---- statistics (pure) ------------------------------------------------------------------------

def stage_table(observations: list[dict]) -> dict:
    """Milliseconds per stage over the given ok observations; ``{"n": 0}`` when there are none."""
    return {stage: common.summary([o["timings_us"][stage] / 1000.0 for o in observations])
            for stage in STAGES}


def plan_summary(ok: list[dict], warm: list[dict]) -> dict:
    plans = [o["plan"] for o in ok]
    warm_plans = [o["plan"] for o in warm]
    counts = [p["inference_count"] for p in warm_plans if p["inference_count"] is not None]
    shapes = Counter()
    for plan in warm_plans:
        for shape in plan["input_shapes"] or []:
            shapes["x".join(str(dim) for dim in shape)] += 1
    shaped = sum(1 for p in warm_plans if p["input_shapes"] is not None)
    truncated = sum(1 for p in plans if p["accessibility_truncated"])
    return {
        "work": {kind: sum(1 for p in plans if p["work"] == kind) for kind in WORK_KINDS},
        "full_refresh_reason": dict(sorted(Counter(
            p["full_refresh_reason"] or "not_applicable" for p in plans).items())),
        "warm_region_count": common.summary([float(p["region_count"]) for p in warm_plans]),
        "warm_region_area_px": common.summary(
            [float(p["region_area_px"]) for p in warm_plans]),
        "warm_inference_count": {
            "null": len(warm_plans) - len(counts),
            **common.summary([float(c) for c in counts])},
        "warm_input_shapes": {"observations": shaped, "null": len(warm_plans) - shaped,
                              "shapes": dict(sorted(shapes.items()))},
        "accessibility_truncated": {"true": truncated, "false": len(plans) - truncated},
        "elements": common.summary([float(len(o["signatures"])) for o in ok]),
    }


def bootstrap(units: list, stats: dict) -> dict:
    """95 % percentile bootstrap intervals of each statistic over whole units (processes).
    Units are resampled with replacement, ``BOOTSTRAP_RESAMPLES`` times, with a fixed seed.
    ``None`` when fewer than two units exist (no interval, not a zero-width one)."""
    if len(units) < 2:
        return {name: None for name in stats}
    rng = random.Random(BOOTSTRAP_SEED)
    draws = {name: [] for name in stats}
    for _ in range(BOOTSTRAP_RESAMPLES):
        resample = [units[rng.randrange(len(units))] for _ in units]
        for name, stat in stats.items():
            value = stat(resample)
            if value is not None:
                draws[name].append(value)
    return {name: ([round(common.percentile(v, 0.025), 4),
                    round(common.percentile(v, 0.975), 4)] if len(v) >= 2 else None)
            for name, v in draws.items()}


def dice(a: Counter, b: Counter) -> float | None:
    """Dice coefficient of two multisets; None when both are empty (not measured)."""
    size = sum(a.values()) + sum(b.values())
    if size == 0:
        return None
    return 2 * sum((a & b).values()) / size


def _mean(values: list) -> float | None:
    return round(statistics.fmean(values), 4) if values else None


def paired_total(samples: list[dict], base: str, treat: str, case: str) -> dict:
    """Per process (run and sequence), the medians of ``total`` over the warm observations of
    each variant; the difference TREAT minus BASE and the ratio, bootstrapped over processes."""
    groups: dict = {}
    for s in samples:
        if s["case"] == case and s["variant"] in (base, treat):
            groups.setdefault((s["run_id"], s["sequence_id"]), {}).setdefault(
                s["variant"], []).append(s)

    def warm_ms(group: list[dict]) -> list[float]:
        return [s["observation"]["timings_us"]["total"] / 1000.0 for s in group
                if classify(s) in WARM]

    units, rows, excluded = [], [], 0
    for key in sorted(groups):
        b = warm_ms(groups[key].get(base, []))
        t = warm_ms(groups[key].get(treat, []))
        if not b or not t:
            excluded += 1
            continue
        mb, mt = statistics.median(b), statistics.median(t)
        ratio = mt / mb if mb > 0 else None
        units.append((mt - mb, ratio))
        rows.append({"run_id": key[0], "sequence_id": key[1],
                     "base_median_ms": round(mb, 3), "treat_median_ms": round(mt, 3),
                     "delta_ms": round(mt - mb, 3),
                     "ratio": None if ratio is None else round(ratio, 4),
                     "base_n": len(b), "treat_n": len(t)})

    def delta_mean(u):
        return statistics.fmean([d for d, _ in u])

    def delta_median(u):
        return statistics.median([d for d, _ in u])

    def ratio_mean(u):
        ratios = [r for _, r in u if r is not None]
        return statistics.fmean(ratios) if ratios else None

    intervals = bootstrap(units, {"delta_mean": delta_mean, "delta_median": delta_median,
                                  "ratio_mean": ratio_mean})
    deltas = [d for d, _ in units]
    ratios = [r for _, r in units if r is not None]
    return {
        "unit": "process (run_id, sequence_id)",
        "processes": len(units),
        "excluded_processes": excluded,
        "delta_ms": {"mean": _mean(deltas),
                     "median": round(statistics.median(deltas), 4) if deltas else None,
                     "ci95_mean": intervals["delta_mean"],
                     "ci95_median": intervals["delta_median"]},
        "ratio": {"mean": _mean(ratios), "ci95_mean": intervals["ratio_mean"]},
        "per_process": rows,
    }


def agreement_dice(samples: list[dict], base: str, treat: str, case: str) -> dict:
    """Detection agreement, not accuracy: the Dice coefficient of the element signatures of
    the same observation number in two variants of the same process."""
    groups: dict = {}
    for s in samples:
        if (s["case"] == case and s["variant"] in (base, treat)
                and s["observation"]["outcome"] == "ok"):
            groups.setdefault((s["run_id"], s["sequence_id"]), {}).setdefault(
                s["variant"], {})[s["observation_id"]] = s["observation"]
    units, rows = [], []
    measured = identical = not_measured = excluded = 0
    for key in sorted(groups):
        b_obs = groups[key].get(base, {})
        t_obs = groups[key].get(treat, {})
        values = []
        for number in sorted(set(b_obs) & set(t_obs)):
            a = Counter(b_obs[number]["signatures"])
            b = Counter(t_obs[number]["signatures"])
            value = dice(a, b)
            if value is None:
                not_measured += 1
                continue
            measured += 1
            identical += int(a == b)
            values.append(value)
        if not values:
            excluded += 1
            continue
        units.append(statistics.fmean(values))
        rows.append({"run_id": key[0], "sequence_id": key[1], "paired": len(values),
                     "mean_dice": round(statistics.fmean(values), 4)})
    intervals = bootstrap(units, {"mean": statistics.fmean})
    return {
        "meaning": "agreement between two variants, not accuracy against ground truth",
        "unit": "process (run_id, sequence_id)",
        "processes": len(units),
        "excluded_processes": excluded,
        "paired_observations": measured,
        "not_measured_observations": not_measured,
        "identical_observations": identical,
        "identical_share": round(identical / measured, 4) if measured else None,
        "mean_dice": _mean(units),
        "ci95_mean_dice": intervals["mean"],
        "per_process": rows,
    }


def case_summary(samples: list[dict], processes: list[dict]) -> dict:
    classes = Counter(classify(s) for s in samples)
    error_codes = Counter(s["observation"]["error_code"] for s in samples
                          if classify(s) == "error")
    ok = [s["observation"] for s in samples if s["observation"]["outcome"] == "ok"]
    warm_full = [s["observation"] for s in samples if classify(s) == "warm_full"]
    warm_regions = [s["observation"] for s in samples if classify(s) == "warm_regions"]
    return {
        "classes": {"first": classes["first"], "second": classes["second"],
                    "warm_full": classes["warm_full"], "warm_regions": classes["warm_regions"],
                    "idle": classes["idle"], "error": classes["error"],
                    "error_codes": dict(sorted(error_codes.items()))},
        "stages_ms": {"full": stage_table(warm_full), "regions": stage_table(warm_regions)},
        "plan": plan_summary(ok, warm_full + warm_regions),
        "processes": {status: sum(1 for p in processes if p["status"] == status)
                      for status in PROCESS_STATUSES},
    }


def build_report(manifest: dict, processes: list[dict], samples: list[dict],
                 pairs: list[tuple[str, str]]) -> dict:
    """The report: the manifest, the raw processes and samples, and every summary computed
    from those raw lists (summaries are never averaged)."""
    summary = {"cases": {}, "first_observation": {}, "paired": {}}
    for case in manifest["cases"]:
        summary["cases"][case] = {}
        for label in manifest["variant_order"]:
            summary["cases"][case][label] = case_summary(
                [s for s in samples if s["case"] == case and s["variant"] == label],
                [p for p in processes if p["case"] == case and p["variant"] == label])
        summary["paired"][case] = {
            f"{base}:{treat}": {
                "total": paired_total(samples, base, treat, case),
                "agreement_dice": agreement_dice(samples, base, treat, case)}
            for base, treat in pairs}
    for label in manifest["variant_order"]:
        first = [s["observation"] for s in samples
                 if s["variant"] == label and classify(s) == "first"]
        summary["first_observation"][label] = {"stages_ms": stage_table(first)}
    return {**manifest, "processes": processes, "samples": samples, "summary": summary}


# ---- configuration and preflight --------------------------------------------------------------

def flatten(table: dict, prefix: str = "") -> dict:
    leaves = {}
    for key, value in table.items():
        dotted = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            leaves.update(flatten(value, dotted))
        else:
            leaves[dotted] = value
    return leaves


def shown(value):
    """A configuration value as the report may keep it: numbers and booleans as they are,
    anything else (a path, a list of applications) only by its type."""
    if isinstance(value, (bool, int, float)):
        return value
    return f"<{type(value).__name__}>"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_variants(specs: list[str]) -> list[Variant]:
    variants, labels = [], set()
    for spec in specs:
        label, sep, rest = spec.partition("=")
        exe, comma, config = rest.rpartition(",")
        if not sep or not comma or not exe or not config:
            raise Refused(f"--variant expects NAME=EXE,CONFIG, got {spec!r}")
        if not LABEL.fullmatch(label):
            raise Refused(f"variant name {label!r} must be letters, digits, '-' or '_'")
        if label in labels:
            raise Refused(f"variant {label!r} is given twice")
        labels.add(label)
        variants.append(Variant(label, exe, config))
    return variants


def parse_cases(text: str) -> list[str]:
    names: list[str] = []
    for name in (part.strip() for part in text.split(",")):
        if name not in CASES:
            raise Refused(f"unknown case {name!r}; use {', '.join(CASES)}")
        if name not in names:
            names.append(name)
    return names


def parse_pairs(specs: list[str], labels: list[str]) -> list[tuple[str, str]]:
    pairs = []
    for spec in specs:
        base, sep, treat = spec.partition(":")
        if not sep or not base or not treat or base == treat:
            raise Refused(f"--paired expects BASE:TREAT with two variants, got {spec!r}")
        for label in (base, treat):
            if label not in labels:
                raise Refused(f"--paired names {label!r}, which is not a variant")
        pairs.append((base, treat))
    return pairs


def preflight(variants: list[Variant], allowed: set[str], testapp: Path) -> dict:
    """Checks every variant before anything starts and returns what the manifest records.
    Refuses an undeclared difference between the variants and a configuration that is not
    input free (``perf_common.config_problem``)."""
    if not variants:
        raise Refused("at least one --variant is needed")
    if not testapp.is_file():
        raise Refused(f"the test application is not built: {testapp}")
    table, leaves_of = {}, {}
    for variant in variants:
        exe, config = Path(variant.exe), Path(variant.config)
        if not exe.is_file():
            raise Refused(f"{variant.label}: no executable at {exe}")
        if not config.is_file():
            raise Refused(f"{variant.label}: no configuration at {config}")
        data = config.read_bytes()
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError as error:
            raise Refused(f"{variant.label}: {config} is not UTF-8 text") from error
        problem = common.config_problem(text)
        if problem is not None:
            raise Refused(f"{variant.label}: {config} could deliver input: {problem}")
        leaves_of[variant.label] = flatten(tomllib.loads(text))
        table[variant.label] = {
            "exe": str(exe), "exe_sha256": sha256_file(exe),
            "config": str(config), "config_sha256": hashlib.sha256(data).hexdigest()}
    differences = []
    exe_hashes = {label: entry["exe_sha256"] for label, entry in table.items()}
    if len(set(exe_hashes.values())) > 1:
        differences.append({"key": "binary", "values": exe_hashes})
    keys = sorted(set().union(*(leaves.keys() for leaves in leaves_of.values())))
    for key in keys:
        present = {label: key in leaves for label, leaves in leaves_of.items()}
        values = {label: (json.dumps(leaves[key], sort_keys=True, default=str)
                          if present[label] else None)
                  for label, leaves in leaves_of.items()}
        if len(set(values.values())) > 1:
            differences.append({"key": key, "values": {
                label: shown(leaves[key]) if present[label] else "<absent>"
                for label, leaves in leaves_of.items()}})
    undeclared = [d["key"] for d in differences if d["key"] not in allowed]
    if undeclared:
        raise Refused("the variants differ in " + ", ".join(undeclared)
                      + "; declare each with --allow-difference or make them equal")
    return {"variants": table, "differences_found": differences,
            "test_app_sha256": sha256_file(testapp)}


def git_commit() -> str | None:
    return common.run_read_only(["git", "rev-parse", "HEAD"])


def new_run_id() -> str:
    """A UTC stamp and the process id: two invocations never share a run, even in one second."""
    return time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + f"-{os.getpid()}"


# ---- running (Windows) ------------------------------------------------------------------------

def foreground_pid() -> int:
    """The process that owns the foreground window (Windows only; OSError elsewhere)."""
    return common.foreground_pid()


def sample_of(process: dict, record: dict) -> dict:
    observation = {key: value for key, value in record.items() if key != "observation_id"}
    return {"experiment_id": process["experiment_id"], "run_id": process["run_id"],
            "process_id": process["process_id"], "sequence_id": process["sequence_id"],
            "observation_id": record["observation_id"], "variant": process["variant"],
            "case": process["case"], "observation": observation}


def run_case(args, variants: list[Variant], name: str, experiment_id: str,
             run_id: str) -> tuple[list[dict], list[dict]]:
    """One test application launch; every variant runs once per process index, interleaved."""
    animate_ms = args.flash_ms if name == "full" else args.animate_ms
    app = subprocess.Popen([str(TESTAPP), "--animate", str(animate_ms), *CASES[name]])
    processes: list[dict] = []
    samples: list[dict] = []
    try:
        time.sleep(4.0)  # the window opens maximised and draws
        for p in range(args.processes):
            # Rotate the order so that no variant is always first or always last.
            shift = p % len(variants)
            order = variants[shift:] + variants[:shift]
            for variant in order:
                process = {
                    "experiment_id": experiment_id, "run_id": run_id,
                    "process_id": f"{name}-{variant.label}-p{p:03d}",
                    "sequence_id": f"{name}-p{p:03d}", "case": name,
                    "variant": variant.label, "order": [v.label for v in order],
                    "status": "ok", "reason": None}
                processes.append(process)
                if foreground_pid() != app.pid:
                    process.update(status="discarded",
                                   reason="the test application was not in the foreground")
                    print(f"[{name}/{variant.label}] process {p}: the test application is not "
                          "in the foreground; discarded", file=sys.stderr)
                    time.sleep(1.0)
                    continue
                done = subprocess.run(
                    [variant.exe, "demo", "--source", "dxgi", "--config", variant.config,
                     "--session-id", process["process_id"],
                     "--observations", str(args.observations), "--timings"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace",
                    cwd=REPO, timeout=300, check=False)
                after = foreground_pid()
                if after != app.pid or done.returncode != 0:
                    reason = ("the foreground changed" if after != app.pid
                              else f"the demo exited with {done.returncode}")
                    process.update(status="discarded", reason=reason)
                    print(f"[{name}/{variant.label}] process {p}: {reason}; discarded",
                          file=sys.stderr)
                    continue
                try:
                    records = parse_records(done.stderr)
                except RecordError as error:
                    process.update(status="failed", reason=str(error))
                    print(f"[{name}/{variant.label}] process {p}: {error}; failed",
                          file=sys.stderr)
                    continue
                samples += [sample_of(process, record) for record in records]
    finally:
        app.terminate()  # only the process we started
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.kill()
    return processes, samples


def measure(args) -> dict:
    if not args.experiment_id:
        raise Refused("--experiment-id is required for a measurement")
    if not args.variant:
        raise Refused("--variant is required unless --combine is given")
    if args.processes < 1 or args.observations < 1:
        raise Refused("--processes and --observations must be at least 1")
    variants = parse_variants(args.variant)
    cases = parse_cases(args.cases)
    labels = [v.label for v in variants]
    pairs = parse_pairs(args.paired, labels)
    facts = preflight(variants, set(args.allow_difference), TESTAPP)
    run_id = new_run_id()
    manifest = {
        "schema": REPORT_SCHEMA, "protocol": PROTOCOL, "experiment_id": args.experiment_id,
        "run_ids": [run_id], "code_commit": git_commit(),
        "test_app_sha256": facts["test_app_sha256"], "animate_ms": args.animate_ms,
        "flash_ms": args.flash_ms, "observations": args.observations,
        "processes_requested": args.processes, "cases": cases, "variant_order": labels,
        "variants": facts["variants"],
        "allowed_differences": sorted(set(args.allow_difference)),
        "differences_found": facts["differences_found"],
    }
    processes: list[dict] = []
    samples: list[dict] = []
    for name in cases:
        done, got = run_case(args, variants, name, args.experiment_id, run_id)
        processes += done
        samples += got
    return build_report(manifest, processes, samples, pairs)


def load_report(path: str, data: bytes) -> dict:
    try:
        doc = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise Refused(f"{path} is not a JSON report: {error}") from error
    if not isinstance(doc, dict):
        raise Refused(f"{path} is not a report object")
    if doc.get("schema") != REPORT_SCHEMA:
        raise Refused(f"{path} has report schema {doc.get('schema')!r}; only "
                      f"{REPORT_SCHEMA!r} can be combined")
    missing = [key for key in REPORT_KEYS if key not in doc]
    if missing:
        raise Refused(f"{path} lacks " + ", ".join(missing))
    for process in doc["processes"]:
        if (not isinstance(process, dict) or any(k not in process for k in PROCESS_KEYS)
                or process["status"] not in PROCESS_STATUSES):
            raise Refused(f"{path} has a process record that is incomplete")
    for sample in doc["samples"]:
        if not isinstance(sample, dict) or any(k not in sample for k in SAMPLE_KEYS):
            raise Refused(f"{path} has a sample that is incomplete")
        if sample["run_id"] not in doc["run_ids"]:
            raise Refused(f"{path} has a sample of run {sample['run_id']} that the report "
                          "does not list")
        try:
            validate_record({**sample["observation"], "observation_id": sample["observation_id"]})
        except (RecordError, TypeError) as error:
            raise Refused(f"{path}: {error}") from error
    return doc


def variant_table(doc: dict) -> dict:
    return {label: {"exe_sha256": entry["exe_sha256"], "config_sha256": entry["config_sha256"]}
            for label, entry in doc["variants"].items()}


def combine(paths: list[str], pair_specs: list[str]) -> dict:
    if not paths:
        raise Refused("--combine needs at least one report")
    chunks, digests = [], {}
    for path in paths:
        data = Path(path).read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest in digests:
            raise Refused(f"{path} has the same content as {digests[digest]}: its samples "
                          "would be counted twice")
        digests[digest] = path
        chunks.append({"path": path, "sha256": digest, "doc": load_report(path, data)})
    first = chunks[0]["doc"]
    for chunk in chunks[1:]:
        doc = chunk["doc"]
        for key in IDENTITY_KEYS:
            if doc[key] != first[key]:
                raise Refused(f"{chunk['path']} is not from the same measurement: {key} "
                              "differs")
        if variant_table(doc) != variant_table(first):
            raise Refused(f"{chunk['path']} has another variant table (executables or "
                          "configurations differ)")
    run_ids: list[str] = []
    for chunk in chunks:
        for run_id in chunk["doc"]["run_ids"]:
            if run_id in run_ids:
                raise Refused(f"run {run_id} appears in more than one report, or twice in "
                              "one: its samples would be counted twice")
            run_ids.append(run_id)
    process_keys: set = set()
    sample_keys: set = set()
    processes: list[dict] = []
    samples: list[dict] = []
    cases: list[str] = []
    for chunk in chunks:
        doc = chunk["doc"]
        for process in doc["processes"]:
            key = (process["run_id"], process["process_id"])
            if key in process_keys:
                raise Refused(f"process {key[1]} of run {key[0]} appears twice")
            process_keys.add(key)
            processes.append(process)
        for sample in doc["samples"]:
            key = (sample["run_id"], sample["process_id"], sample["observation_id"])
            if key in sample_keys:
                raise Refused(f"observation {key} appears twice, in {chunk['path']} or "
                              "another report")
            sample_keys.add(key)
            samples.append(sample)
        cases += [c for c in doc["cases"] if c not in cases]
    manifest = {key: first[key] for key in IDENTITY_KEYS}
    manifest.update(
        schema=REPORT_SCHEMA, run_ids=run_ids, cases=cases,
        variants=first["variants"], differences_found=first["differences_found"],
        processes_requested=sum(c["doc"].get("processes_requested", 0) for c in chunks),
        chunks=[{"path": c["path"], "sha256": c["sha256"], "run_ids": c["doc"]["run_ids"]}
                for c in chunks])
    pairs = parse_pairs(pair_specs, manifest["variant_order"])
    return build_report(manifest, processes, samples, pairs)


# ---- output -----------------------------------------------------------------------------------

def _range(summary: dict) -> str:
    if not summary.get("n"):
        return "-"
    return f"median {summary['median']} p95 {summary['p95']}"


def show(report: dict) -> None:
    """The headline numbers as plain text."""
    for case, labels in report["summary"]["cases"].items():
        print(f"\n== {case}")
        for label, entry in labels.items():
            full = entry["stages_ms"]["full"]["total"]
            regions = entry["stages_ms"]["regions"]["total"]
            classes = entry["classes"]
            print(f"{label:>10}  full n={full.get('n', 0)} ({_range(full)}) | regions "
                  f"n={regions.get('n', 0)} ({_range(regions)}) | idle {classes['idle']} "
                  f"error {classes['error']} | processes {entry['processes']}")
    for case, pairs in report["summary"]["paired"].items():
        for name, entry in pairs.items():
            total, dices = entry["total"], entry["agreement_dice"]
            print(f"\n== {case} {name}: total delta (TREAT - BASE) mean "
                  f"{total['delta_ms']['mean']} ms, CI95 {total['delta_ms']['ci95_mean']}, "
                  f"processes {total['processes']}; agreement (Dice) mean "
                  f"{dices['mean_dice']}, CI95 {dices['ci95_mean_dice']}")


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(
        description="Per-observation latency of ommatix demo, and the B0/B1 comparison.",
        formatter_class=argparse.RawDescriptionHelpFormatter, epilog=__doc__)
    ap.add_argument("--variant", action="append", metavar="NAME=EXE,CONFIG",
                    help="a variant: an executable and its configuration (repeatable)")
    ap.add_argument("--experiment-id", metavar="ID",
                    help="the experiment every sample belongs to (required to measure)")
    ap.add_argument("--allow-difference", action="append", default=[], metavar="KEY",
                    help="a declared difference between the variants: binary, or a dotted "
                         "configuration key (repeatable)")
    ap.add_argument("--paired", action="append", default=[], metavar="BASE:TREAT",
                    help="pair two variants and report TREAT minus BASE (repeatable)")
    ap.add_argument("--combine", nargs="+", metavar="REPORT.json",
                    help="join reports of the same measurement instead of measuring")
    ap.add_argument("--out", required=True, help="the report to write (JSON, atomic)")
    ap.add_argument("--processes", type=int, default=30,
                    help="process indexes per case; each variant runs once per index")
    ap.add_argument("--observations", type=int, default=20,
                    help="observations per demo run")
    ap.add_argument("--animate-ms", type=int, default=30,
                    help="period of the animated labels of the incr cases")
    ap.add_argument("--flash-ms", type=int, default=250,
                    help="period of the veil of the full case")
    ap.add_argument("--cases", default="full,incr1,incr2",
                    help="comma list of full, incr1, incr2")
    return ap


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.combine:
            if args.variant:
                raise Refused("--combine joins reports; it takes no --variant")
            report = combine(args.combine, args.paired)
        else:
            report = measure(args)
    except Refused as error:
        print(f"measure_latency: refused: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"measure_latency: cannot run the measurement: {error}", file=sys.stderr)
        return 2
    common.write_json(Path(args.out), report)
    show(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
