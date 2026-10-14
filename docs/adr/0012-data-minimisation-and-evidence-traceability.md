# 0012. Data minimisation and evidence traceability

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

Pixels, labels and values can hold sensitive information, even when perception runs locally
[F0, p. 2].

## Decision

No screenshots and no values in logs by default. Screen content never raises permissions.
Labels in the cache carry their provenance. Source, proposal and validation are kept apart.

## Alternatives considered

- Full logging, always.
- Trusting the agent's caption.
- Remote telemetry by default.

## Consequences and cost

Diagnostics require specific consent and redaction. Research must use an authorised,
reproducible corpus without unnecessary personal data.

## Required validation

SEC-03..SEC-06, DOC-01 and a review of the release artefacts.

## Reversibility

Allow temporary diagnostics under a local policy, with a TTL and verifiable deletion.

## Provenance

New proposal [P]. Threats and controls are in
[thesis chapter 08](../thesis/08_SEGURIDAD_Y_PRIVACIDAD.md).

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** Pixels, labels y valores pueden contener información sensible, incluso con
  percepción local [F0, p. 2].
- **Decisión:** Sin screenshots ni valores en logs por defecto. Datos de pantalla no elevan
  permisos. Etiquetas del caché con procedencia. Fuente, propuesta y validación se mantienen
  separadas.
- **Alternativas consideradas:** Logging completo siempre; confiar en el caption del agente;
  telemetría remota por defecto.
- **Consecuencias y coste:** Diagnóstico requiere consentimiento específico y redacción. La
  investigación debe usar corpus autorizado y reproducible sin datos personales innecesarios.
- **Validación exigida:** SEC-03..SEC-06, DOC-01 y revisión de artefactos de release.
- **Reversibilidad:** Permitir diagnósticos temporales con política local, TTL y borrado
  verificable.
- **Procedencia:** Propuesta nueva [P].

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
