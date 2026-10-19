"""Offline links, SVG safety, protocol excerpts, asset digests and size checks."""
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


def main() -> int:
    docs = [ROOT / "README.md", ROOT / "README.es.md", ROOT / "docs/README.md"]
    docs += sorted((ROOT / "docs/guide").glob("*.md"))
    docs += sorted((ROOT / ".github").glob("*.md"))
    docs += [ROOT / "tools/readme-media/README.md"]
    errors = []
    for file in docs:
        errors.extend(check_links(file))
    svgs = sorted(ASSETS.glob("*.svg")) + [ROOT / "tools/readme-media/social-preview.svg"]
    for file in svgs:
        errors.extend(check_svg(file))
    manifest = json.loads((ASSETS / "manifest.json").read_text(encoding="utf-8"))
    binaries = 0
    for name, expected in manifest["assets"].items():
        data = (ASSETS / name).read_bytes()
        if len(data) != expected["bytes"] or hashlib.sha256(data).hexdigest() != expected["sha256"]:
            errors.append(f"{name}: digest or size differs from the manifest")
        if Path(name).suffix in {".png", ".mp4", ".gif"}:
            binaries += len(data)
            if len(data) > 5_000_000:
                errors.append(f"{name}: binary exceeds 5 MB")
    if binaries > 12_000_000:
        errors.append("binary total exceeds 12 MB")
    contract = ROOT / "docs/thesis/contratos/ejemplos"
    for file in docs[:2]:
        examples = [json.loads(block) for block in re.findall(r"```json\n(.*?)\n```", file.read_text(encoding="utf-8"), re.S)]
        for name in ("click_input", "simulated_output"):
            if json.loads((contract / (name + ".json")).read_text(encoding="utf-8")) not in examples:
                errors.append(f"{file.name}: the {name} example differs from the dossier")
    print(f"Checked {len(docs)} documents, {len(svgs)} SVGs and {len(manifest['assets'])} asset digests; binaries {binaries} bytes.")
    for error in errors:
        print(error, file=sys.stderr)
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
