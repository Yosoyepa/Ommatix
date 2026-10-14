#!/usr/bin/env python3
"""S0 spike: export Microsoft OmniParser ``icon_detect_v3`` (MIT) to ONNX for Ommatix.

The weights are never part of this repository (AGENTS.md rule 7). This script reads the
TorchScript file the operator downloaded, wraps it with the decoding that the model card
documents, exports the wrapper to ONNX and checks that ONNX Runtime reproduces the
TorchScript output on a synthetic image. It writes only into ``--out-dir``.

Source (pinned): https://huggingface.co/microsoft/OmniParser-v2.0, revision
f55d0750e5b94db2125ef0b45b0fa4a85ddc59b4, files ``icon_detect_v3/model.pt`` and
``icon_detect_v3/LICENSE`` (MIT). Do not use ``icon_detect/`` (AGPL-3.0).

Why a wrapper: the TorchScript module returns the raw heads, six tensors
``(cls_logits, box_ltrb)`` for strides 8, 16 and 32 (``[1, 1, G, G]`` and
``[1, 4, G, G]`` with ``G = 1280 / stride``). The Ommatix ONNX adapter accepts only the
anchor-free YOLO output ``[1, 4 + C, N]`` (``yolo_cxcywh_scores_cn``): box centre and size
in input pixels, then activated class scores. The wrapper applies the model card's decoding
(sigmoid on the logits; ``x0 = (gx + 0.5 - left) * stride`` and so on) and emits
``[1, 5, 33600]``, so the adapter needs no change. NMS stays in the adapter.

Environment used in S0 (Windows 11, Python 3.12.2, CPU only)::

    python -m venv s0venv
    s0venv\\Scripts\\python -m pip install torch --index-url https://download.pytorch.org/whl/cpu
    s0venv\\Scripts\\python -m pip install onnx onnxruntime numpy onnxscript
    s0venv\\Scripts\\python tools/s0/export_omniparser_v3.py ^
        --weights %LOCALAPPDATA%\\Ommatix\\models\\omniparser-icon-detect-v3\\model.pt ^
        --out-dir %LOCALAPPDATA%\\Ommatix\\models\\omniparser-icon-detect-v3

(torch 2.14.1+cpu, onnx 1.23.2, onnxruntime 1.31.0, numpy 2.5.4, onnxscript 0.7.2.)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
import torch

IMGSZ = 1280
STRIDES = (8, 16, 32)
OPSET = 17
EXPECTED_WEIGHTS_SHA256 = "11c6cbb77f22569fab22d86c76407a83ec81ab89dbfe28279854822d6e3fb00c"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


class DecodedHead(torch.nn.Module):
    """TorchScript raw heads -> ``[1, 5, N]``: cx, cy, w, h (input pixels), score."""

    # Tracing cannot enter the nested TorchScript module, so the wrapper is itself
    # scripted (torch.jit.script) and the scripted wrapper is exported.

    def __init__(self, scripted: torch.nn.Module) -> None:
        super().__init__()
        self.scripted = scripted
        for stride in STRIDES:
            grid = IMGSZ // stride
            gy, gx = torch.meshgrid(
                torch.arange(grid, dtype=torch.float32),
                torch.arange(grid, dtype=torch.float32),
                indexing="ij",
            )
            self.register_buffer(f"cx{stride}", (gx + 0.5).reshape(1, -1), persistent=False)
            self.register_buffer(f"cy{stride}", (gy + 0.5).reshape(1, -1), persistent=False)

    @staticmethod
    def level(cls: torch.Tensor, ltrb: torch.Tensor, cx: torch.Tensor, cy: torch.Tensor,
              stride: float) -> torch.Tensor:
        score = cls.sigmoid().flatten(2)[0]  # [1, G*G]
        d = ltrb.flatten(2)[0]  # [4, G*G]: left, top, right, bottom in grid units
        x0 = (cx - d[0:1]) * stride
        y0 = (cy - d[1:2]) * stride
        x1 = (cx + d[2:3]) * stride
        y1 = (cy + d[3:4]) * stride
        return torch.cat([(x0 + x1) * 0.5, (y0 + y1) * 0.5, x1 - x0, y1 - y0, score], dim=0)

    def forward(self, image: torch.Tensor) -> torch.Tensor:
        o = self.scripted(image)
        p8 = self.level(o[0], o[1], self.cx8, self.cy8, 8.0)
        p16 = self.level(o[2], o[3], self.cx16, self.cy16, 16.0)
        p32 = self.level(o[4], o[5], self.cx32, self.cy32, 32.0)
        return torch.cat([p8, p16, p32], dim=1).unsqueeze(0)  # [1, 5, N]


def card_reference(scripted: torch.nn.Module, x: torch.Tensor) -> np.ndarray:
    """The model card's own decoding (before thresholding), as ``[5, N]`` cx, cy, w, h, score."""
    with torch.no_grad():
        outputs = scripted(x)
    boxes, scores = [], []
    for i, stride in enumerate(STRIDES):
        cls = outputs[2 * i].sigmoid()[0, 0]
        ltrb = outputs[2 * i + 1][0]
        grid = cls.shape[-1]
        gy, gx = torch.meshgrid(
            torch.arange(grid, dtype=torch.float32),
            torch.arange(grid, dtype=torch.float32),
            indexing="ij",
        )
        cx, cy = gx + 0.5, gy + 0.5
        left, top, right, bottom = ltrb
        boxes.append(
            torch.stack(
                [(cx - left) * stride, (cy - top) * stride,
                 (cx + right) * stride, (cy + bottom) * stride], dim=-1
            ).reshape(-1, 4)
        )
        scores.append(cls.reshape(-1))
    xyxy = torch.cat(boxes).numpy()
    score = torch.cat(scores).numpy()
    cxcywh = np.stack([
        (xyxy[:, 0] + xyxy[:, 2]) / 2, (xyxy[:, 1] + xyxy[:, 3]) / 2,
        xyxy[:, 2] - xyxy[:, 0], xyxy[:, 3] - xyxy[:, 1], score,
    ])
    return cxcywh


def synthetic_ui(seed: int = 0) -> torch.Tensor:
    """A deterministic, synthetic window-like image (no screen capture): a light canvas
    with a title bar, a menu row of small dark boxes, framed buttons and text-like bars,
    centred in the 1280 letterbox with the 114 grey padding."""
    rng = np.random.default_rng(seed)
    img = np.full((IMGSZ, IMGSZ, 3), 114, dtype=np.uint8)
    top, bottom = 280, 1000  # a 1280x720 window letterboxed like a 1920x1080 frame
    img[top:bottom] = 245
    img[top:top + 32] = 230  # title bar
    for k in range(3):  # caption buttons
        img[top + 6:top + 26, 1180 + 30 * k:1200 + 30 * k] = 60
    for k in range(6):  # menu items
        x = 12 + 70 * k
        img[top + 44:top + 56, x:x + 48] = 40
    for k in range(4):  # framed buttons
        x0, y0 = 100 + 260 * k, 900
        img[y0:y0 + 40, x0:x0 + 180] = 225
        img[y0, x0:x0 + 180] = img[y0 + 39, x0:x0 + 180] = 90
        img[y0:y0 + 40, x0] = img[y0:y0 + 40, x0 + 179] = 90
        img[y0 + 16:y0 + 24, x0 + 40:x0 + 140] = 30
    for row in range(20):  # text-like lines of random words
        y = top + 80 + 26 * row
        x = 16
        while x < 1100:
            w = int(rng.integers(20, 90))
            img[y:y + 12, x:x + w] = 25
            x += w + int(rng.integers(8, 16))
    return torch.from_numpy(img).permute(2, 0, 1).float()[None] / 255.0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--weights", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--name", default="icon_detect_v3.onnx")
    args = parser.parse_args()

    actual = sha256(args.weights)
    if actual != EXPECTED_WEIGHTS_SHA256:
        print(f"weights SHA-256 {actual} != pinned {EXPECTED_WEIGHTS_SHA256}", file=sys.stderr)
        return 1

    torch.manual_seed(0)
    scripted = torch.jit.load(str(args.weights), map_location="cpu").eval()
    wrapper = torch.jit.script(DecodedHead(scripted).eval())
    dummy = torch.zeros(1, 3, IMGSZ, IMGSZ)
    out_path = args.out_dir / args.name
    args.out_dir.mkdir(parents=True, exist_ok=True)
    with torch.no_grad():
        torch.onnx.export(
            wrapper, (dummy,), str(out_path),
            input_names=["image"], output_names=["detections"],
            opset_version=OPSET, do_constant_folding=True, dynamo=False,
        )

    import onnx
    import onnxruntime as ort

    model = onnx.load(str(out_path))
    onnx.checker.check_model(model)
    session = ort.InferenceSession(str(out_path), providers=["CPUExecutionProvider"])
    (inp,), (out,) = session.get_inputs(), session.get_outputs()

    report = {
        "weights_sha256": actual,
        "onnx_path": str(out_path),
        "onnx_sha256": sha256(out_path),
        "onnx_bytes": out_path.stat().st_size,
        "opset": OPSET,
        "input": {"name": inp.name, "shape": inp.shape},
        "output": {"name": out.name, "shape": out.shape},
        "versions": {"torch": torch.__version__, "onnx": onnx.__version__,
                     "onnxruntime": ort.__version__, "numpy": np.__version__},
        "equivalence": [],
    }
    for label, image in [("synthetic_ui", synthetic_ui()),
                         ("uniform_noise_seed0", torch.rand(1, 3, IMGSZ, IMGSZ,
                                                            generator=torch.Generator().manual_seed(0)))]:
        reference = card_reference(scripted, image)  # [5, N]
        with torch.no_grad():
            wrapped = wrapper(image)[0].numpy()
        onnx_out = session.run(None, {inp.name: image.numpy()})[0][0]
        keep = reference[4] >= 0.05
        report["equivalence"].append({
            "image": label,
            "candidates": int(reference.shape[1]),
            "max_abs_diff_onnx_vs_torchscript_box_px": float(np.abs(onnx_out[:4] - reference[:4]).max()),
            "max_abs_diff_onnx_vs_torchscript_score": float(np.abs(onnx_out[4] - reference[4]).max()),
            "max_abs_diff_wrapper_vs_card_decoding": float(np.abs(wrapped - reference).max()),
            "above_0.05_torchscript": int(keep.sum()),
            "above_0.05_onnx": int((onnx_out[4] >= 0.05).sum()),
            "max_score": float(reference[4].max()),
        })
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
