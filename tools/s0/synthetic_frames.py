"""Synthetic frames for the S0 detector measurements (docs/thesis/S0_RESULTADOS.md).

Everything is drawn here from fixed shapes and a fixed seed: no capture, no screen text.
Two outputs, both into --out-dir (outside the repository):

  probe/<name>_<W>x<H>.bgra   raw BGRA frames for the `s0_probe` example (provider and
                              thread equivalence and timings of one full inference);
  seq<k>/NNN.png              PNG sequences for `ommatix demo --source images`: frame 000
                              is the base window (full refresh), then the frames alternate
                              between the base and a variant with k small changes far
                              apart, so every later observation is incremental with k
                              planned regions;
  seqfull/NNN.png             the base and its negative alternating: every observation is
                              a full refresh.

Usage: python tools/s0/synthetic_frames.py --out-dir <dir> [--frames 20]
Needs numpy and Pillow (not dependencies of the binary).
"""

import argparse
import pathlib

import numpy as np
from PIL import Image

W, H = 1920, 1080


def window(seed: int = 0) -> np.ndarray:
    """An RGB window: title bar, menu, toolbar buttons, a text area, a status bar."""
    rng = np.random.default_rng(seed)
    img = np.full((H, W, 3), 238, np.uint8)
    img[0:40] = (32, 32, 40)  # title bar
    for i, x in enumerate((1780, 1830, 1875)):  # caption buttons
        img[10:30, x : x + 30] = (200, 60, 60) if i == 2 else (90, 90, 100)
    img[40:72] = (248, 248, 248)  # menu bar
    for k in range(6):  # menu "words"
        x = 12 + 90 * k
        img[50:62, x : x + 50 + 7 * (k % 3)] = (40, 40, 40)
    img[72:120] = (228, 228, 232)  # toolbar
    for k in range(10):  # buttons with a glyph
        x = 12 + 52 * k
        img[78:114, x : x + 40] = (205, 210, 220)
        img[78:80, x : x + 40] = img[112:114, x : x + 40] = (120, 120, 130)
        img[88:104, x + 12 : x + 28] = rng.integers(20, 120, 3, dtype=np.uint8)
    img[130:1030, 20:1900] = 255  # text area
    for row in range(40):  # text lines of word-like blocks
        y = 150 + 21 * row
        x = 40
        for _ in range(int(rng.integers(3, 14))):
            w = int(rng.integers(20, 110))
            if x + w > 1860:
                break
            img[y : y + 11, x : x + w] = (30, 30, 30)
            x += w + 9
    img[1040:1080] = (240, 240, 240)  # status bar
    img[1050:1068, 30:240] = (70, 70, 70)
    img[1050:1068, 1700:1890] = (70, 70, 70)
    return img


# Small changes far apart (each a 24x16 box), as a caret, a toggled button or a counter.
CHANGES = [(1600, 1052), (300, 600), (1200, 300)]


def variant(base: np.ndarray, k: int) -> np.ndarray:
    img = base.copy()
    for x, y in CHANGES[:k]:
        img[y : y + 16, x : x + 24] = (20, 110, 220)
    return img


def bgra(img: np.ndarray) -> bytes:
    a = np.full(img.shape[:2] + (1,), 255, np.uint8)
    return np.concatenate([img[..., ::-1], a], axis=2).tobytes()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True, type=pathlib.Path)
    ap.add_argument("--frames", type=int, default=20)
    args = ap.parse_args()
    base = window()
    probe = args.out_dir / "probe"
    probe.mkdir(parents=True, exist_ok=True)
    noise = np.random.default_rng(0).integers(0, 256, (H, W, 3), dtype=np.uint8)
    for name, img in (("window", base), ("variant3", variant(base, 3)), ("noise", noise)):
        (probe / f"{name}_{W}x{H}.bgra").write_bytes(bgra(img))
    for k in (1, 2, 3):
        seq = args.out_dir / f"seq{k}"
        seq.mkdir(parents=True, exist_ok=True)
        for i in range(args.frames):
            img = base if i % 2 == 0 else variant(base, k)
            Image.fromarray(img).save(seq / f"{i:03}.png")
    # Every frame changes everywhere (the base and its negative alternate): every
    # observation is a full refresh (changed area above full_refresh_area_ratio).
    full = args.out_dir / "seqfull"
    full.mkdir(parents=True, exist_ok=True)
    for i in range(args.frames):
        Image.fromarray(base if i % 2 == 0 else 255 - base).save(full / f"{i:03}.png")


if __name__ == "__main__":
    main()
