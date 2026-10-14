#!/usr/bin/env python3
"""S4 reduced acceptance protocol: click-only end-to-end runs through ``ommatix serve``.

A minimal MCP 2026-07-28 client (the transport of ``tools/s3/mcp_driver.py``) that runs a
fixed click-only flow N times in one of three authorized applications, each run being:

    observe -> select the target by its exact OCR label -> execute_action (click,
    dry_run: false, expected_effect: visual_change) -> observe again -> check the visible
    text by OCR (and, where the OS offers one, by an independent check)

Applications and flows (docs/thesis/S4_ACEPTACION_REDUCIDA.md):

``testapp``    E2E-01, the controlled test application ``tools/s4/testapp``: ``Sumar`` x3
               (``Valor: tres``), ``Reiniciar`` (``Valor: cero``), ``Activar`` twice
               (``Estado: encendido`` / ``apagado``), ``Siguiente`` / ``Anterior``
               (``Segunda`` / ``Primera página``). Independent check: the window title,
               which mirrors the state, read through ``GetWindowTextW`` after every step.
``calculator`` E2E-02, the Windows 11 Calculator. Its digit and operator buttons get no
               OCR label, so the arithmetic flow cannot select targets by label; the only
               flow left opens and closes the function flyout (OCR label ``Fundón``), and
               the pilot showed that its box is grounded on the x² button: not scored.
``notepad``    E2E-03, Notepad: open the ``Editar`` menu and close it with a second click on
               ``Editar``. No menu item, tab or caption button is ever clicked.

After every delivered click the control under the cursor (where ``SendInput`` left it) is
read through UI Automation, independently of the engine: its name, or the bounding
rectangle of the window's element named like the label, must match the target
("grounding"). An ``effect_observed`` whose title, display or grounding check fails is
counted as an engine false success.

Live input goes only through the engine's gate; this script never injects input itself.
It never types. It starts the test application and the Calculator itself and closes only
the processes it started; Notepad is never started or closed: one existing window is
restored and maximized on the primary monitor (window management, not input) and minimized
again at the end if it was. A started window that opens on another monitor is moved to the
primary one the same way. The
operator's stop is ``[safety] stop_file``: the script creates it and ends the protocol on
anything unexpected (the foreground is not the target application maximized on the
primary monitor, an ambiguous label, the live budget exhausted).

Privacy (AGENTS.md rule 7, ADR-0012): nothing is saved but statuses, codes, counts,
timings and booleans. Labels stay in memory; ``--mode probe`` prints only labels of the
flow's fixed vocabulary (never for Notepad, whose restored tabs may hold the owner's text).
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
import tomllib
from collections import Counter
from ctypes import wintypes
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s3_driver", HERE.parent / "s3" / "mcp_driver.py")
s3 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(s3)  # the S3 transport: Server, META, TIMINGS_KEY, Abort

Abort = s3.Abort
PAGE_LIMIT = 50
LIVE_CAP = 400  # owner's ceiling for the whole protocol
CREATE_NO_WINDOW = 0x08000000

# ---- read-only Windows queries ------------------------------------------------------------

user32 = s3.user32
kernel32 = s3.kernel32
user32.FindWindowExW.argtypes = [wintypes.HWND, wintypes.HWND, wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowExW.restype = wintypes.HWND
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]


def _pid(hwnd) -> int:
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def _exe(pid: int) -> str | None:
    handle = kernel32.OpenProcess(0x1000, False, pid)
    if not handle:
        return None
    try:
        buf = ctypes.create_unicode_buffer(32768)
        size = wintypes.DWORD(len(buf))
        if kernel32.QueryFullProcessImageNameW(handle, 0, buf, ctypes.byref(size)):
            return os.path.basename(buf.value).lower()
        return None
    finally:
        kernel32.CloseHandle(handle)


def _class(hwnd) -> str:
    buf = ctypes.create_unicode_buffer(257)
    user32.GetClassNameW(hwnd, buf, 257)
    return buf.value


def foreground() -> dict:
    """The foreground application as the SendInput actuator resolves it (a frame of
    ApplicationFrameHost.exe stands for its hosted CoreWindow's process), whether its
    window is maximized on the primary monitor, and its pid. No window text is read."""
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return {"exe": None, "pid": None, "maximized_primary": False, "hwnd": None}
    pid = _pid(hwnd)
    exe = _exe(pid)
    if _class(hwnd) == "ApplicationFrameWindow" and exe == "applicationframehost.exe":
        core = user32.FindWindowExW(hwnd, None, "Windows.UI.Core.CoreWindow", None)
        pid = _pid(core) if core else None
        exe = _exe(pid) if pid else None
    monitor = user32.MonitorFromWindow(hwnd, 0)
    info = s3.MONITORINFO(cbSize=ctypes.sizeof(s3.MONITORINFO))
    primary = bool(monitor) and bool(user32.GetMonitorInfoW(monitor, ctypes.byref(info)))
    on_primary = primary and bool(info.dwFlags & 1)
    return {"exe": exe, "pid": pid, "maximized_primary": on_primary and bool(user32.IsZoomed(hwnd)),
            "hwnd": hwnd}


WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [WNDENUMPROC, wintypes.LPARAM]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int,
                                ctypes.c_int, ctypes.c_int, wintypes.UINT]
SW_RESTORE, SW_MAXIMIZE = 9, 3
SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE = 0x0001, 0x0004, 0x0010


def app_windows(pid: int) -> list:
    """The visible top-level windows that show `pid`: its own, or the frames of
    ApplicationFrameHost.exe whose hosted CoreWindow belongs to it."""
    found = []

    def visit(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            if _pid(hwnd) == pid:
                found.append(hwnd)
            elif _class(hwnd) == "ApplicationFrameWindow":
                core = user32.FindWindowExW(hwnd, None, "Windows.UI.Core.CoreWindow", None)
                if core and _pid(core) == pid:
                    found.append(hwnd)
        return True

    user32.EnumWindows(WNDENUMPROC(visit), 0)
    return found


def on_primary_maximized(hwnd) -> bool:
    monitor = user32.MonitorFromWindow(hwnd, 0)
    info = s3.MONITORINFO(cbSize=ctypes.sizeof(s3.MONITORINFO))
    ok = bool(monitor) and bool(user32.GetMonitorInfoW(monitor, ctypes.byref(info)))
    return ok and bool(info.dwFlags & 1) and bool(user32.IsZoomed(hwnd))


def place_on_primary(pid: int) -> int:
    """Window management, not input: moves the windows of a process the protocol started
    to the primary monitor and maximizes them (the Calculator remembers the monitor where
    it last closed). Returns how many windows were moved."""
    moved = 0
    for hwnd in app_windows(pid):
        if on_primary_maximized(hwnd):
            continue
        user32.ShowWindow(hwnd, SW_RESTORE)
        user32.SetWindowPos(hwnd, None, 100, 100, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
        user32.ShowWindow(hwnd, SW_MAXIMIZE)
        moved += 1
    return moved


SW_MINIMIZE = 6
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.IsIconic.argtypes = [wintypes.HWND]


def maximize_on_primary(hwnd) -> None:
    """Window management, not input: restore, move to the primary monitor, maximize."""
    user32.ShowWindow(hwnd, SW_RESTORE)
    user32.SetWindowPos(hwnd, None, 100, 100, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
    user32.ShowWindow(hwnd, SW_MAXIMIZE)


def window_title(hwnd) -> str:
    """Only ever called on the test application's own window."""
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value


class LASTINPUTINFO(ctypes.Structure):
    _fields_ = [("cbSize", wintypes.UINT), ("dwTime", wintypes.DWORD)]


def idle_seconds() -> float:
    """Seconds since the last input event of the session (the owner's or injected)."""
    info = LASTINPUTINFO(cbSize=ctypes.sizeof(LASTINPUTINFO))
    user32.GetLastInputInfo(ctypes.byref(info))
    return round(((kernel32.GetTickCount() - info.dwTime) & 0xFFFFFFFF) / 1000, 1)


def pids_of(image: str) -> set[int]:
    out = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True, check=False).stdout
    pids = set()
    for line in out.splitlines():
        parts = [p.strip('"') for p in line.split('","')]
        if len(parts) > 1 and parts[0].lower() == image.lower():
            pids.add(int(parts[1]))
    return pids


def activate(pid: int) -> None:
    """Asks Windows to activate a window of `pid` (WScript.Shell.AppActivate; no input)."""
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    f"$null = (New-Object -ComObject WScript.Shell).AppActivate({pid})"],
                   check=False, creationflags=CREATE_NO_WINDOW, capture_output=True)


CALC_DISPLAY_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$win = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
  (New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'CalculatorResults')))
if ($win) { [Console]::Out.Write($win.Current.Name) }
"""


UIA_AT_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$p = New-Object System.Windows.Point([double]__X__, [double]__Y__)
$e = [System.Windows.Automation.AutomationElement]::FromPoint($p)
$names = @()
for ($i = 0; $e -and $i -lt 3; $i++) {
  $names += ($e.Current.AutomationId + "|" + $e.Current.Name + "|" + $e.Current.ControlType.ProgrammaticName)
  $e = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($e)
}
[Console]::Out.Write(($names -join "`n"))
"""

UIA_NAMED_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__NAME__'))
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $name)
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
$inside = 0
foreach ($e in $all) {
  $r = $e.Current.BoundingRectangle
  if (-not $r.IsEmpty -and __X__ -ge $r.Left -and __X__ -lt $r.Right -and __Y__ -ge $r.Top -and __Y__ -lt $r.Bottom) { $inside++ }
}
[Console]::Out.Write("$($all.Count)|$inside")
"""

user32.GetCursorPos.argtypes = [ctypes.POINTER(wintypes.POINT)]


def cursor() -> tuple[int, int]:
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    return int(pt.x), int(pt.y)


def uia_named_contains_cursor(hwnd, name: str) -> tuple[int, int]:
    """How many UI Automation elements of window `hwnd` are named `name`, and how many of
    them contain the cursor (where SendInput left it)."""
    import base64
    x, y = cursor()
    script = (UIA_NAMED_PS.replace("__HWND__", str(int(hwnd))).replace("__X__", str(x))
              .replace("__Y__", str(y))
              .replace("__NAME__", base64.b64encode(name.encode("utf-8")).decode("ascii")))
    out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                         capture_output=True, text=True, encoding="utf-8", errors="replace",
                         check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout.strip()
    try:
        count, inside = out.split("|")
        return int(count), int(inside)
    except ValueError:
        return 0, 0


def uia_at_cursor() -> list[tuple[str, str, str]]:
    """The UI Automation element under the mouse cursor and its two ancestors, as
    (AutomationId, Name, ControlType). SendInput leaves the cursor on the click point, so
    this tells, independently of the engine, which control the click reached."""
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    script = UIA_AT_PS.replace("__X__", str(int(pt.x))).replace("__Y__", str(int(pt.y)))
    out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                         capture_output=True, text=True, encoding="utf-8", errors="replace",
                         check=False,
                         creationflags=CREATE_NO_WINDOW, timeout=30).stdout
    rows = []
    for line in out.splitlines():
        parts = line.split("|")
        if len(parts) == 3:
            rows.append((parts[0], parts[1], parts[2].removeprefix("ControlType.")))
    return rows


def calculator_display() -> str | None:
    """The Calculator display's UI Automation name (e.g. "La pantalla muestra 15"), read
    through .NET UI Automation in a separate PowerShell; only its digits are used."""
    out = subprocess.run(["powershell", "-NoProfile", "-Command", CALC_DISPLAY_PS],
                         capture_output=True, text=True, encoding="utf-8", errors="replace",
                         check=False,
                         creationflags=CREATE_NO_WINDOW, timeout=30).stdout.strip()
    if not out:
        return None
    digits = re.findall(r"-?\d[\d.,]*", out)
    return digits[-1] if digits else ""


# ---- flows --------------------------------------------------------------------------------

TESTAPP_TITLE = "Ommatix S4 prueba [valor={} estado={} pagina={}]"


CONTAINERS = {"Pane", "Window", "Group", "Custom", "Document"}


def named_control(label: str, chain: list[tuple[str, str, str]]) -> bool | None:
    """Whether the control under the cursor (or its parent) is named `label`. None when UI
    Automation only reached unnamed containers (XAML islands stop there): the caller then
    falls back to the bounding rectangle of the element named `label`."""
    if any(name == label for _, name, _ in chain[:2]):
        return True
    if not chain or all(ctype in CONTAINERS for _, _, ctype in chain[:2]):
        return None
    return False


CARDINALS = ("cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve")


def value_check(v: int) -> dict:
    """The counter shows exactly the word of `v` among the first five values."""
    return {"present": [CARDINALS[v]], "absent": [w for w in CARDINALS[:5] if w != CARDINALS[v]]}


def testapp_flow() -> dict:
    t = TESTAPP_TITLE.format
    return {
        "exe": "ommatix-testapp.exe",
        "initial": {"present": ["cero", "apagado", "Primera"],
                    "absent": ["uno", "dos", "tres", "encendido", "Segunda"]},
        # (label, OCR check afterwards, independent check: the window title)
        "steps": [
            ("Sumar", value_check(1), t(1, "off", 1)),
            ("Sumar", value_check(2), t(2, "off", 1)),
            ("Sumar", value_check(3), t(3, "off", 1)),
            ("Reiniciar", value_check(0), t(0, "off", 1)),
            ("Activar", {"present": ["encendido"], "absent": ["apagado"]}, t(0, "on", 1)),
            ("Activar", {"present": ["apagado"], "absent": ["encendido"]}, t(0, "off", 1)),
            ("Siguiente", {"present": ["Segunda"], "absent": ["Primera"]}, t(0, "off", 2)),
            ("Anterior", {"present": ["Primera"], "absent": ["Segunda"]}, t(0, "off", 1)),
        ],
        "grounding": named_control,
        "vocabulary": {"Contador", "Interruptor", "Paginador", "Sumar", "Restar", "Reiniciar",
                       "Activar", "Siguiente", "Anterior", "Valor:", "Estado:", "página",
                       "apagado", "encendido", "Primera", "Segunda", *CARDINALS},
        "vocabulary_re": None,
    }


def calculator_flow(label: str) -> dict:
    """The digit and operator buttons are single glyphs that Windows OCR does not read, so
    the arithmetic flow cannot select its targets by label (S4 report). The flow opens and
    closes the Scientific mode's function flyout, whose button is labelled `label` by the
    OCR, and checks that the display stays at 0 (UI Automation)."""
    return {
        "exe": "calculatorapp.exe",
        "initial": {"present": [label]},
        # (label, OCR check afterwards, independent check: the display's UIA value)
        "steps": [
            (label, ("ocr_set", "changed"), "0"),
            (label, ("ocr_set", "restored"), "0"),
        ],
        "grounding": lambda _label, chain: (None if not chain else
                                            any(aid == CALCULATOR_FLYOUT_ID for aid, _, _ in chain[:2])),
        "vocabulary": {label},
        "vocabulary_re": None,
    }


NOTEPAD_MENU = "Editar"
# Words of the Editar menu's items (Windows 11 Notepad, es-ES); never clicked.
NOTEPAD_MENU_ITEMS = ("Deshacer", "Cortar", "Copiar", "Pegar", "Eliminar", "Buscar",
                      "Reemplazar", "Seleccionar", "Fuente")


def notepad_flow() -> dict:
    """Opens the Editar menu and closes it with a second click on Editar. `Ver` is not
    used: on this desktop a second, non-menu `Ver` was visible (S4 report)."""
    return {
        "exe": "notepad.exe",
        "initial": ("menu", "closed"),
        "steps": [(NOTEPAD_MENU, ("menu", "open"), None),
                  (NOTEPAD_MENU, ("menu", "closed"), None)],
        "grounding": named_control,
        "vocabulary": {"Archivo", "Editar", "Ver", *NOTEPAD_MENU_ITEMS},
        "vocabulary_re": None,
    }


FLOWS = {"testapp": testapp_flow, "calculator": calculator_flow, "notepad": notepad_flow}
CALCULATOR_FLYOUT_ID = "FuncButton"  # provisional: the flyout button's AutomationId
CALCULATOR_LABEL = "Fundón"  # how Windows OCR (es-ES) reads the "Función" flyout button
E2E_ID = {"testapp": "E2E-01", "calculator": "E2E-02", "notepad": "E2E-03"}

# ---- the protocol ---------------------------------------------------------------------------


def log(event: str, **fields) -> None:
    print(json.dumps({"t": round(time.monotonic(), 3), "event": event, **fields},
                     ensure_ascii=False), flush=True)


class Protocol:
    def __init__(self, args, server, stop_file: Path | None, flow: dict, ledger: Path):
        self.args = args
        self.srv = server
        self.stop_file = stop_file
        self.flow = flow
        self.ledger = ledger
        self.live_sent = 0
        self.results: list[dict] = []
        self.started: set[int] = set()
        self.initial_set: frozenset | None = None
        self.menu_base: int | None = None
        self.current: int | None = None  # the application kept between runs (--restart never)
        self.notepad: dict | None = None

    # -- safety
    def engage_stop(self, reason: str) -> None:
        if self.stop_file is not None:
            self.stop_file.write_text("", encoding="utf-8")
        log("operator_stop_engaged", reason=reason)
        raise Abort(reason)

    def ledger_total(self) -> int:
        try:
            return int(self.ledger.read_text(encoding="utf-8").strip() or 0)
        except FileNotFoundError:
            return 0

    def spend_live(self) -> None:
        total = self.ledger_total()
        if self.live_sent >= self.args.max_live or total >= LIVE_CAP:
            self.engage_stop("live action budget exhausted")
        self.ledger.write_text(str(total + 1), encoding="utf-8")
        self.live_sent += 1

    def require_foreground(self) -> dict:
        fg = foreground()
        if fg["exe"] != self.flow["exe"] or not fg["maximized_primary"]:
            self.engage_stop(
                f"foreground is not {self.flow['exe']} maximized on the primary monitor "
                f"(got {'the target, not maximized' if fg['exe'] == self.flow['exe'] else 'another application' if fg['exe'] else 'none'})")
        return fg

    # -- tools
    def metrics(self) -> dict:
        m, _, _, _ = self.srv.call("get_engine_metrics", {})
        return m

    def observe(self) -> tuple[dict, list[dict], dict]:
        page, is_error, meta, ms = self.srv.call("observe_screen", {"limit": PAGE_LIMIT})
        if is_error:
            raise Abort(f"observe_screen failed: {page.get('code')}")
        elements = list(page.get("elements", []))
        first = page
        while page.get("next_cursor"):
            page, is_error, _, _ = self.srv.call(
                "observe_screen", {"limit": PAGE_LIMIT, "cursor": page["next_cursor"]})
            if is_error:
                raise Abort(f"observe_screen page failed: {page.get('code')}")
            elements.extend(page.get("elements", []))
        timings = {k.removesuffix("_us"): round(v / 1000, 2)
                   for k, v in meta.get(s3.TIMINGS_KEY, {}).items()}
        obs = {"frame_seq": first["frame_seq"], "epoch": first["geometry_epoch"],
               "elements": first["total_elements"], "degraded": first["degraded"],
               "total_ms": timings.get("total"), "detect_ms": timings.get("detect"),
               "rtt_ms": round(ms, 1)}
        return first, elements, obs

    @staticmethod
    def labels(elements: list[dict]) -> Counter:
        return Counter(e.get("label", "").strip() for e in elements)

    def target(self, elements: list[dict], label: str) -> dict | None:
        found = [e for e in elements if e.get("label", "").strip() == label]
        if len(found) > 1:
            self.engage_stop(f"ambiguous label: {len(found)} elements share the step's label")
        if found and found[0]["actionability"] == "confirmed":
            return found[0]
        return None

    def ocr_set(self, elements: list[dict]) -> frozenset:
        return frozenset(e.get("label", "").strip() for e in elements
                         if e.get("label", "").strip())

    @staticmethod
    def words(elements: list[dict]) -> Counter:
        """The OCR words of every label: a control's label joins the words inside its box,
        so a state word may be alone (a text node) or part of a longer label."""
        return Counter(w for e in elements for w in e.get("label", "").split())

    def ocr_ok(self, elements: list[dict], check) -> bool:
        labels = self.words(elements)
        if isinstance(check, tuple) and check[0] == "ocr_set":
            now = self.ocr_set(elements)
            base = self.initial_set or frozenset()
            added = len(now - base)
            if check[1] == "changed":
                return added >= 3
            return added <= 1 and len(base - now) <= 1
        if isinstance(check, tuple) and check[0] == "menu":
            # Item words visible now, against the closed menu of the initial state (the
            # document may contain some of these words too).
            n = sum(labels.get(item, 0) for item in NOTEPAD_MENU_ITEMS)
            if self.menu_base is None:  # the initial check sets the closed-menu baseline
                self.menu_base = n
                return check[1] == "closed"
            return n >= self.menu_base + 3 if check[1] == "open" else n <= self.menu_base
        return (all(labels.get(text) for text in check.get("present", ()))
                and not any(labels.get(text) for text in check.get("absent", ())))

    def probe_print(self, elements: list[dict]) -> None:
        vocab, rx = self.flow["vocabulary"], self.flow["vocabulary_re"]
        shown = Counter()
        hidden = 0
        for e in elements:
            label = e.get("label", "").strip()
            if label in vocab or (rx is not None and rx.match(label)):
                shown[(label, e["actionability"], e["role"], e["label_source"])] += 1
            else:
                hidden += 1
        log("probe", vocabulary_labels=[list(k) + [n] for k, n in sorted(shown.items())],
            other_elements=hidden,
            label_lengths=dict(sorted(Counter(min(len(e.get("label", "").strip()), 10)
                                              for e in elements).items())),
            label_source=dict(Counter(e["label_source"] for e in elements)),
            actionability=dict(Counter(e["actionability"] for e in elements)))
        if self.args.app == "calculator" and self.args.show_calculator_labels:
            # The Calculator's own text is disposable; shown for protocol design only.
            log("probe_calculator_labels", labels=sorted(
                (e.get("label", "").strip(), e["zone"], e["actionability"]) for e in elements
                if e.get("label", "").strip()))

    # -- app lifecycle
    def notepad_window(self) -> int:
        """Notepad is never started or closed (one process may hold several windows and
        the owner's restored tabs). One existing top-level Notepad window is restored and
        maximized on the primary monitor, and minimized again at the end if it was."""
        if self.notepad is None:
            windows = [(pid, h) for pid in sorted(pids_of("Notepad.exe"))
                       for h in app_windows(pid) if _class(h) == "Notepad"]
            if not windows:
                self.engage_stop("no Notepad window to reuse")
            pid, hwnd = windows[0]
            self.notepad = {"pid": pid, "hwnd": hwnd, "was_minimized": bool(user32.IsIconic(hwnd))}
            log("notepad_window", windows=len(windows), was_minimized=self.notepad["was_minimized"])
        if not on_primary_maximized(self.notepad["hwnd"]):
            maximize_on_primary(self.notepad["hwnd"])
        return self.notepad["pid"]

    def launch(self) -> int | None:
        exe = self.flow["exe"]
        if exe == "notepad.exe":
            return self.notepad_window()
        image = "CalculatorApp.exe" if exe == "calculatorapp.exe" else exe
        before = pids_of(image)
        if before and exe == "calculatorapp.exe":
            self.engage_stop("a Calculator the protocol did not start is running")
        if exe == "ommatix-testapp.exe":
            proc = subprocess.Popen([self.args.testapp_exe])
            pid = proc.pid
        else:
            subprocess.Popen(["cmd", "/c", "start", "", "calc.exe"], creationflags=CREATE_NO_WINDOW)
            pid = None
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and pid is None:
                time.sleep(0.25)
                new = pids_of(image) - before
                pid = min(new) if new else None
            if pid is None:
                self.engage_stop("the Calculator did not start")
        self.started.add(pid)
        return pid

    def wait_foreground(self, pid: int | None) -> tuple[bool, int]:
        """Windows refuses the foreground to a background process while the user is active
        (ForegroundLockTimeout); activation is requested every 2 s (no input is sent)."""
        deadline = time.monotonic() + self.args.foreground_wait
        requests = 0
        last = 0.0
        self.placements = 0
        while time.monotonic() < deadline:
            if pid is not None and pid in self.started:
                self.placements += place_on_primary(pid)
            fg = foreground()
            if fg["exe"] == self.flow["exe"] and (pid is None or fg["pid"] == pid):
                if fg["maximized_primary"]:
                    return True, requests
            time.sleep(0.25)
            if pid is not None and time.monotonic() - last >= 2.0:
                last = time.monotonic()
                requests += 1
                if self.notepad is not None:
                    user32.SetForegroundWindow(self.notepad["hwnd"])  # that window only
                else:
                    activate(pid)
        return False, requests

    def close(self, pid: int | None) -> None:
        if pid is None or pid not in self.started:
            return
        subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True, check=False)
        for _ in range(20):
            if pid not in pids_of("CalculatorApp.exe" if self.flow["exe"] == "calculatorapp.exe"
                                  else self.flow["exe"]):
                break
            time.sleep(0.25)
        self.started.discard(pid)

    # -- one run
    def run_once(self, n: int) -> dict:
        res = {"e2e": E2E_ID[self.args.app], "run": n, "success": False, "steps": 0,
               "actions_requested": 0, "delivered": 0, "effect_observed": 0, "indeterminate": 0,
               "rejected": 0, "stale_retries": 0, "ocr_checks_failed": 0,
               "independent_checks": 0, "independent_failed": 0, "engine_false_success": 0,
               "observations": [], "actions": [], "failure": None}
        start = time.perf_counter()
        res["user_idle_s_at_start"] = idle_seconds()
        reuse = self.args.restart == "never" and self.current is not None
        if reuse:
            pid = self.current
            res["restarted"] = False
            fg = foreground()
            if fg["exe"] != self.flow["exe"] or (pid is not None and fg["pid"] != pid):
                ok, requests = self.wait_foreground(pid)
            else:
                ok, requests = fg["maximized_primary"], 0
        else:
            pid = self.launch()
            res["restarted"] = pid in self.started
            ok, requests = self.wait_foreground(pid)
        res["activation_requests"] = requests
        res["window_placements"] = getattr(self, "placements", 0)
        res["foreground_wait_s"] = round(time.perf_counter() - start, 2)
        if not ok:
            res["failure"] = "the application did not reach the foreground maximized"
            self.close(pid)
            self.current = None
            res["total_s"] = round(time.perf_counter() - start, 2)
            return res
        self.current = pid if self.args.restart == "never" else None
        hwnd = foreground()["hwnd"]
        self.app_hwnd = hwnd
        if not reuse:
            time.sleep(self.args.settle)  # let a new window finish its first paints
        # Settle: observe until the initial state is visible (the window may still paint).
        elements = []
        for _ in range(6):
            snap, elements, obs = self.observe()
            res["observations"].append(obs)
            if self.ocr_ok(elements, self.flow["initial"]):
                self.initial_set = self.ocr_set(elements)
                break
            time.sleep(0.5)
        else:
            res["failure"] = "initial state not visible by OCR"
        if self.args.mode == "probe":
            self.probe_print(elements)
        if res["failure"] is None and self.args.app == "testapp":
            for _ in range(10):  # the title may lag the first paint by a few frames
                if window_title(hwnd) == TESTAPP_TITLE.format(0, "off", 1):
                    break
                time.sleep(0.2)
            if window_title(hwnd) != TESTAPP_TITLE.format(0, "off", 1):
                fg = foreground()
                ours = fg["pid"] == pid and fg["hwnd"] == hwnd
                title = window_title(hwnd) if ours else ""
                res["failure"] = ("initial title differs"
                                  + (f" ({title!r})" if title.startswith("Ommatix S4") else
                                     " (not the test application's window)"))
        if res["failure"] is None and self.args.mode == "run":
            self.steps(res, snap, elements, hwnd, pid)
        res["total_s"] = round(time.perf_counter() - start, 2)
        if self.args.restart == "each" or res["failure"] is not None:
            self.close(pid)
            self.current = None
        return res

    def steps(self, res: dict, snap: dict, elements: list[dict], hwnd, pid) -> None:
        for label, ocr_check, independent in self.flow["steps"]:
            res["steps"] += 1
            target = None
            for _ in range(3):
                target = self.target(elements, label)
                if target is not None:
                    break
                time.sleep(0.3)
                snap, elements, obs = self.observe()
                res["observations"].append(obs)
            if target is None:
                res["failure"] = f"step {res['steps']}: target label not found or not confirmed"
                return
            out = None
            for attempt in range(4):  # again only after STALE_SNAPSHOT (nothing delivered)
                self.require_foreground()
                self.spend_live()
                res["actions_requested"] += 1
                args = {"request_id": f"s4-{self.args.app}-{res['run']}-{res['steps']}-{attempt + 1}",
                        "session_id": snap["session_id"], "snapshot_id": snap["snapshot_id"],
                        "geometry_epoch": snap["geometry_epoch"],
                        "target_id": target["target_id"], "expected_revision": target["revision"],
                        "action": "click", "dry_run": False, "expected_effect": "visual_change"}
                out, _, meta, ms = self.srv.call("execute_action", args)
                err = (out.get("error") or {}).get("code")
                act = {"step": res["steps"], "status": out.get("status"),
                       "input_delivered": out.get("input_delivered"), "verified": out.get("verified"),
                       "observed_effect": out.get("observed_effect"), "error": err,
                       "rtt_ms": round(ms, 1)}
                res["actions"].append(act)
                log("action", run=res["run"], **act)
                if out.get("input_delivered"):
                    res["delivered"] += 1
                if out.get("status") == "effect_observed":
                    res["effect_observed"] += 1
                elif out.get("status") == "indeterminate":
                    res["indeterminate"] += 1
                elif out.get("status") == "rejected":
                    res["rejected"] += 1
                if err == "STALE_SNAPSHOT" and not out.get("input_delivered") and attempt < 3:
                    res["stale_retries"] += 1
                    time.sleep(0.5)
                    snap, elements, obs = self.observe()
                    res["observations"].append(obs)
                    target = self.target(elements, label)
                    if target is None:
                        res["failure"] = f"step {res['steps']}: target lost after STALE_SNAPSHOT"
                        return
                    continue
                break
            if out.get("status") == "rejected":
                res["failure"] = f"step {res['steps']}: rejected ({(out.get('error') or {}).get('code')})"
                if (out.get("error") or {}).get("code") == "POLICY_DENIED":
                    self.engage_stop("POLICY_DENIED during the protocol")
                return
            # Independent grounding: which control is under the cursor (UI Automation).
            if out.get("input_delivered") and self.flow.get("grounding"):
                chain = uia_at_cursor()
                hit = self.flow["grounding"](label, chain)
                if hit is None and self.args.app != "calculator":
                    count, inside = uia_named_contains_cursor(self.app_hwnd, label)
                    hit = None if count == 0 else inside > 0
                    res.setdefault("grounding_by_rect", []).append([count, inside])
                res.setdefault("grounding", []).append(hit)
                if hit is False:
                    res["grounding_wrong"] = res.get("grounding_wrong", 0) + 1
                if self.args.app == "calculator" and self.args.show_calculator_labels:
                    log("probe_calculator_uia", chain=chain)
                if self.args.debug_grounding:
                    log("grounding_chain", step=res["steps"], hit=hit,
                        chain=[(aid, ctype, name == label) for aid, name, ctype in chain])
            # Re-observe and check the visible text (no retry of a delivered click).
            passed = False
            for _ in range(4):
                snap, elements, obs = self.observe()
                res["observations"].append(obs)
                if self.ocr_ok(elements, ocr_check):
                    passed = True
                    break
                time.sleep(0.25)
            indep_ok = True
            if independent is not None:
                res["independent_checks"] += 1
                if self.args.app == "testapp":
                    indep_ok = window_title(hwnd) == independent
                else:
                    indep_ok = calculator_display() == independent
                if not indep_ok:
                    res["independent_failed"] += 1
            if self.args.app == "calculator" and self.args.show_calculator_labels:
                now = self.ocr_set(elements)
                log("probe_calculator_delta", added=sorted(now - (self.initial_set or frozenset())),
                    removed=sorted((self.initial_set or frozenset()) - now))
            grounded = (res.get("grounding") or [None])[-1] if out.get("input_delivered") else None
            # A false success: the engine reported the effect while the independent evidence
            # (the title, the display, the control under the cursor) says the intended effect
            # did not happen.
            if out.get("status") == "effect_observed" and (not indep_ok or grounded is False):
                res["engine_false_success"] += 1
            if not passed and indep_ok and grounded is not False:
                res["ocr_false_negative"] = res.get("ocr_false_negative", 0) + 1
            if passed and (not indep_ok or grounded is False):
                res["ocr_false_positive"] = res.get("ocr_false_positive", 0) + 1
            if not passed and self.args.app == "testapp":
                vocab = self.flow["vocabulary"]
                log("ocr_check_failed", run=res["run"], step=res["steps"],
                    tokens=sorted(k for k in self.words(elements) if k in vocab))
            if not passed or not indep_ok:
                if not passed:
                    res["ocr_checks_failed"] += 1
                res["failure"] = (f"step {res['steps']}: "
                                  f"{'OCR' if not passed else 'independent'} check failed")
                return
        res["success"] = True

    def run(self) -> None:
        failures = 0
        for n in range(self.args.first_run, self.args.first_run + self.args.runs):
            m0 = self.metrics()
            res = self.run_once(n)
            m1 = self.metrics()
            res["engine_delivered_delta"] = m1["delivered_actions"] - m0["delivered_actions"]
            res["engine_rejected_delta"] = m1["rejected_actions"] - m0["rejected_actions"]
            self.results.append(res)
            with open(self.args.results, "a", encoding="utf-8") as fh:
                fh.write(json.dumps(res, ensure_ascii=False) + "\n")
            log("run", **{k: v for k, v in res.items() if k not in ("observations", "actions")},
                observations=len(res["observations"]))
            failures = 0 if res["success"] else failures + 1
            if failures >= 3:
                self.engage_stop("three consecutive failed runs")
            if self.args.app == "notepad":
                time.sleep(self.args.pause)
            else:
                time.sleep(self.args.pause)


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--exe", required=True)
    p.add_argument("--config", required=True, type=Path)
    p.add_argument("--app", choices=sorted(FLOWS), required=True)
    p.add_argument("--mode", choices=["probe", "run"], default="run",
                   help="probe: observe the initial state only, no input (dry server)")
    p.add_argument("--runs", type=int, default=10)
    p.add_argument("--first-run", type=int, default=1, help="number of the first run")
    p.add_argument("--max-live", type=int, default=0)
    p.add_argument("--ledger", type=Path, required=True,
                   help="file holding the protocol's live request count (cap 400)")
    p.add_argument("--results", type=Path, required=True, help="JSONL file to append runs to")
    p.add_argument("--testapp-exe", default=str(HERE / "testapp" / "target" / "release" / "ommatix-testapp.exe"))
    p.add_argument("--session-id", default="s4")
    p.add_argument("--output", type=int, default=0)
    p.add_argument("--foreground-wait", type=float, default=600.0)
    p.add_argument("--pause", type=float, default=1.0)
    p.add_argument("--settle", type=float, default=1.5,
                   help="seconds to wait after a newly started window reaches the foreground")
    p.add_argument("--show-calculator-labels", action="store_true")
    p.add_argument("--debug-grounding", action="store_true",
                   help="log the AutomationId and ControlType under the cursor (never names)")
    p.add_argument("--restart", choices=["each", "never"], default="each",
                   help="each: start and close the application around every run; never: one "
                        "start per invocation, every run checks the initial state first")
    args = p.parse_args()

    raw = args.config.read_bytes()
    config = tomllib.loads(raw.decode("utf-8"))
    stop = config.get("safety", {}).get("stop_file") or None
    stop_file = (args.config.parent / stop) if stop else None
    if stop_file is None:
        log("abort", reason="the configuration has no stop_file")
        return 2
    if stop_file.exists():
        log("abort", reason="the stop file exists before the run")
        return 2
    live = args.mode == "run"
    if live and not config.get("safety", {}).get("actuation_enabled"):
        log("abort", reason="run mode needs the live configuration")
        return 2
    argv = [args.exe, "serve", "--source", "dxgi", "--output", str(args.output),
            "--config", str(args.config), "--session-id", args.session_id]
    if live:
        argv += ["--input", "sendinput"]
    flow = FLOWS[args.app](CALCULATOR_LABEL) if args.app == "calculator" else FLOWS[args.app]()
    log("start", app=args.app, mode=args.mode, runs=args.runs, live_input=live,
        config_sha256=hashlib.sha256(raw).hexdigest(), max_live=args.max_live)
    srv = s3.Server(argv)
    proto = Protocol(args, srv, stop_file, flow, args.ledger)
    code = 0
    try:
        m = proto.metrics()
        log("metrics", mode=m.get("mode"), models_loaded=m.get("models_loaded"))
        proto.run()
    except Abort as e:
        log("abort", reason=str(e))
        code = 1
    finally:
        for pid in list(proto.started):
            proto.close(pid)
        if proto.notepad is not None and proto.notepad["was_minimized"]:
            user32.ShowWindow(proto.notepad["hwnd"], SW_MINIMIZE)
            log("notepad_window", minimized_again=True)
        exit_code = srv.close()
        keep = ("ACTIVE CONTROL", "STOP engaged", "stop cleared", "source=")
        for line in srv.stderr:
            if any(k in line for k in keep):
                log("stderr", line=line[:300])
        ok = sum(r["success"] for r in proto.results)
        log("end", server_exit=exit_code, runs=len(proto.results), successes=ok,
            live_requests=proto.live_sent, ledger_total=proto.ledger_total())
    return code


if __name__ == "__main__":
    sys.exit(main())
