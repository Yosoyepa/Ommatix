"""Offline checks of the READMEs, the guides and their media. Standard library only.

Links and anchors, SVG safety, the manifest (digests, sizes, nothing unlisted), the rules of an
animated SVG, the four variants of every picture, alt text, twins that agree, and the verbatim
contract examples. It reads the repository and writes nothing.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
import unicodedata
from pathlib import Path
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "docs/assets/readme"

# Bytes each kind of asset may weigh, and all raster files together.
BUDGET = {"svg": 16_000, "animated-svg": 48_000, "gif": 600_000, "png": 200_000}
RASTER_TOTAL = 1_000_000
LONGEST_LOOP_S = 30
# The order GitHub must try the sources of a picture in: phone first, dark before light.
MEDIA = ["(max-width: 640px) and (prefers-color-scheme: dark)", "(max-width: 640px)", "(prefers-color-scheme: dark)"]
SUFFIXES = ["mobile-dark.svg", "mobile-light.svg", "dark.svg", "light.svg"]
TWINS = [("README.md", "README.es.md")] + [
    (f"docs/guide/{name}.md", f"docs/guide/{name}.es.md")
    for name in ("getting-started", "configuration", "live-input", "development")
] + [("docs/architecture/overview.md", "docs/architecture/overview.es.md")]


def prose(text: str) -> str:
    """Skip fenced examples, which are data rather than document links/headings."""
    return re.sub(r"(?ms)^(`{3,}|~{3,})[^\n]*\n.*?^\1\s*$", "", text)


def anchors(text: str) -> set[str]:
    result = set(re.findall(r'\bid=["\']([^"\']+)', text))
    seen: dict[str, int] = {}
    for heading in re.findall(r"(?m)^#{1,6}\s+(.+?)\s*#*\s*$", prose(text)):
        heading = re.sub(r"<[^>]*>", "", heading)
        heading = re.sub(r"\[([^]]+)\]\([^)]+\)", r"\1", heading)
        slug = re.sub(r"[^\w\- ]", "", unicodedata.normalize("NFC", heading.lower())).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        result.add(slug + (f"-{count}" if count else ""))
    return result


def destinations(text: str) -> list[str]:
    text = prose(text)
    values = re.findall(r"\]\((<[^>]+>|[^\s)]+)(?:\s+[^)]*)?\)", text)
    values += re.findall(r'\b(?:href|src)=["\']([^"\']+)', text)
    for srcset in re.findall(r'\bsrcset=["\']([^"\']+)', text):
        values += [part.strip().split()[0] for part in srcset.split(",")]
    return [v.strip("<>") for v in values]


def check_links(file: Path) -> list[str]:
    errors = []
    for destination in destinations(file.read_text(encoding="utf-8")):
        url = urlsplit(destination)
        if url.scheme or url.netloc:
            continue
        target = (file.parent / unquote(url.path)).resolve() if url.path else file
        if not target.exists():
            errors.append(f"{file.relative_to(ROOT)}: missing {destination}")
        elif url.fragment and target.suffix in {".md", ".markdown"}:
            if unquote(url.fragment) not in anchors(target.read_text(encoding="utf-8")):
                errors.append(f"{file.relative_to(ROOT)}: missing anchor {destination}")
    return errors


def check_svg(file: Path) -> list[str]:
    errors = []
    document = ET.fromstring(file.read_text(encoding="utf-8"))
    if document.attrib.get("role") != "img":
        errors.append(f"{file.name}: missing image role")
    names = {element.tag.rsplit("}", 1)[-1] for element in document.iter()}
    if not {"title", "desc"}.issubset(names):
        errors.append(f"{file.name}: missing title or description")
    for element in document.iter():
        name = element.tag.rsplit("}", 1)[-1]
        if name in {"script", "foreignObject", "image"}:
            errors.append(f"{file.name}: disallowed {name}")
        for attribute, value in element.attrib.items():
            if attribute.rsplit("}", 1)[-1] == "href" and not value.startswith("#"):
                errors.append(f"{file.name}: external href")
            if attribute.lower().startswith("on"):
                errors.append(f"{file.name}: event handler")
    if re.search(r"(?:@import|url\(\s*['\"]?(?:https?:|//))", file.read_text(encoding="utf-8"), re.I):
        errors.append(f"{file.name}: external CSS")
    return errors


def check_animation(file: Path) -> list[str]:
    """An animated SVG must stay honest and calm: labelled as simulated, a still for reduced motion,
    CSS only, and a loop that ends."""
    text = file.read_text(encoding="utf-8")
    errors = []
    if re.search(r"<(?:animate|animateTransform|animateMotion|set)\b", text):
        errors.append(f"{file.name}: SMIL animation; use the CSS timeline")
    style = "".join(re.findall(r"(?s)<style>(.*?)</style>", text))
    guard = "@media (prefers-reduced-motion:no-preference){"
    start = style.find(guard)
    outside = style if start < 0 else style[:start]
    if start < 0:
        errors.append(f"{file.name}: animations are not inside a reduced-motion guard")
    if re.search(r"animation\s*:", re.sub(r"(?s)@keyframes \w+\{(?:[^{}]*\{[^{}]*\})*\}", "", outside)):
        errors.append(f"{file.name}: an animation runs outside the reduced-motion guard")
    names = set(re.findall(r"@keyframes (\w+)", style))
    played = re.findall(r"\.(\w+)\{animation:(\w+) ([\d.]+)s linear infinite\}", style)
    if not names or {name for _, name, _ in played} != names or any(cls != name for cls, name, _ in played):
        errors.append(f"{file.name}: keyframes and the classes that play them differ")
    if any(float(seconds) > LONGEST_LOOP_S for _, _, seconds in played):
        errors.append(f"{file.name}: a loop longer than {LONGEST_LOOP_S} s")
    if len({seconds for _, _, seconds in played}) > 1:
        errors.append(f"{file.name}: animations of different lengths drift apart")
    for name in sorted(names):
        for group in re.findall(rf'<g class="{name}"([^>]*)>', text):
            if not re.search(r'\b(?:opacity|transform)="', group):
                errors.append(f"{file.name}: a group animated by {name} has no value for the still")
                break
        if f'class="{name}"' not in text:
            errors.append(f"{file.name}: keyframes {name} are never used")
    if "Simulated desktop" not in re.sub(r"(?s)<(?:title|desc)[^>]*>.*?</(?:title|desc)>", "", text):
        errors.append(f"{file.name}: the picture does not say it is a simulated desktop")
    return errors


def check_manifest() -> tuple[list[str], dict, int]:
    errors = []
    manifest = json.loads((ASSETS / "manifest.json").read_text(encoding="utf-8"))
    assets = manifest.get("assets", {})
    if manifest.get("schema") != 2:
        errors.append("manifest.json: schema is not 2")
    on_disk = {file.name for file in ASSETS.iterdir() if file.name != "manifest.json"}
    for name in sorted(on_disk - set(assets)):
        errors.append(f"{name}: in docs/assets/readme but not in the manifest")
    raster = 0
    for name, expected in assets.items():
        if name not in on_disk:
            errors.append(f"{name}: in the manifest but missing")
            continue
        data = (ASSETS / name).read_bytes()
        if len(data) != expected["bytes"] or hashlib.sha256(data).hexdigest() != expected["sha256"]:
            errors.append(f"{name}: digest or size differs from the manifest")
        kind = expected.get("kind")
        if kind not in BUDGET:
            errors.append(f"{name}: unknown kind {kind!r}")
            continue
        if kind != {".svg": "svg", ".gif": "gif", ".png": "png"}.get(Path(name).suffix) and kind != "animated-svg":
            errors.append(f"{name}: kind {kind} does not match the file")
        if len(data) > BUDGET[kind]:
            errors.append(f"{name}: {len(data)} bytes exceeds the {BUDGET[kind]} allowed for a {kind}")
        if expected.get("reproducible") != ("renderer" if kind in ("gif", "png") else "bytes"):
            errors.append(f"{name}: wrong reproducibility class")
        if kind in ("gif", "png"):
            raster += len(data)
        if kind == "animated-svg":
            errors.extend(check_animation(ASSETS / name))
        elif kind == "svg" and "@keyframes" in data.decode("utf-8"):
            errors.append(f"{name}: animated, but listed as a still")
    if raster > RASTER_TOTAL:
        errors.append(f"raster files total {raster} bytes, more than {RASTER_TOTAL}")
    for name in assets:
        if name.endswith("-light.svg") and "mobile" not in name:
            family = name[: -len("light.svg")]
            for suffix in SUFFIXES:
                if family + suffix not in assets:
                    errors.append(f"{family}{suffix}: missing variant")
    return errors, manifest, raster


def check_pictures(file: Path) -> list[str]:
    """Every picture offers the four variants in the order that works, and every image has alt text."""
    text = prose(file.read_text(encoding="utf-8"))
    errors = []
    for picture in re.findall(r"(?s)<picture>(.*?)</picture>", text):
        sources = re.findall(r'<source media="([^"]+)" srcset="([^"]+)">', picture)
        image = re.search(r'<img src="([^"]+)"', picture)
        files = [srcset for _, srcset in sources] + ([image.group(1)] if image else [])
        family = files[-1][: -len("light.svg")] if files and files[-1].endswith("light.svg") else None
        if [media for media, _ in sources] != MEDIA or family is None or files != [family + s for s in SUFFIXES]:
            errors.append(f"{file.relative_to(ROOT)}: a picture does not list its four variants in order: {files}")
    for tag in re.findall(r"<img\b[^>]*>", text):
        if not re.search(r'\balt="[^"]{12,}"', tag):
            errors.append(f"{file.relative_to(ROOT)}: an image without a real alt text: {tag[:80]}")
    for alt, target in re.findall(r"!\[([^\]]*)\]\(([^)\s]+)", text):
        if not alt.strip():
            errors.append(f"{file.relative_to(ROOT)}: no alt text for {target}")
    return errors


def shape(text: str) -> dict:
    """What two language twins must share: structure and every destination, not the words."""
    body = prose(text)

    def target(value: str) -> str:
        url = urlsplit(value)
        return (url.netloc + url.path if url.scheme else url.path).replace(".es.md", ".md")

    def rows(match: re.Match) -> int:
        return len([line for line in match.group(0).strip().splitlines() if not re.fullmatch(r"\|[\s:|-]+\|?", line.strip())])

    return {
        "headings": [len(marks) for marks in re.findall(r"(?m)^(#{1,6})\s", body)],
        "destinations": sorted(target(value) for value in destinations(text) if target(value)),
        "images": [Path(value).name for value in re.findall(r'<img src="([^"]+)"', body)],
        "tables": [rows(match) for match in re.finditer(r"(?m)(?:^\|.*\n)+", body + "\n")],
        "fences": re.findall(r"(?m)^(?:`{3,}|~{3,})(\w+)", text),
        "alerts": re.findall(r"(?m)^> \[!(\w+)\]", body),
        "details": body.count("<details>"),
    }


def check_twins(first: Path, second: Path) -> list[str]:
    a, b = shape(first.read_text(encoding="utf-8")), shape(second.read_text(encoding="utf-8"))
    return [f"{first.relative_to(ROOT)} and {second.name} differ in {key}" +
            (f": {sorted(set(map(str, a[key])) ^ set(map(str, b[key])))[:6]}" if key == "destinations" else f": {a[key]} / {b[key]}")
            for key in a if a[key] != b[key]]


def words(text: str) -> tuple[int, int]:
    """Words a reader sees, and words of alt text. Markup, link targets and attributes are not words."""
    alt = " ".join(re.findall(r'\balt="([^"]*)"', text) + re.findall(r"!\[([^\]]*)\]\(", text))
    seen = re.sub(r"(?s)<!--.*?-->", " ", text)
    seen = re.sub(r"!\[[^\]]*\]\([^)]*\)", " ", seen)
    seen = re.sub(r"\]\([^)]*\)", "] ", seen)
    seen = re.sub(r"<[^>]*>", " ", seen)
    seen = re.sub(r"(?m)^\|[\s:|-]+\|?$", " ", seen)
    count = len(re.findall(r"[\w][\w./:'’%+=-]*", seen))
    return count, len(alt.split())


def main() -> int:
    docs = [ROOT / "README.md", ROOT / "README.es.md", ROOT / "docs/README.md"]
    docs += sorted((ROOT / "docs/guide").glob("*.md"))
    docs += sorted((ROOT / "docs/architecture").glob("*.md"))
    docs += sorted((ROOT / "docs/diagrams").glob("*.md"))
    docs += sorted((ROOT / ".github").glob("*.md"))
    docs += [ROOT / "tools/readme-media/README.md"]
    errors = []
    for file in docs:
        errors.extend(check_links(file))
        errors.extend(check_pictures(file))
    svgs = sorted(ASSETS.glob("*.svg")) + [ROOT / "tools/readme-media/social-preview.svg"]
    for file in svgs:
        errors.extend(check_svg(file))
    manifest_errors, manifest, raster = check_manifest()
    errors.extend(manifest_errors)
    for first, second in TWINS:
        errors.extend(check_twins(ROOT / first, ROOT / second))
    contract = ROOT / "docs/thesis/contratos/ejemplos"
    for file in docs[:2]:
        examples = [json.loads(block) for block in re.findall(r"```json\n(.*?)\n```", file.read_text(encoding="utf-8"), re.S)]
        for name in ("click_input", "simulated_output"):
            if json.loads((contract / (name + ".json")).read_text(encoding="utf-8")) not in examples:
                errors.append(f"{file.name}: the {name} example differs from the dossier")
        used = set(re.findall(r"docs/assets/readme/([\w.-]+)", file.read_text(encoding="utf-8")))
        for name in sorted(set(manifest["assets"]) - used - {"social-preview.png"}):
            errors.append(f"{file.name}: {name} is in the manifest but the README never shows or links it")
    print(f"Checked {len(docs)} documents, {len(svgs)} SVGs, {len(manifest['assets'])} asset digests and "
          f"{len(TWINS)} pairs of twins; raster files {raster} bytes.")
    for file in docs[:2]:
        seen, alt = words(file.read_text(encoding="utf-8"))
        print(f"{file.name}: {seen} words a reader sees, {alt} words of alt text.")
    for error in errors:
        print(error, file=sys.stderr)
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
