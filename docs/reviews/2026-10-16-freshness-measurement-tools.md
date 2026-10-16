# Freshness measurement tools: review and regression

## Finding: D0 is an object among action timings

`tools/s4/e2e_driver.py::stage_ms` divided every value in
`io.ommatix/action_timings` by 1000. Since D0 adds the nested `freshness_diff` object,
a freshness refusal could raise `TypeError` before the client saved its action record.

**Resolution.** Convert only numeric `*_us` fields. Record D0 separately through a strict
allowlist of non-negative integer counts and a boolean, only on rejected actions. Missing
group evidence stays absent. Both the reduced protocol and the dossier decoy preserve it;
the menu restoration attempt also retains its own result in the run record.

**Regression.** `test_d0_nested_counts_do_not_break_timing_conversion` failed with
`TypeError: unsupported operand type(s) for /: 'dict' and 'int'` when the old conversion
was restored, and passes with the fix. The client doubles exercise the resulting records
in `test_each_freshness_refusal_keeps_d0_counts_in_the_action_record` and
`test_decoy_freshness_refusals_record_d0_without_screen_data`. The other tests in
`test_freshness_measurement.py` protect omission, types, config isolation, original paths
and the refusal to put the variant inside a repository.

## Measurement boundaries

M1 reuses the engine's `WindowRead`/group selection and the domain's exact dirty-tile
comparison. Its Windows reader shares the actuator's OS query functions but never
constructs a live actuator; its delivery method always returns an error. Synthetic Rust
tests cover tile boundaries, one-hop grouping, overlapping/background coverage, missing
rectangles, geometry changes, the required operator criterion and the class/application,
stop and pin gates. The product writes no files (SEC-04): the operator-side Python driver validates the numeric
report before saving it outside any repository. No new dependency or stage is introduced into the perception/action
pipeline; the actuator's existing query bodies are factored without extra OS reads or
allocations on that path. Live per-stage measurements remain for the owner; none is claimed.

No screen was captured, no engine ran live and no input was sent in this work. M1, M2 and
IT-08 remain unexecuted; no requirement status or chapter 06 §6.7 checkbox changes.

## Resumen (ES)

La conversión de tiempos fallaba ante el objeto D0 y podía impedir guardar el rechazo.
La regresión falla con la conversión anterior y pasa al separar tiempos y recuentos.
M1 reutiliza la agrupación del motor con un lector incapaz de entregar entrada; las pruebas
usan únicamente fotogramas sintéticos y dobles. M1, M2 e IT-08 siguen sin ejecutar.
