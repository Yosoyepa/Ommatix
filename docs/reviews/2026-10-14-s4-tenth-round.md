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
