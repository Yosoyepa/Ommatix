# Development and repository layout

[Español](development.es.md) · [Documentation](../README.md)

## Layout

```text
src/                       Cargo workspace (modular monolith, ADR-0014)
├── domain/                ommatix-domain        thesis core, no I/O: incremental, fusion,
│                                                appearance, identity, snapshot
├── application/           ommatix-application   SDD ports, the Engine use case, contract types
├── adapters/
│   ├── accessibility/     ommatix-adapter-accessibility  AccessibilityProvider: UI Automation (feature `uia`)
│   ├── capture/           ommatix-adapter-capture  FrameCapturer: scripted, PNG frames, screen, DXGI
│   ├── vision/            ommatix-adapter-vision   VisionDetector, TextRecognizer (flat-region, ONNX, Windows OCR)
│   ├── input/             ommatix-adapter-input    OsActuator: dry run, native input, SendInput
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
tools/                     measurement and acceptance tools outside the workspace (never linked
├── s0/                    into the binary): model export (`--fp16`, `--dynamic`), synthetic frames,
│                          operator configurations
├── repro/                 `environment.py`: commit, toolchain, lockfiles, machine and pinned artefacts (REL-01)
├── s3/, s4/               MCP drivers, the controlled test application (`s4/testapp`, with
│                          the hidden `--animate` flags of the RNF-05 measurements) and the
│                          editor (`s4/testeditor`) and client (`s4/e2e_dossier.py`) of the
│                          dossier's E2E-02 and E2E-03
└── perf/                  `measure_latency.py`: per-stage latency against the test application;
                           `soak.py` (PERF-03: resources and 2 h soak) and `dpi_probe.py` (IT-07: DPI
                           100/125/150 %), read only, no input; IT-07 ran on 15 October 2026, REL-01 not yet
```

How the dossier's SDD components ([chapter 02 §2.3](../thesis/02_SDD_ARQUITECTURA_MVP.md))
map onto the crates:

| SDD component | Where |
|---|---|
| MCP Adapter | `ommatix-adapter-mcp` |
| Observation Coordinator, Policy & Action Gate | `Engine` in `ommatix-application` |
| Spatial Fusion, Temporal Registry | `ommatix-domain` (`fusion`, `identity`), driven by the `Engine` |
| Icon Resolver | `ommatix-domain` (`appearance`), run by the `Engine` on text-less controls of the planned regions with the operator's curated prototypes |
| Capture Provider | `ommatix-adapter-capture` (`FrameCapturer`) |
| Perception Provider | `ommatix-adapter-vision` (`VisionDetector`, `TextRecognizer`) |
| Accessibility Provider | `ommatix-adapter-accessibility` (`AccessibilityProvider`; optional, off by default) |
| OS Actuator | `ommatix-adapter-input` (`OsActuator`) |
| Effect Verifier | the `Engine`'s re-capture after delivery (visual change only); see the architecture for its limits |
| Telemetry | stage timings in MCP `_meta`; see the architecture for its limits |

And how the dossier package maps onto the repository: its chapters, contracts, matrices,
templates (`plantillas/`: configuration and ports), features, diagrams and oracle are in
`docs/thesis/`; its ADRs are in `docs/adr/` in English; the reading edition (PDF and Word), the
manifest and the source conversation [F0] are kept outside the repository
([CHANGELOG 1.0.1](../thesis/CHANGELOG.md)).

## Checks

```bash
git config core.hooksPath .githooks     # once: fmt + clippy + tests before commit/push
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # unit, integration and Gherkin acceptance tests
cargo clippy -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia --all-targets -- -D warnings   # on Windows
cargo test -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia                              # on Windows
```

No test sends real input: the `SendInput` actuator is tested through its pure helpers only.

Contribution rules, for humans and AI agents alike, are in [AGENTS.md](../../AGENTS.md).

## Research and history

The name refers to ommatidia, the units of a compound eye: the project’s analogy is to divide an image into cells and attend to local changes. The thesis is *Incremental visual perception and safe temporal grounding for desktop agents: spatial fusion, perceptual cache and identifier-based control over MCP*.

Chapters, requirements, scenarios and planned tests are indexed in [the dossier](../thesis/README.md); use the [traceability matrices](../thesis/matrices) for the current inventory. Each acceptance box is documented in [§6.7](../thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md#67-definition-of-done-del-producto), and prior decisions and runs live in the [CHANGELOG](../thesis/CHANGELOG.md), [S0](../thesis/S0_RESULTADOS.md), [S3](../thesis/S3_CORRIDAS_SUPERVISADAS.md) and [S4](../thesis/S4_ACEPTACION_REDUCIDA.md). The current blocker list is in §6.7; historical rounds do not describe the current state.
