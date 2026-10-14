# AGENTS.md — Ommatix

Rules for anyone changing this repository: human contributors and AI coding agents alike.
Read this file before your first change. If a rule here conflicts with a request, raise it
instead of silently breaking the rule.

## 1. What this project is

Ommatix (*Spatial Indexing and Incremental Perception for GUI Agents*) is a Rust perception
engine. It turns screen frames into a spatially indexed set of GUI elements, re-analysing
only the parts of the screen that changed, and serves that to LLM agents over MCP.
Perception lives here; reasoning lives in the agent. Keep them apart.

## 2. Language policy (English / Español)

| Artefact | Language |
|---|---|
| Code, identifiers, code comments, commit messages, CLI output, MCP tool text | English |
| `README.md`, `docs/architecture/overview.md` | English, with a Spanish twin `*.es.md` |
| ADRs (`docs/adr/`) | English body + mandatory `## Resumen (ES)` section |
| Diagrams | English labels; captions may be bilingual |
| Issues / PR descriptions | Either language |

When you change an English doc that has a `*.es.md` twin, update the twin in the same PR
(see ADR-0007).

## 3. Architecture: hexagonal modular monolith

One binary, several crates. Each crate is a module with a compiler-enforced boundary.

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
| `ommatix-domain` | `thiserror` only. No I/O, no threads, no clocks, no serialisation, no other Ommatix crate |
| `ommatix-application` | `ommatix-domain` + `thiserror`. No adapter crate; time only through the `Clock` port |
| `ommatix-adapter-*` | `ommatix-application`, `ommatix-domain`, their own external libs. **Never another adapter** (dev-dependencies for integration tests are fine) |
| `ommatix` (app) | everything; it is the only place where concrete adapters are chosen |
| `ommatix-acceptance` | anything, as dev-dependencies only (test crate, never linked into the binary) |

- New external capability (OCR, input injection, a new capture backend) = a **port trait in
  `application/src/ports.rs`** + a **new adapter crate** under `src/adapters/`.
- Do not create a crate for something that has no second implementation in sight; add a
  module instead. Hexagonal boundaries are for swapping technology, not for ceremony.
- Wire DTOs (JSON shapes) live in the adapter that owns the wire format (`adapter-mcp/src/dto.rs`),
  never in the domain or application: neither depends on `serde`.
- Use cases receive their collaborators through constructors (`Box<dyn FrameSource>`,
  `Arc<dyn Clock>`) or method parameters typed as traits (`&dyn ScreenModel`, the read model
  `ActuationService` validates against); they never construct adapters, read the system
  clock or sleep on their own.
- One adapter may implement several ports when they share state by nature (the simulated
  desktop); it still depends only on the application and domain.

## 4. Where things go

| You are adding… | Put it in |
|---|---|
| Geometry, grid, scene, indexing logic | `src/domain/src/` |
| A use case or a port trait | `src/application/src/` |
| Code that touches the OS, files, network, models or a protocol | `src/adapters/<name>/` |
| CLI flags, wiring, config defaults | `src/app/src/` (`cli.rs`, `wiring.rs`) |
| An architectural decision | `docs/adr/NNNN-title.md` (copy `0000-template.md`) |
| A diagram | `docs/diagrams/*.md` as Mermaid |
| A code review | `docs/reviews/YYYY-MM-DD-topic.md`: finding → resolution → regression test |
| Acceptance criteria (Gherkin) | `src/acceptance/features/*.feature` + steps in `src/acceptance/tests/acceptance.rs` |
| Unit tests | next to the code (`#[cfg(test)] mod tests`) |
| Cross-crate / end-to-end tests | `src/<crate>/tests/` |

Nothing MVP-related goes in the repository root except workspace configuration.

## 5. Non-negotiable engineering rules

1. **Determinism.** Same frames ⇒ same elements, ids and JSON. Use `BTreeMap`/`BTreeSet` or
   sort explicitly before anything is emitted; never iterate a `HashMap` into output; no
   randomness in domain or application. Time comes from the injected `Clock` (`ManualClock`
   in tests); run-dependent values (timings) go in MCP `_meta`, never in
   `structuredContent`.
2. **Incrementality.** Never re-process the full frame when only regions changed. Any new
   pipeline stage must take dirty regions as input.
3. **No `unsafe`** (`unsafe_code = "forbid"` at workspace level). Lift it only with an ADR.
4. **No panics on external input.** `unwrap`/`expect` only in tests or on documented
   invariants. Adapters return `PortError`. Geometry saturates; check arithmetic on values
   that come from the wire.
5. **stdout belongs to MCP.** In `serve` mode, diagnostics go to stderr only. Enforced by
   the workspace lint `clippy::print_stdout = "deny"`; only the `demo` printer opts out.
6. **Safety of input.** Live input is opt-in at build time (`native-input`) and at run time
   (`--input native`, which requires `--source screen` so coordinates are mapped to the
   monitor the agent sees). Never make a live `InputController` the default, and never
   bypass `ActuationService` validation. An action that was performed is never reported as
   failed (ADR-0012).
7. **Privacy.** Never commit screen captures, recorded sessions or datasets (`captures/`,
   `datasets/` are git-ignored). Use synthetic frames in tests: `SimulatedDesktop` or
   `ommatix_adapter_sim::scripted_session(theme)`.
8. **Latency is a feature.** Do not add a dependency or an allocation on the hot path
   (`tick()`) without measuring it; report per-stage timings in PRs that touch it
   (`cargo test -p ommatix-adapter-vision --release -- --ignored --nocapture`).
9. Lints: `clippy::pedantic` is on. Fix warnings; `#[allow]` needs a one-line reason.
10. **Regression tests.** Every bug fix ships with a test that fails without it, named
    after the behaviour it protects. Review findings are recorded in `docs/reviews/` with
    the test that covers each one.

## 6. Commands and definition of done

```bash
cargo fmt --all                                        # format
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI-blocking)
cargo test --workspace                                 # unit + integration + Gherkin (CI-blocking)
cargo run -- demo                                      # smoke test of the full pipeline
cargo clippy -p ommatix --features screen,native-input --all-targets -- -D warnings  # when touching capture/input
```

A change is done when fmt, clippy and tests pass, any new user-visible behaviour has a
Gherkin scenario, docs (and their `*.es.md` twins) reflect
the change, and any architectural decision has an ADR.

### Toolchain

Rust stable (see `rust-toolchain.toml`, MSRV in `Cargo.toml`). The `screen` and
`native-input` features on Linux need: `libwayland-dev libpipewire-0.3-dev libgbm-dev libegl1-mesa-dev libxcb1-dev
libxrandr-dev libdbus-1-dev clang`.

## 7. Git workflow

- `main` is protected: change it through pull requests only.
- Branches: `feat/…`, `fix/…`, `docs/…`, `chore/…`, `refactor/…`.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org):
  `feat(domain): add region expansion`. Scope = crate short name (`domain`, `application`,
  `capture`, `vision`, `input`, `sim`, `mcp`, `app`, `acceptance`) or `docs`, `ci`, `repo`.
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

See `docs/adr/README.md`. Highlights: Rust core (0002), hexagonal modular monolith (0003),
MCP over stdio (0004), grid-based incremental perception with uniform-grid index (0005),
bilingual docs (0007), actuation port with dry-run default (0008), Gherkin acceptance
through MCP on a simulated desktop (0009), flat-region detector (0010, supersedes 0006),
invariants of the incremental pipeline (0011), actions that wait for the screen to settle
and a wire contract owned by the MCP adapter (0012).
