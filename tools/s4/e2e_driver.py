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
accesibilidad (ADR-0017)"). The reduced flows have their own identifiers, S4R-02 and S4R-03: the
dossier's E2E-02 (editor) and E2E-03 (file explorer) are other cases. The records of earlier
rounds call them E2E-02 and E2E-03 (field ``e2e``, kept so that evidence stays readable); the
records now also carry ``flow_id``:

``testapp``    E2E-01, the controlled test application ``tools/s4/testapp``: the first-round
               click flow (``Sumar`` x3, ``Reiniciar``, ``Activar`` x2, ``Siguiente``,
               ``Anterior``), then a typing step into its text field ``Campo de texto`` (the
               text ``prueba 123``). The OS-side check is the window title, which mirrors the
               state and the typed text (``GetWindowTextW``) after every step.
``calculator`` S4R-02, the Windows 11 Calculator: ``Siete``, ``Más``, ``Ocho``, ``Es igual a``
               (the display must read 15), then the clear control. The clear control is not a
               fixed name: the Calculator's real accessibility names are read from
               ``observe_screen`` before the flow and again before the last click, and the first
               name of ``CLEAR_CONTROLS`` (``Borrar``, ``Borrar todo``, ``Borrar entrada``) that
               exists as a confirmed element is clicked. With none of them in the initial state
               the run fails before any input. The OS-side check is the UI Automation name of
               the display after every click, and it must return to 0 after the clear.
``notepad``    S4R-03, Notepad: open the ``Editar`` menu and close it with a second click on
               ``Editar``. No menu item, tab or caption button is clicked and nothing is typed.
               The menu is "open" or "closed" by its UI Automation ExpandCollapseState, never by
               screen text. Every run starts by checking that the menu is Collapsed, and a run
               that fails with the menu left expanded closes it with one more click on ``Editar``
               through the engine's gate (never a retry loop, and only if it is expanded).

Grounding is by identity, never by name (S4 fourth round, finding 4). Before each click, UI
Automation finds, on the OS side, the one element of the foreground window of the target
application with the requested exact name and the control type that the engine's role stands
for, and records its RuntimeId and AutomationId. More than one match, none, or none of the two
identifiers means no click: the run fails before any input. After the click the element under
the cursor (where SendInput left it), or its parent, must have the same RuntimeId, or the same
AutomationId when the RuntimeId is unavailable, even if its name changed (a toggle such as
``Abrir navegación`` becomes ``Cerrar navegación``). There is no name-only fallback. When the
point query returns a XAML island host (Notepad's menu items are never returned by it), the
client descends into the host's bounded subtree and grounds on the deepest on-screen element
under the point (ISLAND_HOST_CLASSES, ``descend``; sixth round, finding 1). An
``effect_observed`` whose OS-side or grounding check fails is counted as an engine false
success; there must be none.

``--calc-standard`` is a separate invocation, run once before the S4R-02 series: it switches
the Calculator to Standard mode with two gated clicks, each with identity grounding: the
navigation toggle, then the pane's Standard item. After the first click it polls UI Automation
(read-only, 3 s in steps of 150 ms) for the one on-screen element with AutomationId ``Standard``
of a list-item-like control type, accepts its name only from a closed set (``Estándar``,
``Calculadora estándar``; any other name is never recorded), selects the engine's element by
that exact name and requires the click to reach that very element. It verifies the mode through
UI Automation, also when it stops, and closes the Calculator it started.

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
# The owner's ceiling for the live requests of one round (at most 400). The ledger only
# accumulates across rounds; the round's share is ``ledger - round_start`` (``--round-start``),
# reported as ledger_start..ledger_end.
LIVE_CAP = 400
# The step that switches the Calculator to Standard mode sends two clicks; the rest is room for
# one retry after STALE_SNAPSHOT (nothing delivered).
CALC_STANDARD_MAX_LIVE = 4
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


# The element at a point and two ancestors, as ``RuntimeId, AutomationId, ControlType,
# ClassName`` rows. The class name is how the client recognises a XAML island host
# (ISLAND_HOST_CLASSES); the name of an element is never read.
UIA_AT_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$p = New-Object System.Windows.Point([double]__X__, [double]__Y__)
$e = [System.Windows.Automation.AutomationElement]::FromPoint($p)
$rows = @()
for ($i = 0; $e -and $i -lt 3; $i++) {
  $rid = ''
  try { $rid = ($e.GetRuntimeId() -join '.') } catch {}
  $type = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  $rows += ($rid + "`t" + $e.Current.AutomationId + "`t" + $type + "`t" + $e.Current.ClassName)
  $e = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($e)
}
[Console]::Out.Write(($rows -join "`n"))
"""

# The ControlView subtree of the XAML island host under a point (S4 sixth round, finding 1):
# one row per element in depth-first order, ``index, parent, depth, RuntimeId, AutomationId,
# ControlType, ClassName, IsOffscreen, X, Y, Width, Height``, bounded to __MAXN__ elements below
# the host and __MAXD__ levels, then ``#end<TAB>1`` when the bound cut the walk and ``#end<TAB>0``
# when it did not. The host is found again by point and must keep its RuntimeId, else the script
# prints ``#host_changed``. It reads identifiers, types, classes and rectangles: never a name,
# a value or a pattern, and it sends nothing.
UIA_ISLAND_WALK_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$ErrorActionPreference = 'Stop'
$p = New-Object System.Windows.Point([double]__X__, [double]__Y__)
$island = [System.Windows.Automation.AutomationElement]::FromPoint($p)
$hostRid = ''
if ($island) { try { $hostRid = ($island.GetRuntimeId() -join '.') } catch {} }
if ($hostRid -ne '__HOST_RID__') { [Console]::Out.Write('#host_changed'); exit }
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$script:rows = New-Object System.Collections.ArrayList
$script:truncated = 0
function Add-Row($el, $depth, $parent) {
  $rid = ''
  try { $rid = ($el.GetRuntimeId() -join '.') } catch {}
  $type = $el.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  $r = $el.Current.BoundingRectangle
  $x = 0; $y = 0; $w = 0; $h = 0
  if (-not $r.IsEmpty) {
    $x = [int][Math]::Round($r.X); $y = [int][Math]::Round($r.Y)
    $w = [int][Math]::Round($r.Width); $h = [int][Math]::Round($r.Height)
  }
  $index = $script:rows.Count
  [void]$script:rows.Add(($index.ToString() + "`t" + $parent + "`t" + $depth + "`t" + $rid + "`t" +
    $el.Current.AutomationId + "`t" + $type + "`t" + $el.Current.ClassName + "`t" +
    [int]$el.Current.IsOffscreen + "`t" + $x + "`t" + $y + "`t" + $w + "`t" + $h))
  return $index
}
function Walk-Children($el, $depth, $index) {
  $child = $walker.GetFirstChild($el)
  while ($child) {
    if ($depth -ge __MAXD__ -or ($script:rows.Count - 1) -ge __MAXN__) { $script:truncated = 1; return }
    $i = Add-Row $child ($depth + 1) $index
    Walk-Children $child ($depth + 1) $i
    if ($script:truncated) { return }
    $child = $walker.GetNextSibling($child)
  }
}
$root = Add-Row $island 0 -1
Walk-Children $island 0 $root
[void]$script:rows.Add("#end`t" + $script:truncated)
[Console]::Out.Write(($script:rows -join "`n"))
"""

# The centre of the one on-screen element of the window with the exact name and one of the
# control types, as ``X<TAB>Y``; nothing when it is not exactly one. No name is printed.
UIA_ELEMENT_CENTRE_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__NAME__'))
$types = '__TYPES__'.Split(',')
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $name)
$found = @()
foreach ($c in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
  $type = $c.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  if (($types -contains $type) -and -not $c.Current.IsOffscreen) { $found += $c }
}
if ($found.Count -eq 1) {
  $r = $found[0].Current.BoundingRectangle
  [Console]::Out.Write([string][int][Math]::Round($r.X + $r.Width / 2) + "`t" +
    [string][int][Math]::Round($r.Y + $r.Height / 2))
}
"""

# The elements of a window with an exact name and one of the given control types. Names are
# never printed: only the identifiers, the control type and whether the element is off screen.
UIA_IDENTITY_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__NAME__'))
$types = '__TYPES__'.Split(',')
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $name)
$rows = @()
foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
  $type = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  if ($types -notcontains $type) { continue }
  $rid = ''
  try { $rid = ($e.GetRuntimeId() -join '.') } catch {}
  $rows += ($rid + "`t" + $e.Current.AutomationId + "`t" + $type + "`t" + [int]$e.Current.IsOffscreen)
}
[Console]::Out.Write(($rows -join "`n"))
"""

# The Calculator's mode, read through UI Automation: the header's name (AutomationId
# "Header") and how many of the Scientific-only controls are on screen.
CALC_MODE_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$header = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
  (New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'Header')))
$sci = 0
foreach ($b in '__SCIENTIFIC__'.Split(',')) {
  $name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($b))
  $cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $name)
  foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
    if (-not $e.Current.IsOffscreen) { $sci++ }
  }
}
if ($header) { [Console]::Out.Write("1`t" + $header.Current.Name + "`t" + $sci) }
else { [Console]::Out.Write("0`t`t" + $sci) }
"""

# The elements of a window with an exact AutomationId, as ``RuntimeId, AutomationId, ControlType,
# IsOffscreen, Name`` rows with the name in base64 (a tab or a newline in a name must not break the
# row). Read-only; the client records the name only when it is in CALC_STANDARD_NAMES.
UIA_BY_AUTOMATION_ID_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::AutomationIdProperty, '__AID__')
$rows = @()
foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
  $type = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  $rid = ''
  try { $rid = ($e.GetRuntimeId() -join '.') } catch {}
  $name = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($e.Current.Name))
  $rows += ($rid + "`t" + $e.Current.AutomationId + "`t" + $type + "`t" + [int]$e.Current.IsOffscreen +
    "`t" + $name)
}
[Console]::Out.Write(($rows -join "`n"))
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

# Read-only diagnostic of Notepad's menu step (S4 fifth round, finding 1): the element under a
# point and up to 4 ancestors, as ``RuntimeId, AutomationId, ControlType, ClassName,
# ExpandCollapseState`` rows. Never a name or a value: identifiers, types and states only.
UIA_DIAG_HEAD_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
"""
UIA_DIAG_WALK_PS = r"""
$rows = @()
for ($i = 0; $e -and $i -lt 5; $i++) {
  $rid = ''
  try { $rid = ($e.GetRuntimeId() -join '.') } catch {}
  $type = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  $state = ''
  try {
    $pattern = $null
    if ($e.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
      $state = $pattern.Current.ExpandCollapseState.ToString()
    }
  } catch {}
  $rows += ($rid + "`t" + $e.Current.AutomationId + "`t" + $type + "`t" + $e.Current.ClassName + "`t" + $state)
  $e = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($e)
}
[Console]::Out.Write(($rows -join "`n"))
"""
# The point is the centre of the one on-screen element of the window with the exact name and
# one of the control types (the element the click is about to reach); no row otherwise.
UIA_DIAG_AT_ELEMENT_PS = UIA_DIAG_HEAD_PS + r"""
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]__HWND__)
$name = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__NAME__'))
$types = '__TYPES__'.Split(',')
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $name)
$found = @()
foreach ($c in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
  $type = $c.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  if (($types -contains $type) -and -not $c.Current.IsOffscreen) { $found += $c }
}
$e = $null
if ($found.Count -eq 1) {
  $r = $found[0].Current.BoundingRectangle
  $p = New-Object System.Windows.Point(($r.X + $r.Width / 2), ($r.Y + $r.Height / 2))
  $e = [System.Windows.Automation.AutomationElement]::FromPoint($p)
}
""" + UIA_DIAG_WALK_PS
UIA_DIAG_AT_POINT_PS = UIA_DIAG_HEAD_PS + r"""
$p = New-Object System.Windows.Point([double]__X__, [double]__Y__)
$e = [System.Windows.Automation.AutomationElement]::FromPoint($p)
""" + UIA_DIAG_WALK_PS

user32.GetCursorPos.argtypes = [ctypes.POINTER(wintypes.POINT)]


def cursor() -> tuple[int, int]:
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    return int(pt.x), int(pt.y)


def uia_identify(hwnd, label: str, control_types: tuple[str, ...]) -> list[dict] | None:
    """The elements of window `hwnd` named exactly `label` whose UI Automation control type is
    one of `control_types`, as identity rows (see parse_identity_rows); None when the read
    failed. An OS-side read: no input, and the name is never printed."""
    script = (UIA_IDENTITY_PS.replace("__HWND__", str(int(hwnd)))
              .replace("__NAME__", base64.b64encode(label.encode("utf-8")).decode("ascii"))
              .replace("__TYPES__", ",".join(control_types)))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_identity_rows(out)


def uia_standard_items(hwnd) -> list[dict] | None:
    """The elements of window `hwnd` whose AutomationId is CALC_STANDARD_AUTOMATION_ID (see
    parse_standard_item_rows); None when the read failed. An OS-side read: no input."""
    script = (UIA_BY_AUTOMATION_ID_PS.replace("__HWND__", str(int(hwnd)))
              .replace("__AID__", CALC_STANDARD_AUTOMATION_ID))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_standard_item_rows(out)


def read_calc_mode(hwnd) -> str:
    """The Calculator's mode through UI Automation (see calc_mode): standard, other, unknown."""
    script = (CALC_MODE_PS.replace("__HWND__", str(int(hwnd)))
              .replace("__SCIENTIFIC__", ",".join(
                  base64.b64encode(n.encode("utf-8")).decode("ascii") for n in SCIENTIFIC_ONLY)))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return "unknown"
    parsed = parse_calc_mode(out)
    return "unknown" if parsed is None else calc_mode(*parsed)


def uia_chain_at(x: int, y: int) -> list[dict]:
    """The UI Automation element at a point and its two ancestors, as rows (RuntimeId,
    AutomationId, ControlType, ClassName; never the name). SendInput leaves the cursor on the
    click point, so this tells, independently of the engine, which control the click reached
    (see ground_chain for the elements inside a XAML island)."""
    script = UIA_AT_PS.replace("__X__", str(int(x))).replace("__Y__", str(int(y)))
    out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                         capture_output=True, text=True, encoding="utf-8", errors="replace",
                         check=False,
                         creationflags=CREATE_NO_WINDOW, timeout=30).stdout
    return parse_point_rows(out)


def island_walk_script(x: int, y: int, host_runtime_id: str) -> str:
    return (UIA_ISLAND_WALK_PS.replace("__X__", str(int(x))).replace("__Y__", str(int(y)))
            .replace("__HOST_RID__", host_runtime_id)
            .replace("__MAXN__", str(DESCENT_MAX_ELEMENTS))
            .replace("__MAXD__", str(DESCENT_MAX_DEPTH)))


def uia_island_walk(x: int, y: int, host_runtime_id: str) -> tuple[list[dict], bool] | None:
    """The bounded ControlView subtree of the island host at a point (see UIA_ISLAND_WALK_PS):
    its rows and whether the bound cut the walk, or None when it could not be read. An OS-side
    read: no input, and no name."""
    if not re.fullmatch(r"[0-9.\-]*", host_runtime_id):  # only ever interpolated as digits
        return None
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command",
                              island_walk_script(x, y, host_runtime_id)],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_island_walk(out)


def uia_element_centre(hwnd, label: str, control_types: tuple[str, ...]) -> tuple[int, int] | None:
    """The centre of the one on-screen element of `hwnd` named `label` with one of the control
    types, or None when it is not exactly one or unreadable. The name is never printed."""
    script = (UIA_ELEMENT_CENTRE_PS.replace("__HWND__", str(int(hwnd)))
              .replace("__NAME__", base64.b64encode(label.encode("utf-8")).decode("ascii"))
              .replace("__TYPES__", ",".join(control_types)))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    parts = out.strip().split("\t")
    if len(parts) != 2 or not all(re.fullmatch(r"-?\d+", part) for part in parts):
        return None
    return int(parts[0]), int(parts[1])


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


def parse_diag_rows(output: str) -> list[dict]:
    """Rows of ``RuntimeId<TAB>AutomationId<TAB>ControlType<TAB>ClassName<TAB>State`` as printed
    by the diagnostic scripts; a row of another shape is dropped."""
    rows = []
    for line in output.splitlines():
        parts = line.rstrip("\r").split("\t")
        if len(parts) != 5:
            continue
        rows.append({"runtime_id": parts[0].strip(), "automation_id": parts[1].strip(),
                     "control_type": parts[2].strip().removeprefix("ControlType."),
                     "class_name": parts[3].strip(), "expand_state": parts[4].strip()})
    return rows


def run_diag(script: str) -> list[dict] | None:
    """One read-only diagnostic script; None when it timed out. No input is sent."""
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=CREATE_NO_WINDOW, timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_diag_rows(out)


def uia_diag_at_element(hwnd, label: str, control_types: tuple[str, ...]) -> list[dict] | None:
    """The diagnostic chain under the centre of the one on-screen element of `hwnd` named
    `label` (before the click). The name is never printed."""
    return run_diag(UIA_DIAG_AT_ELEMENT_PS.replace("__HWND__", str(int(hwnd)))
                    .replace("__NAME__", base64.b64encode(label.encode("utf-8")).decode("ascii"))
                    .replace("__TYPES__", ",".join(control_types)))


def uia_diag_at_cursor() -> list[dict] | None:
    """The diagnostic chain under the mouse cursor (after the click, where SendInput left it)."""
    x, y = cursor()
    return run_diag(UIA_DIAG_AT_POINT_PS.replace("__X__", str(x)).replace("__Y__", str(y)))


def diag_record(rows: list[dict], expected_runtime_id: str) -> list[dict]:
    """The recorded form of a diagnostic chain: per element its depth (0 is the element under
    the point), control type, automation id, class name, ExpandCollapse state and whether its
    RuntimeId is the one seen before the click. The RuntimeIds themselves are not recorded."""
    return [{"depth": depth, "control_type": row["control_type"],
             "automation_id": row["automation_id"], "class_name": row["class_name"],
             "expand_state": row["expand_state"],
             "same_runtime_id": bool(expected_runtime_id and row["runtime_id"]
                                     and expected_runtime_id == row["runtime_id"])}
            for depth, row in enumerate(rows[:5])]


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
# The navigation toggle in its closed state and the Standard entry of the pane, by exact label
# (Spanish Windows names). Scientific-only controls (present only in that mode) and the header
# texts that stand for Standard mode.
CALC_NAV_LABEL = "Abrir navegación"
# The pane's Standard item is found by its AutomationId in UI Automation (seventh round, finding
# C1), never by its text alone: its Name is accepted only from a closed set, and only a control
# type of a closed list of list-item-like controls counts. It is polled for a bounded time after
# the navigation click, because the pane opens with an animation.
CALC_STANDARD_AUTOMATION_ID = "Standard"
CALC_STANDARD_NAMES = ("Estándar", "Calculadora estándar")
STANDARD_ITEM_TYPES = ("ListItem", "TreeItem", "TabItem", "MenuItem", "RadioButton")
STANDARD_POLL_WINDOW_S = 3.0
STANDARD_POLL_STEP_S = 0.15
SCIENTIFIC_ONLY = ("Alternar grados", "Notación científica")
STANDARD_HEADERS = ("estándar", "calculadora estándar")
CARDINALS = ("cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve")
NOTEPAD_MENU = "Editar"
# UI Automation ExpandCollapseState of the menu bar item: the OS-side truth of "open" and
# "closed" (screen text never decides it).
MENU_OPEN, MENU_CLOSED = "Expanded", "Collapsed"
# A step whose end state the OCR does not check: only its OS-side check decides.
NOT_BY_OCR = {"present": (), "absent": ()}

def testapp_title(value: int, state: str, page: int, text: str = "") -> str:
    return TESTAPP_TITLE.format(value, state, page, text)


# The UI Automation control types that each contract role stands for: the inverse of the
# accessibility adapter's `role_of` (src/adapters/accessibility/src/lib.rs). A role the table
# does not know (`text_node`, `icon_button`) has no UI Automation control and is never clicked.
ROLE_CONTROL_TYPES = {
    "button": ("Button", "SplitButton"),
    "input": ("Edit", "Document"),
    "checkbox": ("CheckBox",),
    "dropdown": ("ComboBox",),
    "unknown": ("Edit", "Document", "RadioButton", "MenuItem", "TabItem", "ListItem",
                "TreeItem", "DataItem", "Hyperlink", "HeaderItem"),
}


# Every UI Automation control type, for the read-only diagnosis of a step that found no
# usable target (counts and types of the elements of the window with the step's name).
ANY_CONTROL_TYPE = (
    "AppBar", "Button", "Calendar", "CheckBox", "ComboBox", "Custom", "DataGrid", "DataItem",
    "Document", "Edit", "Group", "Header", "HeaderItem", "Hyperlink", "Image", "List",
    "ListItem", "Menu", "MenuBar", "MenuItem", "Pane", "ProgressBar", "RadioButton",
    "ScrollBar", "SemanticZoom", "Separator", "Slider", "Spinner", "SplitButton", "StatusBar",
    "Tab", "TabItem", "Table", "Text", "Thumb", "TitleBar", "ToolBar", "ToolTip", "Tree",
    "TreeItem", "Window")


def parse_identity_rows(output: str) -> list[dict]:
    """Rows of ``RuntimeId<TAB>AutomationId<TAB>ControlType[<TAB>IsOffscreen]`` as printed by
    the UI Automation scripts. A row that does not have that shape is dropped."""
    rows = []
    for line in output.splitlines():
        parts = line.rstrip("\r").split("\t")
        if len(parts) not in (3, 4):
            continue
        rows.append({"runtime_id": parts[0].strip(), "automation_id": parts[1].strip(),
                     "control_type": parts[2].strip().removeprefix("ControlType."),
                     "offscreen": len(parts) == 4 and parts[3].strip() == "1"})
    return rows


def identity_decision(rows: list[dict] | None) -> tuple[str, dict | None]:
    """Whether a click may go ahead, from the on-screen elements that match the target's name
    and control type: ``ok`` only for exactly one that has a RuntimeId or an AutomationId.
    ``unreadable``, ``not_found``, ``ambiguous`` and ``unidentifiable`` mean no click. An
    off-screen element cannot receive the engine's click, so it is not a candidate."""
    if rows is None:
        return "unreadable", None
    candidates = [r for r in rows if not r["offscreen"]]
    if not candidates:
        return "not_found", None
    if len(candidates) > 1:
        return "ambiguous", None
    found = candidates[0]
    if not found["runtime_id"] and not found["automation_id"]:
        return "unidentifiable", None
    return "ok", found


def parse_standard_item_rows(output: str) -> list[dict]:
    """Rows of ``RuntimeId<TAB>AutomationId<TAB>ControlType<TAB>IsOffscreen<TAB>Name(base64)`` as
    printed by UIA_BY_AUTOMATION_ID_PS; a row of another shape is dropped, and a name that is
    not valid base64 UTF-8 is kept as an empty name (which is in no set)."""
    rows = []
    for line in output.splitlines():
        parts = line.rstrip("\r").split("\t")
        if len(parts) != 5:
            continue
        try:
            name = base64.b64decode(parts[4].strip(), validate=True).decode("utf-8")
        except ValueError:  # binascii.Error and UnicodeDecodeError are ValueErrors
            name = ""
        rows.append({"runtime_id": parts[0].strip(), "automation_id": parts[1].strip(),
                     "control_type": parts[2].strip().removeprefix("ControlType."),
                     "offscreen": parts[3].strip() == "1", "name": name})
    return rows


def is_standard_item_row(row: dict) -> bool:
    """A row with the Standard item's AutomationId and a control type of STANDARD_ITEM_TYPES."""
    return (row["automation_id"] == CALC_STANDARD_AUTOMATION_ID
            and row["control_type"] in STANDARD_ITEM_TYPES)


def accepted_standard_name(name: str) -> str | None:
    """The canonical (NFC) name when it is exactly one of CALC_STANDARD_NAMES, else None."""
    name = unicodedata.normalize("NFC", name)
    return name if name in CALC_STANDARD_NAMES else None


def standard_item_decision(rows: list[dict] | None) -> tuple[str, dict | None]:
    """Whether the pane's Standard item was found: ``ok`` only for exactly one on-screen item
    with a RuntimeId and a Name in CALC_STANDARD_NAMES (compared after NFC, case and spaces
    exactly); the item returned carries that canonical name. ``unreadable``, ``not_found``,
    ``ambiguous``, ``unidentifiable`` and ``unknown_name`` all mean no click. An off-screen
    element cannot receive the engine's click, so it is not a candidate."""
    if rows is None:
        return "unreadable", None
    found = [r for r in rows if is_standard_item_row(r) and not r["offscreen"]]
    if not found:
        return "not_found", None
    if len(found) > 1:
        return "ambiguous", None
    item = found[0]
    if not item["runtime_id"]:
        return "unidentifiable", None
    name = accepted_standard_name(item["name"])
    if name is None:
        return "unknown_name", None
    return "ok", {**item, "name": name}


def standard_item_record(status: str, rows: list[dict] | None, polls: int, waited_s: float) -> dict:
    """What the poll for the Standard item records: the decision, counts, the item's control
    type and AutomationId when exactly one, and its name only when it is in CALC_STANDARD_NAMES
    (a name outside the set is never recorded)."""
    found = [r for r in rows or [] if is_standard_item_row(r) and not r["offscreen"]]
    record = {"status": status, "polls": polls, "waited_ms": round(waited_s * 1000),
              "window_s": STANDARD_POLL_WINDOW_S, "step_s": STANDARD_POLL_STEP_S,
              "candidates": len(found),
              "offscreen_ignored": sum(1 for r in rows or []
                                       if is_standard_item_row(r) and r["offscreen"]),
              "control_type": None, "automation_id": None, "name_accepted": False, "name": None,
              "stopped_by": None}
    if len(found) == 1:
        name = accepted_standard_name(found[0]["name"])
        record.update(control_type=found[0]["control_type"],
                      automation_id=found[0]["automation_id"],
                      name_accepted=name is not None, name=name)
    return record


def identity_hit(expected: dict, chain: list[dict]) -> bool:
    """Whether the control under the cursor (or its parent) is the element found before the
    click: the same RuntimeId, or the same AutomationId when the RuntimeId is unavailable on
    either side. The name is never compared, so a toggle that renames itself still matches.
    An unreadable chain does not confirm anything."""
    for row in chain[:2]:
        if expected["runtime_id"] and row["runtime_id"]:
            if expected["runtime_id"] == row["runtime_id"]:
                return True
        elif expected["automation_id"] and expected["automation_id"] == row["automation_id"]:
            return True
    return False


# ---- XAML islands (S4 sixth round, finding 1; docs/reviews/2026-10-14-s4-fifth-round.md) ----
#
# In Notepad the point query (``AutomationElement.FromPoint``) never returns the menu item: it
# returns the XAML island host (a Pane of class ``Microsoft.UI.Content.DesktopChildSiteBridge``),
# the window and the desktop, so "the element under the cursor or its parent" cannot confirm it.
# The rule: when the element the point query returns is an island host (a closed list), the
# client descends into the host's ControlView subtree, bounded, and takes the DEEPEST on-screen
# element whose BoundingRectangle contains the point. Two or more elements at that deepest level
# (elements at one level are never nested in each other) are ambiguous; a walk that reached the
# bound, an unreadable walk and a host that changed confirm nothing. The chain is that element
# followed by its ancestors up to the host, and identity_hit applies to it unchanged. Any other
# class of element: the point query's chain, exactly as before.
ISLAND_HOST_CLASSES = ("Microsoft.UI.Content.DesktopChildSiteBridge",)
DESCENT_MAX_ELEMENTS = 500  # elements visited below the host
DESCENT_MAX_DEPTH = 16  # levels below the host
NO_DESCENT = {"descended": False}


def parse_point_rows(output: str) -> list[dict]:
    """Rows of ``RuntimeId<TAB>AutomationId<TAB>ControlType<TAB>ClassName`` as printed by
    UIA_AT_PS; a row of another shape is dropped."""
    rows = []
    for line in output.splitlines():
        parts = line.rstrip("\r").split("\t")
        if len(parts) != 4:
            continue
        rows.append({"runtime_id": parts[0].strip(), "automation_id": parts[1].strip(),
                     "control_type": parts[2].strip().removeprefix("ControlType."),
                     "class_name": parts[3].strip(), "offscreen": False})
    return rows


def parse_island_walk(output: str) -> tuple[list[dict], bool] | None:
    """The rows and the truncation flag printed by UIA_ISLAND_WALK_PS, or None when the output
    is not a complete, well-formed walk (no ``#end`` line, a row of another shape, a parent that
    does not precede its child, a depth that does not follow its parent's)."""
    rows: list[dict] = []
    truncated = None
    for line in output.splitlines():
        parts = line.rstrip("\r").split("\t")
        if truncated is not None:
            return None  # nothing follows the end line
        if parts[0] == "#end":
            if len(parts) != 2 or parts[1] not in ("0", "1"):
                return None
            truncated = parts[1] == "1"
            continue
        if len(parts) != 12:
            return None
        try:
            index, parent, depth, x, y, w, h = (int(parts[i]) for i in (0, 1, 2, 8, 9, 10, 11))
        except ValueError:
            return None
        if index != len(rows) or (index == 0) != (parent == -1) or parent >= index:
            return None
        if depth != (0 if index == 0 else rows[parent]["depth"] + 1):
            return None
        rows.append({"index": index, "parent": parent, "depth": depth,
                     "runtime_id": parts[3].strip(), "automation_id": parts[4].strip(),
                     "control_type": parts[5].strip().removeprefix("ControlType."),
                     "class_name": parts[6].strip(), "offscreen": parts[7].strip() == "1",
                     "rect": (x, y, w, h)})
    if truncated is None or not rows:
        return None
    return rows, truncated


def is_island_host(row: dict | None) -> bool:
    """Whether a point query's element is a XAML island host (a closed, explicit class list)."""
    return bool(row) and row.get("class_name", "") in ISLAND_HOST_CLASSES


def rect_contains(rect: tuple[int, int, int, int], point: tuple[int, int]) -> bool:
    x, y, w, h = rect
    return w > 0 and h > 0 and x <= point[0] < x + w and y <= point[1] < y + h


def descend(point: tuple[int, int], host: dict, read_walk) -> tuple[list[dict], dict]:
    """The island rule. `read_walk(x, y, host_runtime_id)` returns the bounded walk of the host
    (rows, truncated) or None. Returns (chain, record): the chain is the deepest on-screen
    element that contains `point` followed by its ancestors up to the host, or empty when nothing
    can be confirmed; the record says how it ended, with counts and identifiers only, no names."""
    record = {"descended": True, "status": "unreadable", "depth": None, "visited": 0,
              "truncated": False, "ambiguous": False,
              "max_elements": DESCENT_MAX_ELEMENTS, "max_depth": DESCENT_MAX_DEPTH}
    host_rid = host.get("runtime_id", "")
    try:
        walk = read_walk(point[0], point[1], host_rid) if host_rid else None
    except (OSError, subprocess.SubprocessError):
        walk = None
    if walk is None:
        return [], record
    rows, truncated = walk
    if not rows or rows[0]["runtime_id"] != host_rid:
        record["status"] = "host_changed"
        return [], record
    below = rows[1:]
    record["visited"] = len(below)
    record["truncated"] = (bool(truncated) or len(below) > DESCENT_MAX_ELEMENTS
                           or any(r["depth"] > DESCENT_MAX_DEPTH for r in below))
    if record["truncated"]:
        record["status"] = "truncated"
        return [], record
    hits = [r for r in below if not r["offscreen"] and rect_contains(r["rect"], point)]
    if not hits:
        record["status"] = "not_found"
        return [], record
    deepest = max(r["depth"] for r in hits)
    top = [r for r in hits if r["depth"] == deepest]
    if len(top) > 1:
        record.update(status="ambiguous", ambiguous=True)
        return [], record
    chain, row = [], top[0]
    while True:
        chain.append({key: row[key] for key in
                      ("runtime_id", "automation_id", "control_type", "class_name", "offscreen")})
        if row["index"] == 0:
            break
        row = rows[row["parent"]]
    record.update(status="ok", depth=deepest, control_type=top[0]["control_type"],
                  automation_id=top[0]["automation_id"])
    return chain, record


def ground_chain(point: tuple[int, int], read_chain, read_walk) -> tuple[list[dict], dict]:
    """The chain that grounds a click at `point` and the record of the descent: the point
    query's chain as read, unless its first element is an island host (see descend)."""
    chain = read_chain(point[0], point[1])
    if not chain or not is_island_host(chain[0]):
        return chain, dict(NO_DESCENT)
    return descend(point, chain[0], read_walk)


def nfc_fold(text: str) -> str:
    return unicodedata.normalize("NFC", text).casefold()


def parse_calc_mode(output: str) -> tuple[str | None, int] | None:
    """(header name or None when there is no header, Scientific-only controls on screen) from
    the CALC_MODE_PS read; None when the output is not of that shape."""
    parts = output.strip("\r\n").split("\t")
    if len(parts) != 3 or parts[0] not in ("0", "1") or not parts[2].strip().isdigit():
        return None
    return (parts[1] if parts[0] == "1" else None), int(parts[2])


def calc_mode(header: str | None, scientific_controls: int) -> str:
    """The Calculator's mode from the two UI Automation signals: ``standard`` only when the
    header says so and no Scientific-only control is on screen; ``other`` when the header names
    another mode; ``unknown`` when the header is missing or the signals disagree."""
    if not header or not header.strip():
        return "unknown"
    if nfc_fold(header.strip()) in STANDARD_HEADERS:
        return "standard" if scientific_controls == 0 else "unknown"
    return "other"


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
    """S4R-02, the Windows 11 Calculator: 7 + 8 = 15, then clear. The buttons are selected
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
# The reduced protocol's own flow identifiers (docs/thesis/S4_ACEPTACION_REDUCIDA.md): the dossier's
# E2E-02 (editor) and E2E-03 (file explorer) are other flows. The records keep their `e2e` field
# (recorded evidence refers to it) and carry `flow_id` going forward.
FLOW_ID = {"testapp": "E2E-01", "calculator": "S4R-02", "notepad": "S4R-03"}

# ---- the protocol ---------------------------------------------------------------------------


def use_utf8_stdout() -> None:
    """Writes the log as UTF-8 whatever the console code page is: redirected to a file, stdout
    used the ANSI code page and a line held a non-UTF-8 byte (seventh round, finding C3)."""
    reconfigure = getattr(sys.stdout, "reconfigure", None)
    if reconfigure is not None:
        reconfigure(encoding="utf-8")


def log(event: str, **fields) -> None:
    print(json.dumps({"t": round(time.monotonic(), 3), "event": event, **fields},
                     ensure_ascii=False), flush=True)


ACTION_TIMINGS_KEY = "io.ommatix/action_timings"


def stage_ms(meta: dict) -> dict:
    """The per-stage timings of a response's `_meta`, in milliseconds. `observe_screen`'s
    (io.ommatix/timings, including `accessibility` when the stage ran, ADR-0017) keep their
    names; `execute_action`'s (io.ommatix/action_timings: gate, freshness, delivery, focus,
    verification, total) are recorded as `action_*`."""
    stages = {k.removesuffix("_us"): round(v / 1000, 3)
              for k, v in meta.get(s3.TIMINGS_KEY, {}).items()}
    for key, value in meta.get(ACTION_TIMINGS_KEY, {}).items():
        stages["action_" + key.removesuffix("_us")] = round(value / 1000, 3)
    return stages


def new_result(e2e: str, run: int, flow_id: str | None = None) -> dict:
    """A run's record, with every counter that `Protocol.steps` increments initialised."""
    return {"e2e": e2e, "flow_id": flow_id, "run": run, "success": False, "steps": 0,
            "actions_requested": 0, "delivered": 0, "typed_delivered": 0,
            "effect_observed": 0, "indeterminate": 0, "rejected": 0, "stale_retries": 0,
            "ocr_checks_failed": 0, "independent_checks": 0, "independent_failed": 0,
            "engine_false_success": 0, "grounding_wrong": 0, "observations": [], "actions": [],
            "failure": None}


def single_click_flow(label: str) -> dict:
    """A Calculator flow of one click on `label` with no end-state check: the steps of
    --calc-standard, each grounded by identity before and after its click."""
    return {"exe": "calculatorapp.exe", "initial": NOT_BY_OCR,
            "steps": [(label, NOT_BY_OCR, None)], "vocabulary": set(), "vocabulary_re": None}


def read_ledger(path: Path) -> int:
    """The live request count the ledger file holds (0 when there is no file)."""
    try:
        return int(path.read_text(encoding="utf-8").strip() or 0)
    except FileNotFoundError:
        return 0


def round_problem(live: bool, round_start: int | None, ledger_now: int) -> str | None:
    """Why a round may not start: run mode needs the ledger's value at the round's start, and
    that value must lie within the ledger and leave room under the round's cap."""
    if round_start is None:
        return ("run mode needs --round-start (the ledger's value at the round's start)"
                if live else None)
    if not 0 <= round_start <= ledger_now:
        return "--round-start is outside the ledger (0 to its current value)"
    if ledger_now - round_start >= LIVE_CAP:
        return "the round's live request cap is already spent"
    return None


class Protocol:
    def __init__(self, args, server, stop_file: Path | None, flow: dict, ledger: Path):
        self.args = args
        self.srv = server
        self.stop_file = stop_file
        self.flow = flow
        self.ledger = ledger
        self.round_start = args.round_start  # the ledger's value when the round began
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
        return read_ledger(self.ledger)

    def round_live(self) -> int:
        """The live requests of the current round: the ledger only accumulates, the cap counts
        what was spent since the round began."""
        return self.ledger_total() - self.round_start

    def spend_live(self) -> None:
        total = self.ledger_total()
        if self.live_sent >= self.args.max_live or total - self.round_start >= LIVE_CAP:
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
        res = new_result(E2E_ID[self.args.app], n, FLOW_ID[self.args.app])
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

    def identify(self, hwnd, label: str, control_types: tuple[str, ...]) -> list[dict] | None:
        return uia_identify(hwnd, label, control_types)

    # The descent record of the latest cursor_chain (or diag_at_element) read: None before any.
    last_descent: dict | None = None
    # While set, a click is sent only if the identity lookup finds this very element (the
    # Standard item that was polled); any other RuntimeId under the same name means no click.
    pinned_runtime_id: str | None = None

    def mono(self) -> float:
        return time.monotonic()

    def pause(self, seconds: float) -> None:
        time.sleep(seconds)

    def standard_item_rows(self, hwnd) -> list[dict] | None:
        return uia_standard_items(hwnd)

    def cursor_point(self) -> tuple[int, int]:
        return cursor()

    def point_chain(self, x: int, y: int) -> list[dict]:
        return uia_chain_at(x, y)

    def island_walk(self, x: int, y: int, host_runtime_id: str):
        return uia_island_walk(x, y, host_runtime_id)

    def element_centre(self, hwnd, label: str, control_types: tuple[str, ...]):
        return uia_element_centre(hwnd, label, control_types)

    def cursor_chain(self) -> list[dict]:
        """The chain that grounds the click just delivered: the control under the cursor (where
        SendInput left it) and its ancestors; inside a XAML island, the deepest on-screen
        element under the cursor and its ancestors up to the host (ground_chain). How the
        descent ended is left in `last_descent`."""
        self.last_descent = None
        chain, self.last_descent = ground_chain(self.cursor_point(), self.point_chain,
                                                self.island_walk)
        return chain

    @staticmethod
    def descent_record(descent: dict | None) -> dict:
        return dict(descent) if descent else dict(NO_DESCENT)

    def calc_mode(self, hwnd) -> str:
        return read_calc_mode(hwnd)

    def current_hwnd(self):
        return foreground()["hwnd"]

    def identity_lookup(self, hwnd, label: str, target: dict) -> tuple[dict, dict | None]:
        """The identity of the element a click is about to reach (see identity_decision), found
        through UI Automation in the window `hwnd`: its record (status, candidate counts and,
        when exactly one, its RuntimeId, AutomationId and control type) and the element, or
        None when the click must not be sent. Shared by every click of the client."""
        control_types = ROLE_CONTROL_TYPES.get(target.get("role", ""))
        rows = self.identify(hwnd, label, control_types) if control_types else None
        status, found = identity_decision(rows)
        if (found is not None and self.pinned_runtime_id
                and found["runtime_id"] != self.pinned_runtime_id):
            status, found = "not_the_polled_item", None
        if not control_types:
            status = "no_control_type"  # a role with no UI Automation control is never clicked
        record = {"status": status,
                  "candidates": sum(1 for r in rows or [] if not r["offscreen"]),
                  "offscreen_ignored": sum(1 for r in rows or [] if r["offscreen"])}
        if found is not None:
            record.update(runtime_id=found["runtime_id"], automation_id=found["automation_id"],
                          control_type=found["control_type"])
        return record, found

    def identify_before(self, res: dict, fg: dict, label: str, target: dict) -> dict | None:
        """The identity lookup of a step's click, recorded in the run. Returns None, with the
        run failed, when the click must not be sent."""
        record, found = self.identity_lookup(fg["hwnd"], label, target)
        status = record["status"]
        record = {"step": res["steps"], **record}
        res.setdefault("identity", []).append(record)
        if found is None:
            res["failure"] = f"step {res['steps']}: no click, the element's identity is {status}"
            return None
        return found

    def diag_at_element(self, hwnd, label: str, control_types: tuple[str, ...]):
        """The diagnostic chain under the target's centre. When it starts at an island host the
        descent is read too and left in `last_descent`; it decides nothing."""
        rows = uia_diag_at_element(hwnd, label, control_types)
        if rows and is_island_host(rows[0]):
            centre = self.element_centre(hwnd, label, control_types)
            if centre is not None:
                _, self.last_descent = descend(centre, rows[0], self.island_walk)
        return rows

    def diag_at_cursor(self):
        return uia_diag_at_cursor()

    @staticmethod
    def read_diag(read, expected_runtime_id: str) -> list[dict] | None:
        """One diagnostic chain in its recorded form, or None when it could not be read. A
        diagnostic never decides anything and never stops a run, so any failure of the
        read is only recorded as unreadable."""
        try:
            rows = read()
        except Exception:  # read-only diagnostics must not fail a run
            return None
        return None if rows is None else diag_record(rows, expected_runtime_id)

    def uia_candidates(self, hwnd, names: tuple[str, ...]) -> dict | None:
        """How many elements of the window UI Automation sees under the step's names, of any
        control type, and which types (never the names). None when it could not be read."""
        on_screen, off_screen, types = 0, 0, set()
        for name in names:
            try:
                rows = self.identify(hwnd, name, ANY_CONTROL_TYPE)
            except Exception:  # a diagnosis must not mask the stop it explains
                return None
            if rows is None:
                return None
            for row in rows:
                if row["offscreen"]:
                    off_screen += 1
                else:
                    on_screen += 1
                    types.add(row["control_type"])
        return {"candidates": on_screen, "offscreen": off_screen, "control_types": sorted(types)}

    def record_target(self, res: dict, elements: list[dict], spec, hwnd,
                      stopped: bool = False) -> None:
        """What the observation holds under a step's label(s), without any name (S4 fifth round,
        finding 3): how many candidates, how many are confirmed, their roles, actionability and
        label sources, and, when the step stopped here, the condition that stopped it:
        ``zero_candidates``, ``several_candidates`` or ``not_confirmed`` (one candidate that is
        not confirmed). A stop also records what UI Automation sees under the same names."""
        names = candidates(spec)
        found = [e for e in elements if e.get("label", "").strip() in names]

        def counts(key: str) -> dict:
            return dict(sorted(Counter(e.get(key, "") for e in found).items()))

        record = {"step": res["steps"], "candidates": len(found),
                  "confirmed": sum(1 for e in found if e.get("actionability") == "confirmed"),
                  "roles": counts("role"), "actionability": counts("actionability"),
                  "label_source": counts("label_source"), "stopped_by": None}
        if stopped:
            record["stopped_by"] = ("zero_candidates" if not found else
                                    "several_candidates" if len(found) > 1 else "not_confirmed")
            record["uia"] = self.uia_candidates(hwnd, names)
            log("target_stop", run=res.get("run"), **record)
        res.setdefault("targets", []).append(record)

    def soft_foreground_ok(self) -> bool:
        """Whether the target application is in front and maximized on the primary monitor.
        Unlike require_foreground it never engages the operator's stop: a cleanup that cannot
        proceed just does not click."""
        fg = foreground()
        return fg["exe"] == self.flow["exe"] and fg["maximized_primary"]

    def await_standard_item(self, hwnd, summary: dict) -> dict | None:
        """Polls UI Automation (read-only) for the pane's Standard item after the navigation
        click: the one on-screen element with AutomationId CALC_STANDARD_AUTOMATION_ID of a
        control type in STANDARD_ITEM_TYPES, for at most STANDARD_POLL_WINDOW_S in steps of
        STANDARD_POLL_STEP_S (each read costs a PowerShell start, so a step is a lower bound).
        Returns the item when its name is one of CALC_STANDARD_NAMES; otherwise None, with the
        reason in ``summary["standard_item"]["stopped_by"]``: ``timeout`` (nothing, or nothing
        readable, in the window), ``several_candidates``, ``unknown_name`` or ``unidentifiable``.
        The three last stop at once: waiting does not make them better. A stop also records
        ``by_name``: how many elements UI Automation sees under the accepted names, of any
        control type and AutomationId (counts and types, never names), which tells an
        AutomationId that is not ``Standard`` from a pane that never showed the item."""
        started = self.mono()
        polls = 0
        while True:
            rows = self.standard_item_rows(hwnd)
            polls += 1
            status, item = standard_item_decision(rows)
            waited = self.mono() - started
            if status in ("ok", "ambiguous", "unknown_name", "unidentifiable"):
                break
            if waited >= STANDARD_POLL_WINDOW_S:
                break
            self.pause(STANDARD_POLL_STEP_S)
        record = standard_item_record(status, rows, polls, waited)
        record["stopped_by"] = (None if item is not None else
                                "several_candidates" if status == "ambiguous" else
                                status if status in ("unknown_name", "unidentifiable") else
                                "timeout")
        if item is None:  # what UI Automation sees under the accepted names, by any AutomationId
            record["by_name"] = self.uia_candidates(hwnd, CALC_STANDARD_NAMES)
        summary["standard_item"] = record
        return item

    def calc_click(self, res: dict, summary: dict, kind: str, label: str, hwnd, pid) -> None:
        """One grounded click of --calc-standard on `label` (a new observation first), recorded
        under `kind`. A failed or ungrounded click ends the step."""
        snap, elements, obs = self.observe()
        res["observations"].append(obs)
        self.flow = single_click_flow(label)
        before = res["actions_requested"]
        self.steps(res, snap, elements, hwnd, pid)
        summary["clicks"].append({
            "label_is": kind,
            "requested": res["actions_requested"] - before,
            "status": (res["actions"] or [{}])[-1].get("status"),
            "input_delivered": (res["actions"] or [{}])[-1].get("input_delivered"),
            "grounded": (res.get("grounding") or [None])[-1]})
        if res["failure"] is not None or res["grounding_wrong"]:
            raise Abort(f"calc-standard stops, no further click: {res['failure']}")

    def calc_standard(self) -> dict:
        """Switches the Calculator to Standard mode (--calc-standard), once, before the S4R-02
        series. Two clicks through the engine's gate, each with identity grounding: the
        navigation toggle, selected by its exact label, and then the pane's Standard item. The
        item is first polled in UI Automation by its AutomationId (await_standard_item), its
        name is read from a closed set (``Estándar``, ``Calculadora estándar``), the engine's
        element is selected by that exact name after a new observation (unique and confirmed)
        and the click must reach that very element (``pinned_runtime_id``). The mode is read
        through UI Automation before (no click when it is Standard already, or cannot be read)
        and after, also when the step stops. Nothing else is clicked: a pane that does not
        show the item, an item that is not unique, a name outside the set, an engine element
        that is not unique and confirmed, an identity that is not exactly one or a click that is
        not grounded ends the step. Only the Calculator this step started is closed."""
        res = new_result("MODE", 0, FLOW_ID["calculator"])
        summary = {"step": "calc_standard", "flow_id": FLOW_ID["calculator"],
                   "round_start": self.round_start, "ledger_start": self.ledger_total(),
                   "mode_before": None, "nav_label": None, "standard_item": None,
                   "foreground_wait_s": None, "activation_requests": None,
                   "window_placements": None, "clicks": [], "mode_after": None,
                   "switched": False, "failure": None}
        pid = None
        hwnd = None
        try:
            waiting = time.perf_counter()
            pid = self.launch()
            ok, requests = self.wait_foreground(pid)
            summary.update(foreground_wait_s=round(time.perf_counter() - waiting, 2),
                           activation_requests=requests, window_placements=self.placements)
            if not ok:
                raise Abort("the Calculator did not reach the foreground maximized")
            hwnd = self.current_hwnd()
            self.app_hwnd = hwnd
            time.sleep(self.args.settle)
            summary["mode_before"] = self.calc_mode(hwnd)
            if summary["mode_before"] == "standard":
                summary["switched"] = True  # nothing to do, and nothing clicked
                return summary
            if summary["mode_before"] != "other":
                raise Abort("the Calculator's mode cannot be read through UI Automation: no click")
            summary["nav_label"] = CALC_NAV_LABEL
            self.calc_click(res, summary, "navigation", CALC_NAV_LABEL, hwnd, pid)
            item = self.await_standard_item(hwnd, summary)
            if item is None:
                raise Abort("calc-standard stops, no click on the Standard item: "
                            f"{summary['standard_item']['stopped_by']}")
            self.pinned_runtime_id = item["runtime_id"]
            try:
                self.calc_click(res, summary, "standard", item["name"], hwnd, pid)
            finally:
                self.pinned_runtime_id = None
            for _ in range(8):
                summary["mode_after"] = self.calc_mode(hwnd)
                if summary["mode_after"] == "standard":
                    break
                time.sleep(0.5)
            summary["switched"] = summary["mode_after"] == "standard"
            if not summary["switched"]:
                summary["failure"] = "the Calculator is not in Standard mode after the clicks"
        except Abort as error:
            summary["failure"] = str(error)
        finally:
            if summary["mode_after"] is None:  # a stop records the mode too (read-only)
                summary["mode_after"] = self.mode_now(hwnd)
            if summary["clicks"]:
                time.sleep(self.args.settle)  # let the Calculator save its mode before it closes
            self.close(pid)  # only a process this step started
            summary["identity"] = res.get("identity", [])
            summary["targets"] = res.get("targets", [])
            summary["live_requests"] = res["actions_requested"]
            summary["ledger_end"] = self.ledger_total()
            log("calc_standard", **summary)
            with open(self.args.results, "a", encoding="utf-8") as fh:
                fh.write(json.dumps(summary, ensure_ascii=False) + "\n")
        return summary

    def mode_now(self, hwnd) -> str:
        """The Calculator's mode when the step ends, or ``unknown`` when there is no window to
        read or the read fails. A read-only query: it never stops the step's own record."""
        if hwnd is None:
            return "unknown"
        try:
            return self.calc_mode(hwnd)
        except (OSError, subprocess.SubprocessError):
            return "unknown"

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
        # The same identity grounding as the clicks of steps(), before the click: not exactly
        # one `Editar` in the foreground window means no click at all.
        record, ident = self.identity_lookup(self.current_hwnd(), NOTEPAD_MENU, target)
        restore["identity"] = record
        if ident is None:
            log("menu_restore_skipped", run=res["run"],
                reason=f"the Editar identity is {record['status']}")
            return False
        out = self.restore_click(res, target, snap)
        restore["clicked"] = bool(out.get("input_delivered"))
        restore["status"] = out.get("status")
        if out.get("input_delivered"):
            # ... and after it: the control under the cursor must be that element. A mismatch is
            # recorded and counted as in steps(); nothing more is sent and nothing is retried.
            grounded = identity_hit(ident, self.cursor_chain())
            restore["grounded"] = grounded
            if self.last_descent and self.last_descent["descended"]:
                restore["descent"] = dict(self.last_descent)
            if not grounded:
                res["grounding_wrong"] = res.get("grounding_wrong", 0) + 1
                if out.get("status") == "effect_observed":
                    res["engine_false_success"] = res.get("engine_false_success", 0) + 1
                log("menu_restore_ungrounded", run=res["run"])
        restore["state_polls"] = 0
        for _ in range(8):
            restore["state_after"] = self.menu_state(hwnd)
            restore["state_polls"] += 1
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
                try:
                    label, target = self.find_target(elements, spec)
                except Abort:  # several elements share the label: say what was there, then stop
                    self.record_target(res, elements, spec, hwnd, stopped=True)
                    raise
                if target is not None:
                    break
                time.sleep(0.3)
                snap, elements, obs = self.observe()
                res["observations"].append(obs)
            self.record_target(res, elements, spec, hwnd, stopped=target is None)
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
            ident = None
            for attempt in range(4):  # again only after STALE_SNAPSHOT (nothing delivered)
                fg = self.require_foreground()
                # The element to click, found by UI Automation before any input and before the
                # live request is spent: not exactly one match means no click at all.
                ident = self.identify_before(res, fg, label, target)
                if ident is None:
                    return
                diagnostic = None
                if self.args.app == "notepad":
                    # Read-only (finding 1): what is under the target point before the click.
                    types = ROLE_CONTROL_TYPES[target["role"]]
                    self.last_descent = None
                    diagnostic = {"step": res["steps"], "before": self.read_diag(
                        lambda: self.diag_at_element(fg["hwnd"], label, types),
                        ident["runtime_id"])}
                    diagnostic["before_descent"] = self.descent_record(self.last_descent)
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
                clicked_at = time.monotonic()
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
            # Independent grounding: the control under the cursor is the element found before
            # the click, by identity (RuntimeId, or AutomationId), whatever its name is now.
            if out.get("input_delivered"):
                chain = self.cursor_chain()
                descent = self.last_descent
                hit = identity_hit(ident, chain)
                if descent and descent["descended"]:
                    res.setdefault("descent", []).append({"step": res["steps"], **descent})
                elif not chain:
                    res["grounding_chain_empty"] = res.get("grounding_chain_empty", 0) + 1
                res.setdefault("grounding", []).append(hit)
                if not hit:
                    res["grounding_wrong"] += 1
                if self.args.debug_grounding:
                    log("grounding_chain", step=res["steps"], hit=hit,
                        chain=[(row["automation_id"], row["control_type"]) for row in chain])
                if diagnostic is not None:  # read-only: it decides nothing and is not scored
                    diagnostic["after"] = self.read_diag(self.diag_at_cursor, ident["runtime_id"])
                    diagnostic["after_descent"] = self.descent_record(descent)
                    # How long after the engine's reply the chain was read: a menu may still be
                    # animating, so the reader of the record needs the delay.
                    diagnostic["after_ms"] = round((time.monotonic() - clicked_at) * 1000)
                    res.setdefault("menu_diagnostic", []).append(diagnostic)
                    log("menu_diagnostic", run=res["run"], **diagnostic)
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
            if grounded is False:
                # The click may have reached another control: no further input in this run.
                res["failure"] = (f"step {res['steps']}: the control under the cursor is not the "
                                  "element found before the click")
                return
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
            ledger_start = self.ledger_total()
            res = self.run_once(n)
            res["ledger_start"], res["ledger_end"] = ledger_start, self.ledger_total()
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
                   help="file holding the live request count; it only accumulates")
    p.add_argument("--round-start", type=int, default=None,
                   help="the ledger's value when the round began (required in run mode): the "
                        "round may spend at most LIVE_CAP (400) requests beyond it")
    p.add_argument("--calc-standard", action="store_true",
                   help="switch the Calculator to Standard mode (two gated clicks with identity "
                        "grounding, verified through UI Automation) and exit; run once before "
                        "the S4R-02 series (--app calculator, run mode)")
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
    use_utf8_stdout()

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
    if args.calc_standard and (args.app != "calculator" or not live):
        log("abort", reason="--calc-standard needs --app calculator and run mode")
        return 2
    if args.calc_standard:
        args.max_live = min(args.max_live, CALC_STANDARD_MAX_LIVE)
    ledger_now = read_ledger(args.ledger)
    problem = round_problem(live, args.round_start, ledger_now)
    if problem:
        log("abort", reason=problem)
        return 2
    if args.round_start is None:
        args.round_start = ledger_now
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
        config_sha256=hashlib.sha256(raw).hexdigest(), max_live=args.max_live,
        round_start=args.round_start, ledger_start=ledger_now, round_cap=LIVE_CAP)
    srv = s3.Server(argv)
    proto = Protocol(args, srv, stop_file, flow, args.ledger)
    code = 0
    try:
        m = proto.metrics()
        log("metrics", mode=m.get("mode"), models_loaded=m.get("models_loaded"))
        if args.calc_standard:
            code = 0 if proto.calc_standard()["switched"] else 1
        else:
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
            live_requests=proto.live_sent, ledger_start=ledger_now,
            ledger_end=proto.ledger_total(), round_start=args.round_start,
            round_live_requests=proto.round_live(), ledger_total=proto.ledger_total())
    return code


if __name__ == "__main__":
    sys.exit(main())
