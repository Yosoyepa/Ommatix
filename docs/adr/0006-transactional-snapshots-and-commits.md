# 0006. Transactional snapshots and commits

- **Status:** Proposed (thesis baseline)
- **Date:** 2026-10-13
- **Approval:** Pending

## Context

Clearing regions, or updating the base frame, before inference finishes can publish
inconsistent evidence [F0, pp. 18–20].

## Decision

One writer, immutable snapshots and staging. Publish a coherent registry, index and base in
one commit; a failure keeps the pending dirty regions. Actions apply to the current snapshot.

## Alternatives considered

- Per-region mutation without a transaction.
- Readers that see partially updated state.

## Consequences and cost

Temporary memory grows, and actuation can be blocked while the engine is degraded. Areas that
were not processed are never presented as fresh.

## Required validation

UT-13..UT-15, plus the runtime's concurrency and cancellation tests.

## Reversibility

Optimise the granularity only after the publication and freshness invariants are preserved.

## Provenance

New proposal [P], derived from findings A-09 and A-12 of the
[technical audit](../thesis/00_AUDITORIA_TECNICA.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** Borrar regiones o actualizar frame base antes de terminar inferencia puede
  publicar evidencia inconsistente [F0, pp. 18–20].
- **Decisión:** Un escritor, snapshots inmutables y staging. Publicar registro/índice/base
  coherentes en un commit; fallo conserva dirty pendiente. Acción sobre snapshot vigente.
- **Alternativas consideradas:** Mutación por región sin transacción; lectores con estado
  parcialmente actualizado.
- **Consecuencias y coste:** Incrementa memoria temporal y puede bloquear actuación durante
  degradación. Evita presentar zonas no procesadas como frescas.
- **Validación exigida:** UT-13..UT-15 y pruebas de concurrencia/cancelación del runtime.
- **Reversibilidad:** Optimizar granularidad sólo tras preservar invariantes de publicación y
  vigencia.
- **Procedencia:** Propuesta nueva [P] derivada de A-09/A-12.
