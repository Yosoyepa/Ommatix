# Independent review of the paired M1 probe (PR 85)

Review of `795f2a2..21a31ad`: CHANGES REQUIRED (two P2). Finding, resolution, regression.
ADR-0035 stays Proposed; nothing here was run on a desktop.

## PB1 (P2). Whole-frame coverage could contradict the changed-tile count

Finding: the validator checked `group_tiles <= total_tiles` and, separately, that
`group_covers_whole_frame` matched a fraction of 1, but never the relation between them. A
report could claim both whole-frame coverage and an exclusion of changed tiles, and the
summary counted it as a reduction. Resolution: `freshness_probe.py` requires
`group_tiles == total_tiles` whenever a group covers the whole frame, for the ADR-0032 pair
fields and for `occluder_group`; valid legacy reports still parse. Regression:
`test_whole_frame_coverage_must_keep_every_changed_tile` (a valid full-coverage report, then
the ADR-0032 group, a legacy report and the amended group each claiming full coverage with
fewer group tiles: refused by `validate_report` and by `run`, with no file saved). It failed
first by accepting the impossible report.

## PB2 (P2). The selection margin was not bound to the measurement object

Finding: `read_with_margin` selected the members with its margin but `measure_occluder` took
a second margin, so a read at margin 0 measured at margin 5 returned `scoped = true` for a
smaller group than the engine selects at 5 (a window 4 px from the target omitted). Resolution:
`OccluderRead` keeps the selection margin, `measure_occluder` has no margin parameter and uses
it, `same_occluders` compares it, and the legacy `read` selects (and measures) at margin 0.
The ADR-0032 equality and `measure` are unchanged. Regression:
`probe_measures_at_the_margin_it_selected_with` (window at a 4 px gap: two windows and one
changed tile at margin 5, one window and none at margin 0, `read` equals margin 0, reads at
different margins are different occluder reads). Its first form called the old API with a
mismatched margin and failed with `Some(counts)` where `None` was expected.

## Note. `unattested_rect` is unreachable too

After a successful `read_with_margin` the complete window read and the ADR-0032 guards already
require the rectangles, so `unattested_rect` joins `walk_failed` and `box_outside_target` as
reserved vocabulary (enum docs and S4 §D.1.2). A missing entry in the fallback histogram is
not a measurement of those cases.
