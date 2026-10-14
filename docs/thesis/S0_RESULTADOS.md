# Spike S0: detector OmniParser `icon_detect_v3` y OCR de Windows

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable del proyecto aprobó el 14 de octubre de 2026 el spike con los pesos MIT `icon_detect_v3` de OmniParser y corridas supervisadas limitadas a `notepad.exe` con datos desechables (`CHANGELOG.md`, 1.0.7). **Alcance:** un detector y un OCR, una aplicación, una máquina. Este informe es la salida de S0 que pide el capítulo 06 §6.2 para ese detector; no cierra ningún requisito RF/RNF ni ninguna casilla DoD.

Todas las cifras son **mediciones [V]** en la máquina descrita abajo, con el `n` indicado. Ninguna es una meta ni una extrapolación. No se guardó ningún frame: la pantalla se procesó sólo en memoria (ADR-0012), y en el repositorio no hay pesos, capturas, runtime ni conjuntos de datos.

## 1. Perfil de hardware y software

Mismo formato que `plantillas/hardware_profile.json`; copia en `plantillas/hardware_profile.s0.json`.

| Campo | Valor |
|---|---|
| `os_build` | Windows 11 Pro 26H2, build 26300.9550 |
| `cpu_model` | AMD Ryzen 7 5700X (8 núcleos, 16 hilos) |
| `ram_gib` | 32 (31,9 visibles) |
| `gpu_model` | AMD Radeon RX 6700 XT (no se usa: proveedor CPU) |
| `vram_gib` | 12 (ficha del fabricante; WMI la satura en 4 GiB) |
| `driver` | AMD 32.0.21045.5002 |
| `power_plan` | Alto rendimiento |
| `resolution` | 1920×1080; dos monitores, se captura la salida 0 (ADR-0008: un monitor) |
| `dpi_percent` | 100 |
| `runtime_commit` | rama `feat/s0-omniparser-v3` sobre `eee5c85`; `rustc 1.99.0`, perfil `release` |
| `onnx_runtime` | 1.28.0 (commit `da9b5e364c465de65c49d91e696cd6485270757f`), `load-dynamic` desde `ort` 2.0.0-rc.13 |
| `execution_provider` | `CPUExecutionProvider`, 1 hilo intra-op, `GraphOptimizationLevel::Level3`, cómputo determinista |
| `mcp_profile` | 2026-07-28 (no se usó MCP: se midió con `ommatix demo`) |

## 2. Procedencia, licencia y hashes

Los artefactos viven fuera del repositorio, bajo `%LOCALAPPDATA%\Ommatix\`.

| Artefacto | Origen | Tamaño (bytes) | SHA-256 |
|---|---|---|---|
| `icon_detect_v3/model.pt` (TorchScript) | `microsoft/OmniParser-v2.0`, revisión `f55d0750e5b94db2125ef0b45b0fa4a85ddc59b4` | 281 182 680 | `11c6cbb77f22569fab22d86c76407a83ec81ab89dbfe28279854822d6e3fb00c` |
| `icon_detect_v3/LICENSE` | misma revisión | 1 073 | `646f8936b8ddcd14e13e578ff6857e368780b0d1a4f6066bee89211923a373e2` |
| `README.md` (ficha del modelo) | misma revisión | 7 820 | `6ae154d65bfc2755703440be02acf63a37b741a1101b7c42b666da93a4ed2310` |
| `icon_detect_v3.onnx` (exportado) | `tools/s0/export_omniparser_v3.py` | 232 614 657 | `bcbec780f2b3e5fcbc13c26a73a73541c84ea3482a7696540eb258a076c9d968` |
| `onnxruntime-win-x64-1.28.0.zip` | GitHub `microsoft/onnxruntime`, release v1.28.0 | 78 796 801 | `abef733dacbe2f571547a7150b479b5cb9cc0df22f96c24983a42cadb1b4f8bc` (coincide con el digest publicado en la release) |
| `onnxruntime.dll` (de ese zip) | — | 15 809 848 | `18370c375f07357fa5874344a9d9ac17e6b6fe1eb18b1dd209d79483b4470257` |
| `manifest.json` (= `plantillas/model_manifest.s0.json`) | este repositorio | 2 920 | `8e1c7ad6ffbab6ba543b34774accd67f434a4ac71342afc12f36f7031b7014f1` |

**Licencias.** `icon_detect_v3/LICENSE` es el texto MIT de Microsoft Corporation. La ficha del modelo (metadato `license: mit`) dice que `icon_detect_v3` es "an MIT licensed interactable region detector finetuned from YOLOv9-E using the MIT licensed MultimediaTechLab/YOLO implementation", y que los pesos `icon_detect` (YOLOv8) son AGPL-3.0. **Los pesos AGPL no se descargaron ni se usaron.** ONNX Runtime 1.28.0 es MIT (`LICENSE` del zip). La exportación usa torch, onnx y onnxruntime en un entorno virtual temporal fuera del repositorio; ninguno es dependencia del binario.

**Versión de ONNX Runtime.** Con las features `std` y `load-dynamic`, `ort` 2.0.0-rc.13 pide la versión 17 de la API C (`ORT_API_VERSION`), así que acepta ONNX Runtime 1.17 o posterior. Se fija 1.28.0, la versión con la que se publica esa versión de `ort`. Existe un parche 1.28.3; cambiar a él exige actualizar `onnxruntime_sha256`.

## 3. Exportación a ONNX

El TorchScript no devuelve un tensor YOLO decodificado, sino las cabezas crudas: seis tensores `(cls_logits, box_ltrb)` para los strides 8, 16 y 32 (`[1, 1, G, G]` y `[1, 4, G, G]`, con `G = 1280 / stride`). El adaptador ONNX sólo acepta `[1, 4 + C, N]` o `[1, N, 4 + C]`. Para no tocar su decodificador, `tools/s0/export_omniparser_v3.py` envuelve el módulo con la decodificación que da la ficha (sigmoide sobre los logits; `x0 = (gx + 0,5 − izquierda) · stride`, etc.) y emite `[1, 5, 33600]`: centro y tamaño en píxeles de la entrada y la puntuación ya activada (`output_layout = "yolo_cxcywh_scores_cn"`). La NMS sigue en el adaptador.

El trazado no puede entrar en un módulo TorchScript anidado, así que el envoltorio se compila con `torch.jit.script` y se exporta con el exportador TorchScript (`dynamo=False`), opset 17, IR 8, entrada `image` `[1, 3, 1280, 1280]` y salida `detections` `[1, 5, 33600]`, en un único archivo sin datos externos. Entorno: Python 3.12.2, torch 2.14.1+cpu, onnx 1.23.2, onnxruntime 1.31.0, numpy 2.5.4, onnxscript 0.7.2. El script comprueba el SHA-256 de los pesos antes de exportar y escribe sólo en `--out-dir`.

**Equivalencia numérica [V]** (ONNX Runtime 1.31 en Python frente al TorchScript decodificado como indica la ficha, 33 600 candidatos por imagen):

| Imagen sintética | Máx. dif. absoluta, cajas (px) | Máx. dif. absoluta, puntuación | Envoltorio frente a la decodificación de la ficha | Candidatos ≥ 0,05 (TorchScript / ONNX) |
|---|---|---|---|---|
| Ventana sintética dibujada por el script | 0,01495 | 8,3·10⁻⁷ | 0 (idéntico) | 77 / 77 |
| Ruido uniforme, semilla 0 | 0,00055 | 1,2·10⁻⁷ | 0 (idéntico) | 0 / 0 |

Las diferencias son del orden del redondeo de float32 en coordenadas de hasta 1280 px. Ninguna imagen es una captura.

## 4. Manifiesto y configuración

`plantillas/model_manifest.s0.json` es, byte a byte, el manifiesto que carga el motor (`status: "SELECCIONADO"`). Una prueba (`the_s0_manifest_selects_one_class_without_a_role`) comprueba que el adaptador lo acepta. `tools/s0/ommatix.s0.toml` es la configuración del operador que se copia a `%LOCALAPPDATA%\Ommatix\`. Sus rutas son relativas a ese directorio y sus hashes son los de la tabla anterior. `[safety]` conserva los valores por defecto que lo niegan todo y la entrada es dry run.

**Clase y rol.** El modelo tiene una sola clase (región interactuable). No distingue botón, campo, casilla ni lista, así que el rol es `unknown` (capítulo 06 §6.2: no inventar clases).

**Affordance: decisión y coste.** Se declara `affordance: "interactive"`, con la afirmación de la ficha como `affordance_evidence`. Sus detecciones salen `confirmed`: un agente puede pedir `click` o `scroll` sobre ellas, nunca `type`, que exige el rol `input`. Coste de esta decisión:

- Es una afirmación del proveedor, entrenada sobre páginas web anotadas automáticamente. **Su precisión en escritorio Windows no se midió** porque no hay corpus etiquetado (capítulo 09 §9.3). En la corrida de notepad, 142 de 165 elementos salen `confirmed`, entre ellos los menús Archivo, Editar y Ver. Nadie ha comprobado cuántos de los demás son de verdad interactuables.
- El umbral es el de la ficha (0,05) y la NMS usa 0,45. La ficha advierte que subirlo por encima de 0,1 pierde elementos reales, así que S0 no lo cambia sin datos.
- La alternativa, `unknown`, falla de forma segura pero deja al motor sin ningún objetivo accionable. La decisión es reversible editando el manifiesto, y su hash fijado obliga a que la edición sea deliberada. La entrada real sigue exigiendo la política del operador (ADR-0007).

## 5. Corridas supervisadas

Procedimiento: `Start-Process notepad` sobre un archivo de texto desechable creado para el spike, maximizado en la salida 0 y en primer plano (comprobado con `GetForegroundWindow`). No se envió ninguna tecla ni ningún clic. Después:

```text
ommatix demo --source dxgi --config %LOCALAPPDATA%\Ommatix\ommatix.s0.toml --session-id s0 --observations 20 --timings
```

`--timings` (nuevo en esta etapa) escribe en stderr los tiempos por etapa de cada observación: `capture`, `plan`, `detect`, `recognize`, `fuse`, `track` y `total`. Salen de `ObservePage::timings`, medidos con el reloj monotónico del motor. Además, la salida estándar del demo incluye ahora `label_source` y una línea `totals` con el recuento de roles, accionabilidad y orígenes de la etiqueta en todas las páginas del snapshot.

**Exclusiones.** Hubo dos tandas de 10 procesos con 20 observaciones cada uno (la segunda, tras la corrección de §6). Durante el proceso 8 de la segunda tanda, otra aplicación del operador pasó a primer plano. Por eso los procesos 8 a 10 de **ambas** tandas se excluyen: su contenido quedaba fuera de la autorización. No se registró nada de ellos, salvo que existieron, y la captura se detuvo. Quedan 14 procesos válidos.

### 5.1 Elementos (corrida con la corrección de §6, procesos 1–7)

| Observación | Elementos | Rol | Accionabilidad | Origen de la etiqueta |
|---|---|---|---|---|
| 1 (primer frame, en blanco: ver §7) | 1 | unknown 1 | confirmed 1 | unknown 1 |
| 2 (refresco completo con notepad) | 165 | unknown 142, text_node 23 | confirmed 142, unknown 23 | ocr 55, unknown 110 |
| 3–20 | 165 en 80 observaciones; 159 (35), 144 (17) o 142 (1) tras cambios pequeños en pantalla | — | — | — |

Las palabras OCR que no caen dentro de una detección se publican como `text_node` sin accionabilidad confirmada; las líneas de texto del documento no se detectan como regiones interactuables. Las etiquetas OCR (es-ES) del texto desechable salen razonables pero con errores: `¡Hola!` pierde el `¡`, `pingüino` se lee `pingulno` y un `0` se lee `8`. `Windows.Media.Ocr` no informa confianza.

### 5.2 Latencias medidas (CPU, 1 hilo intra-op)

Mediana y p95 por rango más cercano, en milisegundos. Observaciones de los procesos válidos; dentro de un proceso, las observaciones no son independientes (capítulo 09 §9.6).

| Caso | n | capture | plan | detect | recognize | fuse | track | total (mediana / p95) |
|---|---|---|---|---|---|---|---|---|
| Notepad, obs. 1: refresco completo (frame en blanco) | 14 | 7,6 | 0,03 | 7 566 | 24,0 | 0,03 | 0,02 | 7 599 / 8 550 |
| Notepad, obs. 2: refresco completo con contenido | 14 | 4,3 | 0,20 | 7 788 | 40,3 | 0,14 | 0,14 | 7 836 / 8 661 |
| Notepad, obs. ≥ 3 sin regiones planificadas | 242 | 10,7 | 2,04 | 0 | 0 | 0,10 | 2,88 | 15,6 / 31,2 |
| Notepad, obs. ≥ 3 con regiones (cambios ajenos) | 10 | 3,5 | 1,89 | 7 546 | 4,6 | 0,11 | 3,21 | 7 567 / 15 385 |
| Frames sintéticos (`--source images`), una región pequeña | 16 | 6,1 | 2,12 | 7 736 | 1,6 | 0,01 | 0,01 | 7 747 / 10 178 |
| Frames sintéticos, dos regiones pequeñas | 3 | 5,6 | 2,22 | 15 833 | 1,5 | 0,01 | 0,01 | 15 845 / 15 959 |

Lectura:

- **La inferencia domina.** Una pasada del detector cuesta unos 7,5 s en CPU con un hilo. Comparado con las metas iniciales de RNF-05 (refresco completo en caliente p95 ≤ 700 ms, incremental p95 ≤ 150 ms), el resultado está un orden de magnitud por encima. Esas metas siguen siendo hipótesis (ADR-0010) y este resultado no las cierra ni las descarta para otro proveedor o modelo.
- **Sin regiones planificadas no hay inferencia.** Con la pantalla quieta, la observación cuesta unos 16 ms de mediana; casi todo es la espera de `AcquireNextFrame`.
- **Regiones frente a pantalla completa.** Con este modelo, una región pequeña cuesta lo mismo que la pantalla completa, porque toda región se rellena hasta la entrada fija de 1280×1280. Cada región planificada añade una inferencia completa (dos regiones, unos 15,8 s). La incrementalidad ahorra cuando no hay nada que analizar, pero no abarata el análisis de una región. Hacen falta otra estrategia (agrupar regiones en una sola entrada, una entrada más pequeña o un modelo más ligero) o más cómputo.
- **Hilos [V, fuera del motor].** Con ONNX Runtime 1.31 en Python sobre una entrada sintética, el mismo ONNX tarda 7 339 ms de mediana con 1 hilo intra-op y 2 601 ms con 8 (n = 10 cada uno). El adaptador fija 1 hilo y cómputo determinista. Cambiarlo es una decisión de rendimiento que debe medirse en el motor.
- OCR: unos 40 ms para la pantalla completa de notepad y de 2 a 5 ms para regiones pequeñas. Fusión y tracking quedan por debajo de 6 ms en todos los casos.

## 6. Defecto encontrado y corregido: regiones pequeñas ampliadas

El letterbox escalaba cada recorte por el mayor factor que cabía en la entrada. Una región sucia de unos 100 px se ampliaba más de 10× hasta 1280. Sobre 20 frames sintéticos que sólo difieren en una caja de 24×16 px, los elementos crecieron de 8 a 144, todos `confirmed`: detecciones espurias dentro de cada región ampliada. Con la affordance declarada en §4, esas detecciones habrían sido objetivos accionables.

Corrección (`ommatix-adapter-vision`, `model/letterbox.rs`): la escala nunca supera 1. Un recorte menor que la entrada se rellena a su resolución nativa, como la ficha hace con la pantalla completa, que sólo se reduce. La transformación inversa del SDD §2.8 no cambia. Prueba de regresión: `a_small_crop_is_not_magnified`. Tras la corrección, los mismos 20 frames dan 8 elementos en cada observación. El coste no cambia, porque la entrada sigue siendo 1280×1280.

## 7. Límites y defectos abiertos

- **Primer frame DXGI en blanco (IT-01).** En 15 de los 16 procesos dentro del alcance cuyo recuento se registró, la primera observación produjo 1 elemento y un OCR de unos 24 ms, lo que es coherente con un frame vacío. La segunda observación ve la pantalla real. El adaptador copia el primer frame aunque `LastPresentTime` sea 0 cuando todavía no tiene una imagen previa. Queda para S3 con su prueba. Hasta entonces, la primera observación de una sesión `dxgi` no es fiable.
- **Una sola clase.** Todos los roles son `unknown` y ningún elemento admite `type`. Hace falta otra fuente de evidencia aprobada para distinguir roles (capítulo 06 §6.2).
- **Precisión sin medir.** No hay corpus etiquetado. La cifra de 142 elementos `confirmed` no es precisión ni cobertura.
- **OCR.** `Windows.Media.Ocr` sólo tiene es-ES y es-MX instalados, no informa confianza y pierde signos como `¡`.
- **Privacidad de la corrida.** El Bloc de notas de Windows 11 restaura las pestañas de sesiones anteriores. Sus títulos estaban en pantalla y pasaron por el OCR en memoria, aunque no se guardaron ni se registran aquí. Antes de la próxima corrida conviene usar una sesión limpia del Bloc de notas.
- **Una aplicación, un DPI (100 %), un tema y CPU.** Quedan sin probar DirectML, otros DPI (IT-07), RSS y VRAM (RNF-06), y un cliente MCP real.

## 8. Decisión de S0 para este detector

`icon_detect_v3` (MIT) queda **seleccionado para el spike** como el detector con el que se integra y mide el motor: manifiesto `SELECCIONADO`, roles `unknown`, affordance de la ficha y CPU. No queda aceptado como detector del producto: su latencia en CPU y su precisión sin medir lo impiden. ADR-0009 no se edita; esta decisión se registra en `CHANGELOG.md` (1.0.8).
