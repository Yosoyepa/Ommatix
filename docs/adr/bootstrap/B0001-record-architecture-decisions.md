# B0001. Record architecture decisions

- **Status:** Superseded by the [ADR index](../README.md) and its template on 2026-10-14
- **Previous status:** Accepted
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

Ommatix is a research-grade system that will be defended in front of evaluators and extended
by both humans and AI agents. Decisions made during the bootstrap must stay visible and
justified, otherwise they get silently reversed.

## Decision

Every significant architectural decision is recorded as an ADR in `docs/adr/`, numbered
sequentially, using the template in `0000-template.md`. ADRs are immutable once accepted;
changing a decision means a new ADR that supersedes the old one.

## Consequences

- Reviewers and agents have one place to check before proposing structural changes.
- The thesis/paper can cite ADRs directly as design rationale.
- Small overhead per decision; mitigated by a short template.

## Resumen (ES)

Toda decisión arquitectónica relevante se documenta como ADR numerado en `docs/adr/`. Un ADR
aceptado no se edita: se reemplaza con uno nuevo. Esto da trazabilidad para la sustentación
y evita que personas o agentes reviertan decisiones sin justificar.
