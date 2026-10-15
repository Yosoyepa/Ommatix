# S4 eighth round: findings (2026-10-14)

Findings from the eighth round of the supervised S4 runs on the reference machine. Nothing was scored. `--calc-standard` found the Standard item with its `AutomationId` but refused its name, so it stopped after one live request (the navigation click). The editor and the Explorer never reached the foreground maximized, so no pilot passed and no series ran. The round sent one live request of the 200 authorized (ledger 426 → 427), and `stop.flag` was never created.

The round used the committed clients (`tools/s4/e2e_driver.py`, SHA-256 `a16c53afca63e21f629140ab33951f481b75dfd113866720ab947aa8272ae22d`; `tools/s4/e2e_dossier.py`, SHA-256 `cd959f590fb02f7abc64f903fa1a7773fec25ed34ed8ad214b7409228157f306`), the engine binary built from `main` at `b7a5e7b` (`3a451211763f8ef9c92adb98955dca7b42dedf58974c97248bc97aac513cbb1a`), the editor binary (`a6765dca0b5e63b45cfd3ab8a28b470a0896a3dcdfe1ea062cb48c8ff745119d`) and the configurations `s4r8-live.toml` (`0428daf8…4c`) and `s4r8-explorer.toml` (`ab6c33b5…33d7`). The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Octava ronda", and CHANGELOG 1.0.31.

Each finding is either resolved with a test that fails without the fix (AGENTS.md rule 10) or left open with the decision it needs. No code changed in this round: the clients stayed pinned for the live part, and no finding is shown to be a defect in the committed code.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major, Calculator (open; owner decision) | The Standard item has `AutomationId` `Standard` and is one `ListItem` on screen, but its name is not one of the two accepted names (`Estándar`, `Calculadora estándar`), so the client stopped with `unknown_name` before any second click. The name is not recorded, by design, and no element of the window carries either accepted name (`by_name`: zero) | No change. The owner confirms the exact name shown, with the pane open and without saving it. Changing the closed list needs its own round, a test that fails without the change and a new client hash | None yet |
| 2 | Major, window activation (open; cause not verified) | Since the first editor probe, the editor reached the foreground maximized in one of four attempts (10 activation requests). The other three (two pilots and a second probe) failed after 30 requests with no input, and the Explorer probe failed in the same way after 62.9 s. The editor is raised through `main_window(pid)`, which counts an activation request even when it finds no window. Candidate causes: the process's main window is not the one the editor shows; the Windows foreground lock; maximizing on the primary monitor. The monitor layout was normal | No change. A supervised probe that logs, without input, the process's top-level windows (class, visibility, area, maximized state) and the foreground during activation would settle it; a client change would follow, with a test | None yet |
| 3 | Major, E2E-02 (open; cause not verified) | In the probe that did reach the foreground, the engine reported no accessibility-sourced label (0 of 69 elements). The editor's field "Editor de texto" had no candidate, so the first step would stop before any input, and "Guardar" was confirmed only by OCR | No change. A read-only comparison, with the editor in the foreground, between its UI Automation tree and the engine's labels would settle it | None yet |
| 4 | Minor, client usage (no defect shown) | A relative `--exe` (`target/release/ommatix.exe`) did not start on Windows (`WinError 2`). Nothing started and no request was spent | No client change. Pass absolute paths in `--exe`, `--config`, `--results` and `--ledger`. Recorded in the S4 section | None: the client is pinned |
| 5 | Observation | The windows that did not reach the foreground had between 60 s and 257 s of owner idle time; the one that did had 49 s. Idle time does not explain the pattern | No change | None |
| 6 | Interpretation (owner decision) | The round's order says a pilot that fails on grounding or safety means no series for that flow. This round applied it to any pilot that does not pass, because a series that fails three runs in a row creates `stop.flag`, which also halts the Explorer | The owner confirms the reading. A rule of the protocol, not code | None |

## Verification

Before the live part, from `main` at `b7a5e7b` on `docs/s4-eighth-round`, with no code change:

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: OK, binary SHA-256 `3a451211…cbb1a`. Rebuilt: the seventh round's `11a3d792…` predates PR #19.
- `cargo build --release` in `tools/s4/testeditor`: OK, `ommatix-testeditor.exe` SHA-256 `a6765dca…119d`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 383 passed, 0 failed, 1 ignored.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 193 passed.
- From `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 of 48; `validar_paquete.py`, 218 checks, 0 failed. Four result files changed only in timestamps, platform and Python strings, a duration and line endings, and were restored with `git restore`.

## Live part: counts and desktop

- Ledger 426 → 427: one real request (the navigation click of `--calc-standard`). The round has used 1 of 200.
- Delivered: 1 (`effect_observed`, verified by the engine). Rejected, indeterminate, false successes, `grounding_wrong` and `POLICY_DENIED`: 0.
- Scored runs: 0 (S4R-02: 0; E2E-02: 0; E2E-03: 0). No pilot passed.
- `stop.flag`: never created, absent at the end. `window.pin`: absent at the end.
- No test process left running. The owner's two Notepad processes (8960 and 23164) unchanged. The `ommatix-e2e` temporary folder absent at the end.
- No screen capture and no screen text saved. The logs hold statuses, codes, counts, timings, control types, automation identifiers, element identifiers and the protocol's fixed labels.

## Not in this round

- S4R-02 pilot and series: not run, because Standard mode was not reached.
- E2E-02 pilot and series: not run, because the editor never reached the foreground in the pilots.
- E2E-03 pilot and series: not run, because the probe never reached the foreground, so no navigation target was observed.
- No client change, no matrix change and no regression test: the findings are recorded with their decisions.

## Resolution of findings 1 to 3 (branch `fix/s4-round8-blockers`, CHANGELOG 1.0.32)

No live input was sent. The fixes are in the clients (`tools/s4/e2e_driver.py`, SHA-256 `cbe6905c2f582955ddd626a167716e2062d39b294b3033ff706c09c5e4821265`; `tools/s4/e2e_dossier.py`, SHA-256 `c1ba4a376a5df88bb696c07945a1b0fcd6da1ac0365d81f19348304d9061020e`) and in the editor (`tools/s4/testeditor`, release build SHA-256 `0dcfb86b7bee3e5530ffab46c26f37e21c3d454321d93c7eb391f0e4c33d4e78`). No Rust crate of the workspace changed. The causes below say what was verified and what was not; the read-only checks were made on the reviewer's own `ommatix-testeditor.exe` (launched and closed, no input sent) and never on an application of the owner.

| # | Cause | Fix | Regression tests (`tools/s4`) |
|---|---|---|---|
| 1 | **Verified.** The closed list `CALC_STANDARD_NAMES` was a guess about a name nobody had seen. The item is already identified by `AutomationId` `Standard`, a control type of a closed list, exactly one on screen and a `RuntimeId` | Coordinator decision: the name list is dropped. The name is read into memory only (NFC), to select the engine's element by exact label (unique and confirmed); the click must reach the same `RuntimeId` and is grounded as before, with the island descent. The name is in no record, log or stdout; `standard_item` no longer has `name` or `name_accepted`, and `unknown_name` no longer exists | `test_the_item_is_found_by_its_automation_id_whatever_its_name_is`, `test_the_item_name_is_used_in_memory_only_and_never_written_or_logged`, `test_the_name_list_is_gone`, `test_an_item_with_no_runtime_id_means_no_click`, `test_an_engine_element_that_is_not_unique_means_no_click`, `test_the_click_must_reach_the_item_that_was_polled`, `test_two_items_with_the_automation_id_mean_no_click`, `test_exactly_one_on_screen_item_is_ok_whatever_its_name_is`, `test_the_record_never_holds_a_name` |
| 2 | **Foreground lock (owner's observation: the window opens, its taskbar button flashes, the window stays behind).** Reproduced twice on the reviewer's own editor: the window was visible, the foreground stayed with another application and the client's single method (`AttachThreadInput` plus `SetForegroundWindow`) returned false. **Second defect, verified:** `place_on_primary(pid)` restored and maximized *every* visible top-level window of the process, including winit's `Winit Thread Event Target`, which is visible (16 x 16 at the origin); afterwards it was zoomed at 1920 x 1080 and listed first. `SW_MAXIMIZE` activates, so that helper could be the foreground window, and `foreground()` then reported "maximized on the primary monitor" for it (`IsZoomed`), which passes for the application. That this explains the one success in ten activation requests is **not verified**. **Refuted:** `main_window(pid)` returning a window the editor does not show (it returned the `Window Class` window every time; the helper is excluded by class). The Explorer was already activated by its recorded handle; its failure is the same lock, **not reproduced** here (no Explorer window was opened) | `activate_window(hwnd)`: a ladder of three methods (`attach_foreground`, `attach_show_top`, `attach_all_topmost`) that only attach input queues, show, raise and flip the z-order, each verified with `GetForegroundWindow`; the method that worked is recorded (`activation_method`, `activation_tried`). No key, no click, no taskbar (`Shell_TrayWnd`), no `SystemParametersInfo`, no change of user settings. Helper windows are never placed or accepted as the foreground (`is_helper_class`, also in `require_foreground`); the steps read the main window, not the foreground window. A failed wait records `activation_diagnostic`: class, visible, iconic, zoomed and primary-monitor flag of the process's top-level windows (or of the recorded handle), and the foreground window's class and process image; never a title. `--operator-activation` (off by default, in both clients, with `--operator-activation-wait`, 120 s): the client prints to stderr that the operator must click the flashing taskbar button, waits, bounded, until `GetForegroundWindow` equals the recorded handle, records `operator_activation`, and sends nothing before. The editor asks for focus (for 2 s) and maximizes itself once at start, like the test application | `ActivationLadderTest` (6), `WaitForegroundTest` (7), `OperatorActivationTest` (6), `HelperWindowTest` (5), `ExplorerActivationTest` (5), and in the editor `the_editor_asks_for_the_front_early_and_only_for_a_short_time` |
| 3 | **Refuted, with a read-only UI Automation dump of the reviewer's own editor, taken at the first contact (about 350 ms after launch) and with the editor not in the foreground:** the window holds `Text` «Editor», `Edit` «Editor de texto» (Value pattern available and not read-only), `Button` «Guardar» and `Text` «Estado: pendiente». So the multi-line field is exposed as `Edit` with a writable Value pattern, its name is set on the node the adapter reads, the tree is built at the first read and nothing is truncated: **no defect of the editor's tree or of the adapter's role map**. The engine attests the application through the element UI Automation reports with keyboard focus (`uia.rs`); zero accessibility elements in the probe therefore means that element was not in the editor at that read. **Not verified:** why. It needs the editor to be the active window while the engine reads, which the foreground lock prevented; the probe that reached the foreground had a maximized helper window as the candidate foreground window (finding 2) | The same fix as finding 2 (the helper is never the foreground, the editor asks for focus itself) plus a guard: the editor flow re-reads until the engine reports accessibility-sourced elements (6 reads), and without them it stops before any input with `no accessibility evidence from the editor`, `accessibility_elements: 0` and `uia_focus` (process, class, control type and top-level class of the focused element, and whether it belongs to the editor; read-only, no names or values) | `EditorAccessibilityTest` (6) |

The adapter was not changed: no defect of it was found, and a regression test needs one.

### Finding 6, decided

The owner's decision, passed on by the coordinator: **a pilot run that does not pass, for any cause, blocks its series.** The reading applied in the eighth round stands, and nothing changes in the clients.

### Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, the same with `--features dxgi,sendinput,directml,winrt-ocr,uia`, and in `tools/s4/testeditor` `cargo clippy -- -D warnings`: OK.
- `cargo test --workspace`: 383 passed, 0 failed, 1 ignored (unchanged). `tools/s4/testeditor`: 16 passed (15 and the new one).
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 230 passed (193 before). With the clients of `main`, 39 of the new or changed tests fail (8 failures and 31 errors).
- From `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 of 48; `validar_paquete.py`, 218 checks, 0 failed.

### What the next live round must show

See [`S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), «Preparación de la novena ronda».
