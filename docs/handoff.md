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
| #6 | S2b: puertos del SDD y `Engine` en `ommatix-application` (`observe_screen`, `execute_action`, `get_engine_metrics`); adaptador MCP con tipos de `rmcp` 3.5.1 y bucle acotado (ADR-0015); CT-01..10 y los 17 ejemplos del dossier; BDD-11..27 ejecutados; `serve`/`demo` con el motor nuevo y `--config`; algoritmo 3A dentro de la observación; se retira el prototipo; revisión en [`reviews/2026-10-14-s2b-engine-and-mcp.md`](reviews/2026-10-14-s2b-engine-and-mcp.md) | Abierto |
| por asignar | S3, primera parte (rama `feat/s3-windows-adapters`, sobre S2b): captura DXGI (`--source dxgi`, `--output N`) y entrada `SendInput` (`--input sendinput`) con `unsafe` acotado (ADR-0016, propuesta); detector ONNX que falla de forma segura con manifiesto y SHA-256 y OCR de Windows (`[models]`); job `windows` en la CI; parada del operador (`[safety] stop_file`) y comprobación del foco antes de escribir; BDD-28..32 ejecutados sobre un doble del actuador real | Abierto |

Las matrices de la tesis (`docs/thesis/matrices/`) y su `CHANGELOG.md` (1.0.1–1.0.6)
registran qué pruebas planificadas existen en Rust. Los requisitos RF/RNF siguen pendientes de
aceptación: eso exige capturas, modelos y actuador reales en corridas supervisadas (S3–S4).
Ninguna prueba IT ni SEC se ha ejecutado.

## Cómo está el código

- **`ommatix-domain`**, el núcleo de la tesis, sin I/O:
  - `incremental`: `BgraFrame`, `dirty_tiles`, `plan_regions` (halo, cierre, fusión,
    refresco completo de respaldo).
  - `snapshot`: `SnapshotStore<P>` con `stage`/`commit`, `record_failure`, estado degradado.
  - `fusion`: `internal_owners`, `reading_order`, `choose_external_label`, `fuse`,
    `Affordance::check`.
  - `appearance`: `gradient_descriptor`, `descriptor_distance`, `IconCache::lookup`; el
    `Engine` lo usa en cada observación (abajo).
  - `identity`: `Registry::observe` (asociación uno a uno, tombstones, revisiones),
    `Registry::check_action` (sesión, epoch, snapshot, target, revisión, accionabilidad,
    modo), `ActionStatus`.
  - Del prototipo del arranque sólo quedan los tipos de apoyo que usan los adaptadores
    provisionales: `Frame` RGBA, `Rect`/`Point`, `Detection`, `Action` y `DisplayMapping`.
- **`ommatix-application`**: los puertos del SDD (`FrameCapturer`, `VisionDetector`,
  `TextRecognizer`, `OsActuator`, `Clock`, `PolicyStore`), los tipos del contrato Ommatix 1.0
  (`contract`) y el caso de uso `Engine`:
  - `observe_screen`: captura → plan de regiones → detector y OCR sólo en las regiones
    planificadas → fusión → íconos (algoritmo 3A) → registro → staging y commit. Una etapa
    fallida no publica nada.
  - Íconos (`engine/icons.rs`): los controles `IconButton`, o sin texto interior y con un rol
    que no toma etiqueta externa, se recortan del frame, pasan a luma BT.601, se remuestrean a
    9×8 con medias enteras por bloque y se buscan en el `IconCache` del contexto configurado.
    Una coincidencia es `curated_icon`; cualquier `unknown` deja la etiqueta vacía. La
    etiqueta nunca cambia la accionabilidad. Sólo se vuelven a resolver los candidatos de las
    regiones planificadas. `EngineConfig::icons` es `None` por defecto (resolvedor apagado):
    los umbrales no tienen valor por defecto porque la tesis los calibra (H4).
  - `execute_action`: diario de deduplicación por `request_id` (hash FNV-1a, sin guardar el
    texto); comprobación estricta de snapshot, epoch, sesión y revisión; dry run por defecto
    (`simulated`); entrada real sólo con política local (`actuation_enabled`, sin
    `stopped`), actuador real y una aplicación en primer plano de `allowed_applications`
    atestiguada por el actuador (`OsActuator::foreground_application`: la atestigua
    `SendInput`; `enigo` no, así que con él se deniega siempre);
    comprobación de vigencia con una captura justo antes de entregar (`STALE_SNAPSHOT` si la
    pantalla cambió); recibo de entrega → `input_delivered` o `indeterminate`;
    `visual_change` se verifica con una captura posterior comparada con la previa a la
    entrega y los demás efectos quedan `indeterminate`; un error del actuador también es
    `indeterminate`, nunca se reintenta; cualquier entrega agota el snapshot hasta la
    siguiente observación.
  - `get_engine_metrics`. Pruebas del motor: `ut_14`, `ut_38`..`ut_43` y las del módulo
    `engine::tests::icons`.
- **`ommatix-adapter-mcp`**: tipos de wire de `rmcp` 3.5.1 con un bucle de mensajes propio y
  acotado (ADR-0015); perfil 2026-07-28 siempre, 2025-11-25 sólo con `[mcp] legacy_enabled`;
  conexión fijada en un perfil; 64 KiB por mensaje; argumentos fuera del schema → `-32602`;
  errores del dominio como resultados con `isError`; tiempos en `_meta`
  (`io.ommatix/timings`). Pruebas en `tests/`: `ct_observation.rs` (CT-01..04),
  `ct_protocol.rs` (CT-05..10) y `contract_examples.rs` (los 17 casos de
  `validacion_casos.json`).
- **Adaptadores conducidos**:
  - `ommatix-adapter-sim`: `SimulatedDesktop` y `FixtureDesktop`, deterministas.
  - `ommatix-adapter-capture`: guion, frames PNG, pantalla con `xcap` (feature `screen`,
    provisional) y `DxgiCapturer` (feature `dxgi`, Windows): DXGI Desktop Duplication de una
    salida en píxeles físicos, copia a staging con stride, salida rotada rechazada, epoch
    nuevo ante cambios de tamaño, DPI u origen, duplicación perdida recreada una vez,
    correspondencia de pantalla compartida con la entrada (`mapping_source`). El `unsafe`
    vive sólo en `dxgi/win.rs`.
  - `ommatix-adapter-input`: dry run, `enigo` (feature `native`, provisional) y
    `SendInputActuator` (feature `sendinput`, Windows): atestigua la aplicación en primer
    plano (nombre del ejecutable en minúsculas) y, antes de enviar, rechaza otro proceso en
    primer plano, un punto fuera de sus ventanas, un nivel de integridad mayor y
    modificadores pulsados; una llamada `SendInput` por acción. El `unsafe` vive sólo en
    `sendinput/win.rs`. Se prueba sólo con sus funciones puras: nada envía entrada real.
  - `ommatix-adapter-vision`: detector de regiones planas (roles `unknown`),
    `NullTextRecognizer`, `OnnxVisionDetector` (feature `onnx`: manifiesto fijado por
    SHA-256, ONNX Runtime cargado al ejecutar desde una biblioteca fijada por SHA-256, pesos
    comprobados contra el manifiesto, letterbox, decodificación y NMS; roles y affordances
    del manifiesto) y `WinRtTextRecognizer` (feature `winrt-ocr`: `Windows.Media.Ocr` para
    un único idioma, sin alternativa). Las piezas independientes del modelo (`model`, `crop`,
    `ocr`) se compilan y prueban siempre.
  - Un único `windows = "=0.62.2"` en el workspace; cada crate añade sus features.
- **`ommatix` (app)**: `serve` y `demo` ejecutan el motor nuevo con las opciones `--config`,
  `--source sim|images|screen|dxgi` (`sim` por defecto; `screen` con la feature `screen`,
  `dxgi` con la feature `dxgi`), `--images DIR`, `--output N` (salida de `dxgi`, 0 por
  defecto), `--input dry-run|native|sendinput` (por defecto `[engine] mode`; `native` con la
  feature `native-input` y la fuente `screen`; `sendinput` con la feature `sendinput`, la
  fuente `dxgi` y una lista `allowed_applications` no vacía de nombres en minúsculas; ambas
  exigen `actuation_enabled = true`), `--theme`, `--session-id` y `--legacy`; `demo` añade
  `--observations N` (1–20). Features: `screen`, `native-input`, `dxgi`, `sendinput`,
  `onnx`, `winrt-ocr`, todas apagadas por defecto. La configuración tiene la forma de
  `docs/thesis/plantillas/configuracion_mvp.toml` y se valida de forma estricta: se rechazan
  claves desconocidas, valores fuera de rango y valores que el binario no puede cumplir
  (otra plataforma, perfil o transporte, `detector_path`/`detector_sha256`/`ocr_path`/
  `ocr_sha256`, un detector configurado a medias o con hashes mal formados,
  `allow_destructive`, `allow_screenshots_to_client`, `persist`); `max_snapshot_age_ms` se
  acepta pero no se aplica (nota en stderr). `[models]` elige el detector
  (`detector_manifest`, `detector_manifest_sha256`, `onnxruntime_path`,
  `onnxruntime_sha256`, juntas) y el OCR (`ocr = "none"|"winrt"`, `ocr_language`) para las
  fuentes `images`, `screen` y `dxgi`; las rutas relativas lo son al archivo de
  configuración; cualquier fallo de integridad, feature o idioma detiene el motor antes de
  abrir la captura. `[cache]` admite un archivo JSON lines de prototipos curados con
  `context`, `gradient_tau`, `max_distance` y `min_margin`, todos juntos y sin valores por
  defecto; el archivo se rechaza entero ante cualquier error o si supera `max_entries`.
- **Aceptación**: `src/acceptance/tests/thesis.rs` ejecuta BDD-01..32 desde
  `docs/thesis/features/` (BDD-01..10 y 13..21 sobre el núcleo; BDD-11, 12 y 22..27 a través
  del motor y del adaptador MCP; BDD-28..32 igual, sobre `LiveDouble`, un doble del actuador
  real que registra lo que recibe y no envía nada). BDD-32 usa la política real del operador
  (`ommatix::policy::ConfigPolicy`, expuesta por la biblioteca del crate `ommatix`) con un
  archivo de parada en un directorio temporal. BDD-28 cubre otra aplicación que pasa a
  primer plano antes de escribir; el foco de teclado dentro de la aplicación permitida lo
  comprueba el actuador `SendInput`, que ninguna prueba ejecuta. Se retiraron las siete herramientas del prototipo y su suite de
  aceptación (`src/acceptance/features/` y `tests/acceptance.rs`).
- **Límites conocidos**: la entrada real sólo es posible con `--source dxgi --input
  sendinput` y el consentimiento del operador, y nunca se ha ejecutado de forma supervisada;
  la parada del operador (`[safety] stop_file`) y la comprobación del foco antes de
  escribir sólo se han probado sobre dobles (SEC-02 y SEC-05 esperan corridas supervisadas);
  sólo se verifica el efecto `visual_change`; no hay modelo seleccionado, así que los roles
  siguen `unknown` y no hay OCR por defecto; OR-44 espera "actuación no implementada" y el
  motor responde `POLICY_DENIED` sin entregar nada; el contexto del caché de íconos lo fija
  el operador y sus umbrales no están calibrados; no se ha probado un cliente MCP real
  (capítulo 07 §7.1).

## Qué sigue

1. **Revisar y fusionar** los PR abiertos en orden: #5 (S2a), #6 (S2b) y el de S3, primera
   parte (`feat/s3-windows-adapters`).
2. **Decisiones que esperan al responsable** (ningún agente puede tomarlas):
   - Aprobar o cambiar la ADR-0015 (transporte MCP sobre los tipos de `rmcp`) y la ADR-0016
     (`unsafe` acotado en los adaptadores de captura y entrada de Windows). Ambas están
     propuestas.
   - Elegir los modelos y sus licencias (ADR-0009, spike S0): detector y OCR, su
     exportación ONNX, el manifiesto con estado `SELECCIONADO` y los SHA-256, y la versión
     de ONNX Runtime que se fija. Hallazgos del spike, todavía sin aprobar: hardware Ryzen 7
     5700X, RX 6700 XT, 32 GiB, dos monitores 1920×1080 al 100 % (se usa la salida 0;
     ADR-0008: un monitor); `ort` 2.0.0-rc.13 mantiene el MSRV 1.88 (rc.14 exige 1.92);
     DirectML está en ingeniería sostenida, así que la CPU es la base; los pesos
     `icon_detect` v2 de OmniParser son AGPL-3.0 (sólo línea base local, nunca se
     distribuyen) e `icon_detect_v3` es MIT pero sin ONNX oficial; candidato de OCR
     PP-OCRv5 mobile (Apache-2.0, ONNX oficial, sin "¡" en su diccionario);
     `Windows.Media.Ocr` sólo tiene es-ES y es-MX instalados y no informa confianza.
   - Elegir el cliente MCP real con el que se prueba la conformidad (capítulo 07 §7.1).
   - Elegir las aplicaciones de prueba controladas (para `allowed_applications`, las
     corridas IT y la evaluación de S4).
3. **Corridas supervisadas en el hardware de referencia**, con el operador presente y una
   aplicación de prueba en `allowed_applications`; ninguna está hecha y ninguna prueba
   automática las sustituye:
   - IT-01 (captura DXGI: frames reales, stride, rotación, pérdida de acceso;
     `cargo test -p ommatix-adapter-capture --features dxgi -- --ignored` toma un frame sin
     guardarlo), IT-02 (detector y OCR reales, deshacer el letterbox; necesita modelos),
     IT-03 (conteo de `SendInput`, permisos y foco), IT-04 (postcondición y plazo sin
     reintento), IT-05 (idempotencia ante caídas), IT-06 (telemetría redactada), IT-07 (DPI
     100/125/150 %, epoch y fuera de perfil).
   - SEC-01..06: el agente no puede habilitarse, parada, prompt injection visual, sin
     capturas ni secretos, precheck de foco y ventana con carreras, integridad de modelo y
     caché.
4. **Resto de S3**: comprobar la ventana con el foco antes de escribir (BDD-28, SEC-05), un
   mecanismo de parada del operador que active `LocalPolicy::stopped` e interrumpa una
   secuencia (BDD-32, SEC-02), verificar los efectos de foco, valor y desplazamiento,
   contexto del caché de íconos tomado de la aplicación en primer plano, telemetría y
   auditoría, retirar `xcap`/`enigo`.
5. **S4, aceptación**: 450 corridas de extremo a extremo, benchmarks, soak e informe.

## Convenciones que conviene no perder

- Las features de la tesis se quedan en `docs/thesis/features/`; el validador comprueba esa
  ruta. Sus steps van en `src/acceptance/tests/thesis.rs`.
- Las pruebas planificadas llevan su ID (`ut_34_...`) y toman el resultado esperado del
  oráculo (`docs/thesis/referencia_ejecutable/tests/test_core.py`).
- Los CSV de las matrices son UTF-8 con BOM y fin de línea LF.
- Si al correr el oráculo o el validador sólo cambian las marcas de tiempo o la plataforma
  de `referencia_ejecutable/resultados/`, se descartan con `git restore`.
- Ninguna prueba ni comando de desarrollo envía entrada real; `SendInput` sólo se prueba con
  sus funciones puras. Un escenario sobre un doble del actuador se marca
  `EJECUTADO_SOBRE_CONTRATO_S3_DOBLE` y no cierra la prueba IT o SEC que lo acompaña.
- Cada cambio de estado en las matrices lleva entrada en `docs/thesis/CHANGELOG.md`.
- Commits en Conventional Commits; PR con `.github/pull_request_template.md`; documentos con
  gemelo `*.es.md` se actualizan en el mismo PR.
- Nada en el repositorio menciona el nombre anterior del proyecto. No se versionan capturas,
  pesos de modelos ni la conversación original [F0].
