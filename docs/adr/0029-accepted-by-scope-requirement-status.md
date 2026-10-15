# 0029. Record a requirement accepted by explicit scope change with the status ACEPTADO_POR_ALCANCE

- **Status:** Accepted
- **Date:** 2026-10-15
- **Approval:** The project owner, 2026-10-15: chapter 06 §6.7, decision 8, approved for RNF-02, RNF-03 and RNF-07 only

## Context

Chapter 06 §6.7 closes the MVP with an explicit reduced scope ([ADR-0023](0023-licence-and-reduced-mvp-scope.md)).
Its scope block (a) to (c) accepts RNF-05 as a documented miss and moves EV-01, EV-02, IT-07 and
the frozen corpus of REL-01 to the next cut, each with its reason. The requirement matrix
([`requisitos_trazabilidad.csv`](../thesis/matrices/requisitos_trazabilidad.csv)) gave every
requirement one status, `PENDIENTE_IMPLEMENTACION_O_VALIDACION`, and both
`validar_paquete.py` (`doc01_estado_de_requisitos`) and
`src/acceptance/tests/repository.rs` (`doc_01_every_status_is_from_the_vocabulary_of_the_matrices`)
accepted only that value. The matrix therefore could not tell a requirement the owner accepted by
scope from one that nobody has looked at. Decision 8 of §6.7 asked the owner whether that
acceptance should have a status of its own. Chapter 03 (RNF-10) requires that changes to the
traceability of requirements go through an ADR.

## Decision

1. **A new status.** `ACEPTADO_POR_ALCANCE` joins the vocabulary of requirement statuses. It means
   that the owner accepted the requirement as not met for this MVP, by an explicit scope change
   recorded in chapter 06 §6.7. It does not mean that the requirement is met, and it changes no
   planned test (`plan_pruebas.csv`) and no scenario (`gherkin_trazabilidad.csv`).
2. **A scope note is required.** A requirement with this status cites, in its `evidencia`
   column, either the scope block of chapter 06 (`docs/thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md`
   with §6.7) or a CHANGELOG entry written `CHANGELOG 1.0.N` that exists in
   `docs/thesis/CHANGELOG.md`. `validar_paquete.py` checks it (`doc01_nota_de_alcance:<id>`), and so
   does `doc_01_a_requirement_accepted_by_scope_cites_its_scope_note`. The rule runs one way: a row
   that cites a note but keeps the pending status is not refused.
3. **Three requirements carry the status:** RNF-02 (EV-01), RNF-03 (EV-02) and RNF-07 (IT-07), the
   ones whose acceptance the owner made explicit. RNF-05 keeps the pending status, because its
   acceptance is under §6.7 (a) and INC-07 and its re-evaluation are still in progress. RNF-08 keeps
   it, because the installation on a clean machine (REL-01, session A) is still planned with the
   owner. The test `doc_01_a_requirement_accepted_by_scope_cites_its_scope_note` fixes this set: a
   new acceptance is a deliberate edit of that test, of chapter 06 and of the CHANGELOG.
4. **No checkbox changes.** The status records an acceptance; it ticks no box of chapter 06 §6.7.
   The box that asks for every non-compliance to be resolved or accepted is judged on its own
   evidence.

## Alternatives considered

- **Keep one status and record the scope in chapter 06 only.** Not chosen. The matrix would still
  say `PENDIENTE` for requirements the owner has accepted, and a reader of the matrix alone would
  not see the decision.
- **Mark the three requirements as met.** Not chosen. EV-01, EV-02 and IT-07 have not run, and
  §6.7 and ADR-0023 set a mark only with evidence.
- **Use a planned-test status (`PENDIENTE_PRODUCTO`) for the requirement.** Not chosen. Plan
  statuses describe tests. The requirement is what is accepted, and its tests keep their own state.
- **Free text in the status column.** Not chosen. The vocabulary is what DOC-01 checks, and a free
  value would pass every check.

## Consequences and cost

- The matrix separates requirements the owner accepted by scope from those still open, and each
  acceptance points to its note.
- Two validators keep the same vocabulary and the same rule. A further status needs an ADR, a
  CHANGELOG entry and an edit of both lists.
- The note check is textual. It verifies that the note cites the scope block or an existing
  CHANGELOG entry, not that the scope decision is sound; that judgement stays with the owner.
- RNF-02, RNF-03 and RNF-07 read as accepted, not as met, wherever the matrix is summarised.

## Required validation

- `doc_01_every_status_is_from_the_vocabulary_of_the_matrices`,
  `doc_01_a_requirement_accepted_by_scope_cites_its_scope_note` and
  `doc_01_the_scope_rule_refuses_an_acceptance_without_a_note` in `src/acceptance/tests/repository.rs`.
- `validar_paquete.py`: `doc01_estado_de_requisitos` and `doc01_nota_de_alcance:RNF-02`,
  `doc01_nota_de_alcance:RNF-03` and `doc01_nota_de_alcance:RNF-07` (DOC-01).

## Reversibility

- **The status is withdrawn** by an owner decision. The three rows go back to
  `PENDIENTE_IMPLEMENTACION_O_VALIDACION` in the same change as chapter 06 and the CHANGELOG, and
  both lists shrink. A later ADR supersedes this one; this one is not edited.
- **It lapses on its own** once EV-01, EV-02 or IT-07 has run and been scored. The requirement is
  then met or still open, never accepted by scope.

## Provenance

The owner's approval of 2026-10-15 for the status (chapter 06 §6.7, decision 8), applied to the
scope block (c) of the same section, which [ADR-0023](0023-licence-and-reduced-mvp-scope.md)
recorded. It supersedes no ADR. It refines decision 8 of §6.7.

## Resumen (ES)

La matriz de requisitos gana el estado `ACEPTADO_POR_ALCANCE`, aprobado por el responsable el 15 de
octubre de 2026 para RNF-02 (EV-01), RNF-03 (EV-02) y RNF-07 (IT-07), los requisitos que aceptó por
cambio de alcance explícito en el capítulo 06 §6.7. Significa que el requisito no se cumple y que
su aceptación consta en el alcance, no que esté cumplido. Toda fila con ese estado debe citar su
nota de alcance (el apartado §6.7 o una entrada de CHANGELOG que exista), y el validador y la
prueba de DOC-01 lo comprueban. RNF-05 y RNF-08 conservan su estado, y ninguna casilla de §6.7
cambia por esta decisión.
