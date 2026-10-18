# 0035. Limit the freshness group of a live action to the windows above the target

- **Status:** Accepted
- **Date:** 2026-10-17
- **Approval:** The project owner, 2026-10-17. The owner accepts the amended group knowingly, including the residual limit on legacy blur-behind and accent policy (see Consequences and Implementation notes). The switch keeps its default `screen`, and live use still needs the validation in Required validation. Acceptance changes no test status or checkbox of chapter 06 §6.7.

## Context

ADR-0032, accepted on 2026-10-15, scopes the freshness check of a live action to the **window group** of the target: the top-level window that contains the target, the foreground window, and every top-level window whose rectangle overlaps one of those. The check compares the tiles that intersect that group, exactly and without tolerance. The switch `action_freshness = "target_window"` is off by default, and no live run has used it.

**The measurement (M1, 17 October 2026).** M1 is the no-input probe of ADR-0032. Its procedure and results are in [S4 §D.1 and §D.1.1](../thesis/S4_SESIONES_PENDIENTES.md). It ran with the owner present, on the owner's desktop, with Notepad's menu closed (30 pairs) and open (30 pairs). The machine has two 1920 × 1080 monitors; output 0 was measured, at 100 % scaling. In all 60 pairs the union of the group covered the whole output (`group_covers_whole_frame`). The group had 5 windows with the menu closed and 6 with it open. On this desktop, therefore, the ADR-0032 group compares the whole frame, and `target_window` cannot reduce the compared area. The probe records no window identities, so it does not show which windows made the cover. ADR-0032's implementation notes already say that a background rectangle covering the output can expand the group to the whole screen; M1 confirms the effect, not the cause. The screen did not change during M1, so M1 observed no change at all and says nothing about the refusals of S3 §9.6 and §10.

On 16 October 2026 the owner set M1 as a descriptive measurement, with no pass threshold, and agreed that if M1 showed the group covering the whole output, the architect would propose amending ADR-0032 and the owner would decide. This ADR is that proposal; the owner accepted it on 17 October 2026.

## Decision

Accepted on 2026-10-17: for `action_freshness = "target_window"`, the window group of ADR-0032 (its Decision, point 2) is replaced by the group below. The rest of ADR-0032 keeps its force, as point 7 states.

1. **Group.** The group of a target `T` is:
   - the top-level window `T` that contains the target, as ADR-0032 defines it: the window the OS reports under the target's point at observation;
   - every visible top-level window `W` that lies **above** `T` in the z-order and whose rectangle overlaps `T`'s rectangle, grown by `action_freshness_margin_px`.

   Windows below `T` are excluded, whatever they overlap. The foreground window is not a member for being the foreground window: it is in the group only if it meets the same rule.

2. **Why this set is enough.** `T` is the window the click reaches. While `T` is opaque over the target's box, the pixels that the box shows come from `T` alone, so only a window above `T` can change them, and only where it overlaps `T`. Under ADR-0032 a window below `T` that overlaps it is in the group, so its moves refuse even though its pixels are hidden under `T`. This amendment drops that refusal and keeps the protection, because nothing below an opaque `T` reaches the visible pixels of the box or the point that the click uses. [D] Point 4 states the condition that makes this hold.

3. **Identity.** At the last look the group is read again. Its ordered list (z-order, handle and rectangle) must equal the observed list, as ADR-0032 point 3 requires, and any difference refuses with `STALE_SNAPSHOT`. A window that rises above `T` and overlaps it between the two looks enters the list and refuses. So does a window above `T` that moves into an overlap. Leaving the group, a change of z-order inside it, and a move or resize of `T` refuse too. Windows below `T` that are not in the group are not in the list, so their moves do not refuse. The observed foreground HWND remains a separate identity field, even when that window is outside the comparison group. Every foreground HWND change refuses with `STALE_SNAPSHOT`, independently of ADR-0020 configuration; an application change keeps its existing policy refusal at the gate and the last look.

4. **Opacity, fail closed.** The reasoning of point 2 holds only while `T` is opaque over the target's box. The Windows adapter must attest three things: `T` is neither layered nor click-through in the box; `T` has no translucent content there, and no DWM backdrop material (blur, acrylic or mica) shows what lies beneath it; and the target's box does not touch a rounded corner, which DWM draws transparently. If any of them cannot be attested, the check compares the whole frame, as ADR-0032 point 6 does.

5. **Fallbacks.** The fallbacks of ADR-0032 point 6 remain: a rectangle or group that cannot be attested, a target box that is not inside `T`'s rectangle, and two group reads around the frame capture that differ. Point 4 is added to them. Windows below `T` need no rectangle. The z-order walk keeps its bounds (no cycles, at most 4,096 visited handles), and a walk that fails falls back to the whole frame.

6. **Margin.** `action_freshness_margin_px` (default `0`) grows the rectangle of `T` in the overlap test and the compared region, as ADR-0032 already does for the region. A larger margin can only add windows and tiles, so it can only refuse more. It also decides whether a window whose shadow reaches the target's box enters the group by its identity (see Consequences). Changed pixels inside `T` refuse even at margin 0, because the exact comparison covers all of `T`'s rectangle. Its default stays `0` until the owner sets it from a measurement.

7. **Nothing else loosens.** Everything else in ADR-0032 keeps its force: the switch and its default `screen` (point 1); the whole-frame geometry and epoch checks (point 4); the exact comparison inside the group, with no tolerance and no mask (point 5); the gate, the last look, the age bound of ADR-0025 and the operator's stop (point 7 of ADR-0032); the visual-effect measurement over the whole frame; dry runs that return before the freshness capture; a refusal that spends the snapshot; and the D0 diagnostic of the owner's decision of 15 October 2026, which stays in `_meta` only, with `group_tiles` only when the group is attested.

8. **Naming.** The configuration value stays `"target_window"`. No live run has used it, so no recorded result depends on the old group. The M1 repeat keeps ADR-0032's group as its baseline (see Required validation).

## Alternatives considered

- **Keep ADR-0032's group.** On this desktop it compares the whole frame (M1), so it gains nothing here. It does not justify the switch.
- **The target's rectangle alone (ADR-0032 option B).** Rejected in ADR-0032, because it drops the geometry guard and ignores z-order: a window that rises over the target without changing the rectangle's pixels would pass. This amendment keeps the geometry guard and the identity of the windows above the target.
- **Also include windows below the target that overlap it.** Not chosen. Under an opaque target they cannot reach the visible pixels of the box or the click, so including them only adds refusals for moves that cannot affect the action.
- **A region derived from perception (ADR-0032 option C).** Still not for this amendment: it depends on the detector's supports, which the operating system does not confirm.

## Safety analysis

The table applies to the amended group, with the identity check of point 3 and the opacity condition of point 4.

| Change | Reaches the click or the target's visible pixels? | Result under this amendment |
|---|---|---|
| A dialog, menu popup or toast above the target overlaps it | Yes | In the group: refused (pixels or identity) |
| A window above the target, which does not overlap the target's rectangle, casts a shadow over the target's box | Yes, when the target is opaque | Changed shadow pixels inside `T` refuse even with margin 0; the casting window's identity is included only if its attested bounds intersect the expanded target region |
| A window below the target changes its pixels, hidden under the target | No | Accepted |
| A window below the target moves and overlaps the target | No: the click lands on the opaque target | Accepted. ADR-0032 refuses here |
| A window below the target rises above it between the looks and overlaps it | Yes | Refused: the ordered list changes |
| The target moves or resizes | Yes | Refused: the rectangle is compared, even when the old rectangle is unchanged |
| The target is layered, click-through or translucent, has a DWM backdrop, or its box touches a rounded corner | Yes: what lies beneath shows through | Whole frame, fail closed |
| The window under the point changes | Yes | A changed target HWND refuses through the attested target/group identity check; ADR-0020 gate and last-look checks additionally apply when the operator has configured a window scope; unavailable identity evidence falls back under point 5 |
| The foreground application changes | Possibly | Refused at the gate and the last look by policy (`POLICY_DENIED`) |
| The foreground window (HWND) changes, with the same application and no window scope | Possibly | Refused with `STALE_SNAPSHOT` by the foreground identity field of point 3, whatever the group says |
| DPI, resolution, monitor layout or epoch | Yes | Refused by the whole-frame geometry check |
| The caret, or an animation inside the target | Yes | Refused, as under ADR-0032 (no mask) |

## Consequences and cost

- **Easier.** Changes in windows below the target stop refusing actions, so fewer refusals come from changes that cannot reach the target. The gain depends on the target: a target that covers most of the output, for example a maximised window, gains little. The M1 repeat will show how much on the owner's desktop.
- **Harder.** The actuator port needs, for each window above the target, its rectangle and its place in the z-order, and the opacity attestation of point 4. The Win32 calls go in the `#[cfg(windows)] mod win` of the input adapter, under ADR-0016: one call per `unsafe` block, each with a `// SAFETY:` comment, and no raw pointer outside the module. The test doubles and the simulated desktop need the new attestations.
- **Watch: shadows and effects outside a rectangle.** DWM draws window shadows outside each window's rectangle. A window above the target whose rectangle does not overlap it can still darken part of the target's box through its shadow. Changed pixels inside the target's rectangle refuse even at margin 0, because the exact comparison covers all of that rectangle. What the margin decides is whether the casting window's identity enters the group: a window whose attested bounds intersect the expanded target region is included, and one outside it is not. These two cases are different and are tested separately (see Required validation). The reference PC must measure the shadow extent before any live use, and the owner sets the margin from that measurement.
- **Watch: the click target.** A changed target HWND refuses through the attested target/group identity check. The gate and last-look checks on the window under the point, from ADR-0020, apply additionally when the operator has configured a window scope, and then a window that takes the click refuses there. If the identity evidence cannot be attested, point 5's fallback applies.
- **Residual risk.** As ADR-0032 states, the guarantee narrows from "the screen is as observed" to "the target and the windows above it that overlap it are as observed". A window below the target can change while the target is acted on, and the engine acts on the target as it was observed. That is a reasoning risk on the agent's side. The amendment enlarges the part of the screen that is not guarded, and the owner should accept that knowingly.
- **Determinism (AGENTS.md §5.1).** The group is ordered by z-order and then by handle. No collection is iterated in hash order.

## Required validation

The implementer plans the new tests, not this ADR. The unit and BDD tests get their identifiers when they are planned, and each fails first where it tests new behaviour (AGENTS.md §5.10). They run on synthetic frames and port doubles, and they must show at least:

- a change in a window below the target that overlaps no window above it is accepted; the same change with that window raised above the target refuses;
- a one-pixel change inside the target window refuses far from the target, with no tolerance and no mask;
- a window above the target that overlaps it refuses when it changes pixels or changes the ordered list; a window that rises above the target and overlaps it between two looks refuses;
- a target that is layered, click-through, translucent or has a backdrop, and a box that touches a rounded corner, give the same result as `screen` on the same frames;
- the margin covers exactly the band it reaches, so a change inside that band refuses and a change beyond it is accepted;
- a changed pixel inside the target's rectangle refuses at margin 0, while the window that casts a shadow from outside the expanded region stays out of the list; the two cases are shown separately;
- a window whose attested bounds intersect the expanded target region enters the list at each margin, and one whose bounds do not intersect it does not;
- a foreground HWND change refuses with `STALE_SNAPSHOT` on identical pixels and identical group membership, with the same allowed executable and no window scope configured;
- the geometry and epoch guard refuses in both scopes, and a moved target refuses with its old rectangle unchanged;
- over generated frames and z-orders, the amended scope refuses on pixels only where the ADR-0032 group does, and it never accepts a changed tile that lies inside the amended group.

The tests that pin ADR-0032's group describe the rule this amendment replaces. UT-57, UT-59 and UT-63 in `plan_pruebas.csv` must be revised with it where their expected results change.

Then, before any live use:

- an independent safety review of the group, the identity list, the opacity condition and the margin, recorded under `docs/reviews/` with its findings and the test that covers each one;
- M1 repeated on the owner's desktop, with N and the criterion written down by the owner before the run. The repeat reports ADR-0032's group and the amended group on the same pairs, so the reduction is measured and not inferred. The probe decides nothing about activation;
- the shadow and corner extent measured on the reference PC, and the margin default set by the owner from that result;
- then, and only with the owner's consent for each session, IT-08 (M2), as ADR-0032 describes it;
- `cargo test --workspace`, the clippy runs with the Windows features and the thesis checks, as for any engine change, and the freshness cost (`freshness_us`) reported before and after, as ADR-0010 and ADR-0032 require.

## Reversibility

Reverting this acceptance keeps ADR-0032's group, which the switch uses today. If a live result shows that the amended scope should have refused and did not, or the independent review finds a case the rule does not cover, the scope goes back to `screen` and a superseding ADR either restores the earlier group or closes the gap. Setting `action_freshness = "screen"` restores the whole frame at once. Nothing is persisted, and no wire field changes.

## Provenance

[D] Written from the M1 results of 17 October 2026 ([S4 §D.1.1](../thesis/S4_SESIONES_PENDIENTES.md)), from ADR-0032 (its Context, its point 2 and its safety analysis) and from the owner's request of the same date, and from the owner's agreement of 16 October 2026 on M1 (descriptive, no pass threshold; a cover of the whole output triggers this proposal). [P] The amendment, the group of point 1, is a new proposal that the owner accepted on 2026-10-17. It amends point 2 of ADR-0032 and supersedes nothing else. ADR-0016, ADR-0020 and ADR-0025 keep their force.

## Implementation notes

Written by the implementer while the ADR was Proposed; the owner accepted it on 2026-10-17, after these notes, and nothing here is an acceptance.

- **What was built.** `WindowRead::occluders` (`src/application/src/engine/freshness.rs`) builds the group of point 1 from the complete, sorted window read: the target window plus every window with a smaller `(z_order, handle)` key whose rectangle overlaps the target's rectangle grown by `action_freshness_margin_px`. The group keeps the observed foreground handle as a separate identity field, so a foreground handle change makes the group differ and refuses with `STALE_SNAPSHOT` whatever ADR-0020 is configured to do. The ADR-0032 group (`WindowRead::group`) stays only as the M1 probe's baseline (`FreshnessProbeGroup`), unchanged.
- **Identity by relative order.** The members are ranked inside the group (0, 1, 2 …) instead of keeping the OS z-order index, so a window above the target that overlaps nothing can close or open without changing the identity. A swap of order between two members, a member entering or leaving, and any rectangle change still make the group differ (UT-67). The whole-read equality around the observation capture is unchanged, so it still compares every window.
- **Opacity (point 4).** New port method `OsActuator::window_opacity(&TopLevelWindow) -> Option<WindowOpacity>`, default `None` (= whole frame), so a double that does not implement it behaves as an unattested window. The engine asks at the first and second group reads around the capture and at the last look; an unattested answer at any of them only costs the scope (the whole frame is compared) and is never reported as a change. The Windows adapter attests `Some` only when the window is neither `WS_EX_LAYERED` nor `WS_EX_TRANSPARENT` and `DWMWA_SYSTEMBACKDROP_TYPE` reads `AUTO` (0, the value of a window that never opted in) or `NONE` (1); `MAINWINDOW`, `TRANSIENTWINDOW`, `TABBEDWINDOW`, any other value, a failed query or a window that is gone attest nothing. The simulated desktops attest opaque windows with no corner exclusion, because they are opaque by construction.
- **Corner constant.** `CORNER_EXCLUSION_PX = 32` physical pixels (`src/adapters/input/src/sendinput.rs`). DWM rounds corners with a radius of 8 px at 100 % scaling, so 32 px covers scaling up to 400 %. The adapter reports it for every window, whatever corner preference the window reports (`DWMWCP_DONOTROUND` included), and the engine applies it: a target box that intersects the 32 × 32 square at any of the four corners of the target window (it lies within 32 px of two adjacent edges) is not attested opaque and the whole frame is compared.
- **Limit: blur-behind.** `WS_EX_NOREDIRECTIONBITMAP` windows are never attested (review 2, F2): such a window can be composed with per-pixel alpha by DirectComposition without being layered, and nothing else attests that its box is opaque. A zero extended style counts only when the last error is zero after the query (`SetLastError(0)` before, `GetLastError` after, review 2, F1); a zero with an error is a failed query and attests nothing. Legacy blur-behind (`DwmEnableBlurBehindWindow`) and the undocumented accent policy have no documented getter, so the adapter cannot attest their absence; a window that uses them is not told apart from an opaque one. The residual is bounded: whatever such a material shows through is drawn from the pixels beneath, and a change there is a pixel change inside the target window's rectangle, which the exact comparison refuses. What is not covered is a case with no pixel change at all. The owner accepts or rejects this limit with the ADR.
- **Margin and ADR-0032 (review 2, F4).** The selection rule is the one in point 1: overlap with the target rectangle grown by the margin. At margin 0 the amended group is a subset of the ADR-0032 group, and a test asserts it. At a positive margin it can include a window that ADR-0032 excludes (for example a window 4 px from the target at margin 5), so it can refuse where ADR-0032 accepts: a conservative extension, more refusals and never fewer. The sentence in Required validation that the amended scope "refuses on pixels only where the ADR-0032 group does" holds at margin 0 only; the owner should read it that way.
- **Walk errors (review 2).** The z-order walk captures the thread error around `GetWindow(GW_HWNDNEXT)`: a null answer with an error is a failed enumeration, not the end, and falls back to the whole frame. This also hardens the ADR-0032 path, which shares the walk.
- **Limits of the tests.** The Win32 code is compiled and linted, never exercised live; the pure decision (`window_opacity_of`, UT-75) is unit-tested. The ADR-0032 tests that pinned the old group were rewritten, and UT-57 and UT-63 and the ones that changed the z-order of a lone window are listed in the pull request. The M1 probe now also reports the amended group beside the ADR-0032 one on the same pairs (`occluder_group`, with the engine's own selection code); the repeat itself is not run, see S4 §D.1.2.

## Resumen (ES)

Aceptada por el responsable el 17 de octubre de 2026, con el límite residual que declaran sus notas de implementación: con `target_window`, la comprobación de frescura compara sólo la ventana objetivo y las ventanas que están por encima de ella y la solapan, y no las que están debajo, porque una ventana opaca no deja ver lo que tiene detrás. Motivo: en la medición M1 del 17 de octubre de 2026 el grupo de ADR-0032 cubrió toda la salida en los 60 pares, así que no reducía el área comparada. Sin opacidad atestiguada del objetivo, o con una caja que toque una esquina redondeada, se compara la pantalla entera. El interruptor sigue en `screen` por defecto. El adaptador no puede atestiguar el desenfoque heredado ni la política de acento, y el responsable acepta ese límite: la comparación exacta del rectángulo del objetivo rechaza los cambios de píxeles que esos materiales dejen ver, y no cubre un caso sin ningún cambio de píxeles. Antes de cualquier uso en vivo harán falta la validación que lista el ADR, que incluye la repetición de M1 con los dos grupos, la medición de la sombra y de la esquina en el equipo de referencia y, por sesión y con consentimiento del responsable, IT-08 (M2).
