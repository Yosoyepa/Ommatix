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
| `gpu_model` | AMD Radeon RX 6700 XT (sin uso en §§3–7, que miden la CPU; con DirectML en §9) |
| `vram_gib` | 12 (ficha del fabricante; WMI la satura en 4 GiB) |
| `driver` | AMD 32.0.21045.5002 |
| `power_plan` | Alto rendimiento |
| `resolution` | 1920×1080; dos monitores, se captura la salida 0 (ADR-0008: un monitor) |
| `dpi_percent` | 100 |
| `runtime_commit` | rama `feat/s0-omniparser-v3` sobre `eee5c85`; `rustc 1.99.0`, perfil `release` |
| `onnx_runtime` | 1.28.0 (commit `da9b5e364c465de65c49d91e696cd6485270757f`), `load-dynamic` desde `ort` 2.0.0-rc.13 |
| `execution_provider` | §§3–7: `CPUExecutionProvider`, 1 hilo intra-op, `GraphOptimizationLevel::Level3`, cómputo determinista. §9: además CPU con 4, 8 y 16 hilos, y `DmlExecutionProvider` (ONNX Runtime 1.24.4 con DirectML 1.15.4) |
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
| `manifest.json` (= `plantillas/model_manifest.s0.json`), §§3–7 | este repositorio | 2 920 | `8e1c7ad6ffbab6ba543b34774accd67f434a4ac71342afc12f36f7031b7014f1` |
| `manifest.json` desde §9 (`execution_provider: "cpu,directml"`) | este repositorio | 2 929 | `63d9df24109308d1e2b354648139fee0c1d4e2563f8cb2a83497d4e26b7d728e` |
| `icon_detect_v3_fp16.onnx` (exportado, RNF-05, §10) | `tools/s0/export_omniparser_v3.py --fp16` | 116 609 212 | `d7a8901db0ae650616ee43f53e6ee8f57a829de54ab8daae431be77556cc0805` (el mismo hash en dos exportaciones) |
| `manifest.rnf05.json` (= `plantillas/model_manifest.rnf05.json`), §10 (`execution_provider: "directml"`) | este repositorio | 3 122 | `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47` |
| `microsoft.ml.onnxruntime.directml.1.24.4.nupkg` | NuGet `Microsoft.ML.OnnxRuntime.DirectML` 1.24.4 (commit `2d924974ef147392ced8409d36bd6d2e7fcc8a74`) | 12 458 649 | `57e9f11b73437bef7a309496135d4c1f96b1a8e9ddba60013fa27bfc1d788681` (su SHA-512 coincide con el `packageHash` del catálogo de NuGet) |
| `onnxruntime.dll` (de ese paquete, `runtimes/win-x64/native/`) | — | 17 328 152 | `e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40` (firma Authenticode válida de Microsoft Corporation) |
| `microsoft.ai.directml.1.15.4.nupkg` | NuGet `Microsoft.AI.DirectML` 1.15.4 (la dependencia que declara el paquete anterior) | 202 292 617 | `4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9` (su SHA-512 coincide con el catálogo) |
| `DirectML.dll` (de ese paquete, `bin/x64-win/`, versión 1.15.4) | — | 18 527 776 | `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1` (firma Authenticode válida de Microsoft Corporation) |

**Licencias.** `icon_detect_v3/LICENSE` es el texto MIT de Microsoft Corporation. La ficha del modelo (metadato `license: mit`) dice que `icon_detect_v3` es "an MIT licensed interactable region detector finetuned from YOLOv9-E using the MIT licensed MultimediaTechLab/YOLO implementation", y que los pesos `icon_detect` (YOLOv8) son AGPL-3.0. **Los pesos AGPL no se descargaron ni se usaron.** ONNX Runtime 1.28.0 y 1.24.4 son MIT (`LICENSE` del zip y del paquete). `DirectML.dll` se distribuye con los términos de licencia de software de Microsoft para DirectML (`LICENSE.txt` del paquete): no es MIT, y el repositorio no la incluye ni la redistribuye. La exportación usa torch, onnx y onnxruntime en un entorno virtual temporal fuera del repositorio; ninguno es dependencia del binario.

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
- **Hilos [V, fuera del motor].** Con ONNX Runtime 1.31 en Python sobre una entrada sintética, el mismo ONNX tarda 7 339 ms de mediana con 1 hilo intra-op y 2 601 ms con 8 (n = 10 cada uno). El adaptador fija 1 hilo y cómputo determinista. Cambiarlo es una decisión de rendimiento que debe medirse en el motor (se mide en §9).
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
- **Una aplicación, un DPI (100 %) y un tema.** Otros DPI se probaron después, el 15 de octubre de 2026 (IT-07, [S4_SESIONES_PENDIENTES.md §C.1](S4_SESIONES_PENDIENTES.md)); quedan sin probar RSS y VRAM (RNF-06), y un cliente MCP real. DirectML se mide en §9.

## 8. Decisión de S0 para este detector

`icon_detect_v3` (MIT) queda **seleccionado para el spike** como el detector con el que se integra y mide el motor: manifiesto `SELECCIONADO`, roles `unknown`, affordance de la ficha y CPU. No queda aceptado como detector del producto: su latencia en CPU y su precisión sin medir lo impiden. ADR-0009 no se edita; esta decisión se registra en `CHANGELOG.md` (1.0.8).

Adenda (1.0.11, §9): el manifiesto declara ahora `execution_provider: "cpu,directml"`, porque DirectML dio las mismas detecciones que la CPU en las imágenes sintéticas de §9.2. Con DirectML la latencia deja de impedir la aceptación en esta máquina; la precisión sin medir y la única clase siguen impidiéndola.

## 9. Proveedor de ejecución, hilos y agrupación de regiones (1.0.11)

**Fecha:** 14 de octubre de 2026, misma máquina (§1), rama `perf/s0-detector-ep` sobre `feat/s3-supervised-runs`. Dentro del spike que autorizó el responsable del proyecto, que incluye elegir el proveedor de ejecución. No se envió ninguna tecla ni ningún clic. Ningún frame ni ningún texto de pantalla se guardó: de las corridas con DXGI sólo se conservaron los tiempos de stderr y el número de elementos de cada observación.

### 9.1 Qué cambia en el adaptador

- **Hilos intra-op configurables:** `[models] detector_threads`, de 1 a 64. Con `0`, el valor por defecto, ONNX Runtime usa el número de núcleos físicos (8 en esta máquina; §9.3 lo confirma). El cómputo sigue siendo determinista.
- **Proveedor configurable:** `[models] detector_execution_provider = "cpu" | "directml"`. DirectML necesita la feature de compilación `directml` y una `DirectML.dll` fijada por SHA-256 (`directml_path`, `directml_sha256`). El adaptador la carga por ruta absoluta antes que ONNX Runtime, para que no se use la copia de `System32` (versión 1.15.5 en esta máquina). La sesión se crea con ejecución secuencial, sin patrón de memoria (las dos restricciones de DirectML que cita el SDD §2.4), con `error_on_failure` y con `session.disable_cpu_ep_fallback = 1`. Si DirectML no puede crearse, o si algún nodo fuera a ejecutarse en la CPU, el detector se rechaza y el motor no arranca. No hay vuelta silenciosa a la CPU. Con este modelo la sesión se crea con esa opción, así que **todos los nodos corren en DirectML**.
- **El manifiesto declara los proveedores validados:** `execution_provider` admite ahora `"cpu"`, `"directml"` o `"cpu,directml"`, y el proveedor configurado tiene que estar en esa lista. `plantillas/model_manifest.s0.json` pasa a `"cpu,directml"` por la equivalencia de §9.2. Su nuevo SHA-256 está en §2 y en `tools/s0/ommatix.s0.toml`.
- **Runtime con DirectML.** Microsoft no publica DirectML para ONNX Runtime 1.28: la release v1.28.0 sólo trae compilaciones CPU, CUDA y ARM, y el último paquete NuGet `Microsoft.ML.OnnxRuntime.DirectML` es el 1.24.4. Se usa ese paquete oficial con `Microsoft.AI.DirectML` 1.15.4, la versión de la que depende (procedencia y hashes en §2). `ort` 2.0.0-rc.13 acepta ONNX Runtime 1.17 o posterior. La configuración está en `tools/s0/ommatix.s0.directml.toml`, y la CPU sigue en 1.28.0 (`tools/s0/ommatix.s0.toml`).

Pruebas nuevas: `the_execution_provider_fails_closed` (proveedor no validado por el manifiesto, DirectML sin DLL fijada, DLL con otro hash, DLL fijada con proveedor CPU, demasiados hilos: todo se rechaza antes de cargar ONNX Runtime), `the_manifest_lists_its_validated_execution_providers` y los casos nuevos de `incomplete_or_unsupported_manifests_are_refused` y de la configuración.

### 9.2 Equivalencia de detecciones entre proveedores [V]

Herramientas: `tools/s0/synthetic_frames.py` dibuja los frames sintéticos, y el ejemplo `s0_probe` (`ommatix-adapter-vision`) carga el detector con todas las comprobaciones del motor, hace una inferencia sobre el frame completo de 1920×1080 y emite las cajas tras la NMS (coordenadas de la entrada de 1280) con su puntuación. Las cajas se emparejan por IoU.

| Comparación | Frames | Candidatos | Máx. dif. de caja (px de la entrada) | Máx. dif. de puntuación | IoU mínimo |
|---|---|---|---|---|---|
| DirectML (ORT 1.24.4) frente a CPU 1 hilo (ORT 1.28.0) | ventana, ventana con 3 cambios, ruido | 24 / 24, 24 / 24, 4 / 4 | 0,0002 | 8·10⁻⁷ | 1,0000 |
| DirectML frente a CPU 8 hilos (ORT 1.24.4) | los mismos | iguales | 0,0002 | 8·10⁻⁷ | 1,0000 |
| CPU 1 hilo frente a 8 hilos (ORT 1.28.0) | los mismos | iguales | 0 (con 4 decimales) | 0 (con 7 decimales) | 1,0000 |
| CPU ORT 1.28.0 frente a CPU ORT 1.24.4 (8 hilos) | los mismos | iguales | 0 (con 4 decimales) | 0 (con 7 decimales) | 1,0000 |

Las diferencias de DirectML son del orden del redondeo de float32. Ningún candidato aparece ni desaparece. Más hilos no cambian el resultado.

### 9.3 Una inferencia, fuera del motor [V]

`s0_probe`, frame completo (letterbox incluido), mediana / p95 en ms. n = 10 por frame en CPU y 20 en DirectML, todas en el mismo proceso tras cargar el detector.

| Proveedor | Hilos | Ventana | Ventana con 3 cambios | Ruido |
|---|---|---|---|---|
| CPU, ORT 1.28.0 | 1 | 7 590 / 8 489 | 7 559 / 7 706 | 7 503 / 7 836 |
| CPU, ORT 1.28.0 | 4 | 3 592 / 3 742 | — | — |
| CPU, ORT 1.28.0 | 8 | 2 605 / 2 748 | 2 589 / 2 651 | 2 590 / 2 699 |
| CPU, ORT 1.28.0 | 0 (por defecto de ORT) | 2 650 / 2 857 | — | — |
| CPU, ORT 1.28.0 | 16 (con SMT) | 3 614 / 4 212 | — | — |
| CPU, ORT 1.24.4 | 8 | 2 795 / 2 900 | 2 777 / 2 846 | 2 745 / 2 810 |
| DirectML, ORT 1.24.4 | — | 117,8 / 126,6 | 119,3 / 126,0 | 119,4 / 121,7 |

Lectura: 8 hilos son unas 2,9 veces más rápidos que 1, y el valor por defecto de ORT se comporta como 8. Con los 16 hilos lógicos es más lento que con 8. DirectML es unas 64 veces más rápido que un hilo y unas 22 veces más rápido que 8. Cargar el detector cuesta de 0,7 a 0,8 s en CPU y de 1,2 a 2,4 s con DirectML. La primera inferencia de una sesión DirectML cuesta de 264 a 291 ms (calentamiento).

### 9.4 En el motor: `ommatix demo --timings` [V]

Mediana / p95 en ms del total de la observación, con la etapa `detect` entre paréntesis.

**Notepad en primer plano, maximizado, DXGI salida 0, sin entrada.** `notepad` se lanzó con `Start-Process` y se comprobó con `GetForegroundWindow` antes y después de cada proceso: siempre fue Notepad maximizado, así que no hubo exclusiones. Notepad restauró las pestañas del responsable; su contenido pasó sólo en memoria por el detector y el OCR, y no se registró ni se guardó nada. Hubo 10 procesos por configuración. La observación 1 es el primer frame en blanco ya conocido (§7), así que el refresco completo útil es la observación 2.

| Configuración | Obs. 1 (en blanco), n = 10 | Obs. 2, refresco completo, n = 10 | Obs. ≥ 3 sin regiones | Elementos (obs. 2) |
|---|---|---|---|---|
| CPU, 1 hilo (antes) | 7 569 / 8 019 (7 538) | 7 654 / 7 857 (7 622) | 24,9 / 7 639, n = 10 (una observación tuvo una región) | 94–107 |
| CPU, 8 hilos | 2 674 / 2 807 (2 638) | 2 686 / 2 873 (2 637) | 15,5 / 25,5, n = 20 | 94–105 |
| DirectML | 295 / 359 (264) | **152 / 202** (122) | 15,1 / 25,5, n = 40 | 94 |

El número de elementos varía entre procesos con CPU, pero no con DirectML. La causa no se investigó: la pantalla no se guarda, así que no puede revisarse qué cambió (un cursor, un reloj o un aviso). En la observación 2 con DirectML, OCR cuesta 26,8 ms de mediana.

**Frames sintéticos (`--source images`, `tools/s0/synthetic_frames.py`).** Observaciones 2 en adelante, dentro de un proceso:

| Caso | CPU, 1 hilo | CPU, 8 hilos | DirectML |
|---|---|---|---|
| Refresco completo en caliente (`seqfull`) | 7 410 / 7 666 (7 364), n = 10 | 2 664 / 2 747 (2 610), n = 11 | **170 / 179** (123), n = 19 |
| Incremental, 1 región (`seq1`) | 7 245 / 7 532 (7 235), n = 10 | 2 579 / 2 772 (2 570), n = 19 | **118 / 123** (110), n = 19 |
| Incremental, 2 regiones (`seq2`) | — | — | 231 / 243 (221), n = 19 |
| Incremental, 3 regiones (`seq3`) | — | 7 965 / 8 601 (7 954), n = 10 | 340 / 348 (329), n = 19 |

Cada región planificada sigue costando una inferencia completa de 1280×1280 (de 110 a 123 ms con DirectML). El costo del incremental crece de forma lineal con el número de regiones.

### 9.5 Agrupar regiones en una sola inferencia: medido y descartado

Se implementó y se midió la idea de §5.2: colocar los recortes de todas las regiones planificadas lado a lado, a escala 1, en una sola entrada de 1280×1280, separados por 32 px de relleno, y llevar cada detección a la región cuyo recorte contiene su centro. La geometría era exacta y tenía pruebas unitarias. Con tres regiones de 128×88 a 192×128 px sobre la ventana sintética, una inferencia agrupada costó 109 a 110 ms con DirectML (n = 10), frente a 329 ms con una inferencia por región, y unos 2,6 a 3,2 s con 8 hilos de CPU (n = 3), frente a 7,8 a 8,1 s.

**Las detecciones cambiaron.** Con el bloque en la esquina de la entrada, aparecieron 6 cajas sobre bloques de texto que ni el refresco completo ni la inferencia por región detectan, de 3 a 8 controles en el frame con cambios. Todas habrían salido `confirmed` (§4). Con el bloque centrado, se perdió el cambio de la barra de estado que sí detectan el refresco completo y la inferencia por región. El modelo depende del contexto que rodea a cada recorte, así que agrupar no es una transformación neutra. **No se adopta:** el adaptador sigue haciendo una inferencia por región. Una alternativa futura tendría que validarse contra el refresco completo con datos etiquetados.

### 9.6 RNF-05

Metas iniciales de RNF-05: refresco completo en caliente p95 ≤ 700 ms e incremental p95 ≤ 150 ms. Siguen siendo hipótesis (ADR-0010) y este resultado no cierra el requisito: hay una máquina, un modelo, una aplicación, pocas observaciones que no son independientes dentro de un proceso (capítulo 09 §9.6) y ninguna medición de RSS ni de VRAM (RNF-06).

- **DirectML: cumple en esta medición el refresco completo** (p95 202 ms en Notepad, n = 10; 179 ms en sintéticos, n = 19) **y el incremental con una región** (p95 123 ms, n = 19). **No cumple el incremental con dos o más regiones** (p95 243 ms con dos y 348 ms con tres), porque cada región cuesta una inferencia completa. Tampoco lo cumple la primera observación de una sesión, que paga el calentamiento.
- **CPU: no cumple ninguna meta** con ningún número de hilos. Con 8 hilos, el refresco completo tiene p95 2,7 a 2,9 s y una sola región p95 2,8 s.
- Con la pantalla quieta, sin regiones planificadas, la observación cuesta p95 25,5 ms con cualquier proveedor.
- §10 repite la medición con etapas concurrentes y con la red en float16: refresco completo en caliente p95 98 ms, incremental con una región p95 102 ms y con dos p95 153 ms.


## 10. RNF-05: etapas concurrentes, red en float16 y entradas más pequeñas (1.0.22)

> **Nota (1.0.54, ADR-0024).** El `total` de estas medidas termina al acabar el *tracking*, antes del *commit*. Desde 1.0.54 `total` termina en la publicación del snapshot, como define el capítulo 03 §3.4; los valores de aquí no son comparables con los nuevos y conservan su commit de origen (§11.5).

**Fecha:** 14 de octubre de 2026, misma máquina (§1), rama `perf/rnf05-latency` sobre `adfb1ea`. Dentro del spike y de la aprobación del responsable del proyecto del 14 de octubre de 2026 («rendimiento de RNF-05 primero»). No se envió ninguna tecla ni ningún clic, y no se guardó ningún frame: de las corridas con DXGI sólo se conservaron los tiempos de stderr y el número de elementos de cada observación.

Punto de partida: una observación incremental con DirectML costaba la suma de sus etapas (§9.4 y `S4_ACEPTACION_REDUCIDA.md`): detección unos 115 ms por región (una inferencia de 1280×1280), lectura de accesibilidad unos 60 ms y OCR de 3 a 8 ms. Con una región, 184 ms; con dos, 298 ms.

### 10.1 Qué cambia

- **Etapas concurrentes (ADR-0019).** La detección, el OCR y la lectura de accesibilidad de una observación leen el mismo frame y no dependen entre sí. El motor las ejecuta en hilos acotados (`std::thread::scope`) y las une antes de fusionar: el coordinador sigue siendo el único escritor, y los resultados se recogen por etapa, no por orden de llegada. `[engine] concurrent_stages = false` las ejecuta una tras otra. Los tiempos por etapa son ahora la duración propia de cada una; las tres se solapan y no suman el `total` (capítulo 09 §9.6), que se mide directamente.
- **Red en float16, decodificación en float32.** `tools/s0/export_omniparser_v3.py --fp16` parte del mismo grafo exportado de §3, convierte la red (de la imagen a las seis cabezas) a float16 con entrada y salida float32 y deja en float32 la decodificación (sigmoide y aritmética de rejilla en píxeles de la entrada: en float16 la resolución es de un píxel por encima de 1024). Mismos pesos, mismas clases, mismo umbral (0,05) y misma NMS (0,45). Es otro artefacto con otro hash (§2) y otro manifiesto (`plantillas/model_manifest.rnf05.json`), que lista sólo `directml`: el proveedor de CPU de ONNX Runtime no tiene convolución en float16. La exportación es reproducible (el mismo SHA-256 en dos corridas). El manifiesto de §4 y la CPU siguen como estaban.
- **Calentamiento al cargar.** Con DirectML, el detector hace una inferencia con una entrada vacía al cargarse (la primera inferencia de una sesión cuesta de 2 a 3 veces una en caliente, §9.3), así que la primera observación ya no la paga.
- **Herramientas.** `tools/s4/testapp` acepta las banderas ocultas `--animate <ms>` (con `--animate-labels 1|2`) y `--animate-flash`: una o dos etiquetas `Tic nnnn` lejanas entre sí cuyo número avanza solo, o un velo sobre toda la ventana, de modo que aparecen regiones incrementales (una, dos) o un refresco completo sin que nadie toque nada. En ese modo pide el foco una vez, al arrancar, y el sistema sólo se lo da si el usuario lleva un rato sin tocar el equipo. `tools/perf/measure_latency.py` ejecuta `demo --source dxgi --timings` contra esa aplicación y alterna las variantes proceso a proceso. `s0_shapes` (ejemplo de `ommatix-adapter-vision`) compara detecciones, región contra región o modelo contra modelo, a IoU 0,5.

### 10.2 Entradas más pequeñas: medido y descartado

El coste de una inferencia sigue al área de la entrada, y un recorte no se amplía nunca (§6): una región de 310×160 px se rellena hasta 1280×1280 y casi todo el cálculo es relleno. Se exportó el mismo modelo con alto y ancho libres (`--dynamic`), se fijó cada tamaño en su propia sesión con las sustituciones de dimensión libre de ONNX Runtime (un solo `.onnx` y varias sesiones: una sesión dinámica no corre entera en DirectML) y cada recorte se mandó al tamaño más pequeño que lo contiene sin reducirlo.

Una inferencia con DirectML (float32, mediana de 15): 1280×1280, 105,6 ms; 1280×736, 64,3 ms; 640×640, 31,8 ms; 640×320, 19,7 ms; 320×320, 12,8 ms; 320×160, 8,7 ms. La ganancia es real.

**Pero las detecciones cambian.** Se compararon las cajas de cada recorte en su entrada más pequeña con las del mismo recorte en 1280×1280, sobre 7 frames sintéticos de 1920×1080 (`synthetic_frames.py`: la ventana con las semillas 0 a 5 y la de tres cambios) y regiones barridas de 229×152, 310×160, 318×160, 300×300, 400×200, 600×300, 640×640 y 900×500 px:

| Entrada de los recortes | Comparaciones | Cajas de referencia | Cajas candidatas | Emparejadas a IoU 0,5 | Recall | Precisión | Conjuntos idénticos |
|---|---|---|---|---|---|---|---|
| Mejor ajuste (de 320×160 a 1280×736) | 574 | 1 745 | 5 561 | 1 651 | 0,946 | 0,297 | 167 (29 %) |
| Todo en 1280×736 | 567 | 1 564 | 1 900 | 1 491 | 0,953 | 0,785 | 351 (62 %) |
| Todo en 640×640 | 560 | 1 363 | 2 341 | 1 343 | 0,985 | 0,574 | 266 (48 %) |

Un recorte que llena su entrada (310×160 en 320×160) da unas siete veces más cajas que en 1280×1280 (1 065 frente a 156 en las regiones de 310×160), y las cajas emparejadas difieren hasta 0,69 en puntuación. El modelo depende del contexto que rodea al recorte, como ya mostró la agrupación de §9.5, y cada caja de más sería un objetivo `confirmed` (§4). Incluso recortar sólo el relleno vertical (1280×736) cambia el 38 % de los conjuntos.

**Criterio de aceptación** (fijado antes de medir el float16 y aplicado a ambos): a IoU 0,5, recall de las cajas de referencia y precisión de las candidatas ≥ 0,99; ninguna caja emparejada se mueve más de 2 px ni cambia de puntuación más de 0,02; y toda caja sin pareja tiene una puntuación a menos de 0,01 del umbral. **Las entradas más pequeñas no lo cumplen: no se adoptan.** El manifiesto sigue declarando una sola entrada de 1280×1280. La capacidad (`input_shapes` e `input_dimensions` en el manifiesto, una sesión por tamaño, `best_fit`) queda en el adaptador, con pruebas de manifiesto y de elección de tamaño, para el día en que se valide un tamaño menor con datos etiquetados; `s0_shapes` la mide.

### 10.3 Red en float16: equivalencia [V]

Mismos frames, mismas regiones y el mismo criterio, con DirectML en las dos: referencia = float32 (manifiesto de §9, 1280×1280); candidato = float16 con decodificación en float32 (`manifest.rnf05.json`). 656 comparaciones (los 7 frames de §10.2 y el de ruido).

| Región | Comparaciones | Cajas de referencia | Cajas candidatas | Emparejadas | Recall | Precisión | IoU medio | Dif. máx. de esquina (px) | Dif. máx. de puntuación | Conjuntos idénticos |
|---|---|---|---|---|---|---|---|---|---|---|
| 229×152 | 200 | 327 | 325 | 325 | 0,994 | 1,000 | 0,9989 | 0,65 | 0,0093 | 198 |
| 300×300 | 64 | 244 | 244 | 243 | 0,996 | 0,996 | 0,9989 | 0,34 | 0,0058 | 62 |
| 310×160 | 128 | 172 | 173 | 172 | 1,000 | 0,994 | 0,9992 | 0,57 | 0,0063 | 127 |
| 318×160 | 128 | 188 | 187 | 187 | 0,995 | 1,000 | 0,9991 | 0,13 | 0,0043 | 127 |
| 400×200 | 72 | 184 | 183 | 183 | 0,995 | 1,000 | 0,9988 | 0,30 | 0,0078 | 71 |
| 600×300 | 32 | 147 | 146 | 146 | 0,993 | 1,000 | 0,9990 | 1,41 | 0,0033 | 31 |
| 640×640 | 16 | 203 | 205 | 203 | 1,000 | 0,990 | 0,9985 | 0,30 | 0,0062 | 14 |
| 900×500 | 8 | 202 | 201 | 201 | 0,995 | 1,000 | 0,9988 | 0,17 | 0,0093 | 7 |
| Frame completo | 8 | 181 | 179 | 179 | 0,989 | 1,000 | 0,9979 | 0,35 | 0,0083 | 6 |
| **Total** | **656** | **1 848** | **1 843** | **1 839** | **0,995** | **0,998** | — | **1,41** | **0,0093** | **643 (98,0 %)** |

Las 13 comparaciones con diferencia tienen una sola caja de más o de menos. Las 11 de regiones registran su puntuación, entre 0,0501 y 0,0510: cajas que estaban en el umbral de 0,05 y que el redondeo de float16 deja a un lado o al otro. El IoU mínimo de una caja emparejada es 0,962. **Cumple el criterio.** No es idéntico bit a bit y no lo pretende: es del orden del redondeo de float (§9.2), no un cambio de contexto como en §10.2. La precisión de la detección frente a un corpus etiquetado sigue sin medirse (§7).

### 10.4 Medición en el motor [V]

`python tools/perf/measure_latency.py` contra `tools/s4/testapp` (compilada en `--release`, maximizada y en primer plano, comprobado con `GetForegroundWindow` antes y después de cada corrida; una corrida con otro primer plano se descarta), `ommatix demo --source dxgi --timings` con DirectML, OCR `winrt` es-ES, `[models] accessibility = "uia"`, `dry_run` y sin entrada. 30 procesos por variante y caso (40 en el refresco completo; 20 observaciones cada uno), alternados proceso a proceso y en orden rotatorio, de modo que lo que haga el resto del escritorio (UI Automation tiene que preguntar a las 13 ventanas de nivel superior de esta sesión) afecte por igual a las tres variantes. Medianas / p95 por rango más cercano, en ms. Las observaciones de un proceso no son independientes (capítulo 09 §9.6), y `n` cuenta observaciones. Una observación sin región planificada (el contador no avanzó entre dos capturas; unos 15 ms) no es un análisis incremental y se cuenta aparte. Las tres variantes: **antes** (`main`, `adfb1ea`, float32, etapas en serie); **concurrentes, float32** (este código con el manifiesto de §9); **concurrentes, float16** (este código con `manifest.rnf05.json`). La etiqueta avanza cada 100 ms y el velo del refresco completo, cada 250 ms.

| Caso | Variante | n | capture | plan | detect | recognize | accessibility | fuse | track | total |
|---|---|---|---|---|---|---|---|---|---|---|
| Refresco completo en caliente | antes (`main`) | 349 | 3,9 / 17,9 | 0,3 / 0,4 | 121,4 / 123,1 | 33,3 / 37,2 | 62,5 / 71,3 | 0,1 / 0,1 | 0,5 / 0,7 | 224,5 / 240,2 |
| Refresco completo en caliente | etapas concurrentes, float32 | 101 | 8,3 / 15,9 | 0,3 / 0,3 | 122,9 / 127,5 | 34,8 / 42,9 | 61,8 / 85,0 | 0,1 / 0,1 | 0,5 / 0,8 | 134,6 / 143,8 |
| Refresco completo en caliente | etapas concurrentes, float16 (RNF-05) | 71 | 7,8 / 15,5 | 0,3 / 0,3 | 80,8 / 81,8 | 34,1 / 37,0 | 61,2 / 64,8 | 0,1 / 0,1 | 0,4 / 0,5 | 90,8 / 98,2 |
| Incremental, 1 región | antes (`main`) | 540 | 3,1 / 4,4 | 2,0 / 2,6 | 113,6 / 114,7 | 3,4 / 4,7 | 61,4 / 69,6 | 0,1 / 0,1 | 0,5 / 0,8 | 184,3 / 195,4 |
| Incremental, 1 región | etapas concurrentes, float32 | 444 | 3,1 / 12,5 | 2,1 / 2,7 | 109,9 / 115,2 | 3,7 / 4,8 | 65,4 / 77,4 | 0,1 / 0,1 | 0,5 / 1,0 | 117,0 / 136,3 |
| Incremental, 1 región | etapas concurrentes, float16 (RNF-05) | 313 | 6,9 / 14,7 | 2,1 / 2,4 | 70,4 / 77,9 | 3,6 / 4,3 | 58,3 / 83,7 | 0,1 / 0,1 | 0,5 / 0,6 | 83,0 / 101,6 |
| Incremental, 2 regiones | antes (`main`) | 450 | 3,2 / 3,9 | 2,0 / 2,6 | 221,6 / 223,0 | 7,6 / 9,5 | 62,4 / 72,6 | 0,1 / 0,1 | 0,4 / 0,7 | 297,7 / 309,6 |
| Incremental, 2 regiones | etapas concurrentes, float32 | 450 | 3,1 / 3,7 | 2,2 / 2,9 | 219,4 / 221,8 | 8,8 / 10,2 | 63,2 / 77,8 | 0,1 / 0,1 | 0,5 / 0,9 | 226,5 / 233,4 |
| Incremental, 2 regiones | etapas concurrentes, float16 (RNF-05) | 440 | 3,1 / 6,2 | 2,1 / 2,9 | 129,0 / 135,8 | 8,0 / 9,8 | 72,8 / 138,0 | 0,1 / 0,1 | 0,5 / 0,7 | 136,4 / 153,0 |
| Primera observación de la sesión (refresco completo) | antes (`main`) | 95 | 5,6 / 6,2 | 0,0 / 0,1 | 263,5 / 332,7 | 41,1 / 51,5 | 64,8 / 78,7 | 0,1 / 0,1 | 0,1 / 0,1 | 378,1 / 450,6 |
| Primera observación de la sesión (refresco completo) | etapas concurrentes, float32 | 95 | 5,6 / 11,9 | 0,0 / 0,1 | 123,4 / 125,0 | 42,4 / 50,4 | 67,8 / 81,4 | 0,1 / 0,1 | 0,1 / 0,1 | 130,5 / 135,8 |
| Primera observación de la sesión (refresco completo) | etapas concurrentes, float16 (RNF-05) | 96 | 5,5 / 6,5 | 0,0 / 0,1 | 80,8 / 82,3 | 42,0 / 46,0 | 67,8 / 120,2 | 0,1 / 0,1 | 0,1 / 0,1 | 88,9 / 130,9 |

Lectura:

- **Las etapas concurrentes ahorran casi toda la lectura de accesibilidad y el OCR** (con una región, 184 → 117 ms): el total pasa a ser el de la etapa más larga, la detección. Con dos regiones el ahorro es el mismo y la detección sigue mandando (298 → 226 ms).
- **El float16 recorta la inferencia** de unos 110 ms por región a unos 65 (una inferencia aislada: 105,6 → 60,7 ms): con una región, 117 → 83 ms; con dos, 226 → 136 ms; con refresco completo, 135 → 91 ms; primera observación de la sesión, 130 → 89 ms (p95 131 por casos en que la lectura de accesibilidad tarda 120 ms).
- **Con dos regiones la etapa más lenta es la accesibilidad en su cola**: p95 138 ms (mediana 73), porque mientras la GPU ejecuta float16 el escritorio responde más despacio (§10.5), y esa cola fija el p95 del total.
- Los elementos publicados son los mismos en las tres variantes (75 de mediana con una región, 64 con dos, 75 en el refresco completo).

### 10.5 Hallazgo: la GPU saturada por float16 retrasa a UI Automation

Con una etiqueta que avanza cada **30 ms** (la aplicación de prueba repinta a unos 33 fotogramas por segundo, con OpenGL en la misma GPU) el resultado cambia (15 procesos por variante):

| Caso | Variante | n | capture | plan | detect | recognize | accessibility | fuse | track | total |
|---|---|---|---|---|---|---|---|---|---|---|
| Incremental, 1 región | antes (`main`) | 270 | 3,1 / 4,1 | 2,0 / 2,6 | 116,5 / 118,0 | 3,6 / 5,1 | 64,2 / 79,5 | 0,1 / 0,1 | 0,5 / 0,8 | 190,5 / 208,2 |
| Incremental, 1 región | etapas concurrentes, float32 | 270 | 3,1 / 3,6 | 2,2 / 2,9 | 112,2 / 116,4 | 3,8 / 5,1 | 73,6 / 85,1 | 0,1 / 0,1 | 0,5 / 0,9 | 119,2 / 130,8 |
| Incremental, 1 región | etapas concurrentes, float16 (RNF-05) | 270 | 3,1 / 4,2 | 2,1 / 2,8 | 72,1 / 80,0 | 4,1 / 5,4 | 121,4 / 146,3 | 0,1 / 0,1 | 0,5 / 1,0 | 128,0 / 155,8 |
| Incremental, 2 regiones | antes (`main`) | 270 | 3,1 / 3,9 | 2,0 / 2,6 | 226,9 / 228,6 | 7,7 / 10,2 | 65,2 / 72,0 | 0,1 / 0,1 | 0,4 / 0,8 | 306,0 / 316,6 |
| Incremental, 2 regiones | etapas concurrentes, float32 | 270 | 3,2 / 3,8 | 2,1 / 2,9 | 224,2 / 227,0 | 9,1 / 10,4 | 69,6 / 85,6 | 0,1 / 0,1 | 0,5 / 0,8 | 231,2 / 238,7 |
| Incremental, 2 regiones | etapas concurrentes, float16 (RNF-05) | 270 | 3,0 / 3,8 | 2,1 / 2,7 | 134,7 / 155,9 | 8,6 / 11,2 | 171,0 / 193,2 | 0,1 / 0,1 | 0,5 / 0,9 | 177,7 / 199,5 |

Con float16 la lectura de accesibilidad tarda lo que la GPU más unos 40 a 50 ms (121 ms con una región y 171 con dos, frente a 59 en serie), y con una región el float16 es *peor* que el float32 (p95 156 frente a 131 ms). La causa no está en el motor: con un proceso aparte que sólo carga la GPU con inferencias, la captura DXGI tarda 57 ms en vez de 3 con float16 y 6 ms con float32; desactivar la fusión de grafos de DirectML (`ep.dml.disable_graph_fusion`) no lo arregla y vuelve la inferencia un 29 % más lenta. Los núcleos de float16 de esta GPU parecen ocupar la cola gráfica durante toda la inferencia, y el escritorio (y la aplicación observada, si repinta en ese momento) espera. Con una animación de 100 ms o de 250 ms la lectura no se retrasa (§10.4).

Es una interacción de esta GPU y este controlador, observada en una máquina y una aplicación; no se sabe si ocurre en otro hardware. Para el flujo de S4 (el clic cambia un botón y un texto: dos o tres regiones) el float16 sigue siendo la mejor opción; con una sola región y una aplicación que repinta de forma continua, el float32 concurrente es la alternativa más segura.

### 10.6 RNF-05

Metas iniciales de RNF-05: incremental p95 ≤ 150 ms y refresco completo en caliente p95 ≤ 700 ms (ADR-0010: hipótesis). Con float16 y etapas concurrentes, en esta máquina, con esta aplicación y con la animación de 100 ms:

- **Refresco completo en caliente: se cumple** (p95 98 ms; antes 240).
- **Incremental con una región: se cumple** (p95 102 ms; antes 195).
- **Incremental con dos regiones: no se cumple, por poco** (p95 153,0 ms con 440 observaciones y 150,3 en una repetición de 197; mediana 136; antes, p95 310).
- Con tres o más regiones no se cumple: cada región añade unos 65 ms de GPU (no medido).
- Con una animación continua de 33 fotogramas por segundo tampoco se cumple con una región (p95 156 ms), por §10.5.
- La primera observación de una sesión: p95 131 ms (antes 451).

**RNF-05 no queda cumplido.** Hay una máquina, un modelo, una aplicación, observaciones que no son independientes dentro de un proceso y ninguna medición de RSS ni de VRAM (RNF-06). Con float16 la memoria de vídeo debería bajar (116 MB de pesos en vez de 233), pero no se midió.

### 10.7 Qué queda abierto

- La lectura de accesibilidad es ya el 70 % de una observación incremental de una región (58 de 83 ms): enumerar las 13 ventanas de nivel superior de la sesión cuesta unos 48 ms con cualquier condición o caché que se probó (sin propiedades, condición verdadera, vista de control). Evitarlo exigiría cambiar qué ventanas lee ADR-0017 o guardar la lista entre observaciones, con el riesgo de perder una ventana emergente con un campo de contraseña.
- Dos regiones por debajo de 150 ms (p95) necesitan quitar entre 3 y 5 ms más: bajar la inferencia por región sin cambiar el contexto (§10.2) o evitar la enumeración de ventanas.
- Las detecciones de float16 se validaron contra float32 sobre frames sintéticos, no contra un corpus etiquetado (§7).
- Las corridas de S4 de este repositorio usaban el manifiesto de §9 (float32). **Decisión del responsable (14 de octubre de 2026):** desde la sexta ronda usan el manifiesto float16 de este apartado (`plantillas/model_manifest.rnf05.json`); las rondas anteriores no se repiten. Decidió también no cambiar la enumeración de ventanas de ADR-0017 para llegar a los 150 ms con dos regiones: RNF-05 sigue sin cumplirse por unos 3 ms ([CHANGELOG 1.0.23](CHANGELOG.md)).

## 11. PERF-01 y RNF-04: latencia del núcleo (1.0.35)

**Qué mide.** El núcleo sin detector, sin OCR, sin transporte y sin sistema operativo (capítulo 03 §3.4: «Core: algoritmos sin detector/OCR/transporte/SO»), sobre una escena sintética de **300 controles y 600 fragmentos de texto** en 1920×1080, compilado en `--release`, con `cargo run --release -p ommatix-acceptance --example core_latency` (`src/acceptance/examples/core_latency.rs`). No lee la pantalla ni envía entrada. Dos capas:

- **Núcleo del dominio, etapa por etapa:** Algoritmo 1 (diferencia de tiles y plan de regiones contra la base confirmada y la evidencia anterior), fusión (`fuse`), identidad (`Registry::observe`) y puesta en escena y *commit* del snapshot (`SnapshotStore`). El total es un solo bloque cronometrado, no la suma de los percentiles de las etapas (capítulo 09 §9.6).
- **Núcleo de la aplicación:** `Engine::observe_screen` entero (con la primera página de 200 elementos) sobre dobles instantáneos de los puertos, con reloj monotónico y las etapas en serie (un hilo por etapa sólo mediría el coste de crearlo). Aquí `plan`, `fusión` y `seguimiento` son los tiempos que el propio motor informa en `_meta`.

Tres situaciones: **primera observación** (sin base: refresco completo), **en caliente sin cambios** (la misma pantalla de nuevo: nada que re-analizar, pero todo se vuelve a fusionar y a asociar) y **en caliente con una región** (cambia una celda cada vez). Los percentiles son de rango más cercano sobre las muestras tras 200 de calentamiento: 10 000 muestras por situación en la capa del dominio y 2 000 en la del motor (capítulo 09 §9.6 pide al menos 10 000 para las colas; la capa del motor no las alcanza).

**Máquina y compilación [V].** La de §1 (AMD Ryzen 7 5700X, 8 núcleos y 16 hilos, 31,9 GiB, AMD Radeon RX 6700 XT sin uso, Windows 11 Pro 26H2 build 26300.9550, plan «Alto rendimiento»), comprobada de nuevo con `tools/repro/environment.py`. `rustc 1.99.0 (b940084d7 2026-09-28)`, perfil `release` del espacio de trabajo (`lto = "thin"`, `codegen-units = 1`). Commit `75b339da0efd3ba39c55f0616806c4d0e942abd4`, árbol limpio (al reajustar la rama sobre `main`, que entretanto sólo cambió documentación, ese commit pasó a ser `161d079`: el código medido es el mismo). Un solo hilo de medición; sin `ommatix.exe` en ejecución ni ninguna ronda en vivo durante la corrida (103 s). Registro completo: `plantillas/perf01_core_latency.s0.json`; entorno: `plantillas/environment.rel01.json`.

### 11.1 Resultados [V]

Núcleo del dominio, milisegundos, **p50 / p95 / p99** (10 000 muestras por situación):

| Situación | Plan (Alg. 1) | Fusión | Identidad | Commit | **Total del núcleo** | Máximo |
|---|---|---|---|---|---|---|
| Primera observación (refresco completo) | 0,005 / 0,011 / 0,060 | 1,219 / 1,492 / 2,059 | 0,253 / 0,323 / 0,450 | 0,000 / 0,000 / 0,000 | **1,477 / 1,809 / 2,503** | 5,418 |
| En caliente, sin cambios | 1,172 / 1,651 / 1,967 | 1,206 / 1,492 / 2,075 | 0,512 / 0,643 / 0,932 | 0,016 / 0,025 / 0,036 | **2,926 / 3,666 / 4,851** | 11,493 |
| En caliente, una región | 1,561 / 1,923 / 2,370 | 1,226 / 1,507 / 2,107 | 0,525 / 0,654 / 0,949 | 0,018 / 0,028 / 0,037 | **3,349 / 3,965 / 5,325** | 9,305 |

`Engine::observe_screen` sobre dobles instantáneos, milisegundos, **p50 / p95 / p99** (2 000 muestras por situación; 604 elementos publicados en las tres):

| Situación | `observe_screen` (reloj de pared) | Plan | Fusión | Seguimiento | Plan + fusión + seguimiento |
|---|---|---|---|---|---|
| Primera observación (refresco completo) | 1,721 / 1,963 / 2,235 | 0,005 / 0,007 / 0,010 | 1,269 / 1,403 / 1,630 | 0,409 / 0,484 / 0,581 | 1,686 / 1,882 / 2,190 |
| En caliente, sin cambios | 3,103 / 3,677 / 4,991 | 1,127 / 1,525 / 1,856 | 1,270 / 1,468 / 2,153 | 0,647 / 0,762 / 1,151 | 3,063 / 3,642 / 4,885 |
| En caliente, una región | 3,636 / 4,605 / 6,037 | 1,609 / 2,042 / 2,642 | 1,285 / 1,642 / 2,313 | 0,666 / 0,900 / 1,242 | 3,586 / 4,525 / 5,960 |

La captura del doble tarda menos de un microsegundo (el fotograma se comparte por referencia): la «espera de captura» del capítulo 03 §3.4 no está en estas cifras, como tampoco el detector, el OCR ni la accesibilidad.

### 11.2 Lectura

- **RNF-04 se cumple en el núcleo, en esta máquina y con esta escena sintética:** el p95 peor de las tres situaciones es **3,97 ms** (en caliente con una región; la meta es ≤ 10 ms) y el de `observe_screen` entero, 4,61 ms. El p99 peor es 5,3 ms.
- **Hay colas:** un máximo de 11,5 ms en 10 000 muestras (en caliente, sin cambios) y de 9,3 ms en la situación con una región. La meta es de p95 y el p99 queda en la mitad; el máximo es una muestra aislada, que se conserva (capítulo 09 §9.7: no se publican sólo los éxitos).
- **Dónde se va el tiempo en caliente:** la diferencia de tiles contra la base cuesta 1,2 a 1,6 ms (comparar 8 MB), la fusión 1,2 ms, la asociación de identidad 0,5 ms y el *commit* menos de 0,02 ms. La fusión es cuadrática en controles y palabras (300 × 600) y es la etapa que más crecería con una pantalla más densa.
- **Lo que no mide:** ni el detector, ni el OCR, ni la lectura de accesibilidad, ni la captura (en vivo, §10.4: unos 58 ms de accesibilidad y 70 a 120 de detección por región), ni el transporte MCP ni la serialización. Las observaciones de una situación no son independientes (misma escena); la escena es sintética y regular, no una captura; hay una máquina y un plan de energía. No sustituye a PERF-02 ni a RNF-05, que **siguen sin cumplirse** (§10.6), ni a la casilla de rendimiento de la Definition of Done, que pide también los pesos.

### 11.3 Defecto que PERF-01 encontró y que se corrigió

La primera corrida, con el código de `main` (commit `a5cd1f9`, 300 muestras por situación y 100 en la capa del motor, misma máquina y compilación), **no cumplía RNF-04**: en caliente, el total del núcleo era de 60 / 75 / 87 ms (p50 / p95 / p99), y 57 / 71 / 82 ms de ellos eran la asociación de identidad (`Registry::observe`). La causa: la asociación comparaba cada pista con cada observación (604 × 604 pares) y, **para cada par, normalizaba a Unicode (NFC) y en minúsculas la etiqueta y el discriminante de las dos**, con cuatro asignaciones de cadena por par. La primera observación no lo sufría (0,075 ms) porque no había pistas.

| Situación | Total del núcleo antes (n = 300) | Total del núcleo después (n = 10 000) | Identidad antes → después (p50) |
|---|---|---|---|
| Primera observación | 1,369 / 1,502 / 2,302 | 1,477 / 1,809 / 2,503 | 0,075 → 0,253 |
| En caliente, sin cambios | 60,130 / 75,165 / 86,612 | 2,926 / 3,666 / 4,851 | 56,882 → 0,512 |
| En caliente, una región | 61,383 / 78,657 / 87,656 | 3,349 / 3,965 / 5,325 | 58,021 → 0,525 |

(`observe_screen` en caliente, antes: 61,1 / 72,4 / 77,7 ms y 60,1 / 74,9 / 87,3 ms; ahora 3,1 / 3,7 / 5,0 y 3,6 / 4,6 / 6,0.) La corrección calcula cada clave una vez por elemento y agrupa las observaciones por (rol, etiqueta, discriminante), de modo que una pista sólo se compara con las observaciones de su grupo. **La asociación es la misma**: `perf_01_the_association_is_the_pairwise_definition` la compara con la definición por pares (código independiente) en 400 escenas generadas, con etiquetas repetidas, escritas de distinta forma Unicode y desplazadas, y `perf_01_every_element_is_normalised_once_not_once_per_pair` falla si una etiqueta se normaliza más de una vez por elemento. La primera observación pasa de 0,075 a 0,253 ms de identidad porque ahora agrupa antes de comparar: unos 0,18 ms que se pagan una vez por observación y que en caliente se recuperan de sobra.

### 11.4 Reproducir

`cargo run --release -p ommatix-acceptance --example core_latency -- --out perf01.json` (unos dos minutos de un solo hilo; con `--samples N`, `--engine-samples N` y `--warmup N`). Se niega a correr sin optimizar. Las pruebas de la propia herramienta (`perf_01_*`) corren con `cargo test`. El informe nombra la compilación, el commit y si el árbol estaba limpio; `S4_REPRODUCIBILIDAD.md` §4 da el resto de comandos.

### 11.5 Medición de 1.0.54: antes y después de los tramos nuevos (ADR-0024)

**Qué se midió.** El mismo ejemplo `core_latency` en `--release`, con los valores por defecto (10 000 muestras por situación en el núcleo, 2 000 en `observe_screen` entero, calentamiento de 200). **Antes**: `origin/main` en `f749108`, que incluye INC-03 (vigencia) e INC-04 (identidad nativa). **Después**: la rama `feat/inc01-measurement` con esta PR. La escena es sintética: no se lee la pantalla ni se envía entrada. Rondas anteriores, contra `ecd8986` y contra `114a020`, no son la base de esta PR: `114a020` no mostró el desplazamiento descrito abajo (diez pares, diferencias de p95 entre −0,02 y +0,04 ms).

**Seis pares equilibrados** (tres con la base primero, tres con la rama primero). Tabla de medianas de cada binario; la última columna es la diferencia de medianas, rama menos base:

| Capa | Situación | p50 base | p50 rama | p95 base | p95 rama | Δ p50 | Δ p95 |
|---|---|---|---|---|---|---|---|
| núcleo (`core_total`) | primera observación | 1,49 | 1,49 | 2,62 | 2,62 | −0,007 | −0,004 |
| núcleo (`core_total`) | en caliente, sin cambios | 2,87 | 2,72 | 4,91 | 5,24 | −0,154 | +0,333 |
| núcleo (`core_total`) | en caliente, una región | 3,16 | 3,01 | 5,31 | 5,51 | −0,155 | +0,205 |
| `observe_screen` entero | primera observación | 1,76 | 1,76 | 3,03 | 3,03 | −0,004 | +0,003 |
| `observe_screen` entero | en caliente, sin cambios | 3,12 | 2,97 | 5,29 | 5,63 | −0,143 | +0,346 |
| `observe_screen` entero | en caliente, una región | 3,41 | 3,24 | 5,67 | 5,92 | −0,167 | +0,244 |

Milisegundos. El peor p95 del núcleo, por corrida, fue de 5,27 a 5,40 ms en la base y de 5,50 a 5,54 ms en la rama: los dos rangos no se solapan.

**Comprobación de la causa.** Una variante de prueba, compilada aparte y no incluida en la PR, con la comprobación de la línea de base B0 del planificador desactivada, se midió contra la base y contra la rama en cuatro rondas, en una ventana tranquila (carga del procesador en torno al 10 %), rotando el orden. Mediana de p95 en caliente, sin cambios: base 4,82, rama 5,26, variante 5,33 ms. En una región: base 5,22, rama 5,55, variante 5,36 ms. La variante muestra el mismo desplazamiento que la rama, así que la comprobación del planificador no es la causa. Las diferencias de una ronda a otra van de −1,8 a +2,0 ms; esta ventana también es ruidosa, y el tamaño exacto del efecto no queda establecido.

**Lectura.**

1. **Hay un aumento de p95 en las rutas en caliente, y aparece en todas las corridas de la comparación de seis pares.** La mediana de p95 sube entre 0,2 y 0,35 ms en las situaciones en caliente, mientras que la de p50 baja unos 0,15 ms. Los rangos por corrida no se solapan.
2. **La causa no está atribuida.** La misma rama no mostró el desplazamiento frente a `114a020`, y la variante sin la comprobación del planificador lo muestra igual. Que el efecto dependa de la base apunta a la disposición del binario (alineación y colocación de las funciones calientes), no a la lógica de los cambios. Es una hipótesis: no está verificada. Lo que la confirmaría o la descartaría es un perfil del ejemplo con un perfilador de muestreo, o un ensayo de alineación.
3. **RNF-04 se cumple con holgura.** En todas las corridas de la rama el p95 del núcleo quedó entre 3,4 y 6,3 ms, frente al objetivo de 10 ms (RNF-04, capítulo 03).
4. **Coste por observación.** No medido de forma aislada. La rama añade cuatro lecturas del reloj (fin de la publicación y de la construcción de la página), una copia del registro del plan y unas sumas. Es una estimación del camino de código, de cientos de nanosegundos, y no se presenta como medida.

**Pendiente.** Confirmar o descartar la causa del desplazamiento antes de declarar que no hay regresión de latencia; RNF-04 sobre el equipo de referencia en condiciones controladas (capítulo 06 §6.6); y la medición en vivo de B0 y B1, que programa el coordinador.

**Límites.** Seis pares y cuatro rondas no resuelven diferencias de décimas de milisegundo, y la carga del equipo, que es compartido, varía entre corridas. Estas cifras no sustituyen la medición de referencia.

## 12. PERF-03 / RNF-06: recursos y soak de 2 h (1.0.55; veredicto con el umbral del responsable en 1.0.62)

**Qué mide.** `tools/perf/soak.py` durante 120 minutos, **sin entrada de ningún tipo**: la herramienta sólo llama a `observe_screen` y `get_engine_metrics`, no toma ninguna observación mientras la aplicación de prueba no está en primer plano y no llama a `execute_action`. Se mide `ommatix serve --source dxgi` con la configuración del operador `ommatix.rnf05.directml.toml` (detector en float16 sobre DirectML, la de RNF-05, §10), que observa la aplicación de prueba propia (`tools/s4/testapp` con `--animate 100 --animate-labels 2`: dos etiquetas que cambian cada 100 ms, para que haya regiones que re-analizar). Una muestra cada 5 s. El ajuste de crecimiento deja fuera los primeros 10 minutos; las tablas de latencia los incluyen.

**Sin umbral cuando se midió.** RNF-06 pide «sin crecimiento sostenido» y no fija cifra, y el responsable no fijó ninguna antes de medir. Esta sección da las pendientes y los máximos medidos, y no declara por sí sola que haya o no crecimiento sostenido. Los criterios que el responsable fijó después, el 15 de octubre de 2026, se aplican en el veredicto de más abajo ([`09_EVALUACION_EXPERIMENTAL.md`](09_EVALUACION_EXPERIMENTAL.md) §9.7).

**Condiciones [V].**

| Condición | Valor |
|---|---|
| Commit medido | `fe35d80e292625666fb35c8f45c9acc3026a84b8` (`origin/main` entonces), árbol limpio. `main` avanzó después hasta `f749108`; no se volvió a medir |
| `ommatix.exe` (SHA-256) | `809b4305ef71bb65feb8e20e537dba6662cd44eb486c4852c5052fa17b800801` (`--release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia`) |
| `ommatix-testapp.exe` (SHA-256) | `5050596d46bc300fdc9c31d6835af00d470dd5fb9ca12b464d1e66edec29a942` (`tools/s4/testapp`, `--release --locked`) |
| Compilación | Incremental, sobre directorios `target` ya poblados por compilaciones anteriores |
| Configuración del operador (SHA-256) | `94af2154c74ba93da7500c3c8273ea05654be2b0ee9293e42d560375fecd5298`. Manifiesto `models\manifest.rnf05.json`: `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`, el fijado. `tools/repro/environment.py` no halló ningún `mismatch` |
| Entrada posible | Ninguna: `[engine] mode = "dry_run"`, `[safety] actuation_enabled = false` y `allowed_applications = []`. La comprobación de `soak.py` no halló ningún problema |
| Equipo | AMD Ryzen 7 5700X (8 núcleos, 16 hilos), 31,9 GiB; AMD Radeon RX 6700 XT (driver 32.0.21045.5002); Windows 11 Pro, build 26300.9550 |
| Plan de energía | «Alto rendimiento»; suspensión y apagado de pantalla en corriente alterna: nunca |
| Escala | 100 % (`AppliedDPI` 96) |
| Monitores | Dos conectados. Se captura la salida 0, la principal (1920×1080, origen 0,0); la otra (1920×1080, origen 1920,-137) no se captura |
| Notificaciones | **No desactivadas.** La clave del interruptor no figura en el registro, así que vale el valor por defecto (activadas). No se cambió |
| Protector de pantalla | Activo sin programa asignado (`SCRNSAVE.EXE` vacío): no arranca |
| Carga de otras sesiones | No se muestreó durante la corrida. Al comenzar: CPU total del 42 %, sin `rustc` ni `cargo` en marcha, y 44 procesos `python.exe` de otros proyectos del mismo equipo. Al terminar: CPU total del 7 %, sin `rustc` ni `cargo`, y 44 procesos `python.exe` de otros proyectos todavía activos |
| Ensayo previo | Un minuto con `--interval 2`: completado, 30 de 30 observaciones, sin pérdidas |
| Inicio y fin (hora local) | 02:27:56 y 04:27:59 |
| Terminación (`ended`) | `completed` |
| Observaciones | 1440 ticks de 5 s; 1440 observados; 0 pérdidas de primer plano; 0 errores de observación; 0 latencias descartadas |
| Acciones | Ninguna llegó al sistema: `delivered_actions` máximo 0; `rejected_actions` 0 |

**Recursos [V].** Del proceso `ommatix.exe` (el motor), tras los primeros 10 minutos (n = 1320 muestras). La pendiente por hora es de mínimos cuadrados sobre esas muestras; las medianas son del primer y del último décimo:

| Recurso | Mínimo | Máximo | Mediana, primer décimo | Mediana, último décimo | Pendiente por hora |
|---|---|---|---|---|---|
| Conjunto de trabajo (MiB) | 289,6 | 294,0 | 290,3 | 292,9 | +1,6 |
| Bytes privados (MiB) | 1457,4 | 1461,9 | 1458,8 | 1460,9 | +1,6 |
| Manejadores | 947 | 947 | 947 | 947 | 0 |
| Memoria dedicada de GPU (MiB) | 1203,1 | 1203,1 | 1203,1 | 1203,1 | 0 |

- El salto inicial ocurre entre el instante anterior a la primera observación (246 manejadores, 291,8 MiB privados, 2,4 MiB de GPU) y el quinto segundo (950 manejadores, 1452,5 MiB privados, 1203,1 MiB de GPU). Queda fuera del ajuste.
- La memoria de GPU es la memoria dedicada que los contadores de rendimiento de Windows asignan a `ommatix.exe` (`GPU Process Memory`), no el total del adaptador. Estuvo disponible (`gpu_counter.available = true`).

**Cola y acciones [V].** `pending_regions` fue 0 en las 1440 muestras. La cola del motor cuenta las componentes conexas de `pending_dirty` (`src/application/src/engine.rs`). Ese conjunto sólo recibe regiones cuando una observación **falla** (`record_failure`, `src/application/src/engine/observe.rs`) y se vacía en cada *commit* (`src/domain/src/snapshot.rs`). Con observaciones correctas la cola vale cero por construcción: esta corrida no ejercita un retraso de cambios pendientes, sólo confirma que ninguna observación falló. La época del snapshot fue 1 en todas las muestras y ninguna observación salió degradada.

**Latencia por ventanas de 10 minutos [V].** Milisegundos, p50 / p95 / p99 / máximo. «Motor» es el total que el motor informa en `_meta`; «ida y vuelta» es lo que mide el cliente en `observe_screen`, e incluye el transporte MCP.

| Ventana (min) | n | Motor: p50 / p95 / p99 / máx | Ida y vuelta: p50 / p95 / p99 / máx |
|---|---|---|---|
| 0–10 | 120 | 143,9 / 262,4 / 1146,7 / 6217,5 | 144,7 / 297,6 / 1147,5 / 6218,4 |
| 10–20 | 120 | 142,5 / 151,3 / 156,8 / 169,9 | 143,2 / 152,0 / 157,4 / 170,6 |
| 20–30 | 120 | 143,0 / 150,8 / 152,2 / 153,0 | 143,7 / 151,5 / 152,8 / 153,7 |
| 30–40 | 120 | 142,9 / 150,0 / 155,2 / 160,7 | 143,6 / 150,7 / 156,0 / 161,3 |
| 40–50 | 120 | 143,0 / 149,7 / 152,2 / 156,1 | 143,7 / 150,4 / 152,9 / 156,9 |
| 50–60 | 120 | 142,0 / 149,1 / 152,5 / 153,4 | 142,7 / 149,7 / 153,2 / 154,1 |
| 60–70 | 120 | 142,0 / 148,0 / 151,9 / 152,8 | 142,7 / 148,6 / 152,5 / 153,3 |
| 70–80 | 120 | 142,2 / 148,5 / 154,7 / 159,9 | 142,8 / 149,2 / 155,4 / 160,6 |
| 80–90 | 120 | 142,1 / 149,3 / 152,7 / 155,8 | 142,8 / 150,0 / 153,3 / 156,5 |
| 90–100 | 120 | 143,2 / 155,9 / 213,1 / 214,0 | 143,9 / 156,9 / 213,9 / 215,2 |
| 100–110 | 120 | 142,7 / 150,0 / 152,0 / 154,3 | 143,4 / 150,6 / 152,8 / 155,0 |
| 110–120 | 120 | 141,8 / 150,7 / 153,6 / 156,5 | 142,5 / 151,4 / 154,3 / 157,2 |
| **Global** | 1440 | 142,6 / 151,8 / 193,7 / 6217,5 | 143,3 / 152,5 / 195,5 / 6218,4 |
| **A partir del minuto 10** | 1320 | 142,5 / 150,8 / 155,8 / 214,0 | 143,2 / 151,5 / 156,5 / 215,2 |

- La primera llamada tardó 1050,5 ms de ida y vuelta y 103,2 ms de total del motor. El informe no desglosa la diferencia (unos 950 ms).
- Los primeros 10 minutos tienen una cola: 11 observaciones con más de 200 ms de total del motor, de las que 9 pasan de 250 ms, todas entre los segundos 60 y 152. La mayor, de 6217,5 ms, es del segundo 145. Después del segundo 152 ninguna supera 250 ms. El informe no guarda desglose por etapas y no se muestreó la carga del equipo, así que **la causa no está determinada**.
- Hay dos observaciones de unos 214 ms en la ventana de 90 a 100 minutos, sin causa identificada.

**Lectura.**
- La corrida terminó como `completed`: 120 minutos, 1440 de 1440 observaciones, sin pérdidas de primer plano ni errores. Cumple el protocolo de la sesión B ([`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md) §B) y no es una corrida truncada.
- **Memoria.** Después de los primeros 10 minutos, los bytes privados del motor estuvieron entre 1457,4 y 1461,9 MiB, un intervalo de 4,5 MiB. La pendiente medida es de +1,6 MiB/h sobre 1 h 50 min de muestras, y el conjunto de trabajo tiene la misma pendiente. Los manejadores y la GPU no cambiaron. En 1.0.55 no había umbral, así que esta sección no decía si la pendiente era «crecimiento sostenido»; el veredicto de 1.0.62 más abajo sí lo dice, con los criterios fijados entonces.
- **Cola.** Cero en todas las muestras, por la razón de arriba. RNF-06 pide una cola acotada: esta corrida la cumple sin observaciones fallidas, y no dice cómo se comporta la cola cuando una observación falla.
- **Latencia.** A partir del minuto 10, el total del motor tiene p50 de 142,5 ms, p95 de 150,8 ms y p99 de 155,8 ms, y las medianas de las ventanas se mantienen entre 141,8 y 143,2 ms. Esta medición no es la de RNF-05 (§10) y no cambia el estado de RNF-05.
- **Alcance.** La memoria es la del proceso `ommatix.exe`, no la de todo el sistema ni la de la aplicación de prueba.

**Veredicto con el umbral del responsable (1.0.62).** El 15 de octubre de 2026 el responsable fijó los criterios de [09 §9.7](09_EVALUACION_EXPERIMENTAL.md): el conjunto de trabajo crece no más de 10 MiB/h, y los manejadores no crecen (pendiente no positiva). Aplicados a esta corrida, con la misma pendiente que la herramienta publica (sólo cambió la presentación de `criteria`, no el cálculo):

| Criterio | Medido | Umbral | Resultado |
|---|---|---|---|
| Conjunto de trabajo, pendiente por hora | +1,6 MiB/h | no más de 10 MiB/h | Cumple |
| Manejadores, pendiente por hora | 0 (947 en las 1320 muestras tras el calentamiento) | no positiva | Cumple |
| Cola de regiones pendientes | 0 en las 1440 muestras, sin observaciones fallidas | sin cota fijada | Sin veredicto |
| Memoria dedicada de GPU y bytes privados | 0 y +1,6 MiB/h | sin umbral | Sólo se publica |

Veredicto de esta corrida, en sus condiciones (commit `fe35d80`, sin accesibilidad, sin entrada): **cumple los criterios de memoria de trabajo y de manejadores.** No es un veredicto sobre RNF-06. Falta la cota de la cola; el código medido no es el de `main` (después de `fe35d80` entraron INC-03 (#38), INC-04 (#39), INC-01 (#41) e INC-05 (#43)); y la ruta de UI Automation no tiene soak. Cumplir no demuestra ausencia de fugas. Los criterios se fijaron **después** de conocer la pendiente de esta corrida, así que el resultado no es una prueba preregistrada. La repetición con `uia` sobre el código actual está programada y se juzgará con los mismos valores, fijados antes de leerla.

**Qué queda abierto.**
1. **Umbral de RNF-06.** Fijado el 15 de octubre de 2026 para el conjunto de trabajo y los manejadores (veredicto de arriba). Falta la cota de la cola de regiones pendientes (decisión 9 de [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)): sin ella RNF-06 no tiene veredicto y su estado no cambia (`PENDIENTE_IMPLEMENTACION_O_VALIDACION`).
2. **Una sola corrida.** No hay variación entre corridas ni entre días; otra corrida la daría.
3. **Cola con fallos.** Esta corrida no inyecta fallos de observación. Medir la cola cuando una observación falla necesita un escenario propio.
4. **Causa de la cola de latencia** de los primeros 10 minutos y de las dos observaciones de 214 ms. Una corrida con las etapas de `_meta` y la carga del equipo muestreada durante la corrida la cerraría.
5. **Condiciones que no coinciden con la sesión B:** dos monitores conectados (la sesión pide uno; sólo se captura el principal) y notificaciones activadas. Se dejaron como estaban; los números las incluyen.

**Reproducir.** Desde la raíz del repositorio, con los dos ejecutables compilados (`S4_INSTALACION_Y_REVERSION.md` §3): `python tools/perf/soak.py --config %LOCALAPPDATA%\Ommatix\ommatix.rnf05.directml.toml --minutes 120 --out soak-120.json`. El informe bruto se guarda fuera del repositorio, con el commit y la máquina; no lleva datos de pantalla (ADR-0012). Las pruebas de la herramienta, sobre dobles: `python -m unittest discover -s tools/perf -p "test_*.py"`.

## 13. INC-02: B0 frente a B1 en vivo y PERF-01 sin carga (1.0.56)

**Qué se midió.** El protocolo congelado del [§9.9](09_EVALUACION_EXPERIMENTAL.md): B0 (refresco completo, `[engine] experimental_full_frame = true`) frente a B1 (incremental) sobre la aplicación de prueba propia (`tools/s4/testapp`), con el mismo binario, los mismos pesos y la misma configuración salvo esa clave, en tres casos: `incr1` (una etiqueta animada, una región), `incr2` (dos etiquetas, dos regiones) y `full` (un velo sobre toda la ventana, refresco completo). **Entrada: ninguna.** Ni el comando ni la herramienta envían entrada; la aplicación se anima sola (pidió el primer plano al arrancar) y `demo` sólo observa y ejecuta una acción en seco. El apartado 13.7 repite, sin carga, la comparación de PERF-01 que quedó pendiente en §11.5.

### 13.1 Condiciones [V]

| Condición | Valor |
|---|---|
| Commit medido | `f6d94cd00051548a0a2bdbbdb31d0e79b0ee8535` (`origin/main` tras la PR #43), árbol limpio |
| Compilación | `cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia`; `rustc 1.99.0` (`rust-toolchain.toml`); la aplicación de prueba, `cargo build --release --locked` en `tools/s4/testapp` |
| `ommatix.exe` (SHA-256) | `d28c89654fcd1e8fe879f7711e1ed0171617c6f3d2e2113f69d34e09c077084f` |
| `ommatix-testapp.exe` (SHA-256) | `5050596d46bc300fdc9c31d6835af00d470dd5fb9ca12b464d1e66edec29a942` (la misma que en §12) |
| Configuración B1 (SHA-256) | `2de6a8768374f615b947f62e8b7c9f0f41b618ccd3867b6c47e67c70530ed339` |
| Configuración B0 (SHA-256) | `e96bbe5ab8394b56256db9f5f0592ace6c01f6a83e5e2a7409ef4b8fab885a57` |
| Configuración de origen (SHA-256) | `94af2154c74ba93da7500c3c8273ea05654be2b0ee9293e42d560375fecd5298` (`ommatix.rnf05.directml.toml`, la misma que en §12) |
| Manifiesto del detector (SHA-256) | `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47` (`models\manifest.rnf05.json`, fijado por la configuración) |
| Pesos float16 (SHA-256) | `d7a8901db0ae650616ee43f53e6ee8f57a829de54ab8daae431be77556cc0805` (fijado) |
| Detector y OCR | ONNX Runtime 1.24.4 con DirectML 1.15.4 (`onnxruntime.dll` `e7eedec6…`, `DirectML.dll` `9c9e6d82…`), float16, entrada 1280×1280; OCR de Windows es-ES |
| Accesibilidad | **Ninguna.** La configuración no tiene `[models] accessibility`, así que vale `none`. §10.4 y las sesiones S4 usaron `uia`; ver 13.8 |
| Equipo | AMD Ryzen 7 5700X (8 núcleos, 16 hilos), 31,9 GiB; AMD Radeon RX 6700 XT (driver 32.0.21045.5002); Windows 11 Pro, build 26300.9550; plan «Alto rendimiento» |
| Captura | Salida 0 (1920×1080), `--source dxgi` |
| Animación | `--animate 30` (etiquetas que avanzan cada 30 ms, en `incr1` e `incr2`) y `--animate-flash 250` (velo cada 250 ms, en `full`): los valores por defecto de la herramienta |
| Observaciones por proceso | 20. La primera y la segunda no entran en las estadísticas de latencia |
| Procesos | 40 por variante y caso (240 en total), intercalados en orden rotatorio contra la misma instancia de la aplicación de prueba en cada caso y trozo; cada caso en dos trozos de 20 procesos |
| Identificadores | Experimento `inc02-b0b1-20261015`; semilla del bootstrap 20261015; 10 000 remuestreos |
| Fecha y hora | 15 de octubre de 2026, de 05:11 a 05:25 (hora local) |

**Desviaciones del protocolo, declaradas.**

- **B1 no es el archivo del operador tal cual.** Se le añade `experimental_full_frame = false` bajo `[engine]`, que es el valor que el §9.9 fija para B1 y también el valor por defecto. B0 difiere de B1 sólo en el valor de esa clave: es la única diferencia declarada.
- **Rutas absolutas.** Las tres rutas relativas de `[models]` (manifiesto, ONNX Runtime y DirectML) se escribieron absolutas en ambas copias: una ruta relativa se resuelve respecto al archivo de configuración (`src/app/src/config.rs`), y las copias están fuera del directorio del operador. El cambio es el mismo en las dos.
- **Ensayo previo.** Antes de medir hubo un ensayo de dos procesos por variante y caso (`inc02-ensayo-20261015`). Comprobó la orden; **se excluye** de todo lo que sigue y no se reutiliza como confirmación.

### 13.2 Ejecución y clases de observación [V]

Los 240 procesos terminaron `ok`: ninguno se descartó por cambio de primer plano, ninguno falló y ninguna observación tuvo error. Cada trozo tiene su identificador de corrida, y la unidad de apareamiento es el par (`run_id`, `sequence_id`).

| Caso | Variante | Primera | Segunda | Cálida (clase) | Inactiva | Error |
|---|---|---|---|---|---|---|
| `incr1` | B0 | 40 | 40 | 720 (`full`) | 0 | 0 |
| `incr1` | B1 | 40 | 40 | 668 (`regions`) | 52 | 0 |
| `incr2` | B0 | 40 | 40 | 720 (`full`) | 0 | 0 |
| `incr2` | B1 | 40 | 40 | 720 (`regions`) | 0 | 0 |
| `full` | B0 | 40 | 40 | 720 (`full`) | 0 | 0 |
| `full` | B1 | 40 | 40 | 66 (`full`) | 654 | 0 |

B0 analiza todas las observaciones como refresco completo, por construcción (razón `experimental_baseline`). Las inactivas de B1 son las observaciones sin nada que analizar: se cuentan, no se miden, y se reportan aparte en 13.4.

### 13.3 Latencia y comparación apareada [V]

Milisegundos de `total` (de la captura al snapshot publicado). Intervalos de 95 % bootstrap sobre procesos (secuencias), nunca sobre fotogramas.

| Caso | Variante | n cálidas | Mediana [IC 95 %] | p95 [IC 95 %] | Inferencias por observación | Elementos publicados (mediana / p95) |
|---|---|---|---|---|---|---|
| `incr1` | B0 | 720 | 89,5 [89,2; 89,9] | 97,5 [95,9; 98,3] | 1 | 85 / 85 |
| `incr1` | B1 | 668 | 70,8 [70,6; 71,1] | 78,4 [76,8; 80,9] | 1 | 84 / 92 |
| `incr2` | B0 | 720 | 89,3 [88,9; 89,6] | 96,9 [95,6; 99,1] | 1 | 85 / 86 |
| `incr2` | B1 | 720 | 134,8 [134,7; 135,0] | 142,5 [139,4; 144,5] | 2 | 67 / 85 |
| `full` | B0 | 720 | 100,0 [98,9; 100,8] | 109,5 [108,7; 110,6] | 1 | 80 / 84 |
| `full` | B1 | 66 | 88,6 [87,7; 90,2] | 97,6 [96,7; 97,9] | 1 | 83 / 84 |

Todas las inferencias usan una entrada de `1×3×1280×1280`. En `full`, B1 analizó sólo 66 observaciones en 37 procesos; su mediana y su p95 se apoyan en pocos valores.

**Diferencia apareada por proceso (B1 − B0).** Por proceso se toma la mediana (y el p95) de las cálidas de cada variante; la diferencia es B1 menos B0 (negativa: B1 más rápida); el estimador es la media sobre procesos, con su intervalo bootstrap.

| Caso | Δ de la mediana (ms) [IC 95 %] | Razón B1/B0 [IC 95 %] | Δ del p95 por proceso (ms) [IC 95 %] | Procesos |
|---|---|---|---|---|
| `incr1` | −18,6 [−18,9; −18,2] | 0,79 [0,79; 0,80] | −18,0 [−20,3; −15,6] | 40 |
| `incr2` | +45,8 [45,4; 46,1] | 1,51 [1,51; 1,52] | +46,7 [43,8; 49,7] | 40 |
| `full` | −9,9 [−11,3; −8,5] | 0,90 [0,89; 0,91] | no interpretable: 1 o 2 observaciones de B1 por proceso | 37 (3 excluidos: sin observaciones cálidas de B1) |

**Control de primera observación** (120 por variante, 40 por caso): B0 91,7 ms, B1 91,8 ms; detección 80,6 y 80,7 ms. La primera observación no difiere.

### 13.4 Tramos y observaciones inactivas [V]

Mediana, por proceso, de cada tramo (ms) en B0 y en B1, y diferencia apareada B1 − B0 con su intervalo. Los tramos concurrentes no se suman; el total se mide directamente (§9.4), así que las medianas de tramos no suman el total.

| Caso | Tramo | B0 | B1 | Δ B1 − B0 [IC 95 %] |
|---|---|---|---|---|
| `incr1` | captura | 2,9 | 2,9 | +0,01 [−0,03; 0,05] |
| `incr1` | planificación | 2,1 | 2,1 | +0,01 [−0,01; 0,02] |
| `incr1` | detección | 82,7 | 64,3 | −18,4 [−18,7; −18,2] |
| `incr1` | OCR | 35,9 | 3,2 | −32,6 [−32,7; −32,5] |
| `incr1` | publicación (`commit`) | 0,26 | 0,25 | −0,004 [−0,008; −0,001] |
| `incr2` | captura | 2,9 | 3,0 | +0,06 [+0,03; +0,09] |
| `incr2` | planificación | 2,1 | 2,1 | +0,02 [+0,01; +0,04] |
| `incr2` | detección | 82,4 | 128,2 | +45,9 [+45,6; +46,2] |
| `incr2` | OCR | 36,5 | 7,2 | −29,2 [−29,4; −29,1] |
| `incr2` | publicación (`commit`) | 0,26 | 0,26 | +0,004 [+0,001; +0,008] |
| `full` | captura | 15,9 | 7,5 | −8,2 [−9,4; −6,9] |
| `full` | planificación | 1,8 | 0,2 | −1,6 [−1,6; −1,5] |
| `full` | detección | 80,0 | 80,4 | +0,4 [+0,2; +0,6] |
| `full` | OCR | 34,2 | 34,2 | +0,2 [−0,2; +0,8] |
| `full` | publicación (`commit`) | 0,006 | 0,25 | +0,25 [+0,24; +0,26] |

En `incr1` y `incr2` la fusión, el seguimiento y la respuesta difieren de B0 en menos de 0,02 ms.

**Observaciones inactivas de B1 (aparte, no entran en la comparación).** `incr1`: 52 de 720 (7 %), mediana 5,2 ms, p95 5,9 ms. `incr2`: 0. `full`: 654 de 720 (91 %), mediana 14,1 ms, p95 20,1 ms.

### 13.5 Acuerdo entre variantes [V]

Coeficiente de Dice de los multiconjuntos de firmas de elementos, por observación apareada (mismo proceso y mismo número). **Es acuerdo entre dos variantes, no precisión** (§9.5).

| Caso | Dice medio por proceso [IC 95 %] | Observaciones apareadas | Multiconjuntos idénticos |
|---|---|---|---|
| `incr1` | 0,968 [0,966; 0,969] | 800 | 0 (0 %) |
| `incr2` | 0,845 [0,844; 0,847] | 800 | 0 (0 %) |
| `full` | 0,975 [0,973; 0,976] | 800 | 377 (47 %) |

En `incr1` e `incr2` el texto de la etiqueta animada (`Tic nnnn`) difiere entre dos corridas no sincronizadas, así que cada etiqueta añade una discrepancia y un multiconjunto no puede ser idéntico: la proporción de idénticos no dice nada ahí. En `full`, donde sólo cambia el velo, sí dice algo. **No hay control B0 frente a B0**, así que el desacuerdo de `incr1` mezcla la variante con la animación y no se puede separar.

### 13.6 Lectura

1. **Una región: B1 es más rápida.** En `incr1` la mediana baja de 89,5 a 70,8 ms (−18,6 ms, −21 %). El ahorro es del detector: con una región tarda 64 ms y con el frame completo 83 ms, aunque las dos entradas miden 1280×1280. El OCR baja de 36 a 3 ms, pero corre a la vez que la detección: en las dos variantes la detección es el camino crítico. Por qué una región cuesta menos en el detector no queda establecido: los registros no separan preprocesado, inferencia y postproceso. La hipótesis es que el frame completo se reduce antes de la entrada y el recorte no (§6), pero no está verificada.
2. **Dos regiones: B1 es más lenta.** En `incr2` la mediana sube de 89,3 a 134,8 ms (+45,8 ms, +51 %). Cada región es una inferencia completa: 128 ms para dos, frente a 82 ms del frame completo. Con el resto de B1 casi constante (6,5 ms) y unos 64 ms por región, la igualdad con el refresco completo cae entre una y dos regiones (≈ 1,3). Es una estimación de dos puntos, no una medida con tres o más regiones. A partir de dos regiones, la incrementalidad tal como está construida no ahorra.
3. **Refresco completo (control): no hay ventaja de B1.** Con un velo sobre toda la ventana, la detección (80 ms en ambas) y el OCR (34 ms en ambas) cuestan lo mismo. La diferencia de −9,9 ms viene de la captura (−8,2 ms) y de la planificación (−1,6 ms: B0 calcula la diferencia de teselas antes de forzar el refresco, según ADR-0024, y B1 decide por área en 0,25 ms). Ninguna se debe al algoritmo incremental. Además B1 sólo analizó 66 observaciones, las que siguieron a un cambio del velo, así que no es una muestra al azar de las de B0. Por qué la captura de B0 espera más no queda establecido.
4. **Acuerdo y calidad.** Con dos regiones B1 publica una mediana de 67 elementos frente a 85 de B0: 717 de sus 720 observaciones tienen entre 66 y 69. Con una región la mediana baja de 85 a 84. El registro no guarda cajas, así que no dice qué elementos faltan. El mecanismo está en el código: `merged` (`src/application/src/engine/observe.rs`, líneas 78 a 112) descarta todo control o palabra cuya caja solape una región replanificada, incluido su halo, y sólo conserva lo que la detección del recorte encuentra dentro. Si el recorte no reproduce lo que el frame completo detecta (§10.2), se pierden elementos. Esto no es una medida de precisión. La causa se investigó en 1.0.57 con las cajas (13.10): es el comportamiento del detector sobre recortes, no un defecto del plan.
5. **Qué ahorra B1 y qué no.** Ahorra cuando cambia una región, que es el caso de la prueba más sencillo. No ahorra con dos o más regiones en este modelo y esta GPU. Ahorra en las observaciones sin cambio (unos 5 ms, sin detector), pero esas no entran en la comparación de latencia (§9.9, límite 4). La captura, la publicación y la fusión no cambian. B0 paga unos 2 ms de planificación que un refresco de producción por área no paga (punto 3): B0 es una línea de base de experimento, no un modo de producción (ADR-0024).
6. **RNF-05: sin cambio de estado.** En esta condición (sin accesibilidad, con el manifiesto float16 de §10), el p95 incremental con dos regiones es de 142,5 ms [IC 95 %: 139,4; 144,5] y con una región de 78,4 ms: ambos quedan por debajo de la meta de 150 ms. **No se declara RNF-05 cumplido.** Esta condición no es la de §10, donde con accesibilidad `uia` el p95 de dos regiones fue de 153,0 ms; la accesibilidad forma parte del camino de producto (ADR-0017), y el estado del requisito lo fija el responsable (decisión del 15 de octubre de 2026, [capítulo 03](03_ESPECIFICACION_REQUISITOS.md) RNF-05).

### 13.7 PERF-01 sin carga: `114a020` frente a `f6d94cd` [V]

Cierra en parte el pendiente de §11.5. Tres bloques ABBA (A = `114a020`, antes de #39; B = `f6d94cd`, `main` actual), seis pares: cada bloque es A, B, B, A, y el par 1 es (A, B) y el par 2 es (B, A). Cada corrida es `core_latency` en `--release` (10 000 muestras por situación en el núcleo, 2 000 en `observe_screen`, 200 de calentamiento). `114a020` se midió en un árbol de trabajo temporal, en su propio directorio de compilación. El fichero `core_latency.rs` es el mismo en los dos commits, y el toolchain también. Cada informe registra su commit y un árbol limpio: A `114a020aa27504d87e4d2d1b955ff386da675069`, B `f6d94cd00051548a0a2bdbbdb31d0e79b0ee8535`. Carga de CPU medida antes de cada corrida: entre 2 % y 29 % (media 12 %); ningún `cargo`, `rustc` ni `ommatix` en marcha. Cada corrida duró unos 100 s, de 05:29 a 05:49.

Milisegundos. Mediana de las seis corridas de cada binario (A → B) y diferencia apareada B − A, media de los seis pares, con el intervalo bootstrap sobre pares (seis unidades: un intervalo orientativo) y el número de pares con diferencia positiva.

| Situación | Capa | p50 (A → B) | Δ p50 [IC 95 %] (pares > 0) | p95 (A → B) | Δ p95 [IC 95 %] (pares > 0) |
|---|---|---|---|---|---|
| Primera observación | núcleo | 1,482 → 1,534 | +0,053 [+0,051; +0,057] (6/6) | 1,629 → 1,695 | +0,067 [+0,051; +0,085] (6/6) |
| En caliente, sin cambios | núcleo | 2,835 → 2,885 | +0,047 [+0,037; +0,057] (6/6) | 3,075 → 3,149 | +0,088 [+0,051; +0,131] (6/6) |
| En caliente, una región | núcleo | 3,093 → 3,116 | +0,019 [−0,005; +0,041] (5/6) | 3,468 → 3,462 | −0,027 [−0,105; +0,041] (2/6) |
| Primera observación | `observe_screen` | 1,721 → 1,807 | +0,086 [+0,082; +0,092] (6/6) | 1,960 → 2,062 | +0,103 [+0,085; +0,126] (6/6) |
| En caliente, sin cambios | `observe_screen` | 3,075 → 3,151 | +0,071 [+0,058; +0,091] (6/6) | 3,398 → 3,474 | +0,004 [−0,208; +0,157] (5/6) |
| En caliente, una región | `observe_screen` | 3,336 → 3,389 | +0,058 [+0,030; +0,086] (6/6) | 3,771 → 3,771 | −0,001 [−0,113; +0,116] (3/6) |

**Lectura.**

- **El desplazamiento de p95 no persiste en esta medición.** El de +0,2 a +0,35 ms bajo carga no aparece: la mayor diferencia de p95 en caliente es +0,09 ms (núcleo, sin cambios), y con una región es −0,03 ms en el núcleo y 0,00 ms en `observe_screen`. Los p95 de una misma corrida varían entre 0,1 y 0,2 ms, por eso la comparación es apareada.
- **Persiste un desplazamiento pequeño de la mediana**: entre +0,05 y +0,09 ms (2 a 5 %) en la primera observación y en las observaciones sin cambio, con los seis pares en el mismo sentido. El p50 de `observe_screen` en caliente sube 1,6 a 2,5 %, no el 9 % que se vio bajo carga.
- **RNF-04 se cumple con holgura**: el p95 más alto de las situaciones es 3,8 ms, frente a 10 ms.
- **La causa no está atribuida.** El residuo es de 50 a 90 µs. Seis pares no distinguen efectos menores de una décima de milisegundo; una comparación de `49ec349` (antes de #41) con `f6d94cd`, o un perfil de muestreo, lo cerraría.

### 13.8 Límites

1. Un equipo, una aplicación sintética (dos etiquetas que cambian, un velo), una GPU y un modelo float16. No es la pantalla de un usuario ni la distribución de aplicaciones reales.
2. **Sin accesibilidad.** La configuración de origen no tiene `[models] accessibility = "uia"`. §10.4 y las sesiones S4 la usaron; la comparación con §10.4 no es directa por esa razón. Ver 13.9.
3. En `full`, B1 tiene una o dos observaciones cálidas por proceso: sus medianas y su p95 por proceso no son estables, y la diferencia de `full` no es una ventaja demostrada.
4. Los intervalos son de procesos (secuencias), nunca de fotogramas. No se publica el p95 por proceso de `full`, y no se publica p99: el número de muestras por proceso no alcanza para colas.
5. El acuerdo es entre dos variantes que observan pantallas que no coinciden en el tiempo; no es precisión, y no hay control B0 frente a B0.
6. El registro de observación no guarda cajas. La pérdida de `incr2` se atribuyó en 13.10 con un compilado de diagnóstico local, no con el registro.
7. El ensayo previo queda fuera de todas las cifras.
8. Por diseño del protocolo, las observaciones inactivas de B1 no entran en la latencia; su coste se reporta aparte (13.4).

### 13.9 Pendiente y decisiones para el responsable

1. ~~Repetir B0 y B1 con `[models] accessibility = "uia"`~~ Hecho en 1.0.57 (13.11).
2. ~~Investigar la pérdida de elementos con dos regiones~~ Hecho en 1.0.57 (13.10): comportamiento del detector sobre recortes.
3. **Control B0 frente a B0**, para separar el acuerdo de la variante del efecto de la animación.
4. **Explicar la captura de `full`** (13.6, punto 3) y **atribuir el residuo de PERF-01** (13.7).
5. ~~Cosmético: `region_area_px` como `-0.0`~~ Corregido en 1.0.57 (13.12). Lo que sigue pendiente está en 13.13.

### 13.10 Pérdida de elementos con dos regiones: qué falta y por qué (1.0.57) [V]

Cierra el pendiente 2 de 13.9. **No es un defecto del Algoritmo 1: es el comportamiento del detector sobre recortes**, un coste de calidad de la incrementalidad que el paper pidió medir. El algoritmo no cambia (una política adaptativa entre refresco completo y regional es INC-07, no autorizada).

**Método.** Con un compilado local (commit `875ee5b` más un parche de diagnóstico **no versionado**, activado por una variable de entorno, que escribe en stderr rol y caja de cada control y cada palabra, **nunca** texto ni píxeles; el parche se guardó fuera del repositorio) se observó la aplicación de prueba con `--animate 30 --animate-labels 2` y las configuraciones B0 y B1 de 13.1 (sin accesibilidad), 8 observaciones cada una. Se comparó, observación a observación, la evidencia que publica B1 con la de B0 (IoU ≥ 0,5). No se guardó ningún fotograma. Las etiquetas de la aplicación son sintéticas.

**Qué regiones planifica el motor.** Tras la primera observación (refresco completo), la segunda planifica un recorte de 318×160 px (la etiqueta superior, con su halo) y uno grande de 1410×500 px (su causa no se investigó). Desde la tercera, **dos recortes casi estables: la etiqueta superior (de 258 a 310 × 160 px) y la esquina inferior derecha (de 220 a 224 × 151 px)**, que cubre la bandeja del sistema de la barra de tareas (de 1032 a 1080 de alto). El cierre del halo (Algoritmo 1) funciona: en las observaciones 2 a 8, de 18 a 68 cajas de la base solapan una región y **ninguna** queda cortada por ella (0 de 18 a 68; todas están contenidas dentro de la región, con 1 px de tolerancia de redondeo). `merged` descarta lo que debe: lo que cae en la región.

**Qué falta.** Régimen estable (observaciones 3 a 8, las seis con el mismo patrón):

| | B0 (refresco completo) | B1 (dos recortes) |
|---|---|---|
| Controles publicados como evidencia | 74 a 75 | 58 a 59 |
| Palabras | 33 a 34 | 35 a 37 |
| Controles de B0 sin pareja en B1 (IoU ≥ 0,5) | | 19 a 21 |
| de ellos, dentro de las regiones actuales | | 15 a 17 (14 son iconos pequeños de la bandeja, de 1707 a 1920 × 1032 a 1080) |
| de ellos, fuera de las regiones actuales | | 4 (los perdió el recorte grande de la observación 2 y no se rehacen) |
| Controles de B1 sin pareja en B0 | | 5 |
| Controles que devuelven **los dos recortes juntos** | | **3 a 4** (frente a las 18 a 30 cajas, controles y palabras, de la base que los solapaban), 3 de ellos con una caja que toca 3 o 4 lados de su propia región |

Es decir, el recorte de la bandeja, donde el refresco completo ve unos 14 iconos pequeños y un reloj, devuelve **una caja del tamaño del recorte entero** (`1696,929,1920,1079` y `1696,1032,1920,1079`), y el de la etiqueta superior, otra de `24,0,334,160` (la propia región). Esas cajas pasan a la evidencia en lugar de los iconos. En la observación siguiente son soportes del plan, y el cierre del halo no agranda la región (la caja ya es la región). La pérdida **es estable, no se acumula**: cada observación vuelve a analizar las mismas regiones y obtiene lo mismo.

**Por qué: no es el contexto sólo, es también la escala.** Con la aplicación de prueba quieta (sin animación) se hizo un refresco completo y, **sobre el mismo fotograma**, se llamó al detector con una sola región cada vez (el motor, con el parche de diagnóstico, lo hace en el mismo proceso, sin guardar nada). Referencia: los controles del fotograma completo, cuya caja cae entera dentro de la región. «Pareja» es IoU ≥ 0,5.

| Región (x0,y0,x1,y1) | Tamaño | Controles del recorte | Controles del fotograma completo dentro | Con pareja |
|---|---|---|---|---|
| 0,0,1280,720 | 1280×720 | 10 | 9 | 6 |
| 640,360,1920,1080 | 1280×720 | 47 | 46 | 33 |
| 0,360,1280,1080 | 1280×720 | 37 | 40 | 29 |
| 960,540,1920,1080 | 960×540 | 32 | 31 | 23 |
| 1248,736,1920,1080 | 672×344 | 19 | 19 | 13 |
| 1472,832,1920,1080 | 448×248 | 11 | 15 | 6 |
| 1696,928,1920,1079 | 224×151 | 6 | 11 | 4 |
| 24,0,334,160 | 310×160 | 1 | 0 | 0 |
| 0,0,1920,1080 | 1920×1080 | 76 | 76 | 76 |

Dos hechos, ambos medidos. (1) Incluso un recorte de 1280×720, con mucho contexto, sólo reproduce entre 6 de 9 y 33 de 46 (≈ 67 a 72 %) de lo que ve el fotograma completo: la entrada es siempre de 1280×1280 (§13.3), el recorte entra sin ampliar ni reducir (escala 1,0) y el fotograma de 1920×1080 se reduce a 2/3 antes de entrar. **El detector ve otra escala**, y a otra escala encuentra otros iconos. (2) Cuanto más pequeño el recorte, peor: de 15 a 6 en 448×248 y 11 a 4 en 224×151, donde además aparece la caja del recorte entero. El mismo efecto que en §10.2 (las entradas más pequeñas no encuentran lo mismo que la de 1280×1280), ahora entre el recorte y el fotograma completo. Esto no separa aún cuánto es escala y cuánto es falta de contexto: un recorte de 1280×720 no tiene poco contexto, y aun así pierde la tercera parte, lo que apunta a la escala; confirmarlo exigiría alimentar el mismo recorte a las dos escalas, y esa variante no existe en el detector.

**Qué se descartó.** (a) Cajas cortadas por el borde de una región: 0 (arriba). (b) Descarte indebido de `merged`: sólo descarta lo que solapa una región, y todo eso está contenido en ella. (c) Fusión de los dos recortes o duplicados: no hay; el `dedup` del detector elimina los exactos. (d) Evidencia fuera de las regiones: se conserva tal cual.

**Lectura.** B1 con dos regiones pierde unos 20 de 75 controles (≈ 26 %) por el cambio de escala y de contexto del detector, no por un fallo del plan. Es un coste de calidad medido de la incrementalidad con este modelo (§10.2), que se suma al de latencia de 13.6. **Recomendación (INC-07, no autorizada esta noche):** una política adaptativa que use el refresco completo cuando el área o el número de regiones supere un umbral (con dos regiones B1 ya no ahorra tiempo, 13.6 punto 2, y además pierde calidad), y/o descartar o desconfiar de una detección cuya caja ocupa casi todo su recorte. Ninguna de las dos se implementa aquí.

**Límites.** Un equipo, una aplicación sintética, un modelo; las regiones de la bandeja dependen de dónde esté la barra de tareas; la comparación usa IoU ≥ 0,5 contra B0, que no es la verdad (§9.5); las cifras del régimen estable son de una sola corrida de 8 observaciones, con el mismo patrón en las seis últimas; la tabla de recortes quietos es de una sola captura y de una sola pantalla.

### 13.11 B0 frente a B1 con `[models] accessibility = "uia"` (1.0.57) [V]

Cierra el pendiente 1 de 13.9. Mismo protocolo (§9.9), mismas cifras de procesos que 13.1 a 13.5 y **una sola diferencia con aquella corrida: `accessibility = "uia"` en `[models]` en las dos copias** (se añade tras `ocr_language`; B0 y B1 siguen difiriendo sólo en `experimental_full_frame`, la única diferencia declarada). **Entrada: ninguna.**

| Condición | Valor |
|---|---|
| Identificador | `inc02-b0b1-uia-20261015`; semilla del bootstrap 20261015; 10 000 remuestreos |
| Commit medido | `875ee5b74efe38e347fffe614ef3c10ce7a109e4` (rama `fix/inc02-region-loss`, árbol limpio): `origin/main` (`2b127a7`, que no cambia código respecto de `f6d94cd`) más la corrección de `region_area_px` de 13.12. El parche de diagnóstico de 13.10 **no** forma parte de este binario |
| Compilación | `cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia`; `ommatix.exe` SHA-256 `c7c3bf394266d3e6a3920e67e373f4de779d19de25e79f8294baf43f00f8cc1f` |
| Aplicación de prueba | `5050596d46bc300fdc9c31d6835af00d470dd5fb9ca12b464d1e66edec29a942` (la binaria de 13.1 y §12, copiada del árbol principal: al recompilarla en el árbol de trabajo salió otro hash, posiblemente por las rutas, sin verificar) |
| Configuración B1 / B0 (SHA-256) | `b47c22cee67c22b3e0814ca4757230731aa6354feb14e63b946bd942291219d3` / `907612b61a5d3e3f0b88e435c08e508f4422e0968d41254f7b3258c2eacba520` (las de 13.1 más `accessibility = "uia"`) |
| Procesos | 40 por variante y caso (240 en total, 20 observaciones cada uno), en seis trozos de 20; 0 descartados, 0 fallidos, 0 errores en los trozos válidos |
| Animación | `--animate 30` (las etiquetas) y 250 ms (el velo), como en 13.1 |
| Fecha | 15 de octubre de 2026, de 06:16 a 06:30 (hora local) |

**Trozos descartados y repetidos, declarado.** La primera tanda se lanzó con un guion en segundo plano y la herramienta descartó 4 de los 6 trozos (todos sus procesos) porque la aplicación de prueba no tenía el primer plano; los 2 trozos válidos de esa tanda (`incr1` a y `full` a) también se descartaron de las cifras y se repitieron, igual que los 4 anteriores, **en primer plano**, como en 13.1, para que los seis trozos tuvieran las mismas condiciones. Los informes de la tanda descartada están aparte (`b0b1uia_discarded`) y **no entran en ninguna cifra**. Un lanzamiento previo se rechazó antes de empezar (ruta mal convertida por el intérprete de órdenes) sin ejecutar nada. La herramienta hizo lo que debe: descartó lo que no cumplía el primer plano.

**Latencia** (ms de `total`; IC 95 % bootstrap sobre procesos):

| Caso | Variante | n cálidas | Mediana [IC 95 %] | p95 [IC 95 %] | Inferencias | Elementos publicados (mediana / p95) |
|---|---|---|---|---|---|---|
| `incr1` | B0 | 720 | 123,4 [122,4; 124,1] | 135,6 [134,1; 137,4] | 1 | 80 / 80 |
| `incr1` | B1 | 720 | 123,3 [123,0; 123,6] | 132,9 [130,7; 135,9] | 1 | 79 / 81 |
| `incr2` | B0 | 720 | 123,4 [122,5; 124,3] | 137,0 [135,9; 139,2] | 1 | 80 / 81 |
| `incr2` | B1 | 720 | 166,2 [165,8; 166,4] | 186,3 [185,6; 186,9] | 2 | 62 / 81 |
| `full` | B0 | 720 | 98,1 [97,1; 98,9] | 131,3 [127,0; 141,8] | 1 | 77 / 78 |
| `full` | B1 | 65 | 92,4 [90,3; 93,8] | 102,5 [97,4; 103,1] | 1 | 75 / 78 |

**Diferencia apareada por proceso (B1 − B0).**

| Caso | Δ de la mediana (ms) [IC 95 %] | Δ del p95 por proceso (ms) [IC 95 %] | Procesos | Dice [IC 95 %] |
|---|---|---|---|---|
| `incr1` | +2,4 [+0,8; +4,1] | −7,5 [−15,1; −2,2] | 40 | 0,966 [0,966; 0,967] |
| `incr2` | +46,6 [+44,6; +48,7] | +48,2 [+45,3; +51,0] | 40 | 0,833 [0,831; 0,834] |
| `full` | −5,4 [−7,3; −3,6] | no interpretable: 1 o 2 observaciones de B1 por proceso | 36 (4 excluidos) | 0,978 [0,977; 0,980] |

**Tramos** (mediana por proceso en B0 y B1, y Δ B1 − B0 con su IC 95 %; ms):

| Caso | Tramo | B0 | B1 | Δ [IC 95 %] |
|---|---|---|---|---|
| `incr1` | detección | 81,2 | 71,9 | −9,4 [−9,5; −9,3] |
| `incr1` | OCR | 36,3 | 3,7 | −32,5 [−32,6; −32,3] |
| `incr1` | **accesibilidad** | **116,9** | **116,7** | +2,6 [+0,9; +4,3] |
| `incr2` | detección | 81,1 | 134,3 | +53,1 [+53,0; +53,3] |
| `incr2` | OCR | 36,9 | 8,1 | −28,9 [−29,0; −28,7] |
| `incr2` | **accesibilidad** | **116,6** | **159,4** | **+46,2 [+44,2; +48,4]** |
| `full` | captura | 12,5 | 10,0 | −3,5 [−5,0; −2,0] |
| `full` | planificación | 1,8 | 0,26 | −1,5 [−1,6; −1,5] |
| `full` | detección | 80,2 | 80,5 | +0,3 [+0,1; +0,6] |
| `full` | accesibilidad | 54,1 | 54,0 | +0,07 [−0,6; +0,8] |

Observaciones inactivas de B1 (aparte): `incr1` 0, `incr2` 0, `full` 655 de 720 (mediana 14,2 ms, p95 20,7 ms). Primera observación (control, 120 por variante): B0 y B1 difieren en menos de 1,5 ms (`incr1` 131,5 y 133,0; `incr2` 131,5 y 132,0; `full` 87,6 y 87,7).

**Lectura.**

1. **Con accesibilidad la ventaja de una región desaparece.** En `incr1` B1 ya no es más rápida: +2,4 ms de mediana (frente a −18,6 ms sin accesibilidad) y −7,5 ms de p95. La lectura de UI Automation tarda lo mismo con una región que con el fotograma completo (≈ 117 ms en las dos variantes) y es el tramo más largo, por encima de la detección (72 y 81 ms): el total lo fija ella. El ahorro del detector (−9,4 ms) queda tapado.
2. **Con dos regiones la accesibilidad crece con las regiones** (117 → 159 ms, +46 ms) igual que la detección (81 → 134 ms): cada región es una lectura de UIA y una inferencia. B1 pasa de 123 a 166 ms de mediana (+46,6 ms).
3. **Comparación con §10.** Estas cifras se parecen a las de §10.5 (animación de 30 ms, float16, lectura de UIA de 121 ms con una región y 171 con dos, p95 de dos regiones 199,5 ms), y **no** a las de §10.4 (animación de 100 ms, p95 de dos regiones 153,0 ms). La animación de 30 ms de este protocolo repinta la aplicación de prueba a ≈ 33 fotogramas por segundo en la misma GPU, y la lectura de accesibilidad espera a la GPU saturada por float16 (§10.5). No se ha separado aquí esa causa de otras (otras ventanas abiertas en el equipo); la tabla de tramos sólo establece que la lectura es de ≈ 117 ms en B0, por encima de los 58 a 73 ms de §10.4.
4. **RNF-05: sin cambio de estado.** En esta condición (con `uia`, float16, animación de 30 ms) el p95 incremental con dos regiones es de **186,3 ms** [185,6; 186,9]: **no cumple 150 ms**. Con una región el p95 de B1 es de 132,9 ms [130,7; 135,9] (cumple), igual que el de B0 (135,6 ms), así que sin ventaja. No se declara RNF-05 cumplido ni incumplido: el estado lo fija el responsable.
5. **Elementos publicados.** Con dos regiones, B1 publica una mediana de 62 frente a 80 (sin accesibilidad: 67 frente a 85, 13.3). La pérdida (≈ 22 %) es del orden de la de 13.10; se espera la misma causa, pero no se investigó caja a caja con `uia`, así que no está probado.
6. **Refresco completo (control).** Detección igual (80 ms); la diferencia de −5,4 ms viene de la captura (−3,5 ms) y la planificación (−1,5 ms), como en 13.6 punto 3. B1 analizó sólo 65 observaciones. No es una ventaja de B1.

**Límites.** Los de 13.8, salvo el 2 (ahora hay accesibilidad). Además: los trozos tienen variaciones entre sí (la mediana de B0 en `incr1` fue 112,7 ms en un trozo y 124,4 ms en otro; la lectura de UIA depende de lo que haga el resto del escritorio); el IC de procesos las recoge pero no las explica. El p95 de B0 en `full` (131 ms) viene de colas de la lectura de UIA.

### 13.12 Cosmético: `region_area_px` de una observación inactiva (1.0.57) [V]

La suma de ningún área es `-0.0` en Rust (`Iterator::sum` de `f64` arranca en `-0.0`), así que el registro de una observación inactiva escribía `"region_area_px": -0.0`. `0.0 == -0.0`, por eso la prueba existente no lo veía. Se corrige en `plan_record` (`src/application/src/engine/observe.rs`): la suma arranca en `+0.0`. Prueba de regresión `an_idle_observation_records_its_region_area_as_a_positive_zero` (`src/application/src/engine/tests.rs`), que comprueba el signo y **falla sin la corrección** (se verificó). Ninguna estadística cambia; las medidas de 13.1 a 13.5 conservan el `-0.0` en sus registros de origen.

### 13.13 Pendiente tras 1.0.57

1. **INC-07 (autorizado e implementado en 1.0.65, §16):** política adaptativa entre refresco completo y regional. Las evidencias que la motivan: con dos regiones B1 es más lenta (13.3, 13.11) y publica menos elementos (13.10); con accesibilidad ni siquiera una región ahorra (13.11).
2. Control B0 frente a B0 (acuerdo), captura de `full` y residuo de PERF-01 (13.9, puntos 3 y 4): siguen abiertos.
3. Investigar con `uia` la pérdida de elementos (13.11, punto 5) y separar la causa de la lectura lenta de UIA (13.11, punto 3: GPU saturada frente a ventanas abiertas).
4. El responsable decide el estado de RNF-05 (con `uia` y animación de 30 ms el p95 con dos regiones es 186,3 ms).

## 14. PERF-03 / RNF-06 con `uia` sobre `main`: intento bloqueado (1.0.59)

**Qué se pretendía.** La repetición de §12 que pidió el responsable: `soak.py --minutes 120` con `[models] accessibility = "uia"` sobre el código actual de `main` (`46402356`, árbol limpio), sin entrada y sin umbral. **No hay cifras.** Ni el ensayo de un minuto capturó la pantalla, así que la corrida de 120 minutos no empezó.

**Condiciones y desviaciones [V].**

| Condición | Valor |
|---|---|
| Commit | `46402356fcd881517fa0633796715fd594ed3cb0` (`origin/main`), árbol limpio |
| Compilación | `cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia` y `cargo build --release --locked` en `tools/s4/testapp`; `rustc 1.99.0` |
| `ommatix.exe` (SHA-256) | `3832e639235ca9a7150b34548db82a7fc40ef41ecc555201cc79c1be0465c00c` |
| `ommatix-testapp.exe` (SHA-256) | `5050596d46bc300fdc9c31d6835af00d470dd5fb9ca12b464d1e66edec29a942` (la misma que §12) |
| Configuración del operador (SHA-256) | `94af2154c74ba93da7500c3c8273ea05654be2b0ee9293e42d560375fecd5298`, sin cambios; manifiesto `f8b04be9…`, el fijado, sin cambios |
| Copia con `uia` (SHA-256) | `bfde539ffd8a28aa3185b4e3cd6eb58a18e676ac49116ccd027b449813ce7e7c`, fuera del repositorio. Sólo difiere de la del operador en `accessibility = "uia"` bajo `[models]` y en que las tres rutas de `[models]` son absolutas |
| **Desviación 1: auxiliar de primer plano** | `soakuia_activate.py`, fuera del repositorio, SHA-256 `5102de55589c552181dd904314a8c8a99624a50148f4da67e9976b29dea23245`. Trae al frente sólo la ventana principal del proceso `ommatix-testapp.exe` que arranca `soak.py`, con `activate_window` de `tools/s4/e2e_driver.py` **sin cambios** (SHA-256 `768e77e3aaa065c72aef4cf78ba65a41787f13df624b4f414c55813d3997202d`, igual que en `main`). Ese procedimiento usa `AttachThreadInput`, `ShowWindow`, `SetWindowPos` (sólo orden Z), `BringWindowToTop`, `SetForegroundWindow` y `SetActiveWindow`; no envía teclas ni clics. `soak.py` no se cambió y sigue comprobando el primer plano |
| **Desviación 2** | `--foreground-wait 30` (por defecto, 15), para que la espera del auxiliar quepa en la de la herramienta |
| Equipo | AMD Ryzen 7 5700X; Windows 11, build 26300.9550; dos monitores (la salida 0 es la principal, 1920×1080 en 0,0); escala 100 %; plan «Alto rendimiento»; protector sin programa; 44 procesos `python.exe` de otros proyectos, como en §12 |

**Secuencia [V].**

1. *Ensayo de un minuto sin el auxiliar* (`--minutes 1 --interval 2`): `refused` tras 15 s, con «the test application is not in the foreground and the tool does not use input to put it there». La aplicación de prueba no pasó sola al primer plano: lo tenía `orca.exe`, y desde `8551768` la aplicación no pide el primer plano. Sin entrada; el motor no arrancó.
2. *Ensayo de un minuto con el auxiliar*: a los 0,72 s el auxiliar vio la ventana, y `attach_foreground` la trajo al frente en el primer intento (primer plano comprobado en el proceso de la aplicación de prueba; salida 0). `soak.py` pasó su comprobación, arrancó el motor (`ommatix serve --source dxgi`) y terminó `observe_errors` tras tres observaciones, las tres con `observed = false`.
3. *Diagnóstico de sólo observación* (`soakuia_diag.py`: misma aplicación, mismo auxiliar y mismo `ommatix serve`, tres `observe_screen` por configuración): **3 de 3** respuestas `CAPTURE_UNAVAILABLE` («the screen could not be captured», reintentable) con la copia con `uia`, y **3 de 3** con la del operador (`accessibility = "none"`). El fallo no viene de la accesibilidad ni del auxiliar. El motor no registra en `stderr` el motivo dentro del adaptador de captura.
4. *Error propio, declarado.* Un error de orden de comandos (una variable sin definir) lanzó un ensayo sin la ruta de la configuración. La herramienta lo rechazó antes de arrancar (`configuration file does not exist`) y dejó su informe en la raíz de Git (`E:\Git`), fuera del repositorio. No contiene ningún dato medido.

**Hallazgo en la herramienta (no corregido).** `soak.py` anota el código de error como `page.get("code")`, pero el contrato lo devuelve en `error.code`. Por eso `error_code` queda nulo en `samples` aunque la respuesta sea `CAPTURE_UNAVAILABLE`. `tools/s3/mcp_driver.py` tiene el mismo patrón en sus mensajes de error de `observe_screen`. Su corrección, con prueba de regresión, la decide el responsable.

**Causa probable (no verificada).** Según el responsable, restauró el apagado de pantalla en corriente alterna a 15 minutos hacia las 07:00, después de las corridas B0/B1. Con nadie en el equipo, la pantalla se habría apagado hacia las 07:15. DXGI no captura una pantalla apagada, y volver a «nunca» a las 07:40 no la despierta. Lo que lo apoya: las corridas B0/B1 con `uia` captaron sin fallos hasta las 06:32 (20 procesos sin fallos en cada trozo, de 06:19 a 06:32), y los fallos empiezan después. No está comprobado: la herramienta no lee el estado de la pantalla en cada observación.

**Lo que se descartó [V].** La build (26300.9550) y los dos monitores son los de §12 y §13. El registro del sistema no tiene eventos de controlador de pantalla de las últimas seis horas. Las actualizaciones de Windows de las 04:03 a las 06:00 (aplicaciones de la tienda y KB5007651) son anteriores a las corridas B0/B1 que sí captaron. No se paró ningún proceso y no se intentó despertar la pantalla.

**Plan de energía, en lectura [V].** A las 07:47 (hora local), el plan activo («Alto rendimiento») tenía el apagado de pantalla en corriente alterna en 0x00000384 s (900 s, 15 minutos) y en corriente continua en 0x00000258 s (600 s); el suspendido en corriente alterna era 0 (nunca). El cambio a «nunca» que el responsable comunicó a las 07:40 **no aparece** en esa lectura. Esta sección no cambia ningún ajuste.

**Qué queda.** La repetición con `uia` sobre `main` (decisión 6 de 06 §6.7) sigue sin cifras. Para repetirla hacen falta la pantalla encendida durante toda la sesión, comprobada antes, un ensayo de un minuto que capture y después la corrida de 120 minutos sin umbral. PERF-03 mantiene el estado de 1.0.55 (`EJECUTADO_SUPERVISADO_PARCIAL`), y RNF-06 sigue sin veredicto. Se repitió sobre `745caff` en la §15 (1.0.66).

## 15. PERF-03 / RNF-06 con `uia` sobre `745caff`: corrida completa de 120 minutos (1.0.66)

**Qué mide.** La repetición de §12 que pidió el responsable, con `[models] accessibility = "uia"` sobre el código de `main` en `745caff`: `tools/perf/soak.py --minutes 120`, sin entrada de ningún tipo (sólo `observe_screen` y `get_engine_metrics`), con la aplicación de prueba propia (`tools/s4/testapp`, `--animate 100 --animate-labels 2`) y una muestra cada 5 s. Antes, un ensayo de un minuto con el mismo montaje: `completed`, 30 de 30 observaciones. Los criterios son los del responsable ([capítulo 09 §9.7](09_EVALUACION_EXPERIMENTAL.md)): conjunto de trabajo de no más de 10 MiB/h, y manejadores sin pendiente positiva, medidos después de los primeros 10 minutos.

**Condiciones [V].**

| Condición | Valor |
|---|---|
| Commit medido | `745caff478cb5710f0c10d222a85632162079b55` (`origin/main` entonces), árbol limpio al empezar (`tree_clean`: sí). `main` avanzó después; esta sección no cambia código |
| `ommatix.exe` (SHA-256) | `a66f6557022e00945985dfc039b8c791cc2cfc8240f81effc662a0bdadc43eb9` (`--release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia`; `rustc 1.99.0`; sin avisos) |
| `ommatix-testapp.exe` (SHA-256) | `901a3595d05b0549b42b41fb23a1a10db5253692d4443133274496845e36bcd2` (`tools/s4/testapp`, `--release --locked`). Su código no ha cambiado desde `fe35d80`, pero el hash no es el de §12 (`5050596d…`); probablemente por la ruta de compilación, sin verificar |
| Configuración con `uia` (SHA-256) | `bfde539ffd8a28aa3185b4e3cd6eb58a18e676ac49116ccd027b449813ce7e7c`, fuera del repositorio (la misma que en §14). Sólo difiere de la del operador (`94af2154…`, sin cambios) en `accessibility = "uia"` bajo `[models]` y en rutas absolutas. `config_problem` = None; `dry_run`, actuación desactivada, `allowed_applications = []` |
| Manifiesto `models\manifest.rnf05.json` (SHA-256) | `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`, el fijado |
| Herramienta `tools/perf/soak.py` (SHA-256) | `20dc342d4cf66ad51961bd066bd81d62be0925d367b1a455bee9f08361a7d23b` en `745caff`, anterior a `650ce27`, que añade los criterios por defecto. Los criterios de esta sección se aplicaron al JSON con la misma definición |
| Auxiliar de primer plano | `soakuia_activate.py` (`5102de55…`) y `tools/s4/e2e_driver.py` (`768e77e3…`), sin cambios. Método en ambas ejecuciones: `already_foreground`; la ventana apareció a los 0,86 s (corrida) y a los 9,7 s (ensayo) |
| Plan de energía | «Alto rendimiento». Apagado de pantalla en corriente alterna: `0x00000000` (nunca) antes de empezar y después de la corrida (`powercfg /query`); en corriente continua, 600 s. Este trabajo no cambió ningún ajuste |
| Sistema | Windows 11 Pro, `Windows-11-10.0.26300-SP0`; Python 3.12.2. Monitores y escala no releídos en esta sesión |
| Ensayo previo | Un minuto con `--interval 2`: `completed`, 30 de 30 observaciones, GPU disponible, sin errores |
| Inicio y fin (hora local) | Inicio 08:57:43; último muestreo a los 7195 s (10:57:38) |
| Carga de otras sesiones | No se muestreó durante la corrida. Instantánea al terminar: 44 procesos `python.exe` de otros proyectos, ningún `cargo` ni `rustc` en marcha |
| Terminación (`ended`) | `completed` |

**Observaciones y acciones [V].** 1440 ticks de 5 s; 1440 observados; 0 pérdidas de primer plano; 0 errores de observación; 0 latencias descartadas. `delivered_actions` y `rejected_actions`, máximo 0. Época del snapshot 1 en todas las muestras; ninguna observación salió degradada. `pending_regions` fue 0 en las 1440 muestras: como en §12, ninguna observación falló, así que la cola no se pone a prueba.

**Recursos [V].** Del proceso `ommatix.exe`, tras los primeros 10 minutos (n = 1320). La pendiente es de mínimos cuadrados sobre esas muestras; las medianas son del primer y del último décimo.

| Recurso | Mínimo | Máximo | Mediana, primer décimo | Mediana, último décimo | Pendiente por hora | Criterio del responsable |
|---|---|---|---|---|---|---|
| Conjunto de trabajo (MiB) | 305,0 | 310,7 | 306,8 | 309,0 | +1,25 | no más de 10 MiB/h: **cumple** |
| Manejadores | 1008 | 1009 | 1008 | 1009 | +0,81 | pendiente no positiva: **no cumple** |
| Bytes privados (MiB) | 1463,1 | 1468,1 | 1464,7 | 1467,2 | +1,10 | sin criterio (referencia) |
| Memoria dedicada de GPU (MiB) | 1203,1 | 1203,1 | 1203,1 | 1203,1 | 0 | sin criterio |

**Manejadores, serie completa [V].** En el primer minuto el recuento sube de 285 a entre 1006 y 1017, y hasta el segundo 470 oscila entre 1008 y 1012. Desde el segundo 470 hasta el 4165 (minuto 69) vale 1008. En el segundo 4165 pasa a 1009 y se queda ahí hasta el final: 607 de las 1320 muestras posteriores al calentamiento valen 1009, y 713 valen 1008. No hay tendencia, sólo un paso de +1. La herramienta no atribuye manejadores, así que no se sabe qué lo creó. En §12, sin `uia` y en otro commit, el recuento fue 947 durante toda la corrida.

**Criterios del responsable [V].** Conjunto de trabajo: pendiente de +1,25 MiB/h frente a un máximo de 10 MiB/h: **cumple**. Manejadores: la pendiente, +0,81 por hora, es positiva. El capítulo 09 §9.7 dice que ese criterio es estricto a propósito («cualquier pendiente positiva lo incumple»), así que **no cumple**. La regla se aplica sin cambios; si el responsable quiere tolerancia, la fija antes de la siguiente corrida, no después.

**Latencia, tras el minuto 10 [V].** Milisegundos, sobre las 1320 observaciones posteriores al calentamiento. Ida y vuelta: p50 141,4, p95 164,9, p99 180,8, máximo 3424,1. Total del motor: p50 141,1, p95 164,6, p99 180,4, máximo 3423,8. Una sola observación superó 1 s, en los minutos 10 a 20. Por ventanas de 10 minutos (ida y vuelta: p50 / p95 / p99 / máximo; motor: p99 / máximo):

| Ventana (min) | n | Ida y vuelta: p50 / p95 / p99 / máx | Motor: p99 / máx |
|---|---|---|---|
| 0–10 | 120 | 143,4 / 174,6 / 190,1 / 745,1 | 180,7 / 189,7 |
| 10–20 | 120 | 142,1 / 182,7 / 576,5 / 3424,1 | 576,1 / 3423,8 |
| 20–30 | 120 | 142,5 / 164,4 / 172,4 / 172,7 | 172,0 / 172,3 |
| 30–40 | 120 | 142,4 / 174,2 / 182,4 / 182,7 | 182,0 / 182,3 |
| 40–50 | 120 | 143,2 / 168,3 / 428,0 / 618,2 | 427,7 / 617,7 |
| 50–60 | 120 | 141,3 / 165,4 / 170,1 / 173,7 | 169,8 / 173,4 |
| 60–70 | 120 | 141,2 / 165,8 / 168,0 / 168,0 | 167,7 / 167,7 |
| 70–80 | 120 | 141,0 / 163,3 / 166,8 / 168,4 | 166,5 / 168,2 |
| 80–90 | 120 | 140,1 / 162,1 / 165,2 / 169,0 | 165,0 / 168,8 |
| 90–100 | 120 | 140,6 / 160,2 / 166,7 / 169,5 | 166,5 / 169,3 |
| 100–110 | 120 | 141,2 / 164,3 / 165,5 / 167,0 | 165,2 / 166,8 |
| 110–120 | 120 | 140,3 / 154,7 / 169,9 / 172,2 | 169,6 / 172,0 |

Hay dos colas de latencia (minutos 10 a 20 y 40 a 50) sin causa identificada: el informe no desglosa etapas ni carga. Frente a §12, la latencia tras el minuto 10 (motor, p95 164,6 ms, frente a 150,8 ms) es algo mayor, pero no es comparable directamente: cambian el commit, el código y el binario. Esta medición no cambia el estado de RNF-05 (§10).

**Lectura.** La corrida terminó como `completed`: 120 minutos, 1440 de 1440 observaciones, sin pérdidas de primer plano ni errores, así que no es una corrida truncada. Con los criterios del responsable, el **conjunto de trabajo cumple** (+1,25 MiB/h frente a 10). **Los manejadores no cumplen**: la pendiente es de +0,81 por hora, positiva, por un único manejador que se añadió en el minuto 69 y se mantuvo hasta el final. Por tanto RNF-06 no se cumple en esta corrida: la casilla 7 de §6.7 sigue sin marcar, y PERF-03 mantiene `EJECUTADO_SUPERVISADO_PARCIAL`. Es una sola corrida; no hay variación entre corridas.

**Qué queda abierto.**
1. **Decisión del responsable.** Si el paso de un manejador cuenta como crecimiento (la regla dice que sí), o si fija una tolerancia antes de la siguiente corrida. No se fija después de ver este resultado.
2. **Origen del manejador.** La herramienta no atribuye manejadores; identificarlo requiere una corrida instrumentada, que no se ha hecho.
3. **Cola.** Sin cota (decisión 9) y sin prueba con observaciones fallidas.
4. **Carga.** No se muestreó la carga de otras sesiones durante la corrida.
5. **Latencia.** Dos colas (minutos 10 a 20 y 40 a 50) sin causa identificada.

**Reproducir.** Desde la raíz del repositorio en `745caff` o posterior, con los dos ejecutables compilados (`S4_INSTALACION_Y_REVERSION.md` §3), una copia de `%LOCALAPPDATA%\Ommatix\ommatix.rnf05.directml.toml` con `accessibility = "uia"` bajo `[models]`, y la aplicación de prueba al frente de la pantalla (la herramienta no la trae al frente): `python tools/perf/soak.py --config <copia> --minutes 120 --out soak-120.json`. Desde `650ce27` la herramienta juzga los dos criterios por defecto (`criteria`). El informe bruto se guarda fuera del repositorio (ADR-0012).

**Qué queda.** La repetición con `uia` sobre `main` (decisión 6 de 06 §6.7) sigue sin cifras. Para repetirla hacen falta la pantalla encendida durante toda la sesión, comprobada antes, un ensayo de un minuto que capture y después la corrida de 120 minutos sin umbral. PERF-03 mantiene el estado de 1.0.55 (`EJECUTADO_SUPERVISADO_PARCIAL`), y RNF-06 sigue sin veredicto.

## 16. INC-07: política adaptativa entre refresco completo y regional, en vivo con `uia` (1.0.65)

**Qué se midió.** La regla de coste de la [ADR-0030](../adr/0030-adaptive-full-or-regional-plan.md) (`[engine] adaptive_min_regions`, razón `adaptive_cost`) frente a B1 (incremental sin la regla) y B0 (refresco completo forzado), con el protocolo congelado del [§9.9](09_EVALUACION_EXPERIMENTAL.md) y la aplicación de prueba propia, en los mismos tres casos (`incr1`, `incr2`, `full`). **Entrada: ninguna.** Las tres variantes usan el mismo binario, los mismos pesos y la misma configuración; difieren sólo en dos claves de `[engine]`, declaradas con `--allow-difference`: B0 `experimental_full_frame = true`, `adaptive_min_regions = 0`; B1 `experimental_full_frame = false`, `adaptive_min_regions = 0`; AD `experimental_full_frame = false`, `adaptive_min_regions = 2`.

### 16.1 Condiciones [V]

| Condición | Valor |
|---|---|
| Identificador | `inc07-adaptive-uia-20261015`; semilla del bootstrap 20261015; 10 000 remuestreos. Un ensayo previo de un proceso por variante y un caso (`inc07-rehearsal-20261015`) comprobó la orden y el primer plano; **se excluye** de todo lo que sigue |
| Commit | `95aba26eb35bc565622c7ce5dfdc96cdbc873fba` (rama `feat/inc07-adaptive-plan`: la regla y sus pruebas). El binario se compiló antes de reubicar la rama sobre `66ca158` y antes de quitar una aserción de una prueba de configuración; ni la reubicación ni la aserción tocan `src/` de producción, `tools/` ni `Cargo.lock` |
| Compilación | `cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia`; `ommatix.exe` SHA-256 `f70206bd9e7937038bfa3526773652f2aefd8301b83135c56082610bce071db4` |
| Aplicación de prueba | `5050596d46bc300fdc9c31d6835af00d470dd5fb9ca12b464d1e66edec29a942` (la misma que en §12 y §13, copiada del árbol principal) |
| Configuraciones (SHA-256) | B0 `1e728add142a2036cf683415ddff5dce13667b4eee10ab4886dc28fe7e978a96`, B1 `41f8631822a732b04ec4d0cd88fd3def15c60e4d0463120238cac42bc2e59366`, AD `a0d0200acb5f92f7a379694193f70ff6c106f1a6e1dab78d7fb637af9a5780e2`: la configuración de §13.11 (`ommatix.rnf05.directml.toml` con `accessibility = "uia"` y rutas absolutas) más las dos claves de arriba |
| Detector y OCR | Los de §13.1: float16, entrada 1280×1280, DirectML 1.15.4, OCR de Windows es-ES |
| Accesibilidad | **`uia`** en las tres variantes |
| Equipo y captura | El de §13.1 (Ryzen 7 5700X, RX 6700 XT, Windows 11 Pro 26300.9550, plan «Alto rendimiento»); salida 0 (1920×1080), `--source dxgi` |
| Animación | `--animate 30` (etiquetas) y 250 ms (velo), como en §13.1 |
| Procesos | 40 por variante y caso (360 en total, 20 observaciones cada uno), en seis trozos de 20 procesos, intercalados en orden rotatorio contra la misma instancia de la aplicación de prueba. 0 descartados, 0 fallidos, 0 errores |
| Fecha | 15 de octubre de 2026, de 11:03 a 11:26 (hora local) |

**Desviaciones declaradas.** (1) Hay tres variantes en lugar de dos; el apareamiento se declara por pares (`B0:B1`, `B0:AD`, `B1:AD`) y la herramienta no cambia. (2) No se muestreó la carga del equipo durante la corrida. (3) Las cifras de B0 y B1 de esta corrida no son las de §13.11: es otra corrida, con otro commit y otra hora; se comparan las tres variantes de esta corrida entre sí, nunca con las de §13.11 salvo donde se dice.

### 16.2 Latencia y elementos [V]

Milisegundos de `total` de las observaciones cálidas (clase `warm_full` o `warm_regions`); mediana y p95 con su intervalo bootstrap sobre procesos (se remuestrean procesos, nunca fotogramas, y se reúnen sus observaciones); p99 sin intervalo. Elementos: firmas publicadas por observación.

| Caso | Variante | n cálidas | Mediana [IC 95 %] | p95 [IC 95 %] | p99 | Inferencias | Elementos (mediana / p95 / mín.) | Razón del plan |
|---|---|---|---|---|---|---|---|---|
| `incr1` | B0 | 720 | 113,0 [112,4; 115,1] | 135,2 [134,5; 136,2] | 141,5 | 1 | 78 / 79 / 77 | `experimental_baseline` 720 |
| `incr1` | B1 | 720 | 122,5 [122,0; 123,0] | 135,7 [132,4; 137,7] | 153,3 | 1 (715) o 2 (5) | 77 / 78 / 71 | regiones 720 |
| `incr1` | AD | 720 | 122,9 [122,5; 123,4] | 136,3 [133,5; 139,4] | 148,7 | 1 (720) | 77 / 78 / 75 | regiones 715; `adaptive_cost` 5 |
| `incr2` | B0 | 720 | 114,2 [112,7; 118,3] | 138,1 [135,6; 140,0] | 148,0 | 1 | 79 / 88 / 78 | `experimental_baseline` 720 |
| `incr2` | B1 | 720 | 165,9 [165,6; 166,2] | 186,8 [185,5; 187,6] | 198,4 | 2 | 61 / 63 / 61 | regiones 720 |
| `incr2` | AD | 720 | 113,2 [112,4; 118,5] | 137,6 [135,6; 139,5] | 151,7 | 1 | 79 / 79 / 78 | `adaptive_cost` 720 |
| `full` | B0 | 720 | 99,0 [97,3; 99,9] | 131,8 [126,5; 140,9] | 147,5 | 1 | 76 / 77 / 74 | `experimental_baseline` 720 |
| `full` | B1 | 66 | 91,6 [89,9; 94,7] | 97,6 [96,9; 101,4] | 108,5 | 1 | 75 / 77 / 74 | `area_budget` 66 (y 654 inactivas) |
| `full` | AD | 70 | 92,2 [90,0; 93,2] | 100,3 [96,8; 108,0] | 108,3 | 1 | 74 / 77 / 74 | `area_budget` 70 (y 650 inactivas) |

En `full`, B1 y AD sólo analizaron 66 y 70 observaciones (las que siguieron a un cambio del velo): su mediana y su p95 se apoyan en pocos valores y no son una muestra al azar de las de B0 (igual que en §13.3).

**Diferencia apareada por proceso** (mediana de `total` por proceso; TREAT − BASE; media sobre procesos con IC 95 % bootstrap sobre procesos) y acuerdo (Dice medio por proceso: acuerdo entre variantes, no precisión, §9.5):

| Caso | Par (BASE:TREAT) | Δ de la mediana (ms) [IC 95 %] | Procesos | Dice [IC 95 %] |
|---|---|---|---|---|
| `incr1` | B0:B1 | +7,4 [+5,7; +9,0] | 40 | 0,962 [0,958; 0,965] |
| `incr1` | B0:AD | +7,9 [+6,3; +9,4] | 40 | 0,962 [0,959; 0,965] |
| `incr1` | B1:AD | +0,5 [−0,1; +1,0] | 40 | 0,975 [0,971; 0,979] |
| `incr2` | B0:B1 | +50,5 [+48,8; +52,0] | 40 | 0,836 [0,831; 0,840] |
| `incr2` | B0:AD | +1,2 [−1,1; +3,5] | 40 | 0,968 [0,963; 0,972] |
| `incr2` | B1:AD | −49,2 [−51,3; −47,1] | 40 | 0,839 [0,836; 0,842] |
| `full` | B0:B1 | −6,9 [−8,6; −5,3] | 37 | 0,976 [0,974; 0,978] |
| `full` | B0:AD | −7,4 [−9,3; −5,4] | 38 | 0,976 [0,974; 0,978] |
| `full` | B1:AD | −0,7 [−2,9; +1,5] | 35 | 0,976 [0,972; 0,981] |

(En `full` los procesos son los que tienen observaciones cálidas en las dos variantes del par.)

**Tramos** (mediana, por proceso, en ms; orden B0 / B1 / AD): detección, `incr2` 81,2 / 134,2 / 81,2 y `incr1` 81,2 / 71,8 / 71,7; OCR, `incr2` 37,4 / 8,4 / 37,4 y `incr1` 36,8 / 3,7 / 3,7; accesibilidad, `incr2` 107,5 / 159,4 / 107,1 y `incr1` 106,3 / 115,8 / 116,0; planificación, 2,0 en las tres. La lectura de accesibilidad de B1 con dos regiones crece (159 ms frente a 107 ms): con `uia` cada región se vuelve a leer, igual que en §13.11.

### 16.3 Lectura [V]

1. **Dos regiones: la regla quita el sobrecoste.** AD tiene mediana 113,2 ms y p95 137,6 ms [135,6; 139,5], frente a 165,9 ms y 186,8 ms de B1: Δ = −49,2 ms [−51,3; −47,1]. Frente a B0 (refresco completo forzado) no hay diferencia medible (+1,2 ms [−1,1; +3,5]), como se espera: en `incr2` AD **es** el refresco completo, en las 720 observaciones, con la razón `adaptive_cost` y una sola inferencia.
2. **Elementos: AD no pierde los que B1 pierde.** Con dos regiones AD publica una mediana de 79 elementos (mínimo 78), igual que B0 (79, mínimo 78); B1 publica 61 (mínimo 61). El acuerdo con B0 sube de 0,836 (B1) a 0,968 (AD). No se comparó con una anotación: es acuerdo entre variantes (§9.5).
3. **Una región: la regla no la toca, salvo cuando la animación produce dos a la vez.** En `incr1` AD siguió siendo regional en 715 de 720 observaciones y tomó el refresco completo en 5 (`adaptive_cost`, 0,7 %): dos regiones transitorias que B1 resolvió con dos inferencias (5 observaciones con 2 inferencias). La diferencia AD − B1 es +0,5 ms [−0,1; +1,0], sin efecto medible; su p99 baja de 153,3 a 148,7 ms.
4. **Con `uia` una región no ahorra; aquí tampoco fue más rápida.** En `incr1` B1 y AD tardan 7,4 y 7,9 ms más que B0 [IC 95 %: +5,7; +9,0 y +6,3; +9,4]. §13.11 midió +2,4 ms [+0,8; +4,1]. Son dos corridas con el mismo signo: bajo `uia` la región única no ahorra, y esta vez tarda más. La causa no está establecida: el tramo de accesibilidad de B1 y AD es ≈ 10 ms mayor que el de B0 (116 frente a 106 ms) y la detección ahorra ≈ 10 ms, de modo que no queda ahorro. La regla de la ADR-0030 no cubre este caso; ver la ADR (alternativas) y el punto 7.
5. **Refresco completo (control): sin diferencia entre B1 y AD** (−0,7 ms [−2,9; +1,5]); la ventaja de ≈ 7 ms frente a B0 es de la captura y de la planificación, como en §13.6 punto 3, no del algoritmo.
6. **RNF-05 (incremental p95 ≤ 150 ms; *full warm* p95 ≤ 700 ms), leído con el protocolo del §9.9.**
   - *Cifras.* Con AD y `uia`, el p95 de `total` es 136,3 ms [133,5; 139,4] en `incr1`, **137,6 ms [135,6; 139,5] en `incr2`** y 100,3 ms [96,8; 108,0] en el caso `full`; los p99 son 148,7, 151,7 y 108,3 ms. Los tres p95 quedan por debajo de 150 ms (y el *full warm*, muy por debajo de 700 ms). B1 con dos regiones, sin la regla, da 186,8 ms: no cumple. El p99 de `incr2` con AD (151,7 ms) queda apenas por encima de 150 ms; el requisito no fija meta para el p99.
   - *Cómo se logra.* **AD cumple el p95 de dos regiones porque adelanta el refresco completo del Algoritmo 1; la ruta incremental con dos regiones no se ejecuta.** El texto de RNF-05 ([capítulo 03](03_ESPECIFICACION_REQUISITOS.md)) dice «incremental p95 ≤ 150 ms» y no define si «incremental» nombra el escenario (el cambio parcial de la pantalla, que son los casos `incr1` e `incr2`) o la ruta de código (el plan por regiones). Ni el capítulo 03 ni el §9.9 lo precisan. Con la lectura «escenario», la cifra del protocolo cumple; con la lectura «ruta», el requisito sigue sin cumplirse para dos regiones, como en §10.4 y §13.11.
   - *Lo que falta aunque se lea «escenario».* PERF-02 sigue parcial: no hay caso de caché ni de igualdad de modelos (`plan_pruebas.csv`), el p99 no tiene meta, es un equipo, una aplicación sintética y un modelo, la animación de 30 ms es más exigente que la de 100 ms de §10.4 y las cifras sin `uia` de AD no se midieron en esta corrida.
   - *Estado.* **No se cambia el estado de RNF-05** ni su fila en `requisitos_trazabilidad.csv`: la lectura de «incremental» y la aceptación las decide el responsable. Recomendación: leer el requisito como el escenario del protocolo, registrar esta cifra como evidencia y mantener la aceptación por alcance de la ADR-0023 (apartado a) hasta completar PERF-02; si el responsable exige la ruta, la aceptación por alcance sigue siendo el respaldo.
7. **¿Activada por defecto?** Sí, con dos regiones, y la evidencia es esta: (a) en el caso en que decide (`incr2`) la regla cuesta −49,2 ms [−51,3; −47,1] frente a B1 y recupera los ≈ 18 elementos que B1 pierde (61 frente a 79); (b) en el caso de una región no cambia nada medible (+0,5 ms [−0,1; +1,0] y 5 de 720 observaciones); (c) en el control no hay diferencia; (d) no hubo errores ni procesos descartados, y AD no publica menos elementos que B0 con dos regiones. La condición de la autorización (activada por defecto sólo si la medida la sustenta) se cumple para `uia` y para esta GPU; **sin `uia`** el sobrecoste de dos regiones medido en §13.3 (+46 a +51 %) tiene el mismo mecanismo (una inferencia por región), pero esta corrida no lo repitió con la regla. Un detector más rápido sobre recortes cambiaría el umbral; es una clave de configuración. Queda abierto, y no se decide aquí, un umbral de una región cuando `uia` está activo (punto 4): `adaptive_min_regions = 1` es la configuración que el operador puede probar y que esta corrida no midió.

**Límites.** Un equipo, una aplicación sintética, un modelo y la animación de 30 ms; sin carga de CPU muestreada; las cifras de `full` de B1 y AD se apoyan en 66 y 70 observaciones; los intervalos del p95 son bootstrap sobre 40 procesos; el acuerdo es con B0 (que no es la verdad); la comparación con §13.11 es indicativa, no apareada.

**Reproducir.** Desde la raíz del repositorio, con los dos ejecutables compilados y tres configuraciones que difieren sólo en `[engine] experimental_full_frame` y `[engine] adaptive_min_regions` (B0 `true`/`0`; B1 `false`/`0`; AD `false`/`2`): `python tools/perf/measure_latency.py --experiment-id EXPERIMENT --variant B0=EXE,B0.toml --variant B1=EXE,B1.toml --variant AD=EXE,AD.toml --allow-difference engine.experimental_full_frame --allow-difference engine.adaptive_min_regions --paired B0:B1 --paired B0:AD --paired B1:AD --processes 20 --cases CASE --out chunk.json` por caso (`incr1`, `incr2`, `full`) y trozo, y después `--combine`. Los percentiles con intervalo, los elementos publicados y los tramos de este apartado salen de un guion de lectura del informe combinado (no forma parte del repositorio; su método es el descrito arriba).
