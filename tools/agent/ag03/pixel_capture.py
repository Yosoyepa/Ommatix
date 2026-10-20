#!/usr/bin/env python3
"""AG-03 capture (ADR-0038 decision 5): the only admitted backend renders the window itself.

The guarantee of the pixel boundary is the capture backend's, not a check after the fact. **The only
backend that can be admitted is an app-rendered one** (``PrintWindow`` of the window's handle into an
app-owned bitmap). A screen copy of the window's rectangle is not admitted in any form and not as a
fallback: there is no second backend in this module, and a backend that is not admitted, fails, or
returns the wrong size gives ``CaptureRefused`` and **no image**. Admission rests on a record
written after the Step 2 fidelity and isolation tests on synthetic windows (``AdmissionRecord``);
no runtime check can prove the isolation of an admitted backend, and none is claimed.

This module also draws the synthetic images (a drawn word and a shape, a built-in 5x7 font), used by
the pixel server's synthetic mode, the Step 0 probe, the isolation preflight and the gate smoke:
no capture, no screen, nothing read from the operating system. Standard library only. Images exist
only in memory; this module never writes one anywhere.
"""

from __future__ import annotations

import json
import operator
import struct
import sys
import zlib
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

from pixel_guards import Window  # noqa: E402

ADMISSION_SCHEMA = "ag03-capture-admission-1"
# The one admitted backend name. There is deliberately no other entry (decision 5, alternative K).
ADMITTED_BACKENDS = ("printwindow",)
REQUIRED_SCENARIOS = ("opaque", "translucent", "backdrop", "rounded", "foreign_during_acquisition")
MAX_IMAGE_EDGE = 1568  # pixels; a larger client area is scaled down to fit (the click mapping scales back)
MAX_IMAGE_BYTES = 5 * 1024 * 1024
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


class CaptureRefused(Exception):
    """No image is returned. ``reason`` is a closed token."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


# ---- PNG (stdlib) -------------------------------------------------------------------------------

def _chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def png_encode(width: int, height: int, rgb: bytes) -> bytes:
    """An 8-bit RGB PNG of ``width * height`` pixels (``rgb`` is row-major, 3 bytes a pixel)."""
    if width < 1 or height < 1 or len(rgb) != width * height * 3:
        raise ValueError("pixel buffer does not match the size")
    stride = width * 3
    raw = b"".join(b"\x00" + rgb[y * stride:(y + 1) * stride] for y in range(height))
    return (PNG_MAGIC + _chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + _chunk(b"IDAT", zlib.compress(raw, 6)) + _chunk(b"IEND", b""))


def png_info(data: bytes) -> tuple[int, int] | None:
    """(width, height) of a PNG from its header, or None when ``data`` is not a PNG."""
    if len(data) < 33 or not data.startswith(PNG_MAGIC) or data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", data[16:24])
    return (width, height) if width and height else None


def bgra_to_rgb(bgra: bytes, width: int, height: int) -> bytes:
    """Drops the alpha channel (undefined in a ``PrintWindow`` bitmap) and swaps to RGB."""
    if len(bgra) != width * height * 4:
        raise ValueError("pixel buffer does not match the size")
    out = bytearray(width * height * 3)
    out[0::3], out[1::3], out[2::3] = bgra[2::4], bgra[1::4], bgra[0::4]
    return bytes(out)


def fit_size(width: int, height: int, edge: int = MAX_IMAGE_EDGE) -> tuple[int, int]:
    """The image size for a client area: unchanged when it fits, else scaled so the long edge is ``edge``."""
    if max(width, height) <= edge:
        return width, height
    scale = edge / max(width, height)
    return max(1, round(width * scale)), max(1, round(height * scale))


def scale_rgb(rgb: bytes, width: int, height: int, new_width: int, new_height: int) -> bytes:
    """Nearest-neighbour resampling of row-major RGB (the click mapping inverts it exactly)."""
    if (width, height) == (new_width, new_height):
        return rgb
    cols = [(x * width) // new_width for x in range(new_width)]
    pick = operator.itemgetter(*cols) if len(cols) > 1 else (lambda row: (row[cols[0]],))
    out = bytearray()
    for y in range(new_height):
        src = rgb[((y * height) // new_height) * width * 3:][:width * 3]
        r, g, b = src[0::3], src[1::3], src[2::3]
        row = bytearray(new_width * 3)
        row[0::3], row[1::3], row[2::3] = bytes(pick(r)), bytes(pick(g)), bytes(pick(b))
        out += row
    return bytes(out)


# ---- the admitted backend and its admission ------------------------------------------------------

@dataclass(frozen=True)
class RawFrame:
    width: int
    height: int
    bgra: bytes


class CaptureBackend:
    """How a window's pixels are acquired. ``app_rendered`` is True only when the window renders
    itself into a destination the harness owns."""
    name = ""
    app_rendered = False

    def acquire(self, window: Window) -> RawFrame | None:  # pragma: no cover - interface
        raise NotImplementedError


class PrintWindowBackend(CaptureBackend):
    """``PrintWindow`` of the window's handle into an app-owned bitmap. Written, never executed by
    the tests; the Step 2 tests on synthetic windows decide whether it is admitted."""
    name = "printwindow"
    app_rendered = True

    def __init__(self, gdi):
        self.gdi = gdi

    def acquire(self, window: Window) -> RawFrame | None:
        left, top, right, bottom = window.client
        width, height = right - left, bottom - top
        if width < 1 or height < 1:
            return None
        data = self.gdi.print_window(window.hwnd, width, height)
        return None if data is None else RawFrame(width, height, data)


@dataclass(frozen=True)
class AdmissionRecord:
    backend: str
    scenarios: dict

    def admits(self, backend: CaptureBackend) -> bool:
        return (backend.app_rendered is True and backend.name in ADMITTED_BACKENDS
                and self.backend == backend.name
                and all(self.scenarios.get(name) is True for name in REQUIRED_SCENARIOS))


def load_admission(path: Path) -> AdmissionRecord | None:
    """The record Step 2 writes after the fidelity and isolation tests; None when it is absent or is
    not exactly the expected shape (no record, no capture)."""
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if (not isinstance(data, dict) or data.get("schema") != ADMISSION_SCHEMA
            or not isinstance(data.get("backend"), str) or not isinstance(data.get("scenarios"), dict)):
        return None
    return AdmissionRecord(data["backend"], data["scenarios"])


@dataclass(frozen=True)
class Scene:
    """A fidelity/isolation scenario: the target window, the pixels its own application renders
    (``expected``, BGRA) and the pixels that belong to anything else (``foreign``, a set of 4-byte
    pixel values by construction unique to desktop, backdrop or occluder content)."""
    name: str
    window: Window
    expected: bytes
    foreign: frozenset


def scenario_passes(backend: CaptureBackend, scene: Scene) -> bool:
    """Step 2's per-scene test: the backend returns the right size, no foreign pixel appears, and the
    pixels equal what the application itself renders. Any failure or exception is a failure."""
    try:
        frame = backend.acquire(scene.window)
    except Exception:  # noqa: BLE001 - a failing backend does not pass
        return False
    left, top, right, bottom = scene.window.client
    if frame is None or (frame.width, frame.height) != (right - left, bottom - top):
        return False
    if len(frame.bgra) != frame.width * frame.height * 4:
        return False
    pixels = {frame.bgra[i:i + 4] for i in range(0, len(frame.bgra), 4)}
    return not (pixels & scene.foreign) and frame.bgra == scene.expected


def build_admission(backend: CaptureBackend, scenes: list[Scene]) -> dict:
    """The record body Step 2 writes: every required scenario present and passing, or not admitted."""
    results = {scene.name: scenario_passes(backend, scene) for scene in scenes}
    return {"schema": ADMISSION_SCHEMA, "backend": backend.name,
            "scenarios": {name: results.get(name) is True for name in REQUIRED_SCENARIOS}}


@dataclass(frozen=True)
class Captured:
    png: bytes
    width: int
    height: int


def capture_window(window: Window, backend: CaptureBackend, admission: AdmissionRecord | None) -> Captured:
    """The image of ``window``, or ``CaptureRefused`` and nothing. One backend, no fallback."""
    if admission is None or not admission.admits(backend):
        raise CaptureRefused("BACKEND_NOT_ADMITTED")
    try:
        frame = backend.acquire(window)
    except Exception:  # noqa: BLE001 - a backend that raises is a failed capture, not a crash
        raise CaptureRefused("BACKEND_FAILED") from None
    if frame is None:
        raise CaptureRefused("BACKEND_FAILED")
    left, top, right, bottom = window.client
    if (frame.width, frame.height) != (right - left, bottom - top) or len(frame.bgra) != frame.width * frame.height * 4:
        raise CaptureRefused("WRONG_SIZE")
    return encode_frame(frame)


def encode_frame(frame: RawFrame) -> Captured:
    new_width, new_height = fit_size(frame.width, frame.height)
    rgb = scale_rgb(bgra_to_rgb(frame.bgra, frame.width, frame.height), frame.width, frame.height,
                    new_width, new_height)
    png = png_encode(new_width, new_height, rgb)
    if len(png) > MAX_IMAGE_BYTES:
        raise CaptureRefused("IMAGE_TOO_LARGE")
    return Captured(png, new_width, new_height)


# ---- synthetic images (no capture, no screen) ----------------------------------------------------

FONT = {
    "A": "01110 10001 10001 11111 10001 10001 10001", "B": "11110 10001 10001 11110 10001 10001 11110",
    "C": "01110 10001 10000 10000 10000 10001 01110", "D": "11110 10001 10001 10001 10001 10001 11110",
    "E": "11111 10000 10000 11110 10000 10000 11111", "F": "11111 10000 10000 11110 10000 10000 10000",
    "G": "01110 10001 10000 10111 10001 10001 01111", "H": "10001 10001 10001 11111 10001 10001 10001",
    "I": "01110 00100 00100 00100 00100 00100 01110", "J": "00111 00010 00010 00010 00010 10010 01100",
    "K": "10001 10010 10100 11000 10100 10010 10001", "L": "10000 10000 10000 10000 10000 10000 11111",
    "M": "10001 11011 10101 10101 10001 10001 10001", "N": "10001 11001 10101 10011 10001 10001 10001",
    "O": "01110 10001 10001 10001 10001 10001 01110", "P": "11110 10001 10001 11110 10000 10000 10000",
    "Q": "01110 10001 10001 10001 10101 10010 01101", "R": "11110 10001 10001 11110 10100 10010 10001",
    "S": "01111 10000 10000 01110 00001 00001 11110", "T": "11111 00100 00100 00100 00100 00100 00100",
    "U": "10001 10001 10001 10001 10001 10001 01110", "V": "10001 10001 10001 10001 10001 01010 00100",
    "W": "10001 10001 10001 10101 10101 11011 10001", "X": "10001 10001 01010 00100 01010 10001 10001",
    "Y": "10001 10001 01010 00100 00100 00100 00100", "Z": "11111 00001 00010 00100 01000 10000 11111",
}
# (word, ink colour, shape): contrasting scenes, so that "what is written / which shape" changes with
# the image and a guess is unlikely to pass. The words are harmless and carry no screen text.
SYNTHETIC_SCENES = (
    ("LUNA", (200, 40, 40), "square"), ("RIO", (30, 90, 200), "circle"), ("MAR", (30, 140, 60), "triangle"),
    ("OSO", (170, 90, 20), "circle"), ("PEZ", (110, 40, 160), "square"), ("SOL", (20, 20, 20), "triangle"),
)
SHAPES = ("square", "circle", "triangle")
DEFAULT_SYNTHETIC_SIZE = (1280, 720)


def _fill(rgb: bytearray, width: int, x0: int, y0: int, x1: int, y1: int, colour) -> None:
    run = bytes(colour) * max(0, x1 - x0)
    for y in range(y0, y1):
        rgb[(y * width + x0) * 3:(y * width + x1) * 3] = run


def synthetic_rgb(index: int, width: int, height: int) -> tuple[bytes, str, str]:
    """(RGB pixels, drawn word, drawn shape) of synthetic scene ``index`` at the given size."""
    word, ink, shape = SYNTHETIC_SCENES[index % len(SYNTHETIC_SCENES)]
    rgb = bytearray(bytes((235, 235, 240)) * (width * height))
    _fill(rgb, width, 0, 0, width, max(1, height // 12), (60, 60, 70))  # a fake title bar
    cell = max(2, min(width // (len(word) * 6 + 2), height // 12))
    x_start = (width - len(word) * 6 * cell) // 2
    y_start = height // 6
    for n, char in enumerate(word):
        for row, bits in enumerate(FONT[char].split()):
            for col, bit in enumerate(bits):
                if bit == "1":
                    x0 = x_start + (n * 6 + col) * cell
                    y0 = y_start + row * cell
                    _fill(rgb, width, x0, y0, x0 + cell, y0 + cell, ink)
    size = max(4, height // 4)
    cx, cy = width // 2, y_start + 7 * cell + size
    for y in range(max(0, cy - size), min(height, cy + size)):
        if shape == "square":
            _fill(rgb, width, max(0, cx - size), y, min(width, cx + size), y + 1, ink)
        elif shape == "circle":
            half = int((size * size - (y - cy) ** 2) ** 0.5)
            _fill(rgb, width, max(0, cx - half), y, min(width, cx + half), y + 1, ink)
        else:  # triangle, apex up
            half = (y - (cy - size)) // 2
            _fill(rgb, width, max(0, cx - half), y, min(width, cx + half), y + 1, ink)
    return bytes(rgb), word, shape


def synthetic_png(index: int, width: int = DEFAULT_SYNTHETIC_SIZE[0],
                  height: int = DEFAULT_SYNTHETIC_SIZE[1]) -> tuple[bytes, str, str]:
    """(PNG, word, shape) of synthetic scene ``index``: drawn in memory, never captured."""
    rgb, word, shape = synthetic_rgb(index, width, height)
    return png_encode(width, height, rgb), word, shape
