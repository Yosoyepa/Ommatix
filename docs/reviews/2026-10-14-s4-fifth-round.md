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
evidence it produced.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (open, safety) | E2E-03 pilot: after the click on `Editar`, neither the element under the cursor nor its parent had the same RuntimeId or AutomationId as the element found before the click. The engine reported `effect_observed`, which the client counts as an engine false success. UI Automation read the menu Expanded, so the click probably reached `Editar`, but the cause is not verified: the identifiers may change when the menu expands, or the click may have reached another control. The scored series did not run | Open. Needs the owner's decision: a read-only observation of the expanded menu (one click on `Editar`, with the owner present), or another rule for a menu item whose identifiers change on expansion, specified before any code changes | None. The cause is unknown; a test now would encode a guess |
| 2 | Major (open, client, safety) | `restore_click`, the second click that closes the Edit menu, has no UI Automation identity grounding. `restore_menu` finds `Editar` by its label among the observed elements and sends the click through the engine's gate only. Every other click of the flow is grounded before and after the click | Open. Proposed: the same grounding as `steps()`, before and after the click. Not changed in this round, because the client is pinned to `98f0ae08…1967e5` and a change would alter the evidence | Proposed: a fake accessibility chain in which `Editar` is not the element under the cursor after the restore click; the restore must report it, and no retry may follow. Not written yet |
| 3 | Major (open, client) | `--calc-standard` records `target label not found or not confirmed` for `Estándar` and does not say whether there were no candidates, several, or one that was not confirmed, so the cause of the stop is unknown | Open. Proposed: record the number of candidates and their control types, without names, in the step's record | Proposed: a fake pane with zero candidates and with two candidates produces distinct records. Not written yet |
| 4 | Minor (open, engine) | The restore click returned `indeterminate` (`INDETERMINATE_EFFECT`, verification 509.6 ms), although UI Automation read the menu Collapsed afterwards | Open, no proposal yet: the visual-change check did not confirm the closing, and its cause is not known | None |

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
