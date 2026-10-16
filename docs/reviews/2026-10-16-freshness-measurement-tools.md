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

## Blocking finding P1: a symlinked config can move the operator's stop

The independent review found that `freshness_config` called `resolve()` on the supplied
configuration before rebasing its paths. For `C:\operator\live.toml` linked to
`C:\templates\shared.toml`, a relative `stop_file = "STOP"` changed from
`C:\operator\STOP` to `C:\templates\STOP`. `Config::load` retains the supplied parent,
so M2 could ignore an already-engaged operator stop. The same bug affected pin, purge,
model and prototype paths. Both clients use the helper.

**Resolution (architect's decision).** Reject a supplied config-file symlink before
creating a run folder or copy. Base every relative operator path on the absolute form of
the supplied config directory, retaining lexical parent components and parent links.
Resolve only to verify equivalence; write the lexical absolute path to preserve the
original lookup semantics even when a parent/file link later changes. A resolution
failure, invalid path or unrecognised path field aborts before any copy is written.
The source file is never modified, and the helper never reads or clears stop/pin values.

**Fail-first regressions.** `test_symlinked_configuration_is_refused_before_copying_in_both_clients`
failed with `Abort not raised` for each client before the correction;
`test_unresolvable_operator_path_aborts_before_writing_in_both_clients` exposed an
uncaught resolution failure. Both pass after the correction. Deterministic path doubles
avoid requiring Windows symlink privileges. The companion tests
`test_relative_stop_and_pin_keep_the_supplied_config_directory_in_both_clients`,
`test_rebasing_keeps_lexical_parent_components_and_all_current_path_keys` and
`test_unmapped_or_invalid_operator_paths_are_refused_before_copying` protect an engaged
stop, the original pin, every current path field, lexical parents and refusal before writes.

## CI finding: the guide validator misattributes Python flags

`--exe` was already defined by `freshness_probe.py`, but the guide validator did not
register that program and treated a command containing `ommatix.exe` as a Rust CLI
invocation. The existing `test_every_option_they_use_is_defined_by_the_program_it_belongs_to`
failed locally as in CI. Register M1 and both S4 clients, join PowerShell as well as cmd
continuations, and explicitly declare all target flags in the Python parser. The existing
guide test now passes; `test_powershell_continuations_keep_python_flags_with_their_own_program`
protects complete command ownership across continued lines.

## Owner's M1 protocol

M1 is descriptive: 60 pairs, 30 menu closed and 30 menu open, with no pass threshold.
`--descriptive` omits both `min_outside_pairs` and `criterion_met` from the numeric report;
the optional future pass criterion has no default and conflicts with descriptive mode.
`test_descriptive_mode_neither_supplies_nor_accepts_a_pass_threshold` failed before this
mode was added and now exercises it with a process double. Rust also checks the exclusive
CLI modes without any capture. No measurement was executed by these tests.

## Measurement boundaries

M1 reuses the engine's `WindowRead`/group selection and the domain's exact dirty-tile
comparison. Its Windows reader shares the actuator's OS query functions but never
constructs a live actuator; its delivery method always returns an error. Synthetic Rust
tests cover tile boundaries, one-hop grouping, overlapping/background coverage, missing
rectangles, geometry changes, the explicit operator protocol and the class/application,
stop and pin gates. The product writes no files (SEC-04): the operator-side Python driver validates the numeric
report before saving it outside any repository. No new dependency or stage is introduced into the perception/action
pipeline; the actuator's existing query bodies are factored without extra OS reads or
allocations on that path. Live per-stage measurements remain for the owner; none is claimed.

No screen was captured, no engine ran live and no input was sent in this work. M1, M2 and
IT-08 remain unexecuted; no requirement status or chapter 06 §6.7 checkbox changes.

## Resumen (ES)

La conversión de tiempos fallaba ante el objeto D0 y podía impedir guardar el rechazo.
La regresión falla con la conversión anterior y pasa al separar tiempos y recuentos.
La revisión P1 detectó que canonicalizar una configuración enlazada podía mover el stop:
ahora se rechaza el enlace y todas las rutas conservan el directorio suministrado; las
regresiones fallaron antes del arreglo y cubren ambos clientes. El validador de guías
reconoce las herramientas Python y sus continuaciones PowerShell. M1 queda descriptiva,
con 30 + 30 pares y sin umbral, por decisión del responsable.
M1 reutiliza la agrupación del motor con un lector incapaz de entregar entrada; las pruebas
usan únicamente fotogramas sintéticos y dobles. M1, M2 e IT-08 siguen sin ejecutar.
