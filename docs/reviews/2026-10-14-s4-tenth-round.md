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

## Continuation (2026-10-14)

The continuation (branch `docs/s4-tenth-round-continuation`, CHANGELOG 1.0.40) ran E2E-02's probe and pilot, the ACTIVE CONTROL startup check for the Explorer configuration, and E2E-03's probe and pilot. It sent 4 live requests (ledger 490 → 494; cap 200 from `--round-start 429`). The E2E-02 pilot did not pass its criterion (no `STALE_SNAPSHOT`), so its series did not run. The E2E-03 pilot's first click was reported as a wrong-control grounding, which stopped all live work under the owner's rule. The clients, the engine and the editor were rebuilt or pinned as recorded in the evidence; no code changed. The evidence, in Spanish, is [`S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section «Décima ronda (continuación)».

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 7 | Major, E2E-02 (open; owner decision) | The pilot's criterion is no `STALE_SNAPSHOT`. The first `type` was refused before any key was delivered (freshness stage, 9 ms). The retry was delivered and verified, and so were the save click and both OS-side checks. The series is blocked and the pilot was not repeated. The cause is not verified: the refusal came before the field had keyboard focus, so the caret blink of finding 1 (fixed in the editor) does not explain it. Unchecked candidates: a frame change when the window is activated (`attach_show_top`), or the first repaint | No change. The owner decides whether this counts as passed and whether the pilot is repeated, in a round of its own | None: no code change |
| 8 | Safety, E2E-03 (open; owner decision) | The first click, on the `carpeta-a` breadcrumb (a `SplitButton`), was delivered, and the OS-side folder check passed. The grounding check after delivery found a `Text` child of the breadcrumb under the cursor (island descent, depth 4, not truncated), with a RuntimeId different from the `SplitButton` identified before the click. The client counted `grounding_wrong` 1 and `engine_false_success` 1, and the owner's rule stopped all live work. The identity rule accepts no descendant of the identified control | No change this round (the clients are pinned). Options for the owner, each a client change with a test that fails without it, a new hash and a new round: (a) accept as grounded a descendant of the identified control; (b) keep the rule and use another target for the navigation; (c) read the click as correct and change the rule for this flow only | None: no code change |
| 9 | Observation, Explorer desktop (open; owner) | Two minimized, off-screen `CabinetWClass` windows of the shell process (PID 36160, 160 × 28 at -32000, -32000) were visible at the start of the continuation; the tenth round's report recorded none. The client takes only windows that did not exist before its own launch, and the engine delivers only to the registered window, so they took no part. They were not touched | None. The owner confirms whose they are. The preflight item was accepted for E2E-02 and E2E-03 on that reasoning | None |
| 10 | Observation, owner's Notepad (closed) | The Notepad state matched the tenth round's: 23164 with two minimized windows, 8960 with none | No change | None |
| 11 | Closed: ACTIVE CONTROL for the Explorer configuration | Checked without input at the engine's start: the line names `explorer.exe`, the class `CabinetWClass` and the pin file, and gives no scope warning. This closes item 7 of the tenth round | None | The unit test `the_active_control_indicator_names_the_window_scope` covers the text |
| 12 | Documentation (open) | §3 («El flujo») of the section «Preparación de E2E-02 y E2E-03 del dossier» describes three clicks: `carpeta-a` and `carpeta-b` in the navigation pane, and the root in the path. The pinned client clicks the `carpeta-a` and root breadcrumbs and then «Atrás», starting on `carpeta-b` | Open. The pinned client is the executed contract; the text needs a documentation change after the owner's decision on finding 8 | None |

## After the continuation

- The owner's decisions on findings 7, 8 and 9, and whether the cap continues at 135 with `--round-start 429` or a new round starts at 494.
- For any live run: rebuild and hash the engine and the editor, verify the pinned clients and configurations again, and rerun the read-only preflight. The ACTIVE CONTROL line for the Explorer configuration is already checked.

## Scored series (2026-10-14)

The scored series of E2E-02 (branch `docs/s4-tenth-round-series`, CHANGELOG 1.0.41) ran runs 2 to 11 in the test editor. The engine and the editor were the continuation's builds (SHA-256 `a137ccd1…d548` and `f358b3a9…1d20`; `main` changed only documentation since `0cb126f`). Each run had its own invocation, preceded by the read-only preflight and by a gate that stops on a safety event and pauses on a failed run. Result: 10 of 10 runs succeeded, 20 of 20 OS-side checks passed, 0 engine false successes, 0 wrong-control groundings, and one `STALE_SNAPSHOT` before any input (run 2). 21 live requests (ledger 494 → 515; `--round-start 429`, 114 left). E2E-03 was not run: no Explorer invocation and no probe.

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 13 | Safety, client protocol (open; owner decision) | The pinned client does not stop a protocol on a safety event inside an invocation. A `grounding_wrong` or an `engine_false_success` marks that run failed; `run()` then starts the next run, and only three consecutive failed runs make the client create `stop.flag`. A single invocation of ten runs would have kept sending input after such an event, and an invocation in progress cannot be stopped without killing it. | Not changed: the client is pinned for the round. The series ran as ten single-run invocations, each preceded by the read-only preflight and by a gate (outside the repository) that stops on a safety event, a policy refusal or an abort, and pauses on any failed run. No such event occurred. Proposed: the protocol ends after any safety event. That is a client change, with its own test, a new hash and a new round | None yet: a test that a `grounding_wrong` run ends the protocol needs the change |
| 14 | Observation, E2E-02 (closed by the owner's decision) | The first `type` of an invocation was refused with `STALE_SNAPSHOT` before any key was sent, in two of the eleven invocations (the continuation's pilot and run 2 of the series). The retry was delivered and verified both times, and the refusal did not recur in runs 3 to 11. The owner decided that a refusal before input is allowed and is not a failed pilot, which closes finding 7 for E2E-02. The cause is not verified; the caret blink of finding 1 does not explain it, because the refusal came before the field had focus | None: the refusal is the engine's correct behaviour | None |

## Not in this round

- E2E-03: not run. Finding 8 stays open and the owner's stop holds until the new grounding rule is implemented.
- Finding 9 (the two minimized `CabinetWClass` windows of the shell, PID 36160) and finding 12 (the documented Explorer flow): open, unchanged.
- No client, engine, editor or Rust change.

## What the next round needs

- The owner's approval of `E2E-02` at `EJECUTADO_SUPERVISADO_PARCIAL` in the matrix (this PR), or another status.
- Finding 13: whether the client changes so that a protocol ends after any safety event (a client change, its test, a new hash and a new round), or every series keeps one invocation per run with the gate.
- E2E-03: the new grounding rule, then a round of its own.
- The cap: 114 requests remain under `--round-start 429`, or a new round can start at 515.

## Fix: the grounding rule for clicks that rebuild their own control (2026-10-14, ADR-0022)

Branch `fix/s4-navigating-click-grounding`, CHANGELOG 1.0.42. The owner decided finding 8 on 2026-10-14 («anclar antes + verificar SO») and accepted [ADR-0022](../adr/0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md). The two clients changed (`tools/s4/e2e_driver.py` `a758d172…0f8ce`, `tools/s4/e2e_dossier.py` `359f6f28…0afc2`); no Rust, engine or editor change; no input and no live request (ledger 494).

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 13 | Safety, E2E-03 (resolved in the clients; not confirmed live) | Finding 8: a click that rebuilds the control it clicked (the breadcrumb) was delivered and passed the OS-side folder check, yet its post-click identity check counted a wrong control and a false success, and stopped all live work. The identity of the clicked control is not a reliable witness after a click that navigates | (1) Before every request, in every flow, the element at the target point must be the identified one or have it as parent, in the expected top-level window, or nothing is sent (`pre_grounded`). (2) For the closed list `NAVIGATING_STEPS = {"explorer": (1, 2, 3)}` the identity after the click is recorded (`post_grounded`) and not required; the step's OS-side check is required and decides; if it fails: `grounding_wrong`, `engine_false_success` (when `effect_observed`) and stop. (3) Every other step keeps the identity check after the click; no name comparison, no retry after delivery | `a_navigating_click_whose_control_was_recreated_is_grounded_by_the_os_check`, `a_navigating_click_whose_os_check_fails_is_grounding_wrong_and_stops`, `a_step_that_is_not_on_the_list_still_needs_its_identity_after_the_click`, `a_pre_click_mismatch_sends_no_request`, `a_pre_click_window_that_is_not_the_foreground_one_sends_no_request`, `the_list_of_navigating_steps_is_closed_and_not_a_command_line_option` (`test_e2e_dossier.py`); `PreClickGroundingTest` and `NavigatingStepTest` (`test_e2e_driver.py`) |
| 14 | Observation, cause of finding 8 (open; settled by the next live round) | The records show a `Text` at depth 4 of the island whose chain did not hold the identified RuntimeId, but not why: the client did not record the chain. Hypothesis (owner): navigating rebuilds the breadcrumb, so the control got another RuntimeId. An alternative fits the same records: the control is unchanged and the `Text` is a deeper descendant than the parent that `identity_hit` reads. **Unverified**; the same class of stop happened to the Calculator «Estándar» click in the ninth round | The chain (depth, control type, AutomationId, class, whether it is the identified element; no names, no RuntimeIds) is now recorded before and after every click, so the next round says which cause holds. Not decided here: widening `identity_hit` to a deeper descendant, and adding the Calculator «Estándar» click to the list (its OS-side check would be the mode read through UI Automation) | `the_point_is_read_and_recorded_before_the_request`, `the_element_at_the_point_may_be_a_child_of_the_identified_one_no_deeper`, `the_records_hold_no_name_and_no_runtime_id` (`test_e2e_driver.py`); `the_records_of_the_rule_hold_no_name_and_no_runtime_id` (`test_e2e_dossier.py`) |

Without the change, 33 of the new or adapted tests fail or error (checked by running them against the original clients). The island tests were adapted because every request now has a read before it; their rule is unchanged.

Open for the owner: if the first E2E-03 click is refused with `identity_mismatch` because the `Text` is a grandchild of the identified control, whether to widen the identity rule (a new decision and ADR); whether the Calculator «Estándar» click joins the list; and whether the cap continues at 135 under `--round-start 429` or a new round starts at 494.
