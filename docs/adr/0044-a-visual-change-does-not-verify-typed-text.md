# 0044. A visual change after `type` is reported delivered, never verified

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Merging the branch that carries it (`fix/type-verified-claim`) is the
  owner's acceptance; until then it is a proposal. It amends a consequence of
  [ADR-0018](0018-two-phase-type-with-element-level-focus-confirmation.md), which stays
  accepted and unedited.

## Context

[ADR-0018](0018-two-phase-type-with-element-level-focus-confirmation.md) made `type` a
two-phase action and measured its `visual_change` against a frame taken after the focusing
click, so that the click's own redraw no longer counts. Its consequences accepted a residual
limit: "`effect_observed` after `type` means 'something changed after the keys', not 'the
text was typed'". The status and the field `verified` say more than that to an agent, which
reads them as "my text is there".

The limit has been seen twice:

1. **S4, second round, finding 2** (`docs/thesis/S4_ACEPTACION_REDUCIDA.md`, «Segunda ronda»
   §5; `docs/reviews/2026-10-14-s4-uia-acceptance.md`): `visual_change` on `type` gave
   `effect_observed` with the text absent, one false success by the protocol's count. ADR-0018
   removed the cause seen then (the click's redraw) but not the limit: a caret, the first
   characters or anything else on the screen can still change after the keys. The finding
   proposed that the contract limit `type` to `value_updated` or that the engine refuse
   `visual_change` on `type`.
2. **An exploratory live session on 2026-11-26** (session I-4 of the agent-run live lab on the
   second development laptop, Windows 11 Notepad 11.2609; an engineering session run by an
   agent under the owner's delegation, **not** the dossier's supervised evidence). Asked to
   type 29 characters with `expected_effect: visual_change`, the engine answered
   `effect_observed`, `input_delivered: true`, `verified: true`; an independent UI Automation
   reader found 5 of the 29 characters in the document, then and for five seconds after. The
   engine did exactly what ADR-0018 says: within the verification window at least one tile of
   the screen differed from the reference frame. Why the characters were lost is a separate
   question, measured apart; this decision does not depend on its answer.

What the engine knows after `type` (`src/application/src/engine/act.rs`): that the keys were
handed to the system (`SendInput` inserted every event) and, when asked, whether the screen
changed within the window. It does not know how many characters arrived or which: the
accessibility provider never reads a field's value (ADR-0017, ADR-0018), and `value_updated`
already answers `indeterminate` for that reason. `verified` on the wire is not measured
separately; it is derived from the status (`ActionStatus::verified`).

## Decision

1. **A `type` whose only evidence is a visual change ends `input_delivered`, with
   `verified: false`, never `effect_observed`.** `observed_effect` keeps saying what was seen:
   `visual_change`. The status, the field and the effect are all in the published schema
   already, and the schema and the DTO accept `input_delivered` with an `observed_effect`; no
   error is attached, because nothing went wrong and nothing is to be reconciled. This holds
   on both delivery paths of `type` (two phases, ADR-0018, and the single call without
   accessibility evidence).
2. **Unchanged.** A click's or a scroll's visual change is still its verification
   (`effect_observed`). `type` with no change within the window stays `indeterminate`
   (`INDETERMINATE_EFFECT`, `reconcile_effect`), as does `type` with `value_updated`,
   `focus_changed` or `scroll_moved`. `type` with `none` stays `input_delivered` without an
   effect. Every refusal, the phases, the journal and the replay of a stored result are as
   before. The schema's shape does not change.
3. **The words.** Where an agent or a reader learns what a `type` result means, it says
   exactly: after `type`, the engine knows that the keys were handed to the system and whether
   the screen changed; it does not know how many characters arrived, or which; an agent that
   needs the text must observe again and read what the application exposes. Places: the
   `description` annotations of `status`, `input_delivered`, `verified` and `observed_effect`
   in `execute_action.output.schema.json` (annotations only: the schema accepts the same
   documents), the `execute_action` tool description, chapter 07 §7.4 and §7.5, SDD §2.9, the
   live-input guide and the architecture overview with their twins.

## Alternatives considered

- **Keep the behaviour and only add the words (F1 of audit A6 alone).** Cheapest, but the
  wire would still say `verified: true` for text that may not be there, and an agent that does
  not read descriptions would still be misled. Rejected as insufficient on its own; it is part
  of this decision.
- **Refuse `visual_change` on `type`** (round 2's proposal). It would break callers that ask
  for it today and throw away the one piece of evidence the engine has. Reporting it as
  unverified keeps the evidence and removes the claim.
- **Count the characters that arrived** (F3 of audit A6: a length read through UI Automation's
  Text or Value pattern, never the content). The only way to verify `type` for real, but it
  needs a new ADR that amends ADR-0017 (pattern calls) and says why a length is not a value
  under ADR-0012, and a spike on real applications. Not decided here: a later unit.
- **Put the `SendInput` receipt on the wire.** It would have said 58 of 58 events in the
  session above: it does not tell what the application did with them.

## Consequences and cost

- An agent loses the cheap "something changed" success for `type`. A client that relied on
  `effect_observed` (or `verified: true`) to know that its text arrived must now treat
  `input_delivered` as "the keys left, the screen changed", observe again and decide from what
  the application exposes; a client that only counted deliveries sees no difference
  (`input_delivered` stays `true`). The project's own clients score typing by an OS-side check
  (the window title or the saved file), not by the engine's status, so their verdicts do not
  depend on it; the S4 driver's comment says so.
- Supervised results recorded before this change (S4, E2E-01 and E2E-02) were reported under
  the old meaning: a typed `effect_observed` there means "something changed after the keys".
  The records are not rewritten; `S4_ACEPTACION_REDUCIDA.md` carries a note.
- `verified` and `effect_observed` mean the same thing for every action again: the declared
  postcondition was checked. For `type`, nothing the engine can check today is that
  postcondition.
- No measurable cost: the same captures as before, a different status.

## Required validation

- UT-82 (`src/application/src/engine/tests/typing.rs`): a typed visual change is
  `input_delivered`, not verified, on both paths; no change stays `indeterminate`;
  `value_updated` stays `indeterminate`; a click's visual change is still `effect_observed`;
  the journal replays the new result unchanged.
- CT-14 (`src/adapters/mcp/tests/ct_type_result.rs`): every result the engine can produce for
  `type` validates against the schema and round-trips through the DTO; `verified: true`
  without `effect_observed` is refused; the tool description and the served schema carry the
  sentence.
- BDD-73 (`docs/thesis/features/06_accion_segura.feature`), on the contract with doubles.
- In the live lab, when it next types: the same `type` reports `input_delivered` with
  `verified: false`, never `verified: true`. Engineering evidence, not the dossier's.

## Reversibility

One branch in `report_effect` (`Typed::status_of_a_visual_change`) and the words. Reverting
them restores the behaviour of ADR-0018. Reconsider when a verification that counts what
arrived exists: then `type` can earn `effect_observed` from that count, never from a visual
change.

## Provenance

Audit A6 of the agent workspace (2026-11-26), its options F1 and F2; decision D47 of the
coordinator's plan (correctness before speed). S4 round 2 finding 2. Amends the consequences
of [ADR-0018](0018-two-phase-type-with-element-level-focus-confirmation.md); keeps
[ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md) ("report what happened and no
more") and [ADR-0012](0012-data-minimisation-and-evidence-traceability.md) (no value read).
ADR numbers 0039 to 0043 are taken by proposals on branches not yet merged.

## Resumen (ES)

Tras `type`, el motor sabe que entregó las teclas al sistema y si la pantalla cambió; no sabe
cuántos caracteres llegaron ni cuáles. Por eso un `type` cuya única evidencia es un cambio
visual termina en `input_delivered` con `verified: false` y `observed_effect: visual_change`,
nunca en `effect_observed`. Los clics y desplazamientos siguen verificándose por su cambio
visual; `type` sin cambio, o con `value_updated`, sigue siendo `indeterminate`. El esquema no
cambia de forma: sólo gana descripciones, y la descripción de la herramienta y el capítulo 07
lo dicen. Motivo: en una sesión exploratoria del 26 de noviembre de 2026 el motor respondió
`verified: true` con 5 de 29 caracteres en el campo, el mismo límite que el hallazgo 2 de la
segunda ronda de S4 y que ADR-0018 aceptó como residual. Contar lo que llegó queda para otra
unidad. Es una propuesta: la acepta el responsable al integrar la rama.
