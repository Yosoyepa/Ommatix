#!/usr/bin/env python3
"""Fakes for the AG-03 tests: a clock, an operating system, an input sender and capture backends.

Nothing here calls a Win32 function, reads a screen or sends input: the tests drive the pure guards and
the pixel server through these, with a fake monotonic clock and a timeline of operating-system changes
(a stop file that appears at a given time, a modifier pressed between two readings, a foreground change
between the initial guard and the delivery check). Pixels are tiny made-up buffers; "foreign" pixels are
unique 4-byte values by construction (desktop, backdrop, occluder), so a leak is detectable by equality.
"""

from __future__ import annotations

import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_capture as cap  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_win32 as w32  # noqa: E402


class FakeClock:
    def __init__(self, t: float = 0.0):
        self.t = t

    def __call__(self) -> float:
        return self.t

    def advance(self, dt: float) -> None:
        self.t += dt


TARGET = g.Window(hwnd=100, pid=4242, exe=g.TESTAPP_EXE, cls="TestAppWindow", client=(100, 50, 1380, 770),
                  primary_monitor=True, iconic=False, elevated=False)


class FakeOs(w32.OsReader):
    """Operating-system state the tests set by hand. ``timeline`` is a list of ``(t, mutation)``: a
    mutation (a callable taking this object) runs the first time a reading is made at clock >= t. ``hooks``
    maps the Nth ``evidence`` call (1-based) to a mutation that runs just before that reading is made."""

    def __init__(self, clock: FakeClock, window: g.Window = TARGET):
        self.clock = clock
        self.window = window
        self.foreground: g.Window | None = window
        self.stop: bool | None = False
        self.root: int | None = window.hwnd
        self.modifiers: bool | None = False
        self.focus: g.Focus | None = g.Focus(hwnd=window.hwnd + 1, top_hwnd=window.hwnd, pid=window.pid)
        self.timeline: list[tuple[float, object]] = []
        self.hooks: dict[int, object] = {}
        self.evidence_calls = 0
        self.fail = False

    def _apply_timeline(self):
        for item in list(self.timeline):
            if self.clock.t >= item[0]:
                self.timeline.remove(item)
                item[1](self)

    def stop_present(self):
        self._apply_timeline()
        return self.stop

    def target_window(self, pid):
        return self.window

    def virtual_screen(self):
        return (0, 0, 1920, 1080)

    def ensure_dpi_aware(self):
        return True

    def evidence(self, pid, point, want_focus):
        self.evidence_calls += 1
        if self.fail:
            raise OSError("fake failure")
        if (hook := self.hooks.get(self.evidence_calls)) is not None:
            hook(self)
        self._apply_timeline()
        try:
            desktop = self.virtual_screen() if point is not None else None
        except Exception:  # noqa: BLE001 - an unreadable geometry is an unknown one
            desktop = None
        return g.Evidence(stop_present=self.stop, foreground=self.foreground,
                          root_at_point=self.root if point is not None else None,
                          modifiers_held=self.modifiers, focus=self.focus if want_focus else None, desktop=desktop)


class FakeSender(w32.Sender):
    """Records batches; nothing is ever sent. ``prepare`` builds the native batch (``on_prepare`` may take
    time on the fake clock); ``send`` returns how many events the operating system inserted: all of them,
    ``inserted`` of them (a partial send) or raises when ``raises`` is set."""

    def __init__(self, ok: bool = True, inserted: int | None = None, raises: bool = False):
        self.batches: list[list[dict]] = []
        self.prepared: list[list[dict]] = []
        self.ok, self.inserted, self.raises = ok, inserted, raises
        self.on_prepare = None
        self.trace = None

    def prepare(self, inputs):
        if self.trace is not None:
            self.trace.append("prepare")
        if self.on_prepare is not None:
            self.on_prepare()
        self.prepared.append(list(inputs))
        return list(inputs)

    def send(self, prepared):
        if self.trace is not None:
            self.trace.append("send")
        self.batches.append(list(prepared))
        if self.raises:
            raise OSError("fake SendInput failure")
        if not self.ok:
            return 0
        return len(prepared) if self.inserted is None else self.inserted


# ---- capture fakes ---------------------------------------------------------------------------------

DESKTOP = bytes((1, 2, 3, 255))     # a pixel of the desktop
BACKDROP = bytes((4, 5, 6, 255))    # a pixel of the system backdrop behind a translucent window
OCCLUDER = bytes((7, 8, 9, 255))    # a pixel of a foreign window present only during acquisition
OWN = [bytes((10 + i, 20 + i, 30 + i, 255)) for i in range(12)]  # the application's own pixels (4 x 3)


def window_4x3(hwnd: int = 100) -> g.Window:
    return g.Window(hwnd=hwnd, pid=4242, exe=g.TESTAPP_EXE, cls="W", client=(0, 0, 4, 3), primary_monitor=True,
                    iconic=False, elevated=False)


class SceneBackend(cap.CaptureBackend):
    """A fake backend over one scene description. ``app_rendered`` backends return the application's own
    pixels only; a desktop-source backend returns what is on screen, which is the composite (desktop where
    the window is translucent, backdrop where it is a backdrop, the occluder while one is present)."""

    def __init__(self, scene: str, *, app_rendered: bool, name: str = "printwindow"):
        self.scene, self.app_rendered, self.name = scene, app_rendered, name
        self.calls = 0

    def own(self) -> bytes:
        return b"".join(OWN)

    def acquire(self, window):
        self.calls += 1
        pixels = list(OWN)
        if self.app_rendered:
            return cap.RawFrame(4, 3, b"".join(pixels))
        if self.scene == "translucent":
            pixels[1] = pixels[5] = DESKTOP
        elif self.scene == "backdrop":
            pixels[0] = pixels[4] = BACKDROP
        elif self.scene == "rounded":
            pixels[0] = pixels[3] = DESKTOP  # the corners belong to whatever is behind
        elif self.scene == "foreign_during_acquisition":
            pixels[6] = OCCLUDER  # identical states before and after: present only while the copy happens
        return cap.RawFrame(4, 3, b"".join(pixels))


def scene(name: str) -> cap.Scene:
    return cap.Scene(name=name, window=window_4x3(), expected=b"".join(OWN),
                     foreign=frozenset({DESKTOP, BACKDROP, OCCLUDER}))


class FailingBackend(cap.CaptureBackend):
    name, app_rendered = "printwindow", True

    def __init__(self, mode: str):
        self.mode = mode

    def acquire(self, window):
        if self.mode == "raise":
            raise OSError("PrintWindow failed")
        if self.mode == "none":
            return None
        if self.mode == "wrong_size":
            return cap.RawFrame(2, 2, bytes(16))
        if self.mode == "short_buffer":
            return cap.RawFrame(4, 3, bytes(8))
        raise AssertionError(self.mode)


def full_admission(backend_name: str = "printwindow") -> cap.AdmissionRecord:
    return cap.AdmissionRecord(backend_name, {name: True for name in cap.REQUIRED_SCENARIOS})
