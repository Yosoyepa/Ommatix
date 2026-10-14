# Ommatix

**Spatial Indexing and Incremental Perception for GUI Agents**

🇪🇸 [Leer en español](README.es.md)

Ommatix is a local perception engine that lets an LLM agent operate a desktop safely. It turns
screen frames into GUI elements with stable identifiers, re-analyses only the regions that
changed, and serves the result through the [Model Context Protocol](https://modelcontextprotocol.io).
The agent chooses an element by its identifier; Ommatix keeps the geometry, checks that the
agent's observation is still current, acts through the operating system and reports what it
could verify. Perception lives here; reasoning lives in the agent.

> **Why "Ommatix"?** Named after *ommatidia*, the independent photoreceptor units that make
> up the compound eye of fast-flying insects. Just as each ommatidium processes local changes
> in luminance in parallel without saturating the central nervous system, Ommatix divides the
> screen framebuffer into a spatial matrix of independent cells, processes only the regions
> with visual activity and gives the agent stable element identifiers instead of coordinates.

## The thesis

Ommatix implements the thesis *Incremental visual perception and safe temporal grounding for
desktop agents: spatial fusion, perceptual cache and identifier-based control over MCP*. Its
engineering and research dossier, in Spanish, is the baseline of this repository:
[`docs/thesis/`](docs/thesis/README.md).

| Part | Where |
|---|---|
| Technical audit, thesis framework, SDD, requirements (RF-01..18, RNF-01..10) | [chapters 00–03](docs/thesis/README.md) |
| Algorithms: incremental perception, spatial fusion, icon cache, temporal identity | [chapter 04](docs/thesis/04_ALGORITMOS_CORE.md) |
| TDD plan (76 planned tests), integration plan S0–S4 and definition of done | [chapters 05–06](docs/thesis/05_TDD_PLAN_Y_MATRIZ.md) |
| MCP contract, errors, JSON schemas and examples | [chapter 07](docs/thesis/07_CONTRATOS_MCP_Y_ERRORES.md), [`contratos/`](docs/thesis/contratos/README.md) |
| Security, evaluation protocol, runbook, references | chapters 08–12 |
| 32 Gherkin scenarios (BDD-01..32) and traceability matrices | [`features/`](docs/thesis/features), [`matrices/`](docs/thesis/matrices) |
| Python reference oracle (48 tests) and package validator | [`referencia_ejecutable/`](docs/thesis/referencia_ejecutable) |
| Architecture decisions ADR-0001..0015, in English | [`docs/adr/`](docs/adr/README.md) |

## How it works

```mermaid
flowchart LR
    CAP[Capture<br/>frame + dirty rects] --> PLAN[Algorithm 1<br/>exact tile diff,<br/>halo closure, budget]
    PLAN --> PER[Detector + OCR<br/>on planned regions]
    PER --> FUS[Algorithm 2<br/>spatial fusion,<br/>reading order, labels]
    FUS --> ICO[Algorithm 3A<br/>icon cache<br/>with rejection]
    ICO --> TRK[Algorithm 3B<br/>session IDs,<br/>revisions]
    TRK --> SNAP[Atomic commit<br/>immutable snapshot]
    SNAP -->|observe_screen| AG[LLM agent]
    AG -->|execute_action:<br/>target_id + revision| GATE[Policy and<br/>action gate]
    GATE --> ACT[OS actuator] --> VER[Effect verifier]
```

A failed step publishes nothing: the engine keeps the pending regions, reports itself
degraded and blocks actions until a consistent observation exists. Actions are dry runs
unless the agent and a local policy both allow them. More in the
[architecture overview](docs/architecture/overview.md).

## Status

**The dossier is the specification; the code is catching up with it.**

- `docs/thesis/` holds the consolidated dossier. Its oracle (48/48) and validator run in CI.
- `ommatix-domain` holds the thesis core: Algorithm 1 (exact tile diff, region plan, halo
  closure), Algorithm 2 (spatial fusion), Algorithm 3A (icon cache with rejection),
  Algorithm 3B (identity registry, freshness checks) and transactional snapshots, tested
  against the oracle's cases (UT-01..43).
- `ommatix-application` runs it as one engine behind the SDD ports, and `ommatix-adapter-mcp`
  serves the thesis contract (`observe_screen`, `execute_action`, `get_engine_metrics`) over
  MCP 2026-07-28 ([ADR-0015](docs/adr/0015-mcp-transport-on-rmcp-wire-types.md)). Contract
  tests CT-01..10 and scenarios BDD-01..27 run in CI. `serve` and `demo` use this engine.
- What is **not** real yet: the screen is a simulated desktop or a fixture, the detector is
  the bootstrap's flat-colour region finder (roles `unknown`, no OCR), live input is off by
  default and unverified on Windows, and no real MCP client has been tested. S3 brings DXGI,
  ONNX Runtime and `SendInput`; S4 the acceptance runs. The
  [overview](docs/architecture/overview.md#where-the-code-is-today) lists what remains.
- First platform: **Windows 11 x64**, one monitor, DPI 100/125/150 %
  ([ADR-0008](docs/adr/0008-one-real-platform-first.md), approved 2026-10-14).

## Roadmap

From [thesis chapter 06](docs/thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md):

- [x] Dossier consolidated into the repository; oracle and validator in CI
- [ ] **S0, spike:** hardware, candidate detectors and OCR, licences and ONNX export,
  execution provider, DXGI capture with stride, MCP client compatibility
- [x] **S1, geometric and incremental core:** exact tile diff, halo closure, line grouping,
  text ownership, staging and commit (UT-01..26, BDD-01..10) in `ommatix-domain`; oracle
  tests ported to Rust
- [x] **S2, identity, cache and contract:** temporal registry with tombstones, icon cache
  (UT-27..43, S2a); the engine, the SDD ports and the MCP contract through `rmcp` with an
  honest dry run (CT-01..10, BDD-11..27, S2b)
- [ ] **S3, real adapters and action loop on Windows 11:** DXGI, ONNX Runtime, `SendInput`,
  policy, stop and effect verification; the bootstrap's macOS and X11 adapters are retired
- [ ] **S4, acceptance:** 450 end-to-end runs on controlled applications, benchmarks, soak
  and the acceptance report

## Quick start

Requires Rust stable (pinned in `rust-toolchain.toml`) and Python 3.11 or later for the
thesis checks.

```bash
# Thesis checks (from docs/thesis/; the validator needs jsonschema)
pip install -r docs/thesis/requirements-validacion.txt
cd docs/thesis && python referencia_ejecutable/run_tests.py && python validar_paquete.py && cd -

# The engine
cargo run -- demo                       # observe and act on the simulated desktop, printed
cargo run -- serve                      # MCP server on stdio over the simulated desktop
cargo run -- serve --config ommatix.toml   # same, with a configuration file
```

The configuration file has the shape of
[`docs/thesis/plantillas/configuracion_mvp.toml`](docs/thesis/plantillas/configuracion_mvp.toml)
(`[engine]`, `[mcp]`, `[safety]`, …); without one, the thesis defaults apply. Sources: the
simulated desktop (`sim`, default), a directory of PNG frames (`images`) or the live screen
(`screen`, build feature `screen`). Input is `dry-run` (default) or `native` (build feature
`native-input`), and `native` also needs the `screen` source and `actuation_enabled = true`
under `[safety]`. Even then every action is a dry run unless the agent sets `dry_run: false`.
The legacy MCP profile 2025-11-25 is served only with `[mcp] legacy_enabled = true` or the
matching CLI flag.

`[models]` is empty by default: no model is loaded, the detector is the flat-region finder
and there is no OCR. With the `images` or `screen` source it can select a detector
(`detector_manifest` + `detector_manifest_sha256`, a manifest shaped like
[`model_manifest.json`](docs/thesis/plantillas/model_manifest.json) whose status is
`SELECCIONADO`, plus `onnxruntime_path` + `onnxruntime_sha256`; build feature `onnx`) and
the Windows OCR (`ocr = "winrt"`, `ocr_language`, default `es-ES`; build feature
`winrt-ocr`). Every file is checked against its SHA-256, and a mismatch, a missing build
feature or an uninstalled language stops the engine: nothing falls back. The template's
`detector_path`, `detector_sha256`, `ocr_path` and `ocr_sha256` stay empty, since the
weights are declared in the manifest. No model has been selected yet (S0).

The MCP server exposes three tools, with the dossier's JSON schemas:

| Tool | Does |
|---|---|
| `observe_screen` | Observes the screen and returns a page of the new snapshot: `session_id`, `snapshot_id`, `geometry_epoch`, elements with `target_id`, `revision`, role, label and actionability, never coordinates; `query`, `limit` and a `cursor` bound to the snapshot |
| `execute_action` | `click`, `type` or `scroll` on a `target_id` at an expected revision; dry run by default; returns `simulated`, `rejected`, `input_delivered`, `effect_observed` or `indeterminate` |
| `get_engine_metrics` | Mode, observations, rejected and delivered actions, pending regions, whether models are loaded |

Domain errors come back as tool results with `isError: true`, a `code`, `retryable` and a
`next_step`; stage timings travel in the result's `_meta` under `io.ommatix/timings`.

## Repository layout

```text
src/                       Cargo workspace (modular monolith, ADR-0014)
├── domain/                ommatix-domain        thesis core, no I/O: incremental, fusion,
│                                                appearance, identity, snapshot
├── application/           ommatix-application   SDD ports, the Engine use case, contract types
├── adapters/
│   ├── capture/           ommatix-adapter-capture  FrameCapturer: scripted, PNG frames, screen
│   ├── vision/            ommatix-adapter-vision   VisionDetector, TextRecognizer (flat-region, ONNX, Windows OCR)
│   ├── input/             ommatix-adapter-input    OsActuator: dry run, native input
│   ├── sim/               ommatix-adapter-sim      simulated and fixture desktops (all four
│   │                                               pixel/input ports) for deterministic tests
│   └── mcp/               ommatix-adapter-mcp      MCP server over stdio (driving adapter),
│                                                   CT-01..10 in tests/
├── app/                   ommatix               composition root and CLI binary (serve, demo)
└── acceptance/            ommatix-acceptance    runs the thesis scenarios (tests/thesis.rs)
docs/
├── thesis/                the dossier (Spanish): chapters 00–12 at its root, contratos/,
│                          features/, matrices/, plantillas/, diagramas/, fuentes/, oracle
├── adr/                   architecture decision records (bootstrap ones in adr/bootstrap/)
├── architecture/          overview (EN/ES)
├── diagrams/              Mermaid diagrams, English labels
└── reviews/               code reviews: finding, resolution, regression test
```

How the dossier's SDD components ([chapter 02 §2.3](docs/thesis/02_SDD_ARQUITECTURA_MVP.md))
map onto the crates:

| SDD component | Where |
|---|---|
| MCP Adapter | `ommatix-adapter-mcp` |
| Observation Coordinator, Policy & Action Gate | `Engine` in `ommatix-application` |
| Spatial Fusion, Temporal Registry | `ommatix-domain` (`fusion`, `identity`), driven by the `Engine` |
| Icon Resolver | `ommatix-domain` (`appearance`); joins the `Engine`'s observation with the S3 models |
| Capture Provider | `ommatix-adapter-capture` (`FrameCapturer`) |
| Perception Provider | `ommatix-adapter-vision` (`VisionDetector`, `TextRecognizer`) |
| OS Actuator | `ommatix-adapter-input` (`OsActuator`) |
| Effect Verifier | the `Engine`'s re-capture after delivery; partial, completed in S3 |
| Telemetry | stage timings in MCP `_meta`; partial, completed in S3 |

And how the dossier package maps onto the repository: its chapters, contracts, matrices,
templates (`plantillas/`: configuration and ports), features, diagrams and oracle are in
`docs/thesis/`; its ADRs are in `docs/adr/` in English; the reading edition (PDF and Word), the
manifest and the source conversation [F0] are kept outside the repository
([CHANGELOG 1.0.1](docs/thesis/CHANGELOG.md)).

## Development

```bash
git config core.hooksPath .githooks     # once: fmt + clippy + tests before commit/push
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # unit, integration and Gherkin acceptance tests
```

Contribution rules, for humans and AI agents alike, are in [AGENTS.md](AGENTS.md).

## Documentation

- [Thesis dossier](docs/thesis/README.md) (Spanish)
- [Architecture overview](docs/architecture/overview.md) · [Visión general](docs/architecture/overview.es.md)
- [Architecture decision records](docs/adr/README.md)
- [Diagrams](docs/diagrams/README.md)
- [Glossary / Glosario](docs/glossary.md)
- [Bootstrap MVP code review](docs/reviews/2026-10-14-mvp-review.md)
