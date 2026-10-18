# AGENTS.md — Ommatix

Rules for anyone changing this repository: human contributors and AI coding agents alike.
Read this file before your first change. If a rule here conflicts with a request, raise it
instead of silently breaking the rule.

## 1. What this project is

Ommatix (*Spatial Indexing and Incremental Perception for GUI Agents*) is a Rust perception
engine for desktop agents and the implementation of a thesis. It turns screen frames into GUI
elements with stable session identifiers, re-analyses only the regions that changed,
publishes immutable snapshots and serves them to LLM agents over MCP. Agents act by element
ID and revision, never by coordinates. Perception lives here; reasoning lives in the agent.
Keep them apart.

**The thesis dossier in `docs/thesis/` is the specification.** Requirements (RF/RNF), the
algorithms, the MCP contract, the Gherkin scenarios and the test plan come from it. When the
code and the dossier disagree, the dossier wins unless an ADR changes it. A change to a
requirement, an algorithm, the contract or a scenario updates the dossier in the same PR and
adds an entry to `docs/thesis/CHANGELOG.md`.

**`src/` implements the thesis engine**, realigned with the dossier in stages S0–S4 (thesis
chapter 06). Since S2b, `serve` and `demo` run the `Engine` of `ommatix-application` over the
SDD ports and serve the Ommatix 1.0 contract (`observe_screen`, `execute_action`,
`get_engine_metrics`) over MCP; the bootstrap prototype's use cases, ports, seven MCP tools
and acceptance suite are retired, and `ommatix-domain` keeps only the bootstrap support types
the interim adapters use. Some bootstrap pieces remain as interim adapters until S3 replaces
them (the flat-region detector, `xcap` capture, `enigo` input): do not extend them, and never
put coordinates on the wire. The prototype's decisions are archived in `docs/adr/bootstrap/`.
The icon cache (Algorithm 3A) runs in the observation only with the operator's curated
prototypes (a file pinned by SHA-256 in `[cache] prototypes_sha256`) and thresholds; never give those thresholds defaults (the thesis calibrates them)
and never let an icon label change actionability. Accessibility evidence (ADR-0017, UI
Automation, `[models] accessibility`) is fused with the visual path and never replaces it: it
confirms an element only when it is enabled, on screen and has an action pattern, it never
reads a field's value, and its names are untrusted data like OCR text.

## 2. Language policy (English / Español)

See ADR-0013.

| Artefact | Language |
|---|---|
| Code, identifiers, code comments, commit messages, CLI output, MCP tool text | English |
| Thesis dossier (`docs/thesis/`) | Spanish |
| `README.md`, `docs/architecture/overview.md` | English, with a Spanish twin `*.es.md` |
| ADRs (`docs/adr/`) | English body + mandatory `## Resumen (ES)` section |
| Gherkin | Thesis scenarios stay in Spanish (`# language: es`) and keep their `@BDD`/`@RF`/`@UT` tags |
| Diagrams | English labels; captions may be bilingual; the Spanish sources in `docs/thesis/diagramas/` stay canonical |
| Issues / PR descriptions | Either language |

When you change an English doc that has a `*.es.md` twin, update the twin in the same PR.

## 3. Architecture: hexagonal modular monolith

One process (ADR-0011), one binary, several crates (ADR-0014). Each crate is a module with a
compiler-enforced boundary.

```text
            ┌───────────────────────── app (composition root, CLI) ─────────────────────────┐
            │                                                                               │
  driving   │   adapter-mcp ──▶ application ──▶ domain ◀── adapter-vision    driven         │
  adapters  │                       ▲  ports      ▲      ◀── adapter-capture  adapters      │
            └───────────────────────┴─────────────┴─────────────────────────────────────────┘
```

**Dependency rule (MUST):**

| Crate | May depend on |
|---|---|
| `ommatix-domain` | `thiserror`, plus pure-computation crates listed here: `unicode-normalization` (NFC for labels) and `rstar` (pinned `=0.13.0`: the R*-tree that enumerates candidate controls for Algorithm 2, ADR-0027; its default features only, no I/O, no randomness in the library). No I/O, no threads, no clocks, no serialisation, no other Ommatix crate |
| `ommatix-application` | `ommatix-domain` + `thiserror`. No adapter crate; time only through the `Clock` port. Standard-library scoped threads only for the stages of one observation (ADR-0019) |
| `ommatix-adapter-*` | `ommatix-application`, `ommatix-domain`, their own external libs. **Never another adapter** (dev-dependencies for integration tests are fine) |
| `ommatix` (app) | everything; it is the only place where concrete adapters are chosen |
| `ommatix-acceptance` | anything, as dev-dependencies only (test crate, never linked into the binary) |

- Ports follow the SDD (thesis chapter 02 §2.4): `FrameCapturer`, `VisionDetector`,
  `TextRecognizer`, `AccessibilityProvider`, `OsActuator`, `Clock`, `PolicyStore`. A new
  external capability is a **port trait in `application/src/ports.rs`** plus a **new adapter crate** under
  `src/adapters/`. Driving adapters (MCP) call the `Engine` directly; there are no inbound
  port traits.
- A pure-computation crate in the domain (an R*-tree, Unicode normalisation) is added to the
  table above by the PR that first needs it.
- Do not create a crate for something that has no second implementation in sight; add a
  module instead. Hexagonal boundaries are for swapping technology, not for ceremony.
- Wire DTOs (JSON shapes) live in the adapter that owns the wire format (`adapter-mcp`),
  never in the domain or application: neither depends on `serde`. They must match the JSON
  schemas in `docs/thesis/contratos/esquemas/`.
- Use cases receive their collaborators through constructors (`Box<dyn …>`, `Arc<dyn Clock>`)
  or method parameters typed as traits; they never construct adapters, read the system clock
  or sleep on their own.
- One adapter may implement several ports when they share state by nature (the simulated
  desktop); it still depends only on the application and domain.

## 4. Where things go

| You are adding… | Put it in |
|---|---|
| A change to a requirement, algorithm, contract or scenario | `docs/thesis/` (Spanish) + an entry in `docs/thesis/CHANGELOG.md` |
| Tiles, regions, fusion, descriptors, identity registry, snapshot types | `src/domain/src/` |
| A use case or a port trait | `src/application/src/` |
| Code that touches the OS, files, network, models or a protocol | `src/adapters/<name>/` |
| CLI flags, wiring, config defaults | `src/app/src/` (`cli.rs`, `wiring.rs`) |
| An architectural decision | `docs/adr/NNNN-title.md` (copy `0000-template.md`) |
| A diagram | `docs/diagrams/*.md` as Mermaid |
| A code review | `docs/reviews/YYYY-MM-DD-topic.md`: finding → resolution → regression test |
| Executable acceptance scenarios | Thesis scenarios stay in `docs/thesis/features/` (the validator traces them there); their steps go in `src/acceptance/tests/thesis.rs`, and the PR that adds them adds the scenario to its `IMPLEMENTED` list. There is no other feature directory |
| Unit tests | next to the code (`#[cfg(test)] mod tests`); a test that implements a planned test carries its ID (`ut_13_failed_commit_keeps_pending_dirty`) |
| Cross-crate / end-to-end tests | `src/<crate>/tests/` (the MCP contract tests CT-xx live in `src/adapters/mcp/tests/`, with the telemetry, injection and exfiltration tests IT-06, SEC-03 and SEC-04; the process-level ones in `src/app/tests/`; invariants of the repository itself, REL-01 and DOC-01, in `src/acceptance/tests/repository.rs`) |
| A measurement of the core (PERF-01) | `src/acceptance/examples/core_latency.rs`; its numbers go to `docs/thesis/S0_RESULTADOS.md` with the machine and the commit |

Nothing MVP-related goes in the repository root except workspace configuration. The licence
files (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-DOCS`) are repository metadata, not MVP
content, and are allowed there.

## 5. Non-negotiable engineering rules

1. **Determinism.** Same frames ⇒ same elements, ids and JSON. Use `BTreeMap`/`BTreeSet` or
   sort explicitly before anything is emitted; never iterate a `HashMap` into output; no
   randomness in domain or application. The stages of one observation run concurrently
   (ADR-0019) and are collected by stage, never by arrival order, so the evidence is the same
   as in sequence. Time comes from the injected `Clock` (`ManualClock` in tests);
   run-dependent values (timings) go in MCP `_meta`, never in `structuredContent`.
2. **Incrementality.** Compare against the last committed base, not the previous frame, and
   never re-process the full frame when only regions changed. The full refresh is Algorithm
   1's explicit fallback (geometry change, budget exceeded), never a shortcut. Any new
   pipeline stage takes the region plan as input.
3. **No `unsafe`** (`unsafe_code = "forbid"` at workspace level). Lift it only with an ADR.
   ADR-0016 (accepted) allows it only in the `#[cfg(windows)] mod win` of the capture
   (`dxgi`) and input (`sendinput`) adapters: those crates deny `unsafe_code` and
   `clippy::undocumented_unsafe_blocks`, every block wraps one Win32/COM call with a
   `// SAFETY:` comment, and raw pointers never leave the module.
4. **No panics on external input.** `unwrap`/`expect` only in tests or on documented
   invariants. Adapters return `PortError`. Geometry saturates; check arithmetic on values
   that come from the wire.
5. **stdout belongs to MCP.** In `serve` mode, diagnostics go to stderr only. Enforced by
   the workspace lint `clippy::print_stdout = "deny"`; only the `demo` printer opts out.
6. **Fail-closed actuation** (ADR-0007). Every action is a dry run unless the agent asks
   otherwise and the local policy allows it. Never make a live actuator the default and
   never bypass the action gate. Report what happened and no more: `simulated` never claims
   delivery, `input_delivered` never claims an effect, and an uncertain effect is
   `indeterminate`, never an automatic retry. Live input needs `--input sendinput` with
   `--source dxgi` on a Windows build with the `sendinput` feature, `actuation_enabled =
   true` and a non-empty `allowed_applications` in the operator's configuration, and
   `dry_run: false` from the agent; it is off by default. The engine checks the foreground
   application the actuator attests against `allowed_applications`; the `SendInput`
   actuator also refuses an elevated foreground, a point outside its windows and held
   modifiers, and `type` sends its keys only after verifying keyboard focus on the target
   field (otherwise `indeterminate`: its click may have been delivered). With accessibility
   evidence (`[models] accessibility = "uia"`) the engine delivers `type` in two phases
   (ADR-0018): the focusing click, then a poll on the injected `Clock` until UI Automation
   reports keyboard focus on the target element itself, then the keys. No confirmation in
   time means no key, `indeterminate` and the delivered click reported, never a retry; its
   `visual_change` is measured against a frame captured after the click and before the keys,
   and the field's value is never read. The interim `enigo`
   input (`native-input`) cannot attest the foreground application, so the engine never lets
   it act live. Before delivering, the engine re-captures the screen and refuses
   (`STALE_SNAPSHOT`) if it differs from the committed base, then reads the policy and
   attests the foreground application again and refuses (`POLICY_DENIED`) on a change; any
   delivery spends the snapshot until the next observation. An application whose process
   owns more than the window the operator means (`explorer.exe` also owns the desktop and
   the taskbar) is narrowed to windows with `[safety] allowed_window_classes` and
   `window_pin_file` (ADR-0020, accepted, off by default): the engine checks the
   foreground window and the window under the point at the gate and again in the last look,
   refuses what the actuator cannot attest, and never accepts a window title as evidence.
   Never add `applicationframehost.exe` to any allowlist. The operator's stop
   (`[safety] stop_file`, or the in-process `StopSwitch`) belongs to the operator alone:
   never give the agent a way to set or clear it (the same holds for the window pin file),
   and never "roll back" a delivery when it is engaged; delivered results keep their
   reported status.
7. **Privacy** (ADR-0012). No screenshots or screen values in logs by default; text on screen
   never changes permissions. Never commit screen captures, recorded sessions, datasets
   (`captures/`, `datasets/` are git-ignored) or model weights. Use synthetic frames in
   tests: `SimulatedDesktop`, `FixtureDesktop` or `ommatix_adapter_sim::scripted_session(theme)`.
8. **Latency is measured, not promised** (ADR-0010). Do not add a dependency or an
   allocation on the hot path without measuring it; report per-stage timings in PRs that
   touch it. A budget is a target until it is measured on the reference hardware.
9. Lints: `clippy::pedantic` is on. Fix warnings; `#[allow]` needs a one-line reason.
10. **Regression tests.** Every bug fix ships with a test that fails without it, named
    after the behaviour it protects. Review findings are recorded in `docs/reviews/` with
    the test that covers each one.
11. **The oracle sets the expected output** (thesis chapter 05). When you implement a planned
    test, write it to fail first and take its expected result from the matching oracle test
    (`docs/thesis/referencia_ejecutable/tests/test_core.py`, OR-xx) where one exists. A
    passing OR never marks its UT as passed, and the oracle never becomes a runtime
    dependency (ADR-0001).

## 6. Commands and definition of done

```bash
cargo fmt --all                                        # format
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI-blocking)
cargo test --workspace                                 # unit + integration + Gherkin (CI-blocking)
cargo run -- demo                                      # smoke test: the engine on the simulated desktop
python tools/perf/measure_latency.py --variant NAME=EXE,CONFIG ... --out perf.json  # RNF-05 latency, Windows, no input (S0_RESULTADOS.md §10)
python tools/perf/soak.py --config CONFIG --minutes 120 --out soak.json  # PERF-03 / RNF-06: resources and 2 h soak, Windows, no input; the test app must be in front (S4_SESIONES_PENDIENTES.md B)
python tools/perf/dpi_probe.py --config CONFIG --out dpi-probe.json  # IT-07: DPI per session, read only; never changes the scaling (S4_SESIONES_PENDIENTES.md C)
cargo run --release -p ommatix-acceptance --example core_latency -- --out perf01.json  # PERF-01: the core on 300 controls and 600 spans, no screen (S0_RESULTADOS.md §11)
cargo run -p ommatix-acceptance --example agent_scenes -- --out scenes.json  # AG-01: the synthetic scenes of the offline agent-decision experiment, no screen (ADR-0033, chapter 09 §9.10); the tool below runs it itself
python tools/agent/subq_decisions.py --max-input-tokens N --plan  # AG-01: operator-side; it generates its own synthetic scenes (offline, pinned by SHA-256) and takes no scenes file; drop --plan and add --key-file and --out (outside the checkout) to call the model
python tools/repro/environment.py --out environment.json  # REL-01: commit, toolchain, lockfiles, machine and pinned artefacts (S4_REPRODUCIBILIDAD.md)
python -m unittest discover -s tools/repro -p "test_*.py"  # REL-01 helper tests
python -m unittest discover -s tools/agent -p "test_*.py"  # AG-01 and AG-02 tool tests (fake server on 127.0.0.1 and a fake engine, no network, no screen)
python tools/agent/ag02/preflight.py --exe OMMATIX --model MODEL --out PROOF.json  # AG-02 isolation canary (ADR-0036): a synthetic world of sentinels, the production command against a local stand-in for the API, no real API call; goals run only with its proof
python tools/agent/ag02/run_goal.py --plan --phase 1 --model MODEL  # AG-02 (ADR-0036, chapter 09 §9.11): prints what the owner's session would run and starts nothing; phase 1 is dry-run only, phase 2 needs --live, the owner present, --isolation-proof and the operator's configuration (docs in the script); every path is resolved once and outputs go outside the checkout; python tools/agent/ag02/summarise.py RUN_DIR summarises them
cargo clippy -p ommatix --features screen,native-input --all-targets -- -D warnings  # when touching capture/input
cargo clippy -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia --all-targets -- -D warnings  # on Windows (CI job `windows`)
cargo test -p ommatix-adapter-capture --features dxgi -- --ignored                   # manual: one DXGI frame, never saved
```

Never write a test that injects real input; the `SendInput` actuator is tested through its
pure helpers only.

The thesis checks run from `docs/thesis/` (CI job `thesis`, CI-blocking):

```bash
pip install -r requirements-validacion.txt             # once: jsonschema for the validator
python referencia_ejecutable/run_tests.py              # oracle: 48 reference tests
python validar_paquete.py                              # schemas, examples, traceability, Gherkin structure
```

CI builds, lints and tests the `dxgi,sendinput,onnx,winrt-ocr` features on `windows-latest`
(job `windows`), and also compiles capture and native input for Windows and macOS (job
`cross-check`). To run
it locally: `rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin`, then
`cargo clippy --target <target> -p ommatix-adapter-capture -p ommatix-adapter-input
--features ommatix-adapter-capture/screen,ommatix-adapter-input/native -- -D warnings`.

A change is done when fmt, clippy, tests and the thesis checks pass, any new user-visible
behaviour has a Gherkin scenario traced to its requirement, docs (and their `*.es.md` twins)
reflect the change, and any architectural decision has an ADR. The product's definition of
done is in thesis chapter 06 §6.7; its boxes stay unchecked until the evidence exists.

### Toolchain

Rust at the exact release pinned in `rust-toolchain.toml` (REL-01; the CI installs the same
one; MSRV in `Cargo.toml`) and Python 3.11 or later for the thesis checks. The first target platform is Windows 11 x64 (ADR-0008). The interim
`screen` and `native-input` features (`xcap`, `enigo`) on Linux need: `libwayland-dev libpipewire-0.3-dev
libgbm-dev libegl1-mesa-dev libxcb1-dev libxrandr-dev libdbus-1-dev clang`.

## 7. Git workflow

- `main` is protected: change it through pull requests only.
- Branches: `feat/…`, `fix/…`, `docs/…`, `chore/…`, `refactor/…`.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org):
  `feat(domain): add region expansion`. Scope = crate short name (`domain`, `application`,
  `capture`, `vision`, `input`, `sim`, `mcp`, `app`, `acceptance`) or `docs`, `thesis`, `ci`,
  `repo`.
- Fill in `.github/pull_request_template.md`.

## 8. Hooks

| Hook | Location | What it does |
|---|---|---|
| Git `pre-commit` | `.githooks/pre-commit` | `cargo fmt --check` + `cargo clippy -D warnings` |
| Git `pre-push` | `.githooks/pre-push` | `cargo test --workspace` |
| Claude Code `PostToolUse` | `.claude/hooks/format-rust.sh` | `rustfmt` on any `.rs` file an agent edits |

Enable git hooks once per clone: `git config core.hooksPath .githooks`.
Do not bypass them with `--no-verify` unless the PR says why.

## 9. Decisions already made (do not re-litigate without a new ADR)

See `docs/adr/README.md`. ADR-0001..0012 come from the thesis; the project owner accepted
them on 2026-10-14. ADR-0013 and 0014 are accepted too; ADR-0015, ADR-0016, ADR-0017,
ADR-0018, ADR-0019, ADR-0020, ADR-0021 and ADR-0022 were accepted on 2026-10-14 too.
Highlights: Rust runtime with independent oracles (0001), MCP over stdio with explicit
profiles (0002), conservative spatial fusion with IoC (0003), masked open-set icon cache
(0004), persistent tracking instead of cell hashes (0005), transactional snapshots (0006),
fail-closed actuation and honest simulation (0007), Windows 11 first (0008), models and
licences chosen in the S0 spike (0009), performance budgets subject to measurement (0010),
modular monolith with adaptive incrementality (0011), data minimisation (0012), bilingual
documentation (0013), the hexagonal crate layout (0014), the MCP transport on `rmcp` wire
types with a bounded message loop (0015), bounded `unsafe` for the Windows
capture and input adapters (0016), UI Automation evidence fused with the visual path,
with no `unsafe` of our own (0017), `type` in two phases whose keys wait for
element-level keyboard focus (0018) and the perception stages of one observation running
concurrently on scoped threads (0019), the operator's scope on windows (0020) and every
loaded artefact pinned before anything loads, with screen and agent text kept out of errors
(0021) and the supervised test clients' point grounding before every click, with a closed
list of navigating clicks decided by the operating system's state (0022), and the age bound of every authorisation, the snapshot's and the accessibility evidence's, checked at every real-input boundary (0025, accepted on 2026-10-15), and optional target-window freshness with whole-frame geometry and window-group identity checks, off by default, with refusal counts in MCP `_meta` only (0032, accepted on 2026-10-15), and an external decision model only in an offline experiment on pinned synthetic scenes, never on live observations and never as a safety mechanism (0033, accepted on 2026-10-17), and the retained state of a session documented with its bounds and no budget: the request journal and the accessibility evidence keep their growth, disclosed, and a budget needs a new ADR (0034, accepted on 2026-10-17 as documentation only), and the target-window group limited to the windows above the target, still behind the switch that is off by default (0035, accepted on 2026-10-17; live use still needs the validation its ADR requires), and an LLM-only agent experiment (AG-02) with only the three Ommatix tools, whose observations of the two test applications go to Anthropic's API as the agent's input, in a dry-run phase and then a budgeted live phase behind a proxy that the operator's stop file never depends on (0036, accepted on 2026-10-17; exploratory, no box).
