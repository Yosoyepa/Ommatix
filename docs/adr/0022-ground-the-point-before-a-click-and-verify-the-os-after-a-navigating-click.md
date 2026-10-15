# 0022. Ground the target point before every click, and verify the operating system after a click that rebuilds its own control

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Accepted by the project owner on 2026-10-14

## Context

The supervised end-to-end clients (`tools/s4/e2e_driver.py`, `tools/s4/e2e_dossier.py`) never
trust the engine's word that a click did what was meant. Before a click they find, through UI
Automation on the OS side, the one element that carries the step's name and control type and
record its RuntimeId (the AutomationId only when the RuntimeId is unavailable). After the click
they read the element under the cursor and require it, or its parent, to be that element
(`identity_hit`). A name is never compared (S4 fourth round, finding 4).

That rule stopped the tenth round's E2E-03 pilot on its first click. The click on the
`carpeta-a` breadcrumb (a `SplitButton`, identified by RuntimeId) was delivered and the
engine reported `effect_observed`. The OS-side check passed: the window's folder became
`carpeta-a`. The grounding after delivery then found a `Text`, whose chain did not hold the
identified RuntimeId, so the client counted `grounding_wrong` and `engine_false_success` and
the owner's safety rule stopped all live work (review `2026-10-14-s4-tenth-round`, finding 8).
The ninth round's Calculator «Estándar» click, whose pane closes when it is chosen, is the same
class of stop.

What the records show, and what they do not:

- Shown: the click was delivered at the breadcrumb, the folder changed, and the element found
  by the island descent under the cursor afterwards was a `Text` at depth 4 of the island host
  (64 elements visited, not truncated, not ambiguous) whose chain did not contain the
  identified RuntimeId.
- Not shown: why. The client recorded the element's control type and AutomationId but not its
  chain, so the records cannot tell the two causes apart: (a) navigating rebuilt the
  breadcrumb, so the clicked control was destroyed and created again under another RuntimeId
  (the hypothesis; the same would happen with the Calculator's pane); (b) the control is still
  there, but the `Text` is a deeper descendant than the parent that `identity_hit` reads. This
  decision records the chain from now on, so the next round can tell.

Either way, a click that rebuilds the control it clicked cannot be grounded by that control's
identity afterwards, and the OS has a better witness: the state the click was meant to change
(the folder the window shows).

## Decision

1. **Pre-click point grounding, in every flow.** Immediately before the client sends
   `execute_action`, and again before each retry after `STALE_SNAPSHOT`, it reads (read-only)
   the element under the target point, which is the centre of the identified element, with
   the same point query and island descent as the post-click check. That element must be the
   identified element (the same RuntimeId; the AutomationId only when a RuntimeId is
   unavailable) or have it as parent, and the top-level window under the point must be the
   foreground window the protocol had just required. Otherwise no request is sent: the run
   fails with the reason, nothing is spent on the ledger and nothing is retried. The read and
   its outcome are recorded as `pre_grounded` (status `ok`, `no_point`, `unreadable`,
   `window_mismatch` or `identity_mismatch`; the chain by depth, control type, AutomationId,
   class name and whether it is the identified element). A read that fails or times out is a
   refusal. The same check precedes the Notepad menu restore click.
2. **Navigating steps: decided by the OS.** A step on a closed, explicit list per flow is a
   navigating step. After its click the client still records the identity under the cursor
   (`post_grounded`, with the chain rows) but does not require it. The step's OS-side check
   (for Explorer, the folder the window shows) is required and is the step's success
   criterion. If it fails after a delivered click, the client counts `grounding_wrong` and
   stops, as it does today, and counts `engine_false_success` if the engine said
   `effect_observed`. A navigating step without an OS-side check, or one that types, is
   refused before any request.
3. **The list is closed and lives in code and in the S4 document.** `NAVIGATING_STEPS` in
   `tools/s4/e2e_driver.py` holds `{"explorer": (1, 2, 3)}`: E2E-03's two breadcrumb clicks
   and its Back click, by step number. No option, flag or configuration extends it, and the
   S4 document (`docs/thesis/S4_ACEPTACION_REDUCIDA.md`) names the same list. A step is added
   by a change to the code, to that document and to this ADR, never by a run.
4. **Nothing else is relaxed.** Every other step keeps the identity check after the click
   exactly as before (`grounding`, `grounding_wrong`, `engine_false_success`). There is no
   name comparison, no retry after a delivery, no grounding by state change for a step that
   is not on the list, and no change to the engine, its gate or the owner's stop.

## Alternatives considered

- **Accept any descendant of the identified control as the same control.** It would also
  accept a click on a different child of a composite control (a `SplitButton`'s arrow instead
  of its body). The owner chose not to weaken the identity rule.
- **Ground the navigating click by "a state change happened".** Any redraw would pass.
  The OS-side check names the exact state the click was meant to produce.
- **Drop the post-click identity check for every step.** The Notepad menu steps, the Calculator
  keys and the editor's Save button do not rebuild their control, and the check is what found
  the earlier misclicks. It stays for them.
- **Add the Calculator's «Estándar» click to the list now.** Its OS-side check (the mode read
  through UI Automation, polled) is a different design and the owner has not decided it. It
  is not on the list.
- **Use only the pre-click check and skip the post-click record.** The chain rows are the
  evidence that settles which of the two causes above holds.

## Consequences and cost

- E2E-03 can pass its pilot if the point under each breadcrumb is the breadcrumb or a direct
  child of it. If the `Text` is a grandchild of the identified control, the pre-click
  check refuses the click before any input and records the chain; that is the safe outcome
  and it settles cause (b) without a misdirected click. Widening `identity_hit` for that case
  would be a new decision.
- Each click costs two more read-only UI Automation queries (the point's centre and chain),
  each in its own PowerShell process; the cost is on the client, not on the engine, and no
  latency budget (RNF-05) is affected.
- A navigating step is only as strong as its OS-side check. A click that reached another
  control which happened to navigate to the same folder would pass; the pre-click check, the
  window checks of the engine (ADR-0020) and the closed list are what bound that.

## Required validation

- Unit tests of the clients (`tools/s4/test_e2e_driver.py`, `tools/s4/test_e2e_dossier.py`):
  a navigating step with a recreated control and a passing OS check is grounded and succeeds;
  one whose OS check fails is `grounding_wrong` and `engine_false_success` and stops with no
  further request; a step off the list with a changed identity is still `grounding_wrong`; a
  pre-click mismatch (another element, another window, an unreadable point) sends no request;
  the closed list cannot be extended from the command line; the records hold no name and no
  RuntimeId.
- A supervised E2E-03 pilot in a later round: nothing here is confirmed on a live desktop.

## Reversibility

The change is in the test clients, not in the engine or its contract. Removing the list (an
empty `NAVIGATING_STEPS`) restores the previous post-click rule for every step; the pre-click
check can be removed on its own. Either is a change to this ADR.

## Provenance

The owner's decision of 2026-10-14 («anclar antes + verificar SO»), after the tenth round's
continuation (CHANGELOG 1.0.40, review finding 8). It refines the grounding rule of the S4
fourth round and ADR-0017's rule that accessibility evidence confirms and never replaces the
visual path, and it relies on the window scope of ADR-0020. It supersedes none of them.

## Resumen (ES)

Antes de cada clic, el cliente comprueba que el elemento bajo el punto objetivo es el
identificado (mismo `RuntimeId`, o `AutomationId` sólo si falta) o su padre, y que la ventana
de nivel superior bajo el punto es la esperada; si no, no envía ninguna solicitud. Para una
lista cerrada de pasos de navegación (E2E-03: las dos migas de pan y «Atrás»), cuyo clic
reconstruye su propio control, la identidad posterior se registra pero no se exige: manda la
comprobación del sistema (la carpeta que muestra la ventana), y si falla es
`grounding_wrong` y se detiene todo. Los demás pasos conservan la regla de identidad tal cual.
La lista no se amplía por línea de comandos ni en una corrida.
