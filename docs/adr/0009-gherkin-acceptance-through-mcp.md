# 0009. Executable Gherkin acceptance tests through MCP on a simulated desktop

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

The MVP needs acceptance criteria that evaluators can read and the build can enforce.
Testing the perceive → act → perceive loop needs a desktop that reacts to input, which a
CI runner does not have.

## Decision

- Acceptance criteria are Gherkin features in `src/acceptance/features/`, executed with
  `cucumber` (`cargo test -p ommatix-acceptance`, part of `cargo test --workspace`).
- Steps act **only through the MCP adapter** (JSON-RPC), exactly like an agent, so the
  suite covers protocol, wiring, use cases and domain together.
- `ommatix-adapter-sim` provides a deterministic simulated desktop (a form with a text
  field, Cancel, Save and a modal dialog) implementing both `FrameSource` and
  `InputController` over shared state. Steps may read its state only as ground truth.
- Feature files are written in English with a `# Resumen (ES)` comment (ADR-0007).

## Consequences

- The specification is the test: a scenario cannot drift from behaviour.
- The simulator is also usable interactively (`ommatix serve --source sim`).
- The simulator is not a real GUI toolkit; real-screen validation remains a manual step
  (and later, recorded sessions under `--source images`).

## Alternatives considered

- **Plain `#[test]` integration tests only** — precise but unreadable for evaluators.
- **Headless X server in CI** — realistic, but slow, flaky and platform-specific.

## Resumen (ES)

Los criterios de aceptación del MVP son features Gherkin ejecutables con `cucumber`. Los
pasos hablan con Ommatix solo por MCP, como un agente real, sobre un escritorio simulado
determinista que reacciona a clics y texto. La validación en pantalla real sigue siendo
manual.
