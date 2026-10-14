# B0003. Hexagonal modular monolith as a Cargo workspace

- **Status:** Superseded by [ADR-0014](../0014-hexagonal-crate-layout.md) and [ADR-0011](../0011-modular-monolith-and-adaptive-incrementality.md) on 2026-10-14
- **Previous status:** Accepted
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

We need strong decoupling (capture backends, detectors and agent protocols will change
often) without the operational cost of services: perception must be in-process to meet
latency goals. The structure must also stay low-friction for a small team.

## Decision

A **modular monolith**: one binary, built from a Cargo workspace under `src/`, organised as
**ports and adapters**:

| Layer | Crate | Role |
|---|---|---|
| Domain | `ommatix-domain` | Pure types and algorithms, no I/O |
| Application | `ommatix-application` | Ports (`FrameSource`, `ElementDetector`) + `PerceptionEngine` use case |
| Driven adapters | `ommatix-adapter-capture`, `ommatix-adapter-vision` | Implement ports |
| Driving adapter | `ommatix-adapter-mcp` | Calls the use case |
| Composition root | `ommatix` (`src/app`) | Wires concrete adapters, CLI |

Boundaries are enforced by **crate dependencies**, so a violation is a compile error rather
than a review comment. To limit friction: driving adapters call the engine directly (no
inbound port traits), and a new crate is created only when a second implementation is
realistic.

## Consequences

- Swapping a technology means writing one adapter; domain and use cases are untouched.
- The domain is trivially unit-testable with synthetic frames.
- Integration tests compose real adapters without stdio (see `adapter-mcp/tests`).
- More `Cargo.toml` files than a single crate; incremental compilation benefits offset it.
- Dynamic dispatch at port boundaries (once per frame / per region): negligible cost.

## Alternatives considered

- **Single crate with modules** — less boilerplate, but layering is not enforced.
- **Microservices (perception service + agent bridge)** — IPC per frame breaks the latency
  budget and adds deployment burden.
- **Generic ports (`PerceptionEngine<S, D>`)** — zero-cost, but leaks type parameters into
  every driving adapter; revisit only if profiling shows dispatch matters.

## Resumen (ES)

Monolito modular con arquitectura hexagonal: un solo binario construido desde un workspace
de Cargo, donde cada capa (dominio, aplicación, adaptadores, raíz de composición) es un
crate. Las dependencias entre crates hacen que el compilador imponga los límites. Para no
generar fricción, los adaptadores conductores llaman al motor directamente y solo se crea un
crate nuevo cuando hay una segunda implementación realista.
