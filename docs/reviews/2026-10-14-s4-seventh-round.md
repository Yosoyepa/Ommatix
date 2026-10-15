# S4 seventh round: findings (2026-10-14)

Findings from the seventh round of the reduced S4 protocol on the reference machine. The Notepad pilot of E2E-03 passed, and nine of the ten scored runs of the series passed, all of them with no false success. The series stopped at its tenth run: its live budget (20 requests) ran out before that run's first request, which was refused and not sent, and the client created the operator's stop file. `--calc-standard` and E2E-02 did not run.

The round used the committed client (`tools/s4/e2e_driver.py`, SHA-256 `60486d87…c214a`, checked before each live invocation), the engine binary of `main` (`11a3d792…aca9da`, the same as in the sixth round) and the float16 configuration `s4r7-live.toml` (`b6e9e8e8…e74c`, a copy of the sixth round's). The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Séptima ronda", and CHANGELOG 1.0.26.

Each finding is either resolved with a test that fails without the fix (AGENTS.md rule 10) or left open with the decision it needs. No code changed in this round: the client stayed pinned for the live part, and none of the findings is a client defect.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (planning; open, owner decision) | The series cap of the protocol, `--max-live 20` for ten Notepad runs, did not reserve requests for the retries after `STALE_SNAPSHOT`. Each retry is one request in the ledger, because the driver's `spend_live` runs before every attempt. Runs 2 and 8 each needed one retry, so the budget reached 20 before run 10. The client's guard then engaged the operator's stop before sending the 21st request of the series | No code change: the guard behaved as designed, and no input went out after the stop. The sizing is an owner decision, with the figures in the S4 section (§8). The round has 128 requests left of 150, and the worst case of E2E-02 (five clicks, up to four attempts each) exceeds that | None: no code change. A sizing rule that counts retries is a protocol decision and, if the client changes, needs its own test and a new round with a new client hash |
| 2 | Minor (operation; not a client defect) | The operator watch used for the series (outside the repository) stopped the client process while the client was closing, in the same second that the client wrote `stop.flag`. The client's `end` record and its server shutdown record were not written. Checked afterwards: no Ommatix process, both windows minimized at their saved placement, Edit menu Collapsed | Closed for this round: the desktop state was verified. The watch is not reused in that form. A future watch only reports, and does not stop the client on `stop.flag`, which the client already honours | None: the script is outside the repository |
| 3 | Observation (no defect shown) | Two `STALE_SNAPSHOT` refusals, on the Edit close click of runs 2 and 8, before any delivery. Each retry was grounded again by identity and delivered (`effect_observed`) | No change. Cause not verified: the record does not show what changed on the screen between the observation and the action | None |
| 4 | Observation (client record) | The island walk visited 11 elements per click in the pilot and 20 in the series. The bounds (500 and 16) were far from reached, and the result was the same (depth 3, `MenuItem` `Edit`) | No change. Cause not verified | None |
| 5 | Documentation (pre-existing; owner decision) | `docs/thesis/matrices/plan_pruebas.csv` names E2E-02 "Editor" and E2E-03 "Explorador", while the S4 protocol runs the Calculator and Notepad flows under those identifiers | No change to the matrix. The owner decides the mapping. Until then the S4 evidence cannot set those statuses | None |

## Verification

Before the live runs, from `main` at `7bca432` on the branch `docs/s4-seventh-round`:

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: already up to date, binary SHA-256 `11a3d792…aca9da`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 351 passed, 0 failed, 1 ignored.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 113 passed.
- From `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 of 48; `validar_paquete.py`, 211 checks, 0 failed. Four result files changed only in timestamps, platform and Python strings, a duration and line endings, and were restored with `git restore`.

After the documentation edits, the two thesis checks were run again on this branch (see the pull request).

## Live part: counts and desktop

- Ledger 401 → 423: 22 live requests of 150 in the round (2 in the pilot, 20 in the series).
- Pilot (excluded): passed. Both clicks grounded by identity and by the island descent (depth 3, `MenuItem` `Edit`); the Edit menu went Collapsed → Expanded → Collapsed.
- Series: 9 scored runs, 9 successes, 0 false successes, 0 `grounding_wrong`, 18 delivered (all `effect_observed`), 2 refused before delivery (`STALE_SNAPSHOT`), 0 `indeterminate`, 0 restore clicks.
- `stop.flag`: created by the client at the budget guard, at 18:02:21 local time; present at the end; not removed.
- Owner's Notepad windows: restored and verified against the placement saved before the pilot. Both minimized. Edit menu Collapsed in both (read-only UI Automation).
- No Ommatix, Calculator or test application process left running. Notepad processes 8960 and 23164, unchanged.
- No screen captures or screen text saved.

## Not in this round

- `--calc-standard` and E2E-02: not run, because `stop.flag` existed.
- E2E-03 run 10: not started. Its first request was refused by the guard before it was sent.
- No client change, no matrix change and no regression test: no client defect was shown.

## Continuation: run 10, `--calc-standard`, and E2E-02 not run

Branch `docs/s4-seventh-round-continuation`, from `main` at `885cda7`. The continuation ran the tenth Notepad run and the Calculator's Standard-mode step under the owner's authorization. The Notepad run passed, which completes the scored E2E-03 series: ten of ten scored runs, all successful, with no false success. `--calc-standard` did not reach Standard mode, so E2E-02 did not run. Evidence: the S4 section "Séptima ronda (continuación)" and CHANGELOG 1.0.27.

### Findings and resolution (continuation)

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| C1 | Major, Calculator (open; owner decision) | `--calc-standard` found no element named «Estándar» (in the engine's observation or in UI Automation) for about 1.5 s after the navigation click, and stopped without a second click. The record does not show whether the navigation pane opened, because the mode is not read when the step stops early | No code change in this round: the client stays pinned for the live part. Reading the pane state after the click and waiting longer would change the client; that needs a test that fails without the change and a new round with a new hash | None yet |
| C2 | Minor, operation (open) | About 165 s passed between the Calculator's launch and its first click, while `wait_foreground` waited for the window to be foreground and maximized. The calc-standard summary does not record this wait; the run records do (`foreground_wait_s`). Cause not verified; hypothesis: the Windows foreground lock | No change. The click was sent only with the Calculator foreground, maximized and grounded by identity | None |
| C3 | Minor, client logging (open) | The `calc_standard` line of the driver's log holds one non-UTF-8 byte (`ó`, 0xF3): stdout is written in the console code page when redirected to a file. The JSONL results are UTF-8 and unaffected | No change; no effect on scoring. A fix writes the log as UTF-8 and needs its own test | None |
| C4 | Observation | The post-click diagnostic read of Notepad's Edit step took 1.14 s and 1.19 s in run 10 (not scored) | No change | None |

The first finding of this round (budget sizing with retries) is resolved for E2E-03: run 10 ran in its own invocation with its own cap. It stays open for E2E-02, where the cap must count the retries (owner decision).

### Verification (continuation)

Before the live part, on `docs/s4-seventh-round-continuation` at `885cda7`, with no code change: `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia` (already up to date; binary SHA-256 `11a3d792…aca9da`), `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` (OK), `cargo test --workspace` (351 passed, 0 failed, 1 ignored), `python -m unittest discover -s tools/s4 -p "test_*.py"` (113 passed), `referencia_ejecutable/run_tests.py` (48 of 48) and `validar_paquete.py` (211 checks, 0 failed). Four result files changed only in timestamps, platform and Python strings, a duration and line endings, and were restored with `git restore`. After the documentation edits, the two thesis checks ran again (see the pull request).

### Live part (continuation)

- Ledger 423 → 426: three real requests (two in run 10, one in `--calc-standard`). The round has used 25 of 150.
- Run 10 passed: both clicks were grounded by identity and by the island descent (depth 3, `MenuItem` `Edit`), both OS-side menu checks passed (2 of 2), with no false success and no grounding mismatch.
- `--calc-standard` stopped at step 2 with `zero_candidates`. No second click was sent. The Calculator it had started was closed by the step.
- `stop.flag`: created by the client in the first part and removed for this round under the owner's authorization (the copy is kept as `stop.flag.r7-removed-by-coordinator-with-owner-authorization`). Never created in this continuation; absent at the end.
- No Ommatix, Calculator or test-application process left running. Both Notepad windows were restored and verified against the placement saved before run 10.
- No screen captures or screen text were saved.

### Not in this continuation

- E2E-02 (pilot and series): not run, because Standard mode was not reached.
- No client change, no matrix change and no regression test. The three new findings are recorded with their decisions; none has a fix in this round.


## Eighth round preparation: C1 to C3 and the S4R identifiers (branch `fix/s4-calc-standard-and-ids`)

Client change only, no live input. `tools/s4/e2e_driver.py` is now SHA-256 `a16c53afca63e21f629140ab33951f481b75dfd113866720ab947aa8272ae22d` (it was `60486d87…c214a`). The unit tests use a fake Calculator; the only read made on the real desktop was a read-only check that the new UI Automation script runs and returns an element by AutomationId in an open window (not the Calculator). That the Calculator's Standard item has AutomationId `Standard` is a hypothesis, not yet confirmed live.

### Findings and resolution (eighth round)

| # | Finding | Resolution | Regression test |
|---|---|---|---|
| C1 | `--calc-standard` found no element named exactly «Estándar» about 1.5 s after the navigation click; the record could not tell a pane that never opened from an item with another name or a pane that opened late, and `mode_after` was null | After the (grounded) navigation click the client polls UI Automation, read-only, for 3 s in steps of 150 ms, for the one on-screen element with AutomationId `Standard` and a control type in `ListItem`, `TreeItem`, `TabItem`, `MenuItem`, `RadioButton`. Its name is accepted only if it is `Estándar` or `Calculadora estándar` (NFC, exact) and is never recorded otherwise. The engine's element is then selected by that exact name (unique and confirmed) and the identity lookup must return the polled RuntimeId. Zero, several, an unknown name, no identifier or a timeout: no click, `standard_item.stopped_by`; a timeout also records `by_name` (counts and types under the accepted names, no names) | `StandardItemPollTest` (late pane found; `Calculadora estándar` selected and grounded; two candidates; unknown name not recorded anywhere; timeout; unreadable pane; other AutomationId with the same name; item the engine does not see; another RuntimeId under the same name; failed read by name), `StandardItemRuleTest` (closed lists, exact names, off-screen, parsing) |
| C1 (`mode_after`) | `mode_after` was null when the step stopped | The mode is read (read-only) in every outcome, before the Calculator the step started is closed; `unknown` when there is no window | `test_the_mode_after_is_recorded_when_the_step_stops` (five causes), `test_the_mode_after_is_read_even_when_the_mode_before_was_unreadable`, `test_the_mode_is_read_before_the_calculator_is_closed` |
| C2 | The `calc_standard` summary did not record the foreground wait (about 165 s in the continuation) | The summary records `foreground_wait_s`, `activation_requests` and `window_placements`, also when the window never arrives. Cause of the wait: still unverified | `test_the_foreground_wait_is_in_the_summary`, `test_the_foreground_wait_is_recorded_when_the_window_never_arrives` |
| C3 | The driver's log held a non-UTF-8 byte when redirected to a file | `main()` reconfigures standard output to UTF-8 (`use_utf8_stdout`) | `Utf8LogTest` |
| 5 | `plan_pruebas.csv` named E2E-02 «Editor» and E2E-03 «Explorador» while the S4 protocol ran the Calculator and Notepad flows under those identifiers (owner decision 2026-10-14) | The reduced flows are **S4R-02** (Calculator) and **S4R-03** (Notepad). The matrix gains both rows (S4R-02 `PENDIENTE_PRODUCTO`; S4R-03 `EJECUTADO_SUPERVISADO_PARCIAL`, ten of ten scored runs, 0 false successes); E2E-02 and E2E-03 are unchanged. Run records keep `e2e` and add `flow_id`. `validar_paquete.py` is unchanged: it checks uniqueness and traceability, not identifier patterns | `FlowIdTest`; `validar_paquete.py` (`pruebas_planificadas_unicas`) |

C4 (an observation) and findings 1 to 4 need no client change. The retry-aware budget of the Calculator series (finding 1) stays an owner decision.

### Tests

`python -m unittest discover -s tools/s4 -p "test_*.py"`: 143 passed. Run against the previous client, the 30 new tests fail (6 failures, 24 errors); the rest pass.
