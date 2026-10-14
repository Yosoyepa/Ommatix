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

El SDD define once componentes ([diagrama](../diagrams/components.md)).
[ADR-0014](../adr/0014-hexagonal-crate-layout.md) los ubica en crates:

| Componente | Responsabilidad | Contrato | Crate |
|---|---|---|---|
| MCP Adapter | Validar y enrutar llamadas; describir herramientas | Entradas y salidas versionadas | `ommatix-adapter-mcp` |
| Observation Coordinator | Captura, planificación de regiones y commit | SnapshotRequest → Snapshot | `ommatix-application` (`Engine::observe_screen`) |
| Capture Provider | Píxeles, stride, tiempos, rectángulos dirty y move | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR y transformaciones de imagen | Region → RawEvidence | `ommatix-adapter-vision` |
| Spatial Fusion | Contención, renglones, etiquetas y relaciones | RawEvidence → Candidates | `ommatix-domain` |
| Icon Resolver | Recuperación contextual con rechazo | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisiones y bajas | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Vigencia, permisos y precondiciones | ActionRequest → Permit o Reject | `ommatix-application` (`Engine::execute_action`) |
| OS Actuator | Inyección de eventos y conteo de entrega | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Comprobar un efecto esperado o informar incertidumbre | Antes y después → Verification | `ommatix-application`: nueva captura tras la entrega (parcial; S3) |
| Telemetry | Métricas y auditoría sin texto sensible | Eventos y spans | `get_engine_metrics` y tiempos por etapa en el `_meta` de MCP (parcial; S3) |

El núcleo sólo llega al exterior mediante los puertos `FrameCapturer`, `VisionDetector`,
`TextRecognizer`, `OsActuator`, `Clock` y `PolicyStore`, de modo que cada adaptador puede
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
3. **Percibir** las regiones planificadas con el detector y el OCR.
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
  sentidos. Los tiempos por etapa van en el `_meta` del resultado (`io.ommatix/timings`),
  nunca en `structuredContent`.

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
`ElementDetector`, `InputController`), sus siete herramientas MCP con coordenadas y sus
features Gherkin; sus decisiones archivadas siguen en
[`docs/adr/bootstrap/`](../adr/bootstrap/README.md).

**`observe_screen`** captura un frame, planifica regiones contra la última base confirmada
(algoritmo 1), entrega al detector y al reconocedor sólo las regiones planificadas, conserva
la evidencia fuera de ellas, fusiona (algoritmo 2), sigue las identidades (algoritmo 3B) y
prepara y confirma el snapshot. Una etapa fallida no publica nada, deja pendientes los tiles
planificados y bloquea las acciones. Las páginas se filtran por `query`, se redactan salvo que
la política local permita valores y quedan atadas a su snapshot por el cursor.

**`execute_action`** comprueba los argumentos, busca el `request_id` en un diario de
deduplicación acotado (hash FNV-1a de los argumentos canónicos, de modo que nunca se guarda el
texto tecleado; el mismo ID con otros argumentos es `REQUEST_CONFLICT`) y después comprueba de
forma estricta sesión, epoch geométrico, snapshot, target, revisión y accionabilidad contra el
snapshot publicado. Un dry run (por defecto) es `simulated`. La entrada real exige la política
del operador (`actuation_enabled`, sin parada) **y** un actuador real; si no, `POLICY_DENIED`
sin entregar nada. Un recibo de entrega cuyos eventos insertados coinciden con los enviados es
`input_delivered`; cualquier otra cosa es `indeterminate`. Un `visual_change` esperado se
verifica con una nueva captura comparada con la base confirmada (`effect_observed` o
`indeterminate`); los demás efectos se informan `indeterminate` hasta que S3 pueda
comprobarlos.

| Área | Tesis | Hoy | Se completa en |
|---|---|---|---|
| Detección de cambios, fusión, identidad, snapshots | Algoritmos 1, 2 y 3B; commit transaccional | Implementados en `ommatix-domain` y ejecutados por el `Engine` | S1–S2 (hecho) |
| Caché de íconos | Algoritmo 3A dentro de la observación | Implementado en `ommatix-domain` (`appearance`), todavía fuera de la observación: ningún detector informa íconos sin texto | S3 |
| Percepción | Detector y OCR en ONNX Runtime, roles según el spike S0 | Detector de regiones de color plano (roles `unknown`), sin OCR (`NullTextRecognizer`); los escritorios simulados informan la verdad de terreno | S0, S3 |
| Contrato con el agente | Ommatix 1.0 por MCP 2026-07-28; 2025-11-25 opcional | Implementado (CT-01..10); sin probar todavía con un cliente real | S0/S3 para el cliente |
| Compuerta de acciones | Vigencia, política, allowlist, foco, parada | Vigencia, política y parada; `allowed_applications` no se aplica (aún no hay evidencia de la ventana en primer plano); sin comprobación de foco | S3 |
| Verificación del efecto | Postcondiciones de foco, valor, desplazamiento y cambio visual | Sólo cambio visual, con una nueva captura | S3 |
| Telemetría | Métricas y auditoría sin texto sensible | `get_engine_metrics`; tiempos por etapa en `_meta` | S3 |
| Plataforma | Windows 11 x64: DXGI y `SendInput` | Escritorios simulado y de fixtures; captura con `xcap` y entrada con `enigo` detrás de features de compilación | S3 |

Los escenarios de la tesis (BDD-01..32) se quedan en [`docs/thesis/features/`](../thesis/features),
donde los espera la matriz de trazabilidad del dossier; `src/acceptance/tests/thesis.rs`
ejecuta BDD-01..27: BDD-01..10 y BDD-13..21 sobre el núcleo del dominio, y BDD-11, 12 y 22..27
a través del motor y del adaptador MCP sobre los escritorios simulado y de fixtures.
BDD-28..32 esperan a S3.
