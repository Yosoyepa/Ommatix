#!/usr/bin/env python3
"""AG-03 Win32 bindings behind an interface (ADR-0038 decisions 4 and 5, option O4).

**None of the code in this module that calls Win32 is executed by the repository's tests or by the
work that wrote it.** Importing the module calls no Win32 function and loads no DLL: the classes
``RealOs``, ``RealSender`` and ``RealGdi`` load ``user32``/``kernel32``/``gdi32`` lazily, inside
their methods, and only ``pixel_server`` in real mode (Step 6, the owner present) constructs them.
Everything a test may exercise is pure:

* ``build_click_inputs`` and ``build_text_inputs`` build the event lists of the two batches (one
  ``SendInput`` call each; AGENTS.md section 6: the ``SendInput`` actuator is tested through its pure
  helpers only);
* ``normalise_absolute`` maps a screen point to the 0..65535 range of ``MOUSEEVENTF_ABSOLUTE``;
* the ``OsReader``/``Sender``/``Gdi`` shapes the server is written against (``pixel_server`` takes
  them as arguments; ``fake_os`` implements them for the tests).

Review before any live run (Step 5): the structure layouts below (``INPUT`` is 40 bytes on x64),
the elevation read, the ``PrintWindow`` flags and the DPI call.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

from pixel_guards import Evidence, Focus, Window, is_helper_class  # noqa: E402
from proxy import PRINTABLE_ASCII  # noqa: E402

MAX_TEXT_LEN = 16

# SendInput / INPUT constants (winuser.h).
INPUT_MOUSE, INPUT_KEYBOARD = 0, 1
MOUSEEVENTF_MOVE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP = 0x0001, 0x0002, 0x0004
MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_ABSOLUTE = 0x4000, 0x8000
KEYEVENTF_KEYUP, KEYEVENTF_UNICODE = 0x0002, 0x0004
# Virtual keys whose async state means "a modifier is held".
MODIFIER_VKS = (0x10, 0x11, 0x12, 0x5B, 0x5C)  # shift, control, alt, left and right Windows
PW_CLIENTONLY, PW_RENDERFULLCONTENT = 0x1, 0x2
GA_ROOT = 2


class InputRefused(ValueError):
    """The text or the point of a batch is outside what the experiment allows (closed message)."""


def normalise_absolute(x: int, y: int, virtual_screen: tuple[int, int, int, int]) -> tuple[int, int]:
    """Screen point -> the 0..65535 coordinates of an absolute, virtual-desktop mouse event.
    ``virtual_screen`` is (left, top, width, height) of the virtual desktop."""
    left, top, width, height = virtual_screen
    if width < 2 or height < 2:
        raise InputRefused("virtual screen too small")
    if not (left <= x < left + width and top <= y < top + height):
        raise InputRefused("point outside the virtual screen")
    return (round((x - left) * 65535 / (width - 1)), round((y - top) * 65535 / (height - 1)))


def build_click_inputs(x: int, y: int, virtual_screen: tuple[int, int, int, int]) -> list[dict]:
    """One left click at screen point (x, y) as the events of ONE ``SendInput`` call: move, press,
    release. No double click, right click, drag or wheel exists in this module."""
    dx, dy = normalise_absolute(x, y, virtual_screen)
    move = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK
    return [{"kind": "mouse", "dx": dx, "dy": dy, "flags": move},
            {"kind": "mouse", "dx": 0, "dy": 0, "flags": MOUSEEVENTF_LEFTDOWN},
            {"kind": "mouse", "dx": 0, "dy": 0, "flags": MOUSEEVENTF_LEFTUP}]


def text_problem(text) -> str | None:
    """Why ``text`` may not be typed: 1 to 16 printable ASCII characters, as the AG-02 proxy limits
    ``type``. No Enter, Tab, shortcut or special key can be expressed (control characters are not
    printable ASCII)."""
    if not isinstance(text, str) or not 1 <= len(text) <= MAX_TEXT_LEN:
        return "length"
    if not set(text) <= PRINTABLE_ASCII:
        return "characters"
    return None


def build_text_inputs(text: str) -> list[dict]:
    """The key events of ``text`` as ONE ``SendInput`` batch: for every character a Unicode key
    press and release (2 * len events, at most 32)."""
    if (problem := text_problem(text)) is not None:
        raise InputRefused("text " + problem)
    events: list[dict] = []
    for char in text:
        events.append({"kind": "key", "vk": 0, "scan": ord(char), "flags": KEYEVENTF_UNICODE})
        events.append({"kind": "key", "vk": 0, "scan": ord(char), "flags": KEYEVENTF_UNICODE | KEYEVENTF_KEYUP})
    return events


# ---- pure decoders of Win32 results: a failed read is unknown, never affirmative evidence ----------------

HGDI_ERROR = 0xFFFFFFFF  # SelectObject's failure value for a region; 0 / NULL for a bitmap
DPI_V2 = "per_monitor_v2"


def read_stop(path, stat=None) -> bool | None:
    """The operator's stop file: True if present, False only if genuinely absent, None when it cannot be
    told (a permission error, an I/O error, anything unexpected). ``os.path.exists`` is not used: it turns
    every failure into False. Read only: the file is never written, removed or cleared by this module."""
    try:
        (stat or os.stat)(path)
    except (FileNotFoundError, NotADirectoryError):
        return False
    except Exception:  # noqa: BLE001 - unknown is a refusal
        return None
    return True


def decode_sendinput(inserted, expected: int) -> str:
    """``all``, ``partial`` or ``none`` from SendInput's count of inserted events. Only ``all`` is a clean
    delivery; ``partial`` and ``none`` are an uncertain delivery that is never retried."""
    if inserted == expected:
        return "all"
    return "none" if inserted == 0 else "partial"


def decode_class_name(copied, value) -> str | None:
    """GetClassNameW returns the number of characters copied; zero (or negative) is a failure."""
    return value if isinstance(copied, int) and copied > 0 and isinstance(value, str) and value else None


def decode_ancestor(handle) -> int | None:
    """GetAncestor returns NULL on failure: unknown, never the handle it was asked about."""
    return int(handle) if handle else None


def decode_select_object(result) -> bool:
    """SelectObject returns NULL (or HGDI_ERROR) when it fails to select the bitmap."""
    return bool(result) and result != HGDI_ERROR


def decode_dpi_awareness(equals_v2, awareness) -> str:
    """The effective DPI context as a word, from ``AreDpiAwarenessContextsEqual(ctx, per-monitor-v2)`` and
    ``GetAwarenessFromDpiAwarenessContext(ctx)`` (0 unaware, 1 system, 2 per-monitor; anything else, or a
    missing reading, is unknown)."""
    if equals_v2 is True:
        return DPI_V2
    return {2: "per_monitor", 1: "system", 0: "unaware"}.get(awareness, "unknown") if equals_v2 is False else "unknown"


def dpi_decision(effective) -> bool:
    """Only the explicitly compatible context (per-monitor v2) may proceed. ``SetProcessDpiAwarenessContext``
    failing with access denied only says that some default was set earlier: it proves nothing."""
    return effective == DPI_V2


def decode_display(rect, dpi) -> dict | None:
    """The primary monitor's resolution and numeric scaling (percent) from its rectangle and its effective DPI
    (96 is 100 %). Anything not positive or not a number is unknown (None): the display is then not measured."""
    if (not isinstance(rect, (tuple, list)) or len(rect) != 4 or not all(isinstance(v, int) for v in rect)
            or not isinstance(dpi, int) or isinstance(dpi, bool) or dpi <= 0):
        return None
    width, height = rect[2] - rect[0], rect[3] - rect[1]
    if width <= 0 or height <= 0:
        return None
    return {"resolution": f"{width}x{height}", "scaling": round(dpi * 100 / 96)}


def measure_primary_display(api) -> dict | None:
    """What the series' display really is: the PRIMARY monitor's size and scaling, read through ``api``
    (``ensure_dpi_aware``, ``primary_monitor_rect``, ``primary_effective_dpi``) - not the virtual desktop, whose
    size changes with a secondary monitor. The DPI is read only after ``ensure_dpi_aware`` has VERIFIED, in this
    very process, the per-monitor v2 context the read needs (an unaware caller is handed 96 by the API whatever the
    real scaling); without it the display is unknown. The verified context is recorded beside the numbers."""
    try:
        if not api.ensure_dpi_aware():
            return None
        measured = decode_display(api.primary_monitor_rect(), api.primary_effective_dpi())
        return None if measured is None else measured | {"dpi_context": DPI_V2}
    except Exception:  # noqa: BLE001 - an unreadable display is an unknown display
        return None


# ---- the shapes the server is written against (fakes implement them; nothing here runs in tests) ----

class OsReader:
    """Reads the operating system. Every method is read only."""

    def stop_present(self) -> bool | None:  # pragma: no cover - interface
        raise NotImplementedError

    def target_window(self, pid: int) -> Window | None:  # pragma: no cover - interface
        """The main window of process ``pid`` (not a helper window), or None."""
        raise NotImplementedError

    def evidence(self, pid: int, point: tuple[int, int] | None, want_focus: bool) -> Evidence:  # pragma: no cover
        raise NotImplementedError

    def virtual_screen(self) -> tuple[int, int, int, int]:  # pragma: no cover - interface
        raise NotImplementedError

    def ensure_dpi_aware(self) -> bool:  # pragma: no cover - interface
        raise NotImplementedError


class Sender:
    """Sends one batch of events with ONE operating-system call. ``prepare`` does all the native work (loading
    the DLL, building the event array) so that it is inside the delivery recheck's boundary; ``send`` then does
    nothing but the call and returns how many events the operating system inserted."""

    def prepare(self, inputs: list[dict]):  # pragma: no cover - interface
        raise NotImplementedError

    def send(self, prepared) -> int:  # pragma: no cover - interface
        raise NotImplementedError


class Gdi:
    """The ``PrintWindow`` capture: the window renders itself into an app-owned bitmap."""

    def print_window(self, hwnd: int, width: int, height: int) -> bytes | None:  # pragma: no cover - interface
        """Top-down 32-bit BGRA rows of the client area, ``width * height * 4`` bytes, or None."""
        raise NotImplementedError


# ---- the real implementations: written, reviewed, never executed by this change ----------------------

def _ct():
    """``ctypes`` and ``ctypes.wintypes``, imported on first use: importing this module loads neither."""
    import ctypes
    from ctypes import wintypes
    return ctypes, wintypes


def _structures():
    """The ctypes structures, built on first use (no Win32 call; importing the module builds none)."""
    ctypes, wintypes = _ct()
    # Fixed-width integers, not ``wintypes.DWORD``/``LONG``: on Windows they are the same 32-bit types, and the
    # layout is then identical (and testable) on every platform - ``wintypes.LONG`` is 64-bit on Linux.
    class MOUSEINPUT(ctypes.Structure):
        _fields_ = [("dx", ctypes.c_int32), ("dy", ctypes.c_int32), ("mouseData", ctypes.c_uint32),
                    ("dwFlags", ctypes.c_uint32), ("time", ctypes.c_uint32),
                    ("dwExtraInfo", ctypes.c_size_t)]

    class KEYBDINPUT(ctypes.Structure):
        _fields_ = [("wVk", ctypes.c_uint16), ("wScan", ctypes.c_uint16), ("dwFlags", ctypes.c_uint32),
                    ("time", ctypes.c_uint32), ("dwExtraInfo", ctypes.c_size_t)]

    class _Union(ctypes.Union):
        _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]

    class INPUT(ctypes.Structure):
        _fields_ = [("type", ctypes.c_uint32), ("u", _Union)]

    class GUITHREADINFO(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("flags", wintypes.DWORD), ("hwndActive", wintypes.HWND),
                    ("hwndFocus", wintypes.HWND), ("hwndCapture", wintypes.HWND),
                    ("hwndMenuOwner", wintypes.HWND), ("hwndMoveSize", wintypes.HWND),
                    ("hwndCaret", wintypes.HWND), ("rcCaret", wintypes.RECT)]

    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [("biSize", ctypes.c_uint32), ("biWidth", ctypes.c_int32), ("biHeight", ctypes.c_int32),
                    ("biPlanes", ctypes.c_uint16), ("biBitCount", ctypes.c_uint16),
                    ("biCompression", ctypes.c_uint32), ("biSizeImage", ctypes.c_uint32),
                    ("biXPelsPerMeter", ctypes.c_int32), ("biYPelsPerMeter", ctypes.c_int32),
                    ("biClrUsed", ctypes.c_uint32), ("biClrImportant", ctypes.c_uint32)]

    class TOKEN_ELEVATION(ctypes.Structure):
        _fields_ = [("TokenIsElevated", ctypes.c_uint32)]

    return MOUSEINPUT, KEYBDINPUT, INPUT, GUITHREADINFO, BITMAPINFOHEADER, TOKEN_ELEVATION


def _dlls():
    ctypes, wintypes = _ct()
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
    for fn, restype in ((user32.GetForegroundWindow, wintypes.HWND), (user32.WindowFromPoint, wintypes.HWND),
                        (user32.GetAncestor, wintypes.HWND), (kernel32.OpenProcess, wintypes.HANDLE),
                        (gdi32.CreateCompatibleDC, wintypes.HDC), (gdi32.SelectObject, wintypes.HGDIOBJ)):
        fn.restype = restype
    user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
    user32.WindowFromPoint.argtypes = [wintypes.POINT]
    user32.PrintWindow.argtypes = [wintypes.HWND, wintypes.HDC, wintypes.UINT]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.MonitorFromWindow.argtypes = [wintypes.HWND, wintypes.DWORD]
    user32.MonitorFromWindow.restype = wintypes.HANDLE
    user32.IsIconic.argtypes = [wintypes.HWND]
    user32.IsWindowVisible.argtypes = [wintypes.HWND]
    user32.GetAsyncKeyState.argtypes = [ctypes.c_int]
    user32.GetGUIThreadInfo.argtypes = [wintypes.DWORD, ctypes.c_void_p]
    user32.GetMonitorInfoW.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
    user32.GetSystemMetrics.argtypes = [ctypes.c_int]
    # Handles are pointer-sized: without argtypes ctypes would pass them as 32-bit ints.
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                                                    ctypes.POINTER(wintypes.DWORD)]
    advapi32.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    advapi32.GetTokenInformation.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD,
                                             ctypes.POINTER(wintypes.DWORD)]
    gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
    gdi32.CreateDIBSection.argtypes = [wintypes.HDC, ctypes.c_void_p, wintypes.UINT, ctypes.POINTER(ctypes.c_void_p),
                                       wintypes.HANDLE, wintypes.DWORD]
    gdi32.CreateDIBSection.restype = wintypes.HANDLE
    gdi32.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
    gdi32.DeleteObject.argtypes = [wintypes.HGDIOBJ]
    gdi32.DeleteDC.argtypes = [wintypes.HDC]
    return user32, kernel32, advapi32, gdi32


class RealOs(OsReader):  # pragma: no cover - the Win32 parts are never executed by the tests
    def __init__(self, stop_path, stat=None):
        self.stop_path, self._stat = stop_path, stat

    def stop_present(self):
        return read_stop(self.stop_path, self._stat)

    def ensure_dpi_aware(self):
        """Asks for per-monitor v2 and then READS the context the rectangle reads and the point mapping run
        under; only that reading decides (``dpi_decision``), never the call's result or its error code."""
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        user32.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
        user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
        # The process default may already have been set by something else (access denied): ask for this thread too -
        # a per-thread context needs no process-wide permission - and let the READ below decide.
        user32.SetThreadDpiAwarenessContext.argtypes = [ctypes.c_void_p]
        user32.SetThreadDpiAwarenessContext.restype = ctypes.c_void_p
        user32.SetThreadDpiAwarenessContext(ctypes.c_void_p(-4))
        user32.GetThreadDpiAwarenessContext.restype = ctypes.c_void_p
        user32.AreDpiAwarenessContextsEqual.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        user32.GetAwarenessFromDpiAwarenessContext.argtypes = [ctypes.c_void_p]
        context = user32.GetThreadDpiAwarenessContext()
        if not context:
            return False
        equals = bool(user32.AreDpiAwarenessContextsEqual(context, ctypes.c_void_p(-4)))
        awareness = int(user32.GetAwarenessFromDpiAwarenessContext(context))
        return dpi_decision(decode_dpi_awareness(equals, awareness))

    def primary_monitor_rect(self):
        """rcMonitor of the primary monitor (physical pixels under the verified per-monitor v2 context)."""
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        user32.MonitorFromPoint.argtypes = [wintypes.POINT, wintypes.DWORD]
        user32.MonitorFromPoint.restype = wintypes.HANDLE
        monitor = user32.MonitorFromPoint(wintypes.POINT(0, 0), 1)  # MONITOR_DEFAULTTOPRIMARY
        info = (ctypes.c_byte * 40)()
        ctypes.cast(info, ctypes.POINTER(wintypes.DWORD))[0] = 40
        if not monitor or not user32.GetMonitorInfoW(monitor, info):
            return None
        values = ctypes.cast(info, ctypes.POINTER(wintypes.LONG))
        return (int(values[1]), int(values[2]), int(values[3]), int(values[4]))

    def primary_effective_dpi(self):
        """The effective DPI of the primary monitor (``GetDpiForMonitor``, MDT_EFFECTIVE_DPI); None if unreadable."""
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        shcore = ctypes.WinDLL("shcore")
        user32.MonitorFromPoint.argtypes = [wintypes.POINT, wintypes.DWORD]
        user32.MonitorFromPoint.restype = wintypes.HANDLE
        monitor = user32.MonitorFromPoint(wintypes.POINT(0, 0), 1)
        x, y = wintypes.UINT(), wintypes.UINT()
        shcore.GetDpiForMonitor.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.POINTER(wintypes.UINT),
                                            ctypes.POINTER(wintypes.UINT)]
        if not monitor or shcore.GetDpiForMonitor(monitor, 0, ctypes.byref(x), ctypes.byref(y)) != 0:
            return None
        return int(x.value) if x.value and x.value == y.value else None

    def virtual_screen(self):
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        return tuple(int(user32.GetSystemMetrics(i)) for i in (76, 77, 78, 79))  # SM_{X,Y}VIRTUALSCREEN, CX, CY

    def _exe(self, kernel32, pid):
        ctypes, wintypes = _ct()
        handle = kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            return None
        try:
            buf = ctypes.create_unicode_buffer(32768)
            size = wintypes.DWORD(len(buf))
            return os.path.basename(buf.value).lower() if kernel32.QueryFullProcessImageNameW(
                handle, 0, buf, ctypes.byref(size)) else None
        finally:
            kernel32.CloseHandle(handle)

    def _elevated(self, kernel32, advapi32, pid):
        ctypes, wintypes = _ct()
        *_, TOKEN_ELEVATION = _structures()
        handle = kernel32.OpenProcess(0x1000, False, pid)
        if not handle:
            return None  # an elevated process refuses this: unknown is refused by the guard
        token = wintypes.HANDLE()
        try:
            if not advapi32.OpenProcessToken(handle, 0x0008, ctypes.byref(token)):  # TOKEN_QUERY
                return None
            info, size = TOKEN_ELEVATION(), wintypes.DWORD()
            ok = advapi32.GetTokenInformation(token, 20, ctypes.byref(info), ctypes.sizeof(info),
                                              ctypes.byref(size))  # TokenElevation
            kernel32.CloseHandle(token)
            return bool(info.TokenIsElevated) if ok else None
        finally:
            kernel32.CloseHandle(handle)

    def _facts(self, hwnd) -> Window | None:
        ctypes, wintypes = _ct()
        user32, kernel32, advapi32, _ = _dlls()
        if not hwnd:
            return None
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        buffer = ctypes.create_unicode_buffer(257)
        cls = decode_class_name(user32.GetClassNameW(hwnd, buffer, 257), buffer.value)  # None: unreadable
        rect, origin = wintypes.RECT(), wintypes.POINT(0, 0)
        if not user32.GetClientRect(hwnd, ctypes.byref(rect)) or not user32.ClientToScreen(hwnd, ctypes.byref(origin)):
            return None
        monitor = user32.MonitorFromWindow(hwnd, 0)
        info = (ctypes.c_byte * 40)()  # MONITORINFO: cbSize, rcMonitor, rcWork, dwFlags
        ctypes.cast(info, ctypes.POINTER(wintypes.DWORD))[0] = 40
        primary = bool(monitor) and bool(user32.GetMonitorInfoW(monitor, info)) and bool(
            ctypes.cast(info, ctypes.POINTER(wintypes.DWORD))[9] & 1)
        return Window(hwnd=int(hwnd), pid=int(pid.value), exe=self._exe(kernel32, pid.value), cls=cls,
                      client=(origin.x, origin.y, origin.x + rect.right, origin.y + rect.bottom),
                      primary_monitor=primary, iconic=bool(user32.IsIconic(hwnd)),
                      elevated=self._elevated(kernel32, advapi32, pid.value))

    def target_window(self, pid):
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        found = []

        @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid and user32.IsWindowVisible(hwnd):
                found.append(hwnd)
            return True

        user32.EnumWindows(visit, 0)
        windows = [w for w in (self._facts(h) for h in found) if w is not None]
        main = [w for w in windows if w.cls and not is_helper_class(w.cls)]
        return max(main, key=lambda w: (w.client[2] - w.client[0]) * (w.client[3] - w.client[1])) if main else None

    def evidence(self, pid, point, want_focus):
        ctypes, wintypes = _ct()
        user32, *_ = _dlls()
        GUITHREADINFO = _structures()[3]
        window = self._facts(user32.GetForegroundWindow())
        root = None
        if point is not None:
            hit = user32.WindowFromPoint(wintypes.POINT(int(point[0]), int(point[1])))
            root = decode_ancestor(user32.GetAncestor(hit, GA_ROOT)) if hit else None  # NULL: unknown, not `hit`
        focus = None
        if want_focus and window is not None:
            info = GUITHREADINFO()
            info.cbSize = ctypes.sizeof(info)
            tid = user32.GetWindowThreadProcessId(window.hwnd, None)
            if user32.GetGUIThreadInfo(tid, ctypes.byref(info)) and info.hwndFocus:
                owner = wintypes.DWORD()
                user32.GetWindowThreadProcessId(info.hwndFocus, ctypes.byref(owner))
                top = decode_ancestor(user32.GetAncestor(info.hwndFocus, GA_ROOT))
                focus = Focus(hwnd=int(info.hwndFocus), top_hwnd=top, pid=int(owner.value)) if top else None
        held = any(user32.GetAsyncKeyState(vk) & 0x8000 for vk in MODIFIER_VKS)
        return Evidence(stop_present=self.stop_present(), foreground=window, root_at_point=root,
                        modifiers_held=held, focus=focus)


class RealSender(Sender):  # pragma: no cover - never executed by the tests
    def prepare(self, inputs):
        """Loads the DLL and builds the event array: all native work, before the final recheck."""
        ctypes, wintypes = _ct()
        MOUSEINPUT, KEYBDINPUT, INPUT, *_ = _structures()
        user32, *_ = _dlls()
        user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
        array = (INPUT * len(inputs))()
        for slot, event in zip(array, inputs):
            if event["kind"] == "mouse":
                slot.type = INPUT_MOUSE
                slot.u.mi = MOUSEINPUT(event["dx"], event["dy"], 0, event["flags"], 0, 0)
            else:
                slot.type = INPUT_KEYBOARD
                slot.u.ki = KEYBDINPUT(event["vk"], event["scan"], event["flags"], 0, 0)
        return user32, array, len(inputs), ctypes.sizeof(INPUT)

    def send(self, prepared):
        """Nothing but the call; returns the number of events the operating system inserted (the caller
        decodes it: a partial insertion is an uncertain delivery, never a success)."""
        user32, array, count, size = prepared
        return int(user32.SendInput(count, array, size))


class RealGdi(Gdi):  # pragma: no cover - never executed by the tests
    def print_window(self, hwnd, width, height):
        ctypes, wintypes = _ct()
        *_, BITMAPINFOHEADER, _elev = _structures()
        user32, _k, _a, gdi32 = _dlls()
        header = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
        bits = ctypes.c_void_p()
        memory = gdi32.CreateCompatibleDC(None)  # a memory DC: no screen pixel is read through it
        if not memory:
            return None
        bitmap = gdi32.CreateDIBSection(memory, ctypes.byref(header), 0, ctypes.byref(bits), None, 0)
        if not bitmap or not bits:
            gdi32.DeleteDC(memory)
            return None
        old = gdi32.SelectObject(memory, bitmap)
        selected = decode_select_object(old)
        try:
            if not selected:  # the bitmap is not the destination: no image, whatever PrintWindow would do
                return None
            if not user32.PrintWindow(hwnd, memory, PW_CLIENTONLY | PW_RENDERFULLCONTENT):
                return None
            return ctypes.string_at(bits, width * height * 4)
        finally:
            if selected:
                gdi32.SelectObject(memory, old)
            gdi32.DeleteObject(bitmap)
            gdi32.DeleteDC(memory)
