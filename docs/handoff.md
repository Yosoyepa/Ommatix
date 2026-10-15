# Estado del proyecto y relevo / Project status and handoff

Fecha: 2026-10-15 (versión 1.0.59). Este documento resume dónde quedó el trabajo y qué sigue, para que otra
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
| #5 | S2a: `appearance` (algoritmo 3A, caché de íconos con rechazo) e `identity` (algoritmo 3B, registro de sesión, comprobaciones de vigencia de una acción); UT-27..43; este documento | Fusionado |
| #6 | S2b: puertos del SDD y `Engine` en `ommatix-application` (`observe_screen`, `execute_action`, `get_engine_metrics`); adaptador MCP con tipos de `rmcp` 3.5.1 y bucle acotado (ADR-0015); CT-01..10 y los 17 ejemplos del dossier; BDD-11..27 ejecutados; `serve`/`demo` con el motor nuevo y `--config`; algoritmo 3A dentro de la observación; se retira el prototipo; revisión en [`reviews/2026-10-14-s2b-engine-and-mcp.md`](reviews/2026-10-14-s2b-engine-and-mcp.md) | Fusionado |
| #7 | S3, primera parte (rama `feat/s3-windows-adapters`, sobre S2b): captura DXGI (`--source dxgi`, `--output N`) y entrada `SendInput` (`--input sendinput`) con `unsafe` acotado (ADR-0016); detector ONNX que falla de forma segura con manifiesto y SHA-256 y OCR de Windows (`[models]`); job `windows` en la CI; parada del operador (`[safety] stop_file`) y comprobación del foco antes de escribir; BDD-28..32 ejecutados sobre un doble del actuador real | Fusionado |
| — | Spike S0 (rama `feat/s0-omniparser-v3`): detector OmniParser `icon_detect_v3` (MIT) exportado a ONNX con `tools/s0/export_omniparser_v3.py`, manifiesto `SELECCIONADO` (`docs/thesis/plantillas/model_manifest.s0.json`), ONNX Runtime 1.28.0 en CPU, OCR de Windows es-ES; corridas supervisadas sobre `notepad.exe` sin entrada; letterbox sin ampliación; `demo --timings`; informe en [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) | Fusionado (#8) |
| — | S3, corridas supervisadas (rama `feat/s3-supervised-runs`, apilada sobre #8): captura y entrada reales sobre `notepad.exe` con `tools/s3/mcp_driver.py`; SEC-01 ejecutada, IT-01/03/05 y SEC-02/05 en parte; `indeterminate` informa la entrega del actuador; informe en [`thesis/S3_CORRIDAS_SUPERVISADAS.md`](thesis/S3_CORRIDAS_SUPERVISADAS.md) y revisión en [`reviews/2026-10-14-s3-supervised-runs.md`](reviews/2026-10-14-s3-supervised-runs.md) | Fusionado (#9) |
| — | S3, plazo de verificación y primer frame DXGI (rama `fix/s3-verify-settle`, apilada sobre #9): la verificación del cambio visual vuelve a capturar dentro de `[engine] verify_timeout_ms`/`verify_poll_ms` sin reenviar la entrada; el capturador DXGI ya no entrega un primer frame en blanco; `CHANGELOG.md` 1.0.10 y revisión en [`reviews/2026-10-14-s3-verify-settle-and-first-frame.md`](reviews/2026-10-14-s3-verify-settle-and-first-frame.md) | Abierto |
| — | S3, segunda corrida supervisada (rama `feat/s3-supervised-runs`, #9, con DirectML): plazo de verificación (`effect_observed` 2 de 2) y primer frame DXGI (2 de 2 sesiones) confirmados en vivo; IT-04 ejecutada en parte con `verify_timeout_ms = 0`; paso `it04` en `tools/s3/mcp_driver.py`; informe en [`thesis/S3_CORRIDAS_SUPERVISADAS.md`](thesis/S3_CORRIDAS_SUPERVISADAS.md) §8 (CHANGELOG 1.0.12) | Fusionado (#9) |
| — | S0, proveedor e hilos del detector (rama `perf/s0-detector-ep`, apilada sobre `feat/s3-supervised-runs`): `[models] detector_execution_provider = "cpu" \| "directml"` (feature `directml`, `DirectML.dll` fijada, sin vuelta a la CPU) y `detector_threads`; manifiesto S0 con `"cpu,directml"`; DirectML equivalente a la CPU; agrupar regiones se midió y se descartó porque cambia las detecciones; `tools/s0/synthetic_frames.py` y el ejemplo `s0_probe`; informe en [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §9 (CHANGELOG 1.0.11) | Fusionado (con #9) |
| — | S4, protocolo de aceptación reducido (rama `feat/s4-reduced-e2e`): 20 corridas E2E puntuadas sólo con clics, con `tools/s4/e2e_driver.py` y la aplicación de prueba `tools/s4/testapp` (`eframe`/`egui`, crate independiente). Aplicación de prueba, 9 de 10; Bloc de notas (menú `Editar`), 10 de 10; Calculadora no puntuada. 0 falsos éxitos según el título y el anclaje por UI Automation. El actuador `SendInput` atestigua la aplicación hospedada en vez de `ApplicationFrameHost.exe`. E2E-01 y PERF-02 en parte. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) (CHANGELOG 1.0.13) y revisión en [`reviews/2026-10-14-s4-reduced-acceptance.md`](reviews/2026-10-14-s4-reduced-acceptance.md) | Abierto |
| — | Evidencia de UI Automation (rama `feat/uia-evidence`, ADR-0017): puerto `AccessibilityProvider`, adaptador `ommatix-adapter-accessibility` (feature `uia`, crate seguro `uiautomation`, sin `unsafe` propio), `[models] accessibility = "none" \| "uia"`. Se fusiona con la visión sin sustituirla: rol, nombre (`label_source: accessibility`) y caja del elemento ganan; las cajas visuales que son el mismo control se absorben; una caja centrada sobre otro control accesible pierde la confirmación (hallazgo 2 de S4); `type` permitido sobre una entrada confirmada por accesibilidad. Etapa `accessibility` en `_meta`. UT-44 y BDD-33 (CHANGELOG 1.0.14) | Abierto |
| — | S4, segunda ronda con accesibilidad (rama `feat/uia-evidence`, #11): sondeos sin entrada que confirman por nombre de UI Automation los controles de la aplicación de prueba (`Campo de texto` como `input`), las teclas de la Calculadora y `Editar`; 36 entregas reales, 36 de 36 sobre el control pedido; la escritura de E2E-01 pierde el texto en 3 de 4 intentos (`type` verifica el foco a nivel de ventana: hallazgo abierto); serie puntuada 0 de 3; E2E-02 y E2E-03 no ejecutadas; el archivo de parada queda creado para decisión del responsable. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Segunda ronda con accesibilidad» (CHANGELOG 1.0.15) y revisión en [`reviews/2026-10-14-s4-uia-acceptance.md`](reviews/2026-10-14-s4-uia-acceptance.md) | Abierto |
| — | S4, tercera ronda (rama `feat/uia-evidence`, #11): `type` en dos fases (ADR-0018) en vivo. E2E-01, 10 de 10 corridas puntuadas, con la escritura verificada por el título y 0 teclas perdidas; E2E-02, 0 de 3 (no hay «Borrar» tras «=» en la Calculadora); E2E-03 no puntuada (dos pilotos sin menú fiable). El cliente activa la ventana principal con `AttachThreadInput` (`AppActivate` no llevó la aplicación al primer plano). Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Tercera ronda» (CHANGELOG 1.0.17) y revisión en [`reviews/2026-10-14-s4-third-round.md`](reviews/2026-10-14-s4-third-round.md) | Abierto |
| — | S4, correcciones de la tercera ronda (rama `feat/uia-evidence`, #11, CHANGELOG 1.0.18): el proveedor de accesibilidad declara una lectura truncada y el motor la repite en la observación siguiente sin confiar en lo que falta; `execute_action` informa los tiempos de sus etapas en `_meta`; el cliente elige el control de borrado de la Calculadora por nombre y comprueba y restaura el menú «Editar» del Bloc de notas por UI Automation. Probado con dobles, sin medir en vivo; el responsable repite E2E-02 y E2E-03 | Abierto |
| — | S4, cuarta ronda (rama `feat/uia-evidence`, #11, CHANGELOG 1.0.19): repetición del protocolo con las correcciones de 1.0.18. E2E-01, 10 de 10 corridas puntuadas (escritura verificada por el título, 0 teclas perdidas, 0 falsos éxitos, tiempos de acción en `_meta`); E2E-02 abandonada: sin control de borrado tras «=» en modo Científica, y el clic de navegación de la contingencia no quedó anclado al control pedido; E2E-03 no ejecutada por ese desajuste. Cliente sin cambios, con un envoltorio fuera del repositorio para el límite por ronda y los tiempos de acción. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Cuarta ronda» (CHANGELOG 1.0.19) y revisión en [`reviews/2026-10-14-s4-fourth-round.md`](reviews/2026-10-14-s4-fourth-round.md) | Abierto |
| — | S4, quinta ronda (rama `feat/uia-evidence`, #11, CHANGELOG 1.0.21): E2E-02 no se ejecutó (el cambio a Estándar se detuvo en el paso «Estándar» tras un clic de navegación anclado); E2E-03, piloto con un clic en «Editar» sin anclaje tras el clic (falso éxito según el cliente, hallazgo de seguridad): la serie puntuada no se ejecutó; 0 corridas puntuadas, 3 solicitudes reales (cuaderno 396 → 399), sin archivo de parada. Informe §«Quinta ronda» y revisión en [`reviews/2026-10-14-s4-fifth-round.md`](reviews/2026-10-14-s4-fifth-round.md) | Abierto |

| — | RNF-05, latencia de la percepción incremental (rama `perf/rnf05-latency`, sobre `adfb1ea`, CHANGELOG 1.0.22, ADR-0019): las etapas de una observación (detección, OCR, accesibilidad) corren en hilos acotados y se unen antes de fusionar (`[engine] concurrent_stages`); red de `icon_detect_v3` en float16 con la decodificación en float32 (`export_omniparser_v3.py --fp16`, manifiesto `model_manifest.rnf05.json`, sólo DirectML, equivalente al float32 salvo cajas en el umbral); entradas más pequeñas para regiones pequeñas medidas y descartadas (cambian las detecciones). Con 30 procesos por variante: refresco completo en caliente p95 240 → 98 ms, una región 195 → 102 ms, dos regiones 310 → 153 ms (**RNF-05 no cumplido por 3 ms con dos regiones**). La aplicación de prueba tiene banderas ocultas `--animate` y `tools/perf/measure_latency.py` mide sin entrada. Informe en [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §10 | Abierto |
| — | S4, correcciones del cliente tras la quinta ronda (rama `fix/s4-driver-grounding`, CHANGELOG 1.0.23): el clic que cierra el menú «Editar» se ancla por identidad (hallazgo 2); cada paso registra por qué no halló objetivo (hallazgo 3); el paso del menú registra un diagnóstico de sólo lectura (hallazgo 1, sin cambiar la regla); análisis del cierre `indeterminate` (hallazgo 4, sin cambio del motor); sin ejecución en vivo. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Preparación de la sexta ronda» y revisión en [`reviews/2026-10-14-s4-fifth-round.md`](reviews/2026-10-14-s4-fifth-round.md) | Abierto |
| — | S4, anclaje de los clics dentro de una isla XAML (rama `fix/s4-island-grounding`, CHANGELOG 1.0.25): por decisión del responsable, cuando la consulta por posición devuelve el anfitrión de una isla XAML (`Microsoft.UI.Content.DesktopChildSiteBridge`, lista cerrada) el cliente recorre su subárbol, acotado a 500 elementos y 16 niveles, y ancla el clic en el elemento en pantalla más profundo bajo el punto (ambiguo, truncado o ilegible no confirma nada); se aplica en `steps()`, el cierre del menú y el diagnóstico; 113 pruebas (32 nuevas, que fallan con el cliente anterior), comprobación de sólo lectura sobre el Bloc de notas del responsable (alcanza `MenuItem` `Edit`); SHA-256 del cliente `60486d87…c214a`; sin confirmar en vivo; revisión en [`reviews/2026-10-14-s4-fifth-round.md`](reviews/2026-10-14-s4-fifth-round.md) |
| — | S4, séptima ronda (rama `docs/s4-seventh-round`, CHANGELOG 1.0.26): piloto de E2E-03 aprobado, con el descenso por la isla en los dos clics (`MenuItem` `Edit`, 0 anclajes erróneos); nueve de diez corridas puntuadas, todas exitosas y sin falsos éxitos; la décima no se envió porque el cupo de 20 solicitudes no reservaba los reintentos tras `STALE_SNAPSHOT`; `stop.flag` creado por el cliente y sin retirar; E2E-02 no se ejecutó. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Séptima ronda» y revisión en [`reviews/2026-10-14-s4-seventh-round.md`](reviews/2026-10-14-s4-seventh-round.md) | Abierto (decisión del responsable) |
| — | S4, continuación de la séptima ronda (rama `docs/s4-seventh-round-continuation`, CHANGELOG 1.0.27): la corrida 10 de E2E-03 pasó (2 solicitudes), y con ella la serie puntuada queda en diez de diez corridas, todas exitosas y sin falsos éxitos; `--calc-standard` no alcanzó el modo Estándar (el clic de navegación quedó anclado, pero «Estándar» no apareció en UI Automation) y E2E-02 no se ejecutó (3 solicitudes reales en la continuación; cuaderno 426, 25 de 150 de la ronda); `stop.flag` ausente. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Séptima ronda (continuación)» y revisión en [`reviews/2026-10-14-s4-seventh-round.md`](reviews/2026-10-14-s4-seventh-round.md) | Abierto (decisión del responsable) |
| — | S4, preparación de la octava ronda (rama `fix/s4-calc-standard-and-ids`, CHANGELOG 1.0.28, sin ejecución en vivo): `--calc-standard` sondea UI Automation (3 s, sólo lectura) por el elemento con `AutomationId` `Standard`, acepta sólo los nombres `Estándar` y `Calculadora estándar`, y exige que el clic llegue a ese mismo elemento; registra `mode_after` en todo desenlace y la espera de primer plano; el registro sale en UTF-8 (cliente SHA-256 `a16c53af…ae22d`, 143 pruebas, las 30 nuevas fallan sin el cambio). Los flujos reducidos pasan a **S4R-02** (Calculadora) y **S4R-03** (Bloc de notas); la matriz añade ambos y no cambia E2E-02 ni E2E-03. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Preparación de la octava ronda» | Pendiente la ronda en vivo (cuaderno 426) |
| — | S4, preparación de E2E-02 (editor) y E2E-03 (Explorador) del dossier (rama `feat/e2e-editor-explorer`, CHANGELOG 1.0.29, [ADR-0020](adr/0020-operator-scope-on-windows.md), aprobada el 14 de octubre de 2026): editor propio `tools/s4/testeditor` (`ommatix-testeditor.exe`, guarda sólo `nota.txt` en `%TEMP%\ommatix-e2e\<id>`), cliente `tools/s4/e2e_dossier.py` y alcance por ventana (`[safety] allowed_window_classes` y `window_pin_file`, apagados por defecto) para permitir `explorer.exe` sólo en la ventana que abre el cliente; UT-45, BDD-36 y BDD-37. **Sin ejecución en vivo ni entrada.** Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Preparación de E2E-02 y E2E-03 del dossier» | Abierto (decisión del responsable) |
| — | S4, octava ronda (rama `docs/s4-eighth-round`, CHANGELOG 1.0.31): una solicitud real (cuaderno 426 → 427, de 200); S4R-02 no alcanzó el modo Estándar (`unknown_name`: el elemento tiene `AutomationId` `Standard`, pero su nombre no está en la lista cerrada); E2E-02 y E2E-03 no llegaron al primer plano maximizado en sus pilotos y sondeos, así que no se enviaron entradas. Ninguna corrida puntuada. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Octava ronda» y revisión en [`reviews/2026-10-14-s4-eighth-round.md`](reviews/2026-10-14-s4-eighth-round.md) | Abierto (decisiones del responsable) |
| — | S4, los tres bloqueos de la octava ronda (rama `fix/s4-round8-blockers`, CHANGELOG 1.0.32, sin ejecución en vivo ni entrada): `--calc-standard` identifica «Estándar» por `AutomationId` `Standard` sin lista de nombres (el nombre sólo en memoria); la ventana se pide por su identificador con tres métodos sin entrada, verificados y registrados, con diagnóstico de sólo lectura y `--operator-activation` apagado por defecto; el editor se maximiza y pide el foco, y el cliente se detiene antes de toda entrada si el motor no ve accesibilidad (clientes SHA-256 `cbe6905c…1265` y `c1ba4a37…020e`, editor `0dcfb86b…4e78`, 230 pruebas). Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Preparación de la novena ronda» | Pendiente la ronda en vivo (cuaderno 427) |
| — | S4, novena ronda (rama `docs/s4-ninth-round`, CHANGELOG 1.0.33): en vivo, dos solicitudes reales (cuaderno 427 → 429) en `--calc-standard`; el segundo clic («Estándar») se entregó sin quedar anclado y, por la regla de seguridad del responsable, se detuvo todo el trabajo en vivo. Sin corridas puntuadas; E2E-02 y E2E-03 no se ejecutaron. Matrices sin cambio |
| — | S4, continuación de la novena ronda (rama `docs/s4-ninth-round-continuation`, CHANGELOG 1.0.34): ninguna solicitud real (cuaderno sigue en 429, de 200). S4R-02 no se ejecutó: la comprobación de modo, de sólo lectura, dio «otro» en dos lecturas. El piloto de E2E-02 no pasó sin entrada (sin evidencia de accesibilidad: el foco de teclado lo tiene el escritorio). E2E-03 no se ejecutó: el sondeo no encuentra los objetivos de navegación como el flujo los define. Ninguna corrida puntuada; matrices sin cambio. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Novena ronda (continuación)» |
| — | S4, bloqueos de la novena ronda (rama `fix/s4-round9-blockers`, CHANGELOG 1.0.36, sin ejecución en vivo ni entrada; cuaderno 429): señal de modo de la Calculadora por «contiene estándar/standard» (nombre real desconocido), `--check-mode` de sólo lectura, `--calc-standard --max-live 0` rechazado, contadores y espera de `mode_after` en el resumen, foco de Win32 del editor (verificado en lectura) y flujo del Explorador desde `carpeta-b` (miga de pan, miga de pan, «Atrás»; zona sólo registrada). 258 pruebas de `tools/s4`, 21 del editor. Hashes en `docs/thesis/S4_ACEPTACION_REDUCIDA.md`, «Preparación de la décima ronda». | Hecho (sin verificación en vivo) |
| — | Pruebas de la Definition of Done que no necesitan entrada real ni pantalla (rama `feat/dod-offline-tests`, CHANGELOG 1.0.35, [ADR-0021](adr/0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md), aprobada el 14 de octubre de 2026): IT-06, SEC-03, SEC-04, SEC-06, PERF-01 y DOC-01 en Rust (`IMPLEMENTADO_EN_RUST_S4`) y REL-01 con sus comprobaciones y su guía (sigue `PENDIENTE_PRODUCTO`: falta el corpus de aceptación). Defecto hallado por PERF-01 y corregido: la asociación de identidad normalizaba cada etiqueta una vez por par de pista y observación (57 de los 60 ms de una observación en caliente de 300 controles y 600 textos); ahora p95 del núcleo ≤ 4 ms (3,97 ms; RNF-04, [`S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §11). Otros hallazgos: los pesos se comprobaban después de cargar el runtime, el archivo de íconos curados no tenía hash (`[cache] prototypes_sha256`, obligatorio), `Unknown tool` y `Method not found` repetían texto del agente, UT-45 no tenía ninguna prueba con su identificador. Toolchain fijado en `1.99.0` (CI incluida); `validar_paquete.py` comprueba DOC-01 (449 comprobaciones). Informe en [`thesis/S4_REPRODUCIBILIDAD.md`](thesis/S4_REPRODUCIBILIDAD.md) | Abierto |
| — | S4, bloqueos de la décima ronda (rama `fix/s4-round10-blockers`, CHANGELOG 1.0.39, sin ejecución en vivo ni entrada; cuaderno 490): el cursor de texto del editor deja de parpadear (causa del `STALE_SNAPSHOT` tras escribir, verificada en el toolkit) y el recorrido de accesibilidad lee primero la ventana con el foco y por niveles (causa de que las migas de pan del Explorador no llegaran al motor, verificada por reproducción). Hallazgo para el responsable: un cursor parpadeante en una aplicación real rechazaría toda acción posterior a escribir |
| — | S4, continuación de la décima ronda (rama `docs/s4-tenth-round-continuation`, CHANGELOG 1.0.40; cuaderno 494, de 200): E2E-02 no pasó su piloto (un `STALE_SNAPSHOT` antes de entregar ninguna tecla; serie bloqueada) y E2E-03 se detuvo por la parada de seguridad en su primer clic (el control bajo el cursor no era el identificado). Sin cambio de código ni de matriz. |
| — | S4, serie de E2E-02 de la décima ronda (rama `docs/s4-tenth-round-series`, CHANGELOG 1.0.41; cuaderno 515, de 200): diez corridas puntuadas de diez, cero falsos éxitos, en diez invocaciones de una corrida con compuerta entre ellas; E2E-03 no se ejecutó. Matriz: E2E-02 `EJECUTADO_SUPERVISADO_PARCIAL`, propuesta en el PR para la aprobación del responsable. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Décima ronda (serie)» | Pendiente la aprobación del PR |
| — | S4, la regla de anclaje de los clics que reconstruyen su propio control (rama `fix/s4-navigating-click-grounding`, CHANGELOG 1.0.42, [ADR-0022](adr/0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md), aceptada el 14 de octubre de 2026; sin ejecución en vivo ni entrada; cuaderno 494): antes de cada solicitud el cliente ancla el punto objetivo (el elemento identificado o su padre, en la ventana esperada) o no envía nada (`pre_grounded`); para la lista cerrada `NAVIGATING_STEPS = {"explorer": (1, 2, 3)}` la identidad posterior se registra (`post_grounded`) y decide la comprobación del sistema; los demás pasos no cambian. Clientes `e2e_driver.py` `768e77e3…97202d` y `e2e_dossier.py` `359f6f28…0afc2`, 297 pruebas (41 fallan sin el cambio). Causa de la parada no verificada. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) §«Preparación de la undécima ronda: E2E-03» y revisión `reviews/2026-10-14-s4-tenth-round.md`, hallazgos 13 (de la serie), 15 y 16. |
| — | S4, undécima ronda (rama `docs/s4-eleventh-round`, CHANGELOG 1.0.43; cuaderno 515, de 200 desde 429): E2E-03 no llegó a la sonda. El preflight de sólo lectura halló que las dos ventanas minimizadas del shell (PID 36160) que el responsable confirmó como línea base ya no existen, y que hubo una entrada de teclado o ratón entre dos comprobaciones; no se envió ninguna entrada ni solicitud. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Undécima ronda», y revisión `reviews/2026-10-14-s4-eleventh-round.md`. | Pendiente la decisión del responsable (equipo en uso y línea base) |
| — | S4, continuación de la undécima ronda (rama `docs/s4-eleventh-round-run`, CHANGELOG 1.0.44; cuaderno 515, de 200 desde 429): la línea base nueva del responsable (ninguna ventana `CabinetWClass` visible; la oculta del PID 36160 intacta) cumple la comprobación previa, pero el contador de inactividad marca una entrada de teclado o ratón hacia las 00:23 (hora local), después de su respuesta; por la regla, sin sonda, piloto ni serie. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Undécima ronda (continuación)», y revisión `reviews/2026-10-14-s4-eleventh-round.md`, hallazgos 3 y 4. | Pendiente la respuesta del responsable: ¿estaba el equipo en uso hacia las 00:23? |
| — | S4, segunda continuación de la undécima ronda (rama `docs/s4-eleventh-round-run2`, CHANGELOG 1.0.45; cuaderno de 515 a 530, de 200 desde 429): E2E-03 se ejecutó en vivo por primera vez. La sonda cumple la compuerta y el piloto pasa (3 de 3 comprobaciones del sistema, cero falsos éxitos, cero controles erróneos); las corridas puntuadas 2 a 4 pasan; la corrida 5 no se envió, porque el contador de inactividad marcó entrada que la ronda no provocó entre invocaciones. Matriz: E2E-03 sigue `PENDIENTE_PRODUCTO` (3 de 10). Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Undécima ronda (continuación 2)», y revisión `reviews/2026-10-14-s4-eleventh-round.md`, hallazgos 5 a 9. | Pendiente la respuesta del responsable: ¿fue suya la entrada entre las 00:56 y las 00:58? ¿Reanuda la serie (corridas 5 a 11)? |
| — | S4, tercera continuación de la undécima ronda (rama `docs/s4-eleventh-round-run3`, CHANGELOG 1.0.46; cuaderno de 530 a 558, de 200 desde 429): la serie de E2E-03 se completa. Corridas 5 a 11: siete de siete pasan, con tres comprobaciones del sistema cada una, cero falsos éxitos y cero controles erróneos (diez de diez corridas puntuadas). Matriz: E2E-03 pasa a `EJECUTADO_SUPERVISADO_PARCIAL`. Dos compuertas detuvieron el envío antes de cualquier solicitud; la coordinación atribuyó la segunda entrada al responsable y decidió reanudar. Informe en [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Undécima ronda (continuación 3)», y revisión `reviews/2026-10-14-s4-eleventh-round.md`, hallazgos 10 a 14. | Pendiente: que el responsable confirme la atribución de la entrada de las 01:40 y la revisión de la corrida 11 (132 s) |
| — | Preparación de la evidencia que exige la máquina del responsable (rama `feat/dod-machine-evidence-prep`, CHANGELOG 1.0.47, sin ejecutar nada ni enviar entrada): guía de instalación y reversión ([`S4_INSTALACION_Y_REVERSION.md`](thesis/S4_INSTALACION_Y_REVERSION.md), REL-01), `tools/perf/soak.py` (PERF-03 / RNF-06: recursos y soak de 2 h) y `tools/perf/dpi_probe.py` (IT-07: DPI 100/125/150 %), ambas de sólo lectura y con pruebas sobre dobles, y [`S4_SESIONES_PENDIENTES.md`](thesis/S4_SESIONES_PENDIENTES.md) con lo que cada sesión supervisada necesita del responsable. Prueba `rel_01_the_install_guide_points_at_files_that_exist` y `tools/repro/test_install_guide.py`. Ninguna fila de las matrices ni casilla cambia. | PR abierto |
| — | Alcance del MVP y licencia (rama `docs/mvp-scope-and-licence`, CHANGELOG 1.0.52, [ADR-0023](adr/0023-licence-and-reduced-mvp-scope.md), decisiones del responsable del 15 de octubre de 2026): código `MIT OR Apache-2.0` y documentos CC BY 4.0 (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-DOCS`); propietario de los 28 requisitos; RNF-05 aceptado como incumplimiento documentado; el protocolo reducido de S4 sustituye las 450 corridas de EV-02 para este MVP; EV-01, EV-02, IT-07 y el corpus de REL-01 pasan al próximo corte; reevaluación de la Definition of Done con 2 de 10 casillas marcadas y su evidencia (capítulo 06 §6.7). Prueba `rel_01_the_repository_declares_its_licences`. Ninguna matriz cambia. | PR abierto |
| — | INC-03, vigencia del snapshot y de la evidencia de accesibilidad (rama `feat/inc03-ttl`, CHANGELOG 1.0.49, [ADR-0025](adr/0025-bound-authorisations-by-snapshot-and-evidence-age.md), aprobada el 15 de octubre de 2026 con la aprobación previa del responsable para el siguiente corte): `max_snapshot_age_ms` se aplica en la puerta, también en simulación, y antes de cada entrada real (antes del clic y, en `type`, antes de las teclas), con 10 000 ms por defecto; vencido el plazo, `STALE_SNAPSHOT` y observar de nuevo. Una publicación nueva no renueva una lectura de accesibilidad antigua; la siguiente observación relee las regiones caducadas. UT-46 a UT-54 y BDD-38; cada comprobación, quitada, hace fallar sus pruebas. **Sin ejecución en vivo ni entrada**; la medición de la relectura y la calibración del valor quedan para el responsable. | Abierto (medición pendiente) |
| — | Borrado de estado y prueba diferencial (rama `feat/dod-deletion-and-differential`, CHANGELOG 1.0.53): al terminar la sesión MCP el motor borra su snapshot, evidencia, cuadro base, regiones pendientes y diario de `request_id`, sin reutilizar identificadores (SEC-07, BDD-39; capítulo 08 §8.4). La prueba diferencial `ut_08_differential_tile_diff_matches_scalar_reference_on_generated_frames` compara `tile_changed` con una referencia escalar y no encuentra defectos (capítulo 05 §5.5). §6.7: se marca la casilla 5; la 7 sigue abierta (SEC-02 en parte y borrado bajo demanda). |
| — | Sesión B, PERF-03 / RNF-06 (rama `docs/perf03-soak`, CHANGELOG 1.0.55): soak de 120 minutos completo con `tools/perf/soak.py` sobre la aplicación de prueba, sin entrada y sin umbral: 1440 de 1440 observaciones, cero pérdidas de primer plano, memoria privada del motor +1,6 MiB/h, cola de regiones pendientes en cero. Matriz: PERF-03 `EJECUTADO_SUPERVISADO_PARCIAL`; RNF-06 sin veredicto hasta que el responsable fije el umbral. Informe en [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §12. | Pendiente la aprobación del PR y el umbral del responsable |
| — | INC-05, enumerador R*-tree experimental de los candidatos del Algoritmo 2 (rama `feat/inc05-rstar-index`, CHANGELOG 1.0.51, [ADR-0027](adr/0027-candidate-enumeration-with-an-rstar-index.md)): `[engine] candidate_search = "rstar"`, desactivado por defecto, con los mismos resultados que el recorrido. Medición del núcleo en [`thesis/INDICE_A2_RESULTADOS.md`](thesis/INDICE_A2_RESULTADOS.md): G4 cumplido en cuadrícula y formularios desde 512 controles; G5 incumplido en dos celdas de solapamiento con 128 controles. No se adopta como valor por defecto. | PR abierto |
| — | INC-02, B0 frente a B1 en vivo y PERF-01 sin carga (rama `docs/inc02-b0b1`, CHANGELOG 1.0.56, [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §13 y [`thesis/09_EVALUACION_EXPERIMENTAL.md`](thesis/09_EVALUACION_EXPERIMENTAL.md) §9.9.1): 240 procesos en vivo sobre la aplicación de prueba, sin entrada. Con una región B1 tarda 18,6 ms menos; con dos, 45,8 ms más, porque cada región es una inferencia completa. Con dos regiones B1 publica 67 elementos por observación frente a 85, por investigar. PERF-01 sin carga no reproduce el desplazamiento de p95 de +0,2 a +0,35 ms. RNF-05 sin cambio. | PR abierto |
| — | Seguimiento de INC-02 (rama `fix/inc02-region-loss`, CHANGELOG 1.0.57, [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §13.10 a §13.13 y [`thesis/09_EVALUACION_EXPERIMENTAL.md`](thesis/09_EVALUACION_EXPERIMENTAL.md) §9.9.1): la pérdida de elementos con dos regiones (B1 publica ≈ 20 de 75 controles menos) **no es un defecto del Algoritmo 1**: el cierre del halo no corta ninguna caja; es el comportamiento del detector sobre recortes (escala 1,0 frente a 0,67 del fotograma completo y falta de contexto: la bandeja de 14 iconos se vuelve una caja del tamaño del recorte). B0/B1 repetido con `[models] accessibility = "uia"` (240 procesos, sin entrada): con una región B1 ya no ahorra (+2,4 ms; la lectura de UIA, ≈ 117 ms, fija el total) y con dos regiones es +46,6 ms más lenta, con p95 de 186,3 ms (RNF-05 sin cambio de estado). Corregido el `-0.0` de `region_area_px` en observaciones inactivas, con prueba. Pendiente (no autorizado): INC-07, política adaptativa entre refresco completo y regional. | PR abierto |
| — | Reevaluación de la Definition of Done contra `origin/main` (rama `docs/dod-reevaluation-2`, CHANGELOG 1.0.58, [`thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md`](thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md) §6.7): siguen marcadas tres casillas (captura, diferenciador y lockfiles). Se refrescan las evidencias de `requisitos_trazabilidad.csv` sin cambiar ningún estado, porque el validador sólo admite `PENDIENTE_IMPLEMENTACION_O_VALIDACION`. Se corrigen E2E-03 (diez de diez) y los 52 escenarios BDD, y se añaden las ocho decisiones pendientes del responsable. Sin ejecución en vivo ni entrada. |
| — | Sesión B con `uia` sobre `main` (rama `docs/perf03-soak-uia`, CHANGELOG 1.0.59, [`thesis/S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §14): intento bloqueado antes de la corrida de 120 minutos. El ensayo de un minuto no capturó la pantalla (`CAPTURE_UNAVAILABLE` en todas las observaciones, con y sin `uia`); causa probable, no verificada: el apagado de pantalla de 15 minutos. Sin cifras; PERF-03 y RNF-06 sin cambio. Desviaciones declaradas: auxiliar de primer plano fuera del repositorio y `--foreground-wait 30`. | PR abierto |
| — | Borrado bajo demanda del operador (rama `feat/operator-purge`, CHANGELOG 1.0.60, [ADR-0028](adr/0028-operator-purge-file.md), aprobada el 15 de octubre de 2026): `[safety] purge_file`, apagado por defecto. Al crearlo, el motor borra su estado y su caché en la siguiente petición, una vez por cada archivo nuevo, sin terminar la sesión y sin borrar el archivo (SEC-07, BDD-53; [capítulo 08 §8.4](thesis/08_SEGURIDAD_Y_PRIVACIDAD.md)). Pruebas sobre dobles y archivos temporales, **sin ejecución en vivo**. Aprobar la ADR-0028 es decisión del responsable (§6.7, decisión 2). | PR abierto |
| — | Umbral del responsable para RNF-06 (rama `docs/rnf06-threshold`, CHANGELOG 1.0.62; [capítulo 09 §9.7](thesis/09_EVALUACION_EXPERIMENTAL.md)): conjunto de trabajo de no más de 10 MiB/h y manejadores sin crecimiento, por defecto en `tools/perf/soak.py` (`criteria` juzga esos dos; la cota de la cola sigue sin fijar, decisión 9). La corrida de [S0 §12](thesis/S0_RESULTADOS.md) los cumple en sus condiciones, pero el umbral se fijó después de leer su pendiente y no es un veredicto sobre RNF-06. Casilla 7 sin marcar. Sin ejecución en vivo ni entrada. | PR abierto |

Las matrices de la tesis (`docs/thesis/matrices/`) y su `CHANGELOG.md` (1.0.1–1.0.61)
registran qué pruebas planificadas existen en Rust. Los requisitos RF/RNF siguen pendientes de
aceptación: eso exige capturas, modelos y actuador reales en corridas supervisadas (S3–S4).
S0 ejecutó en parte IT-01 e IT-02; las corridas supervisadas de S3 (1.0.9 y 1.0.12) ejecutan SEC-01 y,
en parte, IT-01, IT-03, IT-04, IT-05, SEC-02 y SEC-05. IT-06, SEC-03, SEC-04 y SEC-06 están
implementadas en Rust sobre dobles, el escritorio simulado y archivos temporales (1.0.35), sin
corridas supervisadas; IT-07 no se ha ejecutado.
El protocolo S4 reducido (1.0.13) ejecuta en parte E2E-01 y PERF-02, con 20 corridas sólo
con clics. No son las 450 del protocolo (EV-02), y RF-18, RNF-03 y RNF-05 siguen pendientes.
La segunda ronda (1.0.15) confirma por nombre de accesibilidad los controles de E2E-01 y E2E-02
en sondeos sin entrada, pero la escritura de E2E-01 falló en sus corridas puntuadas (0 de 3), la
serie se detuvo con el archivo de parada creado y E2E-02 y E2E-03 no se han ejecutado en vivo.
La tercera ronda (1.0.17) ejecuta `type` en dos fases en vivo: E2E-01 pasa 10 de 10 corridas
puntuadas con la escritura verificada por el título, sin teclas perdidas. E2E-02 no puede cumplir
su paso final tal como está especificado (no hay un botón «Borrar» tras «Es igual a» en la
Calculadora), así que su serie se detuvo tras tres fallos, y E2E-03 no se puntúa tras dos
pilotos que no produjeron de forma fiable el menú «Editar». El archivo de parada lo creó el
cliente y lo retira el responsable.
La cuarta ronda (1.0.19) repite el protocolo con las correcciones de 1.0.18: E2E-01 pasa 10 de 10
corridas puntuadas, con 0 teclas perdidas y 0 falsos éxitos. E2E-02 no se puntúa: la Calculadora
sigue en modo Científica, sin control de borrado tras «Es igual a»; el cambio a modo Estándar no se
completó, porque el clic de navegación de la contingencia no quedó anclado al control pedido, y la
serie se abandonó. E2E-03 no se ejecutó por ese mismo desajuste. El archivo de parada no existe: se
retiró una vez al inicio, como autorizó el responsable, y no se creó otro.

La quinta ronda (1.0.21) no puntúa ninguna corrida. E2E-02 no se ejecutó: el cambio a modo Estándar, que pulsa el conmutador de navegación anclado por identidad, se detuvo en el paso «Estándar», que no encontró un objetivo confirmado, y sin Estándar la serie no se lanza según el protocolo. E2E-03 tuvo un piloto cuyo clic en «Editar» quedó sin anclaje confirmado tras el clic, con el efecto informado como `effect_observed`; por tratarse de un hecho de seguridad, la serie puntuada no se ejecutó. La ventana del propietario se restauró y el archivo de parada no existió en ningún momento. Quedan abiertos cuatro hallazgos: la causa del anclaje en «Editar» (decisión del responsable), el cierre del menú sin anclaje por identidad, el registro insuficiente del paso «Estándar» y un cierre del menú `indeterminate`.

Después de la quinta ronda (1.0.23) se corrigió el cliente sin ejecución en vivo (SHA-256 `6dad0dc3…8e2a77`): ancla el cierre del menú, registra por qué un paso no halla objetivo y diagnostica, de sólo lectura, el menú «Editar». Decisiones del responsable del 2026-10-14: (a) las corridas de S4 usan desde ahora el manifiesto float16 de RNF-05 (`model_manifest.rnf05.json`, S0 §10); (b) no se cambia la enumeración de ventanas de ADR-0017 para llegar a la meta de dos regiones, así que RNF-05 sigue sin cumplirse por unos 3 ms con dos regiones; (c) el responsable estará presente para un clic de diagnóstico de sólo lectura sobre «Editar» (hallazgo 1) después de fusionar el PR: se usa la invocación de [`S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md) con `--max-live 2` (nunca 1, que crearía `stop.flag`); ningún agente lo ejecuta. Sigue abierto el conflicto del protocolo sobre `stop.flag` (dos opciones en la revisión de la quinta ronda).

La sexta ronda (1.0.24) ejecuta una corrida de diagnóstico de sólo lectura sobre «Editar» (dos solicitudes reales, cuaderno 399 → 401, no puntuada; SHA-256 del binario `11a3d792…aca9da`). El registro refuta que el identificador de «Editar» cambie al expandirse y no respalda que el clic llegara a otro control: la consulta de UI Automation por posición no devuelve «Editar» ni antes ni después del clic, así que el anclaje por posición no puede confirmarlo en este Bloc de notas. E2E-02 y E2E-03 siguen sin ejecutar y el archivo de parada no existe. Decide el responsable si el propio elemento puede dar fe del clic (ver [`S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Sexta ronda»).

La séptima ronda se prepara en `fix/s4-island-grounding` (1.0.25, sin ejecución en vivo): el responsable decidió «descender dentro de la isla». Cuando la consulta por posición devuelve el anfitrión de una isla XAML, el cliente recorre su subárbol (500 elementos y 16 niveles como máximo) y ancla el clic en el elemento en pantalla más profundo bajo el punto; ambiguo, truncado, ilegible o con otro anfitrión no confirma nada, y no hay anclaje por estado, comparación de nombres ni reintento. SHA-256 del cliente `60486d87…c214a`; 113 pruebas. Una comprobación de sólo lectura sobre el Bloc de notas del responsable alcanza `MenuItem` `Edit` con su RuntimeId. Falta la ronda en vivo (piloto con `--max-live 2`, `--round-start 401`) que confirme la consulta por posición y mida el tiempo del recorrido (ver [`S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Preparación de la séptima ronda»).

La séptima ronda (1.0.26) ejecuta en vivo el piloto de E2E-03, que pasa: el descenso por la isla ancla los dos clics de «Editar» (profundidad 3, `Edit`), sin anclajes erróneos. Después, nueve de las diez corridas puntuadas, todas exitosas y sin falsos éxitos. La décima no se envió: el cupo de la serie (20 solicitudes) no reservaba los reintentos tras `STALE_SNAPSHOT`, dos corridas los usaron, y el cliente creó `stop.flag`, que el responsable debe retirar. `--calc-standard` y E2E-02 no se ejecutaron. Cuaderno 423. Decisiones pendientes del responsable: retirar `stop.flag`, fijar el cupo contando los reintentos y decidir la correspondencia de los casos de la matriz (ver [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Séptima ronda»).

La continuación de la séptima ronda (1.0.27) ejecuta la corrida 10 de E2E-03, que pasa: con ella la serie puntuada queda completa, diez de diez corridas, todas exitosas y sin falsos éxitos. `--calc-standard` no alcanzó el modo Estándar: el clic de navegación quedó anclado, pero «Estándar» no apareció en UI Automation en unos 1,5 s y el paso se detuvo sin segundo clic. Por eso E2E-02 no se ejecutó. Cuaderno 426 (25 de 150 solicitudes de la ronda); `stop.flag` ausente. Decisiones pendientes del responsable: cómo llegar al modo Estándar (con un cambio del cliente y su prueba, en una ronda aparte) y fijar el cupo de E2E-02 contando los reintentos (ver [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Séptima ronda (continuación)»).

La octava ronda se prepara en `fix/s4-calc-standard-and-ids` (1.0.28, sin ejecución en vivo). **Identificadores:** los flujos del protocolo reducido son **S4R-02** (Calculadora: 7 + 8 = 15 y borrar) y **S4R-03** (Bloc de notas: abrir y cerrar «Editar»); donde los informes anteriores dicen «E2E-02» o «E2E-03 del protocolo reducido», son ésos. E2E-02 (editor con escritura y guardado) y E2E-03 (explorador) del dossier siguen pendientes. **Evidencia puntuada:** la aplicación de prueba (E2E-01, parcial) y el Bloc de notas (S4R-03, diez de diez, sólo clics); falta la Calculadora (S4R-02), el editor, el explorador, otros DPI y temas y las 450 corridas, y RF-18 y la Definition of Done siguen abiertos. **Cliente:** tras el clic de navegación, `--calc-standard` sondea UI Automation por el elemento con `AutomationId` `Standard` (hipótesis no verificada en vivo) y sólo hace el segundo clic si es único, con un nombre de la lista cerrada y el mismo RuntimeId que el elemento del motor. Falta la ronda en vivo: `--calc-standard` con cupo 4 y `--round-start 426`, comprobando antes el SHA-256 `a16c53afca63e21f629140ab33951f481b75dfd113866720ab947aa8272ae22d`.

La preparación de E2E-02 y E2E-03 del dossier (1.0.29, rama `feat/e2e-editor-explorer`) no ejecuta nada en vivo. Añade el alcance por ventana de la ADR-0020 (aprobada por el responsable el 14 de octubre de 2026), el editor propio y el cliente de las dos pruebas, con UT-45, BDD-36 y BDD-37. E2E-02 y E2E-03 de la matriz siguen `PENDIENTE_PRODUCTO`. El responsable aprobó la ADR-0020 y autorizó la octava ronda (S4R-02, E2E-02 y E2E-03, con un cupo de 200 solicitudes y sin usar el equipo). Queda por decidir qué hacer si el panel de navegación del Explorador no permite bajar con un clic sencillo (el contrato no tiene doble clic); el cupo; el entorno. El sondeo (`--mode probe`) dirá qué ve el motor en el panel de navegación sin enviar entrada.

La octava ronda (1.0.31, rama `docs/s4-eighth-round`) se ejecutó en vivo con una sola solicitud real (cuaderno 426 → 427, de 200) y sin corridas puntuadas. `--calc-standard` halló «Estándar» por `AutomationId` `Standard` (un `ListItem`), pero su nombre no está en la lista cerrada del cliente y el paso se detuvo sin clic. E2E-02 y E2E-03 no llegaron al primer plano maximizado en sus pilotos y sondeos (el editor en tres de cuatro intentos; el Explorador en su sondeo), así que no se enviaron entradas ni se abrieron las series. Con el sondeo que sí llegó, el motor tampoco expone con etiqueta de accesibilidad el campo del editor. Quedan decisiones del responsable: el nombre exacto del elemento Estándar, el diagnóstico del primer plano, la etiqueta del campo del editor y la regla del piloto. `stop.flag` no existe y el cuaderno está en 427. Las matrices no cambian (ver [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Octava ronda»).

Los tres bloqueos de la octava ronda se corrigen en `fix/s4-round8-blockers` (1.0.32, sin ejecución en vivo). La ronda en vivo con esos hashes se ejecutó después, en la novena ronda (1.0.33; el editor cambió de hash, `541fc0a8…2793`; el motor no). La regla del piloto está decidida: un piloto que no pasa, por cualquier causa, bloquea su serie. La causa de la etiqueta del editor sigue sin verificarse (el árbol del editor y el adaptador están descartados).

La novena ronda (1.0.33, rama `docs/s4-ninth-round`) se ejecutó en vivo y se detuvo pronto: dos solicitudes reales (cuaderno 427 → 429, de 200), las dos de `--calc-standard`. El sondeo de la Calculadora confirmó sus cinco elementos y `--calc-standard` halló «Estándar» por `AutomationId`, pero el segundo clic se entregó sin quedar anclado; por la regla de seguridad del responsable se detuvo todo el trabajo en vivo. No hubo piloto ni serie, y E2E-02 y E2E-03 no se ejecutaron. `stop.flag` no existe y el cuaderno está en 429. Decide el responsable si la ronda continúa y cómo se investiga el segundo clic (ver [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Novena ronda», y la [revisión](reviews/2026-10-14-s4-ninth-round.md)).

La continuación de la novena ronda (1.0.34, rama `docs/s4-ninth-round-continuation`) no envió ninguna solicitud real: el cuaderno sigue en 429 (2 de 200). La comprobación de modo de la Calculadora, de sólo lectura, dio «otro» en dos lecturas, así que S4R-02 no se ejecutó; no se usó `--calc-standard --max-live 0`, que crea `stop.flag` si el modo no es Estándar. El piloto de E2E-02 no pasó sin entrada: UI Automation indica al escritorio como propietario del foco de teclado y el editor no expone elementos de accesibilidad. E2E-03 no se ejecutó: el sondeo no encuentra los objetivos de navegación como el flujo los define. Hallazgos 8 a 13 en la revisión de la ronda. Decide el responsable: el diagnóstico del encabezado de la Calculadora, el foco de teclado del editor, el contrato de la navegación del Explorador y si la ronda continúa (198 solicitudes con `--round-start 427`, o una ronda nueva desde 429).

Los bloqueos de la novena ronda se corrigen en `fix/s4-round9-blockers` (1.0.36, sin ejecución en vivo ni entrada; cuaderno 429, sin cambio). La señal de modo de la Calculadora acepta un encabezado que contenga «estándar» o «standard» (el nombre real que lee la Calculadora cambiada a mano **no se conoce**; el registro guarda tipo, cuenta y longitud para saberlo). `--check-mode` lee el modo sin motor, entrada, cuaderno ni `stop.flag`; `--calc-standard --max-live 0` se rechaza al leer los argumentos. El editor obtiene el foco de teclado de Win32 (la causa del hallazgo 10: ventana en primer plano sin ventana con foco, UI Automation informaba al escritorio), verificado en lectura con el editor propio. Hallazgo 14 (seguridad, mayor): `ViewportCommand::Focus` hacía que winit enviara una tecla Alt sintética (PR #12 en la aplicación de prueba, PR #19 en el editor); eliminada de las dos, con una prueba en cada una; era la causa probable del hallazgo 10. El Explorador empieza en `carpeta-b` y hace miga de pan, miga de pan y «Atrás», con objetivos por tipo de control y rol `button`; su comportamiento en vivo no está verificado. Pendiente del responsable: la alternativa de atestiguar por la ventana en primer plano en el motor (ADR-0018), el hallazgo 1 y si la ronda continúa.

La décima ronda (1.0.38, rama `docs/s4-tenth-round`) se ejecutó en vivo con 61 solicitudes reales (cuaderno 429 → 490, de 200). La comprobación de modo de la Calculadora dio `standard` (encabezado «Modo de calculadora Estándar»). S4R-02 pasó su piloto y sus diez corridas puntuadas, sin falsos éxitos; la matriz lo registra como `EJECUTADO_SUPERVISADO_PARCIAL`. E2E-02: el sondeo leyó 6 elementos de accesibilidad, pero el piloto no pasó (el clic de «Guardar» rechazado cuatro veces con `STALE_SNAPSHOT`), así que la serie queda bloqueada. E2E-03: el sondeo no halla «carpeta-a» ni la raíz como candidatos `button` del motor, y no se ejecutó en vivo. Decide el responsable (revisión `docs/reviews/2026-10-14-s4-tenth-round.md`, hallazgos 1 y 2). Cuaderno: 490, con 139 de 200 disponibles bajo `--round-start 429`. `stop.flag` y `window.pin` ausentes.

Los bloqueos de la décima ronda se corrigen en `fix/s4-round10-blockers` (1.0.39, sin ejecución en vivo ni entrada; cuaderno 490, sin cambio). **E2E-02:** el toolkit del editor parpadea el cursor de texto por defecto y el motor rechaza toda entrega si el fotograma previo difiere de la base, así que el clic de «Guardar» tras escribir daba `STALE_SNAPSHOT`; el editor fija el cursor sin parpadeo, con pruebas, y el motor no cambia (no verificado en una ventana en vivo). **E2E-03:** el adaptador de accesibilidad gastaba unos 50 ms del presupuesto de 250 ms en listar las ventanas de la sesión y recorría en profundidad, así que no llegaba a las migas de pan del Explorador (elemento 83 de 118); ahora lee primero la ventana con el foco y por niveles: con el escritorio en reposo las migas salen con 100 ms de presupuesto (antes 200 ms). «Atrás» `unknown` en el sondeo es esperable (deshabilitado sin historial). Pendiente del responsable: la tolerancia a cursores parpadeantes en aplicaciones reales (opciones en la revisión `docs/reviews/2026-10-14-s4-round10-blockers.md`, hallazgo 5), el presupuesto de 250 ms y el cupo.

La continuación de la décima ronda (1.0.40, rama `docs/s4-tenth-round-continuation`) envió 4 solicitudes reales (cuaderno 490 → 494, de 200). **E2E-02:** el piloto no pasó su criterio «sin `STALE_SNAPSHOT`»: el primer `type` fue rechazado antes de entregar ninguna tecla; el reintento y el resto se entregaron y verificaron, y la serie queda bloqueada. **E2E-03:** el sondeo cumplió la compuerta (las dos migas, únicas y confirmadas), pero el primer clic del piloto se contó como control incorrecto (hijo de texto de la miga «carpeta-a», no el `SplitButton` identificado) y la regla del responsable detuvo todo el trabajo en vivo. Decide el responsable (hallazgos 7 a 9 de la [revisión](reviews/2026-10-14-s4-tenth-round.md)). Sin cambio de código.

La serie puntuada de E2E-02 de la décima ronda (1.0.41, rama `docs/s4-tenth-round-series`) envió 21 solicitudes reales (cuaderno 494 → 515, de 200) en diez invocaciones de una corrida, cada una con su comprobador de sólo lectura y una compuerta antes. **E2E-02:** diez de diez corridas puntuadas, cero falsos éxitos y veinte de veinte comprobaciones del sistema; una negativa `STALE_SNAPSHOT` antes de cualquier entrada, reintentada. La matriz pasa E2E-02 a `EJECUTADO_SUPERVISADO_PARCIAL` en el PR, para la aprobación del responsable. **E2E-03:** no se ejecutó. **Cliente:** no detiene una invocación de varias corridas ante un evento de seguridad (hallazgo 13 de la [revisión](reviews/2026-10-14-s4-tenth-round.md)); decide el responsable.

La regla de anclaje de los clics que reconstruyen su propio control (1.0.42, rama `fix/s4-navigating-click-grounding`, [ADR-0022](adr/0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md), aceptada) responde al hallazgo 8 sin ejecución en vivo ni entrada (cuaderno 494). Antes de cada `execute_action` el cliente lee el elemento bajo el punto objetivo, que debe ser el identificado (o su padre) en la ventana esperada, o no envía nada. Para la lista cerrada `NAVIGATING_STEPS = {"explorer": (1, 2, 3)}` (las dos migas de pan y «Atrás» de E2E-03) la identidad tras el clic se registra y no se exige: decide la comprobación del sistema (la carpeta de la ventana), y si falla es `grounding_wrong` y parada. Los demás pasos conservan la regla. **La causa de la parada no está verificada:** los registros no guardaban la cadena de ancestros, así que no distinguen un control reconstruido de un `Text` más profundo que el padre; la cadena se registra ahora. Si el anclaje previo rechaza el primer clic por eso, ensanchar la regla es una decisión nueva. En la misma rama, una parada de seguridad (`grounding_wrong`, `engine_false_success` o cualquier aborto) termina toda la invocación de varias corridas en el acto y crea `stop.flag`, como tres fallos seguidos (hallazgo 13 de la serie, PR #29); un fallo corriente no. Los clientes tienen hashes nuevos (`docs/thesis/S4_ACEPTACION_REDUCIDA.md`).

La undécima ronda (1.0.43, rama `docs/s4-eleventh-round`) no envió ninguna solicitud real (cuaderno 515, de 200 desde `--round-start 429`). Sus comprobaciones de código pasaron. El preflight de sólo lectura halló que las dos ventanas `CabinetWClass` minimizadas, fuera de pantalla, del shell (PID 36160) que el responsable confirmó como suyas ya no existen, y que el equipo recibió una entrada de teclado o ratón entre dos comprobaciones. No hay causa verificada. Por la regla del preflight y porque el equipo parece en uso, E2E-03 (sonda, piloto y serie) no se ejecutó. Decide el responsable: confirmar el estado del equipo y la línea base antes de cualquier entrada.

La continuación de la undécima ronda (1.0.44, rama `docs/s4-eleventh-round-run`) tampoco envió ninguna solicitud real (cuaderno 515, de 200 desde `--round-start 429`). Con la línea base nueva que indicó el responsable, la comprobación previa cumplió todas sus condiciones, pero el contador de inactividad marcó una entrada de teclado o ratón hacia las 00:23 (hora local), después de su respuesta y sin comandos que la provocaran. Por la regla de la ronda, E2E-03 no se sondeó ni se pilotó. Decide el responsable: si el equipo estaba en uso y si la ronda continúa con una autorización nueva.

La segunda continuación de la undécima ronda (1.0.45, rama `docs/s4-eleventh-round-run2`) envió 15 solicitudes reales (cuaderno de 515 a 530, de 200 desde `--round-start 429`; quedan 99). La sonda de E2E-03 cumplió la compuerta sin entrada; el piloto pasó (3 de 3 comprobaciones del sistema, cero falsos éxitos); las corridas puntuadas 2 a 4 pasaron. La corrida 5 no se envió: el contador de inactividad marcó entrada que la ronda no provocó, varias veces entre las 00:56 y las 00:58 (hora local), y la compuerta detuvo la serie. Decide el responsable: si esa entrada fue suya y si la serie reanuda con una autorización nueva. Matriz: E2E-03 sigue `PENDIENTE_PRODUCTO`.

El 15 de octubre de 2026 el responsable fijó el alcance reducido del MVP y la licencia (1.0.52, [ADR-0023](adr/0023-licence-and-reduced-mvp-scope.md)). RNF-05 se acepta como incumplimiento documentado (dos regiones, p95 153 ms frente a 150 ms). El protocolo reducido de S4 sustituye las 450 corridas de EV-02 para este MVP: E2E-01, E2E-02, S4R-02 y S4R-03 tienen diez de diez corridas puntuadas sin falsos éxitos en `main`, y E2E-03 tiene tres de diez. EV-01, EV-02, IT-07 y el corpus de REL-01 pasan al próximo corte. La reevaluación de la Definition of Done (capítulo 06 §6.7) marca dos casillas con su evidencia y deja ocho sin marcar, cada una con lo que le falta.

La tercera continuación de la undécima ronda (1.0.46, rama `docs/s4-eleventh-round-run3`) envió 28 solicitudes reales (cuaderno de 530 a 558, de 200 desde `--round-start 429`; quedan 71). Las corridas 5 a 11 pasaron las siete: con las corridas 2 a 4, son diez de diez corridas puntuadas, sin falsos éxitos ni controles erróneos, cada una con sus tres comprobaciones del sistema. La matriz pasa a `EJECUTADO_SUPERVISADO_PARCIAL` para E2E-03. Dos compuertas detuvieron el envío antes de cualquier solicitud: la de la corrida 5, anclada en una lectura anterior a una entrada del responsable, y la de la corrida 6, por una entrada a las 01:40:06 que la coordinación atribuyó al responsable, que seguía en el equipo; el registro sólo fecha esa entrada. La coordinación decidió reanudar. Pendiente: que el responsable confirme esa atribución, y revisar la corrida 11, que tardó 132 s sin que el cliente registre las esperas.

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
  `TextRecognizer`, `AccessibilityProvider`, `OsActuator`, `Clock`, `PolicyStore`), los tipos del contrato Ommatix 1.0
  (`contract`) y el caso de uso `Engine`:
  - `observe_screen`: captura → plan de regiones → detector, OCR y árbol de accesibilidad
    (ADR-0017, `engine/accessibility.rs`) sólo en las regiones planificadas → reconciliación
    con la evidencia accesible (`reconcile_accessibility`) y fusión → íconos (algoritmo 3A) → registro → staging y commit. Una etapa
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
    pantalla cambió); recibo de entrega → `input_delivered` o `indeterminate` (un
    `indeterminate` con el recibo completo informa `input_delivered: true` y cuenta en
    `delivered_actions`);
    `visual_change` se verifica comparando con la captura previa a la entrega dentro de un
    plazo acotado (`EngineConfig::verify`: capturas cada `verify_poll_ms` con el `Clock`
    hasta `verify_timeout_ms`; si vence, `indeterminate`, sin reenviar la entrada) y los
    demás efectos quedan `indeterminate`; un error del actuador también es
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
    correspondencia de pantalla compartida con la entrada (`mapping_source`); la primera
    captura tras abrir o recrear la duplicación espera hasta 500 ms una actualización con
    imagen presentada (`LastPresentTime != 0`) y falla antes que entregar un frame en
    blanco. El `unsafe`
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
    `ocr`) se compilan y prueban siempre, entre ellas `model::artifacts::verify`, que lee y
    comprueba cada archivo del detector (manifiesto, DirectML, ONNX Runtime, pesos) **antes** de
    cargar ninguna biblioteca (SEC-06, ADR-0021).
  - Un único `windows = "=0.62.2"` en el workspace; cada crate añade sus features.
- **`ommatix` (app)**: `serve` y `demo` ejecutan el motor nuevo con las opciones `--config`,
  `--source sim|images|screen|dxgi` (`sim` por defecto; `screen` con la feature `screen`,
  `dxgi` con la feature `dxgi`), `--images DIR`, `--output N` (salida de `dxgi`, 0 por
  defecto), `--input dry-run|native|sendinput` (por defecto `[engine] mode`; `native` con la
  feature `native-input` y la fuente `screen`; `sendinput` con la feature `sendinput`, la
  fuente `dxgi` y una lista `allowed_applications` no vacía de nombres en minúsculas; ambas
  exigen `actuation_enabled = true`), `--theme`, `--session-id` y `--legacy`; `demo` añade
  `--observations N` (1–20) y `--timings` (tiempos por etapa en stderr). Features: `screen`, `native-input`, `dxgi`, `sendinput`,
  `onnx`, `winrt-ocr`, todas apagadas por defecto. La configuración tiene la forma de
  `docs/thesis/plantillas/configuracion_mvp.toml` y se valida de forma estricta: se rechazan
  claves desconocidas, valores fuera de rango y valores que el binario no puede cumplir
  (otra plataforma, perfil o transporte, `detector_path`/`detector_sha256`/`ocr_path`/
  `ocr_sha256`, un detector configurado a medias o con hashes mal formados,
  `allow_destructive`, `allow_screenshots_to_client`, `persist`). `max_snapshot_age_ms`
  (10 000 ms por defecto, ADR-0025) sí se aplica: la captura del snapshot y la lectura de
  accesibilidad que confirma un objetivo no pueden tener más de esa edad al autorizar una
  acción; vencida, `STALE_SNAPSHOT`, observar de nuevo. `[models]` elige el detector
  (`detector_manifest`, `detector_manifest_sha256`, `onnxruntime_path`,
  `onnxruntime_sha256`, juntas) y el OCR (`ocr = "none"|"winrt"`, `ocr_language`) para las
  fuentes `images`, `screen` y `dxgi`; las rutas relativas lo son al archivo de
  configuración; cualquier fallo de integridad, feature o idioma detiene el motor antes de
  abrir la captura. `[cache]` admite un archivo JSON lines de prototipos curados con
  `prototypes_sha256` (su SHA-256, que se comprueba antes de leerlo), `context`,
  `gradient_tau`, `max_distance` y `min_margin`, todos juntos y sin valores por defecto; el
  archivo se rechaza entero ante cualquier error, un digest distinto o si supera `max_entries`.
- **Aceptación**: `src/acceptance/tests/thesis.rs` ejecuta BDD-01..37 desde
  `docs/thesis/features/` (BDD-01..10 y 13..21 sobre el núcleo; BDD-11, 12 y 22..27 a través
  del motor y del adaptador MCP; BDD-28..32 igual, sobre `LiveDouble`, un doble del actuador
  real que registra lo que recibe y no envía nada; BDD-33 añade `TreeDouble`, un doble del
  árbol de accesibilidad). BDD-32 usa la política real del operador
  (`ommatix::policy::ConfigPolicy`, expuesta por la biblioteca del crate `ommatix`) con un
  archivo de parada en un directorio temporal. BDD-28 cubre otra aplicación que pasa a
  primer plano antes de escribir; el foco de teclado dentro de la aplicación permitida lo
  comprueba el actuador `SendInput`, que ninguna prueba ejecuta. BDD-36 y BDD-37 (ADR-0020,
  aprobada) muestran el alcance por ventana sobre el mismo doble: el escritorio, la barra de
  tareas y una ventana que el operador no registró (archivo `window_pin_file` del producto) se
  rechazan con `POLICY_DENIED`. Se retiraron las siete herramientas del prototipo y su suite de
  aceptación (`src/acceptance/features/` y `tests/acceptance.rs`).
- **Límites conocidos**: la entrada real sólo es posible con `--source dxgi --input
  sendinput` y el consentimiento del operador; en S3 se ejecutó supervisada con cuatro clics
  sobre un menú de `notepad.exe` (`thesis/S3_CORRIDAS_SUPERVISADAS.md`); la parada por
  archivo se ejecutó en vivo, pero la comprobación del foco antes de escribir y el cambio de
  primer plano sólo se han probado sobre dobles; sólo se verifica el efecto
  `visual_change`; en vivo una única captura justo después de entregar nunca vio el cambio
  (4 de 4 `indeterminate`), y con el plazo de verificación de 1.0.10 la segunda corrida vio
  el cambio en 2 de 2 clics (`effect_observed`, 1.0.12); el recibo de `SendInput` no es visible; el único detector seleccionado (spike S0, `icon_detect_v3`) tiene una sola clase, así que los roles
  siguen `unknown`, una inferencia cuesta unos 7,5 s en CPU y su precisión no está medida; el
  primer frame DXGI en blanco (`thesis/S0_RESULTADOS.md` §7) está corregido (1.0.10) y
  confirmado en vivo (2 de 2 sesiones, 1.0.12); no hay OCR por defecto; OR-44 espera "actuación no implementada" y el
  motor responde `POLICY_DENIED` sin entregar nada; el contexto del caché de íconos lo fija
  el operador y sus umbrales no están calibrados; no se ha probado un cliente MCP real
  (capítulo 07 §7.1). Del protocolo S4 reducido (`thesis/S4_ACEPTACION_REDUCIDA.md`):
  ningún flujo puede escribir, porque `type` exige un `Role::Input` confirmado y el detector
  da `unknown`; el OCR de Windows no lee los botones de un glifo (la Calculadora no es
  direccionable por etiqueta); y una etiqueta puede pertenecer a una caja centrada en otro
  control (en la Calculadora, 4 de 4 clics del piloto cayeron en x²). `effect_observed` sólo
  afirma un cambio visual, así que no detecta ese error.

## Qué sigue

**Estado al 15 de octubre de 2026 (1.0.59, alcance reducido del MVP).** Para cerrar el MVP con el alcance reducido queda:

1. **Protocolo reducido de S4: completo en `main`.** E2E-01, E2E-02, E2E-03, S4R-02 y S4R-03 tienen diez de diez corridas puntuadas sin falsos éxitos (E2E-03 desde 1.0.46). Esas corridas son de commits anteriores a INC-03 (#38) e INC-04 (#39); la decisión 6 de la lista de [06 §6.7](thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md) pide repetirlas sobre el código actual.
2. **Sesión A (REL-01)**: instalar, verificar y revertir siguiendo [`S4_INSTALACION_Y_REVERSION.md`](thesis/S4_INSTALACION_Y_REVERSION.md) en una máquina limpia, y archivar el `environment.json` (decisión 7). Es la evidencia de REL-01; la casilla de documentación ya está marcada.
3. **Sesión B (PERF-03)**: ejecutada en 1.0.55 sobre `fe35d80`, sin accesibilidad (§12 de [`S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md)). El umbral de memoria de trabajo y de manejadores lo fijó el responsable en 1.0.62 (decisión 1, resuelta), y la corrida lo cumple en sus condiciones. Faltan la cota de la cola (decisión 9) y un soak del código actual con `uia` (decisión 6, programado). Sin cota de cola, RNF-06 no tiene veredicto. El intento con `uia` de 1.0.59 quedó bloqueado antes de la corrida porque la captura de pantalla falló (causa probable: el apagado de pantalla); el responsable debe dejar la pantalla encendida antes de repetirlo ([`S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §14).
4. **Pendientes que no están en el alcance cedido**: el borrado bajo demanda del operador, sin terminar la sesión (capítulo 08 §8.4; decisión 2); la parte de percepción de PERF-02, que ya tiene p99 en S0 §12 pero no la variante de caché (B2) ni la igualdad de modelos; y SEC-02, SEC-05 e IT-01 a IT-05 en parte. La prueba diferencial del diferenciador (capítulo 05 §5.5) está hecha desde 1.0.53.
5. **Próximo corte** (fuera de este MVP): EV-01, EV-02, IT-07 y el corpus de REL-01, con el plan del paquete Paper (INC-00 a INC-09; no se copia aquí). RNF-05 sigue como incumplimiento documentado; con `uia` y una animación de 30 ms su caso de dos regiones da 186,3 ms (decisión 5). IT-07 y su sesión con el responsable presente son la decisión 3, y INC-07 la decisión 4 (no autorizada).

Las ocho decisiones están en [06 §6.7](thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md), con la evidencia que el responsable necesita para cada una. El vocabulario de estado de los requisitos (decisión 8) explica por qué ningún RF/RNF cambia de estado aunque tenga evidencia.

**Fuera del cierre del MVP, pendientes tras INC-02 (1.0.57):** INC-07 (política adaptativa entre refresco completo y regional, no autorizada); un control B0 frente a B0 del acuerdo; la captura de `full` y el residuo de PERF-01; investigar con `uia` la pérdida de elementos y separar la causa de la lectura lenta de UIA. Hechos en 1.0.57: la repetición con `uia`, la atribución de la pérdida de elementos y el `-0.0`. Detalle en [`S0_RESULTADOS.md`](thesis/S0_RESULTADOS.md) §13.13.

**Lo que ya no hace falta decidir:** la licencia del código y de los documentos (15 de octubre de 2026) y el propietario de los requisitos.

Las decisiones de antes siguen en la lista de abajo; las que se refieren a la licencia o al alcance de este MVP quedan cerradas por este estado.

1. **Revisar y fusionar** el PR del spike S0 (`feat/s0-omniparser-v3`, #8), después el de
   las corridas supervisadas de S3 (`feat/s3-supervised-runs`, #9, apilado sobre #8) y el
   de sus correcciones (`fix/s3-verify-settle`, apilado sobre #9).
2. **Decisiones que esperan al responsable** (ningún agente puede tomarlas):
   - Detector del producto (ADR-0009). El spike S0 (`thesis/S0_RESULTADOS.md`) seleccionó
     `icon_detect_v3` de OmniParser (MIT, revisión `f55d0750`) para integrar y medir:
     ONNX opset 17 con la cabeza decodificada, ONNX Runtime 1.28.0 en CPU, rol `unknown` y
     affordance `interactive` tomada de su ficha. Queda por decidir si se acepta para el
     producto. Medido en el motor (`thesis/S0_RESULTADOS.md` §9): una inferencia cuesta unos
     7,5 s con un hilo de CPU, 2,6 s con 8 y 0,12 s con DirectML (mismas detecciones). Con
     DirectML, la rama `perf/rnf05-latency` (§10) lleva el refresco completo en caliente a
     p95 98 ms y una región a p95 102 ms, pero dos regiones quedan en p95 153 ms (meta: 150),
     porque cada región planificada cuesta una inferencia completa de unos 65 ms con la red
     en float16. Agrupar regiones en una entrada y usar entradas más pequeñas se midieron y se
     descartaron porque cambian las detecciones (§9.5 y §10.2). Falta: decidir si DirectML (ONNX Runtime 1.24.4,
     la última versión con DirectML, en ingeniería sostenida) es aceptable como proveedor del
     producto, medir la precisión (RSS y VRAM de RNF-06 ya están en §12), o buscar un modelo más ligero. Los
     pesos `icon_detect` v2 (AGPL-3.0) no se usan.
   - RNF-05 (`thesis/S0_RESULTADOS.md` §10). Decidido por el responsable el 2026-10-14:
     (a) las corridas de S4 usan desde ahora la red en float16 (`model_manifest.rnf05.json`,
     sólo DirectML; con una aplicación que repinta de forma continua es peor que el float32
     concurrente porque la GPU saturada retrasa a UI Automation, §10.5); (b) **no** se cambia
     qué ventanas lee ADR-0017 ni se guarda la lista entre observaciones (enumerar las
     ventanas de nivel superior cuesta unos 48 ms de los 58 ms de la lectura de accesibilidad
     y es lo que separa a dos regiones de la meta de 150 ms), así que RNF-05 sigue sin
     cumplirse por unos 3 ms con dos regiones. Sigue pendiente: (c) RSS y VRAM (RNF-06), medidas
     en la corrida de 1.0.55 con float16 (§12 de S0); el umbral y la repetición con `uia` quedan en PERF-03 (decisiones 1, 6 y 9).
   - Protocolo de S4: el conflicto entre crear `stop.flag` tras tres corridas fallidas y dejar
     correr la serie siguiente cuando una se abortó por una causa propia (dos opciones en la
     [revisión de la quinta ronda](reviews/2026-10-14-s4-fifth-round.md)), y el clic de
     diagnóstico sobre «Editar» (hallazgo 1): hecho en la sexta ronda (1.0.24); el responsable decidió descender dentro de la isla (1.0.25) y la séptima ronda (1.0.26) lo confirmó en vivo en el piloto; la corrida 10 de E2E-03 se ejecutó en la continuación (1.0.27) y pasó; quedan S4R-02, detenido en la novena ronda (1.0.33) por la regla de seguridad tras el segundo clic de `--calc-standard`, decisión del responsable; E2E-02 y E2E-03, con series supervisadas de diez corridas (1.0.41 y 1.0.46; matriz `EJECUTADO_SUPERVISADO_PARCIAL`); y el cupo que cuente los reintentos (ver [`thesis/S4_ACEPTACION_REDUCIDA.md`](thesis/S4_ACEPTACION_REDUCIDA.md), «Novena ronda»).
   - OCR: `Windows.Media.Ocr` (es-ES) funciona, pero no informa confianza y pierde signos como
     "¡"; el candidato PP-OCRv5 mobile (Apache-2.0, ONNX oficial) tampoco tiene "¡" en su
     diccionario.
   - Elegir el cliente MCP real con el que se prueba la conformidad (capítulo 07 §7.1).
   - Elegir las aplicaciones de prueba controladas (para `allowed_applications`, las
     corridas IT y la evaluación de S4). Hay una aplicación de prueba propia
     (`tools/s4/testapp`). Para las 450 corridas faltan las familias "editor con escritura y
     guardado" y "explorador de archivos" (preparadas, sin ejecutar, en 1.0.29: editor propio
     `tools/s4/testeditor`, cliente `tools/s4/e2e_dossier.py` y la ADR-0020 aprobada) y una
     Calculadora direccionable o su sustitución.
   - Evidencia de UI Automation (ADR-0017, aprobada el 2026-10-14): implementada y probada con
     dobles. Falta medir su coste en la máquina de referencia (etapa `accessibility` de
     `_meta`; el recorrido está acotado a 2 000 elementos, profundidad 32 y 250 ms, valores
     `[P]` por calibrar), y decidir si se mantiene el IoU 0,5 de la fusión. La segunda ronda
     (1.0.15) dejó abierto que `type` verificara el foco del campo: ADR-0018 lo resuelve en
     código, probado con dobles y, en la tercera ronda (1.0.17), en vivo sobre la aplicación de
     prueba (10 de 10 corridas, sin teclas perdidas). Quedan abiertos: la lectura parcial de
     accesibilidad en el motor, el paso «Borrar» de la Calculadora (decisión del responsable), el
     menú del Bloc de notas y el origen de los eventos de entrada sin acción. Reanudar cuando el
     responsable decida el modo de la Calculadora y revise el anclaje del clic de navegación (cuarta
     ronda, 1.0.19); `stop.flag` ya no existe. La lectura manual del árbol de la aplicación de prueba es la prueba `#[ignore]`
     `reads_the_tree_of_the_s4_test_application` del adaptador (arranca
     `tools/s4/testapp`, lee sólo ese proceso, imprime recuentos por rol y cierra el proceso
     que lanzó; no envía entrada).
3. **Corridas supervisadas en el hardware de referencia**, con el operador presente y una
   aplicación de prueba en `allowed_applications`. S0 ejecutó en parte IT-01 e IT-02 sin
   entrada; S3 (`thesis/S3_CORRIDAS_SUPERVISADAS.md`, cliente `tools/s3/mcp_driver.py`)
   ejecutó SEC-01 y en parte IT-01, IT-03, IT-04, IT-05, SEC-02 y SEC-05; ninguna prueba
   automática sustituye a las que faltan. La segunda corrida (1.0.12) confirmó en vivo el
   plazo de la verificación visual y el primer frame DXGI. Antes de la próxima: abrir el Bloc de notas en una
   sesión limpia (restaura pestañas) y traer al primer plano a mano la ventana de prueba (Windows no
   se lo da a un proceso lanzado en segundo plano; por eso SEC-05 no pudo cambiar el primer
   plano):
   - IT-01 (captura DXGI: frames reales, stride, rotación, pérdida de acceso;
     `cargo test -p ommatix-adapter-capture --features dxgi -- --ignored` toma un frame sin
     guardarlo), IT-02 (detector y OCR reales, deshacer el letterbox; necesita modelos),
     IT-03 (conteo de `SendInput`, permisos y foco), IT-04 (postcondición y plazo sin
     reintento), IT-05 (idempotencia ante caídas), IT-06 (telemetría redactada: en Rust desde
     1.0.35; falta revisar los registros de una corrida en vivo), IT-07 (DPI 100/125/150 %,
     epoch y fuera de perfil; `tools/perf/dpi_probe.py` y la sesión C de
     `docs/thesis/S4_SESIONES_PENDIENTES.md` están preparadas desde 1.0.47, sin ejecutar).
   - SEC-01..06: el agente no puede habilitarse, parada, prompt injection visual, sin
     capturas ni secretos, precheck de foco y ventana con carreras, integridad de modelo y
     caché. SEC-03, SEC-04 y SEC-06 existen en Rust desde 1.0.35; falta repetir SEC-03 con el
     OCR y el árbol de UI Automation reales ante una página de texto hostil, y SEC-06 con los
     archivos reales del operador (ya comprobados por `tools/repro/environment.py`).
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
- El toolchain de Rust está fijado a una versión (`rust-toolchain.toml`, hoy 1.99.0) y la CI instala la misma: al subirlo se cambian los dos a la vez (una prueba lo comprueba). Una prueba de Rust acredita una prueba planificada sólo si lleva su identificador (`ut_45_…`, `sec_03_…`); `src/acceptance/tests/repository.rs` lo comprueba.
- Commits en Conventional Commits; PR con `.github/pull_request_template.md`; documentos con
  gemelo `*.es.md` se actualizan en el mismo PR.
- Nada en el repositorio menciona el nombre anterior del proyecto. No se versionan capturas,
  pesos de modelos ni la conversación original [F0].
