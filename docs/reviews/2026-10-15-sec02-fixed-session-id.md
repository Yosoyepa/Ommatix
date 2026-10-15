# SEC-02 across a restart: the fixed session id (2026-10-15)

Found while closing SEC-02 on doubles (PR #53, CHANGELOG 1.0.64); decided by the coordinator
(option B, CHANGELOG 1.0.67). The dossier's account is chapter 08 §8.4. Each fixed finding
ships with a test that fails without its fix (AGENTS.md rule 10).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major | A fixed `--session-id` is not unique across runs. A restarted engine with the same identifier and the same screen accepted a request built on the earlier engine's identifiers (snapshot `s1`, its target and revision) and delivered the click again: measured in process with a throwaway test (not committed), the replay returned `input_delivered` and a second `deliver` event reached the actuator. The default identifier is unique per process, and the same request is refused with `SESSION_MISMATCH` and no event (measured by `sec_02_a_restarted_engine_with_its_own_session_id_refuses_an_old_request`). | With live input the composition root refuses a fixed `--session-id` at start, before any adapter is built. Dry runs, simulations and `demo` keep it for reproducible output. The S4 clients pass their probe identifiers to probes only: a live run takes none, and the engine derives one per process. | `a_fixed_session_id_is_refused_at_start_with_live_input` (`src/app/src/wiring.rs`); `sec_02_a_fixed_session_id_with_live_input_is_refused_before_serving` (`src/app/tests/serve_process.rs`); `ServeArgvTest` (`tools/s4/test_e2e_driver.py`) and `DossierServeArgvTest` (`tools/s4/test_e2e_dossier.py`) |

## Notes

- **Scope.** The S4 clients are `tools/s4/e2e_driver.py` and `tools/s4/e2e_dossier.py`. Both now build the engine command through one helper (`serve_argv`), which refuses an explicit `--session-id` in a live run and passes the probe identifier (`s4r2`, or `s4d` for the dossier) only to probes. The argparse default of `--session-id` is now empty.
- **Not changed here.** `tools/s3/mcp_driver.py` also passes a fixed `--session-id` (default `s3-supervised`) and adds `--input sendinput` for its `live` command. Under this rule its live runs are refused at start until it is changed; the coordinator decides.
- **SHA-256 of the clients after the change** (LF working copies, equal to the committed content): `tools/s4/e2e_driver.py` `fcfea85de52736dfc0accc7c818d92ad66702fe5c2c6d6d6acfbc0f176355933`; `tools/s4/e2e_dossier.py` `3ed82d34082e67a1528abc706ee9e2a855191f466b9b7300d8bafa8434cd5c73`.
- **Mutants.** The live branch of `serve_argv` forced to keep the identifier fails four Python tests. Removing the composition-root call fails `sec_02_a_fixed_session_id_with_live_input_is_refused_before_serving` (the engine then fails on `sendinput input requires --source dxgi`, not on the rule) and the wiring test.
- **Not verified.** No live run was made. A live run with `--session-id` now stops at start; no live run was started to confirm the refusal on the reference machine.
