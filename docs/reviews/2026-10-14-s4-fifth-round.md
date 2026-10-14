# S4 fifth round: findings (2026-10-14)

Findings from the fifth round of the reduced S4 protocol, which ran the Calculator's switch to Standard
mode and the Notepad series on the reference machine. The round used the committed client
(`tools/s4/e2e_driver.py`, SHA-256 `98f0ae08…1967e5`, checked before every live invocation) and the
engine binary of the fourth round (`ommatix.exe` `2c07e447…ee2c01`). The evidence, in Spanish, is
[`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Quinta ronda",
and CHANGELOG 1.0.21.

The round sent three live requests and scored no run. Each finding is either resolved with a test that
fails without the fix (AGENTS.md rule 10) or left open with the decision it needs. No code changed in
this round: the client is pinned to the hash above for the whole round, and each fix would change the
evidence it produced. The resolutions in the table are in branch `fix/s4-driver-grounding`, prepared
after the round with doubles only (no live input); they change the client to
`6dad0dc349510027f1642df666f4d5c7495bcb73d2e9a4e87c58903e0a8e2a77` and no Rust crate.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (open, safety) | E2E-03 pilot: after the click on `Editar`, neither the element under the cursor nor its parent had the same RuntimeId or AutomationId as the element found before the click. The engine reported `effect_observed`, which the client counts as an engine false success. UI Automation read the menu Expanded, so the click probably reached `Editar`, but the cause is not verified: the identifiers may change when the menu expands, or the click may have reached another control. The scored series did not run | Open. The sixth round's diagnostic run (below) refutes the identifier-change explanation, does not support the other-control explanation, and shows that the point query never returns the menu item. Owner decision needed | `MenuDiagnosticTest` covers the record (identifiers that change on expansion; scoring identical with the read available, unreadable or failing; the identity check not relaxed). No test of the cause: the sixth round's record does not settle it (below) |
| 2 | Major (open, client, safety) | `restore_click`, the second click that closes the Edit menu, has no UI Automation identity grounding. `restore_menu` finds `Editar` by its label among the observed elements and sends the click through the engine's gate only. Every other click of the flow is grounded before and after the click | **Resolved** in `fix/s4-driver-grounding` (client `6dad0dc3…8e2a77`): `restore_menu` grounds the click by identity before and after, like `steps()`; a mismatch is recorded (`menu_restore.grounded` false), counted as `grounding_wrong` and never followed by a retry or more input | `RestoreGroundingTest.test_a_restore_click_that_reaches_another_element_is_reported_and_not_retried` (fails without the fix), and the four cases with no single identifiable `Editar` (no click) |
| 3 | Major (open, client) | `--calc-standard` records `target label not found or not confirmed` for `Estándar` and does not say whether there were no candidates, several, or one that was not confirmed, so the cause of the stop is unknown | **Resolved** in the same branch: every step records its candidates, their roles and, on a stop, `stopped_by` (`zero_candidates`, `several_candidates`, `not_confirmed`) and the control types UI Automation sees under the same names; `--calc-standard` writes them in `targets` | `TargetRecordTest.test_zero_candidates_and_two_candidates_produce_distinct_records`, `test_one_candidate_that_is_not_confirmed_is_a_third_condition`, `test_the_records_hold_no_label_or_name` |
| 4 | Minor (open, engine) | The restore click returned `indeterminate` (`INDETERMINATE_EFFECT`, verification 509.6 ms), although UI Automation read the menu Collapsed afterwards | Analysed, not changed: no engine defect is demonstrated. The shape is what the engine reports when no capture differs from the pre-delivery frame during the whole 500 ms window; see "Finding 4" below. The client now records `state_polls` of the restore to help the next live read | None for the engine: the existing `no_change_within_the_timeout_is_indeterminate` already pins the behaviour. `RestoreGroundingTest.test_the_polls_until_the_menu_reads_closed_are_recorded` covers the new record |

## Observations

- The identity check of the navigation toggle (`TogglePaneButton`, Button) passed after its click during
  the mode switch. It is the first live confirmation of the identity grounding of CHANGELOG 1.0.20, and it
  covers that one control only. It does not show that the check works for `Editar` (finding 1).

## Notes

- Ledger: 396 at the start and 399 at the end, which is 3 live requests of the cap of 400 (1 in the mode
  switch, 2 in the Notepad pilot). All 3 were delivered and none was rejected.
- Mode switch: exit code 1, `switched` false, no second click, the Calculator closed by the step. The
  mode was not read again after the stop.
- E2E-03 pilot: one run, excluded from scoring. `success` false, `engine_false_success` 1 and
  `grounding_wrong` 1 on the first click. The menu-closing click was reported `indeterminate`, and UI
  Automation then read the menu Collapsed.
- Owner's windows: two Notepad windows, both minimized before the pilot. Their placement was saved before
  the pilot and restored after it, and the verification matched. The client minimized the window it had
  reused at the end, as designed.
- Stop: `stop.flag` was absent throughout and was not created by the client, because no series reached
  three consecutive failed runs.
- `user_idle_s_at_start` in the pilot was 43.4 s, consistent with the client's own last click (the
  navigation click, about 43 s earlier). No input of unknown origin was recorded.

## Verification

Before the live runs:

- `cargo fmt --all --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo clippy -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 337 passed, 0 failed, 1 ignored.
- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: already up to date.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 62 passed.

After the live runs, from `docs/thesis`: `run_tests.py` 48 of 48 OK; `validar_paquete.py` 210 checks, 0
failed. Their result files changed only in timestamps, Python and platform strings, a duration and the
jsonschema version, so they were restored with `git restore`.

## Not in this round

- No code change, no driver change and no regression test: no defect was fixed in code. Findings 2 and 3
  each need a test, and finding 1 needs the owner's decision before any further live run.
- E2E-02 and the scored E2E-03 series did not run.

## Preparation for the sixth round (`fix/s4-driver-grounding`, CHANGELOG 1.0.23)

Made with a fake MCP engine and a fake accessibility chain: no live input, no Calculator or Notepad
launched, `stop.flag` neither created nor removed. Nothing read from UI Automation is recorded but
control types, automation ids, class names, states and booleans.

- **Finding 2.** `restore_menu` looks `Editar` up through UI Automation in the foreground window
  (`identity_lookup`, now shared with `steps()`), clicks only if there is exactly one identifiable
  element, and checks the element under the cursor afterwards. A mismatch counts as `grounding_wrong`
  (and as `engine_false_success` if the engine said `effect_observed`) and ends the restore: one click,
  no retry. Without a single identifiable `Editar` nothing is sent and the menu stays expanded for
  the operator: that is the fail-closed price.
- **Finding 3.** See the table. A stop caused by several elements sharing the label still engages the
  operator's stop (unchanged), but now records what was there before it does.
- **Finding 1 (preparation).** `menu_diagnostic` of a Notepad run has, per click, `before` (the
  chain under the centre of the one on-screen `Editar`, read through UI Automation) and `after` (the
  chain under the cursor, where `SendInput` left it), up to five rows each, plus `after_ms`. A
  `same_runtime_id` of row 0 that turns false while the control type, the automation id and the
  `Expanded` state hold is the signature of "the identifiers change on expansion"; a different control
  type or automation id, with `Editar` not `Expanded`, is the signature of "the click reached another
  control". The record does not change the verdict: the identity check is exactly as strict as before.
  The owner-present diagnostic is one pilot run with `--max-live 2`: the click on `Editar` and the click
  that closes the menu. `--max-live 1` must not be used: the restore click would hit the limit and
  create `stop.flag`.

### Finding 4: why the closing click was `indeterminate` while UI Automation read Collapsed

From `src/application/src/engine/act.rs` and the capture adapter:

- After a delivered click with `expected_effect: visual_change` the engine reports `indeterminate`
  only when no capture taken within `verify_timeout_ms` (500 ms, polled every 50 ms) differs, tile by
  tile and pixel by pixel, from the frame captured just before delivery (`report_effect_untimed`,
  `screen_changed_since`, `dirty_tiles`). The observed 509.6 ms of verification is that window plus the
  last capture: every probe found the screen equal to the pre-click frame, or failed to capture
  (`capture_differs` reads a failed capture as no change).
- The same response is produced, in `engine/tests.rs`, by
  `no_change_within_the_timeout_is_indeterminate` (a redraw that comes after the window) and, by
  reading the code, by a change that does not persist until a poll, or by capture errors during the
  window. The engine's answer does not say which, and UI Automation reading Collapsed later is
  consistent with all of them: the menu may have closed after 500 ms, or between polls, or without a
  pixel change that the capture saw.
- No defect is demonstrated: every branch ends in `indeterminate`, input delivered, no retry, and an
  uncertain effect is never turned into a success. The engine is unchanged and no Rust test was added,
  because the existing one already fixes the behaviour and a new one would only restate it.
- What would tell the causes apart is data the engine does not return today: the number of probes, how
  many captures failed and when the last difference was seen. Adding that to `_meta` is a proposal for
  the owner, not done here. The client now records, for the restore, how many UI Automation reads it
  took to see the menu Collapsed (`state_polls`): 1 would mean the menu was already closed at the
  first read, after the engine's 500 ms window.

### Protocol conflict: `stop.flag` after three failed runs, or a series that stops for its own reason

The protocol says two things. The run rules (S4 report, section 1) say the client creates the stop
file when three consecutive runs fail, and `Protocol.run` does it. The third round's record (E2E-03
pilots) says that a series aborted for a reason specific to it lets the next series run. Both cannot
hold for a series that fails three runs for a cause of its own. **Nothing was changed.** Options for
the owner:

1. **Keep the rule as it is.** Three consecutive failed runs, whatever the cause, create `stop.flag`
   and every later invocation aborts until the owner removes it. Safety: the strongest; a cause that
   looks series-specific but is not (a real misbehaviour of the engine, a foreground that changed) can
   never reach another series unattended. Cost: each series-specific abort needs the owner at the
   keyboard before the next series, and a round meant to run unattended stops after the first such
   series. The documentation would say that a series aborted for a reason specific to it still needs
   the owner to remove `stop.flag`.
2. **Classify the abort.** Failures whose cause is a precondition of the series (a control that is
   missing before any input, an initial state that is not the expected one) end the series with exit
   code 1 and no `stop.flag`, so the next series may run. Failures that touch safety
   (`grounding_wrong`, `engine_false_success`, an unexpected foreground, an ambiguous label, a rejected
   or `indeterminate` delivery, the budget) still create it. Safety: weaker; it needs a closed list of
   precondition failures that an agent cannot widen, and a mistake in the list lets an unsafe series
   fall through to the next one. Cost: code and tests for the classification, and a protocol rule that
   says which failures are which.

The stop file is the operator's mechanism (AGENTS.md rule 6): option 2 would put into the client a
decision that today belongs to the owner. This review recommends option 1 until a series is shown to
need option 2.

## Verification of this branch

See the PR description and the report: `python -m unittest discover -s tools/s4 -p "test_*.py"`
(81 passed; the 19 new tests give 16 failures or errors with the old client), `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and from
`docs/thesis` `run_tests.py` and `validar_paquete.py`.

## Sixth round: diagnostic run for finding 1 (2026-10-14)

One live diagnostic run of the Notepad menu flow, with the menu diagnostic: `--app notepad --mode run --runs 1 --max-live 2`. Client `6dad0dc3…8e2a77`, engine built from `main` (`11a3d792…aca9da`), float16 manifest `manifest.rnf05.json`. The owner was present and did not use the PC. Two live requests were sent and both were delivered (ledger 399 → 401). No scored run, and no code changed. Evidence: [`S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Sexta ronda (diagnóstico)", and CHANGELOG 1.0.24.

What the record shows, with no names:

- **The identifiers do not change on expansion.** The identity lookup made while the menu was Expanded (before the closing click) found the same item as before the opening click: control type `MenuItem`, automation id `Edit`, and the same runtime id. Explanation (a) predicts that the item appears after the click with a different runtime id. It appears in no chain (see below), and its identifiers are unchanged. Explanation (a) is refuted for this run.
- **The point query never returns the item.** Before the click, at the centre of the item, and after it, at the cursor, UI Automation returns the same three elements: the XAML island host pane (`Microsoft.UI.Content.DesktopChildSiteBridge`), the Notepad window and the desktop pane. The item is in neither chain. The grounding rule ("the element under the cursor, or its parent") therefore cannot confirm this item, whatever the click did. That is why both clicks failed the grounding.
- **The menu state changed twice, by name.** `Editar` read Collapsed at the start, Expanded after the opening click and Collapsed after the closing click. The run sent nothing else, so only clicks on the item itself can produce these transitions. This does not support explanation (b), whose second half (the state of `Editar` does not change to Expanded) is false.

Conclusion: **inconclusive on the exact point the click reached.** Explanation (a) is refuted, and (b) is not supported. The point-based identity rule cannot confirm `Editar` in this Notepad. The engine's two `effect_observed` reports agree with the menu transitions, so the two `engine_false_success` counts come from the grounding rule, not from a demonstrated engine error.

Not changed: the identity rule, the client, the engine and the scoring. No rule is proposed, because the record does not show (a). The owner must choose between keeping the point rule (E2E-03 then cannot be scored in this application) and letting the item's own state change, read through its runtime id, ground the clicks on `Editar`. The second is a change to the grounding rule. It needs a written specification and a test that fails without it, before any code changes.

Finding 2's restore grounding was exercised for the first time in this run: the mismatch was recorded in `menu_restore.grounded`, counted, and not retried. Finding 4 is unchanged; the closing click returned `effect_observed` this time.

Finding 1 stays open.
