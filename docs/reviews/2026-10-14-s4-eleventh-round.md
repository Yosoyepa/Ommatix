# S4 eleventh round: findings (2026-10-14)

Findings from the eleventh round of the supervised S4 runs on the reference machine. The round was to run only E2E-03 (File Explorer, ADR-0020 and ADR-0022): a probe without input, then a pilot and ten scored runs if the pilot passed. It stopped before the probe. Two read-only checks, a few minutes apart, found that the two minimized, off-screen `CabinetWClass` windows of the shell (PID 36160), which the owner confirmed as their baseline, are gone, and that the PC received keyboard or mouse input between them. A probe opens and maximizes an Explorer window, so the round did not run while someone may be using the PC. No input was sent and no live request was spent: the ledger reads 515, as it did at the start.

The evidence, in Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section «Undécima ronda», and CHANGELOG 1.0.43. No code, client, engine, editor, configuration or matrix changed.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Observation, owner's windows and PC use (open; owner decision) | First check (ledger 515): one visible `CabinetWClass` window where the accepted baseline has two, the minimized window of shell PID 36160 (160 × 28 at −32000, −32000); the second minimized window was already gone. Second check, minutes later: the minimized window is gone too, so no visible `CabinetWClass` window remains. The hidden baseline window (1920 × 1032 at 0, 0) is present in both checks. The time since the last keyboard or mouse input went from 239.8 s to 116.5 s, so the PC received input between the checks; no command of this round sends input (the tests use doubles and the clients do not call `SendInput`), and the source is not verified. Unchanged in both checks: shell PID 36160, Notepad PID 23164 (two windows), Notepad PID 8960 (no window) and ApplicationFrameHost PID 10076. The cause of the disappearance is not verified. The clients close only the window they opened, by its handle, and pick only windows that did not exist before their own launch | None. The probe was not run: the owner's baseline no longer exists, and the PC appears to be in use. The owner decides whether the PC is free and whether the current state is the new baseline | None: no code changed |
| 2 | Documentation (resolved) | `docs/handoff.md` listed the ADR-0022 row and its paragraph twice. One copy cited an earlier driver hash (`a758d172…00f8ce`) and the counts of an intermediate state (286 tests, 33 failing without the change). The stale copy is removed; the current copy (`768e77e3…7202d`, 297 tests, 41 failing without the change) stays | Stale copy removed | None: documentation only |

## Verification before the live part

From `docs/s4-eleventh-round` at `32e1644`, with no code change, `e2e11_verify.sh` (outside the repository) ran every check below, and each exited 0:

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`.
- `cargo test --workspace`: 447 passed, 0 failed, 1 ignored.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 297 tests OK.
- Editor (`tools/s4/testeditor`): `cargo clippy -- -D warnings`; `cargo test`: 24 passed.
- `docs/thesis`: `referencia_ejecutable/run_tests.py`: 48 tests run; `validar_paquete.py`: 451 checks, 0 failed.

After the documentation edits, the same oracle, validator and client tests passed again, and `cargo test -p ommatix-acceptance` exited 0 (37 of 37 scenarios; 15 and 6 tests). The oracle and validation result files changed only in timestamps, Python and platform strings and a duration; they were restored with `git restore`.

Hashes checked by the read-only preflight, before the probe: `tools/s4/e2e_driver.py` `768e77e3…7202d`; `tools/s4/e2e_dossier.py` `359f6f28…0afc2`; `target/release/ommatix.exe` `a137ccd1…d548` (not rebuilt: no `src` change since `0cb126f`); `ommatix-testeditor.exe` `f358b3a9…1d20`; `s4r10-live.toml` `0428daf8…c1e2`; `s4r10-explorer.toml` `ab6c33b5…33d7`; the manifest, ONNX Runtime and DirectML pins unchanged. The preflight also found `stop.flag` and `window.pin` absent, no test process running, the client's temp tree empty and Notepad's baseline held. Its only failure was the window count in finding 1 at the first check; the second check passed with no problems.

## Not in this round

- E2E-03 probe, pilot and ten scored runs: not run (finding 1). E2E-03 stays `PENDIENTE_PRODUCTO`.
- No code, client, engine, editor, configuration or matrix change.

## What the next round needs

- The owner's answer to finding 1: whether the PC is free, whether the two minimized windows were closed by the owner, and whether the current state (no visible `CabinetWClass` window; the hidden baseline kept) is the new baseline. If so, the preflight passes as it is, with no exception, and the round runs in the order of its prompt: the probe without input; a pilot with `--max-live 8`; then ten scored runs, one invocation each with the gate between them, from a series budget of 60. The ledger has 114 requests left under `--round-start 429`.
- If not, an investigation of what closed the windows and what sent the input, before any input is sent.

## Continuation (2026-10-15): the owner's baseline, then input before the probe

The continuation (run 2 of the round, branch `docs/s4-eleventh-round-run`, CHANGELOG 1.0.44) took the owner's answers of 2026-10-15: the owner closed the two minimized Explorer windows, has stopped using the PC, and set the new baseline to no visible `CabinetWClass` window, with the hidden shell window (PID 36160) kept and never touched. The preflight and the gate were updated to that state, and the pre-live checks passed. The round still stopped before its probe, because the idle counter showed keyboard or mouse input the round did not cause. No probe, pilot or scored run was started, and no live request was spent: the ledger still reads 515.

### Findings

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 3 | Observation, owner's PC use (open; owner decision) | The read-only preflight at 00:24 local met every condition of the new baseline: ledger 515; the pinned hashes; no visible `CabinetWClass` window; the hidden window of PID 36160 only; Notepad PID 23164 with two windows; no test process; the client's temp tree empty; `stop.flag` and `window.pin` absent. Its idle counter read 77.7 s, so keyboard or mouse input arrived at about 00:23 local: after the owner's answers and after the pre-live checks, which ended at 00:22 with no input. No command of this round sends input. A second read-only check at 00:25 read 128.7 s, which is the 77.7 s plus the 51 s that passed, so no input came between the two reads. The owner's rule (input the round did not cause: stop all live work and report) applied | Not run: the probe, the pilot and the series. The owner decides whether the PC was in use at about 00:23 local and whether the round resumes under a new authorization; its state then has to pass the same preflight and gate, from ledger 515 | None: no code changed |
| 4 | Observation, scope of the input check (open; owner decision) | The idle counter also counts the clients' own input, so during an invocation it cannot tell an outside keystroke from the client's. The gate (`e2e11b_gate.py`, outside the repository) checks the intervals between invocations: the counter must have grown by the time that passed. Within an invocation the in-run checks decide: the foreground window, the point grounding before each click, the folder check after each navigating click, and the stop on any safety event. Watching the counter continuously would be a new decision | None: no code changed | None |

### Verification and read-only checks

- Pre-live checks, from `docs/s4-eleventh-round-run` at `c91862f`, with no code change; each exited 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; the same with `dxgi,sendinput,directml,winrt-ocr,uia`; `cargo test --workspace`: 447 passed, 0 failed, 1 ignored; `python -m unittest discover -s tools/s4 -p "test_*.py"`: 297 OK; the editor's clippy and `cargo test`: 24 passed; `referencia_ejecutable/run_tests.py`: 48 of 48; `validar_paquete.py`: 451 checks, 0 failed. The oracle and validator result files changed only in timestamps, Python and platform strings and a duration, and were restored with `git restore`.
- Hashes: `tools/s4/e2e_driver.py` `768e77e3…7202d`; `tools/s4/e2e_dossier.py` `359f6f28…0afc2`; `target/release/ommatix.exe` `a137ccd1…d548` (not rebuilt: no change under `src` or `tools/s4/testeditor` since `0cb126f`); `ommatix-testeditor.exe` `f358b3a9…1d20`; `s4r10-live.toml` `0428daf8…c1e2`; `s4r10-explorer.toml` `ab6c33b5…33d7`; the manifest, ONNX Runtime and DirectML pins unchanged.
- Read-only preflights at 00:24 and 00:25 local: no problems in either. The second one shows the same state, and its idle counter is explained by the elapsed time.

### What the next round needs

- The owner's answer to finding 3: whether the PC was in use at about 00:23 local, and whether it is free now. If the round continues, it needs a new authorization. It starts from the same preflight and gate at ledger 515 with `--round-start 429`, and runs in the order of its prompt: the probe without input; the pilot with `--max-live 8`, only if the probe gate holds; then ten scored runs, one invocation each, with the gate between them and a series budget of 60.
- The decision on finding 4: whether a continuous idle monitor is wanted.
