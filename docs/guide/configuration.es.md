# Configuración

[English](configuration.md) · [Primeros pasos](getting-started.es.md)

## Features de compilación, todas apagadas por defecto:

| Feature | Añade | Plataforma |
|---|---|---|
| `screen` | `--source screen`: el monitor principal con `xcap` (provisional) | Windows, macOS, Linux |
| `native-input` | `--input native`: entrada con `enigo` (provisional; implica `screen`; nunca actúa de verdad, ver abajo) | Windows, macOS, Linux |
| `dxgi` | `--source dxgi`: DXGI Desktop Duplication de una salida | Windows |
| `sendinput` | `--input sendinput`: `SendInput` de Win32 (implica `dxgi`) | Windows |
| `onnx` | el detector ONNX de `[models]`; ONNX Runtime se carga al ejecutar desde la biblioteca que fija el operador, nada se descarga al compilar | cualquiera |
| `directml` | `[models] detector_execution_provider = "directml"`: el detector sobre DirectML (implica `onnx`); necesita una compilación de ONNX Runtime con DirectML y una `DirectML.dll` fijada | Windows |
| `winrt-ocr` | `[models] ocr = "winrt"`: el OCR incluido en Windows | Windows |
| `uia` | `[models] accessibility = "uia"`: el árbol de UI Automation de la ventana en primer plano, como evidencia fusionada con la visual ([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)); exige `--source dxgi` o `screen` | Windows |

`serve` y `demo` aceptan las mismas opciones:

| Opción | Valores | Por defecto |
|---|---|---|
| `--config RUTA` | un archivo de configuración (abajo) | los valores de la plantilla del dossier |
| `--source` | `sim` (escritorio simulado, con su propio detector y reconocedor), `images` (frames PNG), `screen` (monitor principal; feature `screen`) o `dxgi` (un monitor con DXGI Desktop Duplication, en píxeles físicos, frames sólo en memoria; feature `dxgi`, Windows). Las fuentes reales usan el detector y el OCR de `[models]` | `sim` |
| `--images DIR` | el directorio de PNG para `--source images` | ninguno (`images` lo exige) |
| `--output N` | la salida que duplica `--source dxgi`, en el orden de DXGI (la 0 suele ser el monitor principal); se rechaza una salida rotada | `0` |
| `--input` | `dry-run` (registra, nunca toca el SO), `native` (`enigo`; feature `native-input`, fuente `screen`) o `sendinput` (`SendInput` de Win32; feature `sendinput`, fuente `dxgi`) | `[engine] mode` (`dry_run`) |
| `--theme` | `light`, `dark`, `contrasting-wallpaper` (sólo el escritorio simulado) | `light` |
| `--session-id ID` | de 1 a 64 caracteres ASCII visibles, para una salida reproducible; con actuación viva, un ID fijo se rechaza al arrancar (capítulo 08 §8.4) | derivado de la hora de inicio y del ID del proceso |
| `--legacy` | sirve además el perfil MCP legacy 2025-11-25 (como `[mcp] legacy_enabled = true`) | apagado |
| `--observations N` (sólo `demo`) | 1–20 | 3 |
| `--plan-trace` (sólo `demo`) | escribe además una línea por observación en stderr, también de la que falló: `ommatix-plan-trace` y un objeto JSON (esquema `ommatix.plan-trace/2`) con por qué el planificador eligió ese trabajo, sólo con conteos, áreas y nombres de clase; cada campo se define [abajo](#la-línea-de---plan-trace). Ningún píxel, texto ni coordenada; nunca por stdout, y ningún cliente MCP puede pedirlo | apagado |
| `--plan-check` (sólo `demo`) | compara además cada observación regional con una lectura completa del mismo fotograma, hecha después de confirmar la observación, y escribe una línea por observación en stderr: `ommatix-plan-check` y un objeto JSON (esquema `ommatix.plan-check/1`) con cuántas cajas tiene cada lectura que la otra no ([ADR-0045](../adr/0045-support-aware-closure.md)); cada campo se define [abajo](#la-línea-de---plan-check). Es un diagnóstico del operador: ejecuta la percepción dos veces, así que una observación regional cuesta alrededor del doble, y su tiempo nunca se informa como el de la observación. Sólo conteos; nunca por stdout, y ningún cliente MCP puede pedirlo | apagado |

### La línea de `--plan-trace`

Cada línea es `ommatix-plan-trace ` y un objeto JSON en la misma línea (claves ordenadas). Sale del
planificador de la observación (Algoritmo 1) y sólo lleva conteos, áreas y nombres de una lista cerrada.
**Las áreas están en píxeles del frame y ninguna lo excede.** Un campo marcado *unión* cuenta un píxel
una sola vez por más cajas que lo cubran; *una caja* es el área de un solo rectángulo. `null` significa
que el planificador no llegó a medir el campo (véase «Cuándo es `null` o está vacío»).

| Campo | Unidad | Significado |
|---|---|---|
| `schema` | texto | `ommatix.plan-trace/2` (el esquema 1, anterior a [ADR-0045](../adr/0045-support-aware-closure.md), no tenía `chosen`, `spanning`, `component_support_classes` ni `region_support_classes`) |
| `observation_id` | número | la posición de la observación en esta corrida de `demo`, desde 1 (como en `--timings`) |
| `outcome` | texto | `ok`, o `error` si la observación falló (no se publicó nada) |
| `error_code` | texto | el código de error del contrato de una observación fallida (por ejemplo `CAPTURE_UNAVAILABLE`); `null` si `outcome` es `ok` |
| `work` | texto | el trabajo del plan: `full`, `regions` o `idle` (como en el registro de la observación) |
| `full_refresh_reason` | texto | la razón de un plan `full` (`no_committed_base`, `geometry_changed`, `closure_budget`, `too_many_regions`, `area_budget`, `experimental_baseline`, `adaptive_cost`); `null` en otro caso |
| `frame_area_px` | píxeles | el área del frame, ancho por alto |
| `tile_size` | píxeles | el lado de una tesela (`[engine] tile_size`) |
| `changed_tiles` | conteo | teselas que difieren de la base confirmada; todas si no había base o su geometría cambió |
| `pending_tiles` | conteo | teselas de la rejilla de este frame aún pendientes de una observación fallida, hayan cambiado o no |
| `dirty_tiles` | conteo | las teselas que el plan deja sucias: cambiadas y pendientes, cada una una vez; toda la rejilla si se decidió un refresco completo antes de cualquier región |
| `components` | lista | una entrada por grupo 8-conexo de teselas sucias, en el orden del planificador, tal como quedó tras el primer cierre sobre la evidencia del último snapshot |
| `components[].tiles` | conteo | teselas del grupo |
| `components[].tight_area_px` | píxeles, una caja | la caja alrededor de las teselas del grupo, antes del halo |
| `components[].padded_area_px` | píxeles, una caja | esa caja tras el halo, recortada al frame: la región **antes** del cierre |
| `components[].closed_area_px` | píxeles, una caja | la región **después** del cierre |
| `components[].largest_support_px` | píxeles, una caja | la mayor caja de evidencia (un control detectado, una palabra, un elemento de accesibilidad o un miembro de una relación) que la región cerrada solapa; `0` si ninguna. Con el cierre clásico la región la contiene; con el cierre consciente de los soportes puede ser una superficie abarcada o una caja de accesibilidad que la región solapa sin contener |
| `regions` | lista | las regiones finales del plan tras fusionar y el último cierre, en el orden del plan |
| `regions[].area_px` | píxeles, una caja | la región |
| `regions[].components` | conteo | cuántos grupos terminaron en esta región |
| `regions[].pre_closure_area_px` | píxeles, **unión** | de qué estaba hecha la región antes de todo cierre o fusión: la unión de las cajas con halo de esos grupos, con cada solape contado una vez; como máximo `area_px` |
| `regions[].largest_support_px` | píxeles, una caja | como `components[].largest_support_px`, para la región final |
| `covered_area_px` | píxeles, suma | la suma de las áreas de las regiones, que es lo que compara el presupuesto de área; las regiones no se solapan, así que también es su unión |
| `budget_area_px` | píxeles | `full_refresh_area_ratio` del frame (`[engine]`): un umbral, como máximo el frame |
| `max_regions` | conteo | `[engine] max_regions`, el máximo de regiones de un plan regional |
| `decided_by` | texto | la comprobación que hizo del plan un refresco completo (los mismos nombres que `full_refresh_reason`); `null` si el plan es de reposo o regional |
| `decisive_region` | índice | con `area_budget`: el índice (desde 0) en `regions` de la mayor región (la primera si hay empate); `null` en otro caso |
| `chosen` | texto | qué cierre hizo el plan que se ejecutó: `spanning` (el cierre consciente de los soportes de [ADR-0045](../adr/0045-support-aware-closure.md)) o `classic` (el detector no declara banda, el plan se decidió antes de toda región, o la regla R4 cedió al plan clásico) |
| `spanning` | objeto | qué hizo el cierre consciente de los soportes, cuando se ejecutó; `null` si no (el detector no declara superficies, o el plan se decidió antes de toda región). Si se ejecutó y R4 cedió al plan clásico, `components`, `regions`, `covered_area_px`, `decided_by` y `decisive_region` describen el intento consciente de los soportes, y `spanning.classic` el plan que se ejecutó |
| `spanning.surfaces_declared` | conteo | superficies distintas que el detector declaró en la última evidencia |
| `spanning.surfaces_spanned` | conteo | de ellas, las abarcadas (sin cerrar sobre ellas) |
| `spanning.largest_spanned_px` | píxeles, una caja | la mayor superficie abarcada; `0` si ninguna |
| `spanning.whole_on_read_cut` | conteo | cajas de accesibilidad que una región final solapa sin contener (regla R1: se releen enteras) |
| `spanning.largest_whole_on_read_cut_px` | píxeles, una caja | la mayor de ellas; `0` si ninguna |
| `spanning.relations_ignored` | conteo | relaciones que el cierre ignoró porque su control es una superficie abarcada o una caja de accesibilidad (regla R3) |
| `spanning.attempt_reason` | texto | la razón por la que el intento consciente de los soportes fue un refresco completo (los nombres de `full_refresh_reason`); `null` si fue regional |
| `spanning.classic` | objeto | el plan clásico, cuando la regla R4 lo calculó y lo ejecutó; `null` en otro caso |
| `spanning.classic.work` | texto | `full` o `regions` |
| `spanning.classic.full_refresh_reason` | texto | su razón; `null` si es regional |
| `spanning.classic.region_count` | conteo | las regiones que produjo su cierre (lo que comparó el presupuesto de regiones); `0` si el cierre agotó las rondas |
| `spanning.classic.covered_area_px` | píxeles, suma | la suma de sus áreas (lo que comparó el presupuesto de área) |
| `component_support_classes` | lista | para cada entrada de `components`, en orden, la clase de su caja `largest_support_px`: `cut_prone` (una caja que una región debe contener entera), `surface` (una superficie del detector) o `whole_on_read` (una caja de accesibilidad); `null` si no toca ninguna o el cierre fue el clásico sin reparto de la evidencia |
| `region_support_classes` | lista | lo mismo para cada entrada de `regions` |

**Cuándo es `null` o está vacío.** Una observación fallida no publica nada, así que su línea sólo lleva
`schema`, `observation_id`, `outcome` y `error_code`: todo lo demás es `null`, y `components` y
`regions` son `[]` (`component_support_classes` y `region_support_classes` son `null`). Un refresco completo decidido antes de que exista una región (`no_committed_base`,
`geometry_changed`, `experimental_baseline`) tiene `components` y `regions` vacíos y `covered_area_px`
en `0`; también `closure_budget`, cuyo componente que agotó las rondas no deja entrada.
`covered_area_px` y `regions` se llenan una vez fusionadas las regiones, así que también se llenan con
`too_many_regions`. Un plan de reposo tiene las dos listas vacías.

Para saber por qué hubo un refresco completo, compare el `padded_area_px` de un componente con su
`closed_area_px` y con el frame: un `padded_area_px` pequeño con un `closed_area_px` y un
`largest_support_px` grandes significa que el cierre sobre una caja grande de evidencia hizo grande la
región; un `padded_area_px` que ya es grande significa que el cambio mismo era grande. Con el esquema
2, la clase de esa caja dice qué era: una caja `surface` o `whole_on_read` en un refresco completo
decidido por el cierre clásico es una caja que el cierre consciente de los soportes habría dejado
fuera.

### La línea de `--plan-check`

Cada línea es `ommatix-plan-check ` y un objeto JSON (claves ordenadas), una por observación. Después
de confirmar una observación **regional**, el detector, el reconocedor y el proveedor de accesibilidad
se ejecutan otra vez sobre el fotograma entero y las dos evidencias se comparan caja a caja, como
multiconjuntos. Una caja «en una caja abarcada o releída entera» solapa una superficie que el cierre
abarcó (una región la tocó sin contenerla) o una caja de accesibilidad que tocó una región: donde
[ADR-0045](../adr/0045-support-aware-closure.md) cambió lo que conserva una observación regional. Sus
conteos ahí deben ser `0`; los conteos en otra parte vienen de una percepción que no es local (el OCR
lee renglones en contexto: límite L3).

| Campo | Unidad | Significado |
|---|---|---|
| `schema` | texto | `ommatix.plan-check/1` |
| `observation_id`, `outcome`, `error_code`, `work` | | como en la línea de `--plan-trace` |
| `checked` | booleano | `true` si la comparación se hizo: la observación fue regional y la lectura completa tuvo éxito |
| `full_truncated` | booleano | el recorrido de accesibilidad de la lectura completa acabó en su presupuesto, así que sus ausencias no prueban nada; `null` si no se comparó |
| `only_regional` | objeto | cajas en la evidencia de la observación regional y no en la de la lectura completa; `null` si no se comparó |
| `only_full` | objeto | cajas en la evidencia de la lectura completa y no en la de la observación regional; `null` si no se comparó |
| `….controls`, `….words`, `….accessibility` | objeto | por tipo de evidencia |
| `….at_spanned_or_whole_on_read` | conteo | de esas cajas, las que solapan una superficie abarcada o una caja de accesibilidad que tocó una región |
| `….elsewhere` | conteo | las demás |

## Archivo de configuración

El archivo de configuración tiene la forma de
[`docs/thesis/plantillas/configuracion_mvp.toml`](../thesis/plantillas/configuracion_mvp.toml)
(`[engine]`, `[mcp]`, `[safety]`, `[models]`, `[cache]`; 64 KiB como máximo); sin él se
aplican los valores de la plantilla, que niegan la actuación y ocultan los valores. Ninguna
opción de la línea de órdenes relaja `[safety]`. El archivo se comprueba de forma estricta:

- **Se rechaza** (el servidor no arranca): secciones o claves desconocidas; valores fuera de
  rango (`tile_size` 8–1024, `halo_px` ≤ 1024, `max_regions` 1–1024,
  `full_refresh_area_ratio` en (0, 1], `adaptive_min_regions` 0–1024 (por defecto `2`; un plan con esa cantidad de regiones o más se procesa como refresco completo, razón `adaptive_cost`, [ADR-0030](../adr/0030-adaptive-full-or-regional-plan.md); `0` apaga la regla), `max_snapshot_age_ms` ≥ 1, `verify_timeout_ms` 0–5000, `verify_poll_ms` 1–1000, `concurrent_stages` booleano (por defecto `true`), `candidate_search` `scan` (por defecto, la referencia) o `rstar` (R*-tree experimental, [ADR-0027](../adr/0027-candidate-enumeration-with-an-rstar-index.md); los mismos resultados), `max_message_bytes`
  4096–65536, `max_entries` ≥ 1, entradas vacías en `allowed_applications`, un `stop_file`,
  `window_pin_file` o `purge_file` en blanco, una entrada de `allowed_window_classes` que nunca podría
  aplicarse o que nombra el escritorio, la barra de tareas o `applicationframehost.exe`); y valores que este binario no puede cumplir: un `platform` distinto de
  `windows`, un `snapshot_policy`
  distinto de `strict`, un `profile` distinto de `2026-07-28` (el perfil legacy se habilita
  con `legacy_enabled`), un `transport` distinto de `stdio`, `allow_destructive = true`,
  `allow_screenshots_to_client = true`, `[models] detector_path`, `detector_sha256`,
  `ocr_path` u `ocr_sha256` (los pesos se declaran en el manifiesto del modelo), un detector
  configurado a medias o con hashes mal formados (abajo) y `[cache] persist = true`.
- **Vigencia** (ADR-0025): `max_snapshot_age_ms` (por defecto `10000`) acota la edad, en el
  reloj del motor, de la captura del snapshot publicado y de la lectura de accesibilidad que
  confirma un objetivo al autorizar una acción. Pasada esa edad, la acción se rechaza con
  `STALE_SNAPSHOT` (observar de nuevo, sin reanclar): en la puerta, también en simulación, y
  antes de cada entrada real, incluidas las teclas de un `type` en dos fases. Una publicación
  nueva no renueva una lectura de accesibilidad antigua.
- **Se acepta sin efecto** (una nota en stderr al arrancar): `actuation_enabled = true` con un
  adaptador de entrada que no es real.

`[cache]` puede además encender el resolvedor de íconos (algoritmo 3A) con seis claves que
van juntas: `prototypes` (un archivo JSON lines, relativo al archivo de configuración, con un
`{"label", "context", "grid"}` por línea, donde `grid` es el recorte 9×8 en escala de grises
como 72 valores), `prototypes_sha256` (el SHA-256 del archivo: se comprueba antes de leerlo, así
que un prototipo añadido después de la revisión del operador detiene el servidor, SEC-06), `context` (la aplicación a la que valen los prototipos; hasta que S3
identifique la aplicación en primer plano, la nombra el operador), `gradient_tau` (1–255),
`max_distance` y `min_margin` (0–1). Ninguna tiene valor por defecto, porque la tesis los
calibra en su corpus (H4); cualquiera de ellas sin las demás se rechaza, y también el archivo
entero ante una línea inválida o más de `max_entries` prototipos. Sin ellas, la nota de
arranque dice que las etiquetas de los íconos quedan desconocidas.

`[models]` está vacío por defecto: no se carga ningún modelo (`models_loaded: false`), el
detector es el de regiones de color plano y no hay OCR. Con la fuente `images`, `screen` o
`dxgi` puede elegir:

| Clave | Significado |
|---|---|
| `detector_manifest`, `detector_manifest_sha256` | un manifiesto con la forma de [`model_manifest.json`](../thesis/plantillas/model_manifest.json), con estado `SELECCIONADO`, cuya entrada de detector nombra los pesos, su SHA-256 y los roles y affordances de cada clase; y el SHA-256 del propio manifiesto (64 caracteres hexadecimales) |
| `onnxruntime_path`, `onnxruntime_sha256` | la biblioteca de ONNX Runtime (`onnxruntime.dll`) y su SHA-256; debe estar en un directorio que sólo el operador pueda escribir |
| `detector_execution_provider` | `"cpu"` (por defecto) o `"directml"` (feature de compilación `directml`); el `execution_provider` del manifiesto debe incluirlo. DirectML nunca recurre a la CPU: si no puede crearse, o algún nodo se ejecutaría en la CPU, el detector se rechaza |
| `directml_path`, `directml_sha256` | sólo con `directml`: la `DirectML.dll` que se carga por ruta absoluta antes de ONNX Runtime (nunca la copia de System32), y su SHA-256 |
| `detector_threads` | hilos intra-op, de `1` a `64`; `0` (por defecto) deja que ONNX Runtime use el número de núcleos físicos |
| `ocr` | `"none"` (por defecto) o `"winrt"`, el OCR incluido en Windows |
| `ocr_language` | la etiqueta BCP-47 del idioma del OCR, por defecto `es-ES`; nunca se sustituye por otro |

Las cuatro claves del detector van juntas (feature de compilación `onnx`); las rutas
relativas lo son al archivo de configuración. El manifiesto, la biblioteca y los pesos se
comprueban cada uno contra su SHA-256 antes de cargar nada, y una discrepancia, una feature
ausente o un idioma de OCR no instalado detienen el motor antes de abrir ninguna captura: no
se recurre a otro modelo ni a otro idioma. Con `--source sim` las claves se ignoran, con una
nota en stderr. No hay un detector aceptado para el producto; el spike seleccionó un candidato para integración y medición (S0,
[ADR-0009](../adr/0009-models-and-licences-decided-in-the-spike.md)).



## Accesibilidad y opciones experimentales

`[models] accessibility = "uia"` añade evidencia de UI Automation fusionada con la visual;
nunca lee el valor de un campo ni reemplaza la ruta visual
([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)). La
[plantilla canónica](../thesis/plantillas/configuracion_mvp.toml) documenta el presupuesto
y el máximo de elementos. `action_freshness = "target_window"` sigue apagada por defecto
y necesita la validación de [ADR-0035](../adr/0035-freshness-group-limited-to-occluders.md)
antes de su uso en vivo. `experimental_full_frame` es exclusivamente una línea base de
investigación ([ADR-0024](../adr/0024-observation-spans-and-a-switched-full-frame-baseline.md)).

## Contrato y diagnósticos

El servidor MCP expone tres herramientas, con los schemas JSON del dossier:

| Herramienta | Hace |
|---|---|
| `observe_screen` | Observa la pantalla y devuelve una página del snapshot nuevo: `session_id`, `snapshot_id`, `geometry_epoch`, elementos con `target_id`, `revision`, rol, etiqueta y accionabilidad, nunca coordenadas; `query`, `limit` y un `cursor` atado al snapshot |
| `execute_action` | `click`, `type` o `scroll` sobre un `target_id` en una revisión esperada; dry run por defecto; devuelve `simulated`, `rejected`, `input_delivered`, `effect_observed` o `indeterminate` |
| `get_engine_metrics` | Modo, observaciones, acciones rechazadas y entregadas, regiones pendientes, si hay modelos cargados |

Los errores del dominio vuelven como resultados de herramienta con `isError: true`, un
`code`, `retryable` y un `next_step`; los valores que dependen de la ejecución viajan en el
`_meta` del resultado: los tiempos por etapa bajo `io.ommatix/timings` y si la evidencia de
accesibilidad es completa bajo `io.ommatix/accessibility` (`observe_screen`), y los tiempos
por etapa de la acción bajo `io.ommatix/action_timings` (`execute_action`).

Véase [control del operador](live-input.es.md) para la parada, purga y alcance por ventanas.
