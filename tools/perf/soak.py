#!/usr/bin/env python3
"""PERF-03 / RNF-06: resources and soak of ``ommatix serve`` observing the test application.

Sends no input of any kind and calls only ``observe_screen`` and ``get_engine_metrics``. It
starts our own test application (``tools/s4/testapp``) with its hidden ``--animate`` flags, so
that regions keep changing by themselves, starts ``ommatix serve --source dxgi`` with the
operator's configuration (models loaded: the weights count, ADR-0010) and, every ``--interval``
seconds for ``--minutes`` (default 120, the soak of RNF-06), samples:

* the working set, the private bytes, the handle and thread counts of the ``ommatix`` process;
* the dedicated GPU memory of that process, when the Windows performance counters give it
  (read only; recorded as unavailable, with the reason, when they do not);
* ``get_engine_metrics``: observations, ``pending_regions`` (the queue), rejected and delivered
  actions (delivered must stay 0);
* the round trip of ``observe_screen`` and the engine's own total from ``_meta``.

With ``--icon-strip`` the test application also paints its hidden strip of eight text-less
synthetic icons (dossier S0 §19), so that the icon cache has candidates; ``--icon-probe-every N``
adds, every N ticks, one more ``observe_screen`` (limit 200, outside the latency figures) that
follows the cursor of the same snapshot (at most ``PROBE_PAGES_MAX`` pages) and only *counts* the
elements by label source, by the closed synthetic vocabulary of the strip and as resolver-eligible
candidates, never keeping a label or a coordinate. A probe that failed, was skipped or stayed
truncated is evidence that is missing, and it is counted as such. ``--expect-cache`` judges (S0
§19.4, C5, as amended) that the engine started with curated prototypes, that at least 95 % of the
*scheduled* probes were complete and saw a candidate, that no probe, complete or not, saw an
accepted label (the measurement configuration accepts nothing), and that at least 95 % of the
observations were full refreshes by the engine's own plan record, which is what makes it
re-describe every candidate. Without these flags the soak is the one of S0 §18.

The report is JSON with no screen data: no frame, no element label, no coordinates. It carries
the raw samples, the latency percentiles per window of time, and for each resource the first
and last medians and the least-squares slope per hour after the warm-up. ``handle_steps`` lists
each change of the handle count after the warm-up with the thread count before and after it.
Its ``criteria`` judge, each with the value measured against it:

* the working-set slope per hour after the warm-up, against 10 MiB per hour (the owner's
  threshold of 15 October 2026, dossier chapter 09, section 9.7; the default of
  ``--max-rss-growth-mib-per-hour``);
* the handle count (``handle_count``), against the owner's rule of 17 October 2026 (the same
  section): after the warm-up, at most one change of the count, and that change exactly +1, and
  the count constant during the last 60 minutes of the run. The warm-up is the configured one
  (``--warmup-minutes``, 10 by default), so a run shorter than that warm-up plus 60 minutes
  cannot meet the rule, and neither can a run that did not complete;
* the handle slope per hour, only when ``--max-handle-growth-per-hour`` is given (0 is the
  no-growth rule of 15 October 2026); it has no default;
* the queue (``pending_regions``), only when ``--max-pending-regions`` is given: the owner's
  bound of 17 October 2026 (dossier chapter 06, decision 9), a structural one, the tile count of
  the current frame, ``ceil(W/64) * ceil(H/64)``, which is 510 for a 1920x1080 frame. It has no
  default, since the bound depends on the frame: for a 1920x1080 frame, pass
  ``--max-pending-regions 510``.

The handle slope is reported in ``resources`` without criterion unless its flag is given. The
tool states no verdict on RNF-06 as a whole; the reading is the owner's.

What it will not do, by construction:

* it never brings a window to the front. The test application opens maximised and in front; if
  it is not the foreground within ``--foreground-wait`` seconds the tool refuses to start
  (exit 2). During the run an observation is taken only while the test application is the
  foreground (the screen would otherwise show something nobody authorised); ticks where it is
  not are counted, and ``--max-foreground-losses`` consecutive ones end the run;
* it refuses an ``ommatix.exe`` or ``ommatix-testapp.exe`` already running (the numbers would
  not be the engine's alone) and an operator configuration that could deliver input;
* it ends only the processes it started.

Example (the supervised session, from the repository root, after ``cargo build --release -p
ommatix --features dxgi,directml,winrt-ocr,uia`` and ``cargo build --release`` in
``tools/s4/testapp``)::

    python tools/perf/soak.py --config %LOCALAPPDATA%\\Ommatix\\ommatix.rnf05.directml.toml ^
        --minutes 120 --out soak-120.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
import sys
import time
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as common  # noqa: E402

MIB = 1024 * 1024
EXIT_OK, EXIT_ENDED_EARLY, EXIT_REFUSED = 0, 1, 2

# The closed vocabulary of the strip's synthetic labels (tools/s4/icon_spec.rs), and the share of
# probes that must see a candidate for the icon cache to count as exercised (S0 §19.4, C5).
SYNTHETIC_LABEL_PREFIX = "cache-icon-"
PROBE_LIMIT = 200
#: Pages of one snapshot a probe follows before it gives up; beyond that it is incomplete.
PROBE_PAGES_MAX = 6
#: The least share (of the scheduled probes, and of the observations) that C5 asks for.
PROBE_SHARE_MIN = 0.95

_NOTE = re.compile(r"\[cache\] (\d+) curated icon prototypes, (\d+) for context")
_NO_NOTE = "[cache] no curated icon prototypes are configured"


# ---- analysis (pure) ----------------------------------------------------------------------


def _error_code(page) -> str | None:
    """The code of a tool error: the contract puts it under `error` (chapter 07 §7.7,
    `tool_error.output.schema.json`); None when the reply has none."""
    if not isinstance(page, dict):
        return None
    error = page.get("error")
    return error.get("code") if isinstance(error, dict) else None

def slope_per_hour(points: list[tuple[float, float]]) -> float | None:
    """Least-squares slope of (seconds, value) points, per hour; None without a trend to fit."""
    if len(points) < 2:
        return None
    mean_t = sum(t for t, _ in points) / len(points)
    mean_v = sum(v for _, v in points) / len(points)
    variance = sum((t - mean_t) ** 2 for t, _ in points)
    if variance == 0:
        return None
    covariance = sum((t - mean_t) * (v - mean_v) for t, v in points)
    return covariance / variance * 3600


def growth(samples: list[dict], key: str, warmup_s: float, scale: float = 1.0) -> dict:
    """First, last and slope of ``key`` after the warm-up (values divided by ``scale``).

    ``first`` and ``last`` are the medians of the first and last tenth of the samples after
    the warm-up, so that one noisy sample is neither the beginning nor the end.
    """
    points = [(s["t_s"], s[key] / scale) for s in samples
              if s.get(key) is not None and s["t_s"] >= warmup_s]
    if len(points) < 2:
        return {"n": len(points)}
    tenth = max(1, len(points) // 10)
    values = [v for _, v in points]
    first = common.summary(values[:tenth])["median"]
    last = common.summary(values[-tenth:])["median"]
    slope = slope_per_hour(points)
    return {
        "n": len(points),
        "min": round(min(values), 3),
        "max": round(max(values), 3),
        "first_median": first,
        "last_median": last,
        "slope_per_hour": None if slope is None else round(slope, 4),
    }


def handle_steps(samples: list[dict], warmup_s: float) -> list[dict]:
    """Every change of the handle count after the warm-up, with the thread count before and
    after it. The input of ``handle_rule``; on its own it carries no criterion. A sample
    without a handle count is skipped, and a change is measured from the last count that was
    read."""
    steps = []
    previous = None
    for s in samples:
        if s.get("handles") is None or s["t_s"] < warmup_s:
            continue
        if previous is not None and s["handles"] != previous["handles"]:
            steps.append({"t_s": s["t_s"],
                          "handles_before": previous["handles"],
                          "handles_after": s["handles"],
                          "threads_before": previous.get("threads"),
                          "threads_after": s.get("threads")})
        previous = s
    return steps


HANDLE_RULE = ("after the warm-up, at most one change of the handle count, of +1, and the count "
               "constant during the last 60 minutes of the run (owner, 17 October 2026)")
FLAT_FINAL_S = 60 * 60


def handle_rule(samples: list[dict], warmup_s: float, completed: bool) -> dict:
    """The owner's handle rule (``HANDLE_RULE``), judged on the changes of ``handle_steps``.

    The run ends at its last sample, which must carry a handle count, so that the count is
    known at the end. A run that did not complete, or that is shorter than the warm-up plus 60
    minutes, cannot meet the rule. ``reason`` says why it is not met, and is None when it is.
    """
    steps = handle_steps(samples, warmup_s)
    readings = [s for s in samples if s["t_s"] >= warmup_s and s.get("handles") is not None]
    end_s = samples[-1]["t_s"] if samples else 0.0
    rule = {"rule": HANDLE_RULE, "warmup_s": round(warmup_s, 1),
            "handle_steps_after_warmup": len(steps),
            "handle_step_deltas": [s["handles_after"] - s["handles_before"] for s in steps],
            "flat_final_s": None, "met": False, "reason": None}
    if not completed:
        rule["reason"] = "the run did not complete its configured duration"
    elif end_s < warmup_s + FLAT_FINAL_S:
        rule["reason"] = "the run is shorter than the warm-up plus 60 minutes"
    elif len(readings) < 2:
        rule["reason"] = "fewer than two handle counts after the warm-up"
    elif samples[-1].get("handles") is None:
        rule["reason"] = "no handle count in the last sample"
    else:
        last_change_s = steps[-1]["t_s"] if steps else readings[0]["t_s"]
        flat_s = end_s - last_change_s  # unrounded: the comparison must see the exact final hour
        rule["flat_final_s"] = round(flat_s, 3)  # the sampler's resolution is milliseconds
        if len(steps) > 1:
            rule["reason"] = f"{len(steps)} changes of the handle count after the warm-up"
        elif steps and rule["handle_step_deltas"][0] != 1:
            rule["reason"] = "the change of the handle count is not +1"
        elif flat_s < FLAT_FINAL_S:
            rule["reason"] = "the handle count changed during the last 60 minutes"
        else:
            rule["met"] = True
    return rule


def latency_windows(points: list[tuple[float, float]], window_s: float) -> list[dict]:
    """p50/p95/p99/max of (seconds, ms) points in consecutive windows of ``window_s``."""
    windows: dict[int, list[float]] = {}
    for t, ms in points:
        windows.setdefault(int(t // window_s), []).append(ms)
    rows = []
    for index in sorted(windows):
        s = common.summary(windows[index])
        rows.append({"from_s": index * window_s, "n": s["n"], "p50": s["median"],
                     "p95": s["p95"], "p99": s["p99"], "max": s["max"]})
    return rows


def cache_note(lines: list[str]) -> dict | None:
    """The engine's start-up note about the icon cache, as numbers: the prototypes it loaded and
    how many belong to the configured context. None when it wrote no such note. Only the counts
    are kept; the context name is not."""
    for line in lines:
        match = _NOTE.search(line)
        if match:
            return {"prototypes": int(match.group(1)), "for_context": int(match.group(2))}
        if _NO_NOTE in line:
            return {"prototypes": 0, "for_context": 0}
    return None


#: Roles that the resolver never takes as icon candidates: text, and every role that takes an
#: external label or holds a value (``icons::is_candidate``, ``Role::takes_external_label`` and
#: ``Role::holds_value``).
NOT_CANDIDATE_ROLES = ("text_node", "input", "dropdown", "checkbox")


def is_candidate(element: dict) -> bool:
    """Whether the resolver may label this element: an ``icon_button``, or a role that takes no
    external label and holds no value (``button``, ``unknown``) with no inner label."""
    role = element.get("role")
    if role in NOT_CANDIDATE_ROLES:
        return False
    if role == "icon_button":
        return True
    return element.get("label_source", "unknown") == "unknown" and not element.get("label")


def probe_counts(page) -> dict | None:
    """What one probe page holds, as counts: elements, by label source, those labelled with the
    synthetic vocabulary, and the resolver-eligible candidates. No label, no coordinate and no
    identifier is kept. None for a page that is not a snapshot page."""
    if not isinstance(page, dict) or not isinstance(page.get("elements"), list):
        return None
    sources: dict[str, int] = {}
    vocabulary = candidates = 0
    for element in page["elements"]:
        if not isinstance(element, dict):
            continue
        source = str(element.get("label_source", "unknown"))
        sources[source] = sources.get(source, 0) + 1
        label = element.get("label")
        if isinstance(label, str) and label.startswith(SYNTHETIC_LABEL_PREFIX):
            vocabulary += 1
        if is_candidate(element):
            candidates += 1
    return {"elements": len(page["elements"]), "by_label_source": sources,
            "curated_icon": sources.get("curated_icon", 0), "vocabulary_labelled": vocabulary,
            "candidates": candidates, "truncated": bool(page.get("next_cursor"))}


def merge_counts(first: dict, second: dict) -> dict:
    """The counts of two pages of the same snapshot, added."""
    sources = dict(first["by_label_source"])
    for key, value in second["by_label_source"].items():
        sources[key] = sources.get(key, 0) + value
    return {"elements": first["elements"] + second["elements"], "by_label_source": sources,
            "curated_icon": first["curated_icon"] + second["curated_icon"],
            "vocabulary_labelled": first["vocabulary_labelled"] + second["vocabulary_labelled"],
            "candidates": first["candidates"] + second["candidates"],
            "truncated": second["truncated"]}


def probe_summary(probes: list[dict], every: int, scheduled: int | None = None) -> dict:
    """The counts of every recorded probe, summarised, with the coverage account: how many probes
    were scheduled, how many are complete, incomplete (a truncated snapshot or a failed page) or
    missing altogether (failed, or taken while the application was not in front)."""
    scheduled = len(probes) if scheduled is None else scheduled
    complete = [p for p in probes if p.get("complete", True)]
    summary = {"every_ticks": every, "scheduled": scheduled, "probes": len(probes),
               "complete": len(complete), "incomplete": len(probes) - len(complete),
               "skipped": scheduled - len(probes)}
    if probes:
        curated = [p["curated_icon"] for p in probes]
        summary.update(
            curated_icon={"min": min(curated), "max": max(curated),
                          "median": statistics.median(curated)},
            vocabulary_labelled_max=max(p["vocabulary_labelled"] for p in probes),
            candidates_max=max(p["candidates"] for p in probes),
            elements_max=max(p["elements"] for p in probes),
            truncated=sum(1 for p in probes if p["truncated"]))
    seen = sum(1 for p in complete if p["candidates"] + p["curated_icon"] >= 1)
    summary["with_candidate"] = seen
    if scheduled:
        summary["share_complete"] = round(len(complete) / scheduled, 4)
        summary["share_with_candidate"] = round(seen / scheduled, 4)
    return summary


def cache_criterion(note: dict | None, summary: dict, plan: dict | None = None) -> dict:
    """S0 §19.4, C5 (option C, as amended): the engine started with curated prototypes; of the
    probes *scheduled*, at least ``PROBE_SHARE_MIN`` are complete and saw a resolver-eligible
    candidate; no probe, complete or not, saw an accepted label (a label would mean that the
    construction of the configuration is wrong); and at least ``PROBE_SHARE_MIN`` of the
    observations were full refreshes by the engine's own plan record. A probe that failed or was
    cut short is missing evidence, never a pass."""
    plan = plan or {}
    scheduled = summary.get("scheduled", 0)
    complete = summary.get("complete", 0)
    share_complete = complete / scheduled if scheduled else None
    share_candidate = summary.get("with_candidate", 0) / scheduled if scheduled else None
    observations = sum(plan.values())
    share_full = plan.get("full_refresh", 0) / observations if observations else None
    loaded = note is not None and note["prototypes"] >= 1
    curated = summary.get("curated_icon", {}).get("max")
    accepted = None if curated is None else max(curated, summary.get("vocabulary_labelled_max", 0))

    def floor(share) -> bool:
        return share is not None and share >= PROBE_SHARE_MIN

    return {
        "rule": (f"curated prototypes loaded; of the scheduled probes >= {PROBE_SHARE_MIN:.0%} "
                 "complete and >= "
                 f"{PROBE_SHARE_MIN:.0%} with a resolver-eligible candidate; zero accepted labels "
                 f"in every probe, complete or not; >= {PROBE_SHARE_MIN:.0%} of the observations "
                 "full refreshes by the engine's plan record"),
        "cache_note": note, "scheduled": scheduled, "complete": complete,
        "incomplete": summary.get("incomplete", 0), "skipped": summary.get("skipped", 0),
        "share_complete": None if share_complete is None else round(share_complete, 4),
        "share_with_candidate": None if share_candidate is None else round(share_candidate, 4),
        "share_full_refresh": None if share_full is None else round(share_full, 4),
        "accepted_labels_max": accepted,
        "met": bool(loaded and scheduled >= 1 and floor(share_complete)
                    and floor(share_candidate) and accepted == 0 and floor(share_full))}


def cache_settings(config_text: str) -> dict | None:
    """The ``[cache]`` settings of the configuration that the report may carry (no path): the
    bound, the context, the thresholds and the pinned digest. None without a prototypes file."""
    try:
        cache = tomllib.loads(config_text).get("cache", {})
    except tomllib.TOMLDecodeError:
        return None
    if not cache.get("prototypes"):
        return None
    keys = ("max_entries", "context", "gradient_tau", "max_distance", "min_margin",
            "prototypes_sha256")
    return {key: cache.get(key) for key in keys}


def criteria(report_resources: dict, pending: dict, handle: dict, args) -> dict:
    """The thresholds the owner set, each with the value measured against it. The working-set
    threshold and the handle rule always apply; the handle slope only when its flag is given,
    and the queue only when it has a bound."""
    result = {}
    slope = report_resources["working_set_mib"].get("slope_per_hour")
    result["working_set_growth_mib_per_hour"] = {
        "limit": args.max_rss_growth_mib_per_hour, "measured": slope,
        "met": slope is not None and slope <= args.max_rss_growth_mib_per_hour}
    result["handle_count"] = handle
    if args.max_handle_growth_per_hour is not None:
        slope = report_resources["handles"].get("slope_per_hour")
        result["handle_growth_per_hour"] = {
            "limit": args.max_handle_growth_per_hour, "measured": slope,
            "met": slope is not None and slope <= args.max_handle_growth_per_hour}
    if args.max_pending_regions is not None:
        result["pending_regions_max"] = {
            "limit": args.max_pending_regions, "measured": pending.get("max"),
            "met": pending.get("max") is not None and pending["max"] <= args.max_pending_regions}
    return result


# ---- the real world -----------------------------------------------------------------------

class Host:
    """Everything the soak touches outside itself; the tests replace it with a fake."""

    def now(self) -> float:
        return time.monotonic()

    def sleep(self, seconds: float) -> None:
        time.sleep(seconds)

    def foreground_pid(self) -> int:
        return common.foreground_pid()

    def running(self, image: str) -> bool:
        return common.image_running(image)

    def counters(self, pid: int) -> dict | None:
        return common.process_counters(pid)

    def gpu(self, pid: int):
        return common.GpuMemory(pid)

    def start_testapp(self, args) -> subprocess.Popen:
        argv = [str(args.testapp), "--animate", str(args.animate_ms),
                "--animate-labels", str(args.animate_labels)]
        if getattr(args, "icon_strip", False):
            argv.append("--icon-strip")
        return subprocess.Popen(argv)

    def start_engine(self, args):
        return common.Engine([str(args.exe), "serve", "--source", "dxgi",
                              "--config", str(args.config), "--session-id", "soak"])


# ---- the run ------------------------------------------------------------------------------

class Soak:
    def __init__(self, args, host: Host):
        self.args = args
        self.host = host

    def refusal(self) -> str | None:
        a = self.args
        if not Path(a.config).is_file():
            return "the configuration file does not exist"
        text = Path(a.config).read_text(encoding="utf-8")
        problem = common.config_problem(text)
        if problem:
            return f"input-free configuration required: {problem}"
        if getattr(a, "expect_cache", False):
            if not getattr(a, "icon_probe_every", 0):
                return "--expect-cache needs --icon-probe-every"
            if cache_settings(text) is None:
                return "--expect-cache needs a [cache] block with a prototypes file"
        for image in ("ommatix.exe", "ommatix-testapp.exe"):
            if self.host.running(image):
                return f"{image} is already running; the soak measures its own processes only"
        return None

    def hold_foreground(self, app_pid: int) -> bool:
        """Waits for the test application to be in front by itself. Never activates it."""
        deadline = self.host.now() + self.args.foreground_wait
        while True:
            if self.host.foreground_pid() == app_pid:
                return True
            if self.host.now() >= deadline:
                return False
            self.host.sleep(0.5)

    def run(self) -> tuple[dict, int]:
        a = self.args
        report: dict = {
            "tool": "tools/perf/soak.py", "requirement": "RNF-06 / PERF-03",
            "minutes": a.minutes, "interval_s": a.interval, "animate_ms": a.animate_ms,
            "animate_labels": a.animate_labels, "window_s": a.window_minutes * 60,
            "icon_strip": bool(getattr(a, "icon_strip", False)),
            "python": platform.python_version(), "platform": platform.platform(),
            **common.source_state(),
        }
        reason = self.refusal()
        if reason:
            report.update(ended="refused", reason=reason)
            return report, EXIT_REFUSED
        for label, path in (("exe", a.exe), ("config", a.config)):
            if Path(path).is_file():
                report[f"{label}_sha256"] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
        report["cache_config"] = cache_settings(Path(a.config).read_text(encoding="utf-8"))
        samples: list[dict] = []
        counts = {"ticks": 0, "observed": 0, "foreground_lost": 0, "observe_errors": 0,
                  "discarded_latencies": 0}
        probes: list[dict] = []
        report.update(ended="running", counts=counts, samples=samples)
        app = engine = gpu = None
        try:
            app = self.host.start_testapp(a)
            if not self.hold_foreground(app.pid):
                report.update(ended="refused",
                              reason="the test application is not in the foreground and the "
                                     "tool does not use input to put it there")
                return report, EXIT_REFUSED
            engine = self.host.start_engine(a)
            report["engine_pid"] = engine.pid
            gpu = self.host.gpu(engine.pid)
            report["gpu_counter"] = {"available": gpu.reason is None, "reason": gpu.reason}
            report["ended"] = self.loop(app, engine, gpu, samples, counts, probes)
        except KeyboardInterrupt:
            report["ended"] = "interrupted"
        except (common.McpTimeout, OSError, RuntimeError) as error:
            report["ended"] = "engine_failed"
            report["reason"] = type(error).__name__  # never the message: it may hold screen text
        finally:
            if gpu is not None:
                gpu.close()
            if engine is not None:
                stderr_lines = getattr(engine, "stderr_lines", None)
                report["cache_note"] = cache_note(stderr_lines() if stderr_lines else [])
                engine.close()
            if app is not None:
                app.terminate()  # only the process this tool started
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
        self.summarise(report, samples, probes)
        return report, EXIT_OK if report["ended"] == "completed" else EXIT_ENDED_EARLY

    def probe(self, app, engine, probes: list[dict], counts: dict) -> None:
        """One extra observation that only counts what the snapshot holds (outside the latency
        figures), following the cursor of the same snapshot up to ``PROBE_PAGES_MAX`` pages. It
        is taken only while the test application is in front before and after it; a probe that
        could not be taken is counted as skipped (the loop counts it as scheduled), and one that stayed truncated or lost a page is
        recorded as incomplete (its counts still count: a label on any page fails C5)."""
        if self.host.foreground_pid() != app.pid:
            counts["probes_skipped"] += 1
            return
        merged = total = None
        pages, cursor, complete = 0, None, False
        while True:
            arguments = {"limit": PROBE_LIMIT}
            if cursor is not None:
                arguments["cursor"] = cursor
            page, is_error, _, _ = engine.call(
                "observe_screen", arguments, timeout=self.args.call_timeout)
            found = None if is_error else probe_counts(page)
            if found is None:
                break
            pages += 1
            merged = found if merged is None else merge_counts(merged, found)
            if total is None:
                total = page.get("total_elements")
            cursor = page.get("next_cursor")
            if not cursor:
                complete = True
                break
            if pages >= PROBE_PAGES_MAX:
                break
        if merged is None or self.host.foreground_pid() != app.pid:
            counts["probes_skipped"] += 1
            return
        merged["complete"] = complete and (total is None or merged["elements"] == total)
        merged["pages"] = pages
        probes.append(merged)

    def loop(self, app, engine, gpu, samples: list[dict], counts: dict,
             probes: list[dict] | None = None) -> str:
        a = self.args
        start = self.host.now()
        end = start + a.minutes * 60
        tick = start
        consecutive_lost = consecutive_errors = 0
        while True:
            now = self.host.now()
            if now >= end:
                return "completed"
            if tick > now:
                self.host.sleep(min(tick - now, end - now))
                continue
            tick += a.interval
            counts["ticks"] += 1
            every = getattr(a, "icon_probe_every", 0)
            due = bool(every and probes is not None and counts["ticks"] % every == 0)
            if due:  # scheduled whether or not it can be taken: a missing probe is missing evidence
                counts["probes_scheduled"] = counts.get("probes_scheduled", 0) + 1
                counts.setdefault("probes_skipped", 0)
            sample = {"t_s": round(now - start, 2)}
            if not engine.alive():
                return "engine_exited"
            counters = self.host.counters(engine.pid)
            if counters is None:
                return "engine_exited"
            sample.update(counters)
            sample["gpu_dedicated"] = gpu.read()
            in_front = self.host.foreground_pid() == app.pid
            if not in_front:
                counts["foreground_lost"] += 1
                if due:
                    counts["probes_skipped"] += 1
                consecutive_lost += 1
                sample["observed"] = False
                samples.append(sample)
                if consecutive_lost >= a.max_foreground_losses:
                    return "foreground_lost"
                continue
            consecutive_lost = 0
            page, is_error, meta, rtt = engine.call(
                "observe_screen", {"limit": 1}, timeout=a.call_timeout)
            if self.host.foreground_pid() != app.pid:
                # The screen changed under the observation: its latency is not comparable.
                counts["discarded_latencies"] += 1
                if due:
                    counts["probes_skipped"] += 1
                sample["observed"] = False
                samples.append(sample)
                continue
            if is_error:
                counts["observe_errors"] += 1
                if due:
                    counts["probes_skipped"] += 1
                consecutive_errors += 1
                sample.update(observed=False, error_code=_error_code(page))
                samples.append(sample)
                if consecutive_errors >= a.max_observe_errors:
                    return "observe_errors"
                continue
            consecutive_errors = 0
            counts["observed"] += 1
            stages = common.stage_ms(meta)
            metrics, _, _, _ = engine.call("get_engine_metrics", {}, timeout=a.call_timeout)
            sample.update(
                observed=True, rtt_ms=round(rtt, 3), total_ms=stages.get("total"),
                plan_work=common.plan_work(meta) or "not_reported",
                degraded=bool(page.get("degraded")), epoch=page.get("geometry_epoch"),
                observations=metrics.get("observations"),
                pending_regions=metrics.get("pending_regions"),
                rejected_actions=metrics.get("rejected_actions"),
                delivered_actions=metrics.get("delivered_actions"))
            samples.append(sample)
            if due:
                self.probe(app, engine, probes, counts)
            if a.out and counts["observed"] % 60 == 0:
                self.checkpoint(samples, counts)

    def checkpoint(self, samples: list[dict], counts: dict) -> None:
        """A partial report, so that a crash at minute 100 keeps 100 minutes of data."""
        partial = {"ended": "running", "counts": counts, "samples": samples,
                   "minutes": self.args.minutes}
        common.write_json(Path(self.args.out), partial)

    def summarise(self, report: dict, samples: list[dict],
                  probes: list[dict] | None = None) -> None:
        a = self.args
        total_s = samples[-1]["t_s"] if samples else 0
        warmup = min(a.warmup_minutes * 60, total_s * 0.1)
        report["warmup_s"] = round(warmup, 1)
        resources = {
            "working_set_mib": growth(samples, "rss", warmup, MIB),
            "private_mib": growth(samples, "private", warmup, MIB),
            "handles": growth(samples, "handles", warmup),
            "threads": growth(samples, "threads", warmup),
            "gpu_dedicated_mib": growth(samples, "gpu_dedicated", warmup, MIB),
        }
        observed = [s for s in samples if s.get("observed")]
        pending_values = [s["pending_regions"] for s in observed
                          if s.get("pending_regions") is not None]
        pending = {"n": len(pending_values)}
        if pending_values:
            pending.update(max=max(pending_values), last=pending_values[-1],
                           zero_samples=sum(1 for v in pending_values if v == 0))
        report["resources"] = resources
        report["pending_regions"] = pending
        handle_warmup = a.warmup_minutes * 60  # the owner's rule takes the warm-up as configured
        report["handle_steps"] = handle_steps(samples, handle_warmup)
        report["delivered_actions_max"] = max(
            (s["delivered_actions"] for s in observed if s.get("delivered_actions") is not None),
            default=None)
        window_s = a.window_minutes * 60
        report["latency_ms"] = {
            "rtt": {**common.summary([s["rtt_ms"] for s in observed]),
                    "windows": latency_windows([(s["t_s"], s["rtt_ms"]) for s in observed],
                                               window_s)},
            "engine_total": {
                **common.summary([s["total_ms"] for s in observed if s.get("total_ms")]),
                "windows": latency_windows(
                    [(s["t_s"], s["total_ms"]) for s in observed if s.get("total_ms")],
                    window_s)},
        }
        rule = handle_rule(samples, handle_warmup, report["ended"] == "completed")
        report["criteria"] = criteria(resources, pending, rule, a)
        every = getattr(a, "icon_probe_every", 0)
        if every:
            plan_work: dict[str, int] = {}
            for sample in observed:
                kind = sample.get("plan_work", "not_reported")
                plan_work[kind] = plan_work.get(kind, 0) + 1
            report["plan_work"] = plan_work
            report["icon_probe"] = probe_summary(
                probes or [], every, report["counts"].get("probes_scheduled", 0))
            if getattr(a, "expect_cache", False):
                report["criteria"]["icon_cache_exercised"] = cache_criterion(
                    report.get("cache_note"), report["icon_probe"], plan_work)


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--config", required=True, type=Path,
                    help="the operator configuration (models loaded; must be input free)")
    ap.add_argument("--exe", type=Path, default=common.DEFAULT_EXE)
    ap.add_argument("--testapp", type=Path, default=common.TESTAPP)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--minutes", type=float, default=120.0,
                    help="duration of the soak (default 120, RNF-06); a shorter value is a dry run")
    ap.add_argument("--interval", type=float, default=5.0, help="seconds between samples")
    ap.add_argument("--animate-ms", type=int, default=100)
    ap.add_argument("--animate-labels", type=int, choices=(1, 2), default=2)
    ap.add_argument("--window-minutes", type=float, default=10.0,
                    help="width of the windows of the latency percentiles over time")
    ap.add_argument("--warmup-minutes", type=float, default=10.0,
                    help="warm-up in minutes: the growth fits leave out at most a tenth of the "
                         "run; the handle rule of 17 October 2026 always leaves out this many "
                         "minutes")
    ap.add_argument("--foreground-wait", type=float, default=15.0)
    ap.add_argument("--max-foreground-losses", type=int, default=3)
    ap.add_argument("--max-observe-errors", type=int, default=3)
    ap.add_argument("--call-timeout", type=float, default=120.0)
    ap.add_argument("--max-rss-growth-mib-per-hour", type=float, default=10.0,
                    help="threshold for the slope of the working set (RSS) after the warm-up, "
                         "in MiB per hour (default 10: the owner's, 15 October 2026)")
    ap.add_argument("--max-handle-growth-per-hour", type=float, default=None,
                    help="optional extra criterion: at most this slope of the handle count after "
                         "the warm-up, per hour (no default: the owner's handle rule of 17 "
                         "October 2026 is criteria.handle_count; 0 is the no-growth rule of 15 "
                         "October 2026)")
    ap.add_argument("--icon-strip", action="store_true",
                    help="start the test application with its hidden strip of eight synthetic "
                         "text-less icons (dossier S0 section 19); AG-02 never uses it")
    ap.add_argument("--icon-probe-every", type=int, default=0, metavar="N",
                    help="every N ticks, one extra observe_screen (limit 200, outside the "
                         "latencies) that only counts elements by label source and by the "
                         "strip's synthetic vocabulary; 0 (default) is off")
    ap.add_argument("--expect-cache", action="store_true",
                    help="judge C5 of S0 section 19.4 (as amended): curated prototypes loaded; "
                         "of the scheduled probes >= 95%% complete and with a resolver-eligible "
                         "candidate; zero accepted labels in every probe, complete or not; >= "
                         "95%% of the observations full refreshes by the engine's plan record "
                         "(needs --icon-probe-every and a [cache] block; the configuration "
                         "accepts nothing)")
    ap.add_argument("--max-pending-regions", type=int, default=None,
                    help="the owner's structural bound for the queue (pending_regions): the tile "
                         "count of the frame, ceil(W/64) * ceil(H/64), 510 for 1920x1080 "
                         "(dossier chapter 06, decision 9); no default, it depends on the frame")
    return ap


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    ap = build_parser()
    args = ap.parse_args(argv)
    if args.minutes <= 0 or args.interval <= 0:
        ap.error("--minutes and --interval must be positive")
    if args.icon_probe_every < 0:
        ap.error("--icon-probe-every must not be negative")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    report, code = Soak(args, Host()).run()
    common.write_json(args.out, report)
    summary = {k: report.get(k)
               for k in ("ended", "reason", "counts", "criteria", "icon_probe", "plan_work", "cache_note")}
    print(json.dumps(summary, indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
