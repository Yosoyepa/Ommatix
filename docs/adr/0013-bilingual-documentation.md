# 0013. Bilingual documentation (EN/ES)

- **Status:** Accepted
- **Date:** 2026-10-14
- **Approval:** Project owner, at the repository bootstrap

## Context

The thesis is written and defended in Spanish, and its dossier ([`docs/thesis/`](../thesis/README.md))
is in Spanish. The code, a future paper and the open-source audience read English. When the
repository was bootstrapped, the project owner asked for English and Spanish.

## Decision

| Artefact | Language |
|---|---|
| Code, identifiers, code comments, commit messages, CLI output, MCP tool text | English |
| Thesis dossier (`docs/thesis/`): chapters, contracts, matrices, Gherkin scenarios | Spanish, as the academic source text |
| `README.md`, `docs/architecture/overview.md` | English, with a Spanish twin `*.es.md` |
| ADRs (`docs/adr/`) | English body and a mandatory `## Resumen (ES)` section; ADRs taken from the dossier keep their original Spanish text there |
| Glossary | One bilingual table |
| Issues and PR descriptions | Either language |

Gherkin scenarios keep the language they were written in. The thesis scenarios use
`# language: es` and stay in Spanish when they become executable; their `@BDD`, `@RF` and
`@UT` tags keep them traceable.

A PR that changes an English document with a `*.es.md` twin changes the twin too.

## Alternatives considered

- **Both languages inline in every file:** noisy to read and to diff.
- **Spanish only:** limits reuse and the paper's audience.
- **Translate the whole dossier now:** doubles the maintenance of a source text that is still
  changing; an English edition can come later.

## Consequences and cost

Evaluators can read everything in Spanish, and the code stays idiomatic. Readers without
Spanish rely on the English overview and ADRs for the thesis content. Twin files can drift;
the PR template and AGENTS.md carry the rule.

## Required validation

Review: every PR that edits a paired document edits both files.

## Reversibility

An English edition of the chapters can be added next to the Spanish source without changing
it.

## Provenance

The project owner's request at the bootstrap. Supersedes bootstrap
[B0007](bootstrap/B0007-bilingual-documentation.md).

## Resumen (ES)

Código, comentarios, commits y textos de la CLI y de MCP en inglés. El dossier de la tesis
(`docs/thesis/`) se mantiene en español como texto académico fuente, incluidos sus escenarios
Gherkin. README y visión de arquitectura en inglés con un gemelo `.es.md`, que se actualiza en
el mismo PR. Cada ADR en inglés con una sección `Resumen (ES)`; las ADR de la tesis guardan
ahí su texto original.
