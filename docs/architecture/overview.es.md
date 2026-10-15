# Visión general de la arquitectura

🇬🇧 [English version](overview.md)

Esta página resume el diseño de la tesis y lo ubica en el repositorio. El texto normativo está
en [`docs/thesis/`](../thesis/README.md): el SDD (capítulo 02), los algoritmos (capítulo 04) y
el contrato MCP (capítulo 07). Si esta página y esos capítulos difieren, mandan los capítulos.

## Problema

Los agentes de uso de computador suelen enviar una captura de pantalla a un modelo de
visión-lenguaje en cada paso y actuar sobre las coordenadas que este adivina. Eso es lento y
costoso, y además inseguro: la pantalla puede cambiar entre la observación y la acción, y
nada comprueba que el clic siga cayendo sobre lo que el agente eligió.

## Enfoque

Ommatix separa la percepción del razonamiento.

- **Percepción (Ommatix, local, Rust):** convierte los frames de pantalla en elementos de
  interfaz con identificadores de sesión estables, re-analiza sólo lo que cambió y publica
  snapshots inmutables.
- **Razonamiento (el agente, cliente MCP):** elige un elemento por su identificador y su
  revisión. Nunca envía coordenadas.

Antes de actuar, Ommatix comprueba que la observación del agente sigue vigente y que una
política local permite la acción. Después inyecta la entrada e informa por separado la
entrega y la verificación. El contenido de la pantalla es un dato no confiable: un texto en
pantalla nunca cambia permisos.

## Límites de confianza

Un operador local autoriza a un agente a usar una instancia del motor, ligada a una sesión de
usuario y a un monitor permitido. Una política local fuera del control del agente decide qué
aplicaciones y acciones están habilitadas, y el operador puede detener el motor en cualquier
momento. El MVP no abre puertos de red; MCP va por stdio. Ver el [contexto](../diagrams/context.md)
y el [capítulo 08 de la tesis](../thesis/08_SEGURIDAD_Y_PRIVACIDAD.md).

## Componentes

El SDD define doce componentes ([diagrama](../diagrams/components.md)).
[ADR-0014](../adr/0014-hexagonal-crate-layout.md) los ubica en crates:

| Componente | Responsabilidad | Contrato | Crate |
|---|---|---|---|
| MCP Adapter | Validar y enrutar llamadas; describir herramientas | Entradas y salidas versionadas | `ommatix-adapter-mcp` |
| Observation Coordinator | Captura, planificación de regiones y commit | SnapshotRequest → Snapshot | `ommatix-application` (`Engine::observe_screen`) |
| Capture Provider | Píxeles, stride, tiempos, rectángulos dirty y move | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR y transformaciones de imagen | Region → RawEvidence | `ommatix-adapter-vision` |
| Accessibility Provider | El árbol de UI Automation de la ventana en primer plano; evidencia opcional fusionada con la visual | Region → AccessibleElement | `ommatix-adapter-accessibility` |
| Spatial Fusion | Contención, renglones, etiquetas y relaciones; absorbe las detecciones que coinciden con un elemento accesible | RawEvidence + AccessibleElement → Candidates | `ommatix-domain` |
| Icon Resolver | Recuperación contextual con rechazo | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisiones y bajas | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Vigencia, permisos y precondiciones | ActionRequest → Permit o Reject | `ommatix-application` (`Engine::execute_action`) |
| OS Actuator | Inyección de eventos y conteo de entrega | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Comprobar un efecto esperado o informar incertidumbre | Antes y después → Verification | `ommatix-application`: nueva captura tras la entrega (parcial; S3) |
| Telemetry | Métricas y auditoría sin texto sensible | Eventos y spans | `get_engine_metrics` y tiempos por etapa en el `_meta` de MCP (parcial; S3) |

El núcleo sólo llega al exterior mediante los puertos `FrameCapturer`, `VisionDetector`,
`TextRecognizer`, `AccessibilityProvider`, `OsActuator`, `Clock` y `PolicyStore`, de modo que cada adaptador puede
sustituirse por un fixture determinista. En Windows 11
([ADR-0008](../adr/0008-one-real-platform-first.md)) la captura usa DXGI Desktop Duplication
y la entrada usa `SendInput`. La inferencia corre en ONNX Runtime, con CPU como baseline y
DirectML como opción; los modelos se eligen en el spike S0
([ADR-0009](../adr/0009-models-and-licences-decided-in-the-spike.md)).

## Modelo de datos

- **Frame:** tamaño físico, stride de fila en bytes, BGRA8, instante monotónico de captura,
  monitor, rotación, epoch geométrico y procedencia de sus regiones de cambio. Los tamaños se
  validan con aritmética comprobada y nunca se asume `row_stride == width * 4`.
- **RawEvidence:** una caja, el tipo de evidencia, texto opcional, el score del proveedor, la
  transformación del tensor al recorte, el modelo con su revisión y el frame de origen.
- **Element:** un `target_id` opaco, una revisión monotónica, un rol o `unknown`, una etiqueta
  con procedencia, contenido interno con `text_kind`, visibilidad, habilitación y
  `actionability`. La geometría se queda dentro del motor.
- **Snapshot:** `session_id`, `snapshot_id`, `geometry_epoch`, instantes de captura y de
  publicación, elementos, relaciones, degradación y métricas, en orden estable.

## Una observación

1. **Capturar** un frame y los rectángulos dirty y move nativos.
2. **Planificar regiones (algoritmo 1).** Comparar tiles de 64×64 de forma exacta, píxel a
   píxel, contra la última base *confirmada*, y sumar los tiles sucios que sigan pendientes.
   Agrupar los tiles sucios en componentes 8-conexos, añadir a cada uno un halo de 32 px y
   cerrarlo sobre la evidencia que toca hasta que nada cambie. Un cambio de geometría, un
   cierre que agota su presupuesto o un plan por encima de los umbrales iniciales (40 % del
   área o más de cuatro regiones) pasa a un refresco completo.
3. **Percibir** las regiones planificadas con el detector y el OCR, y leer dentro de ellas
   el árbol de accesibilidad. Las tres etapas leen el mismo frame y no dependen entre sí,
   así que se ejecutan en hilos acotados y se unen antes de fusionar
   ([ADR-0019](../adr/0019-concurrent-observation-stages.md)); el coordinador sigue siendo
   el único escritor y recoge los resultados por etapa, no por orden de llegada.
4. **Fusionar (algoritmo 2).** Un texto pertenece al control más específico que lo contiene,
   medido por IoC = área(texto ∩ control) / área(texto) ≥ 0,75; ante un empate se abstiene.
   Los textos se agrupan en renglones antes de ordenar la lectura. Las etiquetas externas se
   asocian por un coste anisotrópico dentro de un corredor entre bordes enfrentados, con
   abstención si el margen es pequeño. Un texto sin propietario pasa a ser un `text_node` de
   accionabilidad desconocida.
5. **Resolver íconos (algoritmo 3A).** Los íconos sin texto se buscan en un caché contextual
   con un descriptor de gradientes de 128 bits con máscara; una coincidencia exige soporte
   suficiente y margen sobre el segundo candidato. Lo desconocido sigue siendo desconocido.
6. **Seguir la identidad (algoritmo 3B).** Un matching uno a uno conserva el ID de sesión de
   cada elemento, que es monotónico y nunca se reutiliza; la revisión aumenta cuando cambia
   algo de lo que depende una acción; los IDs retirados dejan tombstones. Ante ambigüedad se
   crea un track nuevo en lugar de adivinar.
7. **Confirmar (commit).** Registro, índice y base se publican juntos como un snapshot
   inmutable. Si algún paso falla no se publica nada, las regiones sucias pendientes se
   conservan y el motor queda degradado, lo que bloquea las acciones
   ([ciclo del snapshot](../diagrams/snapshot-state.md)). Un cambio de monitor, DPI o
   configuración de modelos incrementa el epoch geométrico y fuerza una reconstrucción.

## Una acción

[Diagrama de secuencia](../diagrams/action-sequence.md). `execute_action` nombra la sesión,
el snapshot, el epoch geométrico, el objetivo y su revisión esperada. El motor valida el
schema, deduplica el `request_id`, aplica la política local, comprueba que snapshot, revisión
y epoch estén vigentes, confirma ventana, foco y frescura, registra su intención, inyecta la
entrada, comprueba la entrega y busca el efecto esperado.

La respuesta es `simulated`, `rejected`, `input_delivered`, `effect_observed` o
`indeterminate`, y ninguna afirma más de lo que muestra: una entrada entregada no prueba que
la interfaz obedeciera, y un efecto incierto nunca se reintenta automáticamente. `dry_run` es
verdadero salvo que el agente lo ponga en falso, y aun así se aplican el consentimiento local
y la allowlist ([ADR-0007](../adr/0007-fail-closed-actuation-and-honest-simulation.md)).

## Contrato con el agente

Lo definen el [capítulo 07 de la tesis](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md) y los
schemas JSON de [`docs/thesis/contratos/`](../thesis/contratos/README.md):

- **Perfiles:** MCP 2026-07-28 (`server/discover`, sin `initialize`, versión y capacidades
  en `_meta`); 2025-11-25 es opcional
  ([ADR-0002](../adr/0002-mcp-stdio-with-explicit-profiles.md)). Una conexión queda fijada en
  el perfil de su primera petición que lo selecciona y nunca mezcla los dos juegos de reglas.
- **Transporte:** tipos de wire de `rmcp` 3.5.1 con un bucle de mensajes propio y acotado,
  porque el bucle de servicio de `rmcp` no limita el tamaño, cierra la conexión ante
  notificaciones tempranas, deja mezclar perfiles y descarta en silencio la entrada no
  analizable ([ADR-0015](../adr/0015-mcp-transport-on-rmcp-wire-types.md)).
- **Herramientas:** `observe_screen` (consulta, paginación, sin coordenadas),
  `execute_action` (`click`, `type`, `scroll`) y `get_engine_metrics`, descritas con los
  schemas del dossier.
- **Errores:** llevan un código (`STALE_SNAPSHOT`, `STALE_TARGET`, `AMBIGUOUS_TARGET`,
  `POLICY_DENIED`, `REQUEST_CONFLICT` y otros), `retryable` y un `next_step` como
  `observe_again` u `operator_review`; son resultados de herramienta con `isError: true`. Los
  mensajes mal formados, los métodos o herramientas desconocidos y los argumentos que violan
  el schema (`-32602`) son errores JSON-RPC. Los mensajes se limitan a 64 KiB en ambos
  sentidos. Los valores que dependen de la ejecución van en el `_meta` del resultado, nunca en
  `structuredContent`: los tiempos por etapa de una observación (`io.ommatix/timings`) y si su
  evidencia de accesibilidad es completa (`io.ommatix/accessibility`), y los tiempos por etapa
  de una acción (`io.ommatix/action_timings`).

## Atributos de calidad

| Atributo | Cómo se aborda |
|---|---|
| Corrección | Cada algoritmo tiene invariantes y pruebas planificadas (UT-01..43); un oráculo Python (48 pruebas) es la referencia diferencial |
| Seguridad | Compuerta fail-closed, vigencia estricta, estados de entrega y verificación separados, parada local |
| Determinismo | Colecciones ordenadas, ordenamiento explícito, reloj inyectado; los valores que dependen de la ejecución quedan fuera del resultado estructurado |
| Privacidad | Sin capturas ni valores en logs por defecto; los datos de pantalla nunca elevan permisos ([ADR-0012](../adr/0012-data-minimisation-and-evidence-traceability.md)) |
| Latencia | Presupuestos por etapa, medidos en hardware de referencia antes de convertirse en compromisos ([ADR-0010](../adr/0010-performance-budgets-subject-to-measurement.md)) |
| Modificabilidad | Puertos y adaptadores, con límites impuestos por las dependencias entre crates |

## Dónde está el código hoy

Desde S2b el binario ejecuta el motor de la tesis. `serve` y `demo` construyen un único
`Engine` (`ommatix-application`) sobre los puertos del SDD y, en `serve`, el adaptador MCP
delante. Se retiran los casos de uso del prototipo del arranque, sus puertos (`FrameSource`,
`ElementDetector`, `InputController`), sus siete herramientas MCP con coordenadas y su suite
de aceptación (el directorio `src/acceptance/features/` y `tests/acceptance.rs`). De sus
módulos de dominio, `ommatix-domain` conserva sólo los tipos de apoyo que todavía usan los
adaptadores provisionales (el `Frame` RGBA, `Rect`/`Point`, `Detection`, la `Action` del
usuario simulado y `DisplayMapping`). Sus decisiones archivadas siguen en
[`docs/adr/bootstrap/`](../adr/bootstrap/README.md). La
[revisión de S2b](../reviews/2026-10-14-s2b-engine-and-mcp.md) registra los hallazgos
corregidos antes de cerrar la etapa, cada uno con su prueba de regresión.

**Adaptadores de Windows (S3, primera parte).** Cada uno está tras una feature de compilación
apagada por defecto, y los valores por defecto siguen fallando de forma segura: sin modelo,
sin OCR, entrada en dry run y política que lo niega todo.

- *Captura* (`dxgi`, `--source dxgi --output N`): DXGI Desktop Duplication de una salida en
  píxeles físicos (el proceso declara conciencia de DPI por monitor v2). La textura del
  escritorio se copia a una textura de staging y se lee fila a fila con su propio stride; los
  frames sólo viven en memoria. Se rechaza una salida rotada; un cambio de tamaño, DPI u
  origen incrementa el epoch geométrico; si no llega ninguna actualización del escritorio se
  devuelve de nuevo el último frame; la primera captura tras abrir (o recrear) la
  duplicación espera hasta 500 ms una actualización con una imagen del escritorio presentada
  (`LastPresentTime != 0`) y falla antes que devolver un frame en blanco; una duplicación
  perdida (`DXGI_ERROR_ACCESS_LOST`,
  dispositivo retirado o reiniciado: un cambio de modo, el escritorio seguro) se recrea una
  vez dentro de la captura, una segunda pérdida es un error de captura y el frame siguiente
  abre un epoch geométrico nuevo. Los rectángulos dirty y move sólo se cuentan; el
  comparador del motor decide qué cambió. El capturador comparte su correspondencia de
  pantalla vigente con el adaptador de entrada, que se niega a apuntar mientras está perdida
  o fuera de perfil.
- *Entrada* (`sendinput`, `--input sendinput`, sólo con `--source dxgi`): `SendInput` de Win32
  apuntado con la correspondencia del capturador, una llamada por acción, con un recibo de
  eventos enviados e insertados. Atestigua la aplicación en primer plano (el nombre del
  ejecutable, en minúsculas, del proceso de la ventana en primer plano) para la comprobación
  de la lista permitida del motor y, antes de enviar, rechaza otro proceso en primer plano,
  un punto fuera de las ventanas de ese proceso, un nivel de integridad mayor (UIPI
  descartaría la entrada en silencio) y modificadores o botones del ratón pulsados. `type`
  envía primero el clic que enfoca el campo y sus teclas sólo cuando `GetGUIThreadInfo`
  muestra el foco de teclado, en el mismo proceso en primer plano, en la ventana bajo el
  punto objetivo o en una que la contiene (lo consulta hasta 200 ms mientras se procesa el
  clic); si no, no envía ninguna tecla. Con evidencia de accesibilidad es el motor quien
  controla esa pausa (ADR-0018): `deliver_focus_click` envía el clic, el motor consulta
  `keyboard_focus` del proveedor con su `Clock` hasta que el elemento objetivo tiene el foco
  y sólo entonces llama a `deliver_keys`, que sigue comprobando la aplicación en primer plano
  y los modificadores pulsados. Un rechazo es un error del actuador, que el motor
  informa como `indeterminate` y nunca reintenta: tras un `type` rechazado el clic puede
  haberse entregado. Ninguna prueba envía entrada real.
- *Detector* (`onnx`, `[models] detector_manifest`): un detector genérico de tipo YOLO sobre
  ONNX Runtime cargado al ejecutar (`load-dynamic`) desde una biblioteca fijada por SHA-256.
  El manifiesto del modelo está fijado por SHA-256 y nombra los pesos, su SHA-256 y el rol y
  la affordance de cada clase; los nombres de clase nunca se convierten en roles. Cada región
  planificada se encaja con letterbox en la entrada, se decodifica, se suprime y se lleva de
  vuelta al frame, con una inferencia por región. Una inferencia cuesta casi lo que píxeles
  tiene su entrada, y las entradas más pequeñas para las regiones pequeñas cambian las
  detecciones (S0 §10.2), así que toda región pasa por la entrada completa. El manifiesto de
  RNF-05 para DirectML apunta a una exportación de la red en float16 con la decodificación en
  float32 (S0 §10.3). Corre en la CPU
  (`detector_threads` hilos intra-op) o, con la feature `directml`, en DirectML con una
  `DirectML.dll` fijada, ejecución secuencial, sin patrón de memoria y sin recurrir a la
  CPU; el manifiesto enumera los proveedores con los que se validó el modelo.
- *Accesibilidad* (`uia`, `[models] accessibility = "uia"`,
  [ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)): el árbol de UI Automation
  de la ventana en primer plano que atestigua el actuador (el marco de `ApplicationFrameHost`
  se resuelve a su aplicación hospedada), leído una vez por observación dentro de las regiones
  planificadas, acotado por elementos, profundidad y tiempo, con el crate seguro
  `uiautomation` (sin `unsafe` propio; el `clippy.toml` del adaptador prohíbe las llamadas de
  entrada y de patrones del crate). Es evidencia, no un reemplazo: un elemento habilitado, en
  pantalla y con un patrón de acción es `confirmed` con su rol y su nombre (`label_source:
  accessibility`), las cajas del detector que son el mismo control se funden con él y una caja
  centrada sobre otro control accesible deja de estar confirmada. Un recorrido que termina por
  su presupuesto lo declara (`truncated`): esas regiones se leen otra vez en la observación
  siguiente aunque ningún píxel haya cambiado, la evidencia que la lectura parcial no repite se
  conserva mientras sus píxeles no cambien y una caja visual dentro de un hueco no se confirma.
  Nunca lee el valor de un campo y un campo de contraseña no es una entrada. Sólo con `dxgi` o `screen`; apagado por
  defecto.
- *OCR* (`winrt-ocr`, `[models] ocr = "winrt"`): `Windows.Media.Ocr` para exactamente
  `ocr_language`, nunca otro idioma, sin inventar confianzas; el texto reconocido nunca se
  registra.
- *Configuración*: un detector u OCR configurado que el binario no tiene, cuyos archivos no
  coinciden con su SHA-256 o cuyo idioma no está instalado detiene el motor antes de abrir
  ninguna captura. `images`, `screen` y `dxgi` usan todos el detector y el OCR de
  `[models]`.
- *`unsafe` acotado*: DXGI y `SendInput` son llamadas COM y Win32. La
  [ADR-0016](../adr/0016-bounded-unsafe-for-windows-capture-and-input.md) (aprobada el 14 de octubre de
  2026) sólo permite `unsafe` en el `#[cfg(windows)] mod
  win` de los adaptadores de captura y entrada: esos crates deniegan `unsafe_code` y
  `clippy::undocumented_unsafe_blocks`, cada bloque envuelve una llamada con un comentario
  `// SAFETY:` y los punteros crudos nunca salen del módulo. El OCR usa la proyección segura
  de WinRT y el crate de visión mantiene `unsafe_code = "forbid"`. Todos usan un único crate
  `windows` fijado (`=0.62.2`), y la integración continua compila, revisa y prueba las
  features en `windows-latest`.

**`observe_screen`** captura un frame, planifica regiones contra la última base confirmada
(algoritmo 1), entrega al detector y al reconocedor sólo las regiones planificadas, conserva
la evidencia fuera de ellas, fusiona (algoritmo 2), resuelve íconos (algoritmo 3A), sigue las
identidades (algoritmo 3B) y prepara y confirma el snapshot. Una etapa fallida no publica
nada, deja pendientes los tiles planificados y bloquea las acciones. Las páginas se filtran
por `query`, se redactan salvo que la política local permita valores y quedan atadas a su
snapshot por el cursor.

**Íconos (algoritmo 3A).** Un control es candidato a ícono cuando es un `icon_button`, o
cuando no tiene texto interior y su rol no toma una etiqueta externa. Su caja se recorta del
frame (sin margen interior), se convierte a luma con los pesos BT.601 (0,299 R + 0,587 G +
0,114 B), se remuestrea a la rejilla 9×8 con medias enteras por bloque y se describe con el
descriptor de gradientes con máscara; el caché curado responde una etiqueta para el contexto
configurado sólo cuando la coincidencia tiene soporte suficiente, está lo bastante cerca y
supera por el margen a la siguiente etiqueta distinta. Una coincidencia se etiqueta
`curated_icon`; cualquier otra cosa deja la etiqueta vacía, nunca una conjetura. La etiqueta
nunca cambia la accionabilidad, que viene del detector. Sólo los candidatos dentro de las
regiones planificadas se vuelven a consultar; los demás conservan la etiqueta del snapshot
confirmado. El resolvedor está apagado salvo que el operador configure prototipos y umbrales
(`[cache]`), porque la tesis los calibra (H4); el contexto también se configura hasta que S3
pueda identificar la aplicación en primer plano.

**`execute_action`** comprueba los argumentos, busca el `request_id` en un diario de
deduplicación acotado (hash FNV-1a de los argumentos canónicos, de modo que nunca se guarda el
texto tecleado; el mismo ID con otros argumentos es `REQUEST_CONFLICT`) y después comprueba de
forma estricta sesión, epoch geométrico, snapshot, target, revisión y accionabilidad contra el
snapshot publicado. Un dry run (por defecto) es `simulated`. La entrada real exige la política
del operador (`actuation_enabled`, sin parada), un actuador real **y** una aplicación en
primer plano incluida en la lista no vacía `allowed_applications`, tal como la atestigua el
actuador; si no, `POLICY_DENIED` sin entregar nada. Sólo el actuador `SendInput` atestigua
la aplicación en primer plano; la entrada provisional con `enigo` no puede, así que sus
acciones reales siempre se deniegan. Antes de entregar, una comprobación de vigencia vuelve a
capturar la pantalla: si difiere de la base confirmada, la acción es `STALE_SNAPSHOT` y no se
entrega nada. Después el motor echa un último vistazo: vuelve a leer la política (la parada
del operador, el consentimiento, la lista permitida) y vuelve a pedir al actuador la
aplicación en primer plano; una parada, o una aplicación distinta de la atestiguada en la
compuerta, es `POLICY_DENIED` sin entregar nada. La parada del operador viene de `[safety]
stop_file` (un archivo cuya existencia detiene la entrada nueva, leído en cada acción; una
ruta ilegible cuenta como presente) o de un `StopSwitch` en proceso de la raíz de
composición; ninguna herramienta MCP llega a ninguno de los dos, y los resultados ya
entregados quedan registrados tal como se informaron. Mientras la entrada real es posible,
`serve` escribe en stderr un indicador de control activo, y una línea cada vez que la parada
se activa o se retira. Un recibo de entrega cuyos
eventos insertados coinciden con los enviados es `input_delivered`; cualquier otra cosa es
`indeterminate`. Un `visual_change` esperado se verifica contra el frame previo a la
entrega dentro de un plazo acotado: el motor captura justo después de entregar y, mientras
nada cambie, otra vez cada `[engine] verify_poll_ms` (50 ms) con su reloj inyectado hasta que
pase `verify_timeout_ms` (500 ms; 0 es una sola comprobación). Un cambio es
`effect_observed`; ninguno es `indeterminate`, y la entrada nunca se reenvía; los demás
efectos (foco, valor, desplazamiento) se informan `indeterminate` hasta que puedan
comprobarse. Cualquier entrega, sea
cual sea su resultado, agota el snapshot: la acción siguiente necesita una observación nueva
(`STALE_SNAPSHOT`), mientras que las repeticiones del diario devuelven el resultado guardado.

| Área | Tesis | Hoy | Se completa en |
|---|---|---|---|
| Detección de cambios, fusión, identidad, snapshots | Algoritmos 1, 2 y 3B; commit transaccional | Implementados en `ommatix-domain` y ejecutados por el `Engine` | S1–S2 (hecho) |
| Caché de íconos | Algoritmo 3A dentro de la observación | El `Engine` lo ejecuta sobre los candidatos a ícono de las regiones planificadas con los prototipos curados del operador; apagado sin ellos; contexto configurado, umbrales sin calibrar | S3 (contexto), S4 (calibración) |
| Percepción | Detector y OCR en ONNX Runtime, roles según el spike S0 | Por defecto, detector de regiones de color plano (roles `unknown`) y sin OCR (`NullTextRecognizer`); los escritorios simulados informan la verdad de terreno. Hay adaptadores opcionales para `images`, `screen` y `dxgi`, pero ningún modelo seleccionado: un detector ONNX que falla de forma segura, guiado por un manifiesto fijado por SHA-256 (feature `onnx`), y el OCR de Windows para un único idioma (feature `winrt-ocr`) | S0 (modelos), S3 |
| Contrato con el agente | Ommatix 1.0 por MCP 2026-07-28; 2025-11-25 opcional | Implementado (CT-01..10); guiado en vivo por los clientes mínimos `tools/s3/mcp_driver.py` y `tools/s4/e2e_driver.py` (20 corridas de extremo a extremo puntuadas, sólo con clics, [informe S4](../thesis/S4_ACEPTACION_REDUCIDA.md)), todavía no por un cliente de terceros | S0/S3 para el cliente |
| Compuerta de acciones | Vigencia, política, allowlist, foco, parada | Vigencia del snapshot (comprobada también con una captura antes de entregar), política y la lista permitida contra la aplicación en primer plano que atestigua `SendInput`, todo comprobado de nuevo justo antes de entregar; el archivo de parada del operador (`[safety] stop_file`) y, opcionalmente, el alcance del operador por ventanas (ADR-0020, propuesta: clases de ventana y una ventana registrada, comprobadas sobre la ventana en primer plano y la ventana bajo el punto); el actuador comprueba además el proceso bajo el punto objetivo, el nivel de integridad y los modificadores pulsados, y en `type` el foco de teclado tras el clic que enfoca el campo. Ejecutadas de forma supervisada sobre `notepad.exe` ([corridas de S3](../thesis/S3_CORRIDAS_SUPERVISADAS.md)): el archivo de parada, la comprobación de vigencia y la política rechazaron clics reales sin enviar nada; no se pudo provocar un cambio de aplicación en primer plano (SEC-02 y SEC-05 en parte) | S3 (corridas supervisadas) |
| Verificación del efecto | Postcondiciones de foco, valor, desplazamiento y cambio visual | Sólo cambio visual, con nuevas capturas dentro de un plazo acotado (`verify_timeout_ms`, `verify_poll_ms`); se añadió tras las corridas de S3, donde una única captura justo después de entregar nunca vio el efecto (4 de 4 `indeterminate`); confirmado en vivo en la segunda corrida (2 de 2 clics `effect_observed`; con un plazo de 0 ms, `indeterminate` sin reintento: IT-04 en parte) | S3 |
| Telemetría | Métricas y auditoría sin texto sensible | `get_engine_metrics`; tiempos por etapa en `_meta` | S3 |
| Plataforma | Windows 11 x64: DXGI y `SendInput` | Captura DXGI y entrada `SendInput` tras las features `dxgi` y `sendinput` (ADR-0016), compiladas y probadas en la integración continua en Windows ejecutadas en parte de forma supervisada ([corridas de S3](../thesis/S3_CORRIDAS_SUPERVISADAS.md): IT-01, IT-03 e IT-04, ocho clics reales sobre `notepad.exe` en dos corridas; el primer frame DXGI ya muestra el escritorio en vivo; IT-07 sin ejecutar) y en el protocolo S4 reducido (118 clics reales en la aplicación de prueba propia, el Bloc de notas y la Calculadora; `SendInput` atestigua la aplicación UWP hospedada, no `ApplicationFrameHost.exe`); escritorios simulado y de fixtures para las pruebas; `xcap` y `enigo` provisionales tras `screen` y `native-input` | S3 (corridas supervisadas) |

Los escenarios de la tesis (BDD-01..37) se quedan en [`docs/thesis/features/`](../thesis/features),
donde los espera la matriz de trazabilidad del dossier; `src/acceptance/tests/thesis.rs`
los ejecuta todos: BDD-01..10 y BDD-13..21 sobre el núcleo del dominio, BDD-11, 12 y 22..27
a través del motor y del adaptador MCP sobre los escritorios simulado y de fixtures, y
BDD-28..32 (otra aplicación que pasa a primer plano antes de escribir, verificación
indeterminada sin reintento, un `request_id` repetido, texto en pantalla que pide más
permisos, el archivo de parada del operador durante una secuencia) del mismo modo sobre un
doble de prueba del actuador real que registra lo que recibe y no envía nada. Muestran la
parte del motor de SEC-05, IT-04, IT-05, SEC-03 y SEC-02, que siguen necesitando corridas
supervisadas con el actuador real; la comprobación del foco de teclado dentro de la
aplicación permitida es del actuador `SendInput`, y ninguna prueba lo ejecuta. El `type`
en dos fases de la ADR-0018 (foco confirmado por el motor en el elemento objetivo, BDD-34 y
BDD-35) corre sobre dobles del actuador y del proveedor de accesibilidad. El alcance del
operador por ventanas ([ADR-0020](../adr/0020-operator-scope-on-windows.md), propuesta: clases
de ventana y una ventana registrada, para poder permitir `explorer.exe` sin el escritorio ni la
barra de tareas) son BDD-36 y BDD-37, sobre el mismo doble del actuador; su parte de Windows sólo
está compilada y analizada, no ejecutada en vivo.
