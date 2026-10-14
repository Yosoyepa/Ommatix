# B0007. Bilingual (EN/ES) documentation

- **Status:** Superseded by [ADR-0013](../0013-bilingual-documentation.md) on 2026-10-14
- **Previous status:** Accepted
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

The project is developed and defended in a Spanish-speaking academic context, while the
code, the paper and the open-source audience are English-speaking.

## Decision

- Code, identifiers, comments, commits, CLI and MCP text: **English**.
- User-facing docs (`README`, architecture overview): **paired files**, `name.md` (English,
  canonical) and `name.es.md` (Spanish), each linking to the other.
- ADRs: English body + mandatory `## Resumen (ES)`.
- Glossary: single bilingual table.
- A PR that edits a paired doc updates both files.

## Consequences

- Evaluators can read everything relevant in Spanish; the codebase stays idiomatic.
- Twin files can drift; mitigated by the PR template checkbox and AGENTS.md rule.

## Alternatives considered

- **Everything in both languages inline** — noisy to read and to diff.
- **Spanish only** — limits reuse and the paper audience.

## Resumen (ES)

Código y mensajes en inglés. README y visión de arquitectura en archivos pares `.md`
(inglés) y `.es.md` (español). Cada ADR en inglés con una sección obligatoria de resumen en
español. Todo PR que toque un documento par actualiza ambos.
