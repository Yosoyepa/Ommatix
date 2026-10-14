# 0009. Models and licences decided in the spike

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

"YOLO" names a family of models, not an inventory of classes, and licences differ between
sets of weights [F0, pp. 1–5].

## Decision

Choose the detector and the OCR only after checking export, roles, quality and the licence of
each artefact. Run ONNX behind a port, and do not assume TensorRT on an AMD GPU.

## Alternatives considered

- A large model directly.
- Accessibility as the primary source.
- Our own training or distillation.

## Consequences and cost

No weights are distributed without review. DirectML is an option to evaluate, not a
guaranteed advantage; WinML may be a later step.

## Required validation

IT-02 and a manifest with hash, revision, opset, layout and classes.

## Reversibility

Replace the provider or the model while keeping `RawEvidence` and the quality tests.

## Provenance

S09–S14. Removes fictitious dependencies.

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026).

- **Contexto:** YOLO describe una familia, no el inventario de clases. Hay diferencias de
  licencia entre pesos [F0, pp. 1–5].
- **Decisión:** Elegir detector/OCR sólo tras comprobar exportación, roles, calidad y licencia
  por artefacto. Usar ONNX detrás de un puerto; no asumir TensorRT en GPU AMD.
- **Alternativas consideradas:** Modelo grande directo; accesibilidad primaria;
  entrenamiento/destilación propios.
- **Consecuencias y coste:** No se distribuyen pesos sin revisión. DirectML es opción evaluable,
  no ventaja garantizada; WinML puede ser evolución.
- **Validación exigida:** IT-02 y manifiesto con hash, revisión, opset, layout y clases.
- **Reversibilidad:** Sustituir proveedor o modelo manteniendo RawEvidence y tests de calidad.
- **Procedencia:** S09–S14; elimina dependencias ficticias.

**Aprobación (14 de octubre de 2026):** aprobada por el responsable del proyecto.
