# 0025. Bound every authorisation by the age of the snapshot and of the accessibility evidence, enforced at every real-input boundary

- **Status:** Accepted (2026-10-15)
- **Date:** 2026-10-15
- **Approval:** Accepted by the project owner on 2026-10-15, under the pre-approval given for INC-03 of the next cut, provided the plan is followed exactly: the window is enforced, its default is wider than the dossier's 250 ms, and no safeguard is relaxed

## Context

The action gate checks session, snapshot, revision, policy, window and focus, and before
delivery it captures the screen again and requires it to equal the committed base (SDD §2.9,
RF-11, ADR-0006, ADR-0007). The dossier template sets `max_snapshot_age_ms = 250`, but the
engine only accepted that value: a startup note said it was not enforced, the README said so,
and the paper audit (finding R06) recorded that the gate implemented no time-to-live.

The accessibility evidence (ADR-0017) has a second, unbounded age. An observation reads the
accessibility tree only in the regions the plan re-analyses, plus the gaps of an earlier
truncated read (ADR-0019 and the region plan). An observation that changes no pixel keeps the
evidence it already holds, so a UI Automation answer can outlive the state it described, and a
later publication presents it as if it were new (audit finding R08). Taking only the snapshot's
capture instant is therefore not enough.

LLM agents act seconds after they observe. With 250 ms, almost every action arrives stale.
The freshness capture before each delivery protects the screen whatever the window is, so the
window bounds how long an observation may be relied on, not whether a changed screen can be
acted on.

## Decision

1. **Two instants bound an authorisation.** Both are read from the injected `Clock`, as every
   time in the engine is (AGENTS.md §3).
   - The **capture instant** of the published snapshot (`captured_at`, stamped by the capturer
     on the engine's clock). The snapshot's age is `now − captured_at`.
   - For a target that accessibility evidence confirms (ADR-0017), the **start instant of the
     read** that reported that evidence (`read_at`). The evidence's age is `now − read_at`. A
     read that starts at `read_at` reports the state of an instant no earlier than `read_at`,
     so measuring from the start never understates the age.

   An action is authorised only when both ages are at most `[engine] max_snapshot_age_ms`. The
   bound is inclusive: an age of exactly the window is still valid. A target that no
   accessibility read confirms has no evidence age.
2. **A publication never renews a read.** Evidence keeps its `read_at` when a new publication
   reuses it. An observation reads again the regions whose evidence has expired, whether or not
   pixels changed there, as it already does for the gaps of a truncated read. So a static screen
   can be acted on again after a new observation, and a semantic change with no repaint is seen
   once the window has ended.
3. **Enforced at every real-input boundary.**
   - At the gate, before any capture. Simulated actions meet the same check and get the same
     refusal, so a simulated answer never describes an action the live path would refuse.
     Nothing is sent in simulation, so there is no boundary after the gate.
   - After the freshness capture and before the single click or the focusing click.
   - For `type` in two phases (ADR-0018), after the focusing click and before the keys, so the
     window can end while keyboard focus is awaited.

   The keyboard-focus read is not a bound of its own: the verification window already limits
   how long it may take.
4. **An expired bound is `STALE_SNAPSHOT`:** `observe_again`, not retryable, with the target
   never re-targeted by proximity (chapter 07 §7.7). No new error code is added and no schema
   changes. Before a boundary nothing is sent. If the window ends after the focusing click, the
   result is `indeterminate` with `INDETERMINATE_EFFECT`, `input_delivered: true`, no key and
   `reconcile_effect`, which is the existing rule for a focus that is not confirmed in time
   (ADR-0018).
5. **Default `max_snapshot_age_ms = 10000`**, in the dossier template and in the code
   (`EngineConfig::DEFAULT_MAX_SNAPSHOT_AGE`). The 250 ms of the first draft was never
   measured for agents that act after observing. The value remains the operator's to change:
   any value of at least 1 ms is accepted.
6. **One setting bounds both instants.** A second setting would be a new configuration decision
   without evidence to support it.

## Alternatives considered

- **Bound only the capture instant.** Rejected: a reused accessibility answer could be
  arbitrarily old while the snapshot is fresh, and the freshness capture compares pixels,
  which cannot show that an element became disabled without a repaint.
- **Bound all evidence by the oldest read in the snapshot.** Rejected: one stale region would
  stop every action on the screen, and on a static screen no observation could renew it without
  the re-read this decision adds.
- **A new error code for expired evidence.** Rejected: `STALE_SNAPSHOT` already means "observe
  again, do not re-target" (chapter 07 §7.7), and a new code would change the contract for no
  gain.
- **No bound in simulation.** Rejected: a simulated `ok` would describe an action the live
  path refuses. Simulation sends nothing, so it has no later boundary.
- **Keep 250 ms.** Rejected by the owner on 2026-10-15: at that value an agent that acts after
  observing is refused almost always, and the freshness capture, not the window, is what
  protects the screen.

## Consequences and cost

- Agents observe again within the window. Each observation after an expiry re-reads the
  regions whose evidence has expired, bounded by the accessibility budget of ADR-0017. On a
  static screen that is a full read of the accessible regions after every window. The cost
  is not measured on the reference machine yet, and the window is not calibrated until it is.
- The gate scans the published elements once per authorising check; that is linear in the
  number of elements and small beside the freshness capture.
- Residual risk: a semantic change inside the window with no repaint is not seen until the
  window ends or the next observation reads the region again. This decision bounds that
  exposure; it does not remove it (audit finding R08 remains open for the semantic channel).
- A UI Automation read is not an atomic snapshot. A read that returns after the window has
  spent it from its start (test UT-51).

## Required validation

- UT-46 to UT-54 in `src/application/src/engine/tests/snapshot_age.rs`: the threshold before,
  at and after the bound (dry-run and live); simulation refuses an expired snapshot as the live
  path does; expiry between validation and delivery for the snapshot and for the evidence;
  a new publication does not renew a read, and observing again does; a late read spends the
  window from its capture; expiry between the focusing click and the keys sends no key; a
  semantic change without repaint is refused and then seen; the default window is ten seconds.
  Removing the check at the gate, the check before the click, the check before the keys, or the
  re-read of expired evidence each fails the tests that cover it.
- BDD-38 (`docs/thesis/features/06_accion_segura.feature`): `STALE_SNAPSHOT` with
  `observe_again` through the MCP contract, and a new observation allows the action.
- `the_snapshot_window_is_ten_seconds_unless_the_operator_sets_it` and the template test that
  compares `docs/thesis/plantillas/configuracion_mvp.toml` with the defaults.
- A measurement of the observation with expired evidence on the reference machine, before any
  change to the default. Not done in this change.

## Reversibility

The window itself is a setting: a larger value relaxes it without a code change. Removing the
check, or the re-read of expired evidence, requires a new decision. The re-read is only
meaningful together with the check: without the check nothing expires, and with the check but
without the re-read a static screen could never be acted on again.

## Provenance

The next-cut plan of the paper (INC-03: "vigencia de snapshot y de cada fuente") and its
audit findings R06 (no time-to-live in the gate) and R08 (accessibility evidence not refreshed
without regions or gaps). It refines ADR-0006 (transactional snapshots), ADR-0007 (fail-closed
actuation), ADR-0017 (accessibility evidence) and ADR-0018 (two-phase `type`). It supersedes
none of them.

## Resumen (ES)

Cada autorización de una acción se acota por dos instantes medidos en el reloj inyectado: la
captura del snapshot publicado y el inicio de la lectura de accesibilidad que confirma el
objetivo. Si uno de los dos supera `[engine] max_snapshot_age_ms`, por defecto 10 000 ms, la
acción se rechaza con `STALE_SNAPSHOT` (observar de nuevo, sin reanclar): en la puerta, también
en simulación, y justo antes de cada entrada real, incluido el intervalo entre el clic de enfoque
y las teclas de `type`. Una publicación nueva no renueva una lectura de accesibilidad antigua; la
siguiente observación relee sólo las regiones cuya evidencia ha caducado. Se elige 10 000 ms y
no 250 ms porque un agente actúa segundos después de observar, y la captura de frescura antes de
cada entrega sigue protegiendo la pantalla.
