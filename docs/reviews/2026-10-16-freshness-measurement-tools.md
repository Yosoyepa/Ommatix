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

**First resolution (`09b3fad`; path acceptance narrowed by the next review below).** Reject a supplied config-file symlink before
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
`test_all_current_path_keys_rebase_plain_relatives_against_the_supplied_base` and
`test_unmapped_or_invalid_operator_paths_are_refused_before_copying` protect an engaged
stop, the original pin, every current path field and refusal before writes.

## Second review of `09b3fad`: Windows drive-relative and verbatim joins (P1/P2)

**Findings.** For config `C:\operator\live.toml`, Python joined `C:STOP` to
`C:\operator\STOP`, whereas Rust retains the drive-relative path, whose base is the
C-drive working directory. This could ignore the original engaged stop (P1). For a
verbatim config `\\?\C:\operator\live.toml`, copying `..\STOP` retained that component;
Rust's verbatim join normalises it. The copied path could fail with Win32 error 123 (P2).
Both clients share the helper, so both were affected.

**Resolution (architect's closed rule).** Classify raw strings before any path join.
Accept only standard absolute Windows paths (letter plus root, or UNC) and plain relative
paths without a drive or initial separator; neither may contain a `..` component.
Absolute operator values stay byte-for-byte identical. Plain relatives join the supplied
config directory. Refuse drive-relative, root-relative, verbatim/device-prefixed, parent
traversal and empty paths before creating anything. Apply those restrictions to the
supplied config path as well, retaining the symlink refusal. Omit unused optional path
keys rather than accepting explicit empty strings. Keep resolution-equivalence checks
for the permitted forms, and pass the fixed refusal reason through each client's main
function without printing the path or starting a server. This supersedes the previous
correction's acceptance of lexical `..` components, rather than emulating special joins.

**Fail-first regressions.** Before this correction, the new cases produced 25 assertion
failures, including acceptance of `C:STOP`, `\STOP`, `..\STOP`, empty strings and verbatim
configs in both clients, and rewriting `C:/operator/STOP` separators. After correction:

- `test_ambiguous_windows_operator_paths_refuse_before_writes_in_both_clients` covers the
  rejected operator forms, with path-resolution doubles and no copy/run folder created.
- `test_ambiguous_config_paths_are_refused_before_reading_or_writing` covers verbatim and
  device config prefixes, drive/root relatives, parent components and empty strings.
- `test_absolute_operator_paths_are_preserved_byte_for_byte_in_both_clients` protects
  drive-rooted and UNC values, without contacting a network host.
- The existing stop/pin identity and all-current-path tests protect plain relatives and
  their absolute destinations; the fixture with `../model` now uses an admitted path.
- `test_rejected_path_explanation_reaches_both_clients_without_starting_a_server` protects
  the clear operator reason, exit code and absence of process startup on refusal.

These are filesystem/path doubles only: no live engine, screen capture or OS input.

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
regresiones fallaron antes del arreglo y cubren ambos clientes. La segunda revisión
detectó las diferencias de `C:STOP` y de rutas verbatim: por decisión del arquitecto,
sólo se aceptan absolutas estándar o relativas simples sin `..`; las demás se rechazan
antes de escribir y se informa el motivo sin mostrar la ruta. El validador de guías
reconoce las herramientas Python y sus continuaciones PowerShell. M1 queda descriptiva,
con 30 + 30 pares y sin umbral, por decisión del responsable.
M1 reutiliza la agrupación del motor con un lector incapaz de entregar entrada; las pruebas
usan únicamente fotogramas sintéticos y dobles. M1, M2 e IT-08 siguen sin ejecutar.
