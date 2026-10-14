# S3 follow-up: verification settle window and DXGI first frame (2026-10-14)

Resolution of the two open findings of the
[S3 supervised runs](2026-10-14-s3-supervised-runs.md) (findings 3 and 4), on branch
`fix/s3-verify-settle`, stacked on `feat/s3-supervised-runs`. The evidence is in
[`docs/thesis/S3_CORRIDAS_SUPERVISADAS.md`](../thesis/S3_CORRIDAS_SUPERVISADAS.md) §6;
the dossier change is CHANGELOG 1.0.10. Both fixes still need a new supervised run with
real input.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 3 | Major | `expected_effect: visual_change` was never observed live (4 of 4 delivered clicks on Notepad's File menu were `indeterminate`): the single verification capture ran right after `SendInput`, before the application redrew | Effect verification has a bounded settle window, `EngineConfig::verify` (`VerifyParams`), from `[engine] verify_timeout_ms` (default 500, at most 5000) and `verify_poll_ms` (default 50, 1..1000). After delivery the engine captures at once and, while nothing differs from the pre-delivery frame, sleeps `verify_poll_ms` on the injected `Clock` and captures again until the window has elapsed (measured on the clock, and bounded by a capture count so a clock that never advances cannot loop). A change is `effect_observed`; none is `indeterminate` (`INDETERMINATE_EFFECT`, `reconcile_effect`). Only captures repeat: input is never resent and nothing is published. 0 keeps the old single immediate check. SDD §2.9 and chapter 07 §7.4 state it; the template gains the two keys; BDD-29's deadline is this window | `visual_change_after_a_redraw_delay_is_observed` (redraw on the 4th check, 150 ms on the `ManualClock`), `no_change_within_the_timeout_is_indeterminate` (11 checks over 500 ms; a non-dividing poll ends exactly at the deadline; 0 is one check), `verification_never_resends_input`, `the_verification_window_is_configurable_within_bounds`, the BDD-29 step (the clock advanced by the whole window) |
| 4 | Major | The first DXGI frame of every session was blank (6 of 6 `serve` sessions; S0 §7). The first update after creating the duplication carries no presented image (`LastPresentTime == 0`), and with no last frame to fall back on, its blank texture was copied as frame 1 | A pure decision, `classify_update(last_present_time, has_last_frame, waited)`, chooses copy, reuse the last frame, acquire again (with what is left of the 500 ms first-capture wait) or fail. `mod win` loops on it: without a last frame of the current size it releases updates with no presented image until one arrives, and returns a `PortError` (`CAPTURE_UNAVAILABLE` upstream) if none arrives within 500 ms, never a blank frame | `a_first_update_without_a_desktop_image_is_never_a_frame`; the manual `dxgi_captures_one_frame_of_output_0` now asserts the first frame is not uniform (in memory only): it failed on the code of the runs and passes with the fix on the reference machine (output 0, 1920×1080, 1 dirty rectangle on the first frame instead of 0) |

## Notes

- The settle window adds up to `verify_timeout_ms` of latency to a delivered action whose
  effect does not show, and one capture per `verify_poll_ms` (each DXGI capture waits at
  most its 10 ms acquire timeout). Actions with `expected_effect: none` or with effects the
  engine cannot verify yet do not wait.
- No real input was sent for this change; the manual DXGI test only reads one frame into
  memory and never saves pixels.

## Verification

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p
ommatix --features dxgi,sendinput,onnx,winrt-ocr --all-targets -- -D warnings`, `cargo
clippy -p ommatix-adapter-capture --features dxgi --all-targets -- -D warnings`, `cargo test
--workspace` and the thesis checks pass on `fix/s3-verify-settle`.
