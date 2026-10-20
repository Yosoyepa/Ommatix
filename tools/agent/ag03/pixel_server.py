#!/usr/bin/env python3
"""AG-03 pixel tool server (ADR-0038 decision 3): a stdio MCP server with exactly three tools.

``screenshot()``, ``click(x, y)`` and ``type_text(text)``. It is not the Ommatix engine and the engine
is not involved. Standard library only. The agent's MCP client starts this script; it speaks
line-delimited JSON-RPC on stdin/stdout (stdout belongs to MCP: diagnostics never go there).

Guards (decision 4) are the pure functions of ``pixel_guards``; this module orders them, reads the
operating system through the injected ``OsReader``, reserves in the *AG-02 ledger* (``proxy.reserve``,
imported, not copied: same file shape, same per-goal and per-session caps, an unreadable ledger
refuses everything) and sends through the injected ``Sender``. A test never injects real input: the
fakes of ``fake_os`` stand in for all three.

``--synthetic`` serves drawn PNGs of fake windows and never captures, reads the operating system or
sends anything (phase 1 semantics): the mode of the Step 0 probe, the isolation preflight and the
gate smoke. Real mode (Step 6, the owner present) additionally needs ``--attest-pid``/``--attest-exe``
and a capture admission record (``--admission``) written after the Step 2 tests; without a record no
image is returned.

Privacy (ADR-0038 decision 5, O5): an image exists only in memory, to answer the call. The metadata
log holds the time, the tool, a closed result token, pixel dimensions, byte sizes, latency, counters
and, for a click, a label of the goal's fixed vocabulary (``other`` otherwise) - never image bytes,
a hash of an image, screen text or a typed text. The server never touches the operator's stop file.
"""

from __future__ import annotations

import argparse
import base64
import json
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_capture as cap  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import proxy  # noqa: E402

SERVER_NAME = "pixel"
TOOLS = ("screenshot", "click", "type_text")
ALLOWED_TOOL_NAMES = tuple(f"mcp__{SERVER_NAME}__{t}" for t in TOOLS)
PROTOCOL_VERSION = "2025-06-18"
MAX_SCREENSHOTS_DEFAULT = 30
MAX_TOOL_CALLS_DEFAULT = 60
# Pinned: the same text for every model (the baseline gets no tuning, O12).
SCREENSHOT_TEXT = "Screenshot of the application window, {w} by {h} pixels. Click coordinates are pixels of this image."
RESULT_CODES = frozenset({
    "STOP_ENGAGED", "STOP_UNREADABLE", "SCREENSHOT_MISSING", "SCREENSHOT_STALE", "FOREGROUND_UNREADABLE",
    "EXE_NOT_ALLOWED", "FOREGROUND_NOT_TARGET_PROCESS", "HELPER_WINDOW", "ELEVATED_OR_UNKNOWN",
    "NOT_PRIMARY_MONITOR", "WINDOW_ICONIC", "WINDOW_CHANGED", "GEOMETRY_CHANGED", "POINT_OUTSIDE_CLIENT",
    "POINT_WINDOW_MISMATCH", "MODIFIER_HELD", "MODIFIERS_UNREADABLE", "FOCUS_UNREADABLE",
    "FOCUS_OUTSIDE_WINDOW", "LIVE_BUDGET_EXHAUSTED", "LEDGER_LOCKED", "LEDGER_UNREADABLE",
    "LEDGER_UNWRITABLE", "CAPTURE_REFUSED", "LIMIT_EXCEEDED", "INVALID_ARGUMENT", "UNKNOWN_TOOL",
    "OS_READ_FAILED", "INPUT_FAILED", "NO_LIVE_BUDGET"})
CAPTURE_REASONS = frozenset({"BACKEND_NOT_ADMITTED", "BACKEND_FAILED", "WRONG_SIZE", "IMAGE_TOO_LARGE"})


@dataclass
class ServerConfig:
    phase: int = 1
    synthetic: bool = True
    synthetic_size: tuple[int, int] = cap.DEFAULT_SYNTHETIC_SIZE
    goal_id: str = "G00"
    attested: g.Attested | None = None
    vocabulary: tuple[str, ...] = ()
    max_screenshots: int = MAX_SCREENSHOTS_DEFAULT
    max_tool_calls: int = MAX_TOOL_CALLS_DEFAULT


@dataclass
class Deps:
    clock: Callable[[], float] = time.monotonic
    os: w32.OsReader | None = None
    backend: cap.CaptureBackend | None = None
    admission: cap.AdmissionRecord | None = None
    sender: w32.Sender | None = None
    hit_test: Callable[[int, int], str | None] | None = None  # read-only; the label never reaches the agent
    reserve: Callable[[], None] | None = None  # raises proxy.Refusal (the AG-02 ledger)
    log: Callable[[dict], None] = lambda record: None


class Refused(Exception):
    """A tool call that fails closed: an error result, nothing sent. ``code`` is a closed token."""

    def __init__(self, code: str, **meta):
        super().__init__(code)
        self.code, self.meta = code, meta


def tool_definitions() -> list[dict]:
    return [
        {"name": "screenshot", "description": "Take a screenshot of the application window.",
         "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False}},
        {"name": "click", "description": "Left-click once at pixel (x, y) of the latest screenshot.",
         "inputSchema": {"type": "object", "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}},
                         "required": ["x", "y"], "additionalProperties": False}},
        {"name": "type_text", "description": "Type 1 to 16 printable ASCII characters into the focused field.",
         "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}},
                         "required": ["text"], "additionalProperties": False}},
    ]


def _is_int(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def label_of(label: str | None, vocabulary) -> str:
    """The closed label of a hit-test result: the vocabulary member, ``other`` outside it, ``unknown``
    when there was no reading. Never the raw string."""
    if label is None:
        return "unknown"
    return label if label in vocabulary else "other"


class PixelServer:
    def __init__(self, cfg: ServerConfig, deps: Deps):
        self.cfg, self.deps = cfg, deps
        self.calls = 0
        self.shots = 0
        self.shot: g.Shot | None = None
        self.t0 = deps.clock()

    # ---- JSON-RPC ----------------------------------------------------------------------------

    def handle(self, message) -> dict | None:
        """The response to one JSON-RPC message, or None for a notification."""
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            return {"jsonrpc": "2.0", "id": None, "error": {"code": -32600, "message": "invalid request"}}
        method, rid = message.get("method"), message.get("id")
        if method is None or rid is None:
            return None  # a notification (initialized, cancelled) or a stray response
        if method == "initialize":
            params = message.get("params")
            version = params.get("protocolVersion") if isinstance(params, dict) else None
            return self._ok(rid, {"protocolVersion": version if isinstance(version, str) else PROTOCOL_VERSION,
                                  "capabilities": {"tools": {}},
                                  "serverInfo": {"name": "ommatix-ag03-pixel", "version": "1"}})
        if method == "ping":
            return self._ok(rid, {})
        if method == "tools/list":
            return self._ok(rid, {"tools": tool_definitions()})
        if method == "tools/call":
            params = message.get("params")
            if not isinstance(params, dict) or not isinstance(params.get("name"), str):
                return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32602, "message": "invalid params"}}
            return self._ok(rid, self.call_tool(params["name"], params.get("arguments")))
        return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "method not found"}}

    @staticmethod
    def _ok(rid, result) -> dict:
        return {"jsonrpc": "2.0", "id": rid, "result": result}

    # ---- the tools ---------------------------------------------------------------------------

    def call_tool(self, name: str, arguments) -> dict:
        started = self.deps.clock()
        tool = name if name in TOOLS else "unknown"
        meta: dict = {}
        try:
            self.calls += 1
            if self.calls > self.cfg.max_tool_calls:
                raise Refused("LIMIT_EXCEEDED", limit="tool_calls")
            if name not in TOOLS:
                raise Refused("UNKNOWN_TOOL")
            args = arguments if arguments is not None else {}
            if not isinstance(args, dict):
                raise Refused("INVALID_ARGUMENT")
            result = getattr(self, "_" + name)(args, meta)
            code = None
        except Refused as refusal:
            code, meta = refusal.code, meta | refusal.meta
            result = {"isError": True, "content": [{"type": "text", "text": json.dumps({"error": {"code": code}})}]}
        self.deps.log({"t_ms": round((started - self.t0) * 1000, 1), "tool": tool, "ok": code is None,
                       **({"code": code} if code else {}), **meta,
                       "latency_ms": round((self.deps.clock() - started) * 1000, 1)})
        return result

    def _need_os(self):
        if self.deps.os is None or self.cfg.attested is None:
            raise Refused("OS_READ_FAILED")
        return self.deps.os, self.cfg.attested

    def _read(self, point, want_focus):
        os_, attested = self._need_os()
        try:
            return os_.evidence(attested.pid, point, want_focus)
        except Exception:  # noqa: BLE001 - an unreadable system is a refusal
            raise Refused("OS_READ_FAILED") from None

    def _stop(self):
        if self.cfg.synthetic:
            return
        try:
            stop = self._need_os()[0].stop_present()
        except Refused:
            raise
        except Exception:  # noqa: BLE001
            raise Refused("OS_READ_FAILED") from None
        if (token := g.guard_stop(stop)):
            raise Refused(token)

    def _screenshot(self, args: dict, meta: dict) -> dict:
        if args:
            raise Refused("INVALID_ARGUMENT")
        if self.shots >= self.cfg.max_screenshots:
            raise Refused("LIMIT_EXCEEDED", limit="screenshots")
        self._stop()
        taken = self.deps.clock()
        if self.cfg.synthetic:
            width, height = self.cfg.synthetic_size
            png, _word, _shape = cap.synthetic_png(self.shots, width, height)
            self.shots += 1
            self.shot = g.Shot(t=taken, hwnd=0, client=(0, 0, width, height), width=width, height=height)
        else:
            evidence = self._read(None, False)
            if (token := g.guard_stop(evidence.stop_present) or g.guard_window(evidence.foreground,
                                                                               self._need_os()[1])):
                raise Refused(token)
            try:
                captured = cap.capture_window(evidence.foreground, self.deps.backend, self.deps.admission)
            except cap.CaptureRefused as refused:
                raise Refused("CAPTURE_REFUSED", reason=refused.reason) from None
            png, width, height = captured.png, captured.width, captured.height
            self.shots += 1
            self.shot = g.Shot(t=taken, hwnd=evidence.foreground.hwnd, client=evidence.foreground.client,
                               width=width, height=height)
        meta |= {"width": width, "height": height, "png_bytes": len(png), "screenshots": self.shots}
        return {"content": [{"type": "image", "data": base64.b64encode(png).decode("ascii"), "mimeType": "image/png"},
                            {"type": "text", "text": SCREENSHOT_TEXT.format(w=width, h=height)}]}

    def _text_result(self, status: str) -> dict:
        return {"content": [{"type": "text", "text": json.dumps({"status": status})}]}

    def _click(self, args: dict, meta: dict) -> dict:
        if set(args) != {"x", "y"} or not _is_int(args["x"]) or not _is_int(args["y"]):
            raise Refused("INVALID_ARGUMENT")
        x, y = args["x"], args["y"]
        self._stop()  # 1
        shot = self.shot
        if shot is None:
            raise Refused("SCREENSHOT_MISSING")
        if not (0 <= x < shot.width and 0 <= y < shot.height):
            raise Refused("POINT_OUTSIDE_CLIENT")
        point = g.screen_point(shot, x, y)
        label = None
        if self.deps.hit_test is not None and not self.cfg.synthetic:
            try:
                label = self.deps.hit_test(*point)
            except Exception:  # noqa: BLE001 - a missing reading is "unknown", never a crash
                label = None
        meta |= {"label": label_of(label, self.cfg.vocabulary)}
        if self.cfg.phase == 1 or self.cfg.synthetic:  # 2: simulated, no operating-system call
            return self._text_result("simulated")
        return self._deliver("click", point, shot, meta, lambda: w32.build_click_inputs(
            point[0], point[1], self._need_os()[0].virtual_screen()))

    def _type_text(self, args: dict, meta: dict) -> dict:
        if set(args) != {"text"} or w32.text_problem(args["text"]) is not None:
            raise Refused("INVALID_ARGUMENT")
        text = args["text"]
        meta |= {"text_len": len(text)}
        self._stop()  # 1
        if self.cfg.phase == 1 or self.cfg.synthetic:
            return self._text_result("simulated")
        return self._deliver("type_text", None, self.shot, meta, lambda: w32.build_text_inputs(text))

    def _deliver(self, kind: str, point, shot, meta: dict, build: Callable[[], list[dict]]) -> dict:
        os_, attested = self._need_os()
        if (token := g.guard_shot_age(self.deps.clock(), shot)):  # 3
            raise Refused(token)
        if self.deps.reserve is None:
            raise Refused("NO_LIVE_BUDGET")
        try:
            self.deps.reserve()  # 4: before any input, and the reservation is kept on every refusal below
        except proxy.Refusal as refusal:
            raise Refused(refusal.reason if refusal.reason in RESULT_CODES else "LEDGER_UNREADABLE") from None
        meta |= {"reserved": True}
        if (token := g.check_evidence(self._read(point, kind == "type_text"), shot, attested, point)):  # 5-7
            raise Refused(token, stage="initial")
        inputs = build()
        # 8: the delivery check, a fresh reading of the clock and of the system, immediately before the batch.
        fresh = self._read(point, kind == "type_text")
        if (token := g.check_batch(fresh, self.deps.clock(), shot, attested, point)):
            raise Refused(token, stage="delivery")
        try:
            sent = self.deps.sender.send(inputs)
        except Exception:  # noqa: BLE001
            sent = False
        if not sent:
            raise Refused("INPUT_FAILED", stage="send")
        meta |= {"delivered": True, "events": len(inputs)}
        return self._text_result("input_delivered")

    # ---- the stdio loop ----------------------------------------------------------------------

    def serve(self, stdin, stdout) -> None:
        for raw in stdin:
            try:
                message = json.loads(raw.decode("utf-8")) if raw.strip() else None
            except (UnicodeDecodeError, ValueError):
                stdout.write(json.dumps({"jsonrpc": "2.0", "id": None,
                                         "error": {"code": -32700, "message": "parse error"}}).encode() + b"\n")
                stdout.flush()
                continue
            if message is None:
                continue
            response = self.handle(message)
            if response is not None:
                stdout.write(json.dumps(response, separators=(",", ":")).encode("ascii") + b"\n")
                stdout.flush()


# ---- command line -------------------------------------------------------------------------------

def parse_args(argv):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    p.add_argument("--phase", type=int, choices=(1, 2), default=1)
    p.add_argument("--synthetic", action="store_true",
                   help="serve drawn PNGs of fake windows; capture nothing, read nothing, send nothing")
    p.add_argument("--synthetic-size", default="1280x720", help="WxH of the drawn images")
    p.add_argument("--goal-id", default="G00")
    p.add_argument("--vocabulary", default="", help="comma-separated labels of the goal's application")
    p.add_argument("--log", type=Path, required=True, help="metadata JSONL (absolute, outside the checkout)")
    p.add_argument("--ledger", type=Path)
    p.add_argument("--max-live", type=int, metavar="N")
    p.add_argument("--max-live-goal", type=int, metavar="K")
    p.add_argument("--stop-file", type=Path)
    p.add_argument("--attest-pid", type=int)
    p.add_argument("--attest-exe", choices=g.ALLOWED_EXES)
    p.add_argument("--admission", type=Path, help="the Step 2 capture admission record")
    p.add_argument("--max-screenshots", type=int, default=MAX_SCREENSHOTS_DEFAULT)
    p.add_argument("--max-tool-calls", type=int, default=MAX_TOOL_CALLS_DEFAULT)
    args = p.parse_args(argv)
    try:
        width, height = (int(v) for v in args.synthetic_size.lower().split("x"))
        assert 16 <= width <= 4096 and 16 <= height <= 4096
    except (ValueError, AssertionError):
        p.error("--synthetic-size must be WxH between 16 and 4096")
    args.synthetic_size = (width, height)
    if not proxy.GOAL_ID.match(args.goal_id):
        p.error("--goal-id must be 1-32 plain characters")
    for name in ("log", "ledger", "stop_file", "admission"):
        value = getattr(args, name)
        if value is not None and (problem := proxy.path_problem(value)):
            p.error(f"--{name.replace('_', '-')}: {problem}")
    if not args.synthetic:
        if args.attest_pid is None or args.attest_exe is None or args.stop_file is None or args.admission is None:
            p.error("real mode needs --attest-pid, --attest-exe, --stop-file and --admission (written by Step 2)")
        if args.phase == 2 and (args.ledger is None or args.max_live is None or args.max_live < 0):
            p.error("phase 2 needs --ledger and --max-live")
    return args


def build_real_deps(args, log) -> Deps:  # pragma: no cover - the owner's session only (Step 6)
    if sys.platform != "win32":
        raise SystemExit("real mode runs on Windows only")
    os_ = w32.RealOs(args.stop_file)
    if not os_.ensure_dpi_aware():
        raise SystemExit("could not make this process DPI aware")
    record = cap.load_admission(args.admission)
    backend = cap.PrintWindowBackend(w32.RealGdi())
    if record is None or not record.admits(backend):
        raise SystemExit("no capture admission record admits the backend: Step 2 has not passed (no image is served)")
    reserve = None
    if args.phase == 2:
        def reserve():
            proxy.reserve(args.ledger, args.goal_id, args.max_live, args.max_live_goal)
    return Deps(os=os_, backend=backend, admission=record, sender=w32.RealSender(), reserve=reserve, log=log,
                hit_test=UiaHitTest(tuple(v for v in args.vocabulary.split(",") if v)).label_at)


UIA_NAME_AT_POINT_PS = r"""
Add-Type -AssemblyName UIAutomationClient
$p = New-Object System.Windows.Point([double]__X__, [double]__Y__)
$e = [System.Windows.Automation.AutomationElement]::FromPoint($p)
for ($i = 0; $i -lt 3 -and $e -ne $null; $i++) { Write-Output $e.Current.Name; $e = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($e) }
"""


def label_from_names(names: list[str], vocabulary) -> str | None:
    """The first name (the element, then its ancestors) that is in the fixed vocabulary, else None."""
    return next((n for n in names if n in vocabulary), None)


class UiaHitTest:  # pragma: no cover - never executed by the tests
    """Read-only UI Automation lookup of the control under a screen point (ADR-0022's point query),
    used for scoring and the log only. The names stay in memory and are reduced to the vocabulary."""

    def __init__(self, vocabulary):
        self.vocabulary = vocabulary

    def label_at(self, x: int, y: int) -> str | None:
        import subprocess
        script = UIA_NAME_AT_POINT_PS.replace("__X__", str(int(x))).replace("__Y__", str(int(y)))
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script], capture_output=True, text=True,
                             encoding="utf-8", errors="replace", check=False, timeout=30,
                             creationflags=0x08000000).stdout
        names = [line.rstrip("\r") for line in out.splitlines() if line.strip()]
        return label_from_names(names, self.vocabulary) if names else "other"


def make_log_writer(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)
    handle = open(path, "a", encoding="utf-8")

    def write(record: dict) -> None:
        handle.write(json.dumps(record, ensure_ascii=True) + "\n")
        handle.flush()
    return write


def main(argv=None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    log = make_log_writer(args.log)
    cfg = ServerConfig(
        phase=args.phase, synthetic=args.synthetic, synthetic_size=args.synthetic_size, goal_id=args.goal_id,
        attested=None if args.synthetic else g.Attested(args.attest_pid, args.attest_exe),
        vocabulary=tuple(v for v in args.vocabulary.split(",") if v),
        max_screenshots=args.max_screenshots, max_tool_calls=args.max_tool_calls)
    deps = Deps(log=log) if args.synthetic else build_real_deps(args, log)
    PixelServer(cfg, deps).serve(sys.stdin.buffer, sys.stdout.buffer)
    log({"tool": "server", "ok": True, "end": True})
    return 0


if __name__ == "__main__":
    sys.exit(main())
