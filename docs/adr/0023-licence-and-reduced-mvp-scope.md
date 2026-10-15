# 0023. Licence the code and the documents, and close the MVP with an explicit reduced scope

- **Status:** Accepted
- **Date:** 2026-10-15
- **Approval:** The project owner, 2026-10-15 (decisions recorded in this PR)

## Context

The repository has been `UNLICENSED` since it was bootstrapped, because the owner had not
chosen a licence ([`S4_REPRODUCIBILIDAD.md`](../thesis/S4_REPRODUCIBILIDAD.md) §3 and §6.5).

The MVP's acceptance protocol asks for 450 end-to-end runs (chapter 03 §3.5, EV-02), an
annotated corpus of at least 300 captures (EV-01), a DPI sweep at 100, 125 and 150 % (IT-07)
and a frozen corpus for reproducibility (REL-01). By 15 October 2026 the supervised rounds
had been capped at 200 real requests each, and the reduced protocol had reached ten of ten
for four of its five flows. E2E-03 had three of ten scored runs on `main`
([`S4_ACEPTACION_REDUCIDA.md`](../thesis/S4_ACEPTACION_REDUCIDA.md), eleventh round). RNF-05
misses its two-region target by about 3 ms (p95 153 ms against 150 ms; one region and the full
refresh meet theirs; [`S0_RESULTADOS.md`](../thesis/S0_RESULTADOS.md) §10.6). PERF-03, IT-07
and EV-01 have not run.

Chapter 06 §6.9 asks that every important change record its context, alternatives,
consequences and reversal criterion in an ADR, and chapter 03 §3.6 asks that a missed target
be shown with an explicit scope decision.

## Decision

1. **Licence of the code:** `MIT OR Apache-2.0` for every crate, the test applications in
   `tools/s4` and the tools (`LICENSE-MIT`, `LICENSE-APACHE`; `license` in each manifest).
   **Licence of the documents:** CC BY 4.0 for the thesis dossier, the rest of `docs/`, the
   READMEs and the thesis paper where it is distributed with this repository (`LICENSE-DOCS`).
   Copyright 2026 Juan Carlos Andrade Unigarro. Third-party components keep their own terms
   and are never committed (listed in [`S4_REPRODUCIBILIDAD.md`](../thesis/S4_REPRODUCIBILIDAD.md) §3).
2. **The MVP closes with a reduced and explicit scope**, recorded in chapter 06 §6.7:
   - RNF-05 is accepted as a documented near-miss for this MVP. It is not marked as met.
   - For this MVP the reduced S4 protocol (E2E-01, E2E-02, E2E-03, S4R-02 and S4R-03, each ten
     of ten with no false success) replaces the 450 runs of EV-02.
   - EV-01, EV-02, IT-07 and the frozen corpus of REL-01 move to the next cut, with their reasons.
   - The next cut follows the plan of the Paper (INC-00 to INC-09), which is not copied here.
3. **Checkboxes of chapter 06 §6.7 are ticked only with evidence,** written beside each box.
   This decision does not change any requirement, algorithm, contract, scenario or matrix
   status.

## Alternatives considered

- **Keep the 450-run protocol for the MVP.** Not chosen. It needs several supervised sessions
  with the owner present, and at 200 real requests per round it would keep the MVP open.
- **Meet the two-region RNF-05 target now** by removing 3 to 5 ms from the per-region inference.
  Not done here. It changes the perception path and needs its own measurement, so it belongs to
  the next cut.
- **Other licences** (copyleft families, proprietary terms). Not chosen by the owner. The
  dependency graph is permissive (`S4_REPRODUCIBILIDAD.md` §3), and `MIT OR Apache-2.0` is the
  usual dual licence of the Rust ecosystem.

## Consequences and cost

- The code and the documents can be reused under the licences above, with attribution for the
  documents.
- A dependency with a copyleft licence now fails `rel_01_every_dependency_has_a_licence_and_none_is_copyleft`
  until the licence list is updated with a decision.
- The MVP closes with a documented miss (RNF-05) and with open items. The next cut carries
  EV-01, EV-02, IT-07 and the REL-01 corpus.
- E2E-03 must still reach ten of ten with no false success before any box that depends on it
  can be ticked.

## Required validation

- `rel_01_the_repository_declares_its_licences` and `rel_01_every_dependency_has_a_licence_and_none_is_copyleft`
  (`src/acceptance/tests/repository.rs`), and the helper tests in `tools/repro`.
- `validar_paquete.py` (DOC-01: every requirement has an owner, a test and evidence).
- The evidence beside each box of chapter 06 §6.7, re-checked against `main` before any box
  changes.

## Reversibility

- **Licence:** a different licence needs a new owner decision, a new CHANGELOG entry, new
  `LICENSE-*` files and new `license` fields. The copyright holder stays the same.
- **Scope:** the 450-run protocol returns by reopening EV-02 and the boxes that depend on it.
  That is a change to chapter 06 §6.7 and to the CHANGELOG, with a new ADR.

## Provenance

Owner decisions of 2026-10-15 (licence and scope). It does not supersede any ADR. It applies
ADR-0010 (budgets are targets until they are measured on the reference hardware) and ADR-0012
(captures and data are not committed).

## Resumen (ES)

El código queda bajo MIT OR Apache-2.0 y los documentos bajo CC BY 4.0, por decisión del
responsable del 15 de octubre de 2026. El MVP se cierra con un alcance reducido y explícito:
RNF-05 se acepta como incumplimiento documentado (dos regiones a 153 ms frente a 150 ms), el
protocolo reducido de S4 sustituye las 450 corridas de EV-02 para este MVP, y EV-01, EV-02,
IT-07 y el corpus de REL-01 pasan al próximo corte. Las casillas de la Definition of Done
se marcan sólo con evidencia, y esta decisión no cambia requisitos, algoritmos, contratos,
escenarios ni estados de las matrices.
