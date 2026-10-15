# S4 tenth round: findings (2026-10-14)

Findings from the tenth round of the supervised S4 runs on the reference machine. The round ran the Calculator's read-only mode check, the S4R-02 pilot and its ten scored runs, the E2E-02 editor probe and pilot, and the E2E-03 Explorer probe. S4R-02 passed its pilot and all ten scored runs, with no false success. The E2E-02 pilot did not pass, so its series is blocked under the owner's rule. The E2E-03 probe did not meet the flow's condition, so no Explorer pilot or series ran.

The round used the committed clients (`tools/s4/e2e_driver.py`, SHA-256 `143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`; `tools/s4/e2e_dossier.py`, SHA-256 `85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`), checked before each live invocation, with the engine and the editor rebuilt from `main` at `193e7ce` (`ommatix.exe` SHA-256 `38080631602ab2a0a1e94baf8432492d95fc6ec0db0cd6eee92232b956d0e5d3`; `ommatix-testeditor.exe` SHA-256 `32b8fadd2e97b73462043a53205247fbe4384f818119d769b829918cc2da88f8`) and the configurations `s4r10-live.toml` and `s4r10-explorer.toml`, copies of the ninth round's, unchanged. The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Décima ronda", and CHANGELOG 1.0.38.

No code changed in this round, and the clients stayed pinned for the live part.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major, E2E-02 (open; owner decision) | The save click («Guardar») was refused by the engine four times with `STALE_SNAPSHOT` (the request and three retries). No input was delivered for those four attempts. The typing step before it had been delivered and verified OS-side. The pilot did not pass, so under the owner's rule its series is blocked. The cause is not verified; a candidate is a change on screen between the observation and the click, such as the text caret | No change this round. The owner decides how to investigate: a read-only diagnostic with the owner present, or a client or editor change with a test that fails without it and a new hash, in a round of its own | None: no code change |
| 2 | Major, E2E-03 (open; owner decision) | The probe found no engine element labelled «carpeta-a» or with the root's name, in role `button`, so neither breadcrumb is a candidate. UI Automation reports a `SplitButton` for each. «Atrás» is an engine `button` with actionability `unknown`, and the client targets only confirmed elements; UI Automation reports it as a `Button` with AutomationId `backButton`. In the ninth round the engine saw «carpeta-a» as a confirmed element with zone `unknown`. The cause of the change is not verified | No change. The gate held: no pilot and no series ran, and no input was sent | None |
| 3 | Observation, owner's windows (open; owner) | At the start of the round the Notepad process 8960 had no top-level window and 23164 had two minimized ones. The ninth round recorded one minimized window per process. No client of this round had touched Notepad before that observation, and no client of this round targets it. The cause is not verified | No action. The baseline (23164 with two windows, 8960 with none) was checked before every invocation and held for the whole round | None |
| 4 | Observation, engine (closed) | One `STALE_SNAPSHOT` at one step of the S4R-02 series (run 3). The engine refused before delivering, the client retried, and the retry was delivered as `effect_observed` | Design behaviour (ADR-0007). No change | None |
| 5 | Observation, editor (closed for the probe) | The probe read six accessibility elements, and «Editor de texto» as a confirmed `input`. Ninth-round continuation finding 10 is therefore resolved for the probe: the engine now reads the editor's evidence. `uia_focus` is not recorded, because the client records it only when there is no accessibility evidence | No change | None |
| 6 | Observation, Calculator (closed) | The header of Standard mode is «Modo de calculadora Estándar». The mode rule (the name contains «estándar», and no Scientific-only control is on screen) accepts it, and the check returned `standard` with exit 0. This answers the open question of ninth-round continuation finding 8 for this build. The name is recorded only in the check's record, because it matched the rule | No change | None |

## Verification

Before the live part, from `main` at `193e7ce` on `docs/s4-tenth-round`, with no code change:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: no warnings.
- `cargo test --workspace`: 442 passed, 0 failed, 1 ignored.
- Editor: `cargo build --release`, `cargo clippy -- -D warnings` (no warnings), and `cargo test` (21 passed).
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 258 passed.
- From `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 of 48; `validar_paquete.py`, 449 checks and 0 failed. Four result files under `referencia_ejecutable/resultados/` changed only in the timestamp, the Python and platform strings, a duration and line endings, and were restored with `git restore`.
- Before each live invocation, a read-only preflight outside the repository checked the SHA-256 of the clients, the engine, the editor, the configurations and the pinned model and runtime files; that `stop.flag` and `window.pin` were absent; the ledger value; that no test process was running; that no visible `CabinetWClass` window existed; the Notepad window baseline; and that the client's temp tree was empty.

## Live part: counts and desktop

- Ledger 429 → 490: 61 real requests, within the 200 authorized (139 left under `--round-start 429`).
- Delivered: 56, all `effect_observed`. Verified OS-side: 56 of 56. Rejected: 5, all `STALE_SNAPSHOT` (1 in the S4R-02 series, 4 in the E2E-02 pilot). `indeterminate`: 0. `POLICY_DENIED`: 0.
- Inputs delivered: 55 Calculator clicks (Siete, Más, Ocho, Es igual a and Borrar, once per run in 11 runs) and one typing of the text into the editor's own field.
- S4R-02: pilot passed (5 of 5); 10 scored runs, 10 successes; false successes 0; `grounding_wrong` 0.
- E2E-02: probe passed; pilot did not pass (finding 1); no scored runs.
- E2E-03: probe did not meet the gate (finding 2); no pilot, no series.
- `stop.flag` never created, absent at the end. `window.pin` never created, absent at the end.
- No test process left running. The Calculator and the editor were closed by the client in each invocation. The Explorer window was opened and closed by the client, and its temporary tree was deleted. The owner's Notepad baseline and the shell's hidden `CabinetWClass` window were unchanged.
- No screen capture and no screen text saved. The logs hold statuses, codes, counts, timings, control types, `AutomationId` and the protocol's fixed labels.

## Not in this round

- E2E-02 series: not run, because the pilot did not pass.
- E2E-03 pilot and series: not run, because the probe did not meet the gate.
- No client, engine, editor or Rust change, and no regression test: none of the findings is shown to be a defect that a code change would fix.

## What the next round needs

- The owner's decision on finding 1 (how to investigate the `STALE_SNAPSHOT` on the save click, and whether the E2E-02 pilot is repeated in a round of its own).
- The owner's decision on finding 2 (accept another identification of the breadcrumbs, change the navigation contract in the dossier, or investigate why the engine no longer sees «carpeta-a»).
- Whether the S4R-02 matrix change in this branch (`PENDIENTE_PRODUCTO` to `EJECUTADO_SUPERVISADO_PARCIAL`) stands.
- The cap: 139 requests remain under `--round-start 429`, or a new round can start its own cursor at 490.

## Resolution of findings 1 and 2 (branch `fix/s4-round10-blockers`, CHANGELOG 1.0.39)

Findings 1 and 2 above were left open for the owner. This section is their resolution; its own rows are numbered R1 to R5. They are E2E-02's save click refused `STALE_SNAPSHOT`, and E2E-03's breadcrumbs missing from the engine's elements. Branch `fix/s4-round10-blockers` (CHANGELOG 1.0.39). **No live request and no input were sent** (ledger 490, unchanged). The only checks on the system were read-only: a File Explorer window opened by this work on a temporary folder it created (read with UI Automation, closed by its handle with `WM_CLOSE`, the folder deleted), and the editor's own tests, which run the toolkit headless. The engine's freshness gate, the confirmation rule (enabled, on screen, an action pattern) and ADR-0017 (accessibility never replaces the visual path) are unchanged.

### Findings and resolution

| # | Severity | Finding | Cause | Resolution | Regression test |
|---|---|---|---|---|---|
| R1 | Major, E2E-02 | The save click was refused `STALE_SNAPSHOT` four times in a row after the typing | **Verified from the code and in the toolkit, not in a live window.** The engine delivers only if a frame captured right before delivery has no dirty tile against the committed base (`act.rs`, `fresh_frame`; no tolerance). The editor's toolkit paints a blinking text caret by default (`text_cursor.blink = true`, 0.5 s on and 0.5 s off, from the last edit), and the window is in front with the field focused after the typing, so the screen changes twice a second. The four retries were 0.89 to 0.91 s apart, about one blink period, which is consistent with the same phase recurring (not proof: the live phases were not recorded) | The editor sets the caret to stay on (`configure`), so the editor paints the same frame at any time. The engine is unchanged | `a_focused_field_paints_the_same_frame_at_any_time` (fails with `blink = true`; checked), `the_toolkits_default_caret_blinks` (the premise: the default does change the frame), `the_caret_stays_on_in_both_themes` (fails with `blink = true`; checked). Editor: 24 tests, 21 before |
| R2 | Major, E2E-03 | The engine had no `button` element for the breadcrumbs «carpeta-a» and the root, though UI Automation reports a `SplitButton` each | **Verified by reproduction, not in the live run.** Not a rule: read to completion, the adapter returns both breadcrumbs as confirmed `button`s (enabled, `Invoke` and `ExpandCollapse` available), so the role map, the action-pattern test, name normalisation and the confirmation rule are not the cause. The cause is the walk budget: one 250 ms budget covered the whole read, about 50 ms of it went to listing the session's 13 top-level windows before anything was read (UI Automation costs about 4 ms per window), and the walk was then depth first, so it reached the breadcrumbs at the 83rd of 118 visited elements, after Explorer's navigation pane. On an idle desktop it needed a 200 ms budget to reach them (0 of 6 at 180 ms); the engine's accessibility stage lasted 267 ms and 271 ms in the two Explorer probes, the signature of a walk that ended on its budget (the probe records do not say whether a read was truncated) | The walk reads the window that holds the keyboard focus first, lists the process's other windows only if budget remains, and visits each window breadth first. Bounds, evidence and the confirmation rule are unchanged | `a_walk_reads_the_chrome_before_the_content`, `a_walk_cut_by_elements_loses_the_deepest_nodes_not_a_later_sibling`, `a_walk_cut_by_time_stops_where_the_clock_runs_out`, `a_skipped_subtree_is_not_entered`, `the_depth_bound_is_reported_only_where_something_lies_below_it` (five tests of `walk`; all five fail with a depth-first order; checked). Manual: `reads_the_breadcrumbs_of_a_file_explorer_window` (ignored) |
| R3 | Observation, E2E-03 (closed) | «Atrás» was an engine `button` with actionability `unknown` | **Verified.** On a window that has not navigated, UI Automation reports Back as disabled (`IsEnabled` false), and a disabled element is never confirmed. The flow clicks it as its third step, after two navigations, when it is enabled. The probe, which sends no input, cannot show it confirmed | None. The next probe should expect `unknown` for Back and confirmed `button`s for the two breadcrumbs | None |
| R4 | Observation, ninth round (closed) | Finding 2 of the tenth round said the engine saw «carpeta-a» in the ninth round | **Verified from the records.** In the ninth-round probe, «carpeta-a» was the navigation pane's tree item (UI Automation `ListItem`, engine role `unknown`) and the root's name was the tab title (`TabItem`, role `unknown`): neither was a breadcrumb. The engine never saw a breadcrumb in either round | Corrects the tenth round's reading; no code | None |
| R5 | Finding for the owner and the thesis (open; owner decision) | A real editor with a blinking caret would have every action after typing refused `STALE_SNAPSHOT` | The freshness check compares whole frames, and a caret is a change of a few pixels twice a second in the one place the next action is likely to be. Fix 1 removes the blink from our own application, not from the world | **Not implemented.** Options for the owner, each needing a dossier change and an ADR: (a) a tolerance for dirty tiles confined to the focused field's caret area, which the accessibility evidence can bound (`keyboard_focus` gives the element's rectangle); (b) comparing after masking the focused element's rectangle only for `type` follow-ups; (c) an operational rule for the operator (the system's caret blink rate, which some toolkits honour and others, like this editor's, do not) with no engine change; (d) the client retrying until a frame matches, which only works by luck against a 1 s period. Every option weakens or works around a safety property (SDD §2.9), so none is taken here | None |

### What changed

- `tools/s4/testeditor/src/main.rs`: `configure` (light theme, caret blink off in both themes), the field has a fixed `Id`, `EditorApp::show`, three tests, one pre-existing unchecked `Duration` subtraction in a test folded so that `cargo clippy --all-targets` passes on the editor.
- `src/adapters/accessibility/src/lib.rs`: `walk` (breadth first, bounded; `Walked`) and its five tests.
- `src/adapters/accessibility/src/uia.rs`: `foreground` returns the foreground window too; `snapshot_of` walks it first and lists the other windows afterwards (skipping the one already read); `walk_window` uses `walk`; the manual test above.

### Measurements

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

### Verification

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: no warnings.
- `cargo test --workspace`: 447 passed (442 before), 0 failed, 1 ignored.
- Editor: `cargo test`, 24 passed (21 before); `cargo clippy --all-targets -- -D warnings`, no warnings.
- Python and thesis checks: see CHANGELOG 1.0.39 and the S4 document, «Preparación de la undécima ronda».

### Hashes

`tools/s4/e2e_driver.py` and `tools/s4/e2e_dossier.py` are unchanged (`143ea454…aee3d`, `85111edb…316df`). Rebuilt from this branch in its worktree: `ommatix.exe` (`cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`) `6e8f6fdde0027eaf9ca16946cfc31685677cb5d7129a06b1c9e324a705b4bb52`; `ommatix-testeditor.exe` (`cargo build --release` in `tools/s4/testeditor`) `868cadb7195e7aadf428f9ece056d638ce0f7e20ebb2eabf21706fd3f5debb9d`. A build in another directory differs (debug information carries paths); whoever runs the next round rebuilds and records their own.

### Not verified

- That the caret is what refused the live click: no input could be sent to put the caret on in a real window. The harness shows the toolkit's default changes the frame and the configured editor does not.
- That the live Explorer reads ended on their budget: the records have the stage time, not the `truncated` flag. The reproduction shows that the old order needed 200 of 250 ms to reach the breadcrumbs on an idle machine.
- That the engine, with the new order, delivers a unique usable `button` for each breadcrumb, and that clicking the breadcrumb's centre acts on its name part (a `SplitButton` also has an arrow). That is for the next probe and pilot.
