# 0003. Conservative spatial fusion with IoC

- **Status:** Proposed (thesis baseline)
- **Date:** 2026-10-13
- **Approval:** Pending

## Context

Small texts inside controls make a high IoU unsuitable as the only membership criterion
[F0, pp. 5–11]. IoC is the asymmetric measure `area(T ∩ U) / area(T)` of a text box `T`
against a candidate owner `U`.

## Decision

Keep IoC, the R*-tree index and the anisotropic cost. Add the most specific owner, grouping
into lines before sorting, context and abstention. Actionability is decided separately from
text.

## Alternatives considered

- Naive geometric matching.
- A semantic model.
- Accessibility as an additional source of evidence.

## Consequences and cost

The index enumerates candidates; it guarantees neither semantics nor accuracy. Abstention can
reduce coverage, so it is evaluated together with precision.

## Required validation

UT-16..UT-26 and a multi-column corpus. Compare the index with a simple implementation to
establish whether it pays off.

## Reversibility

Change the matcher or the source of evidence while keeping the `Candidate` contract.

## Provenance

S17–S19. Replaces the accuracy guarantees of the original ADR-003 in [F0]. The algorithm is
specified in [thesis chapter 04](../thesis/04_ALGORITMOS_CORE.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** Los textos pequeños dentro de controles hacen inadecuado un IoU alto como único
  criterio de pertenencia [F0, pp. 5–11].
- **Decisión:** Conservar IoC, índice R*-tree y coste anisotrópico; añadir propiedad específica,
  agrupación previa de renglones, contexto y abstención. Accionabilidad separada del texto.
- **Alternativas consideradas:** Matching geométrico ingenuo; modelo semántico; accesibilidad
  como fuente adicional.
- **Consecuencias y coste:** El índice enumera candidatos, no garantiza semántica ni exactitud.
  La abstención puede reducir cobertura; se evalúa junto con precisión.
- **Validación exigida:** UT-16..UT-26 y corpus multicolumna. Comparar índice con implementación
  simple para determinar beneficio.
- **Reversibilidad:** Cambiar matcher o fuente de evidencia manteniendo el contrato Candidate.
- **Procedencia:** S17–S19; reemplaza garantías de exactitud de ADR-003 original.
