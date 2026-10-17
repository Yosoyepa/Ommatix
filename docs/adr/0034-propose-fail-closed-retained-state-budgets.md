# 0034. Propose fail-closed budgets for retained session state

- **Status:** Proposed
- **Date:** 2026-10-17
- **Approval:** Pending owner decision; no budget is implemented by this ADR

## Context

RNF-06 and DoD §6.7 box 7 require bounded resources and offline evidence is complementary to PERF-03, not a live soak. The [state audit](../reviews/2026-10-17-engine-state-bounds.md) found two unresolved growth paths after eliminating historical retired-ID storage and clipping failed pending tiles to the current grid.

The request journal retains delivered and indeterminate outcomes for replay safety (ADR-0007, SDD §2.9, BDD-30, SEC-02). Its actual formula is `entries <= JOURNAL_CAPACITY + effect_bearing_results_since_purge`; 1,024 only bounds the evictable part. Even `delivered = false` indeterminate outcomes remain protected. Dropping one could let a repeated request deliver again.

Repeated truncated UIA reads may accumulate old evidence on unchanged pixels. The per-read walk budget does not bound the retained union. Removing unseen records can erase password sensitivity or a confirmation that a partial read could not contradict (ADR-0017). Accessibility gaps and fused/tracked state can depend on that union.

## Decision

**Proposal only [P]:** consider an operator-configured, optional per-session budget for effect-bearing journal entries. Reserve capacity at every actual input boundary; once exhausted, refuse new live effects until a new session, while serving protected replays. Never evict effect-bearing results. The owner must define the refusal code, counting/reservation semantics for two-phase type and indeterminate attempts, policy reloads, purge and session lifecycle, and whether byte limits are also needed. Purge must not silently reset an exhausted session's authorization budget and permit request-ID replay. No new constant, configuration key, wire error or runtime behaviour is introduced here.

**Second open proposal [P]:** bound retained accessibility evidence (including dependent gaps/elements), and on overflow fail closed with degraded publication and/or a full visual refresh. Never silently evict safety evidence or turn a truncated read into completeness. The owner must decide how failure preserves password sensitivity, how refresh is budgeted and how persistent overflow recovers. No mechanism is implemented.

## Alternatives considered

- Keep current retention with explicit size formulas and operator-managed session duration: preserves behaviour but offers no fixed session bound.
- Evict the oldest delivered/indeterminate result: rejected because it weakens replay safety.
- Expire uncertain accessibility evidence by age alone: rejected as a silent loss of safety information.
- Persist a journal externally: additional I/O, privacy, crash/replay semantics and a new port/adapter; storage still needs a limit.
- Compress retired IDs or retain a bounded retired-ID cache: unnecessary after deriving retirement from the monotonically issued prefix and active tracks.

## Consequences and cost

Budgets could bound session retention while preserving stored replay outcomes. They can reduce session availability and require explicit operator lifecycle choices, reservations for two input phases and recovery from persistent partial trees. Entry counts do not bound request strings, allocator overhead, model memory or RSS; bytes and live resources must be measured separately. Current behaviour remains unchanged pending acceptance.

## Required validation

Keep existing SEC-02 replay-under-stop, BDD-30 deduplication, ADR-0018 phase-boundary, purge and accessibility/password regressions. Add fail-first threshold/reservation/replay/purge/session tests only after the policy is decided; trace any new visible contract behaviour to dossier scenarios and existing or owner-approved planned IDs. Repeat PERF-03 with UIA on the chosen commit and report failures and memory trends. The offline `rnf_06_*` tests expose current formulas and ignored growth cases; they do not validate a proposed policy.

## Reversibility

Before acceptance, withdraw this proposal without a code change. An implemented budget would require a new documented operator policy and contract changes to reverse; replay-protected records must never be discarded as a shortcut.

## Provenance

Architect-directed follow-up to the 2026-10-17 RNF-06 audit; refines no accepted ADR until the owner decides. ADR-0007, ADR-0017 and ADR-0018 retain their existing force.

## Resumen (ES)

Propuesta pendiente: un presupuesto opcional por sesión denegaría efectos nuevos al agotarse, sin expulsar resultados entregados o indeterminados ni debilitar la repetición segura. Una segunda decisión pendiente acotaría la evidencia de accesibilidad retenida con degradación segura o refresco visual completo, sin borrar silenciosamente información sensible. El responsable debe definir los límites y la recuperación; no se implementa ningún mecanismo ni se cambia un requisito, contrato, escenario o estado.
