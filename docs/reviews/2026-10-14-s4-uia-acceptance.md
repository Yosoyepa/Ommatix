# S4 second round with UI Automation evidence: findings (2026-10-14)

Findings from the second round of the reduced S4 protocol, with `[models] accessibility = "uia"`
(ADR-0017), on the reference machine with real capture and real input in the three applications
the owner authorised on 2026-10-14. The evidence, in Spanish, is
[`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section
"Segunda ronda con accesibilidad (ADR-0017)". Each fixed finding would ship with a test that fails
without its fix (AGENTS.md rule 10). None of these findings is fixed in code in this round: each
one needs a design decision (an ADR and a dossier change) that the owner has not yet taken.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (open) | `type` drops the keys that arrive in the same frame as its focusing click. Three of four typings left the field without the text while the engine reported `input_delivered: true`. The likely cause, from the egui 0.36.2 source (`widgets/text_edit/builder.rs`), is that a `TextEdit` handles text events before it grants focus in that frame (`handle_events` runs before `request_focus`). This was read from the source and not verified frame by frame. The SendInput actuator checks keyboard focus at window level (`focus_holds_target`: the focused window contains the target point), which a single-window application always satisfies, so it cannot see the drop | Not fixed here. A real fix verifies focus on the target field (for example UI Automation `HasKeyboardFocus` on the element under the point). That changes the send gate of ADR-0007 and ADR-0016 and needs an ADR and a dossier change. Interim measure, not yet run: `tools/s4/e2e_driver.py --focus-click`, a click on the field before typing | — |
| 2 | Major (open) | `type` with `expected_effect: visual_change` returns `effect_observed` when only the focusing click changed the screen. In the pilot this was a false success under the protocol's definition (the title did not contain the typed text). The engine verifies only visual changes (chapter 07 §7.5) | The scored series asks `value_updated` for typing. The engine cannot verify that and answers `indeterminate` without a retry, so the OS-side title decides. The contract should say which postcondition `type` may use, or the engine should refuse `visual_change` for `type`. Needs a dossier change | — |
| 3 | Minor (open) | In 1 of 3 scored runs the initial title of a freshly launched test application did not match, with the application in the foreground and in its own process. A related failure, with another window in the foreground, occurred in run 5 of the first round. The cause was not identified, because the client did not record the title | Diagnostics added: `initial_title_mismatch` records the title's length and whether its prefix matches, never its text. Not yet observed in a live run | — |
| 4 | Process | Three consecutive failed scored runs engaged the stop file, as the protocol's rule requires. E2E-02 (Calculator) and E2E-03 (Notepad) were not run | The stop file is left for the operator to review and remove. No live input was sent after the stop | — |

## Notes

- Live requests: 36 (9 in the pilot, 9 in the diagnostic run, 18 in the scored series). All 36
  were delivered; the control under the cursor (UI Automation) was the requested one in 36 of
  36; no rejections, no `STALE_SNAPSHOT`, no `POLICY_DENIED`. The request ledger went from 124
  (the first round's count) to 160 of the 400 allowed.
- Scored E2E-01: 0 of 3 runs succeeded. The eight clicks of each run were `effect_observed`.
  The pilot's `visual_change` typing was the only engine false success (finding 2), and it is
  excluded from the scored runs.
- Latencies (41 observations with live input): `total` median 318.8 ms, p95 495.6 ms;
  `accessibility` median 71.1 ms, p95 111.4 ms. RNF-05 is not met.
- The execute_action response carries no `_meta` timings, so the action latencies are client
  round trips only.
- Driver changes: flows by UI Automation name, typing, per-stage `_meta` timings for every
  observation and action, an NFC comparison of names, and count-only diagnostics for the
  end-state checks.
- Test application: the text field's accessible name is set on its AccessKit node, not drawn,
  so OCR reads no second copy of it. Its contents are mirrored in the window title.

## Verification

The repository's Rust code is unchanged in this round. The checks run on the branch:

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: 319 passed, 0 failed.
- From `docs/thesis`: `python referencia_ejecutable/run_tests.py`: 48 of 48 oracle tests pass.
  `python validar_paquete.py`: 203 checks, 0 failed. The oracle run rewrote three result files,
  and only their timestamps, Python and platform strings and a duration changed, so they were
  restored with `git restore`.

The test application is a standalone crate, checked on its own with `cargo fmt --check`,
`cargo clippy --release --all-targets -- -D warnings` and `cargo test --release` (6 passed). The
driver was checked with `ast.parse`, pyflakes, and by loading the module to list its flows. None
of that sent input.
