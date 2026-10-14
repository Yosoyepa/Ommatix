# 0010. Performance budgets subject to measurement

- **Status:** Proposed (thesis baseline)
- **Date:** 2026-10-13
- **Approval:** Pending

## Context

The millisecond and percentage figures in the source conversation come without traces or
hardware [F0, pp. 20–27].

## Decision

Measure core, perception, transport, action and the agent cycle separately. Initial targets
are measurable, with real histograms and paired baselines. No item of the definition of done
is checked without evidence.

## Alternatives considered

- A universal sub-10 ms SLA.
- Measuring serialisation time only.

## Consequences and cost

Measurement may show that the original target is not met for some workloads. That improves
scope decisions, and it is not hidden behind averages.

## Required validation

PERF and EV tests on frozen hardware and a frozen corpus, with intervals and denominators.

## Reversibility

Update the budgets through an ADR before the final test set is used, and keep their history.

## Provenance

New proposal [P], based on the dossier's methodology
([thesis chapter 09](../thesis/09_EVALUACION_EXPERIMENTAL.md)).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** Las cifras de ms y porcentajes del PDF no tienen trazas ni hardware
  [F0, pp. 20–27].
- **Decisión:** Separar core, percepción, transporte, acción y ciclo del agente. Metas iniciales
  medibles, histogramas reales y baselines emparejados. No marcar DoD sin evidencia.
- **Alternativas consideradas:** SLA sub-10 ms universal; medir sólo tiempo de serialización.
- **Consecuencias y coste:** Puede mostrar que el objetivo original no se cumple en algunas
  cargas. Esto mejora decisiones de alcance y no se oculta con promedios.
- **Validación exigida:** PERF/EV sobre hardware y corpus congelados, con intervalos y
  denominadores.
- **Reversibilidad:** Actualizar budgets por ADR antes de usar test final; mantener históricos.
- **Procedencia:** Propuesta nueva [P], apoyada en metodología del dossier.
