# Live regression round: the menu close was refused twice with `STALE_SNAPSHOT` (2026-10-15)

Found in Run 2 (step `it03`) of the supervised live regression round of 15 October 2026
(thesis `S3_CORRIDAS_SUPERVISADAS.md` §9, CHANGELOG 1.0.74). The round ran the engine built from
`6278b6f` (code identical to `7a91354`) with `dxgi,sendinput,directml,winrt-ocr,uia`, on the
reference PC at 100 % scaling, with the owner present. No code changed in the PR that records
this finding. The raw client logs (no labels, no coordinates) stay outside the repository; every
figure below comes from them.

## Findings and resolution

| # | Severity | Finding | Resolution | Test that covers the behaviour |
|---|---|---|---|---|
| 1 | Observation (fail-closed behaviour; not a safety failure) | After `s3-it03-open-1` was delivered (`effect_observed`), `s3-it03-close-1` and `s3-it03-close-2` were each refused with `STALE_SNAPSHOT` ("the screen changed since the observation; nothing was delivered; observe again"), after a fresh observation each time (`frame_seq` 7 and 9), 5.3 and 5.6 ms round trip, `input_delivered: false`. The close was delivered in the same round in Run 1 (after the `sec02r` step) and in Run 2b, and in S3 §8.2. | **None needed in the engine:** the refusal is the specified behaviour (SDD §2.6 and §2.9: a frame captured right before delivery must still be the committed base, otherwise nothing is delivered and a new observation is asked for). **Follow-up, not done here:** a settle observation in `tools/s3/mcp_driver.py` before the close; a measurement of idle observation pairs; an engine diagnostic that counts differing tiles on a freshness refusal; and, if the cause is confirmed as a harmless blink, a design decision on validating freshness by unchanged region (SDD §2.6 already allows it "after proving its correctness"). All in S3 §9.6. | `a_change_before_delivery_is_not_an_observed_effect` (`src/application/src/engine/tests.rs`: the screen changes after the observation, the live click is refused with `STALE_SNAPSHOT`, nothing is delivered, `delivered_actions` stays 0); `a_live_action_after_a_scaling_change_is_stale_and_sends_nothing` (same `fresh_frame` branch through a geometry change); `ut_38_action_on_previous_snapshot_is_stale_after_a_new_observation`; `an_action_before_any_observation_is_stale`; BDD-38 for the age bound (`stale_snapshot_asks_for_a_new_observation`, `src/acceptance/tests/thesis.rs`) |

## Analysis

- **What the engine compares.** `Engine::fresh_frame` (`src/application/src/engine/act.rs`)
  captures the screen again before every live delivery and calls `dirty_tiles` against the
  committed base (`src/domain/src/incremental.rs`). One differing tile, or a full refresh, is
  `STALE_SNAPSHOT`, and the snapshot is spent. The comparison covers the whole frame (1920x1080 on
  output 0), is exact per pixel on the three colour channels of each 64x64 tile
  (`tile_changed`; only alpha and row padding are ignored) and has no tolerance.
- **What the base is.** The frame captured when the observation started (`captured_at` is that
  instant). The distance to the freshness capture is therefore about the observation's own
  duration, reported as `age_ms`.
- **Figures from the round.** The observation before an opening click was light (no regions to
  analyse, `age_ms` 1-2) in 4 of 4 cases and the click was delivered 4 of 4. The observation before a
  closing click is heavy (it re-analyses the changed regions: `age_ms` 121-155): delivered in the
  Run 1 close (155 ms) and the Run 2b close (121 ms), refused in both Run 2 attempts (138 and 133 ms).
  The screen also changed by itself between observations with no input sent: Run 2 `frame_seq` 9
  to 11 (about 1.2 s) and Run 1 `frame_seq` 7 to 8 (about 1.7 s) re-analysed regions, and so did 2 of 3
  dry-run pairs about 125 ms apart (menu closed), against 0 of 9 pairs 15-30 ms apart.
- **Hypothesis (not verified).** Something on output 0 changes periodically or intermittently, even a
  single pixel, and a heavy observation keeps a window of about 140 ms open in which that change
  fails the exact freshness comparison. The leading candidate is the text caret blink of the
  Notepad editing area: in the tenth S4 round the blinking caret of the test editor caused
  `STALE_SNAPSHOT` after typing and was fixed in the editor (`docs/reviews/2026-10-14-s4-tenth-round.md`,
  CHANGELOG 1.0.39). Other candidates: the menu's opening animation or hover highlight, and any other
  content on output 0 (the taskbar clock changes every minute). With four heavy observations
  (two passed, two refused) the causes cannot be separated or chance excluded, and the round did not
  record which tiles differed.
- **Why it is not a defect.** The refusal sent nothing (`delivered_actions` 1 at the end of Run 2,
  `rejected_actions` 3, the stored result replayed without delivery, `REQUEST_CONFLICT` on a reused
  `request_id` with other arguments). It is the same fail-closed behaviour the owner accepted for
  the first `type` refusal in the tenth round (review finding 14, "a refusal before input is
  allowed"). What it costs is availability: a live flow that follows a visual change may need more
  than one observation.
- **State of the menu.** Unknown after Run 2; Run 2b began with a normal observation and the client
  does not record the menu state.

## Notes

- **Not changed here.** No code, configuration or test. The follow-ups need their own PRs, each with
  a test that fails without it (AGENTS.md rule 10) where it changes behaviour.
- **Coverage gap noticed.** No unit test changes a single pixel of an otherwise stable frame between
  the observation and the live action; the cases above change the screen through a scripted redraw
  (`reactions`) or a geometry change. The engine's tile comparison itself has the differential test
  `ut_08_differential_tile_diff_matches_scalar_reference_on_generated_frames` (`src/domain/src/incremental.rs`),
  so the gap is at the engine boundary, not in the diff.
