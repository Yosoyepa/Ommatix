# 0030. Choose between the full refresh and the regional plan with a measured cost rule

- **Status:** Accepted (2026-10-15)
- **Date:** 2026-10-15
- **Approval:** Accepted by the project owner on 2026-10-15, who authorised INC-07 of the paper's
  next-cut plan (an adaptive full/regional policy) and approved this record in advance, on the
  condition that the full refresh stays Algorithm 1's explicit fallback, that the rule only
  triggers it earlier, and that it is on by default only if the measurement supports it.

## Context

Algorithm 1 (dossier chapter 04 §4.2) plans the regions to re-analyse and falls back to the full
refresh when the closure runs out of rounds, when there are more than `max_regions` regions or
when they cover more than `full_refresh_area_ratio` of the frame. The chapter calls these
thresholds a proposal to be replaced by a measured cost model.

The B0/B1 measurements of INC-02 (`docs/thesis/S0_RESULTADOS.md` §13) give the cost model its
first facts, on one machine, one application and one detector (float16, 1280×1280 input,
DirectML):

- Each region is a whole detector inference, with the same input size as the full frame. One
  inference costs about 64 ms for a region and 80 ms for the full frame, so two regions (128 ms)
  cost more than the full refresh.
- Without accessibility, one region saves about 21 % (89.5 → 70.8 ms median) and two regions
  cost 51 % more (89.3 → 134.8 ms). With UI Automation (`uia`), the accessibility read is the
  critical path: one region saves nothing (123.4 → 123.3 ms) and two regions cost 135 % of the
  full refresh's median (123.4 → 166.2 ms).
- With two regions the regional plan publishes about 20 of 75 controls fewer, because the
  detector sees a crop at scale 1.0 and the full frame at 2/3 (§13.10).

So from two regions on, the regional plan is both slower and less complete than the full refresh.

## Decision

1. **A cost rule in Algorithm 1.** After the hard budgets, a plan with `adaptive_min_regions`
   regions or more becomes the full refresh. The rule is `regions >= adaptive_min_regions`, a
   prediction of inferences (one per region) against the single inference of the full frame. It
   is deterministic and depends on nothing but the plan.
2. **The rule only triggers the fallback earlier.** It adds no new way to process the frame: the
   full refresh is the algorithm's explicit fallback. The budgets are checked first and keep their
   reasons (`closure_budget`, `too_many_regions`, `area_budget`); the baseline B0
   (`experimental_baseline`) and a missing base (`no_committed_base`) keep theirs. The set of dirty
   tiles that a failed observation keeps pending is the one of the regional plan.
3. **A new reason in the record.** `FullRefreshReason::AdaptiveCost`, `adaptive_cost` in
   `io.ommatix/plan` and in the observation record. The reason is a closed name of the planner;
   no schema of `structuredContent` changes.
4. **Configuration.** `[engine] adaptive_min_regions` (0 to 1024; `0` switches the rule off).
   `PlanParams::default()`, the reference planner, has the rule off; the operator's configuration
   defaults to `2`, the value that the measurement below supports.
5. **Not decided here.** The rule does not look at the area of a single region, does not consider
   whether accessibility is enabled, and does not discard a detection that fills its crop. They
   are listed as alternatives below and left to a measurement that supports them.

## Alternatives considered

- **Full refresh from one region on when `uia` is on.** With `uia` one region saves nothing
  (+2.4 ms in §13.11) and in the run of §16 it cost +7.4 ms [+5.7; +9.0] against the full refresh,
  so the measurement now shows a small loss. Not adopted here: the rule would make the engine
  depend on which evidence providers are configured, and the same one-region plan saves about
  21 % without `uia` (§13.3). An operator with `uia` can set `adaptive_min_regions = 1`; that
  configuration was not measured. Left to a later decision.
- **A per-region area threshold.** The detector's input is 1280×1280 whatever the crop; the
  existing `full_refresh_area_ratio` already covers the total area. Rejected: no measurement
  separates the cost by area.
- **A learned or online cost model.** Rejected: it is not deterministic across runs (rule 1 of
  AGENTS.md §5).
- **Dropping a detection whose box fills its crop.** A quality guard, not a cost rule; it needs its
  own evidence (§13.10, recommendation).
- **Leave the rule on by default regardless of the measurement.** Rejected by the owner's condition.

## Consequences and cost

- With two or more regions, the engine processes one inference of the whole frame, loses none of
  the elements that the full frame finds, and costs the full refresh's latency instead of the
  regional plan's. Incrementality remains for one region and for observations with nothing to do.
- The wire contract does not change; a client that reads `io.ommatix/plan` sees a new name.
- Rule 2 of AGENTS.md §5 (incrementality) is kept in letter: the full refresh is Algorithm 1's
  explicit fallback, and here it is taken for a stated, measured cost reason.
- The threshold is a measured value for one detector and one machine. A faster detector on crops
  or a batched inference would change the cost model; the key makes that a configuration change.

## Required validation

- Domain, application and configuration tests `inc_07_*` (`src/domain/src/incremental.rs`,
  `src/application/src/engine/tests.rs`, `src/app/src/config.rs`).
- The live no-input measurement of `docs/thesis/S0_RESULTADOS.md` §16 (adaptive against B1 and B0,
  with `uia`, one and two regions), run with the protocol of chapter 09 §9.9.

## Measurement

Live, no input, with `uia`, 40 processes per variant and case (`docs/thesis/S0_RESULTADOS.md` §16,
experiment `inc07-adaptive-uia-20261015`, commit `95aba26`): B0 (full frame forced), B1 (regional,
rule off) and AD (`adaptive_min_regions = 2`).

- Two regions: AD is the full refresh in 720 of 720 observations, with one inference. Median 113.2 ms
  and p95 137.6 ms [135.6; 139.5] against 165.9 ms and 186.8 ms for B1; paired difference against B1
  −49.2 ms [−51.3; −47.1], against B0 +1.2 ms [−1.1; +3.5]. AD publishes a median of 79 elements
  (minimum 78), as B0 does; B1 publishes 61.
- One region: AD stays regional in 715 of 720 observations (the other five were two transient
  regions); paired difference against B1 +0.5 ms [−0.1; +1.0].
- Full-refresh control: no difference between B1 and AD (−0.7 ms [−2.9; +1.5]).
- No errors, no discarded or failed processes.

The evidence supports the default of `2` for the configuration measured (`uia`, one GPU, one
detector). It does not show that the incremental path meets RNF-05 with two regions: AD meets the
p95 because Algorithm 1's full refresh is taken earlier. RNF-05's status is the owner's.

## Reversibility

Set `adaptive_min_regions = 0`: the planner is then the one of 1.0.64. Removing the variant
`AdaptiveCost` needs no data migration; a record that names it is a research record only.

## Provenance

Dossier chapter 04 §4.2, chapter 09 §9.9, `docs/thesis/S0_RESULTADOS.md` §13 (INC-02) and the
paper's next-cut plan (INC-07), held outside this repository.

## Resumen (ES)

El Algoritmo 1 planifica las regiones a reanalizar y cae al refresco completo cuando se agota el
cierre, hay demasiadas regiones o cubren demasiada área. Las medidas de INC-02 (S0 §13) dan los
primeros hechos de coste: cada región es una inferencia completa del detector; con dos regiones el
plan regional cuesta más que el refresco completo (+51 % sin accesibilidad y +35 % con `uia`) y
publica unos 20 de 75 controles menos. Esta ADR añade una regla de coste: después de los
presupuestos, un plan con `adaptive_min_regions` regiones o más se convierte en refresco completo,
con la razón nueva `adaptive_cost`. La regla sólo adelanta el respaldo que el algoritmo ya tiene;
los presupuestos, la línea de base B0 y la falta de base conservan sus razones, y el conjunto de
teselas pendientes no cambia. `[engine] adaptive_min_regions` (0 a 1024; 0 la apaga) vale 2 por
defecto en la configuración del producto y 0 en el planificador de referencia. No se decide aquí
nada sobre el área de una región, la accesibilidad ni el descarte de detecciones que llenan su
recorte. El responsable la aprobó el 15 de octubre de 2026 con la condición de que sólo esté
activada por defecto si la medida la sustenta. La medida en vivo con `uia` (S0 §16) la sustenta: con
dos regiones el plan es el refresco completo, −49,2 ms frente a B1 y los mismos elementos que B0
(79 frente a 61); el p95 de dos regiones baja de 186,8 a 137,6 ms, pero porque se toma el refresco
completo, no porque la ruta incremental sea más rápida; el estado de RNF-05 es del responsable.
