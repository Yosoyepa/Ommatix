#!/usr/bin/env python3
"""AG-03 coordinate-actuator guards (ADR-0038 decision 4) as pure functions.

Nothing here touches the operating system: every function takes a snapshot (``Window``,
``Evidence``) and returns ``None`` (allowed) or a closed refusal token. The impure parts - reading
the snapshot, reserving in the ledger, sending the input - live in ``pixel_server.py`` and are
injected, so every guard is unit-tested with a fake clock and a fake operating-system state and no
input is ever sent by a test (AGENTS.md section 6).

The order of decision 4, which ``pixel_server`` follows and ``test_guards`` pins:

1. the operator's stop file is absent; 2. the phase allows it (phase 1: ``simulated``);
3. a screenshot taken in this goal is at most ``SCREENSHOT_MAX_AGE_S`` old; 4. the ledger reserves
the live action; 5. the foreground window belongs to the process this run started; 6. the point
maps inside its client rectangle and nothing is in front of it; 7. no modifier is held;
8. the *delivery check*: just before the input batch, with a fresh clock reading and a fresh read of
the operating system, items 1, 3, 5, 6 (7 too) again - and for ``type_text``, which has no point,
the keyboard-focus window of the attested foreground thread. A refusal at 5-8 keeps the
reservation of 4 (the budget is spent, as a refused engine action keeps its reservation).
"""

from __future__ import annotations

from dataclasses import dataclass

SCREENSHOT_MAX_AGE_S = 10.0
# The only executables a pixel click or key may reach (decision 4 item 5): a two-name constant.
# ``explorer.exe`` and ``applicationframehost.exe`` are deliberately not in it (AGENTS.md rule 6).
TESTAPP_EXE = "ommatix-testapp.exe"
EDITOR_EXE = "ommatix-testeditor.exe"
ALLOWED_EXES = (TESTAPP_EXE, EDITOR_EXE)
# A GUI process owns helper windows besides its main window (winit's visible event target, input
# method windows): never a target (tools/s4/e2e_driver.py HELPER_WINDOW_MARKERS).
HELPER_WINDOW_MARKERS = ("Event Target", "IME", "MSCTFIME")

Rect = tuple[int, int, int, int]  # left, top, right, bottom; right and bottom exclusive


@dataclass(frozen=True)
class Window:
    """What the operating system said about one top-level window (no title, no text)."""
    hwnd: int
    pid: int
    exe: str | None
    cls: str
    client: Rect  # client rectangle in screen coordinates
    primary_monitor: bool
    iconic: bool
    elevated: bool | None  # None: could not be read


@dataclass(frozen=True)
class Focus:
    """The keyboard-focus window of the attested foreground thread (``GetGUIThreadInfo``) and the
    top-level window and process it belongs to."""
    hwnd: int
    top_hwnd: int
    pid: int


@dataclass(frozen=True)
class Evidence:
    """One reading of the operating system. ``None`` in a field means it could not be read, which
    every guard treats as a refusal (an unreadable fact is not a pass)."""
    stop_present: bool | None
    foreground: Window | None
    root_at_point: int | None = None  # top-level hwnd at the click point (click only)
    modifiers_held: bool | None = None
    focus: Focus | None = None  # type_text only


@dataclass(frozen=True)
class Attested:
    """What the runner started for this goal: its pid and the executable it must be."""
    pid: int
    exe: str

    def __post_init__(self):
        if self.exe not in ALLOWED_EXES:
            raise ValueError("the attested executable is not one of the two test applications")


@dataclass(frozen=True)
class Shot:
    """The last screenshot the agent was given: when (injected monotonic clock), the window it
    came from and the client rectangle and image size that define its coordinate space."""
    t: float
    hwnd: int
    client: Rect
    width: int
    height: int


def is_helper_class(class_name: str) -> bool:
    return any(marker in class_name for marker in HELPER_WINDOW_MARKERS)


def guard_stop(stop_present: bool | None) -> str | None:
    if stop_present is None:
        return "STOP_UNREADABLE"
    return "STOP_ENGAGED" if stop_present else None


def guard_shot_age(now: float, shot: Shot | None, max_age: float = SCREENSHOT_MAX_AGE_S) -> str | None:
    """There deliberately is no content-freshness check (decision 8): only the age of the shot."""
    if shot is None:
        return "SCREENSHOT_MISSING"
    age = now - shot.t
    if age < 0 or age > max_age:
        return "SCREENSHOT_STALE"
    return None


def guard_window(window: Window | None, attested: Attested, shot: Shot | None = None) -> str | None:
    """Item 5: the foreground window belongs to the process this run started, is one of the two
    test applications, is not a helper window, is not elevated, is on the primary monitor and is not
    iconified; once a screenshot exists it is the window the screenshot came from, with the same
    client rectangle (the point mapping would otherwise be wrong)."""
    if window is None:
        return "FOREGROUND_UNREADABLE"
    if window.exe is None or window.exe not in ALLOWED_EXES or window.exe != attested.exe:
        return "EXE_NOT_ALLOWED"
    if window.pid != attested.pid:
        return "FOREGROUND_NOT_TARGET_PROCESS"
    if is_helper_class(window.cls):
        return "HELPER_WINDOW"
    if window.elevated is None or window.elevated:
        return "ELEVATED_OR_UNKNOWN"
    if not window.primary_monitor:
        return "NOT_PRIMARY_MONITOR"
    if window.iconic:
        return "WINDOW_ICONIC"
    if shot is not None:
        if window.hwnd != shot.hwnd:
            return "WINDOW_CHANGED"
        if window.client != shot.client:
            return "GEOMETRY_CHANGED"
    return None


def rect_contains(rect: Rect, x: int, y: int) -> bool:
    left, top, right, bottom = rect
    return left <= x < right and top <= y < bottom


def guard_point(window: Window, x: int, y: int, root_at_point: int | None) -> str | None:
    """Item 6: the screen point lies inside the client rectangle and ``WindowFromPoint`` resolves it
    to the same top-level window (nothing in front of it)."""
    if not rect_contains(window.client, x, y):
        return "POINT_OUTSIDE_CLIENT"
    if root_at_point is None or root_at_point != window.hwnd:
        return "POINT_WINDOW_MISMATCH"
    return None


def guard_modifiers(held: bool | None) -> str | None:
    if held is None:
        return "MODIFIERS_UNREADABLE"
    return "MODIFIER_HELD" if held else None


def guard_focus(focus: Focus | None, window: Window) -> str | None:
    """``type_text`` has no point: the keyboard-focus window must be the attested top-level window
    or one of its descendants, in the same process. It does not say which field has focus; that is
    what G09 measures."""
    if focus is None:
        return "FOCUS_UNREADABLE"
    if focus.top_hwnd != window.hwnd or focus.pid != window.pid:
        return "FOCUS_OUTSIDE_WINDOW"
    return None


def screen_point(shot: Shot, x: int, y: int) -> tuple[int, int]:
    """The screen coordinates of image pixel (x, y): the client rectangle scaled to the image."""
    left, top, right, bottom = shot.client
    return (left + (x * (right - left)) // shot.width, top + (y * (bottom - top)) // shot.height)


def check_evidence(evidence: Evidence, shot: Shot | None, attested: Attested,
                   point: tuple[int, int] | None) -> str | None:
    """Items 5, 6 (point) or the focus window, and 7 against ONE reading of the operating system.
    ``point`` is the screen point of a click; ``None`` is ``type_text``."""
    return (guard_window(evidence.foreground, attested, shot)
            or (guard_point(evidence.foreground, point[0], point[1], evidence.root_at_point)
                if point is not None else guard_focus(evidence.focus, evidence.foreground))
            or guard_modifiers(evidence.modifiers_held))


def check_batch(evidence: Evidence, now: float, shot: Shot | None, attested: Attested,
                point: tuple[int, int] | None) -> str | None:
    """The delivery check (item 8): items 1 and 3 and ``check_evidence``, against ONE fresh reading of
    the operating system and ONE fresh reading of the injected monotonic clock, immediately before
    the input batch and after the reservation and any blocking work."""
    return (guard_stop(evidence.stop_present) or guard_shot_age(now, shot)
            or check_evidence(evidence, shot, attested, point))
