# 0018. Two-phase type with element-level focus confirmation

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Project owner, 2026-10-14

## Context

The second S4 round with UI Automation evidence (`docs/thesis/S4_ACEPTACION_REDUCIDA.md`,
review `docs/reviews/2026-10-14-s4-uia-acceptance.md`) ran `type` live for the first time
and found two defects:

1. **`type` drops keys.** The `SendInput` actuator sent the focusing click and then the
   keys, after checking keyboard focus at window level (the focused window belongs to the
   attested process and contains the click point). A single-window application satisfies
   that check at once, so the keys reached the application in the same UI frame as the
   click. The likely cause, read from the egui 0.36.2 source and not verified frame by
   frame, is that its text field handles text events before it takes focus in that frame.
   Three of four typings left the field without the text while the engine reported a
   delivery.
2. **A false success.** With `expected_effect: visual_change` the engine compared the
   screen against the frame captured before the whole action, so the redraw caused by the
   focusing click (focus ring, caret) counted as the effect of typing.

The window-level check cannot see either: the right window had focus the whole time. What
decides the outcome is whether the *field* has keyboard focus, and UI Automation
([ADR-0017](0017-ui-automation-evidence-fused-with-vision.md)) can say so. SDD §2.9 already
requires "a confirmed input and verified focus", and distinguishes events delivered, visual
change detected and postcondition checked.

## Decision

The engine owns two phases of `type`, and the actuator only executes them.

1. **Port changes.**
   - `OsActuator` gains `delivers_in_phases()`, `deliver_focus_click(action)` (the move and
     the click on the target, no key) and `deliver_keys(action)` (the keys only). The
     existing `deliver` stays, for clicks, scrolls and for `type` without accessibility
     evidence. The defaults say "no phases" and fail, so an actuator that does not
     implement them is unchanged.
   - `AccessibilityProvider` gains `reads_keyboard_focus()` and `keyboard_focus() ->
     Result<Option<AccessibleElement>, PortError>`: the element that has keyboard focus
     now, in the attested foreground application, on screen and on the captured monitor.
     Reading the focused element is a read, not an action. The adapter's `clippy.toml`
     still forbids input, focus-setting and pattern calls, and the value is never read.
2. **Flow** (when the actuator delivers in phases and the provider reads focus):
   1. Gate and checks as for any live action: policy, allowlist, freshness capture, the
      last look at the stop and the foreground application.
   2. `deliver_focus_click`. A failure or a partial receipt is `indeterminate` with
      `input_delivered: false`, as for any action.
   3. **Confirm focus.** Poll `keyboard_focus` within the verification window
      (`[engine] verify_timeout_ms`, every `verify_poll_ms`) on the injected `Clock`. Focus
      is confirmed when the focused element is the target's own element: an enabled,
      on-screen, non-password input with a writable value whose box matches the target's
      box by the IoU that already decides that two boxes are the same control
      (`accessibility_iou`, ADR-0017). Focus in another element, a container, a password
      or disabled field, or no focus, does not confirm. A failed read counts as no focus.
   4. **No focus in time: no key at all.** The result is `indeterminate`
      (`INDETERMINATE_EFFECT`, `retryable: false`, `next_step: reconcile_effect`) with
      `input_delivered: true`, because the click was delivered. Nothing is retried, the
      snapshot is spent and a repeated `request_id` replays the stored result.
   5. **Reference frame.** With `expected_effect: visual_change`, capture a frame now,
      after the click and before the keys.
   6. **Last look** at the operator's stop, consent and foreground application, right
      before the keys. If any changed, no key is sent and the result is `indeterminate`
      with the click reported delivered; nothing is rolled back.
   7. `deliver_keys`. The `SendInput` actuator refuses it without the matching click, and
      checks that the attested application is still in the foreground and owns the focused
      window and that no modifier is held. A failure or a partial receipt is
      `indeterminate` with `input_delivered: true` (the click was delivered).
   8. **Report.** `visual_change` is measured against the reference frame of step 5, never
      against the pre-click frame, within the same verification window. It is evidence of a
      change, not proof of the typed value, and the field's value is never read.
3. **Without accessibility evidence** (`accessibility = "none"`, the default) or with an
   actuator that cannot split `type`, nothing changes: the actuator's own window-level
   focus check between its click and its keys stays. The two-phase confirmation needs
   `[models] accessibility = "uia"`; this limit is documented, not hidden.
4. **Unchanged.** `click` and `scroll`, the action gate, fail-closed actuation
   ([ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md)), the bounded `unsafe`
   of the Windows adapters ([ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md))
   and the privacy rules of [ADR-0012](0012-data-minimisation-and-evidence-traceability.md).
   The contract's wire format and schemas do not change.

## Alternatives considered

- **A fixed delay between click and keys.** A longer sleep lowers the loss rate without
  knowing the cause, spends time when the field is ready early and still loses keys on a
  slow application. Rejected: an event-based confirmation replaces a guess.
- **Move focus through UI Automation (`SetFocus`) and skip the click.** The provider never
  acts or moves focus (ADR-0017), and focusing through a pattern is an action the gate
  would not have validated. Rejected.
- **Confirm by reading the field's value.** Leaks content the policy may not allow and
  breaks the "never read a value" rule. Rejected.
- **Keep the window-level check and retry the keys.** An automatic retry on an uncertain
  delivery is forbidden (ADR-0007) and could type the text twice.
- **Let the actuator poll for focus.** The window-level facts it can read are exactly the
  ones that were insufficient, and the engine owns time through the injected `Clock`, so
  tests stay deterministic.

## Consequences and cost

What becomes possible: `type` that sends keys only once the target field has keyboard focus,
and an honest report when it does not. What becomes harder: one more read per poll during
`type` (a cached-property read of one element, to be measured on the reference machine,
ADR-0010; the poll interval is the operator's `verify_poll_ms`); the confirmation depends on
what the application exposes through UI Automation, so a field the application does not
expose as an editable input never confirms and `type` ends `indeterminate` there (fail
closed). Residual limits, all documented rather than hidden: a residual window between the
last look and the OS remains (chapter 08 §8.3); a blinking caret or a late animation can
still change pixels between the reference frame and the check, so `effect_observed` after
`type` means "something changed after the keys", not "the text was typed"; and the
IoU threshold is the ADR-0017 `[P]` value, to calibrate.

## Required validation

- Engine tests (`src/application/src/engine/tests/typing.rs`):
  `type_sends_no_keys_until_the_target_field_has_keyboard_focus`,
  `type_without_focus_confirmation_is_indeterminate_and_sends_no_keys`,
  `type_visual_change_is_measured_after_the_focusing_click`,
  `focus_on_another_element_is_not_accepted`,
  `without_accessibility_focus_type_keeps_the_window_level_behaviour`, the stop and
  foreground changes after the click, and the failed phases.
- BDD-34 and BDD-35 (`docs/thesis/features/06_accion_segura.feature`); BDD-28 keeps
  covering `type` without accessibility.
- Unit tests of the focus match (`focus_is_on`) and of the key-phase checks of the
  `SendInput` actuator.
- Manual: the ignored test that reads which element has keyboard focus (role and size
  only). A supervised live run with `type` remains owner-only (S4).

## Reversibility

Set `[models] accessibility = "none"` and `type` returns to the single-call, window-level
behaviour. To remove the change entirely, drop the three port methods and the engine's
`deliver_typing`; the wire format is untouched. Reconsider if the focus read proves slow or
unreliable on the reference applications.

## Provenance

Findings 1 and 2 of `docs/reviews/2026-10-14-s4-uia-acceptance.md`; SDD §2.9. Builds on
[ADR-0017](0017-ui-automation-evidence-fused-with-vision.md) and keeps
[ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md).

## Resumen (ES)

`type` se entrega en dos fases que controla el motor. Primero el clic de enfoque; después
se consulta, por UI Automation y dentro del plazo de verificación (con el `Clock`
inyectado), qué elemento tiene el foco del teclado, y sólo cuando es el propio campo
destino (entrada habilitada, en pantalla, no contraseña, con la misma caja por IoU) se
envían las teclas. Si el foco no se confirma a tiempo no se envía ninguna tecla: el
resultado es `indeterminate` (`INDETERMINATE_EFFECT`, `reconcile_effect`) con la entrega
del clic informada, sin reintento. La postcondición `visual_change` se mide contra un
fotograma capturado después del clic y antes de las teclas, lo que elimina el falso éxito
del clic de enfoque. Nunca se lee el valor del campo. Sin evidencia de accesibilidad
(`accessibility = "none"`) `type` conserva la comprobación de foco a nivel de ventana del
actuador; la confirmación en dos fases necesita `uia`. El puerto `OsActuator` gana
`deliver_focus_click` y `deliver_keys`, y `AccessibilityProvider` gana `keyboard_focus`
(una lectura, no una acción).
