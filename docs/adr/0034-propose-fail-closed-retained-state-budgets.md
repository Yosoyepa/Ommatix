# 0034. Document the bounds of retained session state instead of budgeting it

- **Status:** Accepted (documentation only)
- **Date:** 2026-10-17
- **Approval:** The project owner, 2026-10-17. The owner chose to document the bounds and to add no budget. This ADR implements no mechanism and changes no requirement, contract, scenario or checkbox.

## Context

RNF-06 and DoD §6.7 box 7 require bounded resources and offline evidence is complementary to PERF-03, not a live soak. The [state audit](../reviews/2026-10-17-engine-state-bounds.md) found two unresolved growth paths after eliminating historical retired-ID storage and clipping failed pending tiles to the current grid.

The request journal retains delivered and indeterminate outcomes for replay safety (ADR-0007, SDD §2.9, BDD-30, SEC-02). Its actual formula is `entries <= JOURNAL_CAPACITY + effect_bearing_results_since_purge`; 1,024 only bounds the evictable part. Even `delivered = false` indeterminate outcomes remain protected. Dropping one could let a repeated request deliver again.

Repeated truncated UIA reads may accumulate old evidence on unchanged pixels. The per-read walk budget does not bound the retained union. Removing unseen records can erase password sensitivity or a confirmation that a partial read could not contradict (ADR-0017). Accessibility gaps and fused/tracked state can depend on that union.

## Decision

**Chosen by the owner on 2026-10-17: document the bounds, no budget.** The request journal and the accessibility-evidence union keep their current behaviour, as the [state audit](../reviews/2026-10-17-engine-state-bounds.md) documents it, and their growth is disclosed rather than capped:

- **Request journal.** Its entries are bounded by `1024 + D`, where `D` is the number of effect-bearing results retained since the last journal purge. Delivered and indeterminate results are never evicted, including an uncertain attempt with `delivered = false`. The bound therefore grows with the session's effect-bearing results.
- **Accessibility evidence.** The retained union has no constant bound under repeated truncated reads on unchanged pixels. Its records are not evicted, because dropping one can lose password sensitivity or a confirmation (ADR-0017). The accessibility gaps depend on the same union.

No fail-closed budget, reservation, eviction, cap, configuration key or wire error is added. A budget can be revisited by a new ADR. That ADR must define the refusal code, the counting and reservation semantics for two-phase `type` and indeterminate attempts, policy reloads, purge and session lifecycle, and whether byte limits are also needed. The two budget proposals of the first draft of this ADR are not adopted and are not implemented.

## Alternatives considered

- **Keep current retention with explicit size formulas and operator-managed session duration (chosen):** preserves current behaviour and offers no fixed session bound; the growth is disclosed instead.
- **A per-session budget of effect-bearing entries that refuses new live effects when exhausted (not chosen):** the first proposal of the first draft. It would bound retention while keeping replay safety, but it needs the lifecycle choices listed in the Decision, which the owner has not made.
- **A cap on the accessibility-evidence union, with degraded publication or a full visual refresh on overflow (not chosen):** the second proposal of the first draft. The refusal and the recovery are not defined.
- **Evict the oldest delivered or indeterminate result:** rejected, because it weakens replay safety.
- **Expire uncertain accessibility evidence by age alone:** rejected as a silent loss of safety information.
- **Persist a journal externally:** additional I/O, privacy, crash and replay semantics and a new port and adapter; storage still needs a limit.
- **Compress retired IDs or retain a bounded retired-ID cache:** unnecessary after deriving retirement from the monotonically issued prefix and the active tracks.

## Consequences and cost

Two structures have no constant bound, and their bounds are conditional on evidence counts. The journal grows with the session's effect-bearing results. The accessibility union grows with repeated truncated reads. An operator who runs long sessions with many effects or truncated reads should expect growth, which the state audit measures offline. Entry counts do not bound request strings, allocator overhead, model memory or RSS; bytes and live resources are measured separately. PERF-03 with `uia` on `1c46b87` completed on 2026-10-17 ([S0 §18](../thesis/S0_RESULTADOS.md)); its handle criterion failed, the icon cache was inactive, and box 7 remains unticked. The decision keeps the default behaviour and needs no new configuration. Its cost is that nothing stops a long session from growing: the operator ends or purges the session instead.

## Required validation

The existing regressions stay as they are: SEC-02 replay under stop, BDD-30 deduplication, the ADR-0018 phase boundary, purge, and the accessibility and password regressions. The offline `rnf_06_*` tests document the current formulas and keep their growth cases ignored; they validate no budget. A future budget ADR must add fail-first tests, traced to dossier scenarios, before it implements anything, and must repeat PERF-03 with `uia` on the chosen commit, reporting failures and memory trends.

## Reversibility

Accepting this ADR changed no code, so there is nothing to revert. A future budget would come from a new ADR. An implemented budget would require a documented operator policy and contract changes to reverse. Replay-protected records must never be discarded as a shortcut to meet a budget.

## Provenance

Architect-directed follow-up to the 2026-10-17 RNF-06 audit. The project owner chose the documentation-only option on 2026-10-17. It supersedes no ADR; ADR-0007, ADR-0017 and ADR-0018 keep their existing force.

## Resumen (ES)

Aceptada por el responsable el 17 de octubre de 2026, sólo como documentación: el diario de peticiones y la evidencia de accesibilidad retenida se mantienen sin presupuesto, y su crecimiento queda declarado en la revisión de estado retenido. Los resultados entregados o indeterminados no se expulsan nunca, para no debilitar la repetición segura. Un presupuesto podrá proponerse en un ADR nuevo; esta decisión no cambia código, contrato, requisito, escenario ni casilla.
