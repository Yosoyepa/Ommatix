# S4 second round with UI Automation evidence: findings (2026-10-14)

Findings from the second round of the reduced S4 protocol, with `[models] accessibility = "uia"`
(ADR-0017), on the reference machine with real capture and real input in the three applications
the owner authorised on 2026-10-14. The evidence, in Spanish, is
[`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), section
"Segunda ronda con accesibilidad (ADR-0017)". Each fixed finding ships with a test that fails
without its fix (AGENTS.md rule 10). None of these findings was fixed in code in the round itself:
findings 1 and 2 needed a design decision, which the owner took the same day
([ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md), CHANGELOG 1.0.16)
and which resolves them, as the table records. The resolution is tested on doubles and has not
been re-run live.

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major (resolved in code, ADR-0018) | `type` drops the keys that arrive in the same frame as its focusing click. Three of four typings left the field without the text while the engine reported `input_delivered: true`. The likely cause, from the egui 0.36.2 source (`widgets/text_edit/builder.rs`), is that a `TextEdit` handles text events before it grants focus in that frame (`handle_events` runs before `request_focus`). This was read from the source and not verified frame by frame. The SendInput actuator checks keyboard focus at window level (`focus_holds_target`: the focused window contains the target point), which a single-window application always satisfies, so it cannot see the drop | **Resolved** by ADR-0018. `type` is delivered in two phases: the focusing click, a poll on the injected clock until UI Automation reports keyboard focus on the target element itself (an enabled, on-screen, non-password input whose box matches the target's by IoU), and only then the keys. With no confirmation in the verification window no key is sent and the result is `indeterminate` (`INDETERMINATE_EFFECT`, `reconcile_effect`) with the click reported delivered; the stop, consent and foreground application are looked at again right before the keys. Without accessibility evidence the window-level check stays, which cannot see this class of loss. The interim `--focus-click` mode of `tools/s4/e2e_driver.py` is superseded and stays off | `type_sends_no_keys_until_the_target_field_has_keyboard_focus`, `type_without_focus_confirmation_is_indeterminate_and_sends_no_keys`, `focus_on_another_element_is_not_accepted`, `a_stop_or_a_foreground_change_after_the_click_sends_no_keys`, `without_accessibility_focus_type_keeps_the_window_level_behaviour` (`src/application/src/engine/tests/typing.rs`); `focus_is_on_the_target_only_for_its_own_enabled_writable_input` and `focus_on_another_element_is_not_focus_on_the_target` (`engine/accessibility.rs`); `the_keys_of_a_two_phase_type_need_only_the_attested_application_to_hold_focus` (`SendInput` adapter); BDD-34 and BDD-35 |
| 2 | Major (resolved in code, ADR-0018) | `type` with `expected_effect: visual_change` returns `effect_observed` when only the focusing click changed the screen. In the pilot this was a false success under the protocol's definition (the title did not contain the typed text). The engine verifies only visual changes (chapter 07 §7.5) | **Resolved** by ADR-0018, which keeps `visual_change` as the postcondition of `type` and measures it against a frame captured after the focusing click and before the keys, never against the pre-click frame, so the click's own redraw is not the effect of typing. The field's value is never read. `effect_observed` after `type` still means "something changed after the keys", not "the text was typed" | `type_visual_change_is_measured_after_the_focusing_click` (fails without the fix: the click's redraw gave `effect_observed`), `type_visual_change_after_the_keys_is_an_observed_effect`, `a_click_redraw_alone_is_not_claimed_when_no_effect_is_expected` |
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

The repository's Rust code was unchanged in the round itself. The checks run on the branch at the
time of the round (the ADR-0018 fix has its own checks, recorded in CHANGELOG 1.0.16 and the pull
request):

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
