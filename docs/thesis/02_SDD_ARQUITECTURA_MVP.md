# 02. Software Design Document: arquitectura del MVP

## 2.1 Propósito, estado y alcance

Se conserva la dirección de la conversación: un runtime Rust local, un estado espacial en RAM y una interfaz MCP que permite planificar sin generar coordenadas [F0, pp. 22–24]. Esta revisión especifica **un MVP funcional de control visual**; no confunde el prototipo de algoritmos entregado con el producto terminado.

**Supuesto aprobado el 14 de octubre de 2026 ([ADR-0008](../adr/0008-one-real-platform-first.md)):** primera plataforma Windows 11 x64, escritorio interactivo local, un monitor seleccionado, SDR/BGRA y aplicaciones de prueba sin privilegios elevados. Resolución base 1920×1080; escalas de validación 100%, 125% y 150%. Interfaces en español e inglés, lectura horizontal LTR. Las aplicaciones de prueba siguen pendientes de aprobación. Otras plataformas se mantienen como puertos futuros, no como capacidad implícita.

El alcance mínimo incluye captura real, detección/OCR reales, los tres bloques core, identidad persistente, observación MCP, click/type/scroll bajo política y verificación posterior. Se excluyen entrenamiento desde cero, control de UAC/escritorio seguro, ejecución remota por red, soporte universal de aplicaciones, drag-and-drop y macros especulativas. La clasificación semántica abierta de iconos no es obligatoria: el estado desconocido es válido.

## 2.2 Contexto y límites de confianza

El usuario autoriza un host/agente a usar una instancia del motor. El agente entrega intenciones por MCP y recibe datos estructurados potencialmente sensibles. El sistema operativo y las aplicaciones cambian de forma asíncrona; su contenido visual se trata como **datos no confiables**, no como instrucciones de política. Un texto OCR que ordene “ignora tus reglas” no cambia permisos.

La instancia está vinculada a una sesión de usuario y a un monitor/ventana permitido. La política local, fuera del control del agente, decide qué aplicaciones y acciones están habilitadas. No se abre un puerto TCP en el MVP. El transporte stdio no elimina riesgos de un cliente local malicioso; se restringen su lanzamiento y el acceso a las credenciales del usuario.

## 2.3 Estructura de componentes

| Componente | Responsabilidad | Contrato principal |
|---|---|---|
| MCP Adapter | Validar y enrutar llamadas; describir herramientas | Tool inputs/outputs versionados |
| Observation Coordinator | Captura, planificación de regiones y commit | SnapshotRequest → Snapshot |
| Capture Provider | Pixels, stride, tiempos, dirty/move rects | FrameLease |
| Perception Provider | Detector, OCR y transformaciones de imagen | Region → RawEvidence |
| Spatial Fusion | Contención, renglones, etiquetas y relaciones | RawEvidence → Candidates |
| Icon Resolver | Recuperación contextual con rechazo | CropDescriptor → LabelEvidence |
| Temporal Registry | IDs, revisiones y bajas | Candidates + Snapshot → Tracks |
| Policy & Action Gate | Vigencia, permisos y precondiciones | ActionRequest → Permit/Reject |
| OS Actuator | Inyección de eventos y conteo de entrega | ValidatedAction → DeliveryReceipt |
| Effect Verifier | Comprobar un efecto esperado o incertidumbre | Before/After → Verification |
| Telemetry | Métricas y auditoría sin texto sensible | Events/Spans |

Se propone un **monolito modular**, no una colección de microservicios. Los frames se pasan como leases/buffers con propietario explícito; los índices y tablas se mantienen en memoria. No se necesita Redis para unos cientos de elementos. Separar captura/inferencia en procesos sólo se reconsiderará si aislamiento o perfiles reales lo justifican.

## 2.4 Puertos y adaptadores

La lógica core no depende directamente de DXGI, ONNX ni MCP. Los puertos `FrameCapturer`, `VisionDetector`, `TextRecognizer`, `OsActuator`, `Clock` y `PolicyStore` permiten sustituir adaptadores por fixtures deterministas. Los resultados deben incluir procedencia, frame/region de origen y calidad de evidencia.

**Windows:** DXGI es el candidato de captura; primero se aprovechan move/dirty rects nativos y después el diferenciador propio como respaldo o verificación [S06]. El actuador candidato es SendInput, con recuento de eventos y restricciones documentadas [S07]. Las llamadas se hacen en el contexto de usuario, sin técnicas para evadir controles de seguridad.

**Inferencia:** ONNX Runtime detrás de un puerto; CPU es baseline, DirectML una opción inicial de Windows y WinML una evolución evaluable. DirectML tiene restricciones de concurrencia por sesión y de configuración que el adaptador debe respetar [S09–S10]. Los modelos/pesos se eligen en el spike S0; ninguna tabla de roles se inventa a partir del nombre “YOLO”. Un detector binario interactuable/no-interactuable no se convierte automáticamente en detector de inputs, dropdowns y checkboxes.

**MCP:** preferencia por SDK `rmcp`, fijando versión compatible con el perfil acordado [S23]. El protocolo 2026-07-28 y el legacy 2025-11-25 tienen reglas distintas; se documentan en el contrato de transporte. No se implementa JSON-RPC casero como sustituto de toda la conformidad MCP.

## 2.5 Modelo de datos interno

**Frame:** dimensiones físicas, stride en bytes, formato BGRA8, instante monotónico de captura, identidad de monitor, rotación, epoch geométrico y procedencia de regiones de cambio. La longitud y los productos de dimensiones se validan con aritmética comprobada. No se asume que `row_stride == width*4`.

**RawEvidence:** bbox/polígono, tipo de evidencia, texto opcional, score del proveedor, transformación del tensor al recorte, detector/modelo/revisión y `source_frame_id`. Los scores se conservan por separado para no multiplicarlos como probabilidades independientes sin calibración.

**Element:** `target_id` opaco, revisión monotónica, rol o `unknown`, etiqueta con procedencia, contenido interno con `text_kind`, bbox física, soporte de evidencia, grupo/ventana, visibilidad, habilitación y `actionability`. La geometría permanece interna; se puede exportar sólo en diagnóstico local explícito.

**Snapshot:** `session_id`, `snapshot_id`, `geometry_epoch`, `captured_at`, `published_at`, elementos, relaciones, estado de degradación y métricas. Las listas tienen orden estable por ID o por clave de lectura; no dependen del orden arbitrario de un HashMap.

## 2.6 Identidad y consistencia temporal

Los IDs se asignan al nacer un track, no se recalculan desde su posición. Un contador creciente de sesión evita colisiones por hash y no se reinicia al insertar un popup. La sesión forma parte del alcance de la referencia. La revisión aumenta si cambia un dato que afecta una acción: geometría, contenido discriminante, visibilidad, habilitación o contexto.

Se realizan asociaciones uno-a-uno con gates de contexto y opción de no asignación. Ante filas duplicadas, virtualización, desaparecidos o baja evidencia, se crea un track nuevo o se marca ambiguo. No se reancla silenciosamente una acción. Los tombstones impiden reutilizar IDs retirados durante la misma sesión.

**Política MVP estricta:** una acción debe referenciar el último snapshot publicado, la revisión del target y el epoch vigentes. Un popup no cambia la identidad del resto de tracks, pero puede volver obsoleta una observación para actuar. Se pide una nueva observación. Más adelante puede permitirse validación por región inalterada, tras demostrar su corrección.

## 2.7 Concurrencia y publicación transaccional

Un coordinador es el único escritor del estado publicado. Captura, detector y OCR pueden trabajar de forma concurrente, pero sus resultados están etiquetados con el frame y la configuración. Se proponen colas acotadas y política latest-wins para capturas aún no procesadas; una acción nunca se descarta sin respuesta explícita.

El procesamiento regional se construye en un estado candidato. Sólo al terminar validaciones se publica de forma atómica un snapshot inmutable, incluyendo registro e índice coherentes. Si una región falla, se preserva el estado anterior para inspección y se marca degradación; se bloquea actuar sobre evidencia obsoleta. No se adelanta el framebuffer base confirmado mientras haya trabajo sucio pendiente.

Un cambio de monitor, DPI, rotación, ventana fuera de política o configuración de modelos incrementa el epoch y dispara reconstrucción completa. Resultados de inferencias con epochs viejos se descartan sin publicarse. La cancelación impide nuevas etapas; si una entrada ya se entregó, el resultado debe reflejarlo y no fingir rollback.

## 2.8 Coordenadas y transformaciones

El core usa píxeles físicos del monitor seleccionado y cajas semiabiertas. Para cada inferencia se guarda la transformación completa: recorte, escala, padding, rotación y traslación a escritorio. Para letterboxing, `x_crop=(x_tensor-pad_x)/scale` y análogamente para Y; después se aplica el origen del recorte y, cuando corresponda, el del monitor. Los valores se recortan a límites válidos antes de elegir un punto.

El punto se selecciona dentro de una región visible válida y con margen a bordes. El centroide es un candidato, no una prueba de clic seguro. Elementos tapados, deshabilitados, degenerados o de accionabilidad desconocida no reciben permiso. El adaptador de Windows declara su política DPI y prueba las conversiones, incluida la normalización necesaria para la API de entrada [S08].

Aunque el MVP opere un monitor, las estructuras deben permitir orígenes de escritorio negativos. La compatibilidad multimonitor, HDR y rotaciones adicionales queda fuera de aceptación inicial hasta contar con casos reales.

## 2.9 Ejecución y verificación

Flujo: validar schema → deduplicar request_id → comprobar política → comprobar sesión/snapshot/revisión/epoch → confirmar ventana/foco y frescura → preparar acción → registrar intención → inyectar eventos → comprobar entrega → observar efecto esperado → registrar resultado.

`type` requiere un control de entrada confirmado y foco verificado. No se escribe en un campo sólo porque exista texto cercano. `scroll` declara dirección y magnitud limitada. `click` no obtiene confirmación implícita de efectos destructivos por ser un clic. El MVP sólo actúa dentro de aplicaciones y flujos controlados.

El bloqueo interno evita carreras del propio proceso, no congela el escritorio. Existe una ventana residual entre verificar e inyectar. Se reduce con revalidación inmediata y se documenta; no se promete imposibilidad física de clics equivocados en interfaces arbitrarias.

La verificación distingue: eventos entregados, cambio visual detectado y postcondición comprobada. Son evidencias diferentes. Un fallo de verificación produce `indeterminate`, no un reintento automático. Para operaciones con efectos, un request_id repetido devuelve el resultado almacenado; una caída después de enviar entrada exige reconciliación, no suposición de exactly-once.

## 2.10 Rendimiento y degradación

Se miden por separado captura, preparación, detección, OCR, fusión, tracking, serialización y verificación. El límite del core no incluye modelos; el de percepción sí. La inferencia del planificador y el tiempo de aplicación se miden fuera de esos segmentos. El selector regional compara número de inferencias y shapes contra el coste full-frame; área menor no implica cómputo menor.

Ante saturación se reduce frecuencia de observación, se agrupan regiones, se usa full-frame cuando convenga o se informa degradación. No se reduce seguridad para mantener FPS. La meta de actualizaciones pequeñas sub-10 ms se conserva como objetivo de optimización del núcleo, no como promesa del sistema completo.

## 2.11 Distribución y operación

El paquete productivo incluirá binario, librerías nativas necesarias, configuración, pesos autorizados o instrucciones de obtención, hashes y avisos. “Rust” no implica que ONNX y los modelos queden mágicamente embebidos en un ejecutable estático. Un chequeo inicial informa versión de runtime, proveedor, modelos disponibles y permisos; si falta un requisito, falla de forma explicativa y deja actuadores deshabilitados.

El proceso emite sólo mensajes de protocolo en stdout; diagnósticos van a stderr. Se limita el tamaño de solicitudes, resultados, textos, recortes y cachés. No hay retención de screenshots por defecto. Se soporta parada local independiente del agente.
