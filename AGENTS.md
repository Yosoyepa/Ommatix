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

**`src/` still holds the bootstrap prototype**, which is being realigned with the thesis in
stages S0–S4 (thesis chapter 06). Do not extend behaviour that only the prototype has (the
flat-region detector, hashed cells, the seven bootstrap MCP tools, coordinates on the wire);
new work implements the thesis. The prototype's decisions are archived in
`docs/adr/bootstrap/`.

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
| `ommatix-domain` | `thiserror`, plus pure-computation crates listed here (none yet). No I/O, no threads, no clocks, no serialisation, no other Ommatix crate |
| `ommatix-application` | `ommatix-domain` + `thiserror`. No adapter crate; time only through the `Clock` port |
| `ommatix-adapter-*` | `ommatix-application`, `ommatix-domain`, their own external libs. **Never another adapter** (dev-dependencies for integration tests are fine) |
| `ommatix` (app) | everything; it is the only place where concrete adapters are chosen |
| `ommatix-acceptance` | anything, as dev-dependencies only (test crate, never linked into the binary) |

- Ports follow the SDD (thesis chapter 02 §2.4): `FrameCapturer`, `VisionDetector`,
  `TextRecognizer`, `OsActuator`, `Clock`, `PolicyStore`. A new external capability is a
  **port trait in `application/src/ports.rs`** plus a **new adapter crate** under
  `src/adapters/`. The prototype's `FrameSource`, `ElementDetector` and `InputController`
  are renamed or replaced as S1 and S3 reach them.
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
| Executable acceptance scenarios | `src/acceptance/features/*.feature` + steps in `src/acceptance/tests/acceptance.rs`. A thesis scenario moves there from `docs/thesis/features/`, with its ID and tags, in the PR that adds its steps |
| Unit tests | next to the code (`#[cfg(test)] mod tests`); a test that implements a planned test carries its ID (`ut_13_failed_commit_keeps_pending_dirty`) |
| Cross-crate / end-to-end tests | `src/<crate>/tests/` |

Nothing MVP-related goes in the repository root except workspace configuration.

## 5. Non-negotiable engineering rules

1. **Determinism.** Same frames ⇒ same elements, ids and JSON. Use `BTreeMap`/`BTreeSet` or
   sort explicitly before anything is emitted; never iterate a `HashMap` into output; no
   randomness in domain or application. Time comes from the injected `Clock` (`ManualClock`
   in tests); run-dependent values (timings) go in MCP `_meta`, never in
   `structuredContent`.
2. **Incrementality.** Compare against the last committed base, not the previous frame, and
   never re-process the full frame when only regions changed. The full refresh is Algorithm
   1's explicit fallback (geometry change, budget exceeded), never a shortcut. Any new
   pipeline stage takes the region plan as input.
3. **No `unsafe`** (`unsafe_code = "forbid"` at workspace level). Lift it only with an ADR;
   the Windows adapters of S3 will need one.
4. **No panics on external input.** `unwrap`/`expect` only in tests or on documented
   invariants. Adapters return `PortError`. Geometry saturates; check arithmetic on values
   that come from the wire.
5. **stdout belongs to MCP.** In `serve` mode, diagnostics go to stderr only. Enforced by
   the workspace lint `clippy::print_stdout = "deny"`; only the `demo` printer opts out.
6. **Fail-closed actuation** (ADR-0007). Every action is a dry run unless the agent asks
   otherwise and the local policy allows it. Never make a live actuator the default and
   never bypass the action gate. Report what happened and no more: `simulated` never claims
   delivery, `input_delivered` never claims an effect, and an uncertain effect is
   `indeterminate`, never an automatic retry. In the prototype, live input needs the
   `native-input` build feature, `--input native` and `--source screen`.
7. **Privacy** (ADR-0012). No screenshots or screen values in logs by default; text on screen
   never changes permissions. Never commit screen captures, recorded sessions, datasets
   (`captures/`, `datasets/` are git-ignored) or model weights. Use synthetic frames in
   tests: `SimulatedDesktop` or `ommatix_adapter_sim::scripted_session(theme)`.
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
cargo run -- demo                                      # smoke test of the prototype pipeline
cargo clippy -p ommatix --features screen,native-input --all-targets -- -D warnings  # when touching capture/input
```

The thesis checks run from `docs/thesis/` (CI job `thesis`, CI-blocking):

```bash
pip install -r requirements-validacion.txt             # once: jsonschema for the validator
python referencia_ejecutable/run_tests.py              # oracle: 48 reference tests
python validar_paquete.py                              # schemas, examples, traceability, Gherkin structure
```

CI also compiles capture and native input for Windows and macOS (job `cross-check`). To run
it locally: `rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin`, then
`cargo clippy --target <target> -p ommatix-adapter-capture -p ommatix-adapter-input
--features ommatix-adapter-capture/screen,ommatix-adapter-input/native -- -D warnings`.

A change is done when fmt, clippy, tests and the thesis checks pass, any new user-visible
behaviour has a Gherkin scenario traced to its requirement, docs (and their `*.es.md` twins)
reflect the change, and any architectural decision has an ADR. The product's definition of
done is in thesis chapter 06 §6.7; its boxes stay unchecked until the evidence exists.

### Toolchain

Rust stable (see `rust-toolchain.toml`, MSRV in `Cargo.toml`) and Python 3.11 or later for
the thesis checks. The first target platform is Windows 11 x64 (ADR-0008). The prototype's
`screen` and `native-input` features on Linux need: `libwayland-dev libpipewire-0.3-dev
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

See `docs/adr/README.md`. ADR-0001..0012 come from the thesis and are proposals used as the
baseline until the project owner accepts them; ADR-0008, 0013 and 0014 are accepted.
Highlights: Rust runtime with independent oracles (0001), MCP over stdio with explicit
profiles (0002), conservative spatial fusion with IoC (0003), masked open-set icon cache
(0004), persistent tracking instead of cell hashes (0005), transactional snapshots (0006),
fail-closed actuation and honest simulation (0007), Windows 11 first (0008), models and
licences chosen in the S0 spike (0009), performance budgets subject to measurement (0010),
modular monolith with adaptive incrementality (0011), data minimisation (0012), bilingual
documentation (0013) and the hexagonal crate layout (0014).
