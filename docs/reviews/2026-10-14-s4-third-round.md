# S4 third round: findings (2026-10-14)

Findings from the third round of the reduced S4 protocol, with `type` in two phases
([ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md)) and UI
Automation evidence ([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)), on the
reference machine with real capture and real input in the three applications the owner
authorised on 2026-10-14. The evidence, in Spanish, is
[`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section
"Tercera ronda: type en dos fases (ADR-0018)". Each fixed finding ships with a test that fails
without its fix (AGENTS.md rule 10). Findings 2 to 6 are open: they need a design or an owner
decision, so they are recorded here and not fixed in this round.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (fixed, client) | `WScript.Shell.AppActivate(pid)` did not bring the test application to the foreground. In pilot 1 of E2E-01 it took 164 requests over 333.6 s with the desktop in front. A later direct `SetForegroundWindow` on the visible window returned false, and one `AppActivate` returned true without a foreground change. The client targeted the process, and the process also owns a hidden helper window (winit's event target) that `EnumWindows` lists. The cause inside `AppActivate` is not established | `bring_to_front` attaches this thread to the input queue of the foreground thread, calls `SetForegroundWindow` on the application's main window (`pick_main_window`: the largest visible window that is not a helper), then detaches. It sends no key and no mouse event. Live: one activation request and 0.88 to 1.10 s to the foreground in the 13 scored runs and in the probes | `tools/s4/test_e2e_driver.py`: `the_event_target_helper_is_not_the_main_window`, `the_largest_non_helper_window_is_the_main_one`, `no_main_window_means_no_activation_target`. Against the previous client they fail with `AttributeError` |
| 2 | Major (open, engine) | After a full observation with a smaller tree (80 elements against 82 to 85) and slow accessibility reads (226 and 489 ms against a median of 71 ms), the next observations were incremental: no region was planned and accessibility took 0 ms. The state of the field `Campo de texto` stayed unconfirmed for four observations, so the client sent no key (fail-closed). Probable cause, not verified: the time-bounded walk (250 ms) returned a partial tree, and an incremental observation keeps the last accessibility state | Open. Decision needed: re-read accessibility for a region whose last read was partial or time-bounded, or refresh the snapshot when a target is not confirmed. Changing the fusion needs an ADR (ADR-0017, §4) | none yet |
| 3 | Major (open, specification and application) | In Scientific mode the Calculator exposes no button named `Borrar` after `Es igual a`. Its UI Automation tree lists 49 buttons, among them `Quitar entrada` (clearEntryButton), `Borrar toda la memoria` and `Borrar toda el historial`, which are other controls. `Borrar` was present and confirmed in the initial state, and absent in three observations after `=`. The protocol's exact-label selection refused the target: E2E-02 ran 0 of 3, then the three-failure rule stopped it | Open. The owner decides the final key, the mode or the order of the steps | none (a flow decision) |
| 4 | Major (open, client and engine) | One click on `Editar` did not produce an observable menu in Notepad. Pilot 1 reported `effect_observed` with no menu-item words in four observations; pilot 2 reported `indeterminate` (no visual change within the 500 ms window). The driver's closed-menu check accepts any state on its first call (it sets the baseline without looking at the menu), and it does not close a menu left open after a failed run, so a failing run changes the owner's window with no restore path. UI Automation reported `Editar` as expanded in the minimized window while the visible menu was closed | Open. E2E-03 was not scored: the operator stopped the series after pilot 2, and the cause is specific to this series. Before repeating: check the closed state at the start (a real check, not the baseline), close any open menu at the end of a failed run, and decide the observation behaviour with the owner present | none yet |
| 5 | Minor (open, environment) | Input events that the client did not send reset the idle timer: 0.3 s before the first live start, and 13.5 s after a 20 s pause in which the client sent nothing. The source is not identified. The engine's `SendInput` calls are made only from action delivery (`src/adapters/input/src/sendinput/win.rs`). The client records `user_idle_s_at_start` in every run | Open. Confirm the owner's use of the PC during runs, and identify the source if it persists | none |
| 6 | Minor (open, carried from round 2) | `execute_action` responses carry no `_meta` stage timings (0 of 103 responses in this round), so action latency is the client round trip only | Open | none |

## Notes

- **Activation and input.** Finding 1 was first suspected to be an input-injecting activation. A
  control and a call measured with `GetLastInputInfo` showed no reset (control 59.03 to 59.55 s;
  the call returned true in 0.01 s with the idle timer at 61.56 s before and after). Only the
  success path was measured. The sporadic input of finding 5 remains unexplained.
- **Pilot 1 of E2E-01** used the previous client and a manual foreground step (attach and
  `SetForegroundWindow`, window management only) because of finding 1. It is excluded from scoring.
- **Series actually run:** E2E-01 (two pilots, scored series), E2E-02 (pilot, diagnostic, then
  the scored series after the Notepad pilots), E2E-03 (two pilots, not scored). The protocol's
  order was E2E-01, E2E-02, E2E-03.
- **Diagnostics** (`calc_diag.py` and `notepad_dry_check.py`, in the run directory outside the
  repository) printed only fixed UI names, displays and counts. The dry Notepad check sends no
  input. The calculator diagnostic sent four clicks of the same flow and is excluded.
- **Hashes.** Scored runs: `tools/s4/e2e_driver.py` SHA-256 `8aa80988…c353b9`. After the runs only a
  comment and a docstring were corrected (no code change): the committed file is `0d0fe263…87736e`.
  Pilot 1 of E2E-01: `56353c46…4d53a7`. `ommatix.exe` `97a03896…98ec72`; test application `09ccf30d…c9bf`;
  configuration `ecc7c5f7…2eac5`.
- **Ledger:** 160 to 291 of the 400 live requests allowed. `stop.flag` was removed once at the
  start, as authorised, and was created by the client after the third failure of E2E-02. It remains.

## Verification

- `cargo fmt --all --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 333 passed, 0 failed.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 3 passed. Against the previous client, 3 errors (`AttributeError`).
- `python referencia_ejecutable/run_tests.py` (from `docs/thesis`): 48 of 48 OK. `python validar_paquete.py`: 210 checks, 0 failed. Their result files changed only in timestamps, Python and platform strings and a duration, so they were restored with `git restore`.
