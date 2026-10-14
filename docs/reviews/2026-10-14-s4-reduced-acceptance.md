# S4 reduced acceptance protocol: findings (2026-10-14)

Findings from the reduced S4 protocol: automated click-only end-to-end runs through
`ommatix serve --source dxgi --input sendinput` in three applications the owner authorised
on 2026-10-14 (our test application `tools/s4/testapp`, `notepad.exe`, the Windows
Calculator), with real capture and real input on the reference machine. The evidence, in
Spanish, is [`docs/thesis/S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md).
Each fixed finding ships with a test that fails without its fix (AGENTS.md rule 10).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Major | The `SendInput` actuator attested the Windows 11 Calculator as `applicationframehost.exe`: a UWP app's foreground window is the frame that `ApplicationFrameHost.exe` draws around it. The only way to let the engine act on the Calculator was to allowlist the frame host, which allows every hosted application at once | In the Windows module of the input adapter (ADR-0016 scope; every new unsafe block wraps one Win32 call with a `// SAFETY:` comment), a top-level window of class `ApplicationFrameWindow` owned by `applicationframehost.exe` stands for the process of its `Windows.UI.Core.CoreWindow` child (`calculatorapp.exe`). A frame without hosted content (suspended or minimised app) attests no application, never the host. A point on the frame's own windows (title bar, caption buttons) belongs to no application and is refused. Windows of that class owned by other processes (`explorer.exe` has one) are not resolved. Integrity, the point check and the focus check of `type` use the resolved process. The decisions are pure functions (`is_frame_host`, `shown_application`, `point_application`) | `a_frame_host_window_attests_the_hosted_application` (fails without the fix: the resolution did not exist and the host was attested), `only_the_application_frame_host_is_resolved`, `a_point_on_the_frame_hosts_own_windows_belongs_to_no_application`. Verified live: 4 clicks delivered to the Calculator with only `calculatorapp.exe` allowlisted |
| 2 | Major (open) | A click landed on another control. In the Calculator, the only usable confirmed label (OCR `Fundón`, the "Función" flyout button) belonged to a detector box centred on the x² button; 4 of 4 pilot clicks pressed x² (UI Automation: `xpower2Button`) and were `effect_observed`. With role `unknown` a label is the OCR words lying at least 75 % inside a box (Algorithm 2, IoC) and the click goes to the box centre: the engine behaves as specified, and `effect_observed` truthfully claims only a visual change (chapter 07 §7.5), so it cannot see the wrong target | Not fixed: any fix changes Algorithm 2 or the action point, so it needs a dossier change. Proposals in the report §6: aim at the label words' centre when the box is much larger than them; downgrade such boxes to `unknown` actionability; or accessibility evidence (finding 4). The S4 driver now checks grounding independently (UI Automation under the cursor) and the Calculator was not scored | — |
| 3 | Minor (open) | Windows OCR does not read single-glyph buttons (the Calculator's digits and operators: 129 of 135 elements had no label), so numeric-keypad interfaces are not addressable by label | Recorded as a limitation (EV-01 coverage) | — |
| 4 | Minor (open, known) | No `type` is possible: the engine allows `type` only on a confirmed `Role::Input`, and the S0 detector (one class) yields `unknown`. The rule was not weakened | Proposal: a UI Automation evidence adapter (v1.1 roadmap, chapter 06 §6.8) giving roles, `label_source: accessibility` and real rectangles, fused with the visual path; it needs its own port, ADR and scenario. The test application already exposes its widgets to UI Automation through AccessKit | — |
| 5 | Minor (open, from S3) | The `SendInput` receipt (`submitted`/`inserted`) is still not visible to the agent | Unchanged | — |

## Notes

- Scored runs: 20 (test application 9 of 10, Notepad 10 of 10). False successes: 0 of 92
  `effect_observed`, checked against the window title (test application) and UI Automation
  grounding (both). Live requests: 124 of the 400 allowed (31 in the excluded pilot);
  118 clicks delivered; 6 `STALE_SNAPSHOT` refusals; no `POLICY_DENIED`, no `type`.
- The driver (`tools/s4/e2e_driver.py`) moves windows of the processes it started to the
  primary monitor and asks for activation (window management, not input). It never starts
  or closes Notepad: it restores one existing window and minimises it again at the end.
- Desktop left as found: the test application and the Calculator closed (only the
  processes the protocol started), the Notepad window minimised with its menu closed.

## Verification

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p
ommatix --features dxgi,sendinput,onnx,winrt-ocr --all-targets -- -D warnings` (and with
`directml`), `cargo test --workspace`, `cargo build --release` and `cargo test --release` in
`tools/s4/testapp`, and the thesis checks pass on `feat/s4-reduced-e2e`.
