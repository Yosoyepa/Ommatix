# S4 ninth round: findings (2026-10-14)

Findings from the ninth round of the supervised S4 runs on the reference machine. The round stopped after two live requests. Both were the two clicks of `--calc-standard` in the Calculator: the navigation toggle and the Standard item. The Standard item's click was delivered and the engine reported `effect_observed`, but the identity check after the click did not find the item under the cursor. Under the owner's safety rule (a click grounded on the wrong control, or an engine false success, stops all live work) the round stopped there. Nothing was scored. The Calculator pilot and series, the editor's probe, pilot and series, and the File Explorer's probe, pilot and series were not run.

The round used the committed clients (`tools/s4/e2e_driver.py`, SHA-256 `cbe6905c2f582955ddd626a167716e2062d39b294b3033ff706c09c5e4821265`; `tools/s4/e2e_dossier.py`, SHA-256 `c1ba4a376a5df88bb696c07945a1b0fcd6da1ac0365d81f19348304d9061020e`), checked before the live invocation, the engine binary built from `main` at `9ef66ef` (`3a451211763f8ef9c92adb98955dca7b42dedf58974c97248bc97aac513cbb1a`, unchanged since the eighth round), the editor rebuilt from `main` (`541fc0a8246d451610dacb94c1d2e4001c1bdffa634281f33088ff1cce192793`) and the configurations `s4r9-live.toml` (`0428daf8…5ec1e2`) and `s4r9-explorer.toml` (`ab6c33b5…233d7`). The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Novena ronda", and CHANGELOG 1.0.33.

No code changed in this round. The clients stayed pinned for the live part, and none of the findings below is shown to be a defect that a fix would have to correct before the next run. Findings 2 and 3 are recording gaps in `tools/s4/e2e_driver.py`; their fixes need a new client hash and their own tests, so they wait for the owner's decision on the next round.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major, safety (Calculator; open; owner decision) | The second click (Standard item) was delivered (`input_delivered`), reported `effect_observed`, and the identity check after it did not find the item under the cursor (`grounded: false`). By the client's definitions (`steps()`) this counts as `grounding_wrong` and `engine_false_success`, so the owner's rule stopped all live work. The single mode read right after the click was `other`. **Not verified:** whether the click landed on another control, whether the pane closed before the read, or whether the mode changed after it | No change. The round stops here. The owner decides whether the round continues and how the cause is investigated (a read-only check of the pane after a click, with the owner present, or accepting the risk). The client is not changed to make this case pass | None: no code change |
| 2 | Minor, record (open) | The `--calc-standard` summary records `clicks[].grounded`, but not the `grounding_wrong` and `engine_false_success` counters that the run path keeps. The case is visible only through `grounded: false` | No change this round. Printing the two counters in the summary is a client change: it needs a test that fails without it and a new client hash, in a round of its own | None yet |
| 3 | Minor, record (open) | When `--calc-standard` stops, `mode_after` is read once, right after the failing click, and the step does not wait. Only the success path polls the mode eight times at 0.5 s. So `other` in a stopped step is not conclusive | No change this round. The fix polls the mode with the same bounded wait on the stop path, with a test that fails without it and a new client hash, in a round of its own | None yet |
| 4 | Observation | The Standard item was found on the first read of UI Automation (391 ms; one candidate; `AutomationId` `Standard`; `ListItem`; on screen). Its name was not recorded. Its identity was confirmed before the click (`RuntimeId` `42.5964908.4.119`). The engine gave that element role `unknown` with label source `accessibility`; the navigation toggle has role `button` | No change. Whether the role matters to the grounding is not known | None |
| 5 | Observation | The Calculator was already in the foreground when the step needed it (`activation_method` `already_foreground`; foreground wait 0.87 s; one activation request). The foreground ladder of the eighth round was not needed here, and the editor and File Explorer, which failed to reach the foreground in the eighth round, were not run. The foreground lock is therefore not tested again in this round | Open. Next round: the editor and File Explorer probes, with the same activation diagnostics, once the owner decides the round continues | None |
| 6 | Interpretation (owner decision) | The safety rule was applied to the whole of the live work after the stop, including the read-only probes of E2E-02 and E2E-03 (they send no input but open the windows through the clients). The round did not run them | Pending confirmation by the owner. A protocol reading, not code | None |
| 7 | Observation, gate (outside the live part) | The first pre-push run of this branch failed once in `ommatix-application --lib` (`cargo test --workspace`, run by the hook after the documentation commit). The test name was not captured. It did not reproduce in four full workspace runs, twenty-five runs of the crate's unit tests, or twenty-four parallel runs of its test binary. No code changed; the push was retried and passed | Open. If it recurs, keep the full output of the hook: the test name is what decides whether it is a timing dependency | None: not reproduced |

## Verification

Before the live part, from `main` at `9ef66ef` on `docs/s4-ninth-round`, with no code change:

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: already up to date; binary SHA-256 `3a451211…cbb1a`, the same as in the eighth round.
- `cargo build --release` in `tools/s4/testeditor`: rebuilt from `main`; `ommatix-testeditor.exe` SHA-256 `541fc0a8…2793`. The eighth round's `0dcfb86b…4e78` predates PR #21, which changed the editor.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`, and `cargo clippy -- -D warnings` in `tools/s4/testeditor`: OK, no warnings.
- `cargo test --workspace`: 383 passed, 0 failed, 1 ignored (unchanged). `cargo test` in `tools/s4/testeditor`: 16 passed.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 230 passed.
- From `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 of 48; `validar_paquete.py`, 218 checks, 0 failed. Four result files changed only in the timestamp, the Python and platform strings, a duration and line endings, and were restored with `git restore`.

## Live part: counts and desktop

- Ledger 427 → 429: two real requests, both in `--calc-standard`. The round has used 2 of 200.
- Delivered: 2 (both `effect_observed`). Verified by identity: 1 (the navigation click). Not grounded: 1 (the Standard item's click; finding 1). Rejected, `indeterminate`, `POLICY_DENIED` and `STALE_SNAPSHOT` retries: 0.
- Scored runs: 0. No pilot passed or ran.
- `stop.flag`: never created, absent at the end (the step stopped on its own abort, not on the operator's stop). `window.pin`: never created, absent.
- No test process left running. The Calculator the step started was closed by the step. The owner's two Notepad processes (8960 and 23164) and their two minimized windows are unchanged. No `CabinetWClass` window was opened, so the Explorer's window count did not change.
- No screen capture and no screen text saved. The logs hold statuses, codes, counts, timings, control types, `AutomationId` and the protocol's fixed labels. The Standard item's name appears in none of the round's logs or result files.

## Not in this round

- Calculator pilot and series (S4R-02): not run, because the Standard step stopped on finding 1.
- E2E-02 probe, pilot and series: not run, because the round had stopped.
- E2E-03 probe, pilot and series: not run, because the round had stopped.
- No client change, no matrix change and no regression test: findings 2 and 3 are recorded with the change each one needs.

## What the next round needs

- The owner's decision on finding 1, and on whether the round continues under the same protocol (finding 6).
- If it continues, the cap is the owner's decision: either this round's `--round-start 427` (198 of 200 requests left), or a new round with its own `--round-start` (the ledger is at 429). Either way, the same preflight before each live invocation, then the editor and File Explorer probes, which the eighth round could not complete.
- Findings 2 and 3, if the owner wants them before the next run: one client change with its own tests and a new hash, in a round of its own.

## Continuation: findings 8 to 13

The continuation of this round (branch `docs/s4-ninth-round-continuation`, from `main` at `a5cd1f9`, CHANGELOG 1.0.34) sent **no live request**. The ledger stayed at 429 (2 of 200 used in the round). The clients stayed pinned, with the same hashes as in the first part. Every live invocation stopped before its first action. The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section "Novena ronda (continuación)".

Mode check, read-only: `read_calc_mode` of the pinned client, called from a script outside the repository that starts the Calculator, reads its mode and closes only its own process. It read `other` twice and nothing was clicked. The invocation that the protocol proposed (`--calc-standard --max-live 0`) was not used; see finding 9.

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 8 | Major, S4R-02 (open; owner decision) | Every recorded `--calc-standard` reading of the Calculator's mode is `other` (rounds 5, 7 continuation, 8 and 9), including the reading after the owner switched the mode by hand. The Standard-mode signal (the `Header` name, against the client's two accepted names) has not been read live in Standard mode. This continuation's two readings: header present, name not in the list, zero Scientific-only controls. Which mode the Calculator is in, and whether the list is wrong, are not verified | No change. Proposed: a read-only diagnostic before the next round (number and control types of the `Header` elements, never their names); if the list is wrong, a client change with its own test and a new hash | None yet |
| 9 | Minor, protocol (open) | With `--calc-standard --max-live 0` and a mode other than Standard, the step reaches `Protocol.spend_live` (`steps()` → `require_foreground()` → `identify_before()` → `spend_live()`), which calls `engage_stop`: it creates `stop.flag` before it aborts. Found by reading the code; not run live, and the invocation was not used | No change. Proposed: the calc-standard step attempts no click with a zero budget, or refuses `--max-live 0`; a client change in a round of its own | None yet |
| 10 | Major, E2E-02 (open; cause not verified) | The keyboard focus, as UI Automation reports it, belongs to the desktop (class `#32769`, `Pane`) while the editor is the foreground and maximized window (`is_app` false). Zero accessibility elements in six observations for each of two probes (settle 1.5 s and 5 s) and for the pilot. The activation was `already_foreground`, so no activation method ran. Candidate cause, not verified: the activation ladder does nothing when the window is already in front, so nothing requests keyboard focus for it | No change. Needs a client or editor change with a test that fails without it and a new hash, in a round of its own. No input is possible until the engine sees the field | None yet |
| 11 | Major, E2E-03 (open; owner decision) | Both navigation targets are not found as the flow defines them. The engine reports zone `unknown` for «carpeta-a» and for the root, where the flow requires `left_sidebar` and `top_bar`. UI Automation reports «carpeta-a» as one on-screen `ListItem`, where the flow requires `TreeItem`. «carpeta-b» appears only after the first click, as expected. The root is a `SplitButton`, which is an allowed type | No change; the flow was probed, not run. Options for the owner: accept the observed type and zone (client change, tests, new hash), change the flow's contract (dossier change with a CHANGELOG entry), or find out why the engine assigns no zone | None yet |
| 12 | Observation | Finding 5 of the first part (the foreground ladder was not exercised) is partly covered: the Explorer probe's activation was `attach_show_top` on the first request. The editor was already in the foreground | Partly covered | None |
| 13 | Observation, gate | The `ommatix-application --lib` failure of the first part did not recur: `cargo test --workspace` passed in full in the continuation (383 passed, 0 failed, 1 ignored) | Open, no change | None: not reproduced |

Findings 1 to 7 of the first part stay as they were; finding 5 is covered in part by finding 12.

### Verification before the live part (continuation)

On `docs/s4-ninth-round-continuation` from `main` at `a5cd1f9`, with no code change since `9ef66ef`: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia` (engine SHA-256 `3a451211…cbb1a`, unchanged); `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`; `cargo test --workspace` (383 passed, 0 failed, 1 ignored); the editor's `cargo build --release`, `cargo clippy -- -D warnings` and `cargo test` (16 passed; SHA-256 `541fc0a8…2793`); `python -m unittest discover -s tools/s4 -p "test_*.py"` (230 passed); `referencia_ejecutable/run_tests.py` (48 of 48); `validar_paquete.py` (218 checks, 0 failed).

### What the next round needs

- The owner's decisions on findings 8, 10 and 11, and whether the round continues: 198 requests remain under `--round-start 427`, or a new round can start its own cursor at 429.
- Findings 2, 3 and 9 are client changes, and finding 10 needs a change in the client or the editor. Each needs its own test that fails without the change and a new hash, in a round of its own.
- Only after those: the Calculator's mode check and S4R-02 (finding 8), the editor's probe and pilot (finding 10), and the Explorer's probe and, if its targets are reachable, its pilot (finding 11).
