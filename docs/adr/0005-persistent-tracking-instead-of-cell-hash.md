# 0005. Persistent tracking instead of a cell hash

- **Status:** Proposed (thesis baseline)
- **Date:** 2026-10-13
- **Approval:** Pending

## Context

The FNV ID modulo 90000 changes at cell borders and collides, even with duplicated seeds in
one cell [F0, pp. 13–15].

## Decision

Assign monotonic IDs per session, never reused. Match one-to-one over time, with context, and
use revisions for changes. The tile size plays no part in identity.

## Alternatives considered

- UUIDs without matching.
- A signature hash.
- Native accessibility IDs.
- Neighbours by position.

## Consequences and cost

A real matcher and tombstones are needed. A stable ID does not authorise acting on an old
observation. Silent re-anchoring by proximity is removed.

## Required validation

UT-32..UT-41, virtualisation, duplicates and geometry changes. The oracle only tests the
registry with a supplied association.

## Reversibility

Add an accessibility ID or a more robust matcher without changing the external API.

## Provenance

S20. Replaces the original ADR-005 in [F0], which is not kept as valid.

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** El ID FNV modulo 90000 cambia en bordes y colisiona, incluso con semillas
  duplicadas en una celda [F0, pp. 13–15].
- **Decisión:** Asignar IDs monotónicos por sesión, sin reutilizar. Matching temporal uno-a-uno
  con contexto; revisiones para cambios. El tile_size deja de intervenir en la identidad.
- **Alternativas consideradas:** UUID sin matching; hash de firma; IDs accesibles nativos;
  vecinos por posición.
- **Consecuencias y coste:** Hace falta un matcher real y tombstones. Estabilidad no autoriza
  ejecutar observaciones viejas. Se elimina reanclaje silencioso por cercanía.
- **Validación exigida:** UT-32..UT-41, virtualización, duplicados y cambios de geometría. OR
  sólo prueba el registro con asociación suministrada.
- **Reversibilidad:** Incorporar ID de accesibilidad o matcher más robusto sin cambiar API
  externa.
- **Procedencia:** S20; sustituye ADR-005 original, no lo conserva como válido.
