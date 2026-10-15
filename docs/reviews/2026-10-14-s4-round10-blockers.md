# S4 tenth-round blockers: findings and resolution (2026-10-14)

Resolution of the two findings the tenth round left open ([`2026-10-14-s4-tenth-round.md`](2026-10-14-s4-tenth-round.md), findings 1 and 2): E2E-02's save click refused `STALE_SNAPSHOT`, and E2E-03's breadcrumbs missing from the engine's elements. Branch `fix/s4-round10-blockers` (CHANGELOG 1.0.39). **No live request and no input were sent** (ledger 490, unchanged). The only checks on the system were read-only: a File Explorer window opened by this work on a temporary folder it created (read with UI Automation, closed by its handle with `WM_CLOSE`, the folder deleted), and the editor's own tests, which run the toolkit headless. The engine's freshness gate, the confirmation rule (enabled, on screen, an action pattern) and ADR-0017 (accessibility never replaces the visual path) are unchanged.

## Findings and resolution

| # | Severity | Finding | Cause | Resolution | Regression test |
|---|---|---|---|---|---|
| 1 | Major, E2E-02 | The save click was refused `STALE_SNAPSHOT` four times in a row after the typing | **Verified from the code and in the toolkit, not in a live window.** The engine delivers only if a frame captured right before delivery has no dirty tile against the committed base (`act.rs`, `fresh_frame`; no tolerance). The editor's toolkit paints a blinking text caret by default (`text_cursor.blink = true`, 0.5 s on and 0.5 s off, from the last edit), and the window is in front with the field focused after the typing, so the screen changes twice a second. The four retries were 0.89 to 0.91 s apart, about one blink period, which is consistent with the same phase recurring (not proof: the live phases were not recorded) | The editor sets the caret to stay on (`configure`), so the editor paints the same frame at any time. The engine is unchanged | `a_focused_field_paints_the_same_frame_at_any_time` (fails with `blink = true`; checked), `the_toolkits_default_caret_blinks` (the premise: the default does change the frame), `the_caret_stays_on_in_both_themes` (fails with `blink = true`; checked). Editor: 24 tests, 21 before |
| 2 | Major, E2E-03 | The engine had no `button` element for the breadcrumbs «carpeta-a» and the root, though UI Automation reports a `SplitButton` each | **Verified by reproduction, not in the live run.** Not a rule: read to completion, the adapter returns both breadcrumbs as confirmed `button`s (enabled, `Invoke` and `ExpandCollapse` available), so the role map, the action-pattern test, name normalisation and the confirmation rule are not the cause. The cause is the walk budget: one 250 ms budget covered the whole read, about 50 ms of it went to listing the session's 13 top-level windows before anything was read (UI Automation costs about 4 ms per window), and the walk was then depth first, so it reached the breadcrumbs at the 83rd of 118 visited elements, after Explorer's navigation pane. On an idle desktop it needed a 200 ms budget to reach them (0 of 6 at 180 ms); the engine's accessibility stage lasted 267 ms and 271 ms in the two Explorer probes, the signature of a walk that ended on its budget (the probe records do not say whether a read was truncated) | The walk reads the window that holds the keyboard focus first, lists the process's other windows only if budget remains, and visits each window breadth first. Bounds, evidence and the confirmation rule are unchanged | `a_walk_reads_the_chrome_before_the_content`, `a_walk_cut_by_elements_loses_the_deepest_nodes_not_a_later_sibling`, `a_walk_cut_by_time_stops_where_the_clock_runs_out`, `a_skipped_subtree_is_not_entered`, `the_depth_bound_is_reported_only_where_something_lies_below_it` (five tests of `walk`; all five fail with a depth-first order; checked). Manual: `reads_the_breadcrumbs_of_a_file_explorer_window` (ignored) |
| 3 | Observation, E2E-03 (closed) | «Atrás» was an engine `button` with actionability `unknown` | **Verified.** On a window that has not navigated, UI Automation reports Back as disabled (`IsEnabled` false), and a disabled element is never confirmed. The flow clicks it as its third step, after two navigations, when it is enabled. The probe, which sends no input, cannot show it confirmed | None. The next probe should expect `unknown` for Back and confirmed `button`s for the two breadcrumbs | None |
| 4 | Observation, ninth round (closed) | Finding 2 of the tenth round said the engine saw «carpeta-a» in the ninth round | **Verified from the records.** In the ninth-round probe, «carpeta-a» was the navigation pane's tree item (UI Automation `ListItem`, engine role `unknown`) and the root's name was the tab title (`TabItem`, role `unknown`): neither was a breadcrumb. The engine never saw a breadcrumb in either round | Corrects the tenth round's reading; no code | None |
| 5 | Finding for the owner and the thesis (open; owner decision) | A real editor with a blinking caret would have every action after typing refused `STALE_SNAPSHOT` | The freshness check compares whole frames, and a caret is a change of a few pixels twice a second in the one place the next action is likely to be. Fix 1 removes the blink from our own application, not from the world | **Not implemented.** Options for the owner, each needing a dossier change and an ADR: (a) a tolerance for dirty tiles confined to the focused field's caret area, which the accessibility evidence can bound (`keyboard_focus` gives the element's rectangle); (b) comparing after masking the focused element's rectangle only for `type` follow-ups; (c) an operational rule for the operator (the system's caret blink rate, which some toolkits honour and others, like this editor's, do not) with no engine change; (d) the client retrying until a frame matches, which only works by luck against a 1 s period. Every option weakens or works around a safety property (SDD §2.9), so none is taken here | None |

## What changed

- `tools/s4/testeditor/src/main.rs`: `configure` (light theme, caret blink off in both themes), the field has a fixed `Id`, `EditorApp::show`, three tests, one pre-existing unchecked `Duration` subtraction in a test folded so that `cargo clippy --all-targets` passes on the editor.
- `src/adapters/accessibility/src/lib.rs`: `walk` (breadth first, bounded; `Walked`) and its five tests.
- `src/adapters/accessibility/src/uia.rs`: `foreground` returns the foreground window too; `snapshot_of` walks it first and lists the other windows afterwards (skipping the one already read); `walk_window` uses `walk`; the manual test above.

## Measurements

One File Explorer window on a temporary folder (two nested folders and a file), idle desktop, 16 logical processors, 13 top-level windows in the session, release build of the adapter's test, six runs per cell. «Breadcrumbs» is how many of six runs returned both breadcrumbs as confirmed buttons within the budget (`Budget::max_duration`; the default is 250 ms).

| Budget | Before (depth first, windows listed first) | After | Before, 8 busy threads | After, 8 busy threads |
|---|---|---|---|---|
| 80 ms | 0 of 6 | 1 of 6 | 0 of 6 | 0 of 6 |
| 100 ms | 0 of 6 | 6 of 6 | 0 of 6 | 6 of 6 |
| 140 ms | 0 of 6 | 6 of 6 | 0 of 6 | 6 of 6 |
| 180 ms | 0 of 6 | 6 of 6 | 0 of 6 | 6 of 6 |
| 200 ms | 6 of 6 | 6 of 6 | 5 of 6 | 6 of 6 |
| 250 ms | 6 of 6 | 6 of 6 | 6 of 6 | 6 of 6 |

The complete read is not slower: 63 elements in 189 to 197 ms after and 193 ms before (release builds; the walk visits 118 elements, and listing the windows costs about 50 ms in either order). The margin at the default budget goes from 1.25 times (200 of 250 ms) to about 2.5 times (100 of 250 ms). The live engine runs the walk concurrently with capture and the two models, which is why the margin matters; the sweep with busy threads approximates that and is not the engine's load. These are one machine and one session, not RNF-05 measurements (S0 §10). The worst case of a read is now the budget plus one listing of the windows (about 50 ms here) when the focused window uses the whole budget minus a few milliseconds, instead of the budget alone.

## Verification

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: no warnings.
- `cargo test --workspace`: 447 passed (442 before), 0 failed, 1 ignored.
- Editor: `cargo test`, 24 passed (21 before); `cargo clippy --all-targets -- -D warnings`, no warnings.
- Python and thesis checks: see CHANGELOG 1.0.39 and the S4 document, «Preparación de la undécima ronda».

## Hashes

`tools/s4/e2e_driver.py` and `tools/s4/e2e_dossier.py` are unchanged (`143ea454…aee3d`, `85111edb…316df`). Rebuilt from this branch in its worktree: `ommatix.exe` (`cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`) `6e8f6fdde0027eaf9ca16946cfc31685677cb5d7129a06b1c9e324a705b4bb52`; `ommatix-testeditor.exe` (`cargo build --release` in `tools/s4/testeditor`) `868cadb7195e7aadf428f9ece056d638ce0f7e20ebb2eabf21706fd3f5debb9d`. A build in another directory differs (debug information carries paths); whoever runs the next round rebuilds and records their own.

## Not verified

- That the caret is what refused the live click: no input could be sent to put the caret on in a real window. The harness shows the toolkit's default changes the frame and the configured editor does not.
- That the live Explorer reads ended on their budget: the records have the stage time, not the `truncated` flag. The reproduction shows that the old order needed 200 of 250 ms to reach the breadcrumbs on an idle machine.
- That the engine, with the new order, delivers a unique usable `button` for each breadcrumb, and that clicking the breadcrumb's centre acts on its name part (a `SplitButton` also has an arrow). That is for the next probe and pilot.
