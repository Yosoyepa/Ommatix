#!/usr/bin/env python3
"""PERF-03 / RNF-06: resources and soak of ``ommatix serve`` observing the test application.

Sends no input of any kind and calls only ``observe_screen`` and ``get_engine_metrics``. It
starts our own test application (``tools/s4/testapp``) with its hidden ``--animate`` flags, so
that regions keep changing by themselves, starts ``ommatix serve --source dxgi`` with the
operator's configuration (models loaded: the weights count, ADR-0010) and, every ``--interval``
seconds for ``--minutes`` (default 120, the soak of RNF-06), samples:

* the working set, the private bytes and the handle count of the ``ommatix`` process;
* the dedicated GPU memory of that process, when the Windows performance counters give it
  (read only; recorded as unavailable, with the reason, when they do not);
* ``get_engine_metrics``: observations, ``pending_regions`` (the queue), rejected and delivered
  actions (delivered must stay 0);
* the round trip of ``observe_screen`` and the engine's own total from ``_meta``.

The report is JSON with no screen data: no frame, no element label, no coordinates. It carries
the raw samples, the latency percentiles per window of time, and for each resource the first
and last medians and the least-squares slope per hour after the warm-up. Its ``criteria`` judge
the working-set slope and the handle slope against the owner's thresholds of 15 October 2026
(dossier chapter 09, section 9.7: 10 MiB per hour, and no growth), which are the defaults of
``--max-rss-growth-mib-per-hour`` and ``--max-handle-growth-per-hour``. The queue has the
owner's bound of 17 October 2026 (dossier chapter 06, decision 9): a structural one, the tile
count of the current frame, ``ceil(W/64) * ceil(H/64)``, which is 510 for a 1920x1080 frame.
``--max-pending-regions`` has no default, since the bound depends on the frame: for a 1920x1080
frame, pass ``--max-pending-regions 510``. Its criterion appears only when the flag is given. The
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
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as common  # noqa: E402

MIB = 1024 * 1024
EXIT_OK, EXIT_ENDED_EARLY, EXIT_REFUSED = 0, 1, 2


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


def criteria(report_resources: dict, pending: dict, args) -> dict:
    """The thresholds the owner set, each with the value measured against it. The working-set
    and handle thresholds always apply; the queue only when it has a bound."""
    result = {}
    slope = report_resources["working_set_mib"].get("slope_per_hour")
    result["working_set_growth_mib_per_hour"] = {
        "limit": args.max_rss_growth_mib_per_hour, "measured": slope,
        "met": slope is not None and slope <= args.max_rss_growth_mib_per_hour}
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
        return subprocess.Popen([str(args.testapp), "--animate", str(args.animate_ms),
                                 "--animate-labels", str(args.animate_labels)])

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
        problem = common.config_problem(Path(a.config).read_text(encoding="utf-8"))
        if problem:
            return f"input-free configuration required: {problem}"
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
        samples: list[dict] = []
        counts = {"ticks": 0, "observed": 0, "foreground_lost": 0, "observe_errors": 0,
                  "discarded_latencies": 0}
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
            report["ended"] = self.loop(app, engine, gpu, samples, counts)
        except KeyboardInterrupt:
            report["ended"] = "interrupted"
        except (common.McpTimeout, OSError, RuntimeError) as error:
            report["ended"] = "engine_failed"
            report["reason"] = type(error).__name__  # never the message: it may hold screen text
        finally:
            if gpu is not None:
                gpu.close()
            if engine is not None:
                engine.close()
            if app is not None:
                app.terminate()  # only the process this tool started
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
        self.summarise(report, samples)
        return report, EXIT_OK if report["ended"] == "completed" else EXIT_ENDED_EARLY

    def loop(self, app, engine, gpu, samples: list[dict], counts: dict) -> str:
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
                sample["observed"] = False
                samples.append(sample)
                continue
            if is_error:
                counts["observe_errors"] += 1
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
                degraded=bool(page.get("degraded")), epoch=page.get("geometry_epoch"),
                observations=metrics.get("observations"),
                pending_regions=metrics.get("pending_regions"),
                rejected_actions=metrics.get("rejected_actions"),
                delivered_actions=metrics.get("delivered_actions"))
            samples.append(sample)
            if a.out and counts["observed"] % 60 == 0:
                self.checkpoint(samples, counts)

    def checkpoint(self, samples: list[dict], counts: dict) -> None:
        """A partial report, so that a crash at minute 100 keeps 100 minutes of data."""
        partial = {"ended": "running", "counts": counts, "samples": samples,
                   "minutes": self.args.minutes}
        common.write_json(Path(self.args.out), partial)

    def summarise(self, report: dict, samples: list[dict]) -> None:
        a = self.args
        total_s = samples[-1]["t_s"] if samples else 0
        warmup = min(a.warmup_minutes * 60, total_s * 0.1)
        report["warmup_s"] = round(warmup, 1)
        resources = {
            "working_set_mib": growth(samples, "rss", warmup, MIB),
            "private_mib": growth(samples, "private", warmup, MIB),
            "handles": growth(samples, "handles", warmup),
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
        report["criteria"] = criteria(resources, pending, a)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
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
                    help="samples before this are left out of the growth fit (at most a tenth "
                         "of the run)")
    ap.add_argument("--foreground-wait", type=float, default=15.0)
    ap.add_argument("--max-foreground-losses", type=int, default=3)
    ap.add_argument("--max-observe-errors", type=int, default=3)
    ap.add_argument("--call-timeout", type=float, default=120.0)
    ap.add_argument("--max-rss-growth-mib-per-hour", type=float, default=10.0,
                    help="threshold for the slope of the working set (RSS) after the warm-up, "
                         "in MiB per hour (default 10: the owner's, 15 October 2026)")
    ap.add_argument("--max-handle-growth-per-hour", type=float, default=0.0,
                    help="threshold for the slope of the handle count after the warm-up "
                         "(default 0: no growth; the owner's, 15 October 2026)")
    ap.add_argument("--max-pending-regions", type=int, default=None,
                    help="the owner's structural bound for the queue (pending_regions): the tile "
                         "count of the frame, ceil(W/64) * ceil(H/64), 510 for 1920x1080 "
                         "(dossier chapter 06, decision 9); no default, it depends on the frame")
    args = ap.parse_args(argv)
    if args.minutes <= 0 or args.interval <= 0:
        ap.error("--minutes and --interval must be positive")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    report, code = Soak(args, Host()).run()
    common.write_json(args.out, report)
    summary = {k: report.get(k) for k in ("ended", "reason", "counts", "criteria")}
    print(json.dumps(summary, indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
