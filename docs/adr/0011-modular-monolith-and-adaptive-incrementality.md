# 0011. Modular monolith and adaptive incrementality

- **Status:** Proposed (thesis baseline)
- **Date:** 2026-10-13
- **Approval:** Pending

## Context

The source conversation offers shared memory, Redis and binary formats as alternatives
[F0, pp. 2–4].

## Decision

One process for the MVP, with explicit ownership or leases of buffers and bounded queues.
Use the native change metadata of the capture API and a cost selector between a full refresh
and regions of interest. No Redis, and no frames sent between processes as JSON.

## Alternatives considered

- Microservices.
- `mmap` between processes.
- GPU zero-copy from the start.

## Consequences and cost

Fewer operational failures and fewer interfaces. Zero copies between GPU and CPU are not
guaranteed. Isolation between processes stays open, in case it is ever needed.

## Required validation

A profile of copies and queues, failures, regions and the real cost of tensor shapes.

## Reversibility

Split adapters into processes if evidence about stability or isolation requires it.

## Provenance

S06 and S09–S10. Qualifies the absolute ban on sockets in [F0]. The crate layout of the
monolith is [ADR-0014](0014-hexagonal-crate-layout.md); the incremental algorithm is in
[thesis chapter 04](../thesis/04_ALGORITMOS_CORE.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** El PDF propone memoria compartida, Redis y formatos binarios como alternativas
  [F0, pp. 2–4].
- **Decisión:** Mismo proceso para el MVP, propiedad/leases de buffers y colas acotadas. Usar
  metadatos nativos y selector coste full/ROI. Sin Redis ni IPC de frames en JSON.
- **Alternativas consideradas:** Microservicios; mmap entre procesos; GPU zero-copy desde el
  inicio.
- **Consecuencias y coste:** Menos fallos operativos y menos interfaces. No garantiza cero
  copias GPU/CPU. El aislamiento entre procesos queda pendiente si es necesario.
- **Validación exigida:** Perfil de copias/colas, fallos, regiones y coste real de shapes.
- **Reversibilidad:** Separar adaptadores si evidencia de estabilidad o aislamiento lo requiere.
- **Procedencia:** S06, S09–S10; matiza prohibición absoluta de sockets.
