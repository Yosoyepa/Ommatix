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
