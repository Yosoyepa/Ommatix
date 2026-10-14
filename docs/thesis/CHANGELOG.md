# Historial documental

## 1.0.4 — 2026-10-14: S2b, contrato MCP

No cambia ningún requisito, algoritmo, contrato ni escenario. Cambia el estado de las matrices y la implementación que las respalda:

- CT-01 a CT-10 pasan a `IMPLEMENTADO_EN_RUST_S2` en `matrices/plan_pruebas.csv`. Están en `src/adapters/mcp/tests/` (`ct_observation.rs` para CT-01..04 y `ct_protocol.rs` para CT-05..10) y recorren el motor real con puertos deterministas. `contract_examples.rs` comprueba además los 17 casos de `contratos/validacion_casos.json`: los 9 ejemplos válidos se aceptan y los 8 inválidos se rechazan, igual que con los schemas.
- BDD-11 a BDD-21 pasan a `EJECUTADO_SOBRE_NUCLEO_S2` y BDD-22 a BDD-27 a `EJECUTADO_SOBRE_CONTRATO_S2` en `matrices/gherkin_trazabilidad.csv`. Se ejecutan desde `features/` con `src/acceptance/tests/thesis.rs`: BDD-13..21 sobre el núcleo del dominio; BDD-11, BDD-12 y BDD-22..27 a través del motor y del adaptador MCP, sobre el escritorio simulado o de fixtures. BDD-28 a BDD-32 siguen pendientes: necesitan foco, verificación real, idempotencia ante caídas, política de contenido y parada del operador (S3).
- UT-14 y UT-38..UT-43 tienen además pruebas a nivel del motor (`ut_14_…`, `ut_38_…`…`ut_43_…` en `ommatix-application`); su estado en la matriz no cambia.
- Los requisitos RF/RNF y los riesgos no cambian de estado: su aceptación exige captura, modelos, actuador y un cliente MCP reales.
- Los puertos de la aplicación toman los nombres del SDD (§2.4): `FrameCapturer`, `VisionDetector`, `TextRecognizer`, `OsActuator`, `Clock` y `PolicyStore`. El caso de uso `Engine` reúne `observe_screen` (plan de regiones, detector y OCR sólo en las regiones planificadas, fusión, registro y staging/commit), `execute_action` (diario de deduplicación por `request_id` con hash FNV-1a, sin guardar el texto tecleado; comprobación estricta de snapshot, epoch, sesión y revisión; dry run por defecto) y `get_engine_metrics`.
- Se retira el prototipo del arranque como pipeline en ejecución: sus puertos (`FrameSource`, `ElementDetector`, `InputController`), sus casos de uso, sus siete herramientas MCP con coordenadas y sus features de `src/acceptance/features/`. `serve` y `demo` ejecutan el motor nuevo con una configuración con la forma de `plantillas/configuracion_mvp.toml`.
- [ADR-0015](../adr/0015-mcp-transport-on-rmcp-wire-types.md), propuesta: el adaptador usa los tipos de wire de `rmcp` 3.5.1 con un bucle de mensajes propio y acotado, porque el bucle de servicio de `rmcp` no limita el tamaño de los mensajes, cierra la conexión ante notificaciones tempranas, deja mezclar reglas legacy y modernas en una conexión y descarta en silencio la entrada no analizable. La conformidad con un cliente real sigue pendiente (§7.1).
- El perfil 2026-07-28 se sirve siempre; el legacy 2025-11-25 sólo si se habilita (`[mcp] legacy_enabled` o la opción de línea de órdenes). Una conexión queda fijada en un perfil. Mensajes de hasta 64 KiB en entrada y salida.

Decisiones conocidas de esta implementación:

- Los argumentos que violan el schema de entrada de una herramienta son error JSON-RPC `-32602`, no un resultado de herramienta con `isError`. Los errores del dominio sí son resultados con `isError: true`, `code`, `retryable` y `next_step`. Los tiempos por etapa van en `_meta` del resultado, bajo `io.ommatix/timings`.
- `server/discover` fija el perfil moderno en la conexión, como cualquier petición con metadatos 2026-07-28.
- `allowed_applications` no se aplica antes de S3: sin evidencia de la ventana en primer plano no se puede comprobar. La actuación real sigue deshabilitada por defecto y exige política local y actuador real.
- OR-44 espera "actuación real no implementada"; el motor responde `POLICY_DENIED` sin entregar nada cuando no hay política o actuador real. El efecto es el mismo (no se envía entrada); cambia el código.
- Sólo el efecto `visual_change` se verifica, con una nueva captura comparada con la base confirmada. Los demás efectos esperados se informan como `indeterminate` tras una entrega.
- El caché de íconos (algoritmo 3A) todavía no participa en `observe_screen`: ningún detector disponible produce íconos sin texto. Se conecta con los modelos de S3.

## 1.0.3 — 2026-10-14: S2a, caché de apariencia e identidad

No cambia ningún requisito, algoritmo, contrato ni escenario. Cambia el estado de las matrices:

- UT-27 a UT-43 pasan a `IMPLEMENTADO_EN_RUST_S2` en `matrices/plan_pruebas.csv`. Son pruebas de los módulos `appearance` (algoritmo 3A) e `identity` (algoritmo 3B y comprobaciones de vigencia de una acción) de `ommatix-domain`.
- Los contraejemplos del diseño original (dHash de 64 bits, ID FNV por celda, probabilidad de colisión) sólo existen dentro de las pruebas UT-27, UT-32 y UT-33.
- El caché no fija umbrales de distancia ni de margen: la tesis los calibra antes del test (H4), así que quien lo usa debe elegirlos. El IoU mínimo de asociación (0,5) es un valor inicial [P].
- BDD-11..21 y el contrato MCP (CT-01..10, BDD-22..26) quedan para S2b.

## 1.0.2 — 2026-10-14: S1, núcleo geométrico e incremental

No cambia ningún requisito, algoritmo, contrato ni escenario. Cambia el estado de las matrices:

- UT-01 a UT-26 pasan a `IMPLEMENTADO_EN_RUST_S1` en `matrices/plan_pruebas.csv`. Son pruebas del crate `ommatix-domain` (módulos `incremental`, `snapshot`, `fusion` y `bbox`) que llevan su ID en el nombre y reproducen los casos del oráculo indicados en la matriz.
- BDD-01 a BDD-10 pasan a `EJECUTADO_SOBRE_NUCLEO_S1` en `matrices/gherkin_trazabilidad.csv`. Se ejecutan desde esta carpeta con `src/acceptance/tests/thesis.rs` y ejercitan el núcleo directamente; el recorrido por MCP llega con el contrato en S2. BDD-11 y BDD-12 esperan a S2 porque hablan del ID del elemento y de su publicación.
- Los requisitos RF-02 a RF-07 siguen pendientes: su aceptación exige capturas, detector y OCR reales.
- Diferencia conocida con el oráculo: la normalización de texto en Rust usa NFC y `to_lowercase`; el oráculo usa `casefold`, que además convierte caracteres como `ß` en `ss`. Ninguna prueba del plan depende de esa diferencia.

## 1.0.1 — 2026-10-14: consolidación en el repositorio

El dossier entra al repositorio con el nombre final del proyecto, Ommatix, y pasa a ser su línea base. No cambia ningún requisito, algoritmo, contrato ni escenario; los cambios son de ubicación y de estado:

- Los capítulos, contratos, matrices, plantillas, features, diagramas y la referencia ejecutable viven en `docs/thesis/`. El índice `LEEME.md` pasa a ser `README.md`.
- Las doce ADR pasan a `docs/adr/` como ADR-0001 a ADR-0012, en inglés y con su texto original en español. Se añaden ADR-0013 (documentación bilingüe) y ADR-0014 (crates hexagonales), que recogen decisiones del arranque del repositorio. Las ADR escritas durante ese arranque se archivan en `docs/adr/bootstrap/`.
- El responsable del proyecto aprueba las doce ADR el 14 de octubre de 2026, entre ellas ADR-0008: Windows 11 x64, un monitor y DPI de 100, 125 y 150 %. Siguen pendientes las aplicaciones de prueba, el cliente MCP, los pesos, las licencias, el hardware de evaluación, el corpus y las políticas de uso.
- `validar_paquete.py` lee los capítulos de esta carpeta y las ADR de `docs/adr/`. La integración continua ejecuta el oráculo y el validador en cada cambio.
- Las vistas de `diagramas/` tienen una versión con etiquetas en inglés en `docs/diagrams/`. Los `.svg` no se versionan; se generan desde los `.dot`.
- La conversación original [F0], la edición de lectura en PDF y Word, el manifiesto, las sumas SHA-256 y el informe de maquetación se conservan fuera del repositorio. La integridad la da git.

## 1.0 revisada — 2026-10-13

Se materializa la solicitud de documentos del PDF en archivos separados y dossier de lectura. Se preserva la fuente original sin modificaciones. Se añaden auditoría de 24 hallazgos, fuentes primarias, requisitos, contratos, ADR y validación trazable.

Cambios sustantivos: IDs por tracking en lugar de FNV cuantizado; descriptor perceptual con soporte/rechazo; incrementalidad transaccional; accionabilidad conservadora; política de ejecución; perfiles MCP actuales; selección de modelos por spike; presupuesto medido y separación explícita entre documentación, oráculos y MVP funcional.

El generador de Rust del PDF no se presenta como producto compilado. En lugar de transcribir código truncado y conservar defectos conocidos, se entrega una referencia escalar de pruebas y una plantilla de puertos Rust. Los documentos mantienen el runtime Rust como arquitectura objetivo.
