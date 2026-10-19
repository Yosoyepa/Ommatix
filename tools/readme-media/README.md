# README media

Synthetic, code-generated illustrations for the two repository READMEs. No screen capture,
GUI application, model, input adapter or external account is used. Element labels and
protocol fields are read directly from `docs/thesis/contratos/ejemplos/*.json`.

## Rebuild

Use Node **22.23.1**, Python 3.11+, Windows x64 and FFmpeg
**2024-04-10-git-0e4dfa4709** on `PATH`. The renderer and preview use Chrome Headless Shell
**129.0.6668.58**. A dedicated headless binary avoids Windows Chrome's GUI executable and
its `--version` behaviour. Set up the browser once, before the offline build:

```bash
npm ci --ignore-scripts
node node_modules/@puppeteer/browsers/lib/main-cli.js install chrome-headless-shell@129.0.6668.58 --path .cache
```

Then one command sequence regenerates every committed asset:

```bash
npm ci && npm run build
```

The default browser path is the Windows headless-shell cache above. For another installation
of the same build, set `HYPERFRAMES_BROWSER_PATH`. Browser downloads happen only during
setup. Node dependencies are exact pins with `package-lock.json`; `ffprobe-static` supplies
the media probe. System fonts are used; identical Windows fonts, Chrome and FFmpeg are
needed for byte-for-byte reproduction. Builds on another OS/font rasteriser may differ.

The render subprocess disables telemetry and update checks. `offline.mjs` rejects
non-loopback HTTP from Node, the composition's CSP denies external resources, and the
social-preview, stills and preview browser blocks external requests. Headless Chrome uses a local composition
server; it does not record the desktop. The build never publishes or uploads anything.

`artwork.mjs` contains the palette, typography, 8-pixel-based layout and SVG generators for the
badges, hero, pipeline, results, gate and social preview. `demo.mjs` reads the contract examples
and wraps `demo/scene.js`, the demo composition: every frame is a pure function of time (eased
transitions, no animation library, clock or random source) exposed through HyperFrames' seekable
timeline. `demo-stills.mjs` (`npm run stills -- OUT_DIR 8.5 26.5 ...`) renders single frames for
review before a full render. `social-preview.svg` is the generated SVG source for the social
preview PNG. SVGO preserves titles, descriptions and image roles.

Outputs go to [`docs/assets/readme/`](../../docs/assets/readme). `manifest.json` lists their
sizes and SHA-256 digests. `protocol-demo.mp4` is 1280×720, 30 fps, 44 seconds, silent H.264
with burned-in English captions. `protocol-demo.gif`, the README's inline animation, is cut
from the same render: scenes 01–05 (5.0–35.8 s) at 1.5× speed, 800 px wide, 12 fps, 64 colours;
final FFmpeg encoding uses a single thread and strips variable metadata.
Each binary must be at most 5 MB, all binaries at most 12 MB, and the social preview below
1 MB. These limits are enforced by the build.

## Tool choice and licences

Registry and upstream licence files were checked on **19 October 2026**, before selection:

| Tool | npm version at review | Upstream terms | Decision |
|---|---|---|---|
| [HyperFrames](https://registry.npmjs.org/hyperframes/0.8.178) | 0.8.178 | [Apache-2.0 at the release's git commit](https://github.com/heygen-com/hyperframes/blob/051175719e05a8b711e89fc4b44a495919694c05/LICENSE) | Selected: an HTML/SVG composition and a seekable timeline need no React or GSAP. The local Windows headless render was verified. |
| [Remotion](https://registry.npmjs.org/remotion/4.0.537) | 4.0.537 | `LICENSE.md` in the [release tarball](https://registry.npmjs.org/remotion/-/remotion-4.0.537.tgz), source available with conditions | Its Free License permits an individual's research video; larger for-profit organisations need a Company License, and reselling a derivative of Remotion is restricted. It was not installed or used to render. |

Remotion documents [server-side rendering](https://www.remotion.dev/docs/renderer) and
[licensing conditions](https://www.remotion.dev/docs/license). HyperFrames documents its
[local CLI](https://hyperframes.heygen.com/packages/cli) and
[deterministic frame rendering](https://hyperframes.heygen.com/concepts/determinism).
This is development tooling only; none of it enters Cargo or the Rust binary.

Our media-tool source is MIT OR Apache-2.0; generated documentation artwork is CC BY 4.0.
Dependencies retain their terms: HyperFrames Apache-2.0, Puppeteer Apache-2.0, SVGO MIT,
Markdown-it MIT and Mermaid MIT. `ffprobe-static`'s wrapper is MIT; its bundled FFprobe
executables retain FFmpeg's LGPL/GPL build terms. The local FFmpeg build enables GPL
components. No browser, encoder, probe or dependency binary is committed or redistributed.

## Storyboard and evidence

| Time | Illustration | Source |
|---|---|---|
| 0–4.4 s | Title: agents act on element IDs and revisions, not pixels | Repository README |
| 4.4–10.2 s | A synthetic window changes; only the changed tiles light up | Algorithm 1 (chapter 04) |
| 10.2–16.2 s | Boxes and tags for `e8` and `e7` (revision 3, button, confirmed); what the agent receives and what stays in the engine | `observe_output.json`, chapter 07 |
| 16.2–23 s | The agent calls `observe_screen` and reads the element list (snapshot `s16`) | `observe_output.json` (elements), `stale_output.json` (snapshot) |
| 23–30 s | The window changes (snapshot `s16` → `s17`); `execute_action` on `s16` stops at the gate: `STALE_SNAPSHOT`, no input | `stale_output.json` |
| 30–35.8 s | The agent observes again and acts on `s17` in dry run: `simulated`, `input_delivered: false`, `verified: false` | `click_input.json`, `simulated_output.json` |
| 35.8–39.6 s | The live-input chain: operator policy, foreground app, fresh snapshot, last look, delivery, effect verification | AGENTS.md §5 rule 6, ADR-0007 |
| 39.6–44 s | Name, tagline, repository | Repository README |

The animation illustrates the protocol on a stylised simulated desktop. It is not a
recording, a successful live action, or a latency measurement. A revision mismatch is
`STALE_TARGET`; the illustrated `STALE_SNAPSHOT` is specifically an old snapshot.
The results figure transcribes [chapter 09 §9.11](../../docs/thesis/09_EVALUACION_EXPERIMENTAL.md),
including its phase-1 decoy-verdict exception and provider-version attribution.

## Verify locally

```bash
npm run test
npm run check
npm run preview
```

`check.py` validates relative Markdown links and anchors, `<picture>` sources, SVG XML and
active/external content, asset digests and size budgets, and the verbatim JSON examples.
`preview.mjs` renders locally with Markdown-it and Mermaid, saving HTML and screenshots in
`.qa/` for both languages, light/dark themes and desktop/mobile widths, plus expanded
Mermaid views. Inspect those files before committing. It checks body overflow and image
loading; it does not use GitHub's Markdown API or any online rendering service.

Rebuild twice and compare `manifest.json` to check determinism. The build records source
commit `bf90d32`, where the cited scores and protocol examples were verified. Asset sources
must be updated together with outputs. A future media-freshness CI job could run the build,
checker and digest comparison in a pinned Windows/browser environment; this PR changes no
CI workflow.

## Owner-only publishing steps

GitHub's [attachment documentation](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/attaching-files)
describes uploaded video attachments and recommends H.264. A repository MP4 link opens a
file page rather than an autoplaying README player, so the README shows the looping GIF,
which links to the MP4. Optionally, the owner can upload `protocol-demo.mp4` in GitHub's web interface
and place the resulting attachment URL in both READMEs for an inline player. Uploading
publishes content and is not part of the build.

The owner can upload `social-preview.png` under **Settings → Social preview**. The generated
image is 1280×640. The build does not change repository settings.
