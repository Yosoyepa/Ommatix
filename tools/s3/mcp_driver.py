#!/usr/bin/env python3
"""S3 supervised runs: a minimal MCP 2026-07-28 client for ``ommatix serve``.

It speaks JSON-RPC over the child process's stdin/stdout, one message per line, exactly
as an agent's MCP client would, and prints only statuses, error codes, counts, frame
numbers and timings. Element labels are held in memory to select a target and are never
printed or written anywhere (AGENTS.md rule 7, ADR-0012); nothing here saves a frame.

Live input goes only through the engine's gate (``execute_action`` with
``dry_run: false``); this script never injects input itself (for ``sec05`` it only starts
the Calculator, asks Windows to activate its window and closes it afterwards, if it was not
already running). The operator's stop is the
``[safety] stop_file`` of the configuration: the script creates it when anything
unexpected happens (the foreground application is not the expected one, no unique menu
label, a budget overrun) and then ends the run.

Target rule: only an element whose label, trimmed, equals one of ``--menu-labels``
(default the menu bar names ``Archivo``/``File``, ``Editar``/``Edit``, ``Ver``/``View``),
that is unique in its snapshot and ``confirmed``, is ever acted on; the first allowed label
with exactly one match is chosen and kept for the whole run. The only actions sent are
clicks on it. Windows only (the foreground checks use
user32/kernel32 through ctypes, read-only).

Scenarios (see docs/thesis/S3_CORRIDAS_SUPERVISADAS.md):

``observe`` start a server without live input and only observe (counts and timings).
``dry``   start a server without live input, observe a few times (IT-01: frame_seq,
          geometry epoch, first-frame behaviour), run one dry-run click and one
          ``dry_run: false`` click that the policy must refuse (SEC-01).
``live``  start a live server (``--input sendinput``) and run, in order and each only if
          requested with ``--steps``: ``close`` (one click on the menu label, to close a
          menu an earlier run left open), ``it03`` (open the menu with a live click, observe,
          close it with a second click), ``it05`` (repeat the closing request_id; a
          conflicting reuse), ``sec02`` (stop file engaged before a live click),
          ``sec05`` (another application brought to the foreground between observation
          and action).

Example (from the repository root, PowerShell)::

    python tools/s3/mcp_driver.py --exe target\\release\\ommatix.exe ^
        --config <scratch>\\live.toml --scenario live --steps it03,it05,sec02,sec05 ^
        --max-live 8
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import subprocess
import sys
import threading
import time
import tomllib
from collections import Counter
from ctypes import wintypes
from pathlib import Path

PROTOCOL = "2026-07-28"
META = {
    "io.modelcontextprotocol/protocolVersion": PROTOCOL,
    "io.modelcontextprotocol/clientCapabilities": {},
    "io.modelcontextprotocol/clientInfo": {"name": "ommatix-s3-driver", "version": "1.0"},
}
TIMINGS_KEY = "io.ommatix/timings"
PAGE_LIMIT = 50
CREATE_NO_WINDOW = 0x08000000


class Abort(Exception):
    """Something unexpected: the operator's stop is engaged and the run ends."""


# ---- read-only Windows checks -------------------------------------------------------------

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.MonitorFromWindow.argtypes = [wintypes.HWND, wintypes.DWORD]
user32.MonitorFromWindow.restype = wintypes.HANDLE
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.QueryFullProcessImageNameW.argtypes = [
    wintypes.HANDLE,
    wintypes.DWORD,
    wintypes.LPWSTR,
    ctypes.POINTER(wintypes.DWORD),
]


class MONITORINFO(ctypes.Structure):
    _fields_ = [
        ("cbSize", wintypes.DWORD),
        ("rcMonitor", wintypes.RECT),
        ("rcWork", wintypes.RECT),
        ("dwFlags", wintypes.DWORD),
    ]


user32.GetMonitorInfoW.argtypes = [wintypes.HANDLE, ctypes.POINTER(MONITORINFO)]


def foreground() -> tuple[str | None, bool]:
    """The foreground window's executable name (lowercase) and whether the window is
    maximized on the primary monitor (so no other window's menu bar shows on output 0). No
    window text is read."""
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return None, False
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    handle = kernel32.OpenProcess(0x1000, False, pid.value)  # QUERY_LIMITED_INFORMATION
    name = None
    if handle:
        buf = ctypes.create_unicode_buffer(32768)
        size = wintypes.DWORD(len(buf))
        if kernel32.QueryFullProcessImageNameW(handle, 0, buf, ctypes.byref(size)):
            name = os.path.basename(buf.value).lower()
        kernel32.CloseHandle(handle)
    monitor = user32.MonitorFromWindow(hwnd, 0)  # MONITOR_DEFAULTTONULL
    info = MONITORINFO(cbSize=ctypes.sizeof(MONITORINFO))
    primary = bool(monitor) and bool(user32.GetMonitorInfoW(monitor, ctypes.byref(info)))
    on_primary = primary and bool(info.dwFlags & 1)  # MONITORINFOF_PRIMARY
    return name, on_primary and bool(user32.IsZoomed(hwnd))


def process_running(image: str) -> bool:
    out = subprocess.run(
        ["tasklist", "/FI", f"IMAGENAME eq {image}", "/NH"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    return image.lower() in out.lower()


# ---- MCP client ---------------------------------------------------------------------------


class Server:
    """``ommatix serve`` as a child process; stderr is kept in memory only."""

    def __init__(self, argv: list[str]):
        self.proc = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1,
            creationflags=CREATE_NO_WINDOW,
        )
        self.stderr: list[str] = []
        self.next_id = 1
        threading.Thread(target=self._drain, daemon=True).start()

    def _drain(self) -> None:
        for line in self.proc.stderr:
            self.stderr.append(line.rstrip("\n"))

    def request(self, method: str, params: dict) -> dict:
        rid = self.next_id
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": rid, "method": method, "params": {"_meta": META, **params}}
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise Abort(f"the server closed stdout (exit {self.proc.poll()})")
        reply = json.loads(line)
        if reply.get("id") != rid:
            raise Abort("reply id mismatch")
        if "error" in reply:
            raise Abort(f"JSON-RPC error {reply['error'].get('code')}")
        return reply["result"]

    def call(self, name: str, arguments: dict) -> tuple[dict, bool, dict, float]:
        start = time.perf_counter()
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        elapsed = (time.perf_counter() - start) * 1000
        return (
            result.get("structuredContent", {}),
            bool(result.get("isError")),
            result.get("_meta", {}),
            elapsed,
        )

    def close(self) -> int:
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            return self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            return self.proc.wait()


# ---- run -----------------------------------------------------------------------------------


def log(event: str, **fields) -> None:
    print(json.dumps({"t": round(time.monotonic(), 3), "event": event, **fields}, ensure_ascii=False), flush=True)


class Run:
    def __init__(self, args: argparse.Namespace, server: Server, stop_file: Path | None):
        self.args = args
        self.srv = server
        self.stop_file = stop_file
        self.session = args.session_id
        self.live_sent = 0
        self.menu_labels = tuple(x.strip() for x in args.menu_labels.split(","))
        self.chosen: str | None = None

    # -- safety
    def engage_stop(self, reason: str) -> None:
        if self.stop_file is not None:
            self.stop_file.write_text("", encoding="utf-8")
        log("operator_stop_engaged", reason=reason, stop_file_set=self.stop_file is not None)
        raise Abort(reason)

    def require_foreground(self, expected: str) -> None:
        name, maximized = foreground()
        # The allowlisted application must be maximized on output 0; another expected
        # foreground (SEC-05's calculator) only needs to be the foreground.
        if name != expected or (expected == "notepad.exe" and not maximized):
            self.engage_stop(f"foreground is not {expected} maximized on the primary monitor (got {'other' if name else 'none'})")

    # -- tools
    def metrics(self, tag: str) -> dict:
        m, _, _, _ = self.srv.call("get_engine_metrics", {})
        log("metrics", tag=tag, **{k: m.get(k) for k in (
            "mode", "observations", "rejected_actions", "delivered_actions",
            "pending_regions", "models_loaded", "timings_source")})
        return m

    def observe(self, tag: str) -> tuple[dict, list[dict]]:
        page, is_error, meta, ms = self.srv.call("observe_screen", {"limit": PAGE_LIMIT})
        if is_error:
            log("observe", tag=tag, error=page.get("code"), rtt_ms=round(ms, 1))
            raise Abort(f"observe_screen failed: {page.get('code')}")
        elements = list(page.get("elements", []))
        first = page
        pages = 1
        while page.get("next_cursor"):
            page, is_error, _, _ = self.srv.call(
                "observe_screen", {"limit": PAGE_LIMIT, "cursor": page["next_cursor"]})
            if is_error:
                raise Abort(f"observe_screen page failed: {page.get('code')}")
            elements.extend(page.get("elements", []))
            pages += 1
        timings = {k.removesuffix("_us"): round(v / 1000, 2) for k, v in meta.get(TIMINGS_KEY, {}).items()}
        log(
            "observe",
            tag=tag,
            snapshot_id=first["snapshot_id"],
            geometry_epoch=first["geometry_epoch"],
            frame_seq=first["frame_seq"],
            age_ms=first["age_ms"],
            degraded=first["degraded"],
            total_elements=first["total_elements"],
            pages=pages,
            roles=dict(Counter(e["role"] for e in elements)),
            actionability=dict(Counter(e["actionability"] for e in elements)),
            label_source=dict(Counter(e["label_source"] for e in elements)),
            menu_label_matches={m: n for m in self.menu_labels if (n := self.matches(elements, m))},
            timings_ms=timings,
            rtt_ms=round(ms, 1),
        )
        return first, elements

    @staticmethod
    def matches(elements: list[dict], label: str) -> int:
        return sum(1 for e in elements if e.get("label", "").strip() == label)

    def menu_target(self, elements: list[dict]) -> dict | None:
        """The one confirmed element whose trimmed label is the chosen menu label. The
        first allowed label with exactly one match is chosen and kept for the whole run."""
        labels = [self.chosen] if self.chosen else list(self.menu_labels)
        for label in labels:
            found = [e for e in elements if e.get("label", "").strip() == label]
            if len(found) == 1 and found[0]["actionability"] == "confirmed":
                self.chosen = label
                e = found[0]
                log("menu_target", found=True, label_choice=self.menu_labels.index(label),
                    target_id=e["target_id"], revision=e["revision"], role=e["role"],
                    label_source=e["label_source"])
                return e
        log("menu_target", found=False)
        return None

    def act(self, rid: str, snap: dict, target: dict, *, dry_run: bool,
            expected_effect: str = "none", expect_foreground: str | None = "notepad.exe") -> dict:
        if not dry_run:
            if self.live_sent >= self.args.max_live:
                self.engage_stop("live action budget exhausted")
            if expect_foreground is not None:
                self.require_foreground(expect_foreground)
            self.live_sent += 1
        args = {
            "request_id": rid,
            "session_id": snap["session_id"],
            "snapshot_id": snap["snapshot_id"],
            "geometry_epoch": snap["geometry_epoch"],
            "target_id": target["target_id"],
            "expected_revision": target["revision"],
            "action": "click",
            "dry_run": dry_run,
            "expected_effect": expected_effect,
        }
        self.last_args = args
        return self.send_action(args)

    def send_action(self, args: dict, tag: str | None = None) -> dict:
        out, is_error, _, ms = self.srv.call("execute_action", args)
        err = out.get("error") or {}
        log(
            "execute_action",
            tag=tag or args["request_id"],
            request_id=args["request_id"],
            dry_run=args["dry_run"],
            expected_effect=args.get("expected_effect"),
            status=out.get("status"),
            input_delivered=out.get("input_delivered"),
            verified=out.get("verified"),
            observed_effect=out.get("observed_effect"),
            error_code=err.get("code"),
            error_message=err.get("message"),
            retryable=err.get("retryable"),
            next_step=err.get("next_step"),
            is_error=is_error,
            rtt_ms=round(ms, 1),
            live_requests_so_far=self.live_sent,
        )
        return out

    # -- helpers
    def observe_target(self, tag: str, attempts: int = 3) -> tuple[dict, dict]:
        for i in range(attempts):
            snap, elements = self.observe(f"{tag}#{i + 1}")
            target = self.menu_target(elements)
            if target is not None:
                return snap, target
        self.engage_stop("no unique confirmed menu label")
        raise AssertionError  # unreachable

    def live_click(self, rid: str, tag: str, expected_effect: str, retries: int = 1) -> dict:
        """A live click on the menu label of a fresh observation; one retry only when the
        engine refused with STALE_SNAPSHOT (nothing delivered)."""
        for attempt in range(retries + 1):
            snap, target = self.observe_target(tag)
            out = self.act(f"{rid}-{attempt + 1}", snap, target, dry_run=False,
                           expected_effect=expected_effect)
            code = (out.get("error") or {}).get("code")
            if code != "STALE_SNAPSHOT" or out.get("input_delivered"):
                return out
        return out

    # -- scenarios
    def scenario_dry(self) -> None:
        self.metrics("start")
        for i in range(self.args.observations):
            self.observe(f"obs{i + 1}")
        self.require_foreground("notepad.exe")
        snap, target = self.observe_target("sec01")
        self.act("s3-dry-1", snap, target, dry_run=True, expected_effect="visual_change")
        self.act("s3-sec01-1", snap, target, dry_run=False, expected_effect="visual_change")
        self.metrics("end")

    def scenario_live(self, steps: list[str]) -> None:
        self.metrics("start")
        for i in range(self.args.observations):
            self.observe(f"warmup{i + 1}")
        if "close" in steps:
            # Restores a menu left open by an earlier run: one click on the same label.
            m0 = self.metrics("close.before")
            self.live_click("s3-close", "close", "visual_change")
            time.sleep(1.0)
            self.observe("close.after")
            m1 = self.metrics("close.after")
            log("delta", tag="close", delivered=m1["delivered_actions"] - m0["delivered_actions"],
                rejected=m1["rejected_actions"] - m0["rejected_actions"])
        if "it03" in steps:
            m0 = self.metrics("it03.before")
            opened = self.live_click("s3-it03-open", "it03.open", "visual_change")
            self.close_args = None
            if opened.get("input_delivered"):
                time.sleep(1.0)
                closed = self.live_click("s3-it03-close", "it03.close", "visual_change")
                self.close_args = dict(self.last_args)
                self.close_out = closed
                time.sleep(1.0)
                self.observe("it03.after")
            m1 = self.metrics("it03.after")
            log("delta", tag="it03", delivered=m1["delivered_actions"] - m0["delivered_actions"],
                rejected=m1["rejected_actions"] - m0["rejected_actions"])
        if "it05" in steps and getattr(self, "close_args", None):
            m0 = self.metrics("it05.before")
            self.require_foreground("notepad.exe")
            self.live_sent += 1
            self.send_action(dict(self.close_args), tag="it05.replay")
            conflict = dict(self.close_args, dry_run=True)
            self.send_action(conflict, tag="it05.conflict")
            m1 = self.metrics("it05.after")
            log("delta", tag="it05", delivered=m1["delivered_actions"] - m0["delivered_actions"],
                rejected=m1["rejected_actions"] - m0["rejected_actions"])
            self.observe("it05.after")
        if "sec02" in steps:
            if self.stop_file is None:
                raise Abort("sec02 needs [safety] stop_file")
            m0 = self.metrics("sec02.before")
            snap, target = self.observe_target("sec02")
            self.stop_file.write_text("", encoding="utf-8")
            log("operator_stop", engaged=True)
            time.sleep(0.5)
            self.act("s3-sec02-1", snap, target, dry_run=False, expected_effect="visual_change")
            self.stop_file.unlink()
            log("operator_stop", engaged=False)
            m1 = self.metrics("sec02.after")
            log("delta", tag="sec02", delivered=m1["delivered_actions"] - m0["delivered_actions"],
                rejected=m1["rejected_actions"] - m0["rejected_actions"])
            self.observe("sec02.after")
        if "sec05" in steps:
            self.sec05()
        self.metrics("end")

    def sec05(self) -> None:
        if process_running("CalculatorApp.exe"):
            log("sec05", skipped="calculator already running; not touched")
            return
        m0 = self.metrics("sec05.before")
        snap, target = self.observe_target("sec05")
        self.require_foreground("notepad.exe")
        subprocess.Popen(["cmd", "/c", "start", "", "calc.exe"], creationflags=CREATE_NO_WINDOW)
        deadline = time.monotonic() + 30
        name = None
        activations = 0
        while time.monotonic() < deadline:
            name, _ = foreground()
            if name == "calculatorapp.exe":
                break
            time.sleep(1.0)
            # Windows may refuse the foreground to a process started in the background;
            # ask for the window's activation (no input is sent).
            activations += 1
            subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "$p = Get-Process CalculatorApp -ErrorAction SilentlyContinue | "
                 "Select-Object -First 1; if ($p) { $null = (New-Object -ComObject "
                 "WScript.Shell).AppActivate($p.Id) }"],
                check=False, creationflags=CREATE_NO_WINDOW)
        log("sec05_activation_requests", count=activations)
        log("sec05_foreground", calculator_foreground=name == "calculatorapp.exe")
        try:
            if name == "calculatorapp.exe":
                # The calculator is not on the allowlist: the engine must refuse and send
                # nothing. The driver's own notepad check is skipped on purpose here.
                self.act("s3-sec05-1", snap, target, dry_run=False,
                         expected_effect="visual_change", expect_foreground="calculatorapp.exe")
            else:
                log("sec05", skipped="the calculator did not reach the foreground; nothing sent")
        finally:
            time.sleep(0.5)
            subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "Stop-Process -Name CalculatorApp -ErrorAction SilentlyContinue"],
                check=False, creationflags=CREATE_NO_WINDOW)
            log("sec05", calculator_closed=not process_running("CalculatorApp.exe"))
        m1 = self.metrics("sec05.after")
        log("delta", tag="sec05", delivered=m1["delivered_actions"] - m0["delivered_actions"],
            rejected=m1["rejected_actions"] - m0["rejected_actions"])


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--exe", required=True)
    p.add_argument("--config", required=True, type=Path)
    p.add_argument("--scenario", choices=["observe", "dry", "live"], required=True)
    p.add_argument("--steps", default="it03,it05,sec02,sec05")
    p.add_argument("--session-id", default="s3-supervised")
    p.add_argument("--output", type=int, default=0)
    p.add_argument("--observations", type=int, default=3, help="warm-up observations")
    p.add_argument("--max-live", type=int, default=0, help="dry_run:false requests allowed")
    p.add_argument("--menu-labels", default="Archivo,File,Editar,Edit,Ver,View",
                   help="allowed menu labels, in order of preference")
    args = p.parse_args()

    raw = args.config.read_bytes()
    config = tomllib.loads(raw.decode("utf-8"))
    stop = config.get("safety", {}).get("stop_file") or None
    stop_file = (args.config.parent / stop) if stop else None
    if stop_file is not None and stop_file.exists():
        log("abort", reason="the stop file exists before the run")
        return 2
    live = args.scenario == "live"
    argv = [args.exe, "serve", "--source", "dxgi", "--output", str(args.output),
            "--config", str(args.config), "--session-id", args.session_id]
    if live:
        argv += ["--input", "sendinput"]
    log("start", scenario=args.scenario, live_input=live,
        config_sha256=hashlib.sha256(raw).hexdigest(), max_live=args.max_live)
    name, primary = foreground()
    if name != "notepad.exe" or not primary:
        log("abort", reason="notepad.exe is not in the foreground, maximized on the primary monitor")
        return 2

    srv = Server(argv)
    run = Run(args, srv, stop_file)
    code = 0
    try:
        if live:
            run.scenario_live([s.strip() for s in args.steps.split(",") if s.strip()])
        elif args.scenario == "dry":
            run.scenario_dry()
        else:
            for i in range(args.observations):
                run.observe(f"obs{i + 1}")
            run.metrics("end")
    except Abort as e:
        log("abort", reason=str(e))
        code = 1
    finally:
        exit_code = srv.close()
        time.sleep(0.2)
        # stderr is the engine's own diagnostics (no screen text by design); only its
        # line count and the lines this run relies on are shown.
        keep = ("ACTIVE CONTROL", "STOP engaged", "stop cleared", "source=")
        for line in srv.stderr:
            if any(k in line for k in keep):
                log("stderr", line=line[:300])
        log("end", server_exit=exit_code, stderr_lines=len(srv.stderr), live_requests=run.live_sent)
    return code


if __name__ == "__main__":
    sys.exit(main())
