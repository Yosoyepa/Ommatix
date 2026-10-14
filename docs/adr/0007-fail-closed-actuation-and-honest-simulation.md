# 0007. Fail-closed actuation and honest simulation

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

A coordinate map is not an actuator, and a visual change does not prove that the task
succeeded [F0, pp. 43–45].

## Decision

Deny by default. A local policy gate checks freshness and focus, and `dry_run` is explicit.
Delivery and verification have separate statuses. Actions are never retried blindly.

## Alternatives considered

- Optimistic execution with later correction.
- Automatic re-anchoring.

## Consequences and cost

Rejections and complexity increase. The residual race with the OS remains, so the domain is
limited to low-risk flows.

## Required validation

UT-38..UT-43, IT-03..IT-05, SEC-01..SEC-05 and a real stop.

## Reversibility

Widen the set of actions only with specific policies and verifiers.

## Provenance

S07. New proposal [P]. The statuses and errors are specified in
[thesis chapter 07](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** Un mapa de coordenadas no es un actuador y un cambio visual no prueba éxito de
  negocio [F0, pp. 43–45].
- **Decisión:** Denegación por defecto; policy gate local, frescura y foco; dry_run explícito.
  Estados de entrega y verificación separados. Sin retries ciegos de acciones.
- **Alternativas consideradas:** Ejecución optimista y corrección posterior; reanclaje
  automático.
- **Consecuencias y coste:** Aumenta rechazos y complejidad. No elimina la carrera residual del
  SO; el dominio se limita a flujos de bajo riesgo.
- **Validación exigida:** UT-38..UT-43, IT-03..IT-05, SEC-01..SEC-05 y parada real.
- **Reversibilidad:** Ampliar acciones sólo con políticas y verificadores específicos.
- **Procedencia:** S07; propuesta nueva [P].

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
