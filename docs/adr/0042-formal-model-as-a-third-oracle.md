# 0042. Keep a machine-checked model of the core algorithms as a third oracle, tied to the code by generated tests

- **Status:** Proposed
- **Date:** 2026-11-22
- **Approval:** Pending. Draft written by an AI agent at the owner's request. The model and the three
  tests exist; bringing the model into the repository and into CI is the decision.

## Context

[ADR-0001](0001-rust-runtime-and-independent-oracles.md) keeps an oracle that is independent of the
runtime: the Python reference of chapter 05. An oracle of that kind compares outputs on the cases
someone wrote. It does not say whether a general statement of chapter 04 is true.

In November 2026 the mathematical statements of the preprint's appendix A were written in Lean 4
(core library only, no Mathlib): 12 modules, 186 theorems. The work was organised around one risk:
proving a theorem that does not say what the text says. Each definition quotes the text it models,
each added hypothesis has a counterexample that shows it is needed, and a correspondence table puts
every claim next to its theorem.

Of 23 claims examined, 20 hold as written and 3 were false when read literally: the closure
proposition needed two hypotheses the engine does guarantee; the invariance under theme inversion
gave a wrong derivation; and "a request that cites an earlier observation is always refused with
`STALE_SNAPSHOT`" contradicted the order of checks of Algorithm 2. In all three the code was right
and the prose was not. The preprint was corrected (v0.7.1). The
[review of 2026-11-22](../reviews/2026-11-22-formal-model-against-the-code.md) records the findings.

A model can be right about itself and wrong about the program. The model works on rationals and on
lists; the code works on `f64` and on its own structures.

## Decision

1. **The model lives in the repository**, in `formal/`, with its correspondence table. The Lean
   release is pinned to one version, as the Rust toolchain is (REL-01), and the project depends on
   nothing but Lean's core.
2. **CI builds it and audits its axioms** in a job of its own. The audit fails on `sorry`, on any
   axiom other than the three of Lean's standard foundation, and on a warning.
3. **Every theorem the dossier or the preprint cites has a test on generated cases against the
   implementation**, named in the correspondence table. Three exist:
   `generated_closures_are_the_least_closed_region_whatever_the_order` (`incremental.rs`),
   `inversion_keeps_the_distance_to_every_prototype_on_generated_crops` (`appearance.rs`) and
   `a_reference_to_an_earlier_observation_never_reaches_the_revision_check` (`identity.rs`).
4. **A statement quoted by the model changes together with the model.** A pull request that changes
   a quoted passage of chapter 04 or 07 updates the definition or the theorem that quotes it, or
   says in the correspondence table that the passage is no longer covered.
5. **What is claimed is stated with its limits**, wherever the model is mentioned: the theorems hold
   for the model; the model is tied to the code by tests on generated cases and by reading, not by
   proof; rationals are not `f64`; the same author wrote the statements and their formalisation.
6. **The model never becomes a runtime or a build dependency of the engine**, as ADR-0001 says of
   the oracle.

## Alternatives considered

- **Leave the model in the preprint's package only:** it would drift from the code with the first
  change, and nothing would notice.
- **Verify the Rust code itself** (a verifier for Rust, or extraction from the model): a different
  scale of work, and the floating-point geometry would have to be modelled first.
- **Property tests alone, without the model:** they check the cases a generator reaches. They would
  not have found a proposition that was false for regions outside the frame, which the engine never
  produces and the text did not exclude.
- **Add Mathlib for real numbers:** a large dependency for statements that hold over the rationals.

## Consequences and cost

- A general statement about the core can be checked when it is written, and a change that breaks it
  fails a build.
- A second toolchain to pin and to keep in CI. The job is separate, so a contributor who does not
  touch `formal/` or a quoted passage does not need Lean.
- The correspondence table is one more document that a change to chapter 04 must keep current.
- The model covers what was formalised and nothing else: not the detector, not text recognition, not
  the operating system, not the agent. A proof about the model is not evidence about them.

## Required validation

- `lake build` and the axiom audit pass in CI on the pinned release.
- The three tests above pass in `cargo test --workspace`; each fails when the property it names is
  broken on purpose (checked once by mutation, recorded in the review).
- The correspondence table has no claim without a theorem, a test or an explicit "not covered".
- A comparison of the model's threshold arithmetic over rationals with the `f64` arithmetic of the
  code on the boundary cases of the fusion threshold, kept as a script with its result.

## Reversibility

`formal/` and its CI job can be removed without touching the engine. The three tests are ordinary
unit tests and stay. A disagreement between a theorem and the code is a finding to review, not a
reason to remove the model.

## Provenance

The formalisation of the preprint's appendix A (21 and 22 November 2026) and the review of
2026-11-22. Refines ADR-0001.

## Resumen (ES)

El modelo en Lean de los algoritmos del núcleo (186 teoremas, sólo la biblioteca base) pasa al
repositorio como tercer oráculo, con versión fijada y una tarea de CI propia que lo compila y audita
sus axiomas. Cada teorema citado tiene una prueba sobre casos generados contra el código real; hoy
hay tres. Lo que se afirma lleva siempre sus límites: los teoremas valen para el modelo, y el modelo
se ata al código con pruebas y lectura, no con una demostración.
