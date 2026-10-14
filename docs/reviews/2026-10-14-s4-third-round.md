# S4 third round: findings (2026-10-14)

Findings from the third round of the reduced S4 protocol, with `type` in two phases
([ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md)) and UI
Automation evidence ([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)), on the
reference machine with real capture and real input in the three applications the owner
authorised on 2026-10-14. The evidence, in Spanish, is
[`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section
"Tercera ronda: type en dos fases (ADR-0018)". Each fixed finding ships with a test that fails
without its fix (AGENTS.md rule 10). Findings 2, 3, 4 and 6 were open after the round and are
resolved in the [fourth-round fixes](#fourth-round-fixes-findings-2-3-4-and-6) below; finding 5
stays open (environment).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (fixed, client) | `WScript.Shell.AppActivate(pid)` did not bring the test application to the foreground. In pilot 1 of E2E-01 it took 164 requests over 333.6 s with the desktop in front. A later direct `SetForegroundWindow` on the visible window returned false, and one `AppActivate` returned true without a foreground change. The client targeted the process, and the process also owns a hidden helper window (winit's event target) that `EnumWindows` lists. The cause inside `AppActivate` is not established | `bring_to_front` attaches this thread to the input queue of the foreground thread, calls `SetForegroundWindow` on the application's main window (`pick_main_window`: the largest visible window that is not a helper), then detaches. It sends no key and no mouse event. Live: one activation request and 0.88 to 1.10 s to the foreground in the 13 scored runs and in the probes | `tools/s4/test_e2e_driver.py`: `the_event_target_helper_is_not_the_main_window`, `the_largest_non_helper_window_is_the_main_one`, `no_main_window_means_no_activation_target`. Against the previous client they fail with `AttributeError` |
| 2 | Major (fixed, engine; see below) | After a full observation with a smaller tree (80 elements against 82 to 85) and slow accessibility reads (226 and 489 ms against a median of 71 ms), the next observations were incremental: no region was planned and accessibility took 0 ms. The state of the field `Campo de texto` stayed unconfirmed for four observations, so the client sent no key (fail-closed). Probable cause, not verified: the time-bounded walk (250 ms) returned a partial tree, and an incremental observation keeps the last accessibility state | Fixed in the fourth round: the provider reports a truncated read, the engine reads those regions again on the next observation, and a partial tree never confirms nor unconfirms what it did not show. A fix within ADR-0017's intent (ADR-0017 is accepted and immutable, so the record is this file and CHANGELOG 1.0.18) | `a_truncated_accessibility_read_is_read_again_on_the_next_observation`, `a_truncated_read_never_confirms_from_partial_evidence` |
| 3 | Major (fixed, client protocol) | In Scientific mode the Calculator exposes no button named `Borrar` after `Es igual a`. Its UI Automation tree lists 49 buttons, among them `Quitar entrada` (clearEntryButton), `Borrar toda la memoria` and `Borrar toda el historial`, which are other controls. `Borrar` was present and confirmed in the initial state, and absent in three observations after `=`. The protocol's exact-label selection refused the target: E2E-02 ran 0 of 3, then the three-failure rule stopped it | Fixed in the fourth round in the driver: the clear control is the first of `Borrar`, `Borrar todo`, `Borrar entrada` (exact labels, fixed order) that exists as a confirmed element, read from `observe_screen` before the flow and again before the last click; the run fails before any input when none exists; the display must return to 0 through UI Automation. The owner can still choose the Standard mode before a series (not done by the client) | `tools/s4/test_e2e_driver.py`: `ClearControlTest`, `InitialStateTest` |
| 4 | Major (fixed, client) | One click on `Editar` did not produce an observable menu in Notepad. Pilot 1 reported `effect_observed` with no menu-item words in four observations; pilot 2 reported `indeterminate` (no visual change within the 500 ms window). The driver's closed-menu check accepts any state on its first call (it sets the baseline without looking at the menu), and it does not close a menu left open after a failed run, so a failing run changes the owner's window with no restore path. UI Automation reported `Editar` as expanded in the minimized window while the visible menu was closed | Fixed in the fourth round in the driver: every run starts by reading the `ExpandCollapseState` of `Editar` through UI Automation and fails without input unless it is Collapsed; "opened" and "closed" are verified by that state, never by screen text; after a failed run an Expanded menu is closed with one click on `Editar` through the engine gate (once, only if Notepad is in front and `Editar` is a confirmed target, never touching the stop). Why one click did not produce an observable menu is still not established: the owner repeats E2E-03 present | `tools/s4/test_e2e_driver.py`: `NotepadMenuTest`, `RestoreMenuTest` |
| 5 | Minor (open, environment) | Input events that the client did not send reset the idle timer: 0.3 s before the first live start, and 13.5 s after a 20 s pause in which the client sent nothing. The source is not identified. The engine's `SendInput` calls are made only from action delivery (`src/adapters/input/src/sendinput/win.rs`). The client records `user_idle_s_at_start` in every run | Open. Confirm the owner's use of the PC during runs, and identify the source if it persists | none |
| 6 | Minor (fixed, MCP) | `execute_action` responses carry no `_meta` stage timings (0 of 103 responses in this round), so action latency is the client round trip only | Fixed in the fourth round: each `execute_action` that went through the gate carries its stage timings in result `_meta` under `io.ommatix/action_timings` (`gate_us`, `freshness_us`, `delivery_us`, `focus_us`, `verification_us`, `total_us`), never in `structuredContent`. A replay and a refusal before the gate carry none | `an_action_reports_the_time_of_each_of_its_stages` (engine), `execute_action_results_carry_their_stage_timings_in_meta` (MCP) |

## Fourth-round fixes (findings 2, 3, 4 and 6)

### Finding 2: a partial accessibility read

- **Port.** `AccessibilityProvider::snapshot` returns `AccessibleRead { elements, truncated }`.
  The UI Automation provider sets `truncated` when its walk ends on the element or time budget,
  when the depth bound cuts a subtree that has children, or when a window cannot be read. (The
  `uiautomation` crate cannot tell a failed read of the children from none, so that case is not
  reported.)
- **Gaps.** After a truncated read the published snapshot keeps the regions that were read as
  `accessibility_gaps`. The next observation reads the plan *and* the gaps, whether or not any
  pixel changed there; the detector and the recogniser still see the plan only. A read that is
  complete closes the gaps. A window that is always truncated is read again on every observation
  (bounded by the walk's budget, never a full re-analysis).
- **No confirmation lost.** Accessibility evidence held for a gap region that the partial read
  did not find at the same box is kept, unless the plan re-analysed its area (its pixels changed,
  so it may be gone: a phantom confirmation would be worse than a lost one). So a password field
  keeps its `sensitive` mark (its OCR text never leaks) and an element keeps its ID, revision and
  confirmation while its pixels are unchanged. Where pixels did change, an element the partial
  read does not reach loses its accessibility confirmation until a complete read (fail-closed).
- **No confirmation gained.** A visual box that no accessibility element absorbs loses its
  confirmed affordance while it lies in a gap region: a complete tree might show a control under
  its click point (the rule of S4 finding 2 of the second round), and a partial one cannot rule
  that out. The next complete read gives it back.
- **`_meta`.** `observe_screen` results carry `io.ommatix/accessibility: { "complete": bool }`
  next to the timings; never in `structuredContent`.
- **Mutation check.** With gaps always empty both regression tests fail; with the retention rule
  off the second fails.

### Finding 6: action timings

`Engine::last_action_timings()` gives the durations of the gate (what no other stage accounts
for), the freshness capture and last look, the actuator's calls (for two-phase `type` the click
and the keys), the wait for keyboard focus and the verification window, on the injected `Clock`
(zero under `ManualClock`, except where the engine sleeps). The MCP adapter puts them in
`_meta["io.ommatix/action_timings"]`.

### Findings 3 and 4: the driver (protocol)

- **E2E-02.** Chosen: resolve the clear control from the Calculator's real accessibility names
  (not "put the Calculator in Standard mode first"). `CLEAR_CONTROLS` is a fixed ordered list of
  exact labels; `pick_control` takes the first that exists as a confirmed element; `Quitar
  entrada`, `Borrar toda la memoria` and similar are never chosen. The initial state needs every
  key and at least one clear control, so a Calculator with none stops the run before any input;
  the result records which of them existed at the start, and `alternatives_present` when the last
  step fails. In Scientific mode after `Es igual a` the tree may still lack all three: then the
  run fails closed and the owner decides the mode.
- **E2E-03.** `notepad_menu_state` reads the `ExpandCollapseState` of `Editar` through UI
  Automation (exactly one element with the pattern; anything else is unknown). Steps verify
  `Expanded` then `Collapsed` with that read, with `expected_effect: visual_change` as before and
  no OCR check. `restore_menu` runs only after a failed run, only if the state is Expanded, only
  with Notepad in front and a confirmed `Editar` target, spends one live request through the gate,
  never retries and never touches `stop.flag`.

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

Fourth round: see the verification list at the end of this file.
- `python referencia_ejecutable/run_tests.py` (from `docs/thesis`): 48 of 48 OK. `python validar_paquete.py`: 210 checks, 0 failed. Their result files changed only in timestamps, Python and platform strings and a duration, so they were restored with `git restore`.

### Fourth round

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 337 passed, 0 failed (333 before: three engine tests and one MCP test are new).
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 18 passed.
- No real input was sent and `stop.flag` was not touched. The driver changes were exercised only
  through fakes; they are unmeasured in live runs until the owner repeats E2E-02 and E2E-03.
