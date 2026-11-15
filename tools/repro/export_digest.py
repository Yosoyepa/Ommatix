#!/usr/bin/env python3
"""REL-01, P4.4: why the exported ``icon_detect_v3.onnx`` has another SHA-256 on another machine.

Read-only probes for the export environment of ``tools/s0/export_requirements.txt``
(docs/thesis/S4_SESIONES_PENDIENTES.md §A.2):

* ``kernels`` hashes torch's float32 ``sqrt`` and ``exp`` over 1 000 003 seeded values and counts how
  many results differ from the correctly rounded ones (float64, rounded to float32). The export folds
  each BatchNorm into its convolution with ``sqrt``; on 2026-11-15 the kernel MKL dispatched for it was
  the measured factor of the digest. Needs torch and numpy (the export environment); reads no file.
* ``fingerprint MODEL --out OUT`` writes the SHA-256 of every initializer, in order, and one hash of
  every node, in order, of an ONNX file: two exports can then be compared across machines without
  moving the weights (the JSON holds hashes and shapes, never a value). Needs ``onnx``.
* ``compare A B`` compares two fingerprints: standard library only.

None of them captures the screen, sends input, uses the network or writes anything but ``--out``.

    python tools/repro/export_digest.py kernels
    python tools/repro/export_digest.py fingerprint icon_detect_v3.onnx --out pinned.fingerprint.json
    python tools/repro/export_digest.py compare docs/thesis/evidencias/export-digest-E1-2026-11-15.json pinned.fingerprint.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

FORMAT = "ommatix-export-fingerprint/1"
#: The ``sqrt`` hashes of ``kernels`` measured on 2026-11-15 (Intel Core Ultra 7 155H, torch 2.14.1+cpu):
#: the correctly rounded result (the default AVX/AVX2 path, which exported ``92ae77a1…``) and the one
#: MKL gives with ``MKL_CBWR=SSE4_2`` or ``COMPATIBLE`` (15 % of the values one ULP off; ``b0004092…``).
KNOWN_SQRT = {"fe3367ed85f7": "correctly_rounded", "b5a552aa4d18": "mkl_sse4_2_or_compatible"}
PROBE_SIZE = 1_000_003


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def kernel_path(sqrt_hash: str) -> str:
    """The name of a ``sqrt`` hash recorded on 2026-11-15, or ``not_seen_on_2026_11_15``."""
    return KNOWN_SQRT.get(sqrt_hash, "not_seen_on_2026_11_15")


def kernels() -> dict:
    """torch's float32 ``sqrt`` and ``exp`` on seeded values, against the correctly rounded result."""
    import numpy as np  # the export environment only
    import torch

    def short(array) -> str:
        return _sha256(np.ascontiguousarray(array).tobytes())[:12]

    def ulps(a, b) -> int:
        return int(np.abs(a.view(np.int32).astype(np.int64) - b.view(np.int32)).max())

    x = torch.rand(PROBE_SIZE, generator=torch.Generator().manual_seed(0)) * 10 + 1e-5
    xn = x.numpy()
    report = {"torch": torch.__version__, "numpy": np.__version__,
              "cpu_capability": torch.backends.cpu.get_cpu_capability(),
              "threads": torch.get_num_threads(), "input_sha256": short(xn)}
    for name, op, exact in (("sqrt", torch.sqrt, np.sqrt), ("exp", torch.exp, np.exp)):
        got = op(x).numpy()
        want = exact(xn.astype(np.float64)).astype(np.float32)
        report[name] = {"sha256_12": short(got), "differ_from_correctly_rounded": int((got != want).sum()),
                        "max_ulp": ulps(got, want)}
    report["sqrt"]["path_seen_on_2026_11_15"] = kernel_path(report["sqrt"]["sha256_12"])
    return report


def fingerprint(model: Path) -> dict:
    """The order-aware fingerprint of an ONNX file: hashes and shapes only."""
    import onnx  # the export environment only

    loaded = onnx.load(str(model))
    graph = loaded.graph
    initializers = []
    for tensor in graph.initializer:
        raw = tensor.raw_data if tensor.raw_data else tensor.SerializeToString()
        initializers.append([tensor.name, tensor.data_type, list(tensor.dims), _sha256(raw)])
    nodes = [_sha256(node.SerializeToString()) for node in graph.node]
    with model.open("rb") as handle:
        file_digest = hashlib.file_digest(handle, "sha256").hexdigest()
    return {
        "format": FORMAT,
        "file_sha256": file_digest,
        "header": {"ir_version": loaded.ir_version,
                   "producer": [loaded.producer_name, loaded.producer_version],
                   "opsets": [[o.domain, o.version] for o in loaded.opset_import]},
        "nodes": {"count": len(nodes), "order_sha256": _sha256(json.dumps(nodes).encode())},
        "initializers": initializers,
    }


def compare(a: dict, b: dict) -> dict:
    """Whether two fingerprints are the same file, the same graph, and which initializers differ."""
    for side in (a, b):
        if side.get("format") != FORMAT:
            raise ValueError(f"not a {FORMAT} fingerprint")
    layout = [[name, dtype, dims] for name, dtype, dims, _ in a["initializers"]]
    same_structure = (a["header"] == b["header"] and a["nodes"] == b["nodes"]
                      and layout == [[name, dtype, dims] for name, dtype, dims, _ in b["initializers"]])
    values_b = {name: digest for name, _, _, digest in b["initializers"]}
    differing = [name for name, _, _, digest in a["initializers"]
                 if name in values_b and values_b[name] != digest]
    return {"file_equal": a["file_sha256"] == b["file_sha256"], "same_structure": same_structure,
            "initializers": len(a["initializers"]), "differing_initializer_count": len(differing),
            "differing_initializers": differing}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("kernels", help="torch's float32 sqrt and exp against the correctly rounded result")
    make = sub.add_parser("fingerprint", help="the hashes of an ONNX file's initializers and nodes")
    make.add_argument("model", type=Path)
    make.add_argument("--out", type=Path, required=True)
    cmp_ = sub.add_parser("compare", help="compare two fingerprints")
    cmp_.add_argument("a", type=Path)
    cmp_.add_argument("b", type=Path)
    cmp_.add_argument("--out", type=Path, help="write the JSON here instead of stdout")
    args = parser.parse_args(argv)
    if args.command == "kernels":
        result = kernels()
    elif args.command == "fingerprint":
        args.out.write_text(json.dumps(fingerprint(args.model)) + "\n", encoding="utf-8")
        return 0
    else:
        result = compare(json.loads(args.a.read_text(encoding="utf-8")),
                         json.loads(args.b.read_text(encoding="utf-8")))
    text = json.dumps(result, indent=2) + "\n"
    if getattr(args, "out", None):
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
