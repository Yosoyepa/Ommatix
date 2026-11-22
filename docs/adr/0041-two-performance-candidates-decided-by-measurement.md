# 0041. Measure two performance candidates before adopting either: one inference for several regions, and accessibility evidence reused between observations

- **Status:** Proposed
- **Date:** 2026-11-22
- **Approval:** Pending. Draft written by an AI agent at the owner's request. Nothing was measured
  for it; it proposes what to measure and the rule that would decide.

## Context

The recorded measurements (chapter 09; appendix C of the preprint) say where an incremental
observation spends its time.

- **The detector runs once per region.** `OnnxVisionDetector::detect` loops over the regions of the
  plan and runs one inference for each, at the input size `input_for` chooses. Two regions cost more
  than one inference over the whole frame, so [ADR-0030](0030-adaptive-full-or-regional-plan.md) sets
  the adaptive rule to 2 and a plan with two or more regions becomes a full refresh. In the product
  configuration a regional plan therefore has exactly one region, and the limit of four regions only
  decides the reason that is recorded.
- **With UI Automation the accessibility read sets the total.** With one changing region and no
  accessibility, the incremental plan shortens detection from 82.7 to 64.3 ms and text recognition
  from 35.9 to 3.2 ms, and the observation by 18.6 ms. With UI Automation the read lasts about
  117 ms in both arms and hides that gain: the paired difference is +2.4 ms. This is the
  configuration the 150 ms p95 target is defined for, and the one AG-02 used.

Both facts point at the same thing: the incremental planner does its part, and two stages do not
scale with the size of the change. [ADR-0010](0010-performance-budgets-subject-to-measurement.md)
forbids promising a gain; this ADR orders the measurements.

## Decision

Treat each candidate as an experiment behind a switch that stays off, with its decision rule
written before the first run. Adopt a candidate only if its rule is met on the reference hardware.

**Step 0, before either candidate: find out where the 117 ms go.** Extend the observation spans of
[ADR-0024](0024-observation-spans-and-a-switched-full-frame-baseline.md) inside the accessibility read: nodes
visited, calls into UI Automation, time in the walk and time building elements, and the same counts
for a read restricted to one small region. The second candidate is worth building only if the walk,
not the per-element work, takes most of the time.

**Candidate A. One inference for the regions of a plan.** Two variants, measured against the present
loop and against the full refresh:

- *Batch:* the crops of the plan as one batch at one input size, if the exported model accepts a
  batch dimension.
- *Mosaic:* the crops packed into one canvas with a margin between them, one inference, the boxes
  mapped back, and any detection that touches a margin discarded.

Rule to adopt A: on the cohort of ADR-0030, the plan with two to four regions has a lower median and
a lower p95 than the full refresh, and the detections equal those of the per-region loop on the
evaluation set (same roles, boxes within the IoC of [ADR-0003](0003-conservative-spatial-fusion-with-ioc.md)).
If it is met, ADR-0030's rule is measured again and its threshold moves; if not, A is recorded as
tried and dropped.

**Candidate B. Accessibility evidence reused between observations.** Keep the elements of the last
read with the time each was read. On an observation, read again only the elements that overlap a
changed region, and any subtree for which UI Automation reported a structure or property change
since the last read. Three rules make it safe:

1. An event is a reason to read again sooner. The absence of an event is never evidence that
   nothing changed. Events can be late or lost, and providers differ
   ([review of 2026-11-01](../reviews/2026-11-01-uia-hung-provider.md)).
2. Evidence is reused only where the tiles saw no changed pixel since it was read.
3. Reused evidence keeps its original time, so the age bound of
   [ADR-0025](0025-bound-authorisations-by-snapshot-and-evidence-age.md) applies to it unchanged and reuse can never
   lengthen an authorisation.

Rule to adopt B: with UI Automation on and one changing region, the p95 of the incremental
observation is below 150 ms and the paired difference against the full refresh is negative with its
interval excluding zero; and on the supervised scenarios the fused elements equal, observation by
observation, those of a full read taken at the same instant.

## Alternatives considered

- **Lower the adaptive rule's threshold without changing the detector:** ADR-0030 measured that it
  loses.
- **A smaller or faster model:** a different decision, with its own accuracy evaluation
  ([ADR-0009](0009-models-and-licences-decided-in-the-spike.md)).
- **Read accessibility on a background thread all the time:** it would make evidence arrive between
  observations and break the rule that one observation is one consistent read
  ([ADR-0006](0006-transactional-snapshots-and-commits.md)).
- **Trust events alone and skip the pixel condition:** it would turn a lost event into stale evidence
  that still authorises an action.
- **Ask UI Automation for the element at a point for each region:** it finds one element, not the
  set that overlaps a region; it may still serve as a cross-check in step 0.

## Consequences and cost

- If A works, the regional plan is useful with more than one region, which is the case the planner
  was designed for and the present rule turns off.
- If B works, the latency target is met in the configuration it was defined for.
- The mosaic changes what the detector sees around each crop. Equal detections on the evaluation set
  are a condition, not a hope.
- B adds retained state (the elements and their times) and event handlers, both with bounds to
  document as in [ADR-0034](0034-propose-fail-closed-retained-state-budgets.md), and it must stay
  within [ADR-0017](0017-ui-automation-evidence-fused-with-vision.md): no `unsafe` of our
  own, no value read.
- Either may fail its rule. The record of a failed candidate is a result and goes to chapter 09.

## Required validation

- Step 0 produces span data from the reference hardware, recorded with the machine and the commit.
- A: the paired measurement of ADR-0030 repeated with each variant; equality of detections on the
  evaluation set; a deterministic test that the mosaic maps boxes back exactly and drops detections
  on a margin, on synthetic frames.
- B: the paired measurement of chapter 09 with UI Automation; a differential test on scripted
  sessions of the simulated desktop in which the reused evidence equals a full read at every step,
  including steps where the fake provider drops or delays its events; a test that reused evidence
  older than the bound of ADR-0025 refuses the action.
- New planned tests get their identifiers in the matrix when the ADR is accepted.

## Reversibility

Each candidate is a switch that stays off and a module behind it. Turning the switch off restores
the present path. A candidate that fails its rule is removed and its measurement kept.

## Provenance

The owner's request of 2026-11-22 for better performance; the measurements of chapter 09 and of
ADR-0030. Refines ADR-0010, ADR-0017 and ADR-0030.

## Resumen (ES)

Dos etapas no escalan con el tamaño del cambio: el detector hace una inferencia por región y la
lectura de UI Automation tarda unos 117 ms cambie lo que cambie. Se proponen dos candidatos, cada uno
tras un interruptor apagado y con su regla de decisión escrita antes de medir: una sola inferencia
para las regiones de un plan, y reutilizar la evidencia de accesibilidad donde no cambió ningún
píxel, sin alargar nunca su vigencia. Antes de ambos, medir en qué se van esos 117 ms.
