"""The two raster files: the GIF of the demo and the social preview PNG.

A headless Chromium browser (Edge or Chrome) draws SVG files this directory generated; Pillow
slices, quantises and encodes. The browser reads local files only: its DNS is mapped to
nowhere and it gets a throwaway profile. Nothing on the screen is captured; the frames are
the generator's own, one per tenth of a second.

Needs `pip install -r requirements.txt` in a virtual environment and a browser; set
README_MEDIA_BROWSER to its executable if it is not found.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

CANDIDATES = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
]
NAMES = ["chromium", "chromium-browser", "google-chrome", "chrome", "microsoft-edge", "msedge"]


def browser() -> str:
    chosen = os.environ.get("README_MEDIA_BROWSER")
    if chosen:
        return chosen
    for candidate in CANDIDATES:
        if Path(candidate).is_file():
            return candidate
    for name in NAMES:
        found = shutil.which(name)
        if found:
            return found
    raise SystemExit("No Chromium browser found. Set README_MEDIA_BROWSER to an Edge or Chrome executable.")


def browser_version() -> str:
    """The product version, read without starting the browser where the platform allows it."""
    exe = browser()
    if os.name == "nt":
        script = f"(Get-Item -LiteralPath '{exe}').VersionInfo.ProductVersion"
        done = subprocess.run(["powershell", "-NoProfile", "-Command", script], capture_output=True, text=True, timeout=60)
        return f"{Path(exe).stem} {done.stdout.strip()}"
    done = subprocess.run([exe, "--version"], capture_output=True, text=True, timeout=60)
    return done.stdout.strip()


def screenshot(url: str, png: Path, width: int, height: int, scale: float = 1.0, dark: bool = False,
               reduced_motion: bool = False, dump_dom: bool = False) -> str:
    """One headless run: a PNG of the page, or its DOM. The window is never shown."""
    profile = Path(tempfile.gettempdir()) / "readme-media-browser-profile"
    args = [browser(), "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run",
            "--no-default-browser-check", "--disable-extensions", "--disable-sync", "--disable-background-networking",
            "--disable-component-update", "--allow-file-access-from-files",
            "--disable-features=Translate,msEdgeSidebar,OptimizationHints,MediaRouter", f"--user-data-dir={profile}",
            "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE localhost", f"--window-size={max(width, 600)},{height}",
            f"--force-device-scale-factor={scale}", "--virtual-time-budget=3000",
            "--blink-settings=preferredColorScheme=" + ("0" if dark else "1")]
    if dark:
        args.append("--force-dark-mode")
    if reduced_motion:
        args.append("--force-prefers-reduced-motion")
    args += ["--dump-dom"] if dump_dom else [f"--screenshot={png}"]
    done = subprocess.run(args + [url], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)
    if not dump_dom and not png.is_file():
        raise SystemExit(f"The browser wrote no screenshot for {url}: {done.stderr[-400:]}")
    return done.stdout


def frames(svgs: list[str], width: int, height: int, work: Path, per_page: int = 20):
    """Draws SVG documents with the browser, a column of them per run, and yields RGB images."""
    from PIL import Image
    unique: dict[str, int] = {}
    order = [unique.setdefault(svg, len(unique)) for svg in svgs]
    drawn: list = [None] * len(unique)
    texts = list(unique)
    for start in range(0, len(texts), per_page):
        chunk = texts[start:start + per_page]
        cells = ""
        for offset, svg in enumerate(chunk):
            file = work / f"frame-{start + offset:04d}.svg"
            file.write_text(svg, encoding="utf-8", newline="\n")
            cells += f'<img src="{file.name}" width="{width}" height="{height}" style="display:block">'
        page = work / f"page-{start:04d}.html"
        page.write_text(f'<!doctype html><meta charset="utf-8"><body style="margin:0;background:#fff">{cells}</body>',
                        encoding="utf-8")
        png = work / f"page-{start:04d}.png"
        screenshot(page.as_uri(), png, width, height * len(chunk))
        with Image.open(png) as sheet:
            sheet = sheet.convert("RGB")
            for offset in range(len(chunk)):
                drawn[start + offset] = sheet.crop((0, offset * height, width, (offset + 1) * height))
    return [drawn[index] for index in order]


def render(assets: Path, social_source: Path, gif: dict, social_name: str) -> dict:
    """Writes the GIF and the PNG. Returns what drew them, for the manifest."""
    import PIL
    from PIL import Image

    import demo
    root = assets.parents[2]
    with tempfile.TemporaryDirectory(prefix="readme-media-") as directory:
        work = Path(directory)
        # the social preview: the SVG at its own size
        page = work / "social.html"
        page.write_text(f'<!doctype html><meta charset="utf-8"><body style="margin:0"><img src="{social_source.as_uri()}" '
                        'width="1280" height="640" style="display:block"></body>', encoding="utf-8")
        png = work / "social.png"
        screenshot(page.as_uri(), png, 1280, 640)
        with Image.open(png) as shot:
            shot.convert("RGB").crop((0, 0, 1280, 640)).save(assets / social_name, optimize=True)
        # the GIF: the generator's frames, one palette for the whole loop, unchanged pixels left transparent
        count = round(demo.DURATION * gif["fps"])
        svgs = [demo.demo(gif["theme"], gif["layout"], root, at=index / gif["fps"]) for index in range(count)]
        width, height = (int(float(v)) for v in svgs[0].split('viewBox="0 0 ')[1].split('"')[0].split())
        images = frames(svgs, width, height, work)
        # one frame per second, so every colour of the loop is in the sample the palette is cut from
        sample = list(range(0, count, gif["fps"]))
        mosaic = Image.new("RGB", (width, height * len(sample)))
        for slot, index in enumerate(sample):
            mosaic.paste(images[index], (0, slot * height))
        palette = mosaic.quantize(colors=gif["colours"], method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
        indexed = [image.quantize(palette=palette, dither=Image.Dither.NONE) for image in images]
        # optimize=True is what makes Pillow store only the pixels that changed since the frame before
        indexed[0].save(assets / gif["name"], save_all=True, append_images=indexed[1:], duration=round(1000 / gif["fps"]),
                        loop=0, optimize=True, disposal=1)
    return {"browser": browser_version(), "pillow": PIL.__version__, "gif_frames": count,
            "gif_size": [width, height]}
