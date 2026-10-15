# 0032. Scope the freshness check to the target window, behind a switch that stays off

- **Status:** Proposed
- **Date:** 2026-10-15
- **Approval:** Pending. The project owner decides. This is a proposal: it changes no requirement, no contract default, no test status and no checkbox of chapter 06 §6.7.

## Context

**What the check does today.** Before every live delivery the engine captures the screen again and compares it with the committed base, the frame of the observation that the action refers to (`Engine::fresh_frame`, `src/application/src/engine/act.rs:396-418`). `dirty_tiles` (`src/domain/src/incremental.rs:281-312`) compares every 64×64 tile of the whole captured frame (1920×1080 on output 0), pixel by pixel on the three colour channels (`tile_changed`, lines 314-321), with no tolerance. One differing tile, or a geometry or epoch change, returns `STALE_SNAPSHOT`; nothing is delivered and the snapshot is spent (`act.rs:412`). The base is the frame the observation staged and committed (`SnapshotStore::commit`, `src/domain/src/snapshot.rs:128-141`; `base()` at line 90).

**The evidence.**

1. *Run 2 of the live round of 15 October 2026* (S3 §9.6; finding 1 of `docs/reviews/2026-10-15-live-round-close-refused.md`). The opening click of IT-03 was delivered. The closing click was refused twice with `STALE_SNAPSHOT`, after heavy observations (regions re-analysed) with `age_ms` 138 and 133. The two heavy closings that were delivered had `age_ms` 155 and 121. Light observations (no regions) were delivered 4 times in 4.
2. *Without input, the screen changed by itself.* In dry-run pairs about 125 ms apart, 2 of 3 pairs re-analysed regions with the menu closed. In pairs 15–30 ms apart, 0 of 9 did (S3 §9.6).
3. *The box 4 round of 15 October 2026* (S3 §10.2, §10.6). Freshness refused the action in four runs and six attempts: C0 first (`c0-stale1`), D1 second (`d1-second`), D1 fourth (`d1-fourth`, three times in a row) and the first attempt of D1 sixth (`d1`). Observation-to-action times were 3.9–4.6 s where recorded (§10.5). The round did not record which tiles differed (§10.6), so no refusal is attributed to a cause. Six live runs of the BDD-35 case did not meet the pre-registered criterion (§10.3); freshness was one of the reasons.
4. *The cause is unknown.* The leading candidate in S3 §9.6 is the blinking text caret of the editing area; the test editor had the same problem in the tenth round and fixed it (`CHANGELOG.md` 1.0.39). The other candidates named in S3 §10.6 are the visible Orca terminal, the taskbar clock and the focus frame when the window changes. These are hypotheses. The evidence cannot show that a check limited to the target window would have passed in any refused run.

The engine behaves as specified: nothing was sent, so this is not a safety failure. The cost is availability. It blocks the live cases that the dossier still needs (IT-03's closing click, and the BDD-35 case).

**What the dossier says.**

- SDD §2.6 (`docs/thesis/02_SDD_ARQUITECTURA_MVP.md`): the strict MVP policy asks for a new observation when a popup makes the observation stale. «Validación por región inalterada, tras demostrar su corrección» is named as a later improvement and is not designed.
- SDD §2.7: a change of monitor, DPI, rotation or a window out of policy increments the geometry epoch and forces a full reconstruction.
- SDD §2.8: the core works in physical pixels of the selected monitor, with half-open boxes.
- Chapter 04 §4.2 (Algorithm 1): a window change is a fallback to a full refresh. The caret may be masked only when its location and timing are justified.
- Chapter 08 §8.2: revalidation of focus and visibility is a minimum control.
- RF-11 (chapter 03): the safe action validates «frescura», and that requirement does not define the scope of the comparison.

**What the actuator can attest today.** `TopLevelWindow` carries a class and a handle, and no rectangle (`src/application/src/ports.rs:237-245`). `OsActuator::foreground_window` and `window_at` return it (`ports.rs:262-275`). The engine asks for windows only when an operator window scope applies (`act.rs:484-495`). The Windows input adapter calls `GetForegroundWindow`, `WindowFromPoint` and `GetClassNameW` in its `win` module (`src/adapters/input/src/sendinput/win.rs:114, 165, 184`). It reads no rectangle and enumerates no windows.

## Decision

Proposed: if the owner accepts this ADR, the freshness check of a live action gets a second scope, chosen by configuration.

1. **Switch.** `[engine] action_freshness = "screen"` is the default and is exactly today's whole-frame check. `"target_window"` selects this ADR. The value is validated at start, like the other `[engine]` keys.
2. **Window group.** With `"target_window"`, the pixel comparison runs on the tiles that intersect the **window group** of the target. The group is:
   - the top-level window that contains the target (the window the OS reports under the target's point at observation);
   - the foreground window;
   - every top-level window whose visible rectangle overlaps one of those.

   Rectangles are in physical pixels of the observed output, clipped to the frame. The group is read in the same observation as the frame (see point 6).
3. **Group identity.** `last_look` (`act.rs:450`) reads the group again. Any difference in its ordered list (z-order, handle, rectangle) refuses with `STALE_SNAPSHOT`, with the existing message, and spends the snapshot. A move, a resize, a change of z-order, a window that appears in or leaves the group, or a change of foreground window is therefore a refusal, even when the pixels of the old rectangle did not change. The group check runs after the existing last look, so an application change keeps its current refusal code.
4. **Geometry and epoch stay whole-frame.** A geometry, DPI or epoch change refuses under both values, as `dirty_tiles` already does (`FullRefreshReason::GeometryChanged`).
5. **Exact inside the group.** The comparison inside the region is the current one: exact per pixel, no tolerance, no mask. The only new parameter is `action_freshness_margin_px` (default `0`), which extends the region. A larger margin can only refuse more, so it cannot let a change through.
6. **Fail closed.** The check falls back to the whole frame when the actuator cannot attest a rectangle or the group, when the target box is not inside its window's rectangle, or when the two reads of the group taken around the frame capture differ.
7. **Nothing else loosens.** The gate and the last look keep every check they have: stop, consent, policy, foreground application, window scope (ADR-0020) and the age bound (ADR-0025). The visual-effect measurement keeps comparing against the whole frame. For `type`, the scope applies to the check before the focusing click; the keys still wait for element-level focus (ADR-0018), unchanged.
8. **Off by default.** The switch is turned on only by configuration, and only after the validation in *Required validation* passes and the owner accepts this ADR.

## Alternatives considered

Four options were compared. They differ in which pixels decide a refusal.

- **A. Keep the whole frame (today).** Every changed pixel refuses. No gap in safety; the cost is the availability described in the Context. Verdict: remains the default.
- **B. The target window's rectangle plus a margin; everything outside ignored.** It covers the click's pixels, because the target lies inside its window, and a window that receives the click must overlap the target. As stated it is not enough, for two reasons. It drops the geometry guard: a DPI or resolution change moves every rectangle, and the comparison would run on stale coordinates. And it compares pixels only, so a transparent window that starts to receive clicks, or a change of z-order that leaves the pixels unchanged, passes. Verdict: not sufficient; its fix is D.
- **C. Dirty tiles that intersect the target element's contextual closure (Algorithm 1, chapter 04 §4.2).** The region comes from perception: the target box, grown to a fixed point by its supports and relations. It could exclude a change in the same window that is far from the target, such as the caret. But safety would depend on the detector finding every support: a missed support is a missed region. A popup elsewhere in the same window (SDD §2.6) would not be covered. The closure often grows to most of the window, so the gain is small. Verdict: not for this ADR. It becomes the candidate if the measurement M1 shows that the changed tiles lie inside the target window but outside the target's closure.
- **D. B, with the whole-frame geometry and epoch guard and the group identity check (proposed).** It keeps B's gain and closes both gaps above. Verdict: recommended design, switch off.
- **E. Complementary, not alternatives.** Shorten the observation-to-action window (S3 §10.7, option b; about 4 s today, mostly client-side UI Automation reads), and a settle observation in the client before a close (S3 §9.6, item 1). Both lower the chance that a change lands inside the window. Neither removes the whole-frame refusal, and neither needs this ADR.

## Safety analysis: what a change outside the region can still do

The analysis is for D, with the group identity check.

| Change | Can it alter where the click lands? | Result under D |
|---|---|---|
| A modal dialog, toast or overlay over the target | Only if it overlaps the target window, which puts it in the group | Refused (pixels or group identity) |
| A transparent overlay that receives clicks | Yes, if it overlaps; then it is in the group | Refused (group identity). A click-through overlay cannot receive the click, and it is refused too if it overlaps |
| A window raised above the target (z-order) | Yes, if it overlaps | Refused (order of the group compared) |
| The target window moves or resizes | Yes | Refused (rectangle compared), even when the old rectangle is unchanged |
| The taskbar clock, another application, the desktop | No, unless the window they belong to overlaps the target window. A window that does not extend under the taskbar leaves the clock outside its rectangle [D] | Accepted, as intended |
| The visible Orca terminal or any other window | No, unless it overlaps the target window | Accepted if outside; refused if overlapping |
| The caret, or any animation inside the target window | Changes pixels of the group, not the click's destination | Refused, as intended (no mask) |
| DPI, resolution, monitor layout, epoch | Yes | Refused (whole-frame geometry check) |
| Foreground application or window changes | Yes | Refused (last look and group identity) |

**Residual risk, stated for the owner.** The snapshot also describes other windows. Under D the guarantee narrows from "the screen is as observed" to "the target's group is as observed". If the agent chose the target from an outdated view of another window, the engine acts on the target as it was observed. That is a reasoning risk on the agent's side, not a perception one, and accepting D accepts it.

## Recommended option and why

**D**, with the switch off until the measurement says where the changed tiles were.

1. **Safety rests on facts from the OS, not on perception.** The group is the set of windows the OS reports as overlapping the target, and the click's destination is the window the OS reports under its point. C depends on the detector's supports and relations; D does not.
2. **It covers the popup case that the dossier names.** A popup or dialog that could receive the click is in the group.
3. **It keeps the geometry guard that B drops.**
4. **It excludes the candidates of S3 §10.6 when they lie outside the target window**: the taskbar clock and other windows that do not overlap it.
5. **It is reversible by configuration**, with no persisted state, no wire change and no change of default.
6. **It is the design the dossier anticipated** (SDD §2.6), and this ADR states the proof that the dossier requires.

The limits are stated plainly. D does not fix the leading candidate, the caret in the same window. The evidence does not show which tiles changed. The Orca terminal helps only when it does not overlap the target. So the recommendation is to **design now and switch on later**. If the caret is the cause, D does not solve it, and the next proposal is C.

## What stays forbidden

- Any tolerance in the freshness check (a number of pixels, tiles or a percentage), in either scope.
- A caret mask, a blink exclusion or any per-application pixel exemption. The caret is compared like any other pixel of the group. A mask needs its own ADR under chapter 04 §4.2.
- A perception-derived region (Algorithm 1's closure, detector boxes) as the freshness scope. Option C needs its own ADR.
- A retry on the same snapshot after a refusal. A refusal spends it (`snapshot_spent`).
- Window titles as evidence (ADR-0020), and `applicationframehost.exe` in any allowlist.
- Any change to the age bound (ADR-0025), the last look, the window scope (ADR-0020) or the operator's stop, which belongs to the operator alone.
- `unsafe` outside `src/adapters/input/src/sendinput/win.rs` (ADR-0016).
- Turning the switch on by default, or in a live session without the owner's consent for that session.
- Marking a checkbox or a requirement status from this ADR. Chapter 06 §6.7 and the requirement states change only with evidence, by the owner or the architect.

## Consequences and cost

- **Easier.** Fewer refusals from changes outside the target's group, if the measurement shows that such changes caused the refusals. Fewer tiles compared per action, so `freshness_us` (`_meta`, `io.ommatix/action_timings`, ADR-0010) should not grow. The implementation PR must still report it before and after.
- **Harder.** The actuator port grows: a rectangle per window, and an ordered list of the windows that overlap the target. Both default to `None`, which means the whole frame. The Win32 calls for the rectangle and the z-order walk go in the `#[cfg(windows)] mod win` of the input adapter. ADR-0016 allows `unsafe` exactly there, one call per block, each with a `// SAFETY:` comment. The test doubles in `src/application/src/engine/tests.rs` and the simulated desktop need the new methods.
- **Determinism (AGENTS.md §5.1).** The group is ordered explicitly by z-order and handle, never by iteration over a `HashMap`. The union of rectangles uses the existing `BBox` operations.
- **Watch.** A group that covers most of the screen behaves like A: no gain, no harm. A window partly outside output 0 is clipped, and the target must lie inside the frame. The margin is an unmeasured parameter. Rectangles must be read in the capture's DPI awareness, so that they share the frame's coordinate space (SDD §2.8). Whether the visible frame bounds or the full window rectangle is the right attestation (they differ by the invisible resize border) is decided by measurement on the reference PC.
- **Proof.** The claim must be shown by tests that change a pixel inside the group, outside it and at its boundary. It cannot be argued in prose alone.

## Test plan (proposed; not yet in `plan_pruebas.csv`)

The identifiers below are proposed; they enter `plan_pruebas.csv` only with the PR that accepts this ADR. No oracle test (OR-xx) covers freshness today, so the expected results are those of the whole-frame check (`screen`) on the same frames. Where an oracle test exists, it sets the expected output (AGENTS.md §5.11).

| ID | Name (proposed) | What it protects | Expected result |
|---|---|---|---|
| UT-55 | `a_change_outside_the_target_group_is_accepted_in_target_window_mode` | Changes outside the group do not refuse | Accepted; `screen` refuses on the same frames |
| UT-56 | `a_one_pixel_change_inside_the_target_window_refuses_far_from_the_target` | Same-window changes still refuse; no tolerance, no mask | Refused; nothing sent |
| UT-57 | `a_window_overlapping_the_target_window_is_in_the_group` | The overlap rule | A change in an overlapping window refuses |
| UT-58 | `a_moved_target_window_refuses_even_when_the_old_rectangle_is_unchanged` | Rectangle identity at the last look | Refused with `STALE_SNAPSHOT` |
| UT-59 | `a_change_of_group_order_or_foreground_refuses` | z-order and foreground identity | Refused |
| UT-60 | `a_geometry_or_epoch_change_refuses_in_target_window_mode` | The whole-frame geometry guard (D) | Refused (`GeometryChanged`) |
| UT-61 | `an_actuator_that_cannot_attest_rectangles_checks_the_whole_frame` | Fail-closed fallback | Same result as `screen` |
| UT-62 | `the_visual_effect_is_measured_against_the_whole_frame` | Effect reporting unchanged | `visual_change` identical under both values |
| UT-63 | `target_window_accepts_exactly_when_no_changed_tile_meets_the_group` | Differential test against the whole frame, as UT-08 does for tiles | On generated frames and groups, the two scopes agree whenever the changed tiles lie inside the group |
| UT-64 | `the_default_freshness_scope_is_the_whole_frame_and_unknown_values_are_refused` | Default off; configuration validated | Default `screen`; an unknown value fails at start |
| IT-08 | Live check of the target-window scope (M2 below) | The behaviour on the reference PC | Pre-registered criteria, set by the owner |

BDD sketch, in Spanish, as it would enter `docs/thesis/features/06_accion_segura.feature` (the feature file's own style; the IDs are proposed):

```gherkin
  @BDD-55 @RF-11 @UT-55
  Escenario: BDD-55 Un cambio fuera de la ventana objetivo no rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y una ventana sin relación con el objetivo que cambia su contenido antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces la acción se entrega y el snapshot queda gastado

  @BDD-56 @RF-11 @UT-56
  Escenario: BDD-56 Un cambio dentro de la ventana objetivo sí rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y un cursor de texto que parpadea dentro de la ventana objetivo, lejos del objetivo
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento

  @BDD-57 @RF-11 @UT-57
  Escenario: BDD-57 Una ventana emergente sobre el objetivo rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y una ventana emergente que se solapa con la ventana objetivo aparece antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento

  @BDD-58 @RF-11 @UT-60
  Escenario: BDD-58 Un cambio de geometría rechaza la acción con cualquier alcance
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y el monitor cambia de escala antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento
```

Measurements, both proposed:

- **M1 (no input; owner present).** The owner brings the windows to the front by hand, as in S3 §10. Then N pairs of observations about 150 ms apart, with the menu closed and then open, using the `observe` scenario. The diagnostic D0 counts, for each heavy observation, the tiles that differ, the tiles inside the group and whether any touches the target's box. Only numbers are recorded: no pixels, no labels, no coordinates (ADR-0012), and the raw output stays outside the repository. The owner sets N and the criterion before the run. This ADR does not set them.
- **D0 (diagnostic, a separate PR with a test).** On a freshness refusal, the counts above and nothing else, in stderr or `_meta`. It is the measurement that S3 §9.6 item 3 asks for. It lands before M1 is read.
- **M2 (live, supervised, IT-08).** The IT-03 closing click and the D1 steps with `action_freshness = "target_window"`, under criteria written down before the run, in the form that S3 §9.6 and §10 use. Consent is given for each session, only for the test applications the owner allows for that session, the stop file is outside the repository, and no coordinates or labels are logged. The owner runs the window changes by hand.

## Required validation

Existing tests that must keep passing unchanged under the default `screen`:

- UT-03 (`ut_03_single_pixel_on_odd_row_is_detected`, `src/domain/src/incremental.rs`), UT-07 (`ut_07_epoch_change_forces_full_refresh`), UT-08 (`ut_08_differential_tile_diff_matches_scalar_reference_on_generated_frames`).
- UT-38 (`ut_38_action_on_previous_snapshot_is_stale_after_a_new_observation`), BDD-38 (`stale_snapshot_asks_for_a_new_observation`, `src/acceptance/tests/thesis.rs`).
- The freshness tests of the review: `a_single_pixel_change_before_delivery_is_stale_and_sends_nothing` and `a_single_pixel_change_under_the_target_is_stale_and_sends_nothing` (`src/application/src/engine/tests/freshness.rs`); `a_change_before_delivery_is_not_an_observed_effect`, `a_live_action_after_a_scaling_change_is_stale_and_sends_nothing` and `an_action_before_any_observation_is_stale` (`src/application/src/engine/tests.rs`). The first of the two single-pixel tests changes a pixel far from the target. Its result is correct under `screen` and is the case the new scope changes, so it pins `screen` explicitly once the switch exists.
- IT-03 and SEC-05 keep their live evidence as recorded in S3 §9–10. Nothing in this ADR changes their status.

Also required before the decision is trusted:

- the ten tests of the *Test plan* section above, under their proposed identifiers, each failing first where it tests new behaviour (AGENTS.md §5.10);
- D0, then M1, with the owner's criterion, which the owner records before the run;
- M2, with the owner's consent for each session, and the criteria recorded before it runs;
- `cargo test --workspace` and the thesis checks, as for any change to the engine.

## Reversibility

Setting `action_freshness = "screen"` restores today's behaviour exactly. Nothing is persisted, and no wire field changes, so the code can be removed without a migration. The trigger for reverting is a live result that the scope should have refused and did not, or a refusal the scope should have passed. Either one switches the scope back to `screen` and starts a superseding ADR.

## Provenance

Written from the S3 round records (§9.6 and §10), from the review `docs/reviews/2026-10-15-live-round-close-refused.md`, and from the code cited above. It refines the later improvement that SDD §2.6 names and does not design. It uses ADR-0006 (transactional snapshots and commits), ADR-0016 (where `unsafe` may live), ADR-0020 (window attestation) and ADR-0025 (age bound, unchanged). It supersedes nothing. Labels follow the dossier: [P] marks the proposal and [D] marks a deduction from the code; the hypotheses are named as such.

## Resumen (ES)

Propuesta, sin decidir: la comparación de frescura antes de una acción viva sigue siendo exacta por píxel, pero su alcance puede ser el grupo de ventanas del objetivo (la ventana que lo contiene, la de primer plano y las que se solapan con ellas) en lugar de la pantalla entera. La geometría y la época siguen comprobándose sobre la pantalla entera, y el grupo se vuelve a comprobar justo antes de entregar. El motivo es que cuatro corridas de la ronda de box 4 se rechazaron con `STALE_SNAPSHOT` sin que sepamos qué píxeles cambiaron, y que el grupo lo decide el sistema operativo y no la percepción. El conmutador queda apagado hasta medir dónde cambiaron los tiles y hasta que el responsable acepte; no resuelve el cursor de texto dentro de la misma ventana.
