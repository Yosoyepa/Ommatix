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
