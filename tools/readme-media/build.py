"""Writes every README image and the manifest that lists them.

    python tools/readme-media/build.py            # the SVG files and the manifest (standard library only)
    python tools/readme-media/build.py --check    # regenerate in memory and fail if a committed file differs
    python tools/readme-media/build.py --raster   # also the two raster files (see raster.py and requirements.txt)

The SVG files are a pure function of this directory, three contract examples and the pinned
toolchain: the same bytes on any machine. The GIF and the social preview PNG are drawn by a
browser from those SVG files, so their bytes depend on that browser and its fonts; the
manifest says which is which.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

import demo
import figures

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ASSETS = ROOT / "docs/assets/readme"
SOCIAL_SOURCE = HERE / "social-preview.svg"
GIF = {"name": "protocol-demo.gif", "theme": "light", "layout": "desktop", "fps": 10, "colours": 128}
SOCIAL = "social-preview.png"
# Where the facts in the figures were read, and the commit they were checked against.
INPUTS = {
    "contract": "docs/thesis/contratos/ejemplos",
    "results": "docs/thesis/09_EVALUACION_EXPERIMENTAL.md#911",
    "gate": "AGENTS.md#5 rule 6, src/application/src/engine/act.rs",
    "toolchain": "rust-toolchain.toml",
    "verified_against": "030a3e42e37c36b7243116bff97b94c5e2450c75",
}


def vector_assets() -> dict[str, tuple[str, str]]:
    """Every SVG as `name: (kind, text)`, in the order they are written."""
    rust = re.search(r'channel = "([^"]+)"', (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")).group(1)
    out: dict[str, tuple[str, str]] = {}
    for key, (label, value, role) in figures.BADGES.items():
        out[f"badge-{key}.svg"] = ("svg", figures.badge(label, value.format(rust=rust), role))
    for name in ("hero", "pipeline", "gate", "results"):
        for theme, layout in figures.VARIANTS:
            out[f"{name}-{figures.suffix(theme, layout)}"] = ("svg", getattr(figures, name)(theme, layout))
    for theme, layout in figures.VARIANTS:
        out[f"protocol-demo-{figures.suffix(theme, layout)}"] = ("animated-svg", demo.demo(theme, layout, ROOT))
    return out


def digest(data: bytes) -> dict:
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def manifest(vectors: dict[str, tuple[str, str]], previous: dict) -> dict:
    assets = {name: {"kind": kind, "reproducible": "bytes", **digest(text.encode("utf-8"))}
              for name, (kind, text) in vectors.items()}
    for name, kind in ((GIF["name"], "gif"), (SOCIAL, "png")):
        if (ASSETS / name).is_file():
            assets[name] = {"kind": kind, "reproducible": "renderer", **digest((ASSETS / name).read_bytes())}
    return {
        "schema": 2,
        "generator": "tools/readme-media/build.py",
        "inputs": INPUTS,
        "demo": {"duration_s": demo.DURATION, "poster_s": demo.POSTER, "beats": [beat[2] for beat in demo.BEATS],
                 "gif": {key: GIF[key] for key in ("theme", "layout", "fps", "colours")}},
        "raster": previous.get("raster", {}),
        "assets": dict(sorted(assets.items())),
    }


def dump(data: dict) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="compare with the committed files; write nothing")
    parser.add_argument("--raster", action="store_true", help="also render the GIF and the social preview PNG")
    args = parser.parse_args()
    vectors = vector_assets()
    social = figures.social()
    path = ASSETS / "manifest.json"
    previous = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    if args.check:
        stale = [name for name, (_, text) in vectors.items()
                 if not (ASSETS / name).is_file() or (ASSETS / name).read_bytes() != text.encode("utf-8")]
        if not SOCIAL_SOURCE.is_file() or SOCIAL_SOURCE.read_bytes() != social.encode("utf-8"):
            stale.append(SOCIAL_SOURCE.name)
        if not path.is_file() or path.read_text(encoding="utf-8") != dump(manifest(vectors, previous)):
            stale.append(path.name)
        extra = sorted(file.name for file in ASSETS.glob("*.svg") if file.name not in vectors)
        for name in stale:
            print(f"{name}: differs from what the generator writes; run build.py", file=sys.stderr)
        for name in extra:
            print(f"{name}: not written by the generator", file=sys.stderr)
        print(f"Regenerated {len(vectors) + 1} SVG files in memory; {len(stale)} differ, {len(extra)} unknown.")
        return 1 if stale or extra else 0
    ASSETS.mkdir(parents=True, exist_ok=True)
    for name, (_, text) in vectors.items():
        (ASSETS / name).write_bytes(text.encode("utf-8"))
    SOCIAL_SOURCE.write_bytes(social.encode("utf-8"))
    if args.raster:
        import raster
        previous["raster"] = raster.render(ASSETS, SOCIAL_SOURCE, GIF, SOCIAL)
    path.write_bytes(dump(manifest(vectors, previous)).encode("utf-8"))
    binary = sum(entry["bytes"] for entry in manifest(vectors, previous)["assets"].values() if entry["kind"] in ("gif", "png"))
    print(f"Wrote {len(vectors)} SVG files, {SOCIAL_SOURCE.name} and the manifest; raster files total {binary} bytes.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
