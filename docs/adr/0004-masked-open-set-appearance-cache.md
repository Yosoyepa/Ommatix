# 0004. Masked, open-set appearance cache

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

A thresholded dHash is not complemented by a colour inversion in flat regions, and appearance
does not prove intent [F0, pp. 12–14].

## Decision

Keep dHash as the baseline. Propose signs plus a support mask, context, a threshold and a
rejection margin. Never learn trusted labels without validation.

## Alternatives considered

- pHash.
- Learned descriptors.
- An on-demand captioner, or manual curation.

## Consequences and cost

The descriptor has 128 logical bits. An ideal inversion can be invariant; a real theme needs
evaluation. Unknown icons stay `unknown`.

## Required validation

UT-27..UT-31: synthetic inversion, real themes, conflicts and out-of-distribution false
positives.

## Reversibility

Choose another descriptor by its precision–coverage and latency trade-off, under the same
provenance contracts.

## Provenance

S21. Replaces the absolute invariance claimed by the original ADR-004 in [F0].

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** dHash umbralizado no se complementa en regiones planas y la apariencia no prueba
  intención [F0, pp. 12–14].
- **Decisión:** Mantener dHash como baseline. Proponer signos más máscara de soporte, contexto,
  umbral y margen de rechazo. No aprender etiquetas confiables sin validación.
- **Alternativas consideradas:** pHash, descriptores aprendidos, captioner bajo demanda o
  curación manual.
- **Consecuencias y coste:** El descriptor tiene 128 bits lógicos. La inversión ideal puede ser
  invariante; un tema real requiere evaluación. Desconocidos permanecen unknown.
- **Validación exigida:** UT-27..UT-31; inversión sintética, temas reales, conflictos y falsos
  positivos fuera de distribución.
- **Reversibilidad:** Elegir otro descriptor según precisión-cobertura/latencia con mismos
  contratos de procedencia.
- **Procedencia:** S21; sustituye invariancia absoluta de ADR-004 original.

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
