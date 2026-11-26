# README media

The images of the two repository READMEs, drawn by code. No screen capture, GUI application,
model, input adapter or external account is used. Element labels, identifiers, revisions and
protocol fields are read from `docs/thesis/contratos/ejemplos/*.json`; the rest is a drawing.

## One system

Every picture is built from the same kit ([`kit.py`](kit.py)):

| Part | Rule |
|---|---|
| Size | Drawn at the size GitHub shows it: 840 units for the README column, 360 for a phone. One unit is about one CSS pixel, so no label is set below 12. |
| Variants | Light and dark, desktop and phone, chosen by `<picture>`. Each picture has its own background and border, so it reads on any page. |
| Colour | GitHub's neutrals, and three hues with one meaning each: teal for the engine, iris for the agent, amber for what needs attention (changed, stale, refused, preliminary). |
| Type | GitHub's own sans and monospace stacks. Identifiers and wire names are monospace. |
| Fit | The generator estimates every label's width and refuses to write a label that would overflow its box. |
| Access | Every SVG has the image role, a title and a description; text colours meet 4.5:1 on their backgrounds ([`test_kit.py`](test_kit.py)). |

| File | Draws |
|---|---|
| [`figures.py`](figures.py) | Badges, hero, pipeline, gate, results and the social preview |
| [`demo.py`](demo.py) | The protocol demo, on the timeline of [`timeline.py`](timeline.py) |
| [`build.py`](build.py) | Writes everything to [`docs/assets/readme/`](../../docs/assets/readme) with `manifest.json` |
| [`raster.py`](raster.py) | The GIF and the social preview PNG, drawn by a headless browser |
| [`check.py`](check.py), [`preview.py`](preview.py) | The offline checks, and the pages and frames to look at |

## Rebuild

```bash
python tools/readme-media/build.py           # every SVG and the manifest; Python 3.11+, standard library only
python tools/readme-media/build.py --check   # regenerate in memory; fail if a committed file differs
```

The SVG files are a pure function of this directory, the contract examples and
`rust-toolchain.toml`: the same bytes on any machine, with no dependency to install.

Two files are raster and need a renderer, so their bytes depend on it (`"reproducible":
"renderer"` in the manifest, with the versions that drew the committed files):

```bash
python -m venv .venv && .venv/bin/pip install -r tools/readme-media/requirements.txt
.venv/bin/python tools/readme-media/build.py --raster   # also protocol-demo.gif and social-preview.png
```

`--raster` needs a Chromium browser (Edge or Chrome; `README_MEDIA_BROWSER` names another
executable). It runs headless with a throwaway profile and its DNS mapped to nowhere, opens only
files this build wrote, and never shows a window or reads the screen. Nothing is published or
uploaded by any command here.

## The demo

`protocol-demo-*.svg` is an animated SVG: CSS keyframes inside the file, no script. It is about
30 kB per variant where the GIF it replaces was 4.4 MB, stays sharp at any size and follows the
reader's theme.

| Time | Beat | Source |
|---|---|---|
| 0–4.6 s | A simulated window changes; only the changed tiles are marked | Algorithm 1 (chapter 04) |
| 4.6–8.6 s | Outlines and tags: `e8` revision 1 and `e7` revision 3 | `observe_output.json`, chapter 07 |
| 8.6–12.6 s | The agent calls `observe_screen` and reads the element list of snapshot `s16`, whose age starts to count (compressed clock) | `observe_output.json` (elements), `stale_output.json` (snapshot) |
| 12.6–17.4 s | `s16` passes the 10 s `max_snapshot_age_ms` bound; the dry-run `execute_action` on it stops at the gate: `STALE_SNAPSHOT`, no input | `stale_output.json`, chapter 07 §7.4, ADR-0025 |
| 17.4–22 s | A new observation marks the changed tiles again and publishes `s17`; the dry run on it returns `simulated`, `input_delivered: false` | `click_input.json`, `simulated_output.json` |
| 22–24 s | Summary: the whole exchange stays on screen | — |

The animation illustrates the protocol on a drawn window. It is not a recording, a live action
or a latency measurement, and every variant says so inside the picture. A dry run ends before
any capture, so it never detects pixel changes: its `STALE_SNAPSHOT` here comes from the
snapshot's age. A revision mismatch would be `STALE_TARGET`.

**Reduced motion.** The animations run only inside `@media (prefers-reduced-motion:
no-preference)`. Each animated group also carries, as plain attributes, the value it has in the
summary at 23 s; a reader who asked for reduced motion, or a renderer that plays no CSS
animation, sees that still, which holds the whole exchange.

**One timeline, two outputs.** [`timeline.py`](timeline.py) writes the keyframes and can freeze
the scene at any time. The frames of the GIF, the contact sheets and the still all come from
that second output; `preview.py motion` pauses the browser's animation at given times and
compares it with the computed frames.

`protocol-demo.gif` (840×454, 24 s, 10 frames per second, 128 colours) is the same loop for
places that play no SVG animation. The results figure transcribes
[chapter 09 §9.11](../../docs/thesis/09_EVALUACION_EXPERIMENTAL.md), including its phase-1
decoy-verdict exception and the provider-version attribution.

## Verify

```bash
python -m unittest discover -s tools/readme-media -p "test_*.py"   # standard library only
python tools/readme-media/check.py
.venv/bin/python tools/readme-media/preview.py pages    # READMEs as GitHub lays them out: light, dark, desktop, phone
.venv/bin/python tools/readme-media/preview.py frames   # contact sheets of the demo, every variant
.venv/bin/python tools/readme-media/preview.py motion   # the browser's animation against the computed frames
```

`check.py` validates relative links and anchors, SVG safety (no script, external reference or
event handler), the manifest (digests, a byte budget per kind, nothing unlisted), the rules of
an animated SVG (CSS only, inside the reduced-motion guard, a still for every animated group,
one loop of at most 30 s, the word "Simulated desktop" in the picture), the four variants and
their order in every `<picture>`, alt text on every image, that the language twins have the same
structure and destinations, and the verbatim JSON examples. It also prints each README's word
count. `preview.py` writes to `.qa/`; look at those files before committing. It uses GitHub's own
Markdown parser with a local approximation of its styles, and no online service.

Budgets, enforced by `check.py`: 16 kB per still SVG, 48 kB per animated SVG, 600 kB for the
GIF, 200 kB for the PNG and 1 MB for the raster files together.

## Tools and licences

Our media-tool source is MIT OR Apache-2.0; the generated artwork is CC BY 4.0. Development
tooling only; none of it enters Cargo or the Rust binary, and no browser, wheel or other binary
is committed.

| Tool | Version | Licence | Used for |
|---|---|---|---|
| [Pillow](https://pypi.org/project/pillow/13.0.0/) | 13.0.0 | MIT-CMU | Slicing screenshots, the GIF and the PNG |
| [cmarkgfm](https://pypi.org/project/cmarkgfm/2025.10.22/) | 2025.10.22 | MIT (bundles cmark-gfm, BSD-2-Clause) | The preview's Markdown |
| [cffi](https://pypi.org/project/cffi/2.1.1/), [pycparser](https://pypi.org/project/pycparser/3.12/) | 2.1.1, 3.12 | MIT-0, BSD-3-Clause | Required by cmarkgfm |
| A Chromium browser | recorded in the manifest | its own | Drawing the frames and the preview |

## Owner-only publishing steps

The owner can upload `social-preview.png` (1280×640) under **Settings → Social preview**. The
build changes no repository setting.
