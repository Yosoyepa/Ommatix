#!/usr/bin/env python3
"""IT-07 / RNF-07: what ``ommatix`` sees at the scaling the monitor has right now. Read only.

The dossier's IT-07 asks for three things: the profile's scalings (100, 125 and 150 %), the
geometry epoch that follows a change of scaling, and an explicit refusal outside the profile.
This tool never changes the scaling and sends no input: the owner sets the scaling by hand
(Settings > System > Display > Scale) between sessions, and this tool records, per session:

* ``scaling``: the effective DPI of the primary monitor (read with the process per-monitor-v2
  DPI aware, as the engine's DXGI capture is), the percentage, the monitor's physical size and
  whether the percentage is in the profile;
* ``capture`` (unless ``--skip-capture-test``): the manual DXGI test of the capture adapter
  (``dxgi_captures_one_frame_of_output_0``: one frame in memory, never saved) and the shape it
  prints. Criteria: the frame has the monitor's physical size (frames and input share physical
  pixels, so there is nothing to scale) and the engine's ``DisplayMapping`` is the identity;
* ``observations``: ``--observations`` calls of ``observe_screen`` over ``ommatix serve
  --source dxgi`` while our own test application (``tools/s4/testapp``) is in front: whether
  each succeeded, the ``geometry_epoch`` (it must not move while nothing changes) and which of
  the test application's own fixed labels were found. Only counts of those fixed labels are
  kept; no other label, no frame and no coordinates are ever written;
* ``--watch-minutes N`` keeps observing every ``--interval`` seconds for N minutes while the
  owner changes the scaling by hand, and records the timeline of (scaling, epoch): the epoch
  must rise after every change and never fall. An observation is taken only while the test
  application is the foreground; the owner brings it back after the Settings window.

Outside the profile (a percentage other than 100, 125 or 150) the dossier asks for an explicit
refusal: the criterion ``out_of_profile`` is ``flagged`` only if every observation was refused
with ``UNSUPPORTED_GEOMETRY`` (any other refusal is ``refused_with_another_code``, with the code
in the observation). At the time of writing the engine refuses a rotated output and a format
that is not BGRA8 (as ``CAPTURE_UNAVAILABLE``) and has no check of the scaling itself, so
``served_unflagged`` is the outcome to expect there; the tool records what happened and
claims nothing.

``--combine a.json b.json ...`` joins the sessions of one campaign.

It refuses to start if the test application cannot be in front without input, if
``ommatix.exe`` or the test application is already running, or if the configuration could
deliver input. It ends only the processes it started.

Example (one session; the scaling is whatever it is now)::

    python tools/perf/dpi_probe.py --config %LOCALAPPDATA%\\Ommatix\\ommatix.s0.directml.toml ^
        --out dpi-probe-<percent>.json
"""
from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as common  # noqa: E402

PROFILE = (100, 125, 150)
BUTTON_LABELS = ("Sumar", "Restar", "Reiniciar", "Activar", "Siguiente", "Anterior")
FIELD_LABEL = "Campo de texto"
REFUSAL_CODE = "UNSUPPORTED_GEOMETRY"
CAPTURE_TEST = "dxgi_captures_one_frame_of_output_0"
CAPTURE_LINE = re.compile(
    r"dxgi output 0: (\d+)x(\d+) stride (\d+) epoch (\d+) mapping DisplayMapping \{ "
    r"origin_x: (-?\d+), origin_y: (-?\d+), frame_w: (\d+), frame_h: (\d+), "
    r"os_w: (\d+), os_h: (\d+) \}")
EPOCH_LAG = 3  # observations after a change of scaling within which the epoch must rise
EXIT_OK, EXIT_NOT_MET, EXIT_REFUSED = 0, 1, 2


# ---- pure checks --------------------------------------------------------------------------

def scaling_percent(dpi: int) -> int:
    """96 -> 100, 120 -> 125, 144 -> 150 (the effective DPI of a monitor over 96)."""
    return round(dpi * 100 / 96)


def parse_capture(output: str) -> dict | None:
    """The shape line printed by the manual DXGI test, or None."""
    match = CAPTURE_LINE.search(output)
    if not match:
        return None
    (w, h, stride, epoch, ox, oy, fw, fh, ow, oh) = (int(g) for g in match.groups())
    return {"width": w, "height": h, "stride": stride, "epoch": epoch,
            "mapping": {"origin": [ox, oy], "frame": [fw, fh], "os": [ow, oh]}}


def capture_checks(capture: dict | None, monitor: tuple[int, int]) -> dict:
    """Criteria on the captured frame against the monitor's physical size."""
    if capture is None:
        return {"frame_matches_monitor": None, "mapping_is_identity": None}
    mapping = capture["mapping"]
    return {
        "frame_matches_monitor": [capture["width"], capture["height"]] == list(monitor),
        "mapping_is_identity": mapping["frame"] == mapping["os"]
        and mapping["frame"] == [capture["width"], capture["height"]],
    }


def labels_found(elements: list[dict]) -> dict:
    """How many elements carry each of the test application's own fixed labels."""
    counts = {label: 0 for label in (*BUTTON_LABELS, FIELD_LABEL)}
    for element in elements:
        label = str(element.get("label", "")).strip()
        if label in counts:
            counts[label] += 1
    return counts


def check_epoch(timeline: list[dict]) -> dict:
    """The epoch against the scaling over a timeline of observed entries.

    A change of scaling is answered when the epoch rises from one observation before it to
    ``EPOCH_LAG`` observations after it (the scaling is read just before each observation, the
    capture notices it a moment either side). An unanswered change, a fall of the epoch and a
    rise that no change of scaling explains each make the check fail; a change too close to
    the end of the timeline to judge is counted apart and does not.
    """
    seen = [e for e in timeline if e.get("observed")]
    pairs = list(zip(seen, seen[1:], strict=False))
    changes = [i for i, (a, b) in enumerate(pairs, start=1) if a["percent"] != b["percent"]]
    rises = [i for i, (a, b) in enumerate(pairs, start=1) if b["epoch"] > a["epoch"]]
    decreases = sum(1 for a, b in pairs if b["epoch"] < a["epoch"])
    answered = unanswered = pending = 0
    for c in changes:
        if any(c - 1 <= r <= c + EPOCH_LAG for r in rises):
            answered += 1
        elif len(seen) - 1 < c + EPOCH_LAG:
            pending += 1
        else:
            unanswered += 1
    unexplained = sum(1 for r in rises if not any(c - 1 <= r <= c + EPOCH_LAG for c in changes))
    if unanswered or decreases or unexplained:
        met = False
    else:
        met = True if answered else None
    return {"changes_answered": answered, "changes_unanswered": unanswered,
            "changes_pending_at_end": pending, "decreases": decreases,
            "unexplained_increases": unexplained, "met": met}


def out_of_profile_result(percent: int, observations: list[dict]) -> str:
    """not_applicable inside the profile; flagged when every observation was refused with
    UNSUPPORTED_GEOMETRY; refused_with_another_code when it was refused, but not with that."""
    if percent in PROFILE:
        return "not_applicable"
    attempts = [o for o in observations if o.get("attempted")]
    if attempts and all(o.get("error_code") == REFUSAL_CODE for o in attempts):
        return "flagged"
    if any(o.get("observed") for o in attempts):
        return "served_unflagged"
    return "refused_with_another_code" if attempts else "no_observation"


def evaluate(record: dict) -> dict:
    """The IT-07 criteria of one session, each True, False or None (not measured)."""
    percent = record["scaling"]["percent"]
    observations = record["observations"]
    observed = [o for o in observations if o.get("observed")]
    in_profile = percent in PROFILE
    result = {
        "scaling_in_profile": in_profile,
        "frame_matches_monitor": record["capture_checks"]["frame_matches_monitor"],
        "mapping_is_identity": record["capture_checks"]["mapping_is_identity"],
        "epoch_stable_without_change": (
            None if not observed else len({o["epoch"] for o in observed}) == 1),
        "observations_succeeded": (
            None if not observations else all(o.get("observed") for o in observations)),
        "buttons_found": (
            None if not observed
            else all(observed[-1]["labels"][label] >= 1 for label in BUTTON_LABELS)),
        "out_of_profile": out_of_profile_result(percent, observations),
    }
    if record.get("epoch_timeline"):
        result["epoch_follows_scaling"] = record["epoch_timeline"]["check"]["met"]
    if in_profile:
        checks = [v for k, v in result.items() if k != "out_of_profile" and v is not None]
        result["met_all"] = bool(checks) and all(checks) and observed != []
    else:
        result["met_all"] = result["out_of_profile"] == "flagged"
    return result


def combine(records: list[dict]) -> dict:
    """One campaign: which of the profile's scalings have a complete session."""
    rows = []
    for r in records:
        rows.append({"percent": r["scaling"]["percent"], "in_profile": r["criteria"]["scaling_in_profile"],
                     "met_all": r["criteria"]["met_all"],
                     "epoch_follows_scaling": r["criteria"].get("epoch_follows_scaling"),
                     "out_of_profile": r["criteria"]["out_of_profile"]})
    complete = {row["percent"] for row in rows if row["in_profile"] and row["met_all"]}
    return {
        "sessions": rows,
        "profile_scalings_complete": sorted(complete),
        "profile_scalings_missing": sorted(set(PROFILE) - complete),
        "epoch_transition_checked": any(row["epoch_follows_scaling"] is True for row in rows),
        "out_of_profile_results": sorted({row["out_of_profile"] for row in rows
                                          if row["out_of_profile"] != "not_applicable"}),
    }


# ---- the real world -----------------------------------------------------------------------

class Host:
    """Everything the probe touches outside itself; the tests replace it with a fake."""

    def now(self) -> float:
        import time
        return time.monotonic()

    def sleep(self, seconds: float) -> None:
        import time
        time.sleep(seconds)

    def foreground_pid(self) -> int:
        return common.foreground_pid()

    def running(self, image: str) -> bool:
        return common.image_running(image)

    def start_testapp(self, args) -> subprocess.Popen:
        return subprocess.Popen([str(args.testapp)])

    def start_engine(self, args):
        return common.Engine([str(args.exe), "serve", "--source", "dxgi",
                              "--config", str(args.config), "--session-id", "dpi-probe"])

    def read_display(self) -> dict:
        """DPI and physical size of the primary monitor, with this process per-monitor-v2
        DPI aware (otherwise Windows would answer with virtualised values)."""
        import ctypes
        from ctypes import wintypes
        user32 = ctypes.WinDLL("user32")
        shcore = ctypes.WinDLL("shcore")
        user32.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
        user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # PER_MONITOR_AWARE_V2

        class Monitor(ctypes.Structure):
            _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT),
                        ("rcWork", wintypes.RECT), ("dwFlags", wintypes.DWORD)]

        user32.MonitorFromPoint.restype = wintypes.HANDLE
        user32.MonitorFromPoint.argtypes = [wintypes.POINT, wintypes.DWORD]
        monitor = user32.MonitorFromPoint(wintypes.POINT(0, 0), 1)  # MONITOR_DEFAULTTOPRIMARY
        info = Monitor(cbSize=ctypes.sizeof(Monitor))
        user32.GetMonitorInfoW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Monitor)]
        user32.GetMonitorInfoW(monitor, ctypes.byref(info))
        dpi_x, dpi_y = wintypes.UINT(0), wintypes.UINT(0)
        shcore.GetDpiForMonitor.argtypes = [wintypes.HANDLE, ctypes.c_int,
                                            ctypes.POINTER(wintypes.UINT),
                                            ctypes.POINTER(wintypes.UINT)]
        shcore.GetDpiForMonitor(monitor, 0, ctypes.byref(dpi_x), ctypes.byref(dpi_y))  # EFFECTIVE
        rect = info.rcMonitor
        return {"dpi": int(dpi_x.value), "percent": scaling_percent(int(dpi_x.value)),
                "monitor_px": [rect.right - rect.left, rect.bottom - rect.top],
                "monitor_origin": [rect.left, rect.top]}

    def run_capture_test(self) -> tuple[int, str]:
        done = subprocess.run(
            ["cargo", "test", "-p", "ommatix-adapter-capture", "--features", "dxgi", "--",
             "--ignored", "--nocapture", CAPTURE_TEST],
            cwd=common.REPO, capture_output=True, text=True, timeout=900, check=False)
        return done.returncode, done.stdout + done.stderr


# ---- the session --------------------------------------------------------------------------

class Probe:
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
                return f"{image} is already running"
        return None

    def hold_foreground(self, app_pid: int) -> bool:
        deadline = self.host.now() + self.args.foreground_wait
        while True:
            if self.host.foreground_pid() == app_pid:
                return True
            if self.host.now() >= deadline:
                return False
            self.host.sleep(0.5)

    def observe(self, engine, app_pid: int, t_s: float, percent: int) -> dict:
        """One read-only observation, kept only if the test application stayed in front."""
        entry = {"t_s": round(t_s, 2), "percent": percent, "attempted": False, "observed": False}
        if self.host.foreground_pid() != app_pid:
            entry["skipped"] = "foreground"
            return entry
        entry["attempted"] = True
        elements: list[dict] = []
        page, is_error, _, rtt = engine.call(
            "observe_screen", {"limit": 200}, timeout=self.args.call_timeout)
        if is_error:
            entry["error_code"] = page.get("code")
            return entry
        elements.extend(page.get("elements", []))
        while page.get("next_cursor"):
            page, is_error, _, _ = engine.call(
                "observe_screen", {"limit": 200, "cursor": page["next_cursor"]},
                timeout=self.args.call_timeout)
            if is_error:
                entry["error_code"] = page.get("code")
                return entry
            elements.extend(page.get("elements", []))
        if self.host.foreground_pid() != app_pid:
            entry["skipped"] = "foreground_changed"
            return entry
        entry.update(observed=True, epoch=page["geometry_epoch"], rtt_ms=round(rtt, 3),
                     total_elements=page.get("total_elements", len(elements)),
                     labels=labels_found(elements), degraded=bool(page.get("degraded")))
        return entry

    def run(self) -> tuple[dict, int]:
        a = self.args
        record: dict = {"tool": "tools/perf/dpi_probe.py", "requirement": "RNF-07 / IT-07",
                        "python": platform.python_version(), "platform": platform.platform(),
                        **common.source_state()}
        reason = self.refusal()
        if reason:
            record.update(ended="refused", reason=reason)
            return record, EXIT_REFUSED
        scaling = self.host.read_display()
        record["scaling"] = {**scaling, "in_profile": scaling["percent"] in PROFILE,
                             "profile": list(PROFILE)}
        record.update(observations=[], capture=None,
                      capture_checks=capture_checks(None, (0, 0)))
        app = engine = None
        try:
            app = self.host.start_testapp(a)
            if not self.hold_foreground(app.pid):
                record.update(ended="refused",
                              reason="the test application is not in the foreground and the "
                                     "tool does not use input to put it there")
                return record, EXIT_REFUSED
            if not a.skip_capture_test:
                code, output = self.host.run_capture_test()
                capture = parse_capture(output)
                record["capture"] = {"test_passed": code == 0, "shape": capture}
                record["capture_checks"] = capture_checks(
                    capture, tuple(scaling["monitor_px"]))
            engine = self.host.start_engine(a)
            start = self.host.now()
            for _ in range(a.observations):
                record["observations"].append(
                    self.observe(engine, app.pid, self.host.now() - start,
                                 self.host.read_display()["percent"]))
                self.host.sleep(a.interval)
            if a.watch_minutes:
                record["epoch_timeline"] = self.watch(
                    engine, app.pid, start, record["observations"])
            record["ended"] = "completed"
        except KeyboardInterrupt:
            record["ended"] = "interrupted"
        except (common.McpTimeout, OSError, RuntimeError) as error:
            record["ended"] = "engine_failed"
            record["reason"] = type(error).__name__  # never the message
        finally:
            if engine is not None:
                engine.close()
            if app is not None:
                app.terminate()  # only the process this tool started
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
        record["criteria"] = evaluate(record)
        return record, EXIT_OK if record["criteria"]["met_all"] and record["ended"] == "completed" \
            else EXIT_NOT_MET

    def watch(self, engine, app_pid: int, start: float, prior: list[dict]) -> dict:
        """Observes while the owner changes the scaling; polls the scaling every tick. The
        check runs over the session's earlier observations and the watch together."""
        a = self.args
        end = self.host.now() + a.watch_minutes * 60
        timeline = []
        while self.host.now() < end:
            percent = self.host.read_display()["percent"]
            timeline.append(self.observe(engine, app_pid, self.host.now() - start, percent))
            self.host.sleep(a.interval)
        trimmed = [{k: v for k, v in e.items() if k != "labels"} for e in timeline]
        return {"entries": trimmed, "skipped_foreground": sum(
                    1 for e in timeline if e.get("skipped")),
                "check": check_epoch(prior + timeline)}


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--combine", nargs="+", type=Path, metavar="SESSION.json",
                    help="join the sessions of one campaign instead of probing")
    ap.add_argument("--config", type=Path, help="the operator configuration (input free)")
    ap.add_argument("--exe", type=Path, default=common.DEFAULT_EXE)
    ap.add_argument("--testapp", type=Path, default=common.TESTAPP)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--observations", type=int, default=5)
    ap.add_argument("--interval", type=float, default=2.0)
    ap.add_argument("--watch-minutes", type=float, default=0.0,
                    help="keep observing for this long while you change the scaling by hand")
    ap.add_argument("--skip-capture-test", action="store_true",
                    help="do not run the manual DXGI capture test")
    ap.add_argument("--foreground-wait", type=float, default=15.0)
    ap.add_argument("--call-timeout", type=float, default=120.0)
    args = ap.parse_args(argv)
    if not args.combine and not args.config:
        ap.error("--config is required unless --combine is given")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.combine:
        records = [json.loads(p.read_text(encoding="utf-8")) for p in args.combine]
        document = combine([r for r in records if "criteria" in r])
        common.write_json(args.out, document)
        print(json.dumps(document, indent=2))
        return EXIT_OK if not document["profile_scalings_missing"] else EXIT_NOT_MET
    record, code = Probe(args, Host()).run()
    common.write_json(args.out, record)
    print(json.dumps({k: record.get(k) for k in ("ended", "reason", "scaling", "criteria")},
                     indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
