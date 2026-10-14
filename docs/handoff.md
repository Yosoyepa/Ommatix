# Estado del proyecto y relevo / Project status and handoff

Fecha: 2026-10-14. Este documento resume dónde quedó el trabajo y qué sigue, para que otra
persona o agente continúe sin rehacer nada. La especificación es el dossier de
[`docs/thesis/`](thesis/README.md); las reglas del repositorio están en
[`AGENTS.md`](../AGENTS.md).

## Qué está hecho

| PR | Contenido | Estado |
|---|---|---|
| #1 | Nombre del proyecto | Fusionado |
| #2 | Prototipo hexagonal del arranque (`src/`), sin el dossier | Fusionado |
| #3 | Dossier de la tesis consolidado en `docs/thesis/` como Ommatix; ADR-0001..0012 de la tesis en inglés (aprobadas el 2026-10-14), ADR-0013/0014; ADR del arranque archivadas en `docs/adr/bootstrap/`; job `thesis` en la CI | Fusionado |
| #4 | S1: núcleo geométrico e incremental en `ommatix-domain` (`bbox`, `incremental`, `snapshot`, `fusion`); UT-01..26; BDD-01..10 ejecutados desde `docs/thesis/features/` con `src/acceptance/tests/thesis.rs` | Fusionado |
| #5 | S2a: `appearance` (algoritmo 3A, caché de íconos con rechazo) e `identity` (algoritmo 3B, registro de sesión, comprobaciones de vigencia de una acción); UT-27..43; este documento | Abierto |

Las matrices de la tesis (`docs/thesis/matrices/`) y su `CHANGELOG.md` (1.0.1–1.0.3)
registran qué pruebas planificadas existen en Rust. Los requisitos RF/RNF siguen pendientes de
aceptación: eso exige capturas, modelos y actuador reales (S3–S4).

## Cómo está el código

- El **núcleo de la tesis** vive en `ommatix-domain`, junto al prototipo, y no se usa todavía
  en el pipeline en ejecución:
  - `incremental`: `BgraFrame`, `dirty_tiles`, `plan_regions` (halo, cierre, fusión,
    refresco completo de respaldo).
  - `snapshot`: `SnapshotStore<P>` con `stage`/`commit`, `record_failure`, estado degradado.
  - `fusion`: `internal_owners`, `reading_order`, `choose_external_label`, `fuse`,
    `Affordance::check`.
  - `appearance`: `gradient_descriptor`, `descriptor_distance`, `IconCache::lookup`.
  - `identity`: `Registry::observe` (asociación uno a uno, tombstones, revisiones),
    `Registry::check_action` (sesión, epoch, snapshot, target, revisión, accionabilidad,
    modo), `ActionStatus`.
- El **prototipo** (`ChangeTracker`, `Scene`, `PerceptionEngine`, detector de color plano,
  siete herramientas MCP con coordenadas) sigue siendo lo que ejecutan `demo` y `serve`. No se
  extiende: S2b y S3 lo reemplazan.

## Qué sigue

1. **Revisar y fusionar el PR #5** (S2a).
2. **S2b, contrato MCP de la tesis** (capítulo 07, `docs/thesis/contratos/`):
   - Caso de uso en `ommatix-application` que una el plan de regiones, la fusión, el
     registro y el `SnapshotStore` en una observación transaccional (`observe_screen`) y una
     acción validada (`execute_action`, dry run por defecto), más `get_engine_metrics`.
   - Puertos con los nombres del SDD (`FrameCapturer`, `VisionDetector`, `TextRecognizer`,
     `OsActuator`, `Clock`, `PolicyStore`), renombrando los del prototipo.
   - Adaptador MCP con `rmcp`, perfil moderno 2026-07-28 y legacy 2025-11-25 sólo si se
     habilita; DTOs que validen contra `contratos/esquemas/*.json` y los ejemplos válidos e
     inválidos; límites de 64 KiB; errores con `code`, `retryable` y `next_step`.
   - Pruebas CT-01..10 y step definitions de BDD-11..27 en `src/acceptance/tests/thesis.rs`
     (añadir cada ID a `IMPLEMENTED`). Actualizar matrices y CHANGELOG.
   - Retirar las siete herramientas del prototipo y sus features en
     `src/acceptance/features/` cuando el contrato nuevo las reemplace.
3. **S0, spike en Windows 11** (necesita el PC del responsable): hardware, detectores y OCR
   candidatos, licencias, exportación ONNX, proveedor de ejecución, captura DXGI con stride,
   compatibilidad del cliente MCP. Resultado: decisiones que cierran ADR-0009 y fijan modelos.
4. **S3, adaptadores reales en Windows**: DXGI Desktop Duplication, ONNX Runtime, `SendInput`,
   política local, parada y verificación del efecto. Necesita una ADR que levante
   `unsafe_code = "forbid"` para el adaptador de Windows.
5. **S4, aceptación**: 450 corridas de extremo a extremo, benchmarks, soak e informe.

## Convenciones que conviene no perder

- Las features de la tesis se quedan en `docs/thesis/features/`; el validador comprueba esa
  ruta. Sus steps van en `src/acceptance/tests/thesis.rs`.
- Las pruebas planificadas llevan su ID (`ut_34_...`) y toman el resultado esperado del
  oráculo (`docs/thesis/referencia_ejecutable/tests/test_core.py`).
- Los CSV de las matrices son UTF-8 con BOM y fin de línea LF.
- Si al correr el oráculo o el validador sólo cambian las marcas de tiempo de
  `referencia_ejecutable/resultados/`, se descartan con `git restore`.
- Cada cambio de estado en las matrices lleva entrada en `docs/thesis/CHANGELOG.md`.
- Commits en Conventional Commits; PR con `.github/pull_request_template.md`; documentos con
  gemelo `*.es.md` se actualizan en el mismo PR.
- Nada en el repositorio menciona el nombre anterior del proyecto. No se versionan capturas,
  pesos de modelos ni la conversación original [F0].
