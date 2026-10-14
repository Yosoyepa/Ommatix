# 0019. Run the perception stages of one observation concurrently

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Project owner, 2026-10-14 ("RNF-05 performance first"); this ADR is scoped to the
  stages of one observation and to nothing else

## Context

RNF-05 asks for an incremental perception latency of p95 ≤ 150 ms. On the reference machine,
with DirectML, an observation after a change cost the sum of its stages
(`docs/thesis/S0_RESULTADOS.md` §9.4 and §10): about 115 ms of detection per planned region,
about 60 ms for the UI Automation read and 3 to 8 ms of OCR, so 184 ms with one region and
298 ms with two.

The three stages read the same captured frame and the same region plan, and none of them
needs another's result. SDD §2.7 already says that "capture, detector and OCR can work
concurrently, but their results are tagged with the frame and the configuration", with one
coordinator as the only writer of the published state. The code ran them one after the other.
Chapter 09 §9.6 says that overlapping stages must not be summed and must not be called
wall-clock time: the total is measured directly.

`AGENTS.md` forbids threads in the domain (it is pure computation), forbids the application
to read the system clock or sleep, and says nothing against threads in the application.
Whether the application may start threads is an architectural question, so it is decided here.

## Decision

1. **The application may start scoped threads, for the stages of one observation and for
   nothing else.** `Engine::observe` runs the detector, the recogniser and the accessibility
   provider with `std::thread::scope`, one thread per stage that has work (the calling thread
   takes the accessibility read), and joins all of them before anything is fused. No new
   dependency: standard library threads only, no pool, no async runtime, no channel, no
   shared mutable state. The domain stays free of threads and of anything that touches the
   clock. The adapters stay independent of each other; they only see the `Send` bound that
   their port traits already had.
2. **One writer, collected by stage.** The coordinator is still the only writer of the
   candidate snapshot. Each stage returns its own value, and the coordinator reads them in a
   fixed order (detector, recogniser, accessibility), never in order of arrival, then sorts
   and fuses exactly as before. The same frames give the same elements, identifiers and JSON
   in both modes.
3. **Failure is the same as before.** Every started stage is waited for. The first failure in
   the fixed order ends the observation with `INFERENCE_FAILED`, nothing is published, and the
   planned tiles stay pending. A stage that panics goes on unwinding in the caller, as it did
   when the stages ran in sequence.
4. **A switch, on by default.** `EngineConfig::concurrent_stages` (`[engine] concurrent_stages`
   in the configuration, `true` by default) runs the stages one after the other, stopping at
   the first failure. It exists to diagnose and to measure before and after.
5. **Timings.** Each stage reports its own duration, measured on the injected `Clock`. The
   three overlap, so they need not add up to `total`, which is the wall-clock time of the
   whole observation. The contract's field names and the schemas do not change; `_meta`
   timings are run-dependent and never part of `structuredContent`.

## Alternatives considered

- **Keep the stages in sequence.** The sum of their durations is the latency; with the
  detector's float16 export (S0 §10) one region would cost about 140 ms (measured) and every
  further region 65 ms more.
- **Concurrency inside the adapters.** The detector cannot start the accessibility read: the
  adapters do not know each other, and the overlap is a property of the pipeline, not of one
  technology.
- **A thread pool crate or an async runtime.** A new dependency and a new kind of failure for
  three stages per observation. Starting a thread costs tens of microseconds, against
  tens of milliseconds of work.
- **Pipelining across observations** (capture the next frame while analysing this one). It
  changes what "the last committed base" means (Algorithm 1, rule 2 of `AGENTS.md`) and needs
  its own decision.
- **Batching the regions into one inference.** Measured and rejected in S0 §9.5.

## Consequences and cost

- An observation costs about the longest of its stages instead of their sum: 184 → 117 ms with
  one region and 298 → 226 ms with two, before any model change (S0 §10.4).
- Adapters are called from a thread other than the coordinator's. The UI Automation adapter
  already works on a thread of its own behind channels; the Windows OCR engine is agile; the
  ONNX Runtime session is `Send`. A new adapter must not assume it runs on the thread that
  created it.
- The stages now compete for the machine. On the reference GPU the detector's float16 kernels
  delay UI Automation (S0 §10.5): the gain is smaller than the stage times suggest when the
  observed application repaints continuously.
- Tests that count calls on a shared double still hold, because the doubles lock their state;
  a test that needs the stage order must not rely on it.

## Required validation

- `concurrent_and_sequential_stages_publish_identical_snapshots`: the same scripted session
  (full refresh, one region, two regions, nothing) gives the same pages and the same evidence
  in both modes.
- `a_failed_stage_fails_the_observation_the_same_way_in_both_modes` (UT-14 in both modes).
- `the_three_stages_of_one_observation_overlap` and `sequential_stages_never_overlap`.
- `a_stage_timing_is_the_stage_own_duration`.
- PERF-02: before and after per-stage timings in `docs/thesis/S0_RESULTADOS.md` §10.4.

## Reversibility

Set `concurrent_stages = false`, or delete the `thread::scope` branch in
`src/application/src/engine/observe.rs`. Nothing else depends on it. Revisit this decision if a
stage turns out not to be safe to call from another thread, or if the overlap cannot be shown to
pay on the supported hardware.

## Provenance

SDD §2.7 (concurrency and transactional publication), §2.10 (performance and degradation);
chapter 09 §9.6; `docs/thesis/S0_RESULTADOS.md` §9 and §10. Refines nothing in ADR-0010, which
keeps the budgets as targets until measured.

## Resumen (ES)

Las tres etapas de percepción de una observación (detección, OCR y lectura de accesibilidad)
leen el mismo frame y no dependen entre sí, así que se ejecutan en hilos acotados dentro de la
capa de aplicación (`std::thread::scope`) y se unen antes de fusionar. Sigue habiendo un único
escritor, los resultados se recogen por etapa y no por orden de llegada, y el dominio no usa
hilos. Aprobada por el responsable el 14 de octubre de 2026 («rendimiento de RNF-05 primero»),
con ese alcance. `[engine] concurrent_stages = false` vuelve al orden en serie.
