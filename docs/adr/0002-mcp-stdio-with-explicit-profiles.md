# 0002. MCP over stdio with explicit profiles

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

Interoperability matters, but a standalone JSON-RPC implementation is not enough to be MCP,
and the example in the source conversation declares an old protocol revision
[F0, pp. 23, 44].

## Decision

Use the modern profile, 2026-07-28, through a compatible SDK. The legacy profile, 2025-11-25,
is opt-in and has its own tests. Domain IDs and contracts stay independent of the transport.

## Alternatives considered

- A direct internal API for tests.
- Remote HTTP.
- A hand-written JSON-RPC parser.

## Consequences and cost

Portability across compatible hosts is preserved. The modern profile has no `initialize`, and
support for every client is not promised. stdout stays reserved for the protocol.

## Required validation

CT-05..CT-10 against pinned clients and versions. Validating the JSON of the examples is not
enough.

## Reversibility

Add another profile through an adapter, without changing the algorithms or the snapshots.

## Provenance

S01–S05 and S23. Revises the original ADR-002 in [F0]. The contract itself is in
[thesis chapter 07](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** La interoperabilidad importa, pero JSON-RPC aislado no basta para ser MCP. El
  ejemplo declara una revisión antigua [F0, pp. 23, 44].
- **Decisión:** Perfil moderno 2026-07-28 con SDK compatible. Legacy 2025-11-25 opt-in con
  pruebas propias. Mantener IDs y contratos de dominio independientes del transporte.
- **Alternativas consideradas:** API interna directa para tests; HTTP remoto; parser JSON-RPC
  manual.
- **Consecuencias y coste:** Se conserva portabilidad entre hosts compatibles. El perfil moderno
  no lleva initialize; no se prometen todos los clientes. stdout queda reservado a protocolo.
- **Validación exigida:** CT-05..CT-10 contra clientes/versiones fijadas. No basta validar el
  JSON de ejemplos.
- **Reversibilidad:** Agregar otro perfil mediante adaptador sin modificar algoritmos ni
  snapshots.
- **Procedencia:** S01–S05, S23; revisa ADR-002 original.

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
