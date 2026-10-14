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

``--dynamic`` (RNF-05, docs/thesis/S0_RESULTADOS.md §10) exports the same network with the
height and width of the input free (``image`` ``[1, 3, height, width]``, both multiples of
32) and the decoding grids computed from the heads' own shape, so the one file serves every
input size the engine asks for: the detector pins each size to its own session with
ONNX Runtime's free-dimension overrides. Same weights, same operators, same decoding as the
fixed 1280 export; only the grids stop being constants. RNF-05 measured it and the shipped
manifest does not use it (the detections of a smaller input differ, S0 §10.2).

``--fp16`` (RNF-05, S0 §10.3) exports the same network with its body in float16 and the
decoding in float32: the file is cut at the six raw heads, the network half is converted
(float32 at its input and output), and the halves are joined again. Only DirectML runs the
result (ONNX Runtime's CPU provider has no float16 convolution), so the script does not
check it against TorchScript; compare it on DirectML with the ``s0_shapes`` example of
``ommatix-adapter-vision`` (``S0_CANDIDATE_MANIFEST``). Needs ``onnxruntime`` for its
converter, as above::

    s0venv\\Scripts\\python tools/s0/export_omniparser_v3.py ^
        --weights %LOCALAPPDATA%\\Ommatix\\models\\omniparser-icon-detect-v3\\model.pt ^
        --out-dir %LOCALAPPDATA%\\Ommatix\\models\\omniparser-icon-detect-v3 ^
        --name icon_detect_v3_fp16.onnx --fp16
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

    def __init__(self, scripted: torch.nn.Module, dynamic: bool = False) -> None:
        super().__init__()
        self.scripted = scripted
        self.dynamic = dynamic
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
    def grid(cls: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        """Cell centres of a head, from its own ``[1, 1, rows, cols]`` shape (dynamic export)."""
        rows = cls.shape[2]
        cols = cls.shape[3]
        gx = torch.arange(cols, dtype=torch.float32) + 0.5
        gy = torch.arange(rows, dtype=torch.float32) + 0.5
        cx = gx.unsqueeze(0).expand(rows, cols).reshape(1, -1)
        cy = gy.unsqueeze(1).expand(rows, cols).reshape(1, -1)
        return cx, cy

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

    def decode(self, o0: torch.Tensor, o1: torch.Tensor, o2: torch.Tensor, o3: torch.Tensor,
               o4: torch.Tensor, o5: torch.Tensor) -> torch.Tensor:
        if self.dynamic:
            cx8, cy8 = self.grid(o0)
            cx16, cy16 = self.grid(o2)
            cx32, cy32 = self.grid(o4)
        else:
            cx8, cy8, cx16, cy16, cx32, cy32 = (
                self.cx8, self.cy8, self.cx16, self.cy16, self.cx32, self.cy32)
        p8 = self.level(o0, o1, cx8, cy8, 8.0)
        p16 = self.level(o2, o3, cx16, cy16, 16.0)
        p32 = self.level(o4, o5, cx32, cy32, 32.0)
        return torch.cat([p8, p16, p32], dim=1).unsqueeze(0)  # [1, 5, N]

    def forward(self, image: torch.Tensor) -> torch.Tensor:
        o = self.scripted(image)
        return self.decode(o[0], o[1], o[2], o[3], o[4], o[5])


class DecodedHeadWithHeads(DecodedHead):
    """The same graph, also returning the six raw heads: ``--fp16`` uses their names to tell
    the network from the decoding (the decoding stays in float32)."""

    def forward(self, image: torch.Tensor):
        o = self.scripted(image)
        return (self.decode(o[0], o[1], o[2], o[3], o[4], o[5]),
                o[0], o[1], o[2], o[3], o[4], o[5])


HEAD_OUTPUTS = [f"head{i}" for i in range(6)]


def network_in_float16(path: Path) -> Path:
    """Converts the network of the exported file to float16 and leaves the decoding in float32.

    ``path`` is the export that also returns the six raw heads (``head0``..``head5``). The
    file is cut at them: the network (image to heads) is converted, with float32 at its input
    and output, and the decoding (heads to detections: sigmoid, grid arithmetic in input
    pixels, concatenation) stays in float32, because float16 has a resolution of one pixel
    at coordinates above 1024. The two halves are joined again, so the result still takes
    and returns float32 tensors and is one file. Returns the path of that file.
    """
    import onnx
    from onnx import utils
    from onnxruntime.transformers.float16 import convert_float_to_float16

    network_path = path.with_name(path.stem + ".network.onnx")
    decoding_path = path.with_name(path.stem + ".decoding.onnx")
    utils.extract_model(str(path), str(network_path), ["image"], HEAD_OUTPUTS)
    utils.extract_model(str(path), str(decoding_path), HEAD_OUTPUTS, ["detections"])
    network = convert_float_to_float16(onnx.load(str(network_path)), keep_io_types=True)
    sort_topologically(network.graph)
    # Both halves were cut from one graph, so a node (a shared constant) that is in both has
    # the same name and the same content: the union by name is the original graph, with the
    # network in float16.
    decoding = onnx.load(str(decoding_path))
    merged = network
    graph = merged.graph
    names = {n.name for n in graph.node}
    graph.node.extend(n for n in decoding.graph.node if n.name not in names)
    initializers = {t.name for t in graph.initializer}
    graph.initializer.extend(t for t in decoding.graph.initializer if t.name not in initializers)
    del graph.output[:]
    graph.output.extend(decoding.graph.output)
    del graph.value_info[:]
    sort_topologically(graph)
    onnx.checker.check_model(merged)
    out = path.with_name(path.stem + ".fp16.onnx")
    onnx.save(merged, str(out))
    network_path.unlink()
    decoding_path.unlink()
    return out


def sort_topologically(graph) -> None:
    """Puts the nodes in dependency order: the converter appends its input casts last."""
    known = {i.name for i in graph.input} | {i.name for i in graph.initializer}
    pending = list(graph.node)
    ordered = []
    while pending:
        ready = [n for n in pending if all(not i or i in known for i in n.input)]
        if not ready:
            raise RuntimeError("the graph has a cycle or a dangling input")
        for node in ready:
            known.update(node.output)
        ordered.extend(ready)
        pending = [n for n in pending if n not in ready]
    del graph.node[:]
    graph.node.extend(ordered)


def card_reference(scripted: torch.nn.Module, x: torch.Tensor) -> np.ndarray:
    """The model card's own decoding (before thresholding), as ``[5, N]`` cx, cy, w, h, score."""
    with torch.no_grad():
        outputs = scripted(x)
    boxes, scores = [], []
    for i, stride in enumerate(STRIDES):
        cls = outputs[2 * i].sigmoid()[0, 0]
        ltrb = outputs[2 * i + 1][0]
        rows, cols = cls.shape[-2], cls.shape[-1]
        gy, gx = torch.meshgrid(
            torch.arange(rows, dtype=torch.float32),
            torch.arange(cols, dtype=torch.float32),
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
    parser.add_argument("--dynamic", action="store_true",
                        help="free height and width (multiples of 32) instead of 1280x1280")
    parser.add_argument("--fp16", action="store_true",
                        help="the network in float16, the decoding in float32 (RNF-05)")
    args = parser.parse_args()

    actual = sha256(args.weights)
    if actual != EXPECTED_WEIGHTS_SHA256:
        print(f"weights SHA-256 {actual} != pinned {EXPECTED_WEIGHTS_SHA256}", file=sys.stderr)
        return 1

    torch.manual_seed(0)
    scripted = torch.jit.load(str(args.weights), map_location="cpu").eval()
    head_class = DecodedHeadWithHeads if args.fp16 else DecodedHead
    wrapper = torch.jit.script(head_class(scripted, dynamic=args.dynamic).eval())
    dummy = torch.zeros(1, 3, IMGSZ, IMGSZ)
    out_path = args.out_dir / args.name
    args.out_dir.mkdir(parents=True, exist_ok=True)
    dynamic_axes = ({"image": {2: "height", 3: "width"}, "detections": {2: "candidates"}}
                    if args.dynamic else None)
    with torch.no_grad():
        torch.onnx.export(
            wrapper, (dummy,), str(out_path),
            input_names=["image"],
            output_names=["detections"] + (HEAD_OUTPUTS if args.fp16 else []),
            opset_version=OPSET, do_constant_folding=True, dynamo=False,
            dynamic_axes=dynamic_axes,
        )

    import onnx
    import onnxruntime as ort

    model = onnx.load(str(out_path))
    onnx.checker.check_model(model)
    if args.fp16:
        final = network_in_float16(out_path)
        out_path.unlink()
        final.rename(out_path)
        print(json.dumps({"fp16": True, "onnx_path": str(out_path),
                          "onnx_sha256": sha256(out_path),
                          "onnx_bytes": out_path.stat().st_size,
                          "note": "no CPU equivalence check: ONNX Runtime's CPU provider has "
                                  "no float16 convolution; compare on DirectML with the "
                                  "s0_shapes example"}, indent=2))
        return 0
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
    images = [("synthetic_ui", synthetic_ui()),
              ("uniform_noise_seed0", torch.rand(1, 3, IMGSZ, IMGSZ,
                                                 generator=torch.Generator().manual_seed(0)))]
    if args.dynamic:
        # Other sizes of the same network: a square 640 and a 480x640 crop of the window.
        ui = synthetic_ui()
        images += [("synthetic_ui_640_crop", ui[:, :, 280:920, 0:640].contiguous()),
                   ("synthetic_ui_480x640_crop", ui[:, :, 330:810, 100:740].contiguous())]
    for label, image in images:
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
