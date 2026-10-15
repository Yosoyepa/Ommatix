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
- **Una aplicación, un DPI (100 %) y un tema.** Quedan sin probar otros DPI (IT-07), RSS y VRAM (RNF-06), y un cliente MCP real. DirectML se mide en §9.

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

**Qué se midió.** El mismo ejemplo `core_latency` en `--release`, con los valores por defecto (10 000 muestras por situación en el núcleo, 2 000 en `observe_screen` entero, calentamiento de 200), en **diez pares intercalados**: cinco con la base primero y cinco con la rama primero, para separar el efecto del orden. **Antes**: `origin/main` en `114a020`, que ya incluye INC-03 (vigencia) y la purga de sesión, compilado en su propio directorio de compilación. **Después**: la rama `feat/inc01-measurement` con esta PR. La escena es sintética: no se lee la pantalla ni se envía entrada. La máquina es la de §1, con sesiones abiertas y sin compilaciones en marcha durante la ronda final.

Una ronda anterior, contra `ecd8986` (antes de INC-03 y de la purga) y con otras compilaciones en marcha, mostró en la situación con una región diferencias de +0,6 a +1,2 ms de p50. No se reprodujeron al repetir la medida y no miden el código que se integra, así que no son evidencia de esta PR.

| Capa | Situación | p50 antes | p50 después | p95 antes | p95 después |
|---|---|---|---|---|---|
| núcleo (`core_total`) | primera observación | 1,54 (1,50–1,62) | 1,55 (1,50–1,59) | 2,70 (2,67–2,79) | 2,72 (2,68–2,77) |
| núcleo (`core_total`) | en caliente, sin cambios | 3,12 (2,88–3,40) | 3,10 (2,87–3,48) | 5,49 (5,32–5,65) | 5,56 (5,30–5,65) |
| núcleo (`core_total`) | en caliente, una región | 3,41 (3,14–3,65) | 3,49 (3,15–4,09) | 5,78 (5,58–5,99) | 5,89 (5,60–6,01) |
| `observe_screen` entero | primera observación | 1,86 (1,74–3,04) | 1,83 (1,75–1,93) | 3,17 (3,02–3,31) | 3,15 (3,03–3,24) |
| `observe_screen` entero | en caliente, sin cambios | 3,36 (3,11–5,56) | 3,36 (3,11–3,88) | 5,81 (5,67–6,09) | 5,86 (5,66–6,04) |
| `observe_screen` entero | en caliente, una región | 3,75 (3,42–3,86) | 3,70 (3,33–3,92) | 6,24 (5,85–6,39) | 6,18 (5,92–6,39) |

Cada celda es la mediana de diez corridas, con el mínimo y el máximo entre paréntesis. Milisegundos. Los mínimos y máximos incluyen alguna corrida aislada más lenta (por ejemplo, 3,04 ms en la primera observación de `observe_screen`, en una corrida base); la mediana no las sigue.

**Diferencias apareadas** (después menos antes, mediana de los diez pares, en ms): núcleo, primera observación +0,010 en p50 y +0,013 en p95; núcleo, en caliente sin cambios −0,003 y −0,021; núcleo, una región +0,020 y −0,012; `observe_screen`, primera observación −0,022 y −0,012; `observe_screen`, en caliente sin cambios −0,040 y −0,007; `observe_screen`, una región −0,017 y +0,040.

**Lectura.**

1. **No hay efecto separable del ruido.** Las medianas de las diferencias quedan entre −0,04 y +0,02 ms en p50 y entre −0,02 y +0,04 ms en p95. En la fila del núcleo con una región el signo es positivo en 8 de 10 pares, pero la mediana es de 0,02 ms: es del tamaño del ruido de la propia medida.
2. **Un cambio de la primera ronda no se mantuvo.** Con los seis primeros pares, el núcleo con una región parecía 0,17 ms más lento en p50; la diferencia la explicaban dos pares (+0,37 y +0,51 ms) y los cuatro pares siguientes dieron una mediana de −0,02 ms. La rama sólo añade una comprobación de un indicador que vale `false` en la ruta por defecto, así que no se atribuye el cambio a esta PR.
3. **RNF-04 (p95 del núcleo ≤ 10 ms).** En las veinte corridas, el peor p95 del núcleo fue de 5,58 a 6,01 ms, antes y después, por debajo del objetivo.
4. **Coste por observación.** No medido de forma aislada: la rama añade cuatro lecturas del reloj (fin de la publicación y de la página), una copia del registro del plan y unas sumas. Es una estimación del camino de código, de cientos de nanosegundos, y no se presenta como medida.

**Límites.** Diez pares no resuelven diferencias de décimas de milisegundo, y la carga del equipo varía entre corridas. Estas cifras no sustituyen la medición del RNF-04 sobre el equipo de referencia en condiciones controladas, que sigue pendiente (capítulo 06 §6.6), ni la medición de los tramos nuevos en vivo.

## 12. PERF-03 / RNF-06: recursos y soak de 2 h (1.0.55)

**Qué mide.** `tools/perf/soak.py` durante 120 minutos, **sin entrada de ningún tipo**: la herramienta sólo llama a `observe_screen` y `get_engine_metrics`, no toma ninguna observación mientras la aplicación de prueba no está en primer plano y no llama a `execute_action`. Se mide `ommatix serve --source dxgi` con la configuración del operador `ommatix.rnf05.directml.toml` (detector en float16 sobre DirectML, la de RNF-05, §10), que observa la aplicación de prueba propia (`tools/s4/testapp` con `--animate 100 --animate-labels 2`: dos etiquetas que cambian cada 100 ms, para que haya regiones que re-analizar). Una muestra cada 5 s. El ajuste de crecimiento deja fuera los primeros 10 minutos; las tablas de latencia los incluyen.

**Sin umbral.** RNF-06 pide «sin crecimiento sostenido» y no fija cifra, y el responsable no fijó ninguna antes de medir. Esta sección da las pendientes y los máximos medidos. No declara que haya ni que no haya crecimiento sostenido: esa lectura es del responsable, con el umbral que fije. Un resultado negativo no se reemplaza por un umbral retrospectivo ([`09_EVALUACION_EXPERIMENTAL.md`](09_EVALUACION_EXPERIMENTAL.md) §9.7).

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
- **Memoria.** Después de los primeros 10 minutos, los bytes privados del motor estuvieron entre 1457,4 y 1461,9 MiB, un intervalo de 4,5 MiB. La pendiente medida es de +1,6 MiB/h sobre 1 h 50 min de muestras, y el conjunto de trabajo tiene la misma pendiente. Los manejadores y la GPU no cambiaron. No hay umbral, así que esta sección no dice si la pendiente es «crecimiento sostenido».
- **Cola.** Cero en todas las muestras, por la razón de arriba. RNF-06 pide una cola acotada: esta corrida la cumple sin observaciones fallidas, y no dice cómo se comporta la cola cuando una observación falla.
- **Latencia.** A partir del minuto 10, el total del motor tiene p50 de 142,5 ms, p95 de 150,8 ms y p99 de 155,8 ms, y las medianas de las ventanas se mantienen entre 141,8 y 143,2 ms. Esta medición no es la de RNF-05 (§10) y no cambia el estado de RNF-05.
- **Alcance.** La memoria es la del proceso `ommatix.exe`, no la de todo el sistema ni la de la aplicación de prueba.

**Qué queda abierto.**
1. **Umbral de RNF-06.** Lo fija el responsable antes de leer la pendiente con él. Sin umbral, RNF-06 no tiene veredicto y su estado no cambia (`PENDIENTE_IMPLEMENTACION_O_VALIDACION`).
2. **Una sola corrida.** No hay variación entre corridas ni entre días; otra corrida la daría.
3. **Cola con fallos.** Esta corrida no inyecta fallos de observación. Medir la cola cuando una observación falla necesita un escenario propio.
4. **Causa de la cola de latencia** de los primeros 10 minutos y de las dos observaciones de 214 ms. Una corrida con las etapas de `_meta` y la carga del equipo muestreada durante la corrida la cerraría.
5. **Condiciones que no coinciden con la sesión B:** dos monitores conectados (la sesión pide uno; sólo se captura el principal) y notificaciones activadas. Se dejaron como estaban; los números las incluyen.

**Reproducir.** Desde la raíz del repositorio, con los dos ejecutables compilados (`S4_INSTALACION_Y_REVERSION.md` §3): `python tools/perf/soak.py --config %LOCALAPPDATA%\Ommatix\ommatix.rnf05.directml.toml --minutes 120 --out soak-120.json`. El informe bruto se guarda fuera del repositorio, con el commit y la máquina; no lleva datos de pantalla (ADR-0012). Las pruebas de la herramienta, sobre dobles: `python -m unittest discover -s tools/perf -p "test_*.py"`.
