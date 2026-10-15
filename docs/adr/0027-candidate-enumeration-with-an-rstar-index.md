# 0027. Enumerate the candidate controls of Algorithm 2 with an R*-tree, behind a switch that stays off

- **Status:** Accepted
- **Date:** 2026-10-15
- **Approval:** Project owner, 2026-10-15, pre-approved in the night plan for INC-05 on the condition that the change follows the next-cut plan exactly. Adoption of the index as the default is not decided by this ADR.

## Context

[ADR-0003](0003-conservative-spatial-fusion-with-ioc.md) keeps "the R*-tree index" among the decisions of conservative fusion, but the runtime never had one: `fuse` scans every control for every word and for every corridor (`owner_of` and `choose_external_label`). The S0 report measures the fusion step at a median of about 1.2 ms per observation on a synthetic screen of 300 controls and 600 text spans, and notes that it is the stage that grows fastest with denser screens ([S0_RESULTADOS.md](../thesis/S0_RESULTADOS.md) §11). Whether an index pays off, and at which scale, has not been measured. The next-cut plan (INC-05) asks for a reversible experiment that proves equivalence before it looks at cost. The structure and its query semantics come from the R-tree and R*-tree literature and from the `rstar` documentation [S17–S19].

## Decision

1. `ommatix-domain` gains a `candidates` module with `CandidateSearch::{Scan, RStar}` and `ControlSearch`. A `ControlSearch` is built once per fusion over the reconciled controls, the same slice the fusion reads, so a position always refers to the evidence it was built from. `Scan` is the reference and the default; it builds nothing.
2. The owners of a word, at an IoC threshold above zero, are enumerated with the word's own box as a closed window. The corridor obstacles are enumerated with the corridor gap as a closed window. Every candidate then meets the reference's exact predicates, unchanged: IoC at least the threshold; positive-area overlap; the exclusion of the labelled control and of the controls that contain it; the order by area and then position; the abstention on near-equal areas; the stable order of the label candidates.
3. Completeness is claimed only where it holds. At a threshold above zero an owner shares positive area with the word, so it meets the word's closed window. At a threshold of zero every control qualifies, so the index visits every position: the fallback is the scan. Reconciliation with accessibility evidence keeps the scan.
4. Unchanged: words are not indexed and keep their source order; `halo_close` (A1) is untouched; there is no top-k truncation; ties, costs, roles, normalisation, affordances, window limits and `f64` coordinates keep their meaning.
5. The switch is `[engine] candidate_search = "scan" | "rstar"`, `scan` by default. It changes how candidates are found, never the result. Nothing is persisted: the index is dropped when its fusion returns.
6. Dependency: `rstar = "=0.13.0"` with its default features (its optional `serde` and `mint` features stay off). It is MIT OR Apache-2.0 and needs Rust 1.85; the workspace needs 1.88. It joins the domain row of the dependency table in [AGENTS.md](../../AGENTS.md) §3 and the hexagon test. Its direct dependencies are heapless, num-traits and smallvec; the transitive ones are hash32, byteorder, stable_deref_trait and libm. `rstar` itself forbids `unsafe`; smallvec, a transitive dependency, contains `unsafe` code under its own review, and this change adds none. The next-cut paper inspected 0.12.2; 0.13.0 was the latest release when this was decided, and the functions the code uses (bulk loading, the closed-window intersection search, `AABB::from_corners`) are the same.

## Alternatives considered

- **Keep the scan only.** It remains the reference and the default. Used alone it gives no information about scale, so the experiment would not be run.
- **Uniform grid.** Simple for regular layouts, but a box that crosses many cells costs in both the build and the query, and a second structure before measuring the first one multiplies the work the plan forbids.
- **A static packed index written here.** No dependency, but it reimplements the packing and the query the library already provides. Deferred until a measurement shows the structure matters.
- **An R*-tree built by insertion.** Not needed: the boxes do not change within a fusion, and bulk loading is deterministic and costs O(N log N).
- **An index over the words as well.** Out of scope for this change. The owner window is the word's own box, so no derived window is needed; indexing the words for labels needs conservative windows with their own proof, and A1 needs its own proof about rounds and budgets.

## Consequences and cost

- A new direct dependency and six transitive crates; an index built per fusion (O(N log N) time, O(N) memory); a larger test surface (IDX-01 to IDX-14 and the generated families).
- Equivalence is argued and tested on the protocol's families up to 8192 controls and through the engine. This is evidence on the inputs tried, not a proof that no difference exists: one reproducible counterexample blocks adoption.
- The benefit may be limited to large loads with selective queries, and it may not reach the latency of an observation. The measured decision is recorded in [INDICE_A2_RESULTADOS.md](../thesis/INDICE_A2_RESULTADOS.md).
- The containment match of accessibility reconciliation keeps its quadratic scan. The plan leaves it out.

## Required validation

- IDX-01 to IDX-08, IDX-13 and IDX-14 in the domain (`src/domain/src/candidates.rs`, `src/domain/src/fusion.rs`). Each check has a mutant that drops it and answers differently on the same fixture. Generated families: `src/domain/tests/candidate_index.rs` (IDX-01, IDX-02 and IDX-14).
- IDX-09, IDX-10 and IDX-14 through the engine (`src/application/src/engine/tests/candidates.rs`). IDX-11 and IDX-12 are guard tests of the halo closure, which A1 keeps (`src/domain/src/incremental.rs`).
- BDD-41 to BDD-52 (`docs/thesis/features/08_indice_candidatos.feature`), traced to RF-03, RF-04, RF-06 and RF-10.
- The release benchmark `src/acceptance/examples/candidate_index.rs`: zero differences in G1 and G2 on every family and size, and G4 and G5 measured with intervals.

## Reversibility

`candidate_search = "scan"`, the default, restores the reference path with no other change. Removing the `candidates` module and the dependency restores the previous code. A reproducible equivalence counterexample, a G4 failure in every target family, or a G5 regression on small loads means the switch stays off or the change is removed.

## Provenance

This ADR refines [ADR-0003](0003-conservative-spatial-fusion-with-ioc.md), which keeps the R*-tree index as a decision the runtime had not yet implemented; it does not supersede it. Its source is the next-cut plan (INC-05), approved by the owner on 2026-10-15. The sources are in the [catalogue](../thesis/fuentes/referencias.json).

## Resumen (ES)

- **Contexto:** la fusión recorre todos los controles para cada palabra y cada corredor; el índice R*-tree previsto en ADR-0003 nunca llegó al motor.
- **Decisión:** un enumerador experimental de candidatos (`[engine] candidate_search = "rstar"`), desactivado por defecto, que sólo acelera la búsqueda de propietarios y de obstáculos y conserva los predicados, el orden y las abstenciones del recorrido, con la dependencia `rstar` fijada en 0.13.0. La adopción por defecto no se decide aquí.
