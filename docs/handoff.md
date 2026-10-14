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
| por asignar | S2b: puertos del SDD y `Engine` en `ommatix-application` (`observe_screen`, `execute_action`, `get_engine_metrics`); adaptador MCP con tipos de `rmcp` 3.5.1 y bucle acotado (ADR-0015); CT-01..10 y los 17 ejemplos del dossier; BDD-11..27 ejecutados; `serve`/`demo` con el motor nuevo y `--config`; se retira el prototipo | Abierto |

Las matrices de la tesis (`docs/thesis/matrices/`) y su `CHANGELOG.md` (1.0.1–1.0.4)
registran qué pruebas planificadas existen en Rust. Los requisitos RF/RNF siguen pendientes de
aceptación: eso exige capturas, modelos y actuador reales (S3–S4).

## Cómo está el código

- **`ommatix-domain`**, el núcleo de la tesis, sin I/O:
  - `incremental`: `BgraFrame`, `dirty_tiles`, `plan_regions` (halo, cierre, fusión,
    refresco completo de respaldo).
  - `snapshot`: `SnapshotStore<P>` con `stage`/`commit`, `record_failure`, estado degradado.
  - `fusion`: `internal_owners`, `reading_order`, `choose_external_label`, `fuse`,
    `Affordance::check`.
  - `appearance`: `gradient_descriptor`, `descriptor_distance`, `IconCache::lookup` (aún no
    entra en la observación: ningún detector produce íconos sin texto).
  - `identity`: `Registry::observe` (asociación uno a uno, tombstones, revisiones),
    `Registry::check_action` (sesión, epoch, snapshot, target, revisión, accionabilidad,
    modo), `ActionStatus`.
- **`ommatix-application`**: los puertos del SDD (`FrameCapturer`, `VisionDetector`,
  `TextRecognizer`, `OsActuator`, `Clock`, `PolicyStore`), los tipos del contrato Ommatix 1.0
  (`contract`) y el caso de uso `Engine`:
  - `observe_screen`: captura → plan de regiones → detector y OCR sólo en las regiones
    planificadas → fusión → registro → staging y commit. Una etapa fallida no publica nada.
  - `execute_action`: diario de deduplicación por `request_id` (hash FNV-1a, sin guardar el
    texto); comprobación estricta de snapshot, epoch, sesión y revisión; dry run por defecto
    (`simulated`); entrada real sólo con política local y actuador real; recibo de entrega →
    `input_delivered` o `indeterminate`; `visual_change` se verifica con una nueva captura y
    los demás efectos quedan `indeterminate`.
  - `get_engine_metrics`. Pruebas del motor: `ut_14`, `ut_38`..`ut_43`.
- **`ommatix-adapter-mcp`**: tipos de wire de `rmcp` 3.5.1 con un bucle de mensajes propio y
  acotado (ADR-0015); perfil 2026-07-28 siempre, 2025-11-25 sólo con `[mcp] legacy_enabled`;
  conexión fijada en un perfil; 64 KiB por mensaje; argumentos fuera del schema → `-32602`;
  errores del dominio como resultados con `isError`; tiempos en `_meta`
  (`io.ommatix/timings`). Pruebas en `tests/`: `ct_observation.rs` (CT-01..04),
  `ct_protocol.rs` (CT-05..10) y `contract_examples.rs` (los 17 casos de
  `validacion_casos.json`).
- **Adaptadores conducidos**: `ommatix-adapter-sim` (`SimulatedDesktop` y `FixtureDesktop`,
  deterministas), `ommatix-adapter-capture` (guion, frames PNG, pantalla con `xcap` tras la
  feature `screen`), `ommatix-adapter-vision` (detector de regiones planas con roles
  `unknown`, `NullTextRecognizer`) y `ommatix-adapter-input` (dry run; `enigo` tras la feature
  `native`). Son provisionales hasta S3.
- **`ommatix` (app)**: `--config` con la forma de `docs/thesis/plantillas/configuracion_mvp.toml`;
  `serve` y `demo` ejecutan el motor nuevo; fuente `sim` (por defecto), `images` o `screen`
  (feature); entrada `dry-run` (por defecto) o `native` (feature, exige fuente `screen` y
  `actuation_enabled` en la configuración).
- **Aceptación**: `src/acceptance/tests/thesis.rs` ejecuta BDD-01..27 desde
  `docs/thesis/features/` (BDD-01..10 y 13..21 sobre el núcleo; BDD-11, 12 y 22..27 a través
  del motor y del adaptador MCP). Se retiraron las siete herramientas del prototipo y sus
  features de `src/acceptance/features/`.
- **Límites conocidos**: la actuación real sigue deshabilitada por defecto;
  `allowed_applications` no se aplica antes de S3 (no hay evidencia de la ventana en primer
  plano); OR-44 espera "actuación no implementada" y el motor responde `POLICY_DENIED` sin
  entregar nada; no se ha probado un cliente MCP real (capítulo 07 §7.1).

## Qué sigue

1. **Revisar y fusionar el PR #5** (S2a) y después **el PR de S2b**.
2. **S0, spike en este equipo con Windows 11.** Lo que sigue son hallazgos de investigación:
   una **propuesta pendiente de aprobación del responsable**, no decisiones.
   - Hardware: Ryzen 7 5700X, RX 6700 XT, 32 GiB. Dos monitores de 1920×1080 al 100 %: se
     selecciona la salida 0; el segundo monitor queda fuera del perfil (ADR-0008: un monitor).
   - Captura: DXGI Desktop Duplication disponible; entrega BGRA8 en una textura que no está
     en memoria del sistema (hay que copiarla a una de staging y respetar el stride).
   - `windows` 0.62.2 compila DXGI, `SendInput` y el OCR de WinRT.
   - Inferencia: `ort` 2.0.0-rc.14 (MSRV 1.92; rc.13 mantiene 1.88). DirectML está en
     ingeniería sostenida (sustained engineering), así que CPU sigue siendo la base.
   - Detector: los pesos `icon_detect` v2 de OmniParser son AGPL-3.0, sólo valen como línea
     base local y nunca se distribuyen; `icon_detect_v3` es MIT pero son 281 MB de TorchScript
     sin ONNX oficial. Con ambos los roles siguen siendo `unknown`.
   - OCR: candidato PP-OCRv5 mobile (det + rec latino) con ONNX oficial, Apache-2.0, unos
     13 MB; su diccionario no tiene "¡". `Windows.Media.Ocr` como comparador: sólo es-ES y es-MX
     instalados y no informa confianza.
   - S3 necesita una ADR que permita `unsafe` acotado sólo en módulos `#[cfg(windows)]` de
     capture, input y vision, con comentarios `SAFETY`.
   - `SendInput` bloqueado por UIPI falla en silencio: hace falta comprobar el nivel de
     integridad del proceso destino antes de entregar.
3. **S3, adaptadores reales en Windows**: DXGI Desktop Duplication, ONNX Runtime, `SendInput`,
   política local con evidencia de la ventana en primer plano (`allowed_applications`), foco,
   parada y verificación del efecto; caché de íconos dentro de la observación; telemetría.
4. **S4, aceptación**: 450 corridas de extremo a extremo, benchmarks, soak e informe.

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
