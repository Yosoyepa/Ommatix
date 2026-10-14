#!/usr/bin/env python3
"""S4 reduced acceptance protocol, second round (ADR-0017): supervised end-to-end runs.

A minimal MCP 2026-07-28 client (the transport of ``tools/s3/mcp_driver.py``) that runs a
fixed flow N times in one of three authorized applications through ``ommatix serve``. Each
run is:

    observe -> select the target by its exact label (observe_screen) -> execute_action
    (dry_run: false, expected_effect: visual_change; ``type`` for the typing step) ->
    observe again -> check the visible text by OCR where the flow asks for it -> check the
    end state through an OS-side check that does not depend on the engine

Applications and flows (docs/thesis/S4_ACEPTACION_REDUCIDA.md, section "Segunda ronda con
accesibilidad (ADR-0017)"):

``testapp``    E2E-01, the controlled test application ``tools/s4/testapp``: the first-round
               click flow (``Sumar`` x3, ``Reiniciar``, ``Activar`` x2, ``Siguiente``,
               ``Anterior``), then a typing step into its text field ``Campo de texto`` (the
               text ``prueba 123``). The OS-side check is the window title, which mirrors the
               state and the typed text (``GetWindowTextW``) after every step.
``calculator`` E2E-02, the Windows 11 Calculator: ``Siete``, ``Más``, ``Ocho``, ``Es igual a``
               (the display must read 15), then the clear control. The clear control is not a
               fixed name: the Calculator's real accessibility names are read from
               ``observe_screen`` before the flow and again before the last click, and the first
               name of ``CLEAR_CONTROLS`` (``Borrar``, ``Borrar todo``, ``Borrar entrada``) that
               exists as a confirmed element is clicked. With none of them in the initial state
               the run fails before any input. The OS-side check is the UI Automation name of
               the display after every click, and it must return to 0 after the clear.
``notepad``    E2E-03, Notepad: open the ``Editar`` menu and close it with a second click on
               ``Editar``. No menu item, tab or caption button is clicked and nothing is typed.
               The menu is "open" or "closed" by its UI Automation ExpandCollapseState, never by
               screen text. Every run starts by checking that the menu is Collapsed, and a run
               that fails with the menu left expanded closes it with one more click on ``Editar``
               through the engine's gate (never a retry loop, and only if it is expanded).

After every delivered action the control under the cursor (where SendInput left it) is read
through UI Automation: its name, or the bounding rectangle of the window's element with the
label, must match the target ("grounding"). An ``effect_observed`` whose OS-side or grounding
check fails is counted as an engine false success; there must be none.

Live input goes only through the engine's gate; this script never injects input itself.
It types only into the test application's field. It starts the test application and the
Calculator itself and closes only the processes it started. Notepad is never started or
closed: one existing window is restored and maximized on the primary monitor (window
management, not input) and minimized again at the end if it was. The operator's stop is
``[safety] stop_file``: the script creates it and ends the protocol on anything unexpected
(the foreground is not the target application maximized on the primary monitor, an
ambiguous label, the live budget exhausted).

Privacy (AGENTS.md rule 7, ADR-0012): nothing is saved but statuses, codes, counts, timings
and booleans (plus Notepad's menu state: Collapsed or Expanded). Screen text stays in memory. ``--mode probe`` prints counts and the attributes
of the flow's own fixed labels (the test application's and the Calculator's); it prints no
label of Notepad, whose restored tabs may hold the owner's text.
"""

from __future__ import annotations

import argparse
import base64
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
import unicodedata
from collections import Counter
from ctypes import wintypes
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s3_driver", HERE.parent / "s3" / "mcp_driver.py")
s3 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(s3)  # the S3 transport: Server, META, TIMINGS_KEY, Abort

Abort = s3.Abort
PAGE_LIMIT = 50
# The owner's ceiling for the whole protocol. Round 1 used 124 live requests (S4 report,
# section 4.5) and its ledger was not kept with the runs, so the ledger starts at 124 and the
# cap counts both rounds.
LIVE_CAP = 400
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


user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
kernel32.GetCurrentThreadId.restype = wintypes.DWORD
# winit's hidden event target and the input method windows belong to the same process.
HELPER_WINDOW_MARKERS = ("Event Target", "IME", "MSCTFIME")


def pick_main_window(windows: list[tuple[int, str, int]]) -> int | None:
    """The main window among (handle, class name, area) tuples: the largest one that is not a
    helper window. In the third S4 round AppActivate(pid) did not bring the test application
    to the foreground (164 requests over 333 s), while the process also owns a helper window
    that enumeration lists; activation therefore targets the main window explicitly."""
    main = [w for w in windows if not any(marker in w[1] for marker in HELPER_WINDOW_MARKERS)]
    return max(main, key=lambda w: w[2])[0] if main else None


def main_window(pid: int) -> int | None:
    """The main window of `pid` (see pick_main_window)."""
    found = []
    for hwnd in app_windows(pid):
        rect = wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(rect))
        area = max(0, rect.right - rect.left) * max(0, rect.bottom - rect.top)
        found.append((hwnd, _class(hwnd), area))
    return pick_main_window(found)


def bring_to_front(hwnd) -> bool:
    """Window management, not input. Attaches this thread to the input queue of the foreground
    thread, which lets SetForegroundWindow succeed past the foreground lock, then detaches.
    AttachThreadInput shares input state; it sends no key or mouse event. Returns whether
    `hwnd` became the foreground window."""
    fg_thread = user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), None)
    me = kernel32.GetCurrentThreadId()
    attached = bool(fg_thread) and fg_thread != me and bool(user32.AttachThreadInput(me, fg_thread, True))
    try:
        return bool(user32.SetForegroundWindow(hwnd))
    finally:
        if attached:
            user32.AttachThreadInput(me, fg_thread, False)


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

MENU_STATE_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__NAME__'))
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $name)
$states = @()
foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
  $pattern = $null
  if ($e.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
    $states += $pattern.Current.ExpandCollapseState.ToString()
  }
}
[Console]::Out.Write(($states -join ","))
"""

user32.GetCursorPos.argtypes = [ctypes.POINTER(wintypes.POINT)]


def cursor() -> tuple[int, int]:
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    return int(pt.x), int(pt.y)


def uia_named_contains_cursor(hwnd, name: str) -> tuple[int, int]:
    """How many UI Automation elements of window `hwnd` are named `name`, and how many of
    them contain the cursor (where SendInput left it)."""
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


def notepad_menu_state(hwnd) -> str | None:
    """The ExpandCollapseState of Notepad's `Editar` menu bar item (Collapsed or Expanded),
    read through UI Automation of the window `hwnd`; None when it is not exactly one item
    or unreadable. An OS-side read: no input, and it does not depend on the screen's text."""
    script = (MENU_STATE_PS.replace("__HWND__", str(int(hwnd)))
              .replace("__NAME__", base64.b64encode(NOTEPAD_MENU.encode("utf-8")).decode("ascii")))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_menu_state(out)


def calculator_display() -> str | None:
    """The Calculator display's UI Automation name (e.g. "La pantalla muestra 15"), read
    through .NET UI Automation in a separate PowerShell; only its last number is used."""
    out = subprocess.run(["powershell", "-NoProfile", "-Command", CALC_DISPLAY_PS],
                         capture_output=True, text=True, encoding="utf-8", errors="replace",
                         check=False,
                         creationflags=CREATE_NO_WINDOW, timeout=30).stdout.strip()
    if not out:
        return None
    digits = re.findall(r"-?\d[\d.,]*", out)
    return digits[-1] if digits else ""


# ---- flows --------------------------------------------------------------------------------

TESTAPP_TITLE = "Ommatix S4 prueba [valor={} estado={} pagina={} texto={}]"
TESTAPP_MARK = "texto="  # the title's field that mirrors the typed text
TEXT_FIELD = "Campo de texto"  # the test application's field, by its accessible name
TYPED_TEXT = "prueba 123"  # disposable; typed into that field only
CALC_BUTTONS = ("Siete", "Más", "Ocho", "Es igual a")  # Spanish Windows names
# The clear controls the flow accepts, in order of preference, by exact accessible name. In
# Scientific mode after "Es igual a" no "Borrar" existed in the third S4 round (the tree had
# "Quitar entrada", "Borrar toda la memoria" and "Borrar toda el historial", which are other
# controls and are never in this list). Whichever of these exists is clicked.
CLEAR_CONTROLS = ("Borrar", "Borrar todo", "Borrar entrada")
CARDINALS = ("cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve")
NOTEPAD_MENU = "Editar"
# UI Automation ExpandCollapseState of the menu bar item: the OS-side truth of "open" and
# "closed" (screen text never decides it).
MENU_OPEN, MENU_CLOSED = "Expanded", "Collapsed"
# A step whose end state the OCR does not check: only its OS-side check decides.
NOT_BY_OCR = {"present": (), "absent": ()}

CONTAINERS = {"Pane", "Window", "Group", "Custom", "Document"}


def testapp_title(value: int, state: str, page: int, text: str = "") -> str:
    return TESTAPP_TITLE.format(value, state, page, text)


def named_control(label: str, chain: list[tuple[str, str, str]]) -> bool | None:
    """Whether the control under the cursor (or its parent) is named `label`. None when UI
    Automation only reached unnamed containers (XAML islands stop there): the caller then
    falls back to the bounding rectangle of the element named `label`."""
    def same(name: str) -> bool:
        return unicodedata.normalize("NFC", name) == unicodedata.normalize("NFC", label)

    if any(same(name) for _, name, _ in chain[:2]):
        return True
    if not chain or all(ctype in CONTAINERS for _, _, ctype in chain[:2]):
        return None
    return False


def value_check(v: int) -> dict:
    """The counter shows exactly the word of `v` among the first five values."""
    return {"present": [CARDINALS[v]], "absent": [w for w in CARDINALS[:5] if w != CARDINALS[v]]}


def testapp_flow(focus_click: bool = False) -> dict:
    t = testapp_title
    # --focus-click is superseded by ADR-0018 and stays off: the engine now confirms keyboard
    # focus on the field between the focusing click and the keys, so no extra click is
    # needed. It is kept only so old command lines still parse. (A type that arrived in the
    # frame of its own focusing click was dropped by the field: egui handles text before it
    # grants focus; S4 report, second round.)
    focus = [(TEXT_FIELD, NOT_BY_OCR, t(0, "off", 1))] if focus_click else []
    return {
        "exe": "ommatix-testapp.exe",
        "initial": {"present": ["cero", "apagado", "Primera"],
                    "absent": ["uno", "dos", "tres", "encendido", "Segunda"]},
        # (label, OCR check afterwards, OS-side check: the window title[, text to type]).
        "steps": [
            ("Sumar", value_check(1), t(1, "off", 1)),
            ("Sumar", value_check(2), t(2, "off", 1)),
            ("Sumar", value_check(3), t(3, "off", 1)),
            ("Reiniciar", value_check(0), t(0, "off", 1)),
            ("Activar", {"present": ["encendido"], "absent": ["apagado"]}, t(0, "on", 1)),
            ("Activar", {"present": ["apagado"], "absent": ["encendido"]}, t(0, "off", 1)),
            ("Siguiente", {"present": ["Segunda"], "absent": ["Primera"]}, t(0, "off", 2)),
            ("Anterior", {"present": ["Primera"], "absent": ["Segunda"]}, t(0, "off", 1)),
            *focus,
            (TEXT_FIELD, NOT_BY_OCR, t(0, "off", 1, TYPED_TEXT), TYPED_TEXT),
        ],
        "grounding": named_control,
        "vocabulary": {"Contador", "Interruptor", "Paginador", "Sumar", "Restar", "Reiniciar",
                       "Activar", "Siguiente", "Anterior", "Valor:", "Estado:", "página",
                       "apagado", "encendido", "Primera", "Segunda", "Texto", TEXT_FIELD,
                       *CARDINALS},
    }


def candidates(spec) -> tuple[str, ...]:
    """The accessible names a step may click: its label, or an ordered tuple of alternatives."""
    return spec if isinstance(spec, tuple) else (spec,)


def pick_control(elements: list[dict], spec) -> str | None:
    """The label to click for a step: `spec` itself, or the first name of an ordered tuple of
    alternatives that exists among the confirmed elements, by exact label. Never a name that is
    not in the tuple. Uniqueness is the caller's check (`Protocol.target` stops on ambiguity)."""
    confirmed = Counter(e.get("label", "").strip() for e in elements
                        if e.get("actionability") == "confirmed")
    for label in candidates(spec):
        if confirmed.get(label):
            return label
    return None


def calculator_flow() -> dict:
    """E2E-02, the Windows 11 Calculator: 7 + 8 = 15, then clear. The buttons are selected
    by their UI Automation names, which the engine reports as accessibility labels
    (ADR-0017). The clear control is whichever of CLEAR_CONTROLS exists when it is needed:
    the third S4 round showed that "Borrar" can be absent after "Es igual a" in Scientific
    mode, so the flow does not depend on one name. The display is checked after every click
    through its UI Automation name, not by OCR: Windows OCR does not read the single-glyph
    keys (S4 report, finding 3); after the clear it must read 0."""
    return {
        "exe": "calculatorapp.exe",
        # Every key of the flow and at least one clear control must exist before any input.
        "initial": {"exact": list(CALC_BUTTONS), "any_of": CLEAR_CONTROLS},
        "initial_display": "0",
        # (label or ordered alternatives, OCR check (none), OS-side check: the display's
        # number after the click)
        "steps": [
            ("Siete", NOT_BY_OCR, "7"),
            ("Más", NOT_BY_OCR, "7"),
            ("Ocho", NOT_BY_OCR, "8"),
            ("Es igual a", NOT_BY_OCR, "15"),
            (CLEAR_CONTROLS, NOT_BY_OCR, "0"),
        ],
        "grounding": named_control,
        "vocabulary": {*CALC_BUTTONS, *CLEAR_CONTROLS},
        "vocabulary_re": None,
    }


def notepad_flow() -> dict:
    """Opens the Editar menu and closes it with a second click on Editar. `Ver` is not
    used: on this desktop a second, non-menu `Ver` was visible (S4 report). The menu's state
    is its UI Automation ExpandCollapseState: Collapsed before, Expanded after the first
    click, Collapsed after the second (never read from the screen's text)."""
    return {
        "exe": "notepad.exe",
        "initial": NOT_BY_OCR,
        "steps": [(NOTEPAD_MENU, NOT_BY_OCR, MENU_OPEN),
                  (NOTEPAD_MENU, NOT_BY_OCR, MENU_CLOSED)],
        "grounding": named_control,
        "vocabulary": set(),  # Notepad's labels are never printed (its tabs hold owner text)
        "vocabulary_re": None,
    }


def parse_menu_state(output: str) -> str | None:
    """The ExpandCollapseState a PowerShell read printed: exactly one element with the
    pattern, in a state the protocol knows. Anything else (none, several, another state) is
    None: the menu's state is unknown and nothing is assumed."""
    states = [s.strip() for s in output.strip().split(",") if s.strip()]
    if len(states) == 1 and states[0] in (MENU_OPEN, MENU_CLOSED):
        return states[0]
    return None


def menu_start_failure(state: str | None) -> str | None:
    """Why a Notepad run may not start: the Edit menu must be Collapsed, a real check (a
    first call that merely records a baseline accepted an open menu in the third round)."""
    if state == MENU_CLOSED:
        return None
    return f"the Edit menu is not closed at the start (state: {state or 'unknown'})"


def needs_menu_restore(state: str | None) -> bool:
    """Only a menu that is expanded is closed again after a failure."""
    return state == MENU_OPEN


FLOWS = {"testapp": testapp_flow, "calculator": calculator_flow, "notepad": notepad_flow}
E2E_ID = {"testapp": "E2E-01", "calculator": "E2E-02", "notepad": "E2E-03"}

# ---- the protocol ---------------------------------------------------------------------------


def log(event: str, **fields) -> None:
    print(json.dumps({"t": round(time.monotonic(), 3), "event": event, **fields},
                     ensure_ascii=False), flush=True)


def stage_ms(meta: dict) -> dict:
    """The per-stage timings of a response's `_meta` (io.ommatix/timings), in milliseconds.
    Includes `accessibility` when the stage ran (ADR-0017)."""
    return {k.removesuffix("_us"): round(v / 1000, 3)
            for k, v in meta.get(s3.TIMINGS_KEY, {}).items()}


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
        self.current: int | None = None  # the application kept between runs (--restart never)
        self.notepad: dict | None = None
        self.app_hwnd = None
        self.placements = 0

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
        obs = {"frame_seq": first["frame_seq"], "epoch": first["geometry_epoch"],
               "elements": first["total_elements"], "degraded": first["degraded"],
               "rtt_ms": round(ms, 1), "stages_ms": stage_ms(meta)}
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

    def find_target(self, elements: list[dict], spec) -> tuple[str | None, dict | None]:
        """The label a step clicks now (see pick_control) and its confirmed, unique element."""
        label = pick_control(elements, spec)
        return label, (self.target(elements, label) if label is not None else None)

    @staticmethod
    def words(elements: list[dict]) -> Counter:
        """The OCR words of every label: a control's label joins the words inside its box,
        so a state word may be alone (a text node) or part of a longer label."""
        return Counter(w for e in elements for w in e.get("label", "").split())

    def ocr_ok(self, elements: list[dict], check) -> bool:
        if check is NOT_BY_OCR:
            return True
        if isinstance(check, dict) and "exact" in check:
            labels = self.labels(elements)
            return (all(labels.get(text) for text in check["exact"])
                    and (not check.get("any_of") or any(labels.get(t) for t in check["any_of"])))
        words = self.words(elements)
        return (all(words.get(text) for text in check.get("present", ()))
                and not any(words.get(text) for text in check.get("absent", ())))

    def probe_print(self, elements: list[dict]) -> None:
        """Counts, and the attributes (never the text of other labels) of the flow's own
        fixed labels. Notepad prints counts only."""
        vocab = self.flow["vocabulary"]
        fixed = {}
        if self.args.app != "notepad":
            for label in sorted(vocab):
                rows = Counter((e["role"], e["actionability"], e["label_source"])
                               for e in elements if e.get("label", "").strip() == label)
                if rows:
                    fixed[label] = [[*key, n] for key, n in sorted(rows.items())]
        # How many elements carry each step's target label, without the label itself: a
        # target must be unique and confirmed before any live action.
        step_targets = [candidates(step[0]) for step in self.flow["steps"]]
        step_rows = [[[*key, n] for key, n in sorted(Counter(
            (e["role"], e["actionability"], e["label_source"]) for e in elements
            if e.get("label", "").strip() in names).items())] for names in step_targets]
        log("probe", fixed_labels=fixed, step_targets=step_rows,
            vocabulary_elements=sum(1 for e in elements if e.get("label", "").strip() in vocab),
            elements=len(elements),
            label_source=dict(Counter(e["label_source"] for e in elements)),
            roles=dict(Counter(e["role"] for e in elements)),
            actionability=dict(Counter(e["actionability"] for e in elements)))

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
        (ForegroundLockTimeout); the main window is brought to the front every 2 s (window
        management only: no input is sent)."""
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
                    bring_to_front(self.notepad["hwnd"])  # that window only
                else:
                    target = main_window(pid)
                    if target is not None:
                        bring_to_front(target)
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
               "actions_requested": 0, "delivered": 0, "typed_delivered": 0,
               "effect_observed": 0, "indeterminate": 0, "rejected": 0, "stale_retries": 0,
               "ocr_checks_failed": 0, "independent_checks": 0, "independent_failed": 0,
               "engine_false_success": 0, "observations": [], "actions": [], "failure": None}
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
        res["window_placements"] = self.placements
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
        snap = None
        for _ in range(6):
            snap, elements, obs = self.observe()
            res["observations"].append(obs)
            if self.ocr_ok(elements, self.flow["initial"]):
                break
            time.sleep(0.5)
        else:
            res["failure"] = "initial state not visible by OCR"
        if self.args.mode == "probe":
            self.probe_print(elements)
        if res["failure"] is None and self.args.app == "testapp":
            initial = testapp_title(0, "off", 1)
            for _ in range(10):  # the title may lag the first paint by a few frames
                if window_title(hwnd) == initial:
                    break
                time.sleep(0.2)
            if window_title(hwnd) != initial:
                fg = foreground()
                ours = fg["pid"] == pid and fg["hwnd"] == hwnd
                now = window_title(hwnd)
                # Diagnostics without text: the title's shape and the foreground's identity.
                log("initial_title_mismatch", title_chars=len(now),
                    head_matches=now.partition(TESTAPP_MARK)[0] == initial.partition(TESTAPP_MARK)[0],
                    text_chars=len(now.partition(TESTAPP_MARK)[2].rstrip("]")), ours=ours,
                    foreground_exe=fg["exe"], launched_pid_is_foreground=fg["pid"] == pid)
                res["failure"] = ("initial title differs"
                                  + ("" if ours else " (not the test application's window)"))
        if res["failure"] is None and self.args.app == "calculator":
            display = None
            for _ in range(10):  # the display may lag the first paint
                display = calculator_display()
                if display == self.flow["initial_display"]:
                    break
                time.sleep(0.5)
            if display != self.flow["initial_display"]:
                res["failure"] = "initial display differs"
        if res["failure"] is None and self.args.app == "notepad":
            # A real check of the closed menu, every run (UI Automation, not the screen).
            state = self.menu_state(hwnd)
            res["menu_state_at_start"] = state
            res["failure"] = menu_start_failure(state)
        if res["failure"] is None and self.args.app == "calculator":
            # The Calculator's real names, read before the flow: which clear controls exist
            # (fixed UI names only; the run fails before any input when none does).
            res["clear_controls_at_start"] = sorted(
                {e.get("label", "").strip() for e in elements} & set(CLEAR_CONTROLS))
        if res["failure"] is None and self.args.mode == "run":
            self.steps(res, snap, elements, hwnd, pid)
            if res["failure"] is not None and self.args.app == "notepad":
                self.restore_menu(res, hwnd)
        res["total_s"] = round(time.perf_counter() - start, 2)
        if self.args.restart == "each" or res["failure"] is not None:
            self.close(pid)
            self.current = None
        return res

    def menu_state(self, hwnd) -> str | None:
        return notepad_menu_state(hwnd)

    def soft_foreground_ok(self) -> bool:
        """Whether the target application is in front and maximized on the primary monitor.
        Unlike require_foreground it never engages the operator's stop: a cleanup that cannot
        proceed just does not click."""
        fg = foreground()
        return fg["exe"] == self.flow["exe"] and fg["maximized_primary"]

    def restore_click(self, res: dict, target: dict, snap: dict) -> dict:
        """One live click through the engine's gate and nothing else (no retry)."""
        self.spend_live()
        res["restore_actions"] = res.get("restore_actions", 0) + 1
        args = {"request_id": f"s4r2-{self.args.app}-{res['run']}-restore",
                "session_id": snap["session_id"], "snapshot_id": snap["snapshot_id"],
                "geometry_epoch": snap["geometry_epoch"],
                "target_id": target["target_id"], "expected_revision": target["revision"],
                "action": "click", "dry_run": False, "expected_effect": "visual_change"}
        out, _, meta, ms = self.srv.call("execute_action", args)
        log("menu_restore_action", run=res["run"], status=out.get("status"),
            input_delivered=out.get("input_delivered"),
            error=(out.get("error") or {}).get("code"), rtt_ms=round(ms, 1),
            stages_ms=stage_ms(meta))
        return out

    def restore_menu(self, res: dict, hwnd) -> bool:
        """After a failed Notepad run, close an Edit menu that is left expanded by clicking
        `Editar` again, through the engine's gate only. It acts only if UI Automation reports
        the menu Expanded, only when Notepad is in front, once, and it never touches the
        operator's stop. Whether it was closed is read again through UI Automation."""
        before = self.menu_state(hwnd)
        restore = {"state_before": before, "clicked": False, "state_after": before}
        res["menu_restore"] = restore
        if not needs_menu_restore(before):
            return False
        if not self.soft_foreground_ok():
            log("menu_restore_skipped", run=res["run"], reason="notepad is not in front")
            return False
        snap, elements, obs = self.observe()
        res["observations"].append(obs)
        _, target = self.find_target(elements, NOTEPAD_MENU)
        if target is None:
            log("menu_restore_skipped", run=res["run"], reason="no confirmed Editar target")
            return False
        out = self.restore_click(res, target, snap)
        restore["clicked"] = bool(out.get("input_delivered"))
        for _ in range(8):
            restore["state_after"] = self.menu_state(hwnd)
            if restore["state_after"] == MENU_CLOSED:
                break
            time.sleep(0.25)
        log("menu_restore", run=res["run"], **restore)
        return restore["state_after"] == MENU_CLOSED

    def read_end_state(self, hwnd) -> str | None:
        """The OS-side state of the application under test."""
        if self.args.app == "testapp":
            return window_title(hwnd)
        if self.args.app == "notepad":
            return self.menu_state(hwnd)
        return calculator_display()

    def os_check(self, hwnd, expected: str) -> bool:
        """The end state read by the OS, not by the engine: the test application's window
        title, the Calculator display's UI Automation name or Notepad's menu ExpandCollapse
        state. Read up to 8 times, since a repaint may lag the action."""
        now = None
        for _ in range(8):
            now = self.read_end_state(hwnd)
            if now == expected:
                return True
            time.sleep(0.25)
        if self.args.app == "testapp" and now is not None and TESTAPP_MARK in expected:
            # Diagnostics without text (ADR-0012): how the title's typed part compares.
            typed_now = now.partition(TESTAPP_MARK)[2].rstrip("]")
            typed_expected = expected.partition(TESTAPP_MARK)[2].rstrip("]")
            log("os_check_mismatch", title_chars=len(now), text_chars=len(typed_now),
                expected_text_chars=len(typed_expected),
                text_is_prefix=typed_expected.startswith(typed_now),
                text_is_suffix=typed_expected.endswith(typed_now),
                head_matches=now.partition(TESTAPP_MARK)[0] == expected.partition(TESTAPP_MARK)[0])
        return False

    def steps(self, res: dict, snap: dict, elements: list[dict], hwnd, pid) -> None:
        for step in self.flow["steps"]:
            label, ocr_check, independent, *rest = step
            text = rest[0] if rest else None  # a typing step has its text
            res["steps"] += 1
            spec = label  # a name, or ordered alternatives (the clear control)
            target = None
            for _ in range(3):
                label, target = self.find_target(elements, spec)
                if target is not None:
                    break
                time.sleep(0.3)
                snap, elements, obs = self.observe()
                res["observations"].append(obs)
            if target is None:
                res["failure"] = f"step {res['steps']}: target label not found or not confirmed"
                if isinstance(spec, tuple):  # which of the fixed alternatives exist at all
                    res["alternatives_present"] = sorted(
                        {e.get("label", "").strip() for e in elements} & set(spec))
                return
            if isinstance(spec, tuple):
                res.setdefault("resolved", []).append(label)
            if text is not None and target["role"] != "input":
                res["failure"] = f"step {res['steps']}: the typing target is not a confirmed input"
                return
            out = None
            for attempt in range(4):  # again only after STALE_SNAPSHOT (nothing delivered)
                self.require_foreground()
                self.spend_live()
                res["actions_requested"] += 1
                kind = "click" if text is None else "type"
                # Both ask for a visual change. Since ADR-0018 the engine measures a typing's
                # change against a frame captured after its focusing click and before its keys,
                # so the click's own redraw is not counted (it was in the pilot, S4 report
                # §4.6). The OS-side check (the window title) still decides; the engine's
                # effect_observed is only evidence that something changed after the keys.
                effect = "visual_change"
                args = {"request_id": f"s4r2-{self.args.app}-{res['run']}-{res['steps']}-{attempt + 1}",
                        "session_id": snap["session_id"], "snapshot_id": snap["snapshot_id"],
                        "geometry_epoch": snap["geometry_epoch"],
                        "target_id": target["target_id"], "expected_revision": target["revision"],
                        "action": kind, "dry_run": False, "expected_effect": effect}
                if text is not None:
                    args["text"] = text
                out, _, meta, ms = self.srv.call("execute_action", args)
                err = (out.get("error") or {}).get("code")
                act = {"step": res["steps"], "action": kind, "expected_effect": effect,
                       "status": out.get("status"),
                       "input_delivered": out.get("input_delivered"), "verified": out.get("verified"),
                       "observed_effect": out.get("observed_effect"), "error": err,
                       "rtt_ms": round(ms, 1), "stages_ms": stage_ms(meta),
                       "target_role": target["role"], "label_source": target["label_source"]}
                res["actions"].append(act)
                log("action", run=res["run"], **act)
                if out.get("input_delivered"):
                    res["delivered"] += 1
                    if text is not None:
                        res["typed_delivered"] += 1
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
                    label, target = self.find_target(elements, spec)
                    if target is None:
                        res["failure"] = f"step {res['steps']}: target lost after STALE_SNAPSHOT"
                        return
                    continue
                break
            if out.get("status") == "rejected":
                res["failure"] = f"step {res['steps']}: rejected ({err})"
                if err == "POLICY_DENIED":
                    self.engage_stop("POLICY_DENIED during the protocol")
                return
            # Independent grounding: which control is under the cursor (UI Automation).
            if out.get("input_delivered") and self.flow.get("grounding"):
                chain = uia_at_cursor()
                hit = self.flow["grounding"](label, chain)
                if hit is None:
                    count, inside = uia_named_contains_cursor(self.app_hwnd, label)
                    hit = None if count == 0 else inside > 0
                    res.setdefault("grounding_by_rect", []).append([count, inside])
                res.setdefault("grounding", []).append(hit)
                if hit is False:
                    res["grounding_wrong"] = res.get("grounding_wrong", 0) + 1
                if self.args.debug_grounding:
                    log("grounding_chain", step=res["steps"], hit=hit,
                        chain=[(aid, ctype, name == label) for aid, name, ctype in chain])
            # Re-observe (the next step needs fresh targets) and check the visible text.
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
                indep_ok = self.os_check(hwnd, independent)
                if not indep_ok:
                    res["independent_failed"] += 1
            grounded = (res.get("grounding") or [None])[-1] if out.get("input_delivered") else None
            # A false success: the engine reported the effect while the OS-side evidence or
            # the control under the cursor says the intended effect did not happen.
            if out.get("status") == "effect_observed" and (not indep_ok or grounded is False):
                res["engine_false_success"] += 1
            if ocr_check is not NOT_BY_OCR:  # OCR counters only where OCR decides a step
                if not passed and indep_ok and grounded is not False:
                    res["ocr_false_negative"] = res.get("ocr_false_negative", 0) + 1
                if passed and (not indep_ok or grounded is False):
                    res["ocr_false_positive"] = res.get("ocr_false_positive", 0) + 1
            if not passed and self.args.app == "testapp":
                vocab = self.flow["vocabulary"]
                log("ocr_check_failed", run=res["run"], step=res["steps"],
                    matched=sum(1 for k in self.words(elements) if k in vocab))
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
    p.add_argument("--session-id", default="s4r2")
    p.add_argument("--output", type=int, default=0)
    p.add_argument("--foreground-wait", type=float, default=600.0)
    p.add_argument("--pause", type=float, default=1.0)
    p.add_argument("--settle", type=float, default=1.5,
                   help="seconds to wait after a newly started window reaches the foreground")
    p.add_argument("--focus-click", action="store_true",
                   help="SUPERSEDED by ADR-0018 (the engine confirms keyboard focus before "
                        "typing); off by default and not to be used. testapp: click the field "
                        "once before typing")
    p.add_argument("--debug-grounding", action="store_true",
                   help="log the AutomationId and ControlType under the cursor and whether each name matches (never names)")
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
    if args.focus_click:
        log("warning", reason="--focus-click is superseded by ADR-0018 and should not be used")
    flow = testapp_flow(args.focus_click) if args.app == "testapp" else FLOWS[args.app]()
    log("start", app=args.app, mode=args.mode, runs=args.runs, live_input=live,
        focus_click=args.focus_click,
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
