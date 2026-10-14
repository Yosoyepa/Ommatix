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
| Observation Coordinator | Captura, planificación de regiones y commit | SnapshotRequest → Snapshot | `ommatix-application` |
| Capture Provider | Píxeles, stride, tiempos, rectángulos dirty y move | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR y transformaciones de imagen | Region → RawEvidence | `ommatix-adapter-vision` |
| Spatial Fusion | Contención, renglones, etiquetas y relaciones | RawEvidence → Candidates | `ommatix-domain` |
| Icon Resolver | Recuperación contextual con rechazo | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisiones y bajas | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Vigencia, permisos y precondiciones | ActionRequest → Permit o Reject | `ommatix-application` |
| OS Actuator | Inyección de eventos y conteo de entrega | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Comprobar un efecto esperado o informar incertidumbre | Antes y después → Verification | `ommatix-application` |
| Telemetry | Métricas y auditoría sin texto sensible | Eventos y spans | puerto en `ommatix-application` y un adaptador |

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

- **Perfiles:** MCP 2026-07-28 mediante el SDK `rmcp` (`server/discover`, sin `initialize`,
  versión y capacidades en `_meta`); 2025-11-25 es opcional
  ([ADR-0002](../adr/0002-mcp-stdio-with-explicit-profiles.md)).
- **Herramientas:** `observe_screen` (consulta, paginación, sin coordenadas),
  `execute_action` (`click`, `type`, `scroll`) y `get_engine_metrics`.
- **Errores:** llevan un código (`STALE_SNAPSHOT`, `STALE_TARGET`, `AMBIGUOUS_TARGET`,
  `POLICY_DENIED`, `REQUEST_CONFLICT` y otros) y un `next_step` como `observe_again` u
  `operator_review`. Los mensajes se limitan a 64 KiB.

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

Casi todo el código de `src/` se escribió al arrancar el repositorio, antes de consolidar la
tesis. Ya sigue la organización en crates, el reloj inyectado, stdout reservado a MCP y la
entrada en dry-run por defecto, pero no el pipeline de la tesis.

S1 añadió el núcleo de la tesis a `ommatix-domain`, junto al prototipo: `incremental`
(algoritmo 1: diff exacto por tiles, plan de regiones, cierre halo, refresco completo de
respaldo), `snapshot` (staging y commit atómico, tiles sucios pendientes, estado degradado) y
`fusion` (algoritmo 2: propiedad por IoC, orden de lectura, labels externos, abstención,
accionabilidad). Están probados (UT-01..26, BDD-01..10) pero aún no se conectan al pipeline
en ejecución; eso ocurre cuando S2 y S3 reemplacen el caso de uso y los adaptadores del
prototipo.

| Área | Tesis | Prototipo en `src/` | Se reemplaza en |
|---|---|---|---|
| Detección de cambios | Tiles de 64×64 exactos contra la base confirmada, cierre halo con presupuesto | Celdas de 32 px con hash contra el frame anterior, expansión por margen de contexto | Núcleo en S1; se conecta en S2–S3 |
| Fusión | Propiedad por IoC, orden de lectura, labels externos con abstención | No hay (sin texto) | Núcleo en S1; se conecta en S3 con el OCR |
| Percepción | Detector y OCR en ONNX Runtime, roles según el spike S0 | Detector de regiones de color plano, sin texto, todo elemento es `blob` | S0, S3 |
| Estado | Snapshots inmutables, staging, commit atómico, epoch geométrico | Una escena mutable | Núcleo en S1; se conecta en S2 |
| Identidad | IDs de sesión, revisiones, tombstones, matching uno a uno con abstención | IDs monotónicos emparejados por IoU | S2 |
| Contrato con el agente | `observe_screen`, `execute_action`, `get_engine_metrics`; sin coordenadas | Siete herramientas que exponen coordenadas; MCP 2025-06-18 | S2 |
| Plataforma | Windows 11 x64: DXGI y `SendInput` | `xcap` y `enigo` en macOS, Linux X11 y Windows al 100 % | S3 |

Las features Gherkin del prototipo en `src/acceptance/features/` y sus decisiones archivadas
en [`docs/adr/bootstrap/`](../adr/bootstrap/README.md) lo describen hasta que se reemplace. Los
escenarios de la tesis (BDD-01..32) se quedan en [`docs/thesis/features/`](../thesis/features),
donde los espera la matriz de trazabilidad del dossier; `src/acceptance/tests/thesis.rs`
ejecuta los que cada etapa vuelve ejecutables (BDD-01..10 desde S1).
