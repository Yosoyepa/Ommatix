# S4 fourth round: findings (2026-10-14)

Findings from the fourth round of the reduced S4 protocol, which repeated E2E-01, E2E-02 and
E2E-03 on the reference machine with the fixes of CHANGELOG 1.0.18 (`e9e5b2d`). The evidence,
in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md),
section "Cuarta ronda: correcciones de la ronda 3", and CHANGELOG 1.0.19.

The round changed no code: the committed driver (`tools/s4/e2e_driver.py`, SHA-256
`2af90bf3…a283`) ran unchanged, through a wrapper outside the repository that sets the round's
live-request cap and records action timings (findings 1 and 2). Each finding below is either
resolved with a test that fails without the fix (AGENTS.md rule 10), or it stays open with the
decision it needs.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (open, client) | `LIVE_CAP` in `tools/s4/e2e_driver.py` is 400 and is checked against the whole ledger, not against the round. With the ledger at 291 only 109 live requests were left; the three series and their pilots need about 190. The owner's note for this round says the per-round cap restarts | Worked around without changing the driver: `s4r4_run.py` (outside the repository) sets `LIVE_CAP` to 291 + 400 = 691, so the round gets 400 requests and the ledger continues from 291 (ending at 396). The driver is not changed this round, so that the committed client stays the evidence | Proposed: take the round's base value of the ledger (for example `--round-base 291`) and check `total - base >= 400`. Test: with a ledger at 291 and base 291, 400 requests are allowed and the 401st stops the run. Not written yet |
| 2 | Minor (open, client) | `stage_ms` reads only `io.ommatix/timings`, so the `io.ommatix/action_timings` that the third round's finding 6 fix publishes are dropped: the `stages_ms` of every `execute_action` is empty. The protocol asks to record them | Worked around in the same way: the wrapper adds them to `stages_ms` with the prefix `action_`. 91 of 91 action responses in E2E-01 carried them. Not fixed in the driver this round | Proposed: `stage_ms` also merges `io.ommatix/action_timings`. Test: a response with those timings yields `action_total`, `action_gate` and the other stages. Not written yet |
| 3 | Major (open, protocol) | In Scientific mode the Calculator has no clear control after `Es igual a`. In the pilot, at step 5, none of `Borrar`, `Borrar todo` or `Borrar entrada` was confirmed after four delivered and verified clicks. The third round reported the same | The driver fails closed as designed, and the flow cannot pass in Scientific mode. Owner decision needed: Standard mode, another final key, or another order | `tools/s4/test_e2e_driver.py` (`ClearControlTest`) covers the selection. The live evidence is the pilot record, which is not a test |
| 4 | Major (open, safety) | In the mode-switch attempt, the navigation click on `Abrir navegación` was delivered and reported `effect_observed`, but the grounding check after the click did not confirm the requested control: the control under the cursor was not named `Abrir navegación`. No second click was sent. The likely cause is that the navigation toggle changes its name when the pane opens and that `named_control` compares names after the click. This is not verified, and checking it needs another live click | Not resolved. The Calculator series was aborted, and the Notepad series did not run, because the owner's rules say to stop on a safety issue. Proposal: identify a toggle whose name changes on activation by its `AutomationId` (`TogglePaneButton`), and keep the name check elsewhere. This needs a design decision | Not written yet. It needs a fake accessibility chain in which the toggle's name changes after the click |
| 5 | Minor (open, helper outside the repository) | The mode-switch helper `s4r4_calc_standard.py` stopped with `KeyError: 'engine_false_success'` after one delivered click. Its result dictionary lacked keys that the driver's `steps()` increments. The error came before the second click, and the helper's cleanup closed the Calculator it had started | Not reused in this round. The helper is outside the repository; before any reuse it must initialise the keys that `steps()` increments | Not applicable: the helper is not part of the repository |
| 6 | Minor (open, environment) | Input events without a client action (the first and third rounds' finding 5). None was seen in this round: the idle time at the start of each scored run was 16.8 s for the first run and 2.1 to 2.3 s for the others, which fits the client's own last click. The log cannot see input between runs | Open. The driver keeps recording `user_idle_s_at_start` | none |

## Notes

- **Ledger:** 291 at the start of the round, 396 at the end: 105 live requests in this round
  (pilots 9 and 4, scored E2E-01 91, one request of the mode-switch attempt). 104 were
  delivered and 1 was rejected with `STALE_SNAPSHOT` (run 7, retried successfully).
- **Series:** E2E-01 scored 10 of 10, no false success, no lost key. E2E-02 pilot failed closed
  at step 5 (finding 3); the mode-switch attempt stopped at finding 4; the series was aborted with
  0 scored runs. E2E-03 did not run.
- **Hashes:** `ommatix.exe` `2c07e447…ee2c01`; test application `09ccf30d…c9bf`; configuration
  `ecc7c5f7…2eac5`; `e2e_driver.py` `2af90bf3…a283`; `s4r4_run.py` (wrapper) `9ec27312…9e6a12`;
  `s4r4_calc_probe.py` `f4bb378c…bcd1c5`; `s4r4_calc_standard.py` (helper) `7f148352…6d68e`.
- **Stop file:** removed once at the start, as the owner authorised. The client did not create
  it in this round.

## Verification

- `cargo fmt --all --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 337 passed, 0 failed. No Rust code changed in this round.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 18 passed.
- From `docs/thesis`: `python referencia_ejecutable/run_tests.py`: 48 of 48 OK. `python validar_paquete.py`: 210 checks, 0 failed.
  Both rewrote their files in `referencia_ejecutable/resultados/`. The changes were only the
  timestamp, the Python and platform strings, a duration and the jsonschema version, so those
  files were restored with `git restore`.

## Not in this round

- No code change and no change to the driver: the cap (finding 1) and the timings (finding 2)
  are worked around outside the repository, and the proposals need a test each.
- No regression test was written, because no defect was fixed in code.
- The Calculator decision (finding 3), the grounding design (finding 4) and the E2E-03 run need
  the owner.

## Preparation for the fifth round (2026-10-14)

Only the client changes: `tools/s4/e2e_driver.py` (SHA-256 `98f0ae08…1967e5`) and its tests, now 62
(`python -m unittest discover -s tools/s4 -p "test_*.py"`; 40 of them fail against the
fourth-round driver). Nothing ran live: the tests use a fake desktop and send no input. The
owner decided that the E2E itself switches the Calculator to Standard mode and that round 5
runs E2E-02 and E2E-03. Each defect below is followed by its fix and its test.

| # | Defect | Fix | Regression test |
|---|---|---|---|
| 1 | `LIVE_CAP` counted the whole ledger | `LIVE_CAP` (at most 400) limits the requests of the round: `--round-start` is the ledger's value when the round began (required in run mode, checked to lie within the ledger and to leave room), and a request is refused when `ledger - round_start >= LIVE_CAP`. The ledger only accumulates; each run, the start and the end record `ledger_start..ledger_end` | `RoundCapTest`: a ledger at 291 with the round started at 291 allows 400 requests and stops the 401st; 110 requests pass where the whole-ledger cap would have stopped at 109; `round_problem` rejects a missing, out-of-ledger or exhausted start |
| 2 | `stage_ms` dropped `io.ommatix/action_timings` | `stage_ms` merges them as `action_total`, `action_gate`, `action_freshness`, `action_delivery`, `action_focus` and `action_verification`; `io.ommatix/timings` keeps its names, so the two never collide | `ActionTimingsTest`; `test_the_engine_timings_of_each_action_are_recorded_per_action` |
| 4 | Grounding compared names after the click, so a toggle that renames itself was not confirmed | Identity grounding. Before each click and before the live request is spent, UI Automation (OS side) finds the elements of the foreground window of the target application with the exact name and a control type that the engine's role stands for (the inverse of the accessibility adapter's `role_of`), ignores off-screen twins, and requires exactly one with a RuntimeId or an AutomationId; it records both. After the click the element under the cursor, or its parent, must have the same RuntimeId, or the same AutomationId when a RuntimeId is unavailable; names are never compared. No name-only fallback: `named_control` and the rectangle check are gone. A click that is not grounded ends the run with no further input | `IdentityFunctionsTest`; `GroundingByIdentityTest`: a toggle renamed by its own click is grounded; two candidates, none, an unreadable tree, no identifier or a role with no UI Automation control send nothing and spend no request; another element under the cursor ends the run before the next step |
| 5 | The round-4 helper did not initialise counters that `steps()` increments | The mode switch is a step of the client (`--calc-standard`) and builds its record with `new_result`, which `run_once` uses too | `ResultCountersTest`: all counters start at 0 and `steps()` counts `effect_observed`, `indeterminate` and `rejected` without a missing key |

`--calc-standard` (owner decision): one invocation before the E2E-02 series, through the
engine's gate only, at most 4 live requests. It reads the mode through UI Automation (header
`Header` and the absence of the Scientific-only controls; no click if it is Standard already or
unreadable), clicks the navigation toggle by its exact label (recorded; "Abrir navegación"),
observes again, clicks "Estándar" by its exact label, each click with identity grounding, and
verifies the mode again through UI Automation. It stops without another click when the pane does
not open, when "Estándar" is not unique and confirmed, or when a click is not grounded. It leaves
the Calculator closed and closes only the process it started. Tests: `CalcStandardTest`
(happy path, already Standard, unreadable mode, pane that does not open, "Estándar" duplicated or
not confirmed, ungrounded or ambiguous navigation click, mode not reached, own live budget) and
`CalcModeTest`.

Not verified live: the two mode signals, the control type of "Estándar" and whether the
Calculator keeps its mode when `taskkill` closes it right after the switch. The first two end the
step safely when wrong (no click or no second click); the third shows in the E2E-02 pilot as a
missing clear control, which fails closed. Findings 3 and 6 stay as they were.
