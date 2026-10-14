# 0014. Hexagonal crate layout for the modular monolith

- **Status:** Accepted
- **Date:** 2026-10-14
- **Approval:** Project owner, at the repository bootstrap

## Context

[ADR-0011](0011-modular-monolith-and-adaptive-incrementality.md) keeps the MVP in one process.
The SDD ([thesis chapter 02 §2.3–2.4](../thesis/02_SDD_ARQUITECTURA_MVP.md)) defines eleven
components and the ports `FrameCapturer`, `VisionDetector`, `TextRecognizer`, `OsActuator`,
`Clock` and `PolicyStore`, so that adapters can be replaced by deterministic fixtures. When
the repository was bootstrapped, the project owner asked for a highly decoupled modular
monolith with a hexagonal architecture. The boundaries should be enforced by the compiler
rather than by review.

## Decision

One binary, built from a Cargo workspace under `src/`. Each layer of the hexagon is a crate,
and the crate dependencies are the boundaries:

| Layer | Crate | SDD components |
|---|---|---|
| Domain | `ommatix-domain` | Algorithms and types of Spatial Fusion, Icon Resolver and Temporal Registry; tiles, regions, snapshots |
| Application | `ommatix-application` | Observation Coordinator, Policy & Action Gate, Effect Verifier; the port traits |
| Driven adapters | `ommatix-adapter-*` | Capture Provider, Perception Provider, OS Actuator, policy store, telemetry sinks |
| Driving adapter | `ommatix-adapter-mcp` | MCP Adapter and the wire DTOs |
| Composition root | `ommatix` (`src/app`) | Wiring and CLI; the only place that picks concrete adapters |
| Acceptance | `ommatix-acceptance` | Executable Gherkin scenarios (test crate, never linked into the binary) |

Rules:

- The domain does no I/O, starts no threads, reads no clock and does not serialise. It depends
  on `thiserror` and on pure-computation crates the algorithms need (for example an R*-tree for
  ADR-0003), each listed in AGENTS.md §3 by the PR that adds it.
- The application depends on the domain only; time comes through the `Clock` port.
- An adapter depends on the application and the domain, never on another adapter.
- Driving adapters call the use cases directly; there are no inbound port traits.
- A new crate exists only when a second implementation is realistic; otherwise it is a module.

Port names follow the SDD. The prototype's `FrameSource`, `ElementDetector` and
`InputController` are renamed or replaced when S1 and S3 reach them.

## Alternatives considered

- **One crate with modules:** less boilerplate, but nothing enforces the layering.
- **Microservices:** ruled out by ADR-0011.
- **Generic ports (`Engine<S, D>`):** no dynamic dispatch, but the type parameters leak into
  every driving adapter. Revisit only if profiling shows that dispatch matters.

## Consequences and cost

Replacing a technology means writing one adapter; the domain and the use cases do not change.
The domain can be unit-tested with synthetic frames, and the oracle's tests can be ported to it
one by one. There are more `Cargo.toml` files than in a single crate. Dynamic dispatch happens
once per frame or per region, which is negligible next to inference.

## Required validation

A forbidden dependency fails the build, because the crate is not declared. Reviews check new
dependencies against AGENTS.md §3.

## Reversibility

Crates without a second implementation in sight can be folded back into modules.

## Provenance

The project owner's request at the bootstrap; refines ADR-0011. Supersedes bootstrap
[B0003](bootstrap/B0003-hexagonal-modular-monolith.md).

## Resumen (ES)

Un solo binario construido desde un workspace de Cargo; cada capa hexagonal es un crate y las
dependencias entre crates son los límites: dominio (algoritmos de fusión, íconos y registro
temporal), aplicación (coordinador, compuerta de acciones, verificador de efectos y puertos),
adaptadores conducidos y conductor (MCP), raíz de composición y crate de aceptación. El dominio
no hace I/O, ni lee reloj, ni serializa. Los nombres de los puertos siguen el SDD de la tesis.
