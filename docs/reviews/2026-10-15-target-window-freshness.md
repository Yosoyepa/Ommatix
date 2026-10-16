# Target-window freshness implementation review (ADR-0032)

## 1. Extra OS reads must not outlive operator authorisation

Finding: checking window-group identity after the existing last look adds an OS walk. An operator stop engaged during that walk initially reached delivery. Resolution: revalidate stop, consent, foreground application, window scope and age after the new walk, before using its identity result; check age again at the final boundary in `target_window` mode. Regression: `the_operators_stop_during_the_final_group_read_sends_nothing` failed with no refusal before the correction and passes after it; `a_policy_change_keeps_its_refusal_before_group_identity_is_checked` preserves `POLICY_DENIED` priority. No real actuator was exercised.

## 2. Losing attestation must not authorise ignored pixels

Finding: an empty scoped diff cannot justify delivery when the final group read becomes unavailable. Resolution: fall back to a whole-frame comparison of the already captured frame. Regression: `losing_group_attestation_after_the_last_look_cannot_ignore_changed_pixels`, `changing_or_losing_group_attestation_around_the_action_capture_checks_the_whole_frame`, and `an_unstable_window_read_during_observation_keeps_whole_frame_freshness`.

## 3. Region geometry and identity are separate guards

Finding: pixel equality alone misses a transparent popup, a move and a z-order change; a bounding envelope or transitive closure would also compare unintended gaps. Resolution: retain the observation's OS window evidence, use the exact union of the two seeds and their direct overlaps, and compare the explicitly sorted identity after the last look. Regression: UT-58, UT-59, `a_group_change_after_the_last_look_spends_the_snapshot_without_a_pixel_change`, `the_window_group_is_one_hop_from_the_two_seeds_not_a_transitive_closure`, `window_read_order_is_normalized_by_z_order_then_handle`, and generated differential UT-63 (400 scoped cases, each paired with the whole-screen check). UT-60 preserves the global dimensions/epoch guard; UT-62 measures an effect outside the group. The point/foreground queries can omit rectangles: only the complete window list supplies them, so default `screen` does not add DWM reads to the existing window-policy gate.

## 4. A resized window must not disappear into the fallback

Finding: initially, a resized target window that no longer contained the old target box was treated as an unattested group. An unchanged whole frame could then pass without the definite rectangle change being refused. Resolution: separate pixel-scope containment from group identity. Complete evidence of a moved or disappeared observed seed still rejects, even if the point query becomes unavailable; only missing attestation invokes the fallback. Regression: `a_resized_window_that_no_longer_contains_the_target_still_refuses_by_identity` failed with `input_delivered` before the correction and passes after it; `a_known_resize_cannot_hide_behind_an_unavailable_point_attestation` covers the missing point read.

## 5. P1: an unavailable final read erased an already-attested group change

Independent review finding: with observed group A and action reads `[B, B, None]`, where B differs only in z-order and every frame byte is unchanged, the final unavailable read made both identity predicates false. Whole-frame equality then authorised delivery despite the earlier positive evidence. Resolution: each complete read records identity mismatch independently of pixel-scope availability, including before target-containment filtering. Each positive mismatch immediately spends the snapshot, so even a later policy refusal cannot revive it. The freshness result latches the first/second reads' known change through the last look; a missing or restored final read cannot clear it. The existing policy revalidation still precedes the identity refusal, which returns `STALE_SNAPSHOT` and spends the snapshot. A capture failure after an attested mismatch also spends it and returns `STALE_SNAPSHOT`, without fabricated D0 counts.

Fail-first regression: `a_known_group_change_refuses_even_when_the_final_read_is_unavailable` initially reported `input_delivered`; it now covers `[B, B, None]`, `[B, None, None]`, `[A, B, None]`, `[B, A, None]`, `[B, B, A]`, `[B, A, A]` and `[A, B, A]`, checks zero pixel differences, no input and a spent snapshot. `a_known_group_change_spends_the_snapshot_even_when_the_capture_fails` initially returned `CAPTURE_UNAVAILABLE` and now pins the same knowledge-preservation rule on that path. `unavailable_group_reads_without_a_known_change_keep_the_whole_frame_fallback` ensures missing evidence alone still uses the unchanged whole-frame fallback. The extended `a_policy_change_keeps_its_refusal_before_group_identity_is_checked` first preserves `POLICY_DENIED`, then verifies that restoring policy and group does not revive the already-spent observation; that additional assertion also failed before immediate spending was added.

## 6. Screen must preserve the existing age-check sequence

Independent review finding: the unconditional extra final age check could newly refuse `screen` when the bound expired during its foreground/window-policy checks. Resolution: run the additional final check only under `target_window`, where the new OS read needs it; `screen` retains its original gate, capture and last-look sequence. Fail-first regression: `screen_preserves_the_original_age_check_sequence_through_the_last_look` initially returned `STALE_SNAPSHOT`; an injected clock crosses the bound during the final foreground query and the corrected screen path preserves delivery with the original `foreground → capture → foreground → deliver` trace. `target_window_rechecks_age_after_the_last_foreground_read` confirms the optional mode still rejects the same expiry and sends nothing. These regressions use synthetic ports only, with no owner-screen capture or OS input.

## Synthetic measurement and limits

`cargo run --release -p ommatix-acceptance --example freshness_latency -- screen` (or `target_window`) measures an unchanged `SimulatedDesktop` at 640×400 with 20 warmup actions and 200 samples, on a monotonic injected clock. Its synthetic wrapper reaches freshness but delegates delivery to the simulator's zero-event receipt: no OS events are sent or claimed delivered. For the initial implementation measurements below, the after columns refer to `8297874`. Baseline is PR 1 commit `a54eb15`; the comparison uses the same source harness, omitting only the new scope selection/attestation when compiling the baseline. Windows 11 x64, `AMD64 Family 25 Model 33 Stepping 2, AuthenticAMD`, Rust 1.99.0.

| Stage, µs p50 / p95 | Before (`screen`) | After (`screen`) | After (`target_window`) |
|---|---:|---:|---:|
| Freshness | 410 / 754 | 421 / 810 | 415 / 775 |
| Gate | 0 / 2 | 1 / 2 | 1 / 2 |
| Delivery | 0 / 0 | 0 / 0 | 0 / 0 |
| Focus | 0 / 0 | 0 / 0 | 0 / 0 |
| Verification | 0 / 0 | 0 / 0 | 0 / 0 |
| Total | 411 / 756 | 422 / 812 | 416 / 778 |

The small simulated window covers almost the whole tile grid; these samples show overhead and scheduling noise, not an availability or latency improvement. No Windows window walk, owner-screen capture, DWM/GetWindowRect comparison, M1, M2 or IT-08 was executed. All visible intersecting top-level windows remain eligible, including background windows; an output-sized background can conservatively expand the group to the whole frame. `screen` remains the default, and requirement states and chapter 06 checkboxes are unchanged.

### Safety-review follow-up measurement (2026-10-16)

Same synthetic harness, machine, release build, 20 warmups and 200 samples per scope, comparing `8297874` against the corrections for findings 5 and 6. Each run again verified zero OS input delivery. The runs were sequential and include scheduling noise; no improvement is inferred from their differences.

| Stage, µs p50 / p95 | Before screen | Corrected screen | Before target_window | Corrected target_window |
|---|---:|---:|---:|---:|
| Freshness | 406 / 743 | 418 / 775 | 395 / 624 | 392 / 709 |
| Gate | 0 / 2 | 1 / 2 | 0 / 1 | 0 / 2 |
| Delivery | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Focus | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Verification | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Total | 407 / 744 | 420 / 777 | 396 / 625 | 393 / 709 |
