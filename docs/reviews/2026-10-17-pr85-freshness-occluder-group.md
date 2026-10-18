# Independent review of PR 85 (ADR-0035 occluder group)

Review of draft PR #85 at `305be66`: CHANGES REQUIRED. Each finding, its resolution and the
test that covers it. ADR-0035 stays Proposed and `screen` stays the default.

## F1 (P1). A failed extended-style query was treated as an opaque style

Finding: `GetWindowLongPtrW` returns 0 on failure and for an empty style, and the error was
never read, so a failed query plus a backdrop of `NONE` attested an opaque window.
Resolution: `SetLastError(0)` before and `GetLastError` after the call, each in its own
`unsafe` block; `extended_style_of` turns a zero with a nonzero error into `None`, and
`window_opacity_of` takes that result. A zero with no error stays a valid empty style.
Regression: `ut_75_a_failed_style_query_is_not_an_empty_style` (`sendinput.rs`); the engine
path is `ut_70_losing_the_opacity_at_any_look_takes_the_whole_frame_path`. The pure seam did
not exist before, so the identical test cannot compile against the old commit; that does not
make a behavioural red test impossible. The round-3 reviewer checked the seam and tests in
memory with the old behaviour as a mutation (ignoring the zero-style error): the test fails
behaviourally with it and passes without it.

## F2 (P1). DirectComposition transparency was attested opaque

Finding: a window with `WS_EX_NOREDIRECTIONBITMAP` can carry per-pixel alpha with neither
layered bit nor a backdrop. Resolution: the bit (`0x0020_0000`) is rejected in
`window_opacity_of`, alone and in combinations; no other attestation exists for it.
Regression: `ut_75_a_window_without_a_redirection_bitmap_is_never_attested_opaque` (failed on
the old predicate with `Some(WindowOpacity { corner_exclusion_px: 32 })`), and the engine
whole-frame path in `ut_70_losing_the_opacity_at_any_look_takes_the_whole_frame_path`.

## F3 (P2). An identity refusal reported a group when opacity was unavailable

Finding: `freshness_refusal` received `Some(expected)` whenever an observed group existed, so
`group_tiles` was `Some(0)` with the opacity unattested. Resolution: `FreshFrame` keeps
`scoped` (the scope was usable around the capture) apart from `expected`; the refusal reports
the group only if `scoped` and the last look still attests it. The refusal and the spent
snapshot are unchanged. Regression:
`ut_70_an_identity_refusal_without_attested_opacity_omits_the_group_count` (failed first with
`group_tiles = Some(0)`), covering opacity never attested and lost at the last look.

## F4 (P2). The subset claim of UT-74 is false for positive margins

Finding: at margin 5 a window 4 px from the target enters ADR-0035's group and not
ADR-0032's. Ruling (coordinator): the rule stays as written. Resolution: the subset against
ADR-0032 is asserted at margin 0 only; the ADR notes and the UT-74 plan row record the
conservative positive-margin extension. Regression:
`ut_74_a_window_that_enters_only_through_the_margin_refuses_where_adr_0032_accepts` and
`ut_74_generated_layouts_match_the_scalar_reference_for_every_margin` (deterministic
rectangles and relative z-orders, an independent scalar reference per margin 0, 1, 5, 64; it
fails if the margin is dropped from the membership test).

## F5 (P3). The plan rows still specified the replaced group

Resolution: UT-57, UT-59 and UT-63 in `plan_pruebas.csv` now describe the amended rule and
trace to ADR-0032 and proposed ADR-0035, keeping ADR-0032's group as the M1 baseline; UT-70,
UT-74 and UT-75 carry the new cases. No requirement state or box changed.

## Inherited gap (P2). Enumeration error and normal end were conflated

Finding: `GetWindow(GW_HWNDNEXT).ok()` discarded the error, so a failed enumeration with a
valid cursor returned a partial list. Resolution: `next_sibling` clears the error before the
call and reads it right after, with no other native call between; `sibling` separates the end
from a failure, and `walk_windows` (pure, generic) returns `None` unless the walk ended
normally. This also hardens the ADR-0032 path, which shares the walk. Regression:
`a_failed_enumeration_is_not_a_complete_walk` (scripted walk: normal end, error with a valid
cursor, destroyed cursor, cycle, null handle). As for F1, the round-3 reviewer mutated the old
behaviour (a missing sibling with an error taken as the end): the test fails with the
mutation and passes with the fix. No historical red run is claimed before the seam existed.

## R1 (round 3, P2). Omitting an unattested group also suppressed the whole-frame scan

Finding: the F3 fix passed `None` as the reportable group on an identity refusal after a
regional comparison, and `freshness_refusal` used that same value to decide whether to scan
the whole frame, so the regional tile count was published as the global `total_tiles`. With a
changed external pixel, a final foreground change and the opacity lost at the last look, the
diagnostic said 0 instead of 1. The refusal itself, the spent snapshot and the missing
delivery were right; only the D0 count was wrong. Resolution: `freshness_refusal` takes
`regional` (the supplied diff was computed over the group's regions) and scans the whole frame
when the group may be reported or the diff is regional; `group_tiles` stays omitted without an
attested group, and no second capture is made. Regression:
`ut_70_an_identity_refusal_after_a_regional_comparison_still_counts_the_whole_frame`
(failed first with `(0, None)` against the expected `(1, None)`; no delivery; the retry on the
spent snapshot is refused).
