# 2026-10-15: a stage-timing test raced on the shared test clock

## Finding

`engine::tests::concurrency::a_stage_timing_is_the_stage_own_duration` failed once in CI
(PR #57, job `fmt · clippy · test`): with `concurrent_stages = true` the recognizer's timing
was 10 ms instead of 0. The stages of one observation share the injected `ManualClock`; the
test's detector advances it by 10 ms on its own thread while the recognizer may be timing
itself on another, so the recognizer sees zero or the detector's tick depending on the
scheduler. The engine is correct (each stage is timed from its own start to its own end on
the injected clock, ADR-0019); the test's expectation of exactly zero in the concurrent case
was not.

## Resolution

The test keeps the exact expectations for the sequential case and for the detector and the
total in both cases, and in the concurrent case accepts zero or the detector's tick for the
recognizer, never more. No production code changes.

## Regression test

`a_stage_timing_is_the_stage_own_duration` itself (30 consecutive local runs pass). The
interleaving cannot be forced from the test without changing the engine, so there is no test
that fails deterministically without the change.
