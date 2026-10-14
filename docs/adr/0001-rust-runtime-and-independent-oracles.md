# 0001. Rust for the runtime; independent oracles

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

The source conversation chooses Rust for performance and distribution, but it overstates
Rust's determinism and claims a universal superiority it does not show [F0, pp. 3–5].

## Decision

Keep Rust for the core and for the production adapters. Use Python only as an oracle and
for offline evaluation. Never turn the Python reference into a deployment requirement.

## Alternatives considered

- Python with native kernels.
- C++.
- Rust with IPC to Python.

## Consequences and cost

The runtime stays in one language and memory stays under control. We accept the cost of FFI,
of the ONNX integration and of the learning curve. No latency is promised on the strength of
the language.

## Required validation

Build and profile on the S0 hardware, run differential tests against the oracle, and
distribute with the real dependencies.

## Reversibility

Reopen if the integration or the models block delivery and another runtime satisfies the same
contracts.

## Provenance

S22. Replaces the absolute justification of the original ADR-001 in [F0].

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** El PDF elige Rust por rendimiento y distribución, pero exagera determinismo y
  superioridad universal [F0, pp. 3–5].
- **Decisión:** Mantener Rust para core y adaptadores productivos. Usar Python sólo como
  oráculo y evaluación offline. No convertir la referencia Python en requisito de despliegue.
- **Alternativas consideradas:** Python con kernels nativos; C++; Rust con IPC a Python.
- **Consecuencias y coste:** Menor dispersión del runtime y mejor control de memoria. Se asumen
  coste de FFI, integración ONNX y curva de desarrollo; no se prometen latencias por lenguaje.
- **Validación exigida:** Compilación y profiling en hardware S0, tests diferenciales y
  distribución con dependencias reales.
- **Reversibilidad:** Reabrir si integración/modelos bloquean entrega y un runtime alternativo
  satisface los mismos contratos.
- **Procedencia:** S22; sustituye la justificación absoluta de ADR-001 original.

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
