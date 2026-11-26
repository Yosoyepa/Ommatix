"""Look at the result before committing it: pages as GitHub lays them out, and the demo's frames.

    python tools/readme-media/preview.py pages  [FILE.md ...]   # light, dark, desktop and phone
    python tools/readme-media/preview.py frames                 # contact sheets of the demo
    python tools/readme-media/preview.py motion                 # the browser's animation against the generator's frames

Everything lands in `tools/readme-media/.qa/` (ignored by git). The page view is a local
approximation of GitHub's Markdown styles, rendered with GitHub's own parser; it calls no online
service, and the remote CI badge is replaced by a placeholder. A page is laid out inside a frame
of the requested width, because a browser window cannot be as narrow as a phone.

Needs requirements.txt and a Chromium browser (see raster.py).
"""
from __future__ import annotations

import argparse
import html
import json
import re
import sys
from pathlib import Path

import demo
import figures
from raster import frames, screenshot

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
QA = HERE / ".qa"
WIDTHS = {"desktop": 1280, "mobile": 390}

LIGHT = dict(bg="#ffffff", fg="#1f2328", muted="#59636e", border="#d1d9e0", soft="#d1d9e0b3", subtle="#f6f8fa",
             code="#818b981f", link="#0969da", note="#0969da", tip="#1a7f37", important="#8250df",
             warning="#9a6700", caution="#cf222e")
DARK = dict(bg="#0d1117", fg="#f0f6fc", muted="#9198a1", border="#3d444d", soft="#3d444db3", subtle="#151b23",
            code="#656c7633", link="#4493f8", note="#4493f8", tip="#3fb950", important="#ab7df8",
            warning="#d29922", caution="#f85149")

CSS = """
*{box-sizing:border-box}html,body{margin:0;padding:0}
body{background:%(bg)s;color:%(fg)s;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI","Noto Sans",Helvetica,Arial,sans-serif,"Apple Color Emoji","Segoe UI Emoji";font-size:16px;line-height:1.5;word-wrap:break-word}
.frame{width:896px;margin:24px auto;border:1px solid %(border)s;border-radius:6px}
.bar{height:48px;border-bottom:1px solid %(border)s;display:flex;align-items:center;padding:0 16px;font-size:14px;font-weight:600;gap:8px}
.bar span{padding:12px 8px;border-bottom:2px solid #fd8c73}.bar i{font-style:normal;color:%(muted)s;font-weight:400;padding:12px 8px}
.md{padding:32px}
@media(max-width:767px){.frame{width:auto;margin:0;border-left:0;border-right:0;border-radius:0}.md{padding:16px}}
.md>*:first-child{margin-top:0!important}.md>*:last-child{margin-bottom:0!important}
.md a{color:%(link)s;text-decoration:underline;text-underline-offset:.2rem}
.md p,.md blockquote,.md ul,.md ol,.md dl,.md table,.md pre,.md details{margin-top:0;margin-bottom:16px}
.md h1,.md h2,.md h3,.md h4{margin-top:24px;margin-bottom:16px;font-weight:600;line-height:1.25}
.md h1{font-size:2em;padding-bottom:.3em;border-bottom:1px solid %(soft)s}
.md h2{font-size:1.5em;padding-bottom:.3em;border-bottom:1px solid %(soft)s}.md h3{font-size:1.25em}
.md ul,.md ol{padding-left:2em}.md li+li{margin-top:.25em}
.md img{max-width:100%%;box-sizing:content-box;border-style:none}.md picture{display:inline}
.md code,.md tt{padding:.2em .4em;margin:0;font-size:85%%;white-space:break-spaces;background:%(code)s;border-radius:6px;font-family:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,"Liberation Mono",monospace}
.md pre{padding:16px;overflow:auto;font-size:85%%;line-height:1.45;background:%(subtle)s;border-radius:6px}
.md pre code{padding:0;background:transparent;white-space:pre;font-size:100%%}
.md table{border-spacing:0;border-collapse:collapse;display:block;width:max-content;max-width:100%%;overflow:auto}
.md th,.md td{padding:6px 13px;border:1px solid %(border)s}.md th{font-weight:600}
.md tr{background:%(bg)s;border-top:1px solid %(soft)s}.md tr:nth-child(2n){background:%(subtle)s}
.md blockquote{padding:0 1em;color:%(muted)s;border-left:.25em solid %(border)s;margin-left:0;margin-right:0}
.md blockquote>:last-child{margin-bottom:0}
.md .alert{padding:.5rem 1rem;margin-bottom:1rem;color:inherit;border-left:.25em solid %(border)s}
.md .alert .t{display:flex;font-weight:500;align-items:center;line-height:1;margin-bottom:16px}
.md .alert .t::before{content:"";width:16px;height:16px;border-radius:50%%;border:2px solid currentColor;margin-right:.5rem}
.md .alert.note{border-left-color:%(note)s}.md .alert.note .t{color:%(note)s}
.md .alert.tip{border-left-color:%(tip)s}.md .alert.tip .t{color:%(tip)s}
.md .alert.important{border-left-color:%(important)s}.md .alert.important .t{color:%(important)s}
.md .alert.warning{border-left-color:%(warning)s}.md .alert.warning .t{color:%(warning)s}
.md .alert.caution{border-left-color:%(caution)s}.md .alert.caution .t{color:%(caution)s}
.md summary{cursor:pointer}.md sub{font-size:75%%;line-height:0;position:relative;vertical-align:baseline;bottom:-.25em}
.md hr{height:.25em;padding:0;margin:24px 0;background:%(border)s;border:0}
.md .mermaid{border:1px dashed %(border)s;border-radius:6px;padding:16px;color:%(muted)s;font-size:13px;white-space:pre-wrap;font-family:ui-monospace,Consolas,monospace}
.ci{display:inline-block;height:20px;border-radius:3px;background:#555;color:#fff;font-size:11px;line-height:20px;padding:0 6px;vertical-align:middle}
"""
REPORT = ("<script>addEventListener('load',()=>{const top=+(location.hash.match(/top=(\\d+)/)||[0,0])[1];"
          "document.body.style.marginTop=(-top)+'px';parent.postMessage(JSON.stringify({"
          "overflow:document.documentElement.scrollWidth>innerWidth,width:innerWidth,"
          "height:document.documentElement.scrollHeight+top,"
          "broken:[...document.images].filter(i=>!i.complete||i.naturalWidth===0).map(i=>i.currentSrc||i.src),"
          "chosen:[...document.images].map(i=>(i.currentSrc||i.src).split('/').pop())}),'*')})</script>")


def page(markdown: Path, theme: str) -> str:
    """The Markdown file as an HTML page in GitHub's proportions."""
    import cmarkgfm
    from cmarkgfm.cmark import Options
    text = markdown.read_text(encoding="utf-8")
    text = re.sub(r"\[!\[[^\]]*\]\(https://github\.com/[^)]*badge\.svg[^)]*\)\]\([^)]*\)",
                  '<span class="ci">CI · offline preview</span>', text)
    text = re.sub(r"(?ms)^```mermaid\n(.*?)^```\s*$",
                  lambda m: '<div class="mermaid">[Mermaid diagram, drawn by GitHub]\n' + html.escape(m.group(1)) + "</div>\n", text)
    body = cmarkgfm.github_flavored_markdown_to_html(text, options=Options.CMARK_OPT_UNSAFE)
    body = re.sub(r"<blockquote>\s*<p>\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*\n?",
                  lambda m: f'<blockquote class="alert {m.group(1).lower()}"><p class="t">{m.group(1).capitalize()}</p><p>', body)
    palette = LIGHT if theme == "light" else DARK
    return (f'<!doctype html><html lang="en"><meta charset="utf-8"><base href="{markdown.parent.as_uri()}/">'
            f'<style>{CSS % palette}</style><body><div class="frame"><div class="bar"><span>README</span><i>License</i></div>'
            f'<article class="md">{body}</article></div>{REPORT}</body></html>')


def framed(inner: Path, width: int, height: int, top: int, theme: str) -> str:
    background = (LIGHT if theme == "light" else DARK)["bg"]
    return (f'<!doctype html><meta charset="utf-8"><body style="margin:0;background:{background}">'
            f'<iframe src="{inner.as_uri()}#top={top}" style="border:0;display:block;width:{width}px;height:{height}px"></iframe>'
            "<script>addEventListener('message',e=>{const q=document.createElement('pre');q.id='qa';q.textContent=e.data;"
            "document.body.appendChild(q)})</script></body>")


def pages(files: list[str], tile: int) -> int:
    from PIL import Image
    QA.mkdir(exist_ok=True)
    findings, failed = [], False
    for relative in files:
        markdown = (ROOT / relative).resolve()
        for theme in ("light", "dark"):
            for layout, width in WIDTHS.items():
                name = f"{markdown.relative_to(ROOT).as_posix().replace('/', '-').removesuffix('.md')}-{theme}-{layout}"
                inner, outer = QA / f"{name}.html", QA / f"{name}-frame.html"
                inner.write_text(page(markdown, theme), encoding="utf-8")
                outer.write_text(framed(inner, width, 900, 0, theme), encoding="utf-8")
                dom = screenshot(outer.as_uri(), QA / "unused.png", width, 900, dark=theme == "dark", dump_dom=True)
                found = re.search(r'<pre id="qa"[^>]*>(.*?)</pre>', dom, re.S)
                info = json.loads(html.unescape(found.group(1))) if found else {"missing": True}
                findings.append({"page": name, **info})
                failed |= bool(info.get("overflow") or info.get("broken") or info.get("missing"))
                top = index = 0
                while top < info.get("height", 0):
                    height = min(tile, info["height"] - top)
                    outer.write_text(framed(inner, width, height, top, theme), encoding="utf-8")
                    shot = QA / f"{name}-{index:02d}.png"
                    screenshot(outer.as_uri(), shot, width, height, dark=theme == "dark", reduced_motion=True)
                    with Image.open(shot) as image:
                        image.crop((0, 0, width, height)).save(shot)
                    top, index = top + height, index + 1
                outer.unlink()
    (QA / "findings.json").write_text(json.dumps(findings, indent=2) + "\n", encoding="utf-8")
    for item in findings:
        print(item["page"], "overflow" if item.get("overflow") else "fits", "broken:", item.get("broken"), "height:", item.get("height"))
    return 1 if failed else 0


def sheets(step: float) -> int:
    """Every `step` seconds of the demo, in each of its four variants, as one image per variant."""
    import tempfile
    from PIL import Image, ImageDraw
    QA.mkdir(exist_ok=True)
    times = [round(index * step, 2) for index in range(int(demo.DURATION / step))] + [demo.POSTER]
    for theme, layout in figures.VARIANTS:
        svgs = [demo.demo(theme, layout, ROOT, at=time) for time in times]
        width, height = (int(float(v)) for v in svgs[0].split('viewBox="0 0 ')[1].split('"')[0].split())
        with tempfile.TemporaryDirectory(prefix="readme-media-") as directory:
            images = frames(svgs, width, height, Path(directory), per_page=12 if layout == "mobile" else 20)
        columns = 6 if layout == "mobile" else 3
        rows = (len(images) + columns - 1) // columns
        sheet = Image.new("RGB", (columns * (width + 10) + 10, rows * (height + 26) + 10), "#808080")
        draw = ImageDraw.Draw(sheet)
        for index, (time, image) in enumerate(zip(times, images)):
            x, y = 10 + (index % columns) * (width + 10), 10 + (index // columns) * (height + 26)
            draw.text((x, y), f"{time:.1f} s" + ("  (poster)" if index == len(times) - 1 else ""), fill="#ffffff")
            sheet.paste(image, (x, y + 16))
        target = QA / f"frames-{figures.suffix(theme, layout).removesuffix('.svg')}.png"
        sheet.save(target)
        print(target.relative_to(ROOT).as_posix(), f"{len(images)} frames")
    return 0


def motion(times: list[float], limit: int) -> int:
    """Pauses the CSS animation in the browser at each time and compares it with the computed frame."""
    import tempfile
    from PIL import Image, ImageChops
    QA.mkdir(exist_ok=True)
    worst = 0
    for theme, layout in figures.VARIANTS:
        animated = demo.demo(theme, layout, ROOT)
        width, height = (int(float(v)) for v in animated.split('viewBox="0 0 ')[1].split('"')[0].split())
        with tempfile.TemporaryDirectory(prefix="readme-media-") as directory:
            work = Path(directory)
            paused = [animated.replace("</style>", f"*{{animation-play-state:paused!important;animation-delay:-{time}s!important}}</style>", 1)
                      for time in times]
            played = frames(paused, width, height, work, per_page=8)
            computed = frames([demo.demo(theme, layout, ROOT, at=time) for time in times], width, height, work, per_page=8)
            (work / "still.svg").write_text(animated, encoding="utf-8", newline="\n")
            (work / "still.html").write_text(f'<!doctype html><meta charset="utf-8"><body style="margin:0;background:#fff">'
                                             f'<img src="still.svg" width="{width}" height="{height}" style="display:block"></body>', encoding="utf-8")
            screenshot((work / "still.html").as_uri(), work / "still.png", width, height, reduced_motion=True)
            with Image.open(work / "still.png") as image:
                reduced = image.convert("RGB").crop((0, 0, width, height))
            poster = frames([demo.demo(theme, layout, ROOT, at=demo.POSTER)], width, height, work)[0]
        for time, a, b in list(zip(times, played, computed)) + [("reduced motion", reduced, poster)]:
            difference = ImageChops.difference(a, b).convert("L").point(lambda value: 255 if value > 24 else 0)
            count = sum(difference.histogram()[255:])
            worst = max(worst, count)
            print(f"{theme:5} {layout:7} {str(time):>14}: {count} pixels differ")
    print(f"Worst case: {worst} pixels; the limit is {limit}.")
    return 1 if worst > limit else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    one = commands.add_parser("pages")
    one.add_argument("files", nargs="*", default=["README.md", "README.es.md"])
    one.add_argument("--tile", type=int, default=1200, help="height of each screenshot")
    two = commands.add_parser("frames")
    two.add_argument("--step", type=float, default=1.0)
    three = commands.add_parser("motion")
    three.add_argument("--times", type=float, nargs="*", default=[0.5, 2.3, 5.2, 9.7, 13.4, 15.0, 15.8, 19.9, 20.7, 23.8])
    three.add_argument("--limit", type=int, default=150, help="pixels that may differ: antialiasing at a moving edge")
    args = parser.parse_args()
    if args.command == "pages":
        return pages(args.files, args.tile)
    if args.command == "frames":
        return sheets(args.step)
    return motion(args.times, args.limit)


if __name__ == "__main__":
    sys.exit(main())
