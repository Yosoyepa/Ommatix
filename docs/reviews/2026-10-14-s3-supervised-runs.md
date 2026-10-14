# S3 supervised runs: findings (2026-10-14)

Findings from the first supervised runs with real DXGI capture and real `SendInput` input
on the reference machine, limited to `notepad.exe` with disposable data (owner's
authorisation of 2026-10-14). The evidence, in Spanish, is
[`docs/thesis/S3_CORRIDAS_SUPERVISADAS.md`](../thesis/S3_CORRIDAS_SUPERVISADAS.md). Each
fixed finding ships with a test that fails without its fix (AGENTS.md rule 10).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major | A live click the OS accepted (the File menu opened) was reported `indeterminate` with `input_delivered: false`, and `delivered_actions` stayed 0, because both derived from the status alone. Chapter 07 §7.5 defines `input_delivered` as "the actuator reported delivery" and SDD §2.9 keeps delivery, visual change and postcondition apart; an agent told "not delivered" may act again on an effect that already happened | `ActionResult` carries `delivered`: true when the actuator's receipt was complete, also for an `indeterminate` whose effect could not be verified; false when the actuator failed or the OS took fewer events. Such deliveries count in `delivered_actions`. `verified` is unchanged. The DTO accepts either value only for `indeterminate`. Chapter 07 §7.5 states it (CHANGELOG 1.0.9); the output schema already allowed it | `an_unverified_effect_still_reports_the_delivered_input`, `an_indeterminate_result_reports_whether_its_input_was_delivered`, BDD-29 (`input_delivered: true`, `delivered_actions` 1); `unverifiable_or_partial_delivery_is_indeterminate_and_never_retried` now expects 2 delivered of 4 |
| 2 | Minor | The stderr `STOP engaged` line had 18 spaces in the middle: a source line continuation without its `\` | The line is built by `stop_notice` with a proper continuation | `the_stop_notice_reads_as_one_sentence` |
| 3 | Major (open) | `expected_effect: visual_change` was never observed (4 of 4 delivered clicks): the verification capture runs right after `SendInput`, before the application redraws. The menu change shows in the next observation, about 7 s later | Not fixed here: it needs a verification deadline that re-captures through the `Clock` port until the change shows or the deadline passes (the "plazo" of BDD-29). Fixed in [the follow-up](2026-10-14-s3-verify-settle-and-first-frame.md) | See the follow-up |
| 4 | Major (open) | The first DXGI frame of every session is blank (6 of 6 `serve` sessions; S0 §7); the first observation is useless and costs a full inference | Not fixed here; fixed in [the follow-up](2026-10-14-s3-verify-settle-and-first-frame.md) | See the follow-up |
| 5 | Minor (open) | The `SendInput` receipt (`submitted`/`inserted`) is not visible to the agent or on stderr, so IT-03 cannot count events | Not fixed in this PR | — |

## Notes

- Live requests sent: 8 of the 10 allowed; 4 clicks reached the OS, all on the `Archivo`
  menu label of a maximised Notepad window in the foreground; 3 were refused before
  anything was sent (`POLICY_DENIED` twice, `STALE_SNAPSHOT` once); 1 was a replay.
- Run B used the code of PR #8; run C, the code with findings 1 and 2 fixed.
- `tools/s3/mcp_driver.py` refuses to act unless Notepad is the maximised foreground
  window on the primary monitor and the menu label is unique; two visible Notepad menu bars
  made it stop (and engage the operator's stop) before sending anything.

## Verification

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p
ommatix --features dxgi,sendinput,onnx,winrt-ocr --all-targets -- -D warnings`, `cargo test
--workspace` and the thesis checks pass on `feat/s3-supervised-runs`.
