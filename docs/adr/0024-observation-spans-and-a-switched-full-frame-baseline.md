# 0024. Observation spans on the research record, and an experimental full-frame baseline behind a switch

- **Status:** Accepted (2026-10-15)
- **Date:** 2026-10-15
- **Approval:** Accepted by the project owner on 2026-10-15, who approved the implementation of
  INC-01 of the next-cut plan and this record in advance, on the condition that they follow that
  plan exactly.

## Context

The stage timings of an observation (`io.ommatix/timings` in MCP `_meta`, `timings` in the
demo) closed `total` when tracking ended. The commit and the response were measured nowhere, so
the number was not the perception latency that the dossier defines (chapter 03 §3.4: from the
frame's acquisition to the published snapshot). The research protocol needs more than the
timings: the effective plan of each observation (the work kind, the regions and their area, the
reason of a full refresh, the inferences and their input shapes, whether the accessibility read
was truncated) and an identity for every sample. The B0/B1 comparison of INC-02 also needs a full
frame baseline that differs from the product only in the planner's decision.

## Decision

1. **Span boundaries are fixed.** `total` runs from the start of the capture to the end of the
   commit, that is, to the published snapshot, as chapter 03 §3.4 defines it. `commit` runs from
   the end of tracking to the end of the commit. `response_build` starts after the publication
   and ends when the engine has built the first page; it is `null` until then. Serialisation and
   transport are not in any `_meta` they would have to contain: the client measures the round trip.
   Stages that overlap are never summed.
2. **The `_meta` keys.** `io.ommatix/timings` gains `commit_us` and `response_build_us`. A new
   `io.ommatix/plan` carries the work kind (`full`, `regions`, `idle`), the full-refresh reason,
   the region count and area, the inference count and input shapes (`null` when the detector does
   not count its inferences), whether the accessibility read was truncated, and the evidence counts.
   Only numbers, booleans, `null` and the closed names of the planner appear there. The JSON
   schemas of `structuredContent` do not change.
3. **The per-observation record.** `ommatix demo --timings` writes one `ommatix-observation` line
   per observation, a JSON object of schema `ommatix.observation/1`: stage times in whole
   microseconds, the plan, and one signature per element (FNV-1a 64 over role, label and
   actionability). No label is written.
4. **The baseline B0 is a switch.** `[engine] experimental_full_frame` defaults to `false`. When it
   is `true`, the planner computes its tile diff as usual and then forces Algorithm 1's full refresh
   on every observation, with the reason `experimental_baseline`. A note on stderr says that it is
   not a production mode. B0 and B1 are the same binary, the same weights, the same policy and the
   same initial state; the measurement tool refuses any other difference between them.
5. **The detector reports its inferences.** `VisionDetector::inference_shapes` has a default of
   `None`. The ONNX adapter records the shape of each inference of the last `detect` call.

## Alternatives considered

- *Keep `total` at the end of tracking and rename it.* Rejected: the headline latency would then
  disagree with the dossier's definition and with every table that reads `total`.
- *Time the commit inside the payload it publishes.* Impossible: the commit cannot be timed before
  it happens. The timings are kept beside the snapshot and attached after the commit.
- *Put the serialisation of a response in its own `_meta`.* Impossible: a message cannot contain
  the time it takes to finish being written. The client measures it.
- *Make B0 a separate binary.* Rejected: a separate build differs in more than the baseline. A
  configuration key keeps "the same everything else" checkable by the tool.
- *Derive the inference count from the region geometry.* Rejected: the number of model calls is a
  fact of the adapter. Deriving it would present an estimate as a measurement.

## Consequences and cost

- Each observation builds a small plan record and copies two counts. The cost is not yet measured
  on the reference machine; the PR reports the core latency (PERF-01) before and after, and
  the change is reversible (see below).
- Totals recorded before this decision ended at tracking and are not comparable with totals
  from now on. Earlier measurements keep their commit of origin and are not re-attributed.
- The measurement tool (`tools/perf/measure_latency.py`) now keeps the identity of every sample,
  refuses incompatible manifests, duplicate keys and unknown schema versions, and reports paired
  differences over processes. Its agreement figure between B0 and B1 is agreement, not accuracy.
- The experimental switch is one more configuration key. It is announced at start and is never
  set by the product's defaults.

## Required validation

- IT-06: `it_06_an_observation_reports_the_time_of_every_stage_in_meta` and
  `it_06_telemetry_carries_numbers_only_and_nothing_from_the_screen` (the new keys, the closed
  vocabulary of the plan, and nothing from the screen), and `it_06_demo_timings_on_stderr_are_numbers_only`.
- The planner's tests for the switch (`experimental_full_frame_processes_the_whole_frame_even_with_nothing_changed`,
  `experimental_full_frame_does_not_hide_a_missing_base`).
- PERF-01: `cargo run --release -p ommatix-acceptance --example core_latency` before and after the
  change, on the same machine, reported in `docs/thesis/S0_RESULTADOS.md` §11.
- The measurement tool's tests under `tools/perf` (refusals, duplicates, paired analysis, a
  synthetic end-to-end run of the collector).

## Reversibility

Delete the two new `_meta` keys and the plan key, and move `total` back to the end of tracking.
The switch is removed by deleting its planner branch and the configuration key; nothing else reads
it. The inference hook is a defaulted method: removing it leaves every detector without a count.

## Provenance

Dossier chapter 03 §3.4 (perception latency), chapter 07 §7.3 (telemetry only in `_meta`), chapter 09
§9.2 (baselines B0 and B1), §9.4 (instrumentation) and §9.6 (procedure). The next-cut plan of the
paper (INC-01 and INC-02, audit findings R02 to R05), held outside this repository.

## Resumen (ES)

Esta ADR fija los tramos de una observación en el registro de investigación. `total` va desde el
inicio de la captura hasta el fin del *commit*, es decir, hasta el snapshot publicado, como
define el capítulo 03 §3.4; `commit` es la publicación sola, y `response_build` es la
construcción de la primera página después de publicar (`null` hasta entonces). La serialización
y el transporte no caben en el `_meta` que los contiene: el cliente mide la ida y vuelta. Los
tramos solapados no se suman. Las medidas anteriores a 1.0.54 terminaban en el *tracking* y no son
comparables; conservan su commit de origen.

En `_meta`, `io.ommatix/timings` añade `commit_us` y `response_build_us`, y `io.ommatix/plan`
describe el plan de la observación: tipo de trabajo, razón de refresco completo, regiones y
área, inferencias y formas de entrada (`null` si el detector no las cuenta), truncamiento de la
lectura de accesibilidad y recuentos de evidencia. Sólo hay números, booleanos, `null` y nombres
cerrados; ninguna palabra de la pantalla. Los esquemas JSON de `structuredContent` no cambian.

`ommatix demo --timings` escribe un registro JSON por observación (esquema
`ommatix.observation/1`) con los tiempos en microsegundos enteros, el plan y una firma por
elemento, sin etiquetas. La línea de base B0 es un interruptor de configuración,
`[engine] experimental_full_frame`, apagado por defecto: fuerza el refresco completo de
Algoritmo 1 en cada observación y no es un modo de producción. B0 y B1 usan el mismo binario, los
mismos pesos, la misma política y el mismo estado inicial; la herramienta de medida rechaza
cualquier otra diferencia entre ellos. `VisionDetector::inference_shapes` tiene valor por defecto
`None`, y el adaptador ONNX registra la forma de cada inferencia.
