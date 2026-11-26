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



## Archivo de configuración

El archivo de configuración tiene la forma de
[`docs/thesis/plantillas/configuracion_mvp.toml`](../thesis/plantillas/configuracion_mvp.toml)
(`[engine]`, `[mcp]`, `[safety]`, `[models]`, `[cache]`; 64 KiB como máximo); sin él se
aplican los valores de la plantilla, que niegan la actuación y ocultan los valores. Ninguna
opción de la línea de órdenes relaja `[safety]`. El archivo se comprueba de forma estricta:

- **Se rechaza** (el servidor no arranca): secciones o claves desconocidas; valores fuera de
  rango (`tile_size` 8–1024, `halo_px` ≤ 1024, `max_regions` 1–1024,
  `full_refresh_area_ratio` en (0, 1], `adaptive_min_regions` 0–1024 (por defecto `2`; un plan con esa cantidad de regiones o más se procesa como refresco completo, razón `adaptive_cost`, [ADR-0030](../adr/0030-adaptive-full-or-regional-plan.md); `0` apaga la regla), `max_snapshot_age_ms` ≥ 1, `verify_timeout_ms` 0–5000, `verify_poll_ms` 1–1000, `concurrent_stages` booleano (por defecto `true`), `candidate_search` `scan` (por defecto, la referencia) o `rstar` (R*-tree experimental, [ADR-0027](../adr/0027-candidate-enumeration-with-an-rstar-index.md); los mismos resultados), `max_message_bytes`
  4096–65536, `max_entries` ≥ 1, `type_key_pause_ms` 0–50, `type_boundary_pause_ms` 0–250, entradas vacías en `allowed_applications`, un `stop_file`,
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
- **Opcional, apagado por defecto** (`[safety] type_key_pause_ms`, `type_boundary_pause_ms`): con
  ambos en `0` el actuador `SendInput` en vivo entrega al sistema el texto entero de un `type` en
  una sola llamada. Con cualquiera mayor que cero envía una llamada por carácter (un par
  sustituto va junto), espera la primera pausa tras cada carácter al que sigue otro y también la
  segunda tras un espacio o un signo de puntuación al que sigue más texto, y antes de cada
  carácter después del primero vuelve a comprobar la parada del operador, la ventana en primer
  plano, la aplicación en primer plano con el foco de teclado y las teclas modificadoras
  pulsadas. Una comprobación que falla termina el texto ahí y la acción queda `indeterminate`
  (entrada entregada, snapshot gastado), sin reintento. Un `type` con pausas cuya suma pase de
  30 s se **rechaza** antes de enviar nada, como `LIMIT_EXCEEDED` con `next_step:
  reduce_request` (enviar menos texto por llamada), también en simulación, y no se gasta el
  snapshot. Estas claves cambian cómo llegan las teclas a la aplicación, no si pueden llegar;
  ninguna opción de la línea de órdenes ni el agente puede fijarlas. El efecto en una
  aplicación real aún no está medido: déjelas en `0` salvo que lo esté midiendo.
  **Lo que cuesta un `type` espaciado.** El motor atiende una petición a la vez, así que queda
  retenido, y cualquier otra llamada de la sesión espera, durante toda la entrega. Sólo las
  pausas suman como mucho 30 s (la cota de arriba: 2048 caracteres, el `type` más largo, a 14 ms
  cada uno son 28,7 s; 600 caracteres a 50 ms son 30 s). A eso se suman los envíos y las
  comprobaciones de cada carácter (no medidos; decenas a cientos de microsegundos cada uno,
  como mucho un segundo para 2048 caracteres) y el exceso del temporizador: cada pausa es una
  sola espera (la pausa de límite de palabra se suma a la de tecla, no se espera aparte), y una
  espera puede durar hasta un tic del temporizador más de lo pedido, unos 15,6 ms donde el
  temporizador del sistema es grueso. El peor caso, 2048 caracteres, es por tanto de unos 30 s
  + 1 s + 2047 × 15,6 ms ≈ 63 s; una frase de 29 caracteres con 10 ms y 50 ms tarda 0,53 s más
  0,44 s como mucho. La parada del operador se lee antes de cada carácter, así que surte efecto
  en una pausa (300 ms como mucho) más el exceso. Entre una comprobación y el `SendInput` que la
  sigue hay una ventana de decenas a cientos de microsegundos (las propias comprobaciones y la
  reserva de los eventos; no medida). No puede ser cero: Windows no tiene una llamada que lea
  la ventana de primer plano y envíe entrada de forma atómica, así que el primer plano puede
  cambiar entre cualquier comprobación y el envío siguiente, como antes de la primera tecla
  (capítulo 08 §8.3).
  **Un perfil medido: un portátil, una aplicación, no una validación.** En una medición
  exploratoria ejecutada por un agente, en un portátil, con una compilación de una aplicación
  (el Bloc de notas 11.2609, cuyo cuadro de texto es WinUI), un `type` enviado de golpe (ambas
  claves en `0`) se corrompía después de un límite de palabra. `type_boundary_pause_ms = 100`
  solo estuvo limpio con textos de unos 30 caracteres o menos (12 de 12 corridas).
  `type_key_pause_ms = 20` con `type_boundary_pause_ms = 100` estuvo limpio con todos los textos
  probados, incluido uno de 200 caracteres (12 de 12 y 3 de 3 corridas), a unos 41–43 ms por
  carácter frente a unos 4. Pausas de 1–10 ms entre caracteres o de 5–20 ms tras un límite no
  lo curaron. La aplicación de pruebas del proyecto no necesitó ninguna. Estos números no son
  una validación del dossier y no dicen nada de otra aplicación o compilación; los valores por
  defecto siguen en `0` y el operador elige los valores para las aplicaciones de un despliegue.
  **Cuando una comprobación detiene un `type` espaciado**, la acción queda `indeterminate`
  (se enviaron algunas teclas) y su mensaje nombra la comprobación (la parada del operador; la
  ventana de primer plano; el foco de teclado o la aplicación en primer plano; una tecla
  modificadora o un botón del ratón pulsados) y dice cuántos caracteres del texto se enviaron
  de cuántos: un recuento, nunca el texto.
  **Lo que un `type` espaciado no vuelve a comprobar entre caracteres.** La ventana de primer
  plano se compara por identificador con la que el motor autorizó justo antes de las teclas; la
  misma ventana cumple la lista de aplicaciones, el alcance por clase de ventana y la ventana
  registrada como las cumplía entonces, así que quedan implícitas, tal como estaban al empezar
  el texto. No se repiten: el foco de UI Automation en el elemento objetivo (sólo se comprueban
  la aplicación en primer plano y la ventana con el foco de teclado, así que no se detecta que
  el foco pase a otro control de la misma ventana); el archivo de la ventana registrada (se lee
  una vez: cambiarlo durante un texto no detiene ese texto, use la parada); la ventana bajo el
  punto objetivo; y la edad del snapshot (`max_snapshot_age_ms` se comprueba antes de la primera
  tecla y un `type` espaciado de hasta 30 s puede superarla).
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
