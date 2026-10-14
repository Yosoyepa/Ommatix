# B0002. Rust for the perception core

- **Status:** Superseded by [ADR-0001](../0001-rust-runtime-and-independent-oracles.md) on 2026-10-14
- **Previous status:** Accepted
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

The core loop (capture → hash every cell → diff → detect in regions → update index) runs
on every frame and must stay within a few milliseconds at desktop resolutions. It must also
be deterministic and safe to run unattended next to an autonomous agent.

## Decision

Implement the whole engine in **Rust (stable, edition 2024)**. Python or other languages
may appear only as external tooling (evaluation scripts), never on the perception path.

## Consequences

- Predictable latency: no GC pauses; zero-cost abstractions for ports (`Box<dyn Trait>` on
  the cold path only).
- Memory safety without `unsafe` (forbidden workspace-wide, see AGENTS.md).
- Single static binary is easy to register as an MCP server.
- Mature crates for capture (`xcap`), images (`image`), serialization (`serde`).
- Steeper learning curve and slower iteration than Python for ML experiments; learned
  detectors will be integrated via ports (e.g. ONNX runtime adapter) rather than rewritten.

Measured on the bootstrap demo (640×400, release build): ~1.3 ms for an unchanged frame,
~1.8 ms for a frame where a modal dialog opens.

## Alternatives considered

- **Python + OpenCV** — fastest to prototype, but GIL and GC make tail latency hard to
  bound; packaging as an MCP server is heavier.
- **C++** — comparable speed, but memory safety and build tooling cost more.
- **Go** — simpler, but GC pauses and weaker SIMD/image ecosystem.

## Resumen (ES)

El motor se implementa en Rust estable. Motivos: latencia predecible sin recolector de
basura, seguridad de memoria sin `unsafe`, binario único fácil de registrar como servidor
MCP y buen ecosistema de captura e imágenes. Los modelos aprendidos se integrarán como
adaptadores detrás de puertos, no reescribiendo el núcleo.
