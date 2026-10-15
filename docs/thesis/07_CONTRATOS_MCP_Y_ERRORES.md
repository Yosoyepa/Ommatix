# 07. Contratos MCP, payloads y errores

## 7.1 Versiones y alcance de conformidad

El contrato de herramientas tiene versión **Ommatix 1.0**, independiente de la versión MCP. El perfil moderno de diseño es MCP **2026-07-28**. Lleva versión y capacidades en `params._meta`, no usa `initialize/initialized`, y sus resultados incluyen `resultType`. La compatibilidad con **2025-11-25** es opcional y usa un adaptador legacy con su propio ciclo [S01–S05]. No se declara soporte del ejemplo original 2024-11-05 por conservar un campo de versión.

El estado del motor —snapshots, tracks y sesiones de control— es estado de aplicación. Que el transporte MCP moderno sea stateless no obliga a descartar ese estado: cada llamada incluye referencias explícitas para identificarlo. Se debe comprobar la compatibilidad del SDK/cliente elegido antes de afirmar interoperabilidad.

El servidor moderno debe implementar `server/discover` y anunciar únicamente versiones/capacidades que realmente soporte. Los resultados de descubrimiento y `tools/list` incorporan `ttlMs` y `cacheScope`; el listado es determinista y no depende de una conexión implícita. La revisión de conformidad incluye estas obligaciones, no sólo `tools/call` [S01, S03–S04].

Se propone usar `rmcp` para el protocolo y schemas propios para el dominio. Los ejemplos JSONL son sintéticos. La validación local comprueba payloads, no conexión, cancelación, lifecycle ni conformidad integral de un servidor.

## 7.2 Transporte

Un mensaje UTF-8 por línea en stdin/stdout, sin logs en stdout. Diagnósticos por stderr. Una notification no recibe respuesta como si fuera request. El adaptador limita entrada a 64 KiB por mensaje y evita buffers no acotados. Las reglas exactas de error/cancelación se aplican según el perfil negociado o declarado [S02–S05].

No se implementa HTTP en el MVP. Cuando se agregue, usar la revisión de Streamable HTTP seleccionada y su modelo de autorización; no tratar el SSE legacy como equivalente automático.

## 7.3 observe_screen

Input: query opcional (256 caracteres), limit de 1 a 200, cursor opaco e include_values=false por defecto. La observación inicial retorna `session_id`, `snapshot_id`, `geometry_epoch`, frame_seq, age_ms, degraded, elementos y truncamiento explícito. Cada elemento expone target_id, revision, role, label, procedencia y accionabilidad; no coordenadas.

Se exige snapshot completo como capacidad MVP. El modo delta se difiere hasta tener base/expiración y bajas explícitas. La paginación permanece en el mismo snapshot; si expira, se rechaza el cursor y se solicita reobservación. Query sólo prioriza/filtra la vista, no borra entidades del registro interno. Se informa truncamiento para que el agente no asuma que ve todos los controles.

La procedencia de la etiqueta (`label_source`) puede ser `accessibility` cuando el motor tiene activada la evidencia de UI Automation (ADR-0017): el rol y el nombre vienen de la aplicación, y la accionabilidad `confirmed` sigue exigiendo un elemento habilitado, en pantalla y con un patrón de acción; el valor `accessibility` ya figuraba en el enum y no cambia ningún schema. Los tiempos por etapa, incluida `accessibility_us`, van sólo en `_meta`, no en `structuredContent`. Roles unknown, valores null y etiquetas desconocidas son resultados válidos. `text_kind` distingue contenido de valor/placeholder sólo con evidencia. El agente no debe inferir accionabilidad de que el elemento tenga ID. La política puede redactar contenidos incluso si el cliente solicita include_values.

## 7.4 execute_action

Campos obligatorios: request_id, session_id, snapshot_id, geometry_epoch, target_id, expected_revision y action. Acciones MVP: click, type y scroll. `type` requiere text; `scroll`, direction y steps acotados; click no admite texto ni coordenadas. `dry_run` se aplica como true si se omite. Un valor false no habilita el actuador sin consentimiento local y allowlist.

La precondición de snapshot es estricta en el MVP. Una petición vieja no se redirige a otro target cercano. Tras una nueva observación, el agente selecciona explícitamente de nuevo y usa otro request_id si cambia la intención. El diario de deduplicación asocia request_id con hash de argumentos; un mismo ID con argumentos diferentes produce REQUEST_CONFLICT. La vigencia (ADR-0025) forma parte de esa precondición: al autorizar, la captura del snapshot y la lectura de accesibilidad que confirma el objetivo no pueden tener más de `[engine] max_snapshot_age_ms` (10 000 ms por defecto), medidos en el reloj inyectado. La comprobación se repite justo antes de cada entrada real; en simulación se hace en la puerta, con la misma respuesta, y la simulación nunca afirma entrega. Vencido el plazo, la respuesta es `STALE_SNAPSHOT` y no se envía nada. Una publicación nueva no renueva una lectura de accesibilidad antigua: la siguiente observación relee sólo las regiones cuya evidencia ha caducado.

`expected_effect` permite una postcondición limitada: foco, texto de prueba, desplazamiento o cambio visual. Confirmar un cambio visual no equivale a confirmar un objetivo de negocio. El cambio visual se busca dentro del plazo de verificación configurado por el operador (`[engine] verify_timeout_ms` y `verify_poll_ms`, SDD §2.9); es el "plazo" de BDD-29. Si vence sin evidencia, el resultado es `indeterminate` con `INDETERMINATE_EFFECT` y `next_step: reconcile_effect`, y la entrada no se reenvía. No se permiten instrucciones arbitrarias de shell o afirmaciones del agente que reemplacen la política de seguridad.

Nota sobre `type` (ADR-0018). Con la evidencia de UI Automation activada, el motor entrega `type` en dos fases: el clic de enfoque y, sólo cuando el foco de teclado está en el elemento objetivo dentro del plazo de verificación, las teclas. Si el foco no se confirma a tiempo no se envía ninguna tecla y el resultado es `indeterminate` con `INDETERMINATE_EFFECT`, `retryable: false` y `next_step: reconcile_effect`; `input_delivered` es `true` porque el clic de enfoque sí se entregó, y la entrega de las teclas no se afirma. Si tras el clic cambian la parada del operador, el consentimiento o la aplicación en primer plano, el resultado es el mismo y tampoco se deshace nada. Lo mismo ocurre si vence la vigencia de la observación (ADR-0025) entre el clic y las teclas: no se envía ninguna tecla, `input_delivered` es `true` y el resultado es `indeterminate`. Con `expected_effect: visual_change`, el cambio se mide contra un fotograma capturado después del clic de enfoque y antes de las teclas, no contra el anterior al clic, así que el redibujado del clic no cuenta como efecto de escribir. El contrato y los schemas no cambian, y el valor del campo no se lee. Sin evidencia de accesibilidad, `type` conserva la comprobación de foco a nivel de ventana del actuador.

## 7.5 Estados de respuesta

| Estado Ommatix | Significado | Lo que no afirma |
|---|---|---|
| simulated | Validación/preparación sin entrada real | No acredita efecto en el SO |
| rejected | No se envió entrada por precondición/política | No es un éxito de tarea |
| input_delivered | El actuador reportó entrega | No prueba que la UI obedeciera |
| effect_observed | Se verificó la postcondición declarada | No prueba objetivos no verificados |
| indeterminate | No hay certeza suficiente del efecto/entrega | No autoriza retry automático |

Los resultados incluyen `input_delivered` y `verified` para evitar el ambiguo `status: ok` original. Las coordenadas ejecutadas no se exportan por defecto; pueden existir en diagnóstico local protegido. El modo simulado tiene prohibido marcar input_delivered=true. En `indeterminate`, `input_delivered` dice si el actuador reportó la entrega completa de los eventos: `true` cuando sólo falta verificar el efecto declarado, `false` cuando la propia entrega es incierta (fallo del actuador o eventos no aceptados por el SO); `verified` es siempre `false`. En los demás estados ambos campos se deducen del estado.

## 7.6 get_engine_metrics y recortes

`get_engine_metrics` informa contadores, estado de modelos, regiones pendientes y origen de tiempos. No retorna números de latencia inventados al arrancar: usa `not_measured` cuando no hay observaciones instrumentadas. El desglose ampliado se consulta localmente y no necesita enviarse en cada turno al LLM.

`get_element_crop`, mencionado en las primeras páginas del PDF, se conserva como extensión opcional. Requiere snapshot/target vigentes, permiso para exponer imágenes, tamaño acotado y redacción; no es requisito de aceptación del core textual. No se añade un schema que aparente una implementación inexistente.

## 7.7 Taxonomía de errores

Errores de transporte/protocolo usan los códigos JSON-RPC/MCP del perfil: parseo, request inválida, método desconocido o versión no soportada. Errores esperados del dominio se devuelven como resultado de herramienta con `isError: true` y código Ommatix [S04]. No convertir un target inexistente en una excepción interna ni devolver éxito vacío.

Códigos de vigencia: SESSION_MISMATCH, EPOCH_MISMATCH, STALE_SNAPSHOT, STALE_TARGET y TARGET_NOT_FOUND. Códigos de decisión: AMBIGUOUS_TARGET, NOT_ACTIONABLE y POLICY_DENIED. Códigos de adaptadores: PERMISSION_DENIED, CAPTURE_UNAVAILABLE e INFERENCE_FAILED. Códigos operativos: UNSUPPORTED_GEOMETRY, REQUEST_CONFLICT, CANCELLED, LIMIT_EXCEEDED e INDETERMINATE_EFFECT.

La respuesta incluye next_step (`observe_again`, `operator_review`, `reduce_request`, `reconcile_effect` o `stop`) y retryable. Para acciones con efectos se evita interpretar retryable como reenvío ciego. `STALE_SNAPSHOT` pide observar y reformular, no repetir exactamente la llamada. También lo devuelve la vigencia (ADR-0025): una captura o una lectura de accesibilidad más antiguas que `max_snapshot_age_ms` no autorizan la acción, y el mensaje lo dice; la respuesta no cambia de código ni de esquema.

## 7.8 Compatibilidad y costes de contexto

`structuredContent` contiene el objeto Ommatix validable; el bloque textual puede serializar el mismo objeto para clientes que lo necesiten. El adaptador debe evitar que el host duplique ambos innecesariamente en el contexto del modelo. El coste se mide con el tokenizador y cliente reales; cinco dígitos no garantizan ahorro frente a una cadena corta.

Los schemas tienen `additionalProperties: false` en el payload de dominio, límites de texto y condiciones por acción. La validez JSON no confirma que IDs sean únicos, que el snapshot exista, que un target siga visible o que una acción esté autorizada. Esas invariantes pertenecen al runtime y tienen tests separados.
