# S4: protocolo de aceptación reducido (corridas E2E supervisadas)

> **Nota de identificadores (14 de octubre de 2026).** Donde este informe y sus rondas dicen «E2E-02 del protocolo reducido» (la Calculadora) y «E2E-03 del protocolo reducido» (el Bloc de notas), el responsable decidió que esos flujos tengan identificadores propios: **S4R-02** (Calculadora: 7 + 8 = 15 y borrar) y **S4R-03** (Bloc de notas: abrir y cerrar «Editar»). **E2E-01** sigue siendo la aplicación de prueba. E2E-02 («Editor: escritura y guardado de archivo temporal») y E2E-03 («Explorador: navegar carpetas de prueba sin destrucción») de `matrices/plan_pruebas.csv` son otros casos y siguen `PENDIENTE_PRODUCTO`. El historial no se reescribe: las secciones de las rondas anteriores conservan las etiquetas con las que se escribieron, y sus registros guardan `e2e` con `E2E-02` y `E2E-03`; desde la octava ronda los registros añaden `flow_id` (`S4R-02`, `S4R-03`). Estado de la matriz: S4R-03 `EJECUTADO_SUPERVISADO_PARCIAL` (diez de diez corridas puntuadas, 0 falsos éxitos, séptima ronda) y S4R-02 `PENDIENTE_PRODUCTO` (no ejecutada).

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable del proyecto autorizó el 14 de octubre de 2026 un protocolo S4 **reducido** en la máquina de referencia: corridas E2E automatizadas (unas decenas, no las 450 del diseño) dentro de tres aplicaciones autorizadas, que son una aplicación de prueba propia con datos desechables, `notepad.exe` y la Calculadora de Windows. La parada es local, por `stop_file`, y no se guarda ninguna captura. **Alcance:** una máquina, un monitor capturado (salida 0), DPI 100 %, sólo clics. Este informe registra lo que se ejecutó, con qué resultado y qué queda pendiente para el protocolo completo. No cierra RF-18, RNF-03 ni RNF-05, ni ninguna casilla DoD (capítulo 06 §6.7).

Todas las cifras son **mediciones [V]** con el `n` indicado. No se guardó ningún frame ni ninguna etiqueta de pantalla. El OCR (es-ES) se procesó en memoria (ADR-0012). El cliente sólo registra estados, códigos, recuentos, tiempos y booleanos. La única excepción es el texto propio de la aplicación de prueba y de la Calculadora, que se mostró en la consola durante el piloto para diseñar los flujos. Del Bloc de notas nunca se mostró ni registró texto: sus pestañas restauradas pueden tener texto del responsable.

## 1. Protocolo ejecutado frente al diseño del capítulo 09

| Aspecto | Diseño (capítulo 03 §3.5, capítulo 06 §6.6, capítulo 09) | Ejecutado en S4 reducido |
|---|---|---|
| Familias | Formulario HTML local, editor de texto, explorador de archivos | Aplicación de prueba propia (análoga al formulario local), Bloc de notas (familia editor) y Calculadora (que no puntuó, §4.2) |
| Tareas | 5 por familia, con escritura y guardado donde aplica | 1 flujo por aplicación, **sólo clics**: ningún `type`, ningún guardado |
| Repeticiones | 10 por tarea y por DPI (100/125/150 %): 450 corridas | 10 por flujo puntuado, sólo a 100 %: **20 corridas puntuadas** |
| Temas | Claro/oscuro equilibrados | Sólo el tema por defecto de cada aplicación (claro en la de prueba) |
| Verificador | Independiente del motor, con postcondición externa | Aplicación de prueba: el título de la ventana leído por el SO tras cada paso. Todas: **anclaje por UI Automation** (qué control quedó bajo el cursor). Bloc de notas: el resultado del flujo se comprueba sólo por OCR del motor (§5) |
| Congelación | Configuración y corpus congelados antes de medir | Piloto exploratorio (31 solicitudes reales, excluido de los resultados, §4.4). Después se fijaron el cliente, la aplicación de prueba, el binario y la configuración (§2) antes de las 20 corridas |
| Planificador | Planificador fijo (agente LLM) | Ninguno: el cliente sigue una secuencia fija de etiquetas. Mide el motor y el bucle de acción, no un agente |

**Correspondencia con la matriz.** La autorización llama E2E-01 a la aplicación de prueba, E2E-02 a la Calculadora y E2E-03 al Bloc de notas, y el cliente usa esas etiquetas. En `matrices/plan_pruebas.csv` esos identificadores son otros casos: E2E-01 es un formulario local con valores dummy, E2E-02 un editor con escritura y guardado de un archivo temporal y E2E-03 un explorador de archivos. Sólo E2E-01 de la matriz queda cubierto en parte (§7). En este informe se usan las etiquetas de la autorización.

## 2. Entorno

| Campo | Valor |
|---|---|
| Hardware y sistema | Los de [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §1: Windows 11 Pro 26H2 (build 26300.9550), Ryzen 7 5700X, 32 GiB, Radeon RX 6700 XT (driver 32.0.21045.5002), 1920×1080, dos monitores, se captura la salida 0, DPI 100 % |
| Binario | `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr`, `rustc 1.99.0`, desde `main` (`974e9b5`) más la corrección del §6.1 (`36fabf6`, rama `feat/s4-reduced-e2e`). SHA-256 de `ommatix.exe`: `2edc9833…7447fe` |
| Detector y OCR | `icon_detect_v3` en DirectML (ONNX Runtime 1.24.4, DirectML 1.15.4), manifiesto `63d9df24…7d728e`; OCR `winrt` es-ES. Igual que en la segunda corrida de S3 |
| Aplicaciones | `ommatix-testapp.exe` ([`tools/s4/testapp`](../../tools/s4/testapp), `eframe`/`egui` 0.36.2, MIT/Apache-2.0, sin `unsafe` propio; SHA-256 `b01584cc…0c5`); Bloc de notas `Microsoft.WindowsNotepad` 11.2607.14.0; Calculadora `Microsoft.WindowsCalculator` 11.2607.0.0 |
| Cliente MCP | [`tools/s4/e2e_driver.py`](../../tools/s4/e2e_driver.py), que reutiliza el transporte de `tools/s3/mcp_driver.py` (JSON-RPC 2026-07-28 sobre stdio). SHA-256 del cliente usado en las corridas puntuadas: `6895e92a…88048a` |
| Servidor | `ommatix serve --source dxgi --output 0 --input sendinput --config <local>` |
| Configuración | `[engine] verify_timeout_ms = 500`, `verify_poll_ms = 50`. `[safety] actuation_enabled = true`, `allowed_applications = ["ommatix-testapp.exe", "calculatorapp.exe", "notepad.exe"]` (los nombres que atestigua el actuador; nunca `applicationframehost.exe`), `include_values = false`, `stop_file` en el directorio temporal de la sesión. `[models]` como `tools/s0/ommatix.s0.directml.toml`, con rutas absolutas. SHA-256 `9ef2685c…7f65c36a5`. La configuración sin actuación del piloto (`actuation_enabled = false`) tiene SHA-256 `7da8e791…d7f00`. No contienen secretos |

**Reglas de la corrida.** Entrada real sólo a través de la compuerta del motor: `execute_action` con `action: click`, `dry_run: false` y `expected_effect: visual_change`. Nunca se usó `type`, `scroll` ni ninguna otra vía de entrada. Antes de cada solicitud real, el cliente comprueba, sólo leyendo, que la aplicación en primer plano es la del flujo y que está maximizada en el monitor principal. Resuelve las ventanas de `ApplicationFrameHost.exe` igual que el actuador (§6.1). El objetivo es el único elemento cuya etiqueta, recortada, es exactamente la del paso, y debe estar `confirmed`. Si dos elementos comparten esa etiqueta, el cliente crea el archivo de parada y termina. También lo hace si el primer plano no es el esperado, si se agota el presupuesto (400 solicitudes reales en total, con un contador compartido entre invocaciones) o si tres corridas seguidas fallan. Un `STALE_SNAPSHOT` (nada entregado) permite observar de nuevo y repetir, como mucho tres veces. Un clic entregado nunca se repite. El cliente inicia y cierra sólo los procesos que lanzó: la aplicación de prueba y la Calculadora. El Bloc de notas no se inicia ni se cierra (§3).

## 3. Preparación e incidencias

- **Primer plano.** Mientras el responsable usaba el equipo, Windows negó el primer plano a la aplicación de prueba, que se lanzaba en segundo plano: una espera de 437 s y otra de 600 s sin éxito (ForegroundLockTimeout = 200 s). Ninguna petición de activación envía entrada. El responsable avisó entonces de que la Calculadora se abría en el otro monitor, donde se cerró la última vez. El cliente pasó a mover las ventanas de los procesos que lanzó al monitor principal y maximizarlas (`ShowWindow`/`SetWindowPos`, gestión de ventanas, no entrada). Desde entonces el primer plano llegó en 0,8 s de mediana.
- **Bloc de notas.** Las dos ventanas visibles del Bloc de notas (proceso 23164, creadas en S3) estaban minimizadas. Un proceso del Bloc de notas puede tener varias ventanas y las pestañas restauradas del responsable, así que cerrarlo podía destruir texto suyo. Por eso el cliente no lanza ni cierra ningún Bloc de notas: restaura y maximiza en el monitor principal una de esas ventanas, pide su activación (`SetForegroundWindow` sobre esa ventana) y al terminar la vuelve a minimizar. No hubo reinicios entre corridas del Bloc de notas.
- **`Ver` era ambiguo.** En una observación de sondeo, sin entrada, había dos elementos `Ver`: el menú (`confirmed`) y un nodo de texto, probablemente una palabra del documento restaurado. Por la regla de ambigüedad, el flujo usa el menú `Editar`, único y `confirmed` en todas las observaciones, en lugar de `Ver`. El clic sólo abre y cierra el menú; ningún elemento del menú se tocó.
- **Estado de la aplicación de prueba por OCR.** El OCR devuelve una palabra por elemento: `Valor: 3` daba dos elementos, `Valor:` y `3`, y una cifra no es distintiva. Por eso la aplicación escribe cada estado con una palabra única: cardinales para el contador (`cero`…`nueve`), ordinales para el paginador (`Primera`…`Novena página`) y `apagado`/`encendido`. Cuando el detector encierra la línea en una caja, el elemento lleva la frase entera (`Valor: cero`), así que el cliente compara palabras dentro de las etiquetas. El objetivo de cada clic se sigue eligiendo por etiqueta exacta.

## 4. Resultados

### 4.1 E2E-01, aplicación de prueba (10 corridas, reinicio entre corridas)

Flujo de 8 clics: `Sumar` ×3 (`uno`, `dos`, `tres`), `Reiniciar` (`cero`), `Activar` ×2 (`encendido`, `apagado`), `Siguiente` (`Segunda`) y `Anterior` (`Primera`). Cada paso se comprueba por OCR (la palabra del estado esperado está y las de los otros estados no). Además se comprueba por el título de la ventana, que refleja el estado y se lee con `GetWindowTextW`, y por el anclaje UIA: AccessKit expone los botones de egui a UI Automation, y el control bajo el cursor debe llamarse como la etiqueta.

| Medida | Valor |
|---|---|
| Corridas con éxito | **9 de 10** (IC 95 % de Wilson: 0,60–0,98) |
| Pasos / solicitudes reales / clics entregados | 72 / 73 / 72 (`delivered_actions` del motor: +72) |
| `effect_observed` / `indeterminate` / rechazos | 72 / 0 / 1 (`STALE_SNAPSHOT`, nada entregado; repetido con éxito) |
| Comprobación por título | 72 de 72 coinciden |
| Anclaje UIA | 72 de 72 en el botón de la etiqueta |
| Falsos éxitos | **0** |
| Fallos de la comprobación por OCR | 0 |
| Tiempo por corrida con éxito | mediana 8,3 s, p95 9,0 s (con reinicio y espera de 1,5 s) |

**Fallo (corrida 5).** La comprobación previa del título encontró en primer plano una ventana que no era la de la aplicación de prueba recién lanzada. No se envió nada. La aplicación se cerró y la corrida cuenta como fallo en el denominador. La causa no quedó registrada: el cliente no guarda qué ventana era. El capítulo 03 incluye en el denominador los fallos, los tiempos agotados y las intervenciones.

### 4.2 E2E-02, Calculadora: no puntuada

- **Atestiguación (corregido, §6.1).** El actuador `SendInput` atestiguaba `applicationframehost.exe`, el proceso que dibuja el marco de las aplicaciones UWP. Allowlistarlo habría autorizado todas las aplicaciones hospedadas. Con la corrección atestigua `calculatorapp.exe`, el proceso de la `CoreWindow` hospedada. En el piloto, **4 clics reales se entregaron con sólo `calculatorapp.exe` en la lista** (ningún `POLICY_DENIED`), lo que confirma la corrección en vivo.
- **El flujo aritmético es inviable por etiqueta OCR.** La Calculadora estaba en modo Científica. De 135 elementos, 129 no tenían etiqueta: el OCR de Windows no lee los botones de un solo glifo (`7`, `+`, `=`, `C`). Las únicas etiquetas eran palabras deformadas (`Fundón` por `Función`, `figono`, `tífica`, `mod`, `exp`), y sólo tres eran `confirmed`.
- **Error de anclaje.** El único control `confirmed` con una etiqueta utilizable era `Fundón`, el desplegable de funciones. El piloto lo usó para un flujo de abrir y cerrar. Los 4 clics dieron `effect_observed`, pero el clic llegó al botón **x²** (`xpower2Button`, «Cuadrado»), no al desplegable. UI Automation lo comprobó en la corrida en que su lectura ya funcionaba, y el OCR mostró `sqr(0)` en la línea de expresión en las corridas en que se registró. Fue inocuo: 0² = 0, y la pantalla de la Calculadora, leída por UI Automation, siguió en 0. Pero es un éxito falso respecto de la intención: el motor informó con verdad de un cambio visual, sin que el control fuese el pedido (§6.2). La comprobación por OCR del piloto, que sólo pedía que cambiara el conjunto de etiquetas, también lo habría dado por bueno. Por eso el cliente añadió el anclaje UIA y se dejó de actuar sobre la Calculadora. **E2E-02 no tiene corridas puntuadas.**

### 4.3 E2E-03, Bloc de notas (10 corridas, misma ventana)

Flujo de 2 clics: `Editar` abre el menú y `Editar` lo cierra. El resultado se comprueba por OCR: las palabras de los elementos del menú (`Deshacer`, `Cortar`, `Copiar`, `Pegar`, …) aparecen al menos 3 veces más que con el menú cerrado y luego vuelven a su recuento inicial. El anclaje UIA es aquí doble. El elemento bajo el cursor es un panel sin nombre, porque el Bloc de notas es una isla XAML y `ElementFromPoint` se detiene en el anfitrión. Por eso se comprueba además que el cursor está dentro del rectángulo del único elemento UIA llamado `Editar` de la ventana.

| Medida | Valor |
|---|---|
| Corridas con éxito | **10 de 10** (IC 95 % de Wilson: 0,72–1,00) |
| Pasos / solicitudes reales / clics entregados | 20 / 20 / 20 (`delivered_actions`: +20) |
| `effect_observed` / `indeterminate` / rechazos | 20 / 0 / 0 |
| Anclaje UIA (rectángulo de `Editar`) | 20 de 20 |
| Falsos éxitos | **0** |
| Tiempo por corrida | mediana 3,6 s, p95 3,9 s (sin reinicio) |

### 4.4 Piloto (excluido)

12 corridas exploratorias antes de congelar el protocolo, con 31 solicitudes reales y 26 clics entregados. Sirvieron para corregir el cliente: la ventana en el monitor principal, la espera tras el arranque, los reintentos tras `STALE_SNAPSHOT`, las palabras únicas, la comparación por palabras, el título y el anclaje UIA. También sirvieron para descubrir lo de la Calculadora (§4.2). Desglose: Calculadora, 4 corridas, 5 solicitudes y 4 clics, todos sobre x². Aplicación de prueba, 5 corridas, 19 solicitudes y 16 clics; en la última pasaron los 8 pasos. Bloc de notas, 3 corridas, 7 solicitudes y 6 clics: el menú se abrió y se cerró en las 3. En las dos primeras, el anclaje sin el rectángulo de respaldo dio un resultado no concluyente, el panel sin nombre. No se cuentan en ninguna tasa.

### 4.5 Totales y seguridad

- **Solicitudes `dry_run: false`: 124 de 400** (31 en el piloto y 93 puntuadas). **Clics entregados: 118** (26 y 92). Rechazos sin entrega: 6 `STALE_SNAPSHOT`, ningún `POLICY_DENIED`. Ningún `type`, `scroll`, guardado, clic en pestañas, botones de ventana o elementos de menú.
- **Falsos éxitos en las corridas puntuadas: 0 de 92** `effect_observed`. Se cuenta como falso éxito un `effect_observed` cuyo título o cuyo anclaje UIA contradicen el efecto pedido. Con 0 eventos en 92, la aproximación binomial da un límite superior del 95 % cercano a 3/92 ≈ 3,3 %. Los clics de una misma corrida no son independientes, así que es sólo orientativo (capítulo 09 §9.7). En el piloto hubo 4, todos en la Calculadora (§4.2).
- **Tasa global puntuada: 19 de 20** (IC 95 % de Wilson: 0,76–0,99). Por aplicación: 90 % y 100 %. Estas cifras no evalúan RNF-03, que exige el protocolo definido: 450 corridas con verificador independiente.
- **Parada:** el archivo de parada no existía antes de ninguna sesión y no se activó en las corridas puntuadas. En el piloto tampoco hizo falta. `geometry_epoch` fue 1 en las 113 observaciones puntuadas y ninguna estuvo degradada.
- **Estado final del escritorio:** la aplicación de prueba y la Calculadora, cerradas (sólo los procesos lanzados aquí). La ventana del Bloc de notas, minimizada otra vez, con el menú cerrado. Ningún proceso de `ommatix` en ejecución.

## 5. Latencias (DirectML, `_meta` y cliente)

Mediana / p95 (máximo), en milisegundos. Las observaciones y acciones de una corrida no son independientes; no se aplicó bootstrap por secuencia.

| Medida | E2E-01 (n) | E2E-03 (n) | Total (n) |
|---|---|---|---|
| Observación con inferencia (`total` de `_meta`) | 241,5 / 357,5 (363,3) (79) | 149,7 / 167,5 (325,4) (22) | 239,6 / 357,1; p99 360,3 (101) |
| Observación sin regiones planificadas | 6,6 / 7,5 (4) | 21,9 / 25,0 (8) | — |
| Etapa `detect` en observaciones con inferencia | — | — | 223,0 / 335,2 (101) |
| `execute_action` entregada, ida y vuelta en el cliente (vigencia, entrega, verificación) | 81,2 / 90,9 (94,4) (72) | 89,8 / 95,1 (20) | 82,8 / 93,8 (92) |
| `execute_action` con `STALE_SNAPSHOT` | 6,4 (1) | — | — |
| Primer plano tras lanzar o restaurar | 0,8 s (mediana) | 0,0–0,14 s | — |

- En la aplicación de prueba, cada clic cambia el valor, el título y el resaltado del botón bajo el cursor: dos o tres regiones, y cada una cuesta una inferencia (S0 §9). Por eso su observación tras un clic, con un p95 de 357 ms, cuesta más que la del Bloc de notas, que se queda en 168 ms. **Ninguna de las dos cumple la meta inicial de RNF-05** para el incremental (p95 ≤ 150 ms). El Bloc de notas se queda cerca (mediana de 150 ms).
- El efecto se observó dentro del plazo de verificación en 92 de 92 clics: con una captura de vigencia, la inmediata y una más tras 50 ms, la ida y vuelta es de unos 80–95 ms. Un clic del Bloc de notas vio el cambio en la primera captura (35,6 ms).

## 6. Hallazgos

1. **Corregido: la Calculadora se atestiguaba como `applicationframehost.exe`.** En Windows 11, la ventana en primer plano de una aplicación UWP pertenece a `ApplicationFrameHost.exe`. El actuador atestiguaba ese proceso, así que sólo allowlistando el anfitrión (es decir, todas las aplicaciones hospedadas) podía actuar. Ahora, en el módulo de Windows del adaptador de entrada (ámbito de ADR-0016), un marco de clase `ApplicationFrameWindow` cuyo proceso es `applicationframehost.exe` se resuelve al proceso de su hija `Windows.UI.Core.CoreWindow`. Un marco sin contenido hospedado (aplicación suspendida o minimizada) no se atestigua: nunca se devuelve el anfitrión. Un punto sobre las ventanas propias del marco (barra de título y botones de minimizar, maximizar y cerrar) no pertenece a ninguna aplicación y se rechaza. Las ventanas de esa clase de otro proceso (`explorer.exe` tiene una) no se resuelven. La integridad, el foco de `type` y la comprobación del punto usan el proceso resuelto. Pruebas: `a_frame_host_window_attests_the_hosted_application` (regresión), `only_the_application_frame_host_is_resolved` y `a_point_on_the_frame_hosts_own_windows_belongs_to_no_application`. Verificado en vivo: 4 clics entregados con `calculatorapp.exe` en la lista.
2. **Abierto: un clic anclado en otro control.** Con rol `unknown`, la etiqueta de un control son las palabras OCR que caen al menos en un 75 % dentro de su caja (algoritmo 2, IoC), y el clic va al centro de la caja. En la Calculadora, la caja que se llevó la palabra `Función` estaba centrada sobre x². El motor cumple lo especificado y no ve el error: `effect_observed` sólo afirma un cambio visual (capítulo 07 §7.5), y lo hubo. Un agente que confíe en la etiqueta creería haber abierto el desplegable. Propuestas, que exigen cambiar el dossier y no se aplican aquí: (a) apuntar al centro de las palabras de la etiqueta cuando la caja es mucho mayor que ellas; (b) rebajar a `unknown` la accionabilidad de una caja cuyo texto ocupa una fracción pequeña de su área; (c) la evidencia de accesibilidad del hallazgo 4, que da el rectángulo real del control.
3. **Limitación del OCR: glifos sueltos.** El OCR de Windows no lee botones de un solo carácter (dígitos y operadores de la Calculadora). Sin otra fuente de etiquetas, las interfaces de teclado numérico no son direccionables por etiqueta. Esto afecta a EV-01 (cobertura).
4. **Limitación conocida, sin `type`.** El motor sólo permite `type` sobre un `Role::Input` confirmado, y el detector de S0 tiene una sola clase (rol `unknown`). Esa regla no se relajó. Por eso ningún flujo escribe, y la aplicación de prueba no tiene campos de texto. **Propuesta:** un adaptador de evidencia de UI Automation (puerto `AccessibilityProvider` del roadmap v1.1, capítulo 06 §6.8) que aporte roles (`Edit` → `input`), `label_source: accessibility` y rectángulos, fusionados con la ruta visual. La aplicación de prueba ya expone sus controles a UI Automation mediante AccessKit, y el anclaje de este informe muestra que ese árbol es legible desde fuera. Otra vía es un detector con clases de rol. No se implementa aquí: es un puerto nuevo con su ADR y su escenario.
5. **Abierto (de S3): el recibo de `SendInput`** (`submitted`/`inserted`) sigue sin llegar al agente.
6. **Del cliente, no del motor:** el primer plano bajo el bloqueo de Windows, las ventanas que se abren en otro monitor, `Ver` ambiguo por el texto del documento y el OCR por palabras se resolvieron en el cliente (§3) y no son defectos del motor.

## 7. Estado en la matriz

| Prueba o requisito | Estado | Justificación |
|---|---|---|
| E2E-01 (formulario local) | `EJECUTADO_SUPERVISADO_PARCIAL` | 10 corridas en una aplicación local controlada con datos desechables y confirmación observable (OCR, título y UIA), 9 de 10 con éxito y 0 falsos éxitos. Parcial: sólo clics (no se escriben valores dummy), una tarea, DPI 100 %, un tema |
| E2E-02 (editor: escritura y guardado) | sin cambio | El flujo del Bloc de notas sólo abre y cierra un menú: ni escribe ni guarda |
| E2E-03 (explorador) | sin cambio | No ejecutado |
| PERF-02 (percepción) | `EJECUTADO_SUPERVISADO_PARCIAL` | Latencia incremental medida en vivo, en release, con pesos y hardware fijados (n = 101 observaciones con inferencia en dos aplicaciones). Sin separar frío/caliente ni caché, sin refresco completo en caliente controlado, un solo DPI |
| PERF-01, PERF-03, EV-01, EV-02 | sin cambio | No ejecutados. Las 20 corridas no son el protocolo de EV-02 |
| RF-18 | sin cambio (`PENDIENTE_IMPLEMENTACION_O_VALIDACION`) | Flujos reales verificables en 2 de las 3 aplicaciones, sin escritura. La Calculadora no se pudo puntuar (§4.2) |
| RNF-03, RNF-05 | sin cambio | RNF-03 exige el protocolo definido (19 de 20 no lo es). RNF-05, incremental: p95 de 357 y 168 ms, por encima de 150 ms |

## 8. Qué falta para el protocolo completo de 450 corridas

- **Entrada de texto:** roles fiables (`input`) por accesibilidad o por un detector con clases (hallazgo 4). Sin eso no hay E2E-01 con valores dummy ni E2E-02 con escritura y guardado.
- **Tercera familia:** el explorador de archivos con carpetas de prueba (E2E-03 de la matriz) y una Calculadora direccionable (hallazgo 3) o su sustitución por otra aplicación autorizada.
- **DPI 125 y 150 %** (IT-07 no ejecutada) y temas claro/oscuro equilibrados.
- **Verificador independiente para todas las familias:** contenido del archivo temporal, estado del formulario o carpeta mostrada. El título y el anclaje UIA de este informe son un primer paso.
- **Planificador fijo** (agente LLM) y un cliente MCP de terceros, porque el capítulo 09 reporta aparte la evaluación E2E con planificador.
- **Corrección del anclaje** (hallazgo 2) antes de medir el éxito en aplicaciones con cajas grandes.
- **Medición:** bootstrap por secuencia, p99 con al menos 10 000 muestras temporales (capítulo 09 §9.6), frío y caliente, RSS y VRAM (PERF-03) y soak.
- **Entorno:** una sesión sin uso concurrente del equipo. Aquí el responsable trabajaba a la vez, lo que obligó a pedir el primer plano y añade riesgo de interferencia.

## 9. Cómo repetirlo

Desde la raíz del repositorio, con la configuración con actuación del §2 (nunca en el repositorio), el binario del §2 y la aplicación de prueba compilada (`cargo build --release` en `tools/s4/testapp`):

```text
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <con_actuacion>.toml --app testapp --runs 10 --max-live 100 --restart each --session-id s4-e2e01 --ledger <dir>\ledger.txt --results <dir>\s4.results.jsonl
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <con_actuacion>.toml --app notepad --runs 10 --max-live 40 --session-id s4-e2e03 --ledger <dir>\ledger.txt --results <dir>\s4.results.jsonl
```

`--mode probe`, con la configuración sin actuación, observa el estado inicial sin entrada. El archivo de resultados (JSONL) contiene estados, códigos, recuentos, tiempos y booleanos; ningún texto de la pantalla.

## Segunda ronda con accesibilidad (ADR-0017)

**Fecha:** 14 de octubre de 2026. **Autorización:** la de §1 (protocolo reducido, tres aplicaciones, parada local por `stop_file`, ninguna captura guardada), con la evidencia de UI Automation de [ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md) activada. **Resultado en una línea:** la serie puntuada de E2E-01 no tuvo ningún éxito, porque la escritura perdió el texto en tres de cuatro intentos, y la serie se detuvo por la regla de tres corridas seguidas fallidas. E2E-02 y E2E-03 no se ejecutaron. Sí quedó medido que los controles con nombre se confirman por accesibilidad y que los 36 clics y escrituras entregados llegaron al control pedido. Esta ronda no cierra RF-18, RNF-03 ni RNF-05, ni ninguna casilla DoD.

Todas las cifras son **mediciones [V]**. No se guardó ningún frame ni captura. Los registros contienen sólo estados, códigos, recuentos, tiempos y booleanos; la única excepción son las etiquetas fijas de la aplicación de prueba y de la Calculadora impresas en los sondeos, que quedan en el directorio de ejecución, fuera del repositorio.

### 1. Cambios frente a la primera ronda

| Aspecto | Primera ronda (§1 a §9) | Segunda ronda |
|---|---|---|
| Binario | `dxgi,sendinput,directml,winrt-ocr` | `dxgi,sendinput,directml,winrt-ocr,uia`, `rustc 1.99.0`, SHA-256 `72915ba1…e67c` |
| Accesibilidad | ninguna | `[models] accessibility = "uia"`: roles, nombres y patrones de UI Automation, fusionados con la visión |
| Aplicación de prueba | sin campo de texto | campo `Campo de texto` (un `TextEdit` simple cuyo nombre de accesibilidad se fija en AccessKit, sin texto dibujado) y su contenido en el título (`texto=…`). SHA-256 `09ccf30d…c9bf` |
| Cliente | `tools/s4/e2e_driver.py`, sólo clics | el mismo archivo con flujos por nombre de UI Automation, escritura con `type`, etapas de `_meta` (incluida `accessibility`) y verificación independiente sin texto. SHA-256 `451668e4…10b4d2` |
| Configuración local | `actuation_enabled = true`, `include_values = false` | igual, más `[models] accessibility = "uia"` y rutas absolutas. SHA-256 `ecc7c5f7…2eac5`, sin secretos |
| Cuaderno de solicitudes | 124 de 400 (no se guardó) | empieza en 124, que es el recuento de la primera ronda, y marca **160 de 400** al final |

### 2. Sondeos sin entrada

Cada sondeo arrancó la aplicación, observó el estado inicial y la cerró. Ningún sondeo envió entrada (`input=dry-run`, 0 solicitudes reales).

| Objetivo | Elementos con la etiqueta exacta | Rol, accionabilidad y origen de la etiqueta |
|---|---|---|
| E2E-01: `Sumar`, `Restar`, `Reiniciar`, `Activar`, `Siguiente`, `Anterior` | 1 de cada uno | `button`, confirmado, `accessibility` |
| E2E-01: `Campo de texto` | 1 | `input`, confirmado, `accessibility` |
| E2E-02: `Siete`, `Más`, `Ocho`, `Es igual a`, `Borrar` | 1 de cada uno | `button`, confirmado, `accessibility`; pantalla inicial `0`, leída por UI Automation |
| E2E-03: `Editar` (Bloc de notas) | 1 | elemento de menú (`unknown`), confirmado, `accessibility` |

Las teclas de la Calculadora, que la primera ronda no podía direccionar (hallazgo 3 de [`reviews/2026-10-14-s4-reduced-acceptance.md`](../reviews/2026-10-14-s4-reduced-acceptance.md)), quedan direccionables por su nombre de accesibilidad. El Bloc de notas no imprimió ninguna etiqueta.

### 3. Corridas con entrada real

| Corrida | Escritura | Resultado | Solicitudes | Efecto del motor |
|---|---|---|---|---|
| Piloto, E2E-01, 1 corrida (excluida) | `visual_change` | falló el paso 9: el título no contenía el texto | 9 | `effect_observed` con el texto ausente: **1 falso éxito** según el protocolo |
| Diagnóstico, E2E-01, 1 corrida (excluida) | `visual_change` | éxito, 9 de 9 pasos | 9 | `effect_observed`, con `prueba 123` en el título |
| Serie puntuada, corridas 1 y 2, E2E-01 | `value_updated` | falló el paso 9: campo vacío en las dos | 9 cada una | `indeterminate`, que es lo correcto, y el título sin texto |
| Serie puntuada, corrida 3, E2E-01 | `value_updated` | fallo previo: el título inicial no coincidió. No se envió nada | 0 | — |
| E2E-02, Calculadora | — | **no ejecutada** | 0 | — |
| E2E-03, Bloc de notas | — | **no ejecutada** | 0 | — |

La escritura del piloto y del diagnóstico usó `visual_change`. Antes de la serie puntuada, el protocolo cambió a `value_updated` para la escritura, porque el piloto mostró que `visual_change` puede darse por cumplido con el clic de enfoque (hallazgo 2). El cambio queda documentado aquí, y el piloto cuenta en el recuento de falsos éxitos.

Totales de la ronda:

- **Solicitudes `dry_run: false`: 36** (9 del piloto, 9 del diagnóstico y 18 de la serie). Clics y escrituras entregados: **36**. Efectos: `effect_observed` 34 e `indeterminate` 2. Rechazos: 0. `STALE_SNAPSHOT` reintentados: 0. `POLICY_DENIED`: 0.
- **Agarre por UI Automation:** en las 36 entregas, el control bajo el cursor era el pedido.
- **Falsos éxitos:** 1, en el piloto, que se cuenta aunque se excluya de las puntuadas. **0 en la serie puntuada.**
- **Éxitos puntuados de E2E-01: 0 de 3.** La escritura dejó el texto en el título en 1 de 4 intentos, el diagnóstico.

### 4. Latencias

Mediana, p95 y máximo, en milisegundos, de las 41 observaciones de las corridas con entrada. Son los `timings` de `_meta`:

| Etapa de `observe_screen` | Mediana | p95 | Máximo |
|---|---|---|---|
| `total` | 318,8 | 495,6 | 624,6 |
| `detect` | 224,6 | 336,0 | 438,6 |
| `accessibility` | 71,1 | 111,4 | 373,6 |
| `recognize` | 11,9 | 43,2 | 44,0 |
| `capture` | 4,1 | 9,2 | 18,5 |
| `plan` | 2,1 | 3,2 | 3,3 |
| `track` | 0,5 | 0,7 | 0,9 |
| `fuse` | 0,1 | 0,1 | 0,1 |

- Ida y vuelta de `execute_action` en el cliente: clic, mediana 84,3 y p95 93,7 (n = 32); escritura, mediana 41,1 y p95 80,6 (n = 4). La respuesta de `execute_action` no trae etapas en `_meta`, y esa falta es un hueco de medición.
- La etapa `accessibility` cuesta una mediana de 71 ms por observación con regiones. El p95 total, de 496 ms, queda muy por encima de los 150 ms de RNF-05: **RNF-05 sigue sin cumplirse**.

### 5. Hallazgos

1. **Abierto, grave: `type` pierde las teclas que llegan en el mismo fotograma que su clic de enfoque.** Tres de cuatro escrituras no dejaron texto en el campo, y el motor informó `input_delivered: true`, porque el SO aceptó todos los eventos. La causa probable, según el código de egui 0.36.2 (`widgets/text_edit/builder.rs`), es que el campo procesa el texto antes de conceder el foco en ese mismo fotograma. No se verificó fotograma a fotograma. El actuador comprueba el foco a nivel de **ventana** (`focus_holds_target`), que en una ventana única siempre se cumple, así que no lo detecta. No se corrige en esta ronda: exige verificar el foco del campo, por ejemplo con la propiedad `HasKeyboardFocus` de UI Automation sobre el elemento bajo el punto, y eso cambia la compuerta de envío de ADR-0007 y ADR-0016. Propuesta: una ADR nueva. Mientras tanto, `--focus-click` hace un clic de enfoque antes de escribir. Ese modo no se ha ejecutado.
2. **Abierto: `visual_change` en `type` puede dar `effect_observed` sin el texto.** El motor sólo verifica cambios visuales (capítulo 07 §7.5), y el clic de enfoque ya los produce. En el piloto dio un falso éxito según el protocolo. Por eso la serie puntuada usó `value_updated`, que el motor no sabe verificar y devuelve `indeterminate` sin reintento. Propuesta: que el contrato limite la postcondición de `type` a `value_updated`, o que el motor rechace `visual_change` en `type`. Requiere cambiar el dossier.
3. **Abierto: el título inicial no coincidió en una de las tres corridas puntuadas**, con la aplicación de prueba en primer plano y en su mismo proceso. Algo parecido, con otra ventana en primer plano, pasó en la corrida 5 de la primera ronda. Tampoco se identificó la causa, porque el cliente no registró el título. Desde ahora registra sólo su forma (longitudes y booleanos, `initial_title_mismatch`). Esa salida no existe en las corridas de esta ronda.
4. **Parada del operador.** Tras tres corridas puntuadas fallidas seguidas, el cliente creó `stop.flag` según la regla del protocolo. **El archivo sigue creado y no se ha retirado**: la reanudación es decisión del responsable. Con la parada activa no se envió ninguna entrada, así que E2E-02 y E2E-03 no pudieron ejecutarse.
5. **Abierto, de la primera ronda: el recibo de `SendInput`** (`submitted`/`inserted`) no llega al agente.

Esta ronda no cambia ningún código de las crates de Rust, así que no hay regresión nueva que añadir. Los hallazgos 1 y 2 necesitan una decisión de diseño, por eso no se corrigen aquí. Constan en [`reviews/2026-10-14-s4-uia-acceptance.md`](../reviews/2026-10-14-s4-uia-acceptance.md).

**Nota posterior (CHANGELOG 1.0.16).** El responsable decidió el 14 de octubre de 2026 la corrección de los hallazgos 1 y 2 con la [ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md): `type` en dos fases, con el foco confirmado sobre el elemento objetivo por UI Automation antes de enviar las teclas, y `visual_change` medido tras el clic de enfoque. Está en el código y probado con dobles (BDD-34 y BDD-35, y `src/application/src/engine/tests/typing.rs`); **no se ha repetido la corrida viva**. Los resultados de esta ronda describen el comportamiento anterior a esa corrección.

### 6. Estado en la matriz

- **E2E-01:** `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambio. Esta ronda confirma los clics y los controles por accesibilidad, pero la escritura no se verificó en la serie puntuada (0 de 3).
- **E2E-02 y E2E-03** de la matriz (editor con escritura y guardado; explorador de archivos): sin cambio, no ejecutadas.
- **PERF-02:** sin cambio, parcial. La etapa `accessibility` ya está medida en vivo.
- **UT-44, IT-01 a IT-07, SEC y EV:** sin cambio.
- **RF-18, RNF-03 y RNF-05:** sin cambio. RNF-05 sigue por encima de 150 ms.

### 7. Cómo repetirlo y reanudar

El cuaderno de solicitudes (`ledger.txt`) está en 160 y el tope es 400. El archivo de parada, `stop.flag`, existe y **lo retira sólo el responsable**. Antes de reanudar conviene un sondeo y un piloto de escritura con el código posterior a la ADR-0018. El modo `--focus-click` del cliente (un clic extra sobre el campo antes de escribir) **queda sustituido por esa corrección**: se conserva apagado por defecto, nunca se ejecutó y no debe usarse, porque ya no es necesario y su clic adicional falsearía la medición de `visual_change`.

```text
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app testapp --mode probe --runs 1 --ledger <dir>\ledger.txt --results <dir>\probe.jsonl
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app testapp --mode run --runs 10 --max-live 100 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e01
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app calculator --mode run --runs 10 --max-live 60 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e02
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app notepad --mode run --runs 10 --max-live 30 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e03
```

La configuración local es la de §1 de esta ronda. Nunca se guarda en el repositorio.

## Tercera ronda: type en dos fases (ADR-0018)

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable autorizó el mismo día repetir el protocolo reducido completo (aplicación de prueba, Calculadora y Bloc de notas) con `type` en dos fases y la evidencia de UI Automation activada, con parada local por `stop_file`, sin capturas guardadas y sin uso del equipo durante la ejecución. **Resultado en una línea:** E2E-01 pasó 10 de 10 corridas puntuadas, con la escritura verificada por el título en las 10, 0 falsos éxitos y 0 teclas perdidas. E2E-02 no puede completar su flujo tal como está especificado: en el modo Científica de la Calculadora no existe un botón «Borrar» tras «Es igual a»; 0 de 3, y la serie se detuvo por la regla de tres fallos seguidos. E2E-03 no se puntuó: dos pilotos no produjeron de forma fiable el menú «Editar» que el flujo verifica. Esta ronda no cierra RF-18, RNF-03 ni RNF-05, ni ninguna casilla DoD (capítulo 06 §6.7).

Todas las cifras son **mediciones [V]**. No se guardó ningún frame ni texto de pantalla (ADR-0012); los registros contienen estados, códigos, recuentos, tiempos y booleanos. En la consola sólo se imprimieron los nombres fijos de los botones de la Calculadora y su número en pantalla (en los sondeos y en el diagnóstico de §4.2), y del Bloc de notas sólo recuentos de las palabras fijas de su menú.

### 1. Cambios frente a la segunda ronda

| Aspecto | Segunda ronda | Tercera ronda |
|---|---|---|
| Binario | `dxgi,sendinput,directml,winrt-ocr,uia`, SHA-256 `72915ba1…e67c` | mismas características, `rustc 1.99.0`, desde `45cbade`. `ommatix.exe` SHA-256 `97a03896…98ec72` |
| `type` | `value_updated` tras el piloto | `visual_change`, medido tras el clic de enfoque y antes de las teclas, con confirmación de foco por UI Automation (ADR-0018) |
| Activación de la ventana (cliente) | `WScript.Shell.AppActivate` | ventana principal con `AttachThreadInput` y `SetForegroundWindow` (§3) |
| Cliente | `tools/s4/e2e_driver.py`, SHA-256 `451668e4…10b4d2` | mismo cliente con la activación corregida, SHA-256 `8aa80988…c353b9` en las corridas puntuadas. Después de ellas sólo se corrigieron un comentario y una docstring, sin cambio de código: la versión confirmada tiene SHA-256 `0d0fe263…87736e`. El piloto 1 de E2E-01 usó la versión anterior, `56353c46…4d53a7` |
| Aplicación de prueba | SHA-256 `09ccf30d…c9bf` | igual |
| Configuración | `ecc7c5f7…2eac5` | igual (copia de `s4r2-live.toml`, con `[models] accessibility = "uia"`) |
| Cuaderno de solicitudes | 160 de 400 | 160 → 291 de 400 (131 solicitudes en esta ronda) |

### 2. Sondeos, pilotos y orden de las series

- **Sondeos sin entrada** (`--mode probe`, `input=dry-run`): la aplicación de prueba expone sus seis botones y `Campo de texto` como `input` confirmado por accesibilidad. La Calculadora expone `Siete`, `Más`, `Ocho`, `Es igual a` y `Borrar`, una vez cada una, confirmadas, con la pantalla en `0`.
- **Piloto 1 de E2E-01** (excluido): 9 de 9 pasos entregados y verificados, la escritura incluida. Se hizo con la versión anterior del cliente y con el primer plano preparado a mano (§3), así que queda excluido por partida doble.
- **Piloto 2 de E2E-01** (excluido): 8 clics entregados y verificados. En el paso 9, `Campo de texto` no estaba confirmado en las observaciones de búsqueda, así que no se envió ninguna tecla y la corrida falló (§4.5).
- **Piloto de E2E-02** (excluido): 4 clics entregados y verificados (pantalla 7, 7, 8 y 15). En el paso 5, «Borrar» no se encontró confirmado (§4.2).
- **Pilotos 1 y 2 de E2E-03** (excluidos): ver §4.3.
- **Orden real de las series:** E2E-01 (pilotos y serie puntuada), E2E-02 (piloto, diagnóstico y serie puntuada) y E2E-03 (dos pilotos, sin serie puntuada). Se puntuó E2E-02 después de los pilotos de E2E-03 para que la parada que cerró la ronda no impidiera el Bloc de notas. El orden del protocolo era E2E-01, E2E-02 y E2E-03.

### 3. Incidencias de preparación y del cliente

- **Primer plano.** En el piloto 1 de E2E-01, `AppActivate` no llevó la aplicación de prueba al primer plano: 164 solicitudes en 333,6 s, con el escritorio delante. Un `SetForegroundWindow` directo sobre la ventana visible devolvió falso. Con `AttachThreadInput` sobre el hilo del primer plano, seguido de `SetForegroundWindow` y de la desconexión, devolvió verdadero al primer intento, sin enviar ninguna entrada. La causa dentro de `AppActivate` no se ha establecido: la enumeración de ventanas de la aplicación lista también una ventana auxiliar (`Winit Thread Event Target`) que no es la principal. El cliente activa ahora la ventana principal (`pick_main_window`: la mayor ventana visible que no es auxiliar) con `bring_to_front`. Prueba de regresión: `tools/s4/test_e2e_driver.py`, tres pruebas de selección que fallan con la versión anterior (`AttributeError`). Con la versión corregida, en las 13 corridas puntuadas y en los sondeos hubo 1 solicitud de activación y el primer plano llegó en 0,88–1,10 s. Es gestión de ventanas: ningún clic, tecla ni movimiento de ratón.
- **Medición de `AppActivate`.** Para descartar que la activación inyectara entrada, se midió el temporizador de inactividad (`GetLastInputInfo`) alrededor de una llamada a `AppActivate` sobre una aplicación de prueba propia. Sin llamada, el temporizador no se reinició (59,03 → 59,55 s). La llamada devolvió verdadero en 0,01 s y el temporizador no cambió (61,56 s antes y después). Sólo se midió el caso de éxito.
- **Bloc de notas.** Antes de la primera corrida se guardó la posición de sus dos ventanas (`GetWindowPlacement`, sin títulos). Tras cada tramo en que se usaron, se restauró esa posición y se comprobó que coincide. Las dos ventanas siguen minimizadas y en la misma posición que al empezar.
- **Archivo de parada.** Se retiró una vez, al inicio, como autorizó el responsable. Lo creó el cliente al tercer fallo de E2E-02, según el protocolo, y sigue creado. No se retira en esta ronda.
- **Eventos de entrada sin acción.** El temporizador de inactividad se reinició varias veces sin que el cliente enviara nada: 0,3 s antes del primer arranque en vivo y 13,5 s tras una pausa de 20 s sin llamadas de entrada. Su origen no se ha identificado. El envío de `SendInput` del motor sólo se llama desde la entrega de acciones (`src/adapters/input/src/sendinput/win.rs`), y el cliente no envía entrada fuera de ellas. El cliente registra `user_idle_s_at_start` en cada corrida.

### 4. Resultados

#### 4.1 E2E-01, aplicación de prueba (10 corridas puntuadas, reinicio entre corridas)

Flujo: `Sumar` ×3, `Reiniciar`, `Activar` ×2, `Siguiente` y `Anterior` (8 clics) y, después, `type` de `prueba 123` en `Campo de texto` con `expected_effect: visual_change`. Verificación por el título de la ventana (SO) tras cada paso y por el anclaje de UI Automation.

| Medida | Valor |
|---|---|
| Corridas con éxito | **10 de 10** (IC 95 % de Wilson: 0,72–1,00) |
| Pasos / solicitudes reales / entregas | 90 / 90 / 90 (80 clics y 10 escrituras) |
| `effect_observed` / `indeterminate` / rechazos | 90 / 0 / 0 |
| Comprobación por título (SO) | 80 de 80 clics; 10 de 10 escrituras (`texto=prueba 123`) |
| Teclas perdidas | **0** |
| Anclaje UIA (el control bajo el cursor es el pedido) | 90 de 90 |
| Falsos éxitos | **0** |
| Duración por corrida con éxito | mediana 10,2 s, p95 10,5 s |

#### 4.2 E2E-02, Calculadora (3 corridas puntuadas; serie detenida)

Flujo: `Siete`, `Más`, `Ocho` y `Es igual a`, con la pantalla comprobada por UIA tras cada clic (7, 7, 8 y 15), y después `Borrar`.

| Medida | Valor |
|---|---|
| Corridas con éxito | **0 de 3** (IC 95 % de Wilson: 0,00–0,56) |
| Solicitudes / entregas | 13 / 12. Una `STALE_SNAPSHOT` (nada entregado, repetida con éxito) |
| `effect_observed` / `indeterminate` | 12 / 0 |
| Pantalla comprobada por UIA tras cada clic | 12 de 12 |
| Anclaje UIA | 12 de 12 |
| Falsos éxitos | **0** |
| Fallo | paso 5, «Borrar» no encontrado confirmado, en las tres corridas |

**Diagnóstico** (excluido; cuatro clics, el mismo flujo): tras «Es igual a» la Calculadora está en el modo Científica (con «Alternar grados», «Notación científica», etc.), y su árbol de UI Automation expone 49 botones. Ninguno se llama «Borrar». Sí aparecen «Quitar entrada» (CE), «Borrar toda la memoria» y «Borrar toda el historial», que son otros controles. El botón «Borrar» estaba, confirmado, en el estado inicial, y en tres observaciones sucesivas tras «=» no aparece. La causa no está establecida (un cambio de la interfaz tras el resultado). En cualquier caso, el paso tal como está especificado no puede cumplirse. **Decisión para el responsable:** otra tecla final (por ejemplo «Quitar entrada», que sí aparece), otro modo (estándar) u otro orden de los pasos.

#### 4.3 E2E-03, Bloc de notas (sin puntuar)

Flujo: `Editar` abre el menú y `Editar` lo cierra. No hay escritura.

- **Piloto 1.** El clic sobre `Editar` se entregó, y el cursor quedó dentro del rectángulo de `Editar` (anclaje confirmado). El motor informó `effect_observed`, pero en las cuatro observaciones siguientes no aparecieron palabras de los elementos del menú. La corrida falló en el paso 1 y no se envió el segundo clic.
- **Piloto 2.** El mismo clic se entregó y el motor informó `indeterminate` (`INDETERMINATE_EFFECT`): no hubo cambio visual en la ventana de verificación de 500 ms. La corrida falló en el paso 1 y no se reintentó.
- **Estado del menú.** Tras el piloto 1, la lectura de UI Automation (sólo lectura) marcó `Editar` como expandido en la ventana minimizada. Tras restaurarla y observar sin entrada, el menú no aparecía en pantalla. El indicador parece obsoleto; no está establecido.
- **Por qué no se puntuó.** El cliente no comprueba que el menú esté cerrado al empezar (la primera comprobación acepta cualquier estado y fija la referencia), y no cierra un menú que quede abierto tras un fallo. Cada corrida fallida cambia el estado de la ventana del responsable sin que el cliente pueda restaurarlo. La causa del resultado del clic no se ha establecido. Se detuvo la serie por decisión del operador tras el piloto 2, antes de llegar a la regla de tres fallos. La causa es específica de esta serie, así que se continuó con E2E-02 (§2).

#### 4.4 Totales y seguridad

- **Solicitudes `dry_run: false`: 131** (de 160 a 291). Entregadas: **129**, de ellas 118 clics y 11 escrituras. Rechazadas sin entrega: 2 `STALE_SNAPSHOT`, ambas repetidas con éxito. `POLICY_DENIED`: 0.
- **Efecto:** 128 `effect_observed` y 1 `indeterminate` (clic de E2E-03, piloto 2).
- **Escrituras:** 11 entregadas, las 11 verificadas por el título del SO. **Teclas perdidas: 0.** Ninguna escritura fuera de `Campo de texto`.
- **Falsos éxitos: 0** en todas las corridas, puntuadas y pilotos.
- Ningún `scroll`, clic en pestañas, botón de ventana ni elemento de menú distinto de `Editar` en el Bloc de notas.
- **Parada:** la creó el cliente al tercer fallo de E2E-02 y sigue creada. No se ha retirado.

#### 4.5 Piloto 2 de E2E-01: el campo no se confirmó en las observaciones de búsqueda

Tras los pasos 7 y 8 del flujo, la lectura de accesibilidad tardó 226 ms y 489 ms (la mediana de la serie puntuada es 71 ms) y devolvió 80 elementos, frente a 82–85 en las observaciones anteriores. Las observaciones 10 a 12, de búsqueda de `Campo de texto`, fueron incrementales: no se planificó ninguna región, la lectura de accesibilidad no se repitió (0 ms) y el campo conservó el estado de la observación 9. Causa probable, no verificada: el recorrido de UI Automation, acotado a 250 ms (ADR-0017), devolvió un árbol parcial en esas observaciones, y el estado de accesibilidad de una observación incremental es el de la última lectura completa. El efecto fue el comportamiento seguro: no se envió ninguna tecla. No se corrige aquí, porque cambia la fusión del motor (hallazgo 2).

#### 4.6 Latencias (`_meta` y cliente)

Mediana / p95 (máximo), en milisegundos, de las observaciones de las corridas puntuadas.

| Etapa de `observe_screen` | E2E-01 (n = 100) | E2E-02 (n = 25) |
|---|---|---|
| `total` | 321,3 / 437,1 (543,1) | 298,2 / 483,1 (490,1) |
| `accessibility` (con regiones) | 71,0 / 88,9 (111,5) (n = 100) | 109,1 / 142,1 (150,3) (n = 16) |
| `detect` (con regiones) | 224,0 / 333,9 (443,6) | 222,5 / 337,4 (338,7) |
| `recognize` | 11,2 / 16,5 (44,0) | 13,8 / 32,2 (57,7) |
| `capture` | 3,9 / 5,1 (10,0) | 13,0 / 23,2 (24,6) |
| `plan` | 2,2 / 3,0 (3,3) | 2,3 / 3,1 (3,2) |
| `track` | 0,5 / 0,8 (1,0) | 2,2 / 3,3 (4,3) |
| `fuse` | 0,1 / 0,1 (0,1) | 0,1 / 0,2 (0,3) |

- Ida y vuelta de `execute_action` entregada, en el cliente: clic de E2E-01, mediana 82,2 y p95 92,6 (n = 80); clic de E2E-02, 78,2 y 84,2 (n = 12); escritura de E2E-01, 109,0 y 120,5 (n = 10).
- `execute_action` no trae etapas en `_meta` (0 de 103 respuestas), así que la medida de acciones es sólo la ida y vuelta del cliente.
- RNF-05 (incremental, p95 ≤ 150 ms) **no se cumple**: el p95 de `total` es 437 ms en E2E-01 y 483 ms en E2E-02.

#### 4.7 Hallazgos

Detalle, resolución y prueba de regresión en [`reviews/2026-10-14-s4-third-round.md`](../reviews/2026-10-14-s4-third-round.md).

1. **Corregido: activación de la ventana del cliente** (§3). Prueba: `tools/s4/test_e2e_driver.py`.
2. **Corregido después (CHANGELOG 1.0.18), motor:** lectura de accesibilidad parcial sin relectura en observaciones incrementales (§4.5). El proveedor informa de la lectura truncada y el motor la repite en la observación siguiente.
3. **Corregido después (1.0.18), cliente:** «Borrar» no se expone tras «Es igual a» en la Calculadora, modo Científica (§4.2). El control de borrado se elige entre «Borrar», «Borrar todo» y «Borrar entrada».
4. **Corregido después (1.0.18), cliente:** el cliente no verificaba ni cerraba el estado del menú (§4.3). Ahora comprueba `Collapsed` al empezar y cierra un menú abierto tras un fallo. La causa de que el clic no produjera un menú observable sigue sin establecerse.
5. **Abierto, entorno:** eventos de entrada sin origen identificado (§3).
6. **Corregido después (1.0.18):** `execute_action` devuelve etapas en `_meta` (`io.ommatix/action_timings`). El recibo de `SendInput` sigue sin llegar al agente.

Las correcciones de 1.0.18 se probaron con dobles, sin entrada real: no se han medido en vivo.

### 5. Estado en la matriz

`matrices/plan_pruebas.csv` no cambia. Ninguna fila necesita un estado nuevo, porque los actuales ya reflejan la ejecución supervisada parcial: E2E-01 sigue `EJECUTADO_SUPERVISADO_PARCIAL`, ahora con escritura de un valor de prueba y confirmación observable (10 de 10); sigue siendo parcial (una tarea, un DPI, un tema). E2E-02 y E2E-03 de la matriz (editor con escritura y guardado; explorador de archivos) no se han ejecutado. PERF-02, IT-03 e IT-04 siguen parciales. RF-18, RNF-03 y RNF-05 no cambian: RNF-05 no se cumple.

### 6. Qué queda y cómo reanudar

- **Repetir E2E-02** con el cliente de 1.0.18 (el control de borrado se elige por nombre). Si tras «=» no existe ninguno de «Borrar», «Borrar todo» y «Borrar entrada» (modo Científica), la corrida falla sin entrada: el responsable decide el modo de la Calculadora. El cuaderno está en 291 de 400.
- **Repetir E2E-03 con el responsable presente:** el cliente comprueba `Collapsed` al empezar y cierra el menú tras un fallo; falta ver por qué un clic en «Editar» no producía un menú observable.
- **Medir en vivo la relectura de accesibilidad** (hallazgo 2, corregido con dobles): cuántas observaciones vuelven a leer el árbol y cuánto cuesta en aplicaciones con árboles grandes. Una ventana que siempre se trunca se lee entera en cada observación.
- **Identificar el origen de los eventos de entrada** sin acción (§3) y confirmar que nadie usa el equipo durante las corridas.
- **Para reanudar:** el responsable retira `stop.flag`; después, un sondeo sin entrada, un piloto y la serie puntuada.

## Cuarta ronda: correcciones de la ronda 3

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable autorizó el mismo día repetir el protocolo reducido completo (aplicación de prueba, Calculadora y Bloc de notas) con las correcciones de [CHANGELOG 1.0.18](CHANGELOG.md), sin uso del equipo durante la ejecución, con la retirada de `stop.flag` una sola vez al inicio de la ronda, sin capturas ni texto de pantalla guardados y con un límite de 400 solicitudes reales **por ronda**, con el cuaderno continuando desde 291. **Resultado en una línea:** E2E-01 pasó 10 de 10 corridas puntuadas, con los 80 clics y las 10 escrituras verificados por el título de la ventana y 0 teclas perdidas. E2E-02 no se puntuó: en el modo Científica no hay control de borrado tras «Es igual a», y el cambio a modo Estándar no se pudo completar de forma verificable (el único clic de navegación no quedó confirmado sobre el control pedido), así que la serie se abandonó. E2E-03 no se ejecutó: se detuvo antes de cualquier entrada en el Bloc de notas, por ese mismo desajuste sin explicar. Esta ronda no cierra RF-18, RNF-03 ni RNF-05, ni ninguna casilla DoD (capítulo 06 §6.7).

Todas las cifras son **mediciones [V]**. No se guardó ningún frame ni texto de pantalla (ADR-0012); los registros contienen estados, códigos, recuentos, tiempos y booleanos. Del Bloc de notas no se leyó ni registró texto.

### 1. Cambios frente a la tercera ronda

| Aspecto | Tercera ronda | Cuarta ronda |
|---|---|---|
| Binario | `dxgi,sendinput,directml,winrt-ocr,uia`, `ommatix.exe` SHA-256 `97a03896…98ec72` | mismas características, `rustc 1.99.0`, desde `e9e5b2d` (correcciones 2, 3, 4 y 6). `ommatix.exe` SHA-256 `2c07e447…ee2c01` |
| Aplicación de prueba | SHA-256 `09ccf30d…c9bf` | igual (el mismo binario) |
| Cliente | `e2e_driver.py` SHA-256 `0d0fe263…87736e` (versión confirmada tras la tercera ronda) | `e2e_driver.py` SHA-256 `2af90bf3…a283`, **sin cambios** en el archivo; se ejecutó mediante `s4r4_run.py` (§2) |
| Configuración | `ecc7c5f7…2eac5` | igual, SHA-256 `ecc7c5f7…2eac5`: `s4r4-live.toml` es una copia de `s4r3-live.toml`, idéntica a `s4r2-live.toml` |
| Cuaderno de solicitudes | 160 → 291 de 400 | 291 → 396 (105 solicitudes en esta ronda, de 400 por ronda) |

### 2. Decisiones de ejecución (para revisar)

1. **Límite por ronda.** `LIVE_CAP` de `e2e_driver.py` cuenta el cuaderno completo. Con el cuaderno en 291 sólo quedaban 109 solicitudes, y el protocolo necesita unas 190. El responsable indicó que el límite de 400 se reinicia por ronda y que el cuaderno continúa desde 291. El archivo del cliente no cambió: `s4r4_run.py` fija `LIVE_CAP` en 291 + 400 = 691 sobre el cuaderno, de modo que esta ronda tiene 400 solicitudes y el cuaderno sigue en cifras acumuladas. Defecto registrado en la [revisión de la cuarta ronda](../reviews/2026-10-14-s4-fourth-round.md) (hallazgo 1).
2. **Tiempos de acción.** El cliente sólo lee `io.ommatix/timings`; los tiempos de `execute_action` están en `io.ommatix/action_timings` (corrección 6 de la tercera ronda). `s4r4_run.py` los añade a `stages_ms` con el prefijo `action_`. Hallazgo 2 de la revisión.
3. **`stop.flag`.** Se retiró una vez, al inicio, como autorizó el responsable. Era un archivo de 0 bytes creado por el cliente tras el tercer fallo de E2E-02 en la tercera ronda. No se creó ninguno en esta ronda.
4. **Asistente del cambio de modo.** Para la Calculadora se usó un asistente de un solo uso (`s4r4_calc_standard.py`, SHA-256 `7f148352…6d68e`) que reutiliza las primitivas del cliente: el clic de navegación y el de «Estándar» pasan por la puerta del motor, cada uno en su propia llamada, y el segundo sólo se envía si el primero se entregó y quedó anclado al control pedido. Ver §5.1.
5. **Scripts fuera del repositorio.** `s4r4_run.py` (SHA-256 `9ec27312…9e6a12`), `s4r4_calc_probe.py` (SHA-256 `f4bb378c…bcd1c5`, sólo lectura) y `analyze_s4r4.py` están en el directorio de ejecución, no en el repositorio.

### 3. Sondeos y pilotos (excluidos de la puntuación)

- **Sondeo de la aplicación de prueba** (`--mode probe`, sin entrada): los nueve objetivos aparecen una vez cada uno, confirmados por accesibilidad (ocho botones y `Campo de texto`, `input`).
- **Sondeo de la Calculadora** (`s4r4_calc_probe.py`, sin `execute_action`): modo Científica, con «Alternar grados» y «Notación científica» presentes; «Abrir navegación» confirmado como botón; «Borrar» confirmado en el estado inicial.
- **Piloto de E2E-01** (excluido): 9 de 9 pasos entregados y verificados, con la escritura incluida; anclaje 9 de 9.
- **Piloto de E2E-02** (excluido): 4 clics entregados y verificados, con anclaje 4 de 4. En el paso 5, tras «Es igual a», ningún control de borrado (`Borrar`, `Borrar todo` o `Borrar entrada`) estaba confirmado: `alternatives_present` vacío. La corrida falló cerrada, sin entrada adicional. **Es el caso que la autorización prevé**: no hay control de borrado tras «=» en modo Científica.

### 4. E2E-01, aplicación de prueba (10 corridas puntuadas, reinicio entre corridas)

Flujo: `Sumar` ×3, `Reiniciar`, `Activar` ×2, `Siguiente` y `Anterior` (8 clics), y después `type` de `prueba 123` en `Campo de texto` con `expected_effect: visual_change`. Verificación por el título de la ventana (SO) tras cada paso y por anclaje de UI Automation.

| Medida | Valor |
|---|---|
| Corridas con éxito | **10 de 10** (IC 95 % de Wilson: 0,72–1,00) |
| Pasos / solicitudes reales / entregas | 90 / 91 / 90 (80 clics y 10 escrituras) |
| `effect_observed` / `indeterminate` / rechazos | 90 / 0 / 1 (`STALE_SNAPSHOT` en la corrida 7, sin entrega, repetido con éxito) |
| Comprobación por título (SO) | 80 de 80 clics; 10 de 10 escrituras |
| Teclas perdidas | **0** |
| Anclaje UIA (el control bajo el cursor es el pedido) | 90 de 90 |
| Falsos éxitos | **0** |
| Activación | 1 solicitud por corrida; primer plano en 0,37–0,42 s |
| Duración por corrida | mediana 9,4 s, p95 9,9 s (con reinicio) |

### 5. E2E-02, Calculadora (sin puntuar)

#### 5.1 Lo que ocurrió

- En modo Científica, el piloto falló cerrado en el paso 5 (§3). Sin control de borrado tras «=», el paso final no puede cumplirse tal como está especificado. Es el hallazgo de la tercera ronda, reproducido con el cliente de 1.0.18.
- Se aplicó la contingencia que prevé la autorización: cambiar a modo Estándar con clics por la puerta del motor, elegidos por nombre exacto (primero el botón de navegación, después «Estándar»).
- **Un único intento.** El clic sobre «Abrir navegación» se entregó y el motor informó `effect_observed` (cambio visual). Sin embargo, la comprobación posterior al clic por UI Automation **no confirmó** el control pedido: el control bajo el cursor no se llamaba «Abrir navegación». No se envió el clic sobre «Estándar» ni se repitió el de navegación.
- El asistente se detuvo por un error propio de contabilidad: su registro de resultados no inicializaba claves que el cliente incrementa. El fallo ocurrió después del clic entregado y antes de cualquier otro; el asistente cerró la Calculadora que lanzó. No es un defecto del motor ni del cliente.
- La causa más probable es que el botón de navegación cambie de nombre al abrirse el panel, y que la comprobación compare nombres después del clic. **No está verificado**: comprobarlo exige otro clic de navegación, y la regla de seguridad no lo permite sin una decisión del responsable. Queda como hallazgo 4 de la revisión.
- Por la regla de la autorización («si no es posible con seguridad, abortar la serie e informar»), **E2E-02 se abandona**: 0 corridas puntuadas. Tampoco se verificó en qué modo quedó la Calculadora tras el intento.

#### 5.2 Totales de E2E-02

| Medida | Valor |
|---|---|
| Corridas puntuadas | 0 (serie abandonada) |
| Piloto | 4 clics entregados, `effect_observed` 4, anclaje 4 de 4; fallo en el paso 5 |
| Contingencia | 1 clic entregado (navegación), anclaje **no** confirmado; 0 clics sobre «Estándar» |

### 6. E2E-03, Bloc de notas (no ejecutada)

No se envió ninguna entrada al Bloc de notas: ni piloto ni serie. La razón es que el desajuste de anclaje del clic de navegación de la Calculadora no está explicado, y la autorización prohíbe continuar si la causa es de seguridad. Un clic que podría haber llegado a otro control del Bloc de notas (con pestañas y ventanas del responsable) entra en ese caso. Las dos ventanas del Bloc de notas siguen minimizadas (`showCmd` 2) y sus tres procesos son los de antes; `notepad_placement_final_r4.json` (sólo posiciones, sin títulos) lo registra. Como E2E-03 no se ejecutó, el cliente no tocó esa ventana.

### 7. Totales y seguridad

- **Solicitudes `dry_run: false`: 105** (de 291 a 396): pilotos 13 (9 y 4), serie puntuada de E2E-01 91 (90 entregas y 1 rechazo `STALE_SNAPSHOT`) y contingencia de la Calculadora 1. **Entregas: 104.** **Rechazos sin entrega: 1.** `POLICY_DENIED`: 0. Ningún `scroll`, ninguna escritura fuera de `Campo de texto`, ningún clic en pestañas, botones de ventana ni elementos de menú.
- **Falsos éxitos en las corridas puntuadas: 0 de 90** `effect_observed`. Fuera de la puntuación, el clic de navegación de la contingencia fue un `effect_observed` con anclaje no confirmado, que el criterio del cliente contaría como falso éxito candidato. Hallazgo 4 de la revisión.
- **Teclas perdidas: 0** de 10 escrituras. **Parada:** no se creó `stop.flag` en la ronda.
- **Entrada sin origen:** el tiempo sin entrada al inicio de cada corrida puntuada fue de 16,8 s en la primera y de 2,1–2,3 s en las demás, coherente con el último clic del propio cliente. El registro no ve la entrada entre corridas, así que no es concluyente.
- **Estado final del escritorio:** la aplicación de prueba y la Calculadora, cerradas (sólo los procesos lanzados aquí); ningún proceso de `ommatix` en ejecución. El Bloc de notas, sin tocar, con sus dos ventanas minimizadas.

### 8. Latencias (`_meta` y cliente, E2E-01)

Mediana, p95 y máximo, en milisegundos, de las 101 observaciones de las corridas puntuadas (todas con inferencia).

| Etapa de `observe_screen` | Mediana | p95 | Máximo |
|---|---|---|---|
| `total` | 312,8 | 429,9 | 622,3 |
| `detect` | 221,7 | 332,3 | 334,0 |
| `accessibility` | 65,1 | 82,3 | 390,5 |
| `recognize` | 9,7 | 15,2 | 55,8 |
| `capture` | 3,9 | 7,8 | 14,7 |
| `plan` | 2,2 | 2,7 | 2,8 |
| `track` | 0,4 | 0,6 | 0,9 |
| `fuse` | 0,1 | 0,1 | 0,1 |

| Etapa de `execute_action` (`io.ommatix/action_timings`, n = 91) | Mediana | p95 | Máximo |
|---|---|---|---|
| `total` | 82,8 | 112,3 | 117,0 |
| `verification` | 58,0 | 65,1 | 71,6 |
| `freshness` | 19,4 | 27,2 | 29,8 |
| `delivery` | 2,0 | 11,2 | 13,3 |
| `focus` (0 en los clics; sólo `type` espera el foco) | 0,0 | 14,3 | 17,2 |
| `gate` | 0,2 | 0,5 | 1,1 |

- Ida y vuelta en el cliente, clics entregados: mediana 81,4 y p95 89,8 (n = 80). Escrituras: 112,7 y 117,0 (n = 10).
- **RNF-05 (incremental, p95 ≤ 150 ms) no se cumple:** el p95 de `total` es 430 ms.

### 9. Hallazgos

Detalle, resolución y pruebas en la [revisión de la cuarta ronda](../reviews/2026-10-14-s4-fourth-round.md).

1. **Cliente: límite acumulativo** (§2.1). Abierto. Evitado con el envoltorio, sin cambiar el archivo.
2. **Cliente: tiempos de acción no leídos** (§2.2). Abierto. Evitado con el envoltorio.
3. **Calculadora: sin control de borrado tras «=» en modo Científica** (§3 y §5). Abierto: decisión del responsable (modo Estándar, otra tecla final u otro orden).
4. **Calculadora: el clic de navegación no quedó anclado** (§5.1). Abierto. Hipótesis sin verificar: el nombre del botón cambia al abrirse el panel. Propuesta: identificar ese control por `AutomationId` cuando cambia de nombre al activarse, con una prueba de regresión.
5. **Asistente de contingencia: error propio de contabilidad** (§5.1). No se corrigió ni se volvió a usar en esta ronda; debe corregirse antes de cualquier reutilización. No afecta al motor ni al cliente.
6. **Eventos de entrada sin origen** (hallazgo 5 de la tercera ronda). Sin evidencia nueva en esta ronda (§7).
7. **Recibo de `SendInput`** (hallazgo 5 de la primera ronda): abierto, sin cambio.

### 10. Estado en la matriz

`matrices/plan_pruebas.csv` **sin cambios**, con esta justificación por fila:

- **E2E-01** (formulario local con valores dummy): `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambio. Esta ronda repite la prueba (10 de 10, escritura de `prueba 123`, sin falsos éxitos), pero sigue siendo parcial: una tarea, un DPI y un tema.
- **E2E-02** (editor con escritura y guardado) y **E2E-03** (explorador de archivos): `PENDIENTE_PRODUCTO`, sin cambio. Las pruebas de S4 con la Calculadora y el Bloc de notas no son esas pruebas de la matriz, y esta ronda no ejecutó ninguna de ellas.
- **PERF-02** (percepción): `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambio. Se volvió a medir la latencia incremental en vivo (n = 101), sin ampliar el alcance.
- **RF-18, RNF-03 y RNF-05**: sin cambio. RNF-05 no se cumple (p95 de 430 ms).

### 11. Qué queda y cómo reanudar

- **Decisión del responsable (Calculadora):** el modo para E2E-02 (Estándar, u otra tecla final) y si el clic de navegación debe identificarse por `AutomationId`. Sin eso, E2E-02 no puede puntuar.
- **Decisión del responsable (Bloc de notas):** si E2E-03 se ejecuta en la próxima ronda, con el responsable presente y después de revisar el hallazgo 4.
- **Cliente:** corregir `LIVE_CAP` (límite por ronda) y leer `io.ommatix/action_timings`, con prueba de regresión y el cambio de hash documentado. No se ha hecho en esta ronda.
- **Reanudar:** no hace falta retirar `stop.flag`, porque no existe. Antes de una nueva corrida en vivo: un sondeo sin entrada, un piloto y la serie puntuada.

### Preparación de la quinta ronda

**Fecha:** 14 de octubre de 2026. Sólo cambia el cliente (`tools/s4/e2e_driver.py`, SHA-256 `98f0ae08…1967e5`) y sus pruebas; no cambia ninguna crate de Rust ni se ejecutó nada en vivo (las pruebas usan un escritorio simulado y no envían entrada). Decisiones del responsable: el propio E2E cambia la Calculadora a modo Estándar (el responsable no lo hace a mano) y la quinta ronda ejecuta E2E-02 y E2E-03. Detalle en la [revisión](../reviews/2026-10-14-s4-fourth-round.md), sección «Preparation for the fifth round».

| Hallazgo (§9) | Defecto | Corrección | Prueba |
|---|---|---|---|
| 1 | `LIVE_CAP` (400) se comparaba con el cuaderno completo, no con la ronda | `LIVE_CAP` limita las solicitudes de la ronda: `--round-start N` (obligatorio en modo `run`) es el valor del cuaderno al empezar la ronda y se detiene cuando `cuaderno − N ≥ 400`. El cuaderno sólo acumula; el cliente registra `ledger_start..ledger_end` por corrida, por ronda y al final | `RoundCapTest`: con el cuaderno en 291 y la ronda en 291 se permiten 400 solicitudes y la 401 detiene la ejecución |
| 2 | `stage_ms` sólo leía `io.ommatix/timings` | cada acción registra además `io.ommatix/action_timings` como `action_total`, `action_gate`, `action_freshness`, `action_delivery`, `action_focus`, `action_verification`; `observe_screen` conserva `io.ommatix/timings` | `ActionTimingsTest`, `test_the_engine_timings_of_each_action_are_recorded_per_action` |
| 4 | El anclaje comparaba nombres tras el clic; un conmutador que cambia de nombre («Abrir navegación» → «Cerrar navegación») no se confirmaba | anclaje por identidad: antes de cada clic, UI Automation (lado SO) busca en la ventana en primer plano de la aplicación el elemento con el mismo nombre exacto y un tipo de control propio del rol que informó el motor, y registra su `RuntimeId` y `AutomationId`; si no hay exactamente uno (ninguno, varios, ilegible o sin identificador) no se envía ningún clic y la corrida falla antes de la entrada. Tras el clic, el elemento bajo el cursor (o su padre) debe tener el mismo `RuntimeId`, o el mismo `AutomationId` si el `RuntimeId` no está disponible, aunque su nombre haya cambiado. No hay respaldo por nombre (se eliminaron `named_control` y la comprobación por rectángulo). Un clic no anclado termina la corrida sin más entrada | `IdentityFunctionsTest`, `GroundingByIdentityTest` (un conmutador que se renombra queda anclado; varios candidatos, ninguno o sin identificador no envían nada; otro elemento bajo el cursor termina la corrida) |
| 5 | El asistente de la ronda 4 no inicializaba contadores que `steps()` incrementa | el cambio de modo es un paso del cliente (`--calc-standard`) y usa `new_result`, que inicializa todos los contadores | `ResultCountersTest` |

**Cambio de modo (`--calc-standard`).** Se ejecuta una vez antes de la serie de E2E-02, como invocación aparte (`--app calculator --mode run --round-start <N> --max-live 4`), por la puerta del motor únicamente: lee el modo por UI Automation (encabezado `Header` y ausencia de los controles propios de Científica); sin clic si ya es Estándar o si no se puede leer; pulsa el conmutador de navegación por su etiqueta exacta («Abrir navegación», que se registra), vuelve a observar y pulsa «Estándar» por su etiqueta exacta, cada clic con anclaje por identidad; verifica el modo por UI Automation, nunca por texto de pantalla en los registros. Si el panel no se abre, o «Estándar» no es único y confirmado, o un clic no queda anclado, se detiene sin pulsar nada más. Deja la Calculadora cerrada (recuerda el modo) y cierra sólo el proceso que lanzó. Gasta como máximo 4 solicitudes reales.

**Sin comprobar en vivo.** Las dos señales del modo (el nombre del encabezado `Header` y los controles propios de Científica) y el tipo de control de «Estándar» (el rol `unknown` admite `ListItem`, `MenuItem`, `RadioButton`, `TabItem` y otros) se tomaron de la documentación de la aplicación y de los sondeos de las rondas 3 y 4, y no se han leído en vivo. Si el encabezado no se lee, el paso termina sin ningún clic. Que la Calculadora conserve el modo tras cerrarla con `taskkill` tampoco está comprobado: lo comprobará el piloto de E2E-02, que falla cerrado si no existe un control de borrado.

**Para la quinta ronda:** `--round-start` con el valor del cuaderno (396), un sondeo sin entrada, `--calc-standard`, el piloto y la serie de E2E-02, y después E2E-03 con el responsable presente. Los comandos de §9 y de la segunda ronda necesitan `--round-start`.

## Quinta ronda: E2E-02 no se ejecutó y E2E-03 se detuvo por un hallazgo de seguridad en el piloto

**Fecha:** 14 de octubre de 2026. Rama `feat/uia-evidence` (PR #11), desde `e8f443c`. Se ejecutó el cliente tal como está comprometido (`tools/s4/e2e_driver.py`, SHA-256 `98f0ae08…1967e5`, comprobado antes de cada invocación en vivo), sin envoltorios, con la configuración `s4r5-live.toml` (copia de la de la cuarta ronda, SHA-256 `ecc7c5f7…2eac5`) y el binario `ommatix.exe` de la cuarta ronda (`2c07e447…ee2c01`). Autorización del responsable: E2E-02 y E2E-03, diez corridas puntuadas cada una, sin presencia del responsable en el equipo.

**Resultado:** cero corridas puntuadas. El cambio de modo a Estándar se detuvo en el paso «Estándar» sin segundo clic, así que E2E-02 no se ejecutó. El piloto de E2E-03 (excluido de la puntuación) tuvo un clic en «Editar» sin anclaje confirmado tras el clic: según la regla de seguridad, la serie de diez corridas no se ejecutó. El archivo de parada no existió en ningún momento, y la ventana del propietario quedó con su colocación original.

### 1. Verificación previa a la ejecución en vivo

Antes de cualquier entrada real:

- `cargo fmt --all --check`: correcto.
- `cargo clippy --workspace --all-targets -- -D warnings`: correcto.
- `cargo clippy -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia --all-targets -- -D warnings`: correcto.
- `cargo test --workspace`: 337 pruebas pasadas, 0 fallidas, 1 ignorada.
- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: el binario ya estaba al día.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 62 pruebas correctas.

Al terminar la ronda: `referencia_ejecutable/run_tests.py`, 48 de 48 correctas; `validar_paquete.py`, 210 comprobaciones y 0 fallidas. Los archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y la versión de jsonschema; se restauraron con `git restore`.

### 2. Ejecución en vivo

| Paso | Invocación (resumen) | Solicitudes reales | Resultado |
|---|---|---|---|
| Cambio de modo | `--app calculator --mode run --calc-standard --max-live 4 --round-start 396` | 1 | Detenido en el paso «Estándar»; sin segundo clic |
| E2E-02, piloto y serie | no se ejecutaron | 0 | Sin cambio a Estándar (§3.2) |
| E2E-03, piloto (excluido) | `--app notepad --mode run --runs 1 --max-live 4 --round-start 396` | 2 | Clic en «Editar» sin anclaje tras el clic; cierre del menú sin anclaje por identidad (§3.3 y §3.4) |
| E2E-03, serie | no se ejecutó | 0 | Regla de seguridad (§3.3) |

El cupo de la ronda era de 400 solicitudes desde 396. Se usaron 3: el cuaderno pasó de 396 a 399.

### 3. Resultados

#### 3.1 Cambio de modo (`--calc-standard`)

- Modo al empezar: `other`, es decir, no Estándar. Si hubiera sido Estándar, el paso no habría hecho ningún clic.
- Clic 1, «Abrir navegación» (la etiqueta exacta del conmutador): entregado, `effect_observed`. Antes del clic, un candidato (`AutomationId` `TogglePaneButton`, tipo `Button`); después del clic, el elemento bajo el cursor coincidió: anclaje confirmado. Es la primera confirmación en vivo del anclaje por identidad de 1.0.20.
- Paso 2, «Estándar»: `target label not found or not confirmed`. No hubo segundo clic. El modo no se volvió a leer (`mode_after` nulo). El paso cerró la Calculadora que había iniciado.
- El registro no dice si había cero candidatos, varios, o uno que no se confirmó (hallazgo 3).

La Calculadora no cambió a Estándar, y su modo final no se volvió a leer.

#### 3.2 E2E-02, Calculadora: no ejecutada

Sin Estándar no hubo piloto ni serie, según la regla del responsable. El hallazgo 3 de la cuarta ronda (sin control de borrado tras «Es igual a» en Científica) sigue abierto; esta ronda no lo ha podido comprobar.

#### 3.3 E2E-03, Bloc de notas: piloto con hallazgo de seguridad; serie no ejecutada

- **Ventanas:** dos ventanas del propietario, ambas minimizadas (`showCmd` 2 y 2). Antes del piloto se guardó su colocación (sólo posiciones, con un auxiliar fuera del repositorio que no lee texto ni envía entradas) y después se restauró; la verificación coincide. Al terminar, el cliente volvió a minimizar la ventana que había usado, como hace con una ventana que estaba minimizada.
- **Clic 1, «Editar» (abrir):** antes del clic, un candidato (`AutomationId` `Edit`, tipo `MenuItem`). Entregado, `effect_observed`. UI Automation lee el menú en estado Expandido.
- **Anclaje después del clic: no confirmado.** Ni el elemento bajo el cursor ni su padre tienen el mismo `RuntimeId` ni el mismo `AutomationId` que el elemento encontrado antes del clic. El cliente marcó `grounding_wrong` y contó un `engine_false_success` = 1, y la corrida se detuvo sin más entradas.
- **Clic 2, cierre del menú (ruta de restauración):** entregado; el motor lo informó `indeterminate` con `INDETERMINATE_EFFECT` (verificación de 509,6 ms). Después UI Automation lee el menú Colapsado. No hubo reintento.

**Decisión.** El efecto que confirma UI Automation (menú Expandido) es el esperado, así que el clic probablemente llegó a «Editar». Pero el anclaje posterior al clic no se confirmó, y el motor informó `effect_observed` en un clic que no quedó anclado. El cliente lo cuenta como falso éxito y el responsable lo trata como hecho de seguridad. Un hecho de seguridad no permite continuar con la serie siguiente, así que las diez corridas puntuadas de E2E-03 no se ejecutaron.

**Causa no verificada.** Hay dos explicaciones: (a) una falsa alarma del anclaje, por ejemplo porque el `RuntimeId` del elemento cambia al expandirse el menú; (b) un clic que no llegó a «Editar» y que el motor informó como efecto. Los datos de esta ronda no permiten distinguirlas, y ninguna está comprobada.

#### 3.4 La ruta de cierre del menú no ancla por identidad

El cierre del menú no pasa por el anclaje de `steps()`. `restore_menu` busca «Editar» por su etiqueta entre los elementos observados y envía `restore_click` sin comprobar UI Automation antes ni después del clic. El resto de clics del cliente sí se anclan (hallazgo 2).

### 4. Totales y seguridad

| Serie | Corridas puntuadas | Éxitos | Falsos éxitos | Solicitudes reales | Entregadas | Estados del motor |
|---|---|---|---|---|---|---|
| Cambio de modo | — | — | — | 1 | 1 | `effect_observed` 1 |
| E2E-02 | 0 | — | — | 0 | 0 | — |
| E2E-03, piloto (excluido) | 1 (excluida) | 0 | 1 (regla del cliente) | 2 | 2 | `effect_observed` 1; `indeterminate` 1 |
| E2E-03, serie | 0 | — | — | 0 | 0 | — |
| Ronda | 0 | 0 | 0 puntuados | 3 de 400 | 3 | rechazos 0 |

- **Archivo de parada:** ausente durante toda la ronda. El cliente no lo creó: ningún intento llegó a tres corridas fallidas seguidas.
- **Entradas de origen desconocido:** ninguna en el registro. `user_idle_s_at_start` del piloto fue 43,4 s, lo que coincide con el último clic del cliente, el de navegación.
- **Capturas y texto de pantalla:** no se guardó ninguno. Los registros contienen estados, códigos, contadores, tiempos e identificadores de elemento (`AutomationId`, tipo de control). Las etiquetas son las fijas del protocolo.
- **Escritorio al terminar:** tres procesos Notepad (PID 8960, 23164 y 40380); ningún proceso de la Calculadora ni de `ommatix`; las dos ventanas del propietario minimizadas y con la colocación restaurada.

### 5. Latencias (`_meta`)

Sin series puntuadas no hay medianas ni percentiles: son tres acciones y tres observaciones, así que se registran tal cual, en milisegundos.

| Acción | `action_total` | `action_verification` | `action_freshness` | `action_delivery` | `action_gate` | Cliente (rtt) |
|---|---|---|---|---|---|---|
| Navegación (cambio de modo) | 87,2 | 58,2 | 26,7 | 2,0 | 0,2 | 87,5 |
| «Editar» (piloto) | 79,5 | 58,5 | 18,8 | 1,9 | 0,3 | 79,8 |
| Cierre del menú (restauración) | 537,8 | 509,6 | 26,4 | 1,6 | 0,3 | 538,1 |

Observaciones del piloto: `total` 493,6, 370,1 y 25,5; `detect` 259,7, 121,6 y 0; `accessibility` 182,7, 185,5 y 0. RNF-05 no se evalúa con estos datos.

### 6. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor, seguridad | Piloto de E2E-03: el anclaje tras el clic en «Editar» no se confirmó y el motor informó `effect_observed`. Causa no verificada (§3.3). | Abierto. Decisión del responsable: una lectura de sólo observación del menú expandido exige un clic en «Editar» con el responsable presente; otro criterio para un elemento cuyo identificador cambia al expandirse exige especificación y prueba antes de cambiar el código. |
| 2 | Mayor, cliente, seguridad | `restore_click` no ancla por identidad (§3.4). | Abierto. Corrección propuesta: el mismo anclaje que `steps()`, antes y después del clic, con una prueba que falle sin él. No se cambió el código: el cliente está fijado a `98f0ae08…1967e5` durante la ronda. |
| 3 | Mayor, cliente | `--calc-standard` no distingue cero candidatos, varios o un objetivo no confirmado para «Estándar» (§3.1). | Abierto. Propuesta: registrar el número de candidatos y sus tipos de control, sin nombres, con una prueba. Ver el panel con el conmutador pulsado exige un clic con el responsable presente. |
| 4 | Menor, motor | El cierre del menú devolvió `indeterminate` (`INDETERMINATE_EFFECT`, verificación de 509,6 ms), aunque UI Automation lo leyó Colapsado después. | Abierto. Sin propuesta todavía: la verificación de cambio visual no confirmó el cierre, y no se conoce la causa. |
| 5 | Observación | El anclaje del conmutador «Abrir navegación» se confirmó tras el clic (un candidato, `TogglePaneButton`). Es la primera confirmación en vivo del anclaje de 1.0.20 para ese control. | No prueba el anclaje de «Editar» (hallazgo 1). Se mantiene el seguimiento del hallazgo 4 de la cuarta ronda, en lo que toca al conmutador. |

### 7. Estado en la matriz

`docs/thesis/matrices/plan_pruebas.csv` no cambia. E2E-02 y E2E-03 siguen en `NO_EJECUTADO`: no hay serie puntuada de ninguna de las dos. La confirmación del anclaje en el conmutador no basta para cambiar ningún estado. E2E-01 mantiene `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambios en esta ronda.

### 8. Qué queda y cómo reanudar

1. **Responsable:** decidir el hallazgo 1 y, si procede, autorizar la observación con su presencia. Hasta entonces no se ejecuta ninguna corrida puntuada de E2E-03.
2. **Responsable:** decidir el hallazgo 3 (qué registro necesita el paso de Estándar) y cómo llegar a Estándar si el anclaje actual no lo permite. Cambiar de modo a mano queda fuera del protocolo.
3. **Cliente, en una ronda nueva con cambio de hash:** el hallazgo 2 y, si se aprueba, el hallazgo 3, cada uno con su prueba de regresión que falle sin el cambio.
4. **Cuaderno:** 399. Cada ronda usa su propio `--round-start`.

**Ejecución.** El cliente se usó sin cambios y sin envoltorios; cada invocación en vivo comprobó antes el SHA-256 del cliente y el valor del cuaderno. Las colocaciones de las ventanas del propietario se guardaron y restauraron con un auxiliar fuera del repositorio que sólo gestiona ventanas.

## Preparación de la sexta ronda: el cliente ancla el cierre del menú, registra por qué un paso no halla objetivo y diagnostica el menú «Editar»

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-driver-grounding`, sobre `main` tras la fusión de #12 (`9a2aea8`). Sólo cambian el cliente (`tools/s4/e2e_driver.py`, SHA-256 **`6dad0dc349510027f1642df666f4d5c7495bcb73d2e9a4e87c58903e0a8e2a77`**, antes `98f0ae08…1967e5`) y sus pruebas; no cambia ninguna crate de Rust, ni se ejecutó nada en vivo: las pruebas usan un motor falso y una cadena de accesibilidad falsa, y no envían entrada. Detalle en la [revisión](../reviews/2026-10-14-s4-fifth-round.md), secciones «Resolution» y «Preparation for the sixth round».

### Qué cambió

| Hallazgo (quinta ronda, §6) | Cambio en el cliente | Prueba que falla sin el cambio |
|---|---|---|
| 2 | `restore_click` (el cierre del menú) ancla por identidad como los clics de `steps()`: antes del clic, UI Automation debe hallar exactamente un «Editar» en la ventana en primer plano (si no, no hay clic); después, el elemento bajo el cursor o su padre debe ser ese. Una discrepancia queda en `menu_restore.grounded` (falso), cuenta como `grounding_wrong` (y como `engine_false_success` si el motor informó `effect_observed`) y no va seguida de ningún reintento ni de más entrada. Se registra además `state_polls`, las lecturas del estado del menú hasta verlo Colapsado | `RestoreGroundingTest`: «Editar» no es el elemento bajo el cursor tras el clic de cierre → se informa, un solo clic; ninguno, varios, ilegible o sin identificador → ningún clic |
| 3 | Cada paso registra en `targets` cuántos candidatos tiene su etiqueta, cuántos están confirmados, sus roles, su aptitud para actuar y el origen de la etiqueta, sin nombres. Si el paso se detiene, `stopped_by` dice cuál fue la condición (`zero_candidates`, `several_candidates`, `not_confirmed`) y `uia` cuenta lo que UI Automation ve en la ventana bajo los mismos nombres (número en pantalla, fuera de pantalla y tipos de control; nunca nombres). `--calc-standard` lo vuelca en `targets` de su registro | `TargetRecordTest`: cero candidatos y dos candidatos producen registros distintos, y un candidato no confirmado, un tercero |
| 1 (preparación) | El paso del menú del Bloc de notas escribe `menu_diagnostic`: antes y después del clic, el elemento bajo el centro del objetivo y hasta 4 ancestros, con tipo de control, `AutomationId`, nombre de clase, estado de ExpandCollapse y si su `RuntimeId` es el visto antes del clic (sólo el booleano), más `after_ms`, el retardo de la lectura posterior. No cambia la puntuación ni relaja el anclaje | `MenuDiagnosticTest`: con identificadores que cambian al expandirse el menú se registran los cambios, la puntuación es idéntica con la lectura disponible, ilegible o con error, y el anclaje sigue fallando |

El hallazgo 4 (el cierre del menú `indeterminate`) se analizó desde el código y no es un defecto demostrado del motor: ver la revisión. El conflicto del protocolo entre el archivo de parada tras tres corridas fallidas y la regla de que una serie abortada por una causa propia deja correr la siguiente **no se cambió**: la revisión expone las dos opciones para el responsable.

### Decisiones del responsable (14 de octubre de 2026)

- **(a)** Las corridas de S4 usan desde ahora el manifiesto de la red en float16 de RNF-05 (`plantillas/model_manifest.rnf05.json`, sólo DirectML; [S0_RESULTADOS §10](S0_RESULTADOS.md)). El manifiesto float32 deja de usarse en S4, así que ninguna medición de rondas anteriores se mezcla con la de la sexta. Hay que tenerlo presente: con una aplicación que repinta de forma continua el float16 retrasa la lectura de accesibilidad (§10.5).
- **(b)** La enumeración de ventanas de ADR-0017 **no se cambia** para llegar a la meta de dos regiones de RNF-05. Por tanto RNF-05 sigue **sin cumplirse por unos 3 ms con dos regiones** (p95 153,0 ms frente a 150 ms; S0 §10.6).
- **(c)** El responsable estará presente para **un clic de diagnóstico, de sólo lectura, sobre «Editar» en el Bloc de notas** (hallazgo 1), después de fusionar este PR. No lo ejecutó el agente que preparó el cliente.

### Cómo se usa el registro en ese clic

Invocación (sólo con el responsable presente, el cliente de este hash comprobado antes, el cuaderno en 399 y `--round-start 399`):

`python tools/s4/e2e_driver.py --exe <ommatix.exe> --config <configuración en vivo> --app notepad --mode run --runs 1 --max-live 2 --ledger <cuaderno> --round-start 399 --results <JSONL>`

Gasta **dos** solicitudes reales: el clic de diagnóstico sobre «Editar» (abre el menú) y el clic que lo cierra (el segundo paso del flujo, o la restauración si el primero falló). **No usar `--max-live 1`:** la restauración llegaría al límite y crearía `stop.flag`. La parada la crea el cliente y la retira sólo el responsable.

El registro de la corrida (`menu_diagnostic`, también en la salida estándar como evento `menu_diagnostic`) contiene, para el clic de abrir, `before` y `after`: hasta cinco filas (`depth` 0 es el elemento bajo el punto) con `control_type`, `automation_id`, `class_name`, `expand_state` y `same_runtime_id`, y `after_ms`. Con él se distinguen las dos explicaciones del hallazgo 1:

- **(a) El identificador cambia al expandirse:** tras el clic, la fila 0 sigue siendo `MenuItem` con `AutomationId` `Edit`, `expand_state` `Expanded` y `same_runtime_id` falso (o la fila 1, el padre). El clic llegó a «Editar»; falta especificar otra regla para un elemento cuyo identificador cambia, y probarla, antes de cambiar el anclaje.
- **(b) El clic llegó a otro control:** la fila 0 tras el clic es de otro tipo o de otro `AutomationId`, ninguna fila es «Editar», y el estado de ExpandCollapse de «Editar» no pasa a `Expanded`. El anclaje funciona como debe y el fallo está en el punto del clic.

El registro no contiene nombres, valores ni texto de pantalla. Una lectura que falla queda como `null` y no cambia el resultado de la corrida. La corrida sigue puntuándose como siempre: con el identificador cambiado, `grounding_wrong` es 1 y es la **observación**, no una corrida de la serie.

### Estado de las pruebas y verificación

`python -m unittest discover -s tools/s4 -p "test_*.py"`: 81 pruebas correctas (62 anteriores y 19 nuevas); las nuevas sin el cambio del cliente dan 16 fallos o errores. Las demás verificaciones, en la revisión.

## Sexta ronda (diagnóstico): una corrida de sólo lectura sobre el menú «Editar» del Bloc de notas

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-sixth-round-diagnostic`, desde `main` (`7c15817`, fusión de #13). No cambia ningún código ni el cliente. Se usó el cliente comprometido (`tools/s4/e2e_driver.py`, SHA-256 **`6dad0dc349510027f1642df666f4d5c7495bcb73d2e9a4e87c58903e0a8e2a77`**, comprobado justo antes de la invocación), el binario de `main` (`target/release/ommatix.exe`, SHA-256 **`11a3d7921cbe1582739a068714435ea380eb1e70fc460fe82b00c0a410aca9da`**) y la configuración en float16 (`s4r6-live.toml`, SHA-256 `b6e9e8e8c9cec2324fa218645de1843bce58148c2cefe384f32956f76fe4e74c`). Autorización del responsable: una corrida del flujo del menú con el registro de diagnóstico, con dos solicitudes reales como máximo y sin nada más. El responsable estuvo presente y no usó el equipo durante la corrida.

**Resultado:** una corrida de diagnóstico, no puntuada. Dos solicitudes reales, las dos entregadas y ninguna rechazada; cuaderno de 399 a 401. E2E-02 y E2E-03 siguen `NO_EJECUTADO`. El registro **no respalda** ni la explicación (a) ni la (b) del hallazgo 1 tal como estaban formuladas: el identificador de «Editar» no cambia al expandirse, y la consulta de UI Automation por posición no devuelve «Editar» ni antes ni después del clic. Ver §4.

### 1. Verificación previa a la entrada en vivo

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia` sobre `main`: correcto, sin avisos.
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: 351 pasadas, 0 fallidas, 1 ignorada.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 81 pruebas correctas.
- Configuración: copia de `s4r5-live.toml` con dos líneas cambiadas. `detector_manifest` apunta a `models\manifest.rnf05.json` y `detector_manifest_sha256` es `f8b04be9…7fe47`, comprobado en disco antes de la corrida. El resto no cambia (DirectML, `accessibility = "uia"`, OCR de Windows en es-ES, `stop_file` y lista de aplicaciones).
- Cuaderno en 399 (`--round-start 399`), `stop.flag` ausente y ningún proceso `ommatix.exe`.
- Escritorio: dos ventanas del Bloc de notas, ambas minimizadas (`showCmd` 2). Su colocación se guardó antes de la corrida con un auxiliar fuera del repositorio que sólo gestiona ventanas, y coincide con la de la quinta ronda. Tres procesos `notepad.exe` (PID 8960, 23164 y 40380).

### 2. Ejecución en vivo

Una sola invocación, sin envoltorios ni cambios en el cliente:

`python tools/s4/e2e_driver.py --exe <ommatix.exe> --config <s4r6-live.toml> --app notepad --mode run --runs 1 --max-live 2 --ledger <ledger.txt> --round-start 399 --results <s4r6.diagnostic.jsonl>`

La salida estándar y la de error van a `s4r6-diagnostic.log`. Código de salida del cliente y del servidor: 0.

| Solicitud | Qué era | Estado del motor | Entregada | `action_verification` (ms) | `action_total` (ms) |
|---|---|---|---|---|---|
| 1 | clic en «Editar» (abrir el menú) | `effect_observed` (`visual_change`) | sí | 113,7 | 122,3 |
| 2 | clic de restauración en «Editar» (cerrar el menú) | `effect_observed` | sí | 67,4 | 94,6 |

- **Cuaderno:** 399 → 401. Cupo de la ronda: 2 de 400. Contadores del motor: entregadas +2, rechazadas +0.
- **Cliente:** `success` falso. Fallo: «paso 1: el control bajo el cursor no es el elemento hallado antes del clic». `grounding_wrong` 2 y `engine_false_success` 2, porque el anclaje por posición falló en las dos solicitudes, que el motor informó como `effect_observed`. No hubo reintento ni entrada adicional.
- **Menú «Editar»** (UI Automation, por nombre, estado ExpandCollapse): **Colapsado** al empezar; **Expandido** tras la solicitud 1; **Colapsado** tras la solicitud 2, en la primera lectura.
- **Inactividad del propietario al empezar:** 258,8 s.

### 3. Registro de diagnóstico (`menu_diagnostic`, sin nombres)

Hubo un registro, del clic 1. Las cadenas son las del centro de «Editar» antes del clic (`before`) y las del cursor después del clic (`after`, leída 735 ms después de la respuesta del motor). Las dos son idénticas:

| Fila | Tipo de control | AutomationId | Clase | Estado ExpandCollapse | Mismo RuntimeId que «Editar» |
|---|---|---|---|---|---|
| 0 | Pane | (vacío) | `Microsoft.UI.Content.DesktopChildSiteBridge` | (ninguno) | no |
| 1 | Window | (vacío) | `Notepad` | (ninguno) | no |
| 2 | Pane | (vacío) | `#32769` | (ninguno) | no |

«Editar» no aparece en ninguna fila de ninguna de las dos cadenas.

Identidad de «Editar» (campos `identity` y `menu_restore.identity`; el valor del RuntimeId no se transcribe, sólo se compara):

| Momento | Tipo de control | AutomationId | RuntimeId | Candidatos |
|---|---|---|---|---|
| Antes del clic 1 (menú Colapsado) | MenuItem | `Edit` | valor A | 1 |
| Antes de la solicitud 2 (menú Expandido) | MenuItem | `Edit` | valor A, igual | 1 |

### 4. Conclusión

- **(a) El identificador cambia al expandirse: refutada en esta corrida.** La explicación predice que «Editar» aparece tras el clic con un RuntimeId distinto. No aparece en ninguna cadena, y con el menú Expandido UI Automation encuentra el mismo elemento (tipo `MenuItem`, `AutomationId` `Edit`) y el mismo RuntimeId que antes del clic.
- **(b) El clic llegó a otro control: no respaldada.** Sólo se cumple la mitad: ninguna fila tras el clic es «Editar». La otra mitad no se cumple: el estado de «Editar» pasó a Expandido y después a Colapsado. En este flujo no se envió ninguna otra entrada, así que sólo un clic sobre ese elemento puede producir esas transiciones.
- **Lo que el registro sí muestra: la consulta por posición no devuelve el elemento.** Antes y después del clic, UI Automation devuelve la isla XAML en el centro de «Editar» y bajo el cursor. El anclaje «el elemento bajo el cursor o su padre» no puede confirmar «Editar» con esa consulta, sea cual sea el destino real del clic, y esa cadena explica el fallo de anclaje de las dos solicitudes. La quinta ronda sólo registró el booleano, así que es la primera vez que se ve la cadena.
- **Veredicto: inconcluso sobre el punto exacto que recibió el clic.** Es concluyente en dos cosas: el identificador no cambia al expandirse, y la consulta por posición no puede confirmar «Editar» en este Bloc de notas. El cambio de estado del propio «Editar», que es lo único que cambió, es coherente con que los dos clics lo activaran.
- **Consecuencia para el recuento de falsos éxitos.** Según este registro, los dos `effect_observed` del motor coinciden con los cambios de estado del menú. Los falsos éxitos que cuenta el cliente proceden del anclaje por posición, que no puede confirmar este elemento. No se ha demostrado un falso éxito del motor. El fallo quedó cerrado como se diseñó: sin reintento ni entradas adicionales.

### 5. Totales y seguridad

| Serie | Corridas puntuadas | Solicitudes reales | Entregadas | Estados del motor |
|---|---|---|---|---|
| Diagnóstico E2E-03 (no puntuado) | 0 | 2 | 2 | `effect_observed` 2 |
| Ronda | 0 | 2 de 400 | 2 | rechazos 0 |

- **Archivo de parada:** ausente antes y después. El cliente no lo creó, porque una corrida fallida no llega a tres.
- **Procesos:** ningún `ommatix.exe` en ejecución al terminar. Los procesos de Python son los mismos 44 de antes de la corrida, sin ninguno nuevo. Los del Bloc de notas no cambiaron (PID 8960, 23164 y 40380): ninguno se inició ni se cerró.
- **Ventanas del propietario:** ambas, minimizadas y con la colocación guardada. La verificación coincidió antes y después de la restauración explícita, hecha con el auxiliar fuera del repositorio (sólo colocación, sin texto ni entradas). Al terminar, el cliente volvió a minimizar la ventana que había usado, como hace con una ventana que estaba minimizada.
- **Menú «Editar» al terminar:** Colapsado en las dos ventanas, según una lectura de UI Automation sólo de lectura.
- **Entradas de origen desconocido:** el registro de inicio marca 258,8 s de inactividad, coherente con la última entrada del propietario antes de la solicitud. El registro no muestra otra entrada, aunque no puede descartar del todo las que no dejan rastro.
- **Capturas y texto de pantalla:** ninguno guardado. El registro contiene estados, códigos, contadores, tiempos, tipos de control, `AutomationId`, nombres de clase y booleanos. Ningún nombre ni valor.

### 6. Latencias (`_meta`; valores crudos de una corrida no puntuada)

| Acción | `action_total` | `action_verification` | `action_freshness` | `action_delivery` | `action_gate` | Cliente (rtt) |
|---|---|---|---|---|---|---|
| Clic «Editar» (abrir) | 122,3 | 113,7 | 5,6 | 2,4 | 0,6 | 122,7 |
| Restauración (cerrar) | 94,6 | 67,4 | 25,1 | 1,8 | 0,3 | 94,9 |

Observaciones (`total` / `detect` / `accessibility`, ms): 257,8 / 84,7 / 250,9; 228,7 / 81,3 / 215,8; y 28,8 / 0 / 0, en la que no hubo reanálisis (ni detección, ni OCR, ni accesibilidad). RNF-05 no se evalúa con una corrida no puntuada.

### 7. Hallazgos

| # | Severidad | Estado tras la sexta ronda |
|---|---|---|
| 1 | Mayor, seguridad | Abierto. La explicación (a) queda refutada y (b) no tiene respaldo; la consulta por posición no devuelve «Editar». Decisión del responsable (§9.1). |
| 2 | Mayor, cliente, seguridad | Resuelto en el cliente (1.0.23) y ejercitado en vivo por primera vez: el cierre se ancla por identidad, la discrepancia quedó registrada y no hubo reintento. |
| 3 | Mayor, cliente | Sin cambio: el paso «Estándar» no se ejecutó en esta ronda. |
| 4 | Menor, motor | Sin cambio. Esta vez el cierre devolvió `effect_observed` y el menú se leyó Colapsado en la primera lectura; en la quinta ronda devolvió `indeterminate`. |

### 8. Estado en la matriz

Sin cambio. Esta corrida no es puntuada: `docs/thesis/matrices/plan_pruebas.csv` no cambia, E2E-02 y E2E-03 siguen `NO_EJECUTADO` y E2E-01 mantiene `EJECUTADO_SUPERVISADO_PARCIAL`.

### 9. Qué queda y cómo reanudar

1. **Responsable (hallazgo 1), decisión previa a cualquier otra corrida de E2E-03.** Con la regla actual, «Editar» no puede confirmarse por posición en este Bloc de notas, así que la serie puntuada no puede empezar. Hay dos caminos: (i) mantener la regla, y E2E-03 queda sin ejecutar; (ii) que el propio elemento dé fe del clic, leyendo su RuntimeId tras el clic desde su árbol de UI Automation y exigiendo que su estado ExpandCollapse sea el esperado. (ii) cambia la regla de anclaje para este control: necesita una especificación escrita, una prueba que falle sin ella y una ronda nueva con cambio de hash del cliente. **Recomendación:** (ii) sólo con esa especificación; mientras tanto, (i). **Decidido el 14 de octubre de 2026:** el responsable eligió descender dentro de la isla (ni (i) ni (ii)); ver «Preparación de la séptima ronda».
2. **Cliente:** sin cambios en esta ronda. La regla de anclaje y el cliente se quedan como están.
3. **Cuaderno:** 401. La siguiente ronda usa `--round-start 401`.
4. **Ninguna solicitud en vivo más** sin una nueva autorización del responsable.

**Ejecución.** Las únicas entradas de la corrida fueron las dos del cliente. Guardar y restaurar la colocación de las ventanas es gestión de ventanas, sin entradas, y las lecturas de UI Automation posteriores son de sólo lectura.

## Preparación de la séptima ronda: el cliente ancla los clics dentro de una isla XAML

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-island-grounding`, sobre `main` tras la fusión de #14 (`9fb4748`). Sólo cambian el cliente (`tools/s4/e2e_driver.py`, SHA-256 **`60486d87ea06313fd687ea1bd64ef913795cadf6b6007cc4df7764ce3b5c214a`**, antes `6dad0dc3…8e2a77`) y sus pruebas; no cambia ninguna crate de Rust, ni se ejecutó entrada en vivo: las pruebas usan un motor falso y una cadena de accesibilidad falsa. La regla, escrita antes del código, y la revisión del hallazgo 1 están en la [revisión](../reviews/2026-10-14-s4-fifth-round.md), sección «Seventh round: grounding inside a XAML island».

### Decisión del responsable (14 de octubre de 2026)

Entre las dos opciones del §9.1 de la sexta ronda, el responsable eligió **«descender dentro de la isla»**: no se relaja el anclaje por identidad ni se ancla por cambio de estado; se especifica cómo se obtiene el elemento bajo el punto cuando la consulta por posición devuelve el anfitrión de una isla XAML. La opción (i), mantener la regla, queda descartada.

### La regla

1. **Cuándo.** El primer elemento de la cadena de la consulta por posición (`FromPoint`) tiene una clase de la lista cerrada y explícita `ISLAND_HOST_CLASSES`, que hoy contiene sólo `Microsoft.UI.Content.DesktopChildSiteBridge`. Con cualquier otra clase, o con una cadena vacía, el comportamiento no cambia: la cadena de la consulta es la cadena de anclaje y no se recorre nada.
2. **El descenso.** El cliente recorre el subárbol de la vista de control (ControlView) de ese anfitrión, en profundidad, y lee de cada elemento su RuntimeId, `AutomationId`, tipo de control, clase, `IsOffscreen` y `BoundingRectangle`; nunca un nombre, un valor ni un patrón. El recorrido está acotado a **500 elementos por debajo del anfitrión y 16 niveles** (`DESCENT_MAX_ELEMENTS` y `DESCENT_MAX_DEPTH`; los dos son [P] y se registran en cada descenso). El anfitrión se vuelve a leer por el punto al empezar y debe conservar el RuntimeId de la consulta, o el recorrido no lee nada.
3. **El resultado.** De los elementos en pantalla por debajo del anfitrión cuyo `BoundingRectangle` contiene el punto, **el del nivel más profundo**. La cadena es ese elemento seguido de sus ancestros hasta el anfitrión, y `identity_hit` se aplica a ella **sin cambios**: el elemento o su padre deben tener el RuntimeId esperado, o el `AutomationId` esperado sólo cuando falta el RuntimeId en alguno de los dos lados.
4. **No se confirma nada** (la cadena queda vacía y el clic no está anclado) cuando: dos o más elementos contienen el punto en el nivel más profundo (los de un mismo nivel nunca están anidados entre sí, así que son ambiguos); el recorrido llegó a una cota (también si las filas superan la cota, diga lo que diga el lector); el recorrido no se pudo leer o está mal formado; el anfitrión cambió; o ningún elemento por debajo del anfitrión contiene el punto.
5. **Lo que no cambia.** No hay anclaje por cambio de estado (el estado ExpandCollapse del menú sigue siendo sólo la comprobación del lado del sistema operativo), ni comparación de nombres, ni reintento. Un descenso fallido es un fallo de anclaje igual que una discrepancia: `grounding_wrong` (y `engine_false_success` si el motor informó `effect_observed`), la corrida termina y no se envía más entrada.
6. **Dónde se aplica.** En todos los lugares donde el cliente ancla un clic por posición: `steps()`, `restore_menu` (el clic que cierra el menú) y el diagnóstico del menú.

### Registro (sin nombres)

Cada descenso deja `descended`, `status` (`ok`, `not_found`, `ambiguous`, `truncated`, `host_changed`, `unreadable`), `depth`, `visited`, `truncated`, `ambiguous`, `max_elements` y `max_depth`, y, si halló un elemento, su `control_type` y `automation_id`. Una consulta que no es una isla deja `{descended: false}`. Está en `descent` de la corrida (uno por clic anclado, con su `step`), en `menu_restore.descent` y en `menu_diagnostic` como `before_descent` y `after_descent`; las filas `before` y `after` del diagnóstico siguen siendo la cadena cruda de la consulta, así que el anfitrión de la isla sigue viéndose en ellas.

### Pruebas

`python -m unittest discover -s tools/s4 -p "test_*.py"`: **113 pruebas correctas** (81 anteriores, sin cambios, y 32 nuevas). Con el cliente de la sexta ronda las 32 nuevas fallan (17 fallos y 15 errores). Cubren: (a) la consulta devuelve la isla y el descenso halla «Editar» (`MenuItem`, `AutomationId` `Edit`, RuntimeId esperado) → anclado, en una corrida de dos clics y en el clic de cierre; (b) el descenso halla otro control → no anclado y sin reintento; (c) dos elementos no anidados que contienen el punto → ambiguo, aunque uno sea el elemento; (d) se llega a la cota → no anclado aunque se haya visto el elemento; (e) una cadena que no es de una isla se comporta como antes y no se recorre; (f) los registros no contienen nombres.

### Comprobación de sólo lectura sobre el Bloc de notas del responsable (sin entrada)

Se hizo una vez, sin clics, sin enfocar, sin restaurar y sin escribir. Las dos ventanas del Bloc de notas estaban minimizadas, así que no se pudo usar la consulta por posición (el elemento está en torno a `-31915,-31960`): se llegó al anfitrión de la isla desde los dos elementos «Editar» hallados por nombre, y se aplicaron el mismo script de recorrido y la misma regla (`descend`) con el centro del elemento como punto. Resultado, en cada ventana: la clase del anfitrión está en la lista; 20 elementos por debajo, nivel más profundo 3, sin truncar (lejos de 500 y 16); tipos bajo el anfitrión: `Button` 12, `MenuItem` 3, `Pane` 3, `MenuBar` 1, `Window` 1; 17 en pantalla y 3 que contienen el centro (el elemento, la barra de menús y un panel); el descenso termina `ok` en el nivel 3 con un `MenuItem`, `AutomationId` `Edit` y **el mismo RuntimeId que el elemento hallado por nombre**; la cadena es `MenuItem Edit → MenuBar → Pane → Pane (anfitrión)`. Lo que esto **no** demuestra: que la consulta por posición devuelva el anfitrión en vivo (las ventanas están minimizadas y no se podía traer ninguna al frente) ni el tiempo del recorrido con la ventana al frente.

### Qué debe mostrar la próxima ronda en vivo

Un piloto de Notepad con `--max-live 2` y el responsable presente debe registrar, para el clic de apertura y para el de cierre, `descent.status` `ok`, profundidad 3 y `automation_id` `Edit`, `grounding_wrong` 0 y el menú `Collapsed → Expanded → Collapsed`. Cualquier otra cosa es un hallazgo. Un recorrido de hasta 500 elementos por UI Automation tarda un tiempo que esta rama no ha medido en vivo; `after_ms` del diagnóstico lo registrará la primera vez. El hash del cliente cambia, así que la próxima ronda empieza con `--round-start 401` y la comprobación del SHA-256 de arriba.

### Estado

Hallazgo 1: regla especificada e implementada en el cliente, **sin confirmar en vivo**. Hallazgos 2 a 4 y el conflicto de `stop.flag`: sin cambio. E2E-02 y E2E-03 siguen `NO_EJECUTADO`; las matrices no cambian.

## Séptima ronda: piloto aprobado y nueve corridas puntuadas de diez de E2E-03; el cupo cortó la serie y E2E-02 no se ejecutó

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-seventh-round`, desde `main` (`7bca432`, fusión de #15). No cambia ningún código ni el cliente. Se usó el cliente comprometido (`tools/s4/e2e_driver.py`, SHA-256 `60486d87ea06313fd687ea1bd64ef913795cadf6b6007cc4df7764ce3b5c214a`, comprobado justo antes de cada una de las dos invocaciones en vivo), el binario de `main` (`target/release/ommatix.exe`, SHA-256 `11a3d7921cbe1582739a068714435ea380eb1e70fc460fe82b00c0a410aca9da`, el mismo de la sexta ronda: desde entonces `main` no ha cambiado ningún crate de Rust) y la configuración en float16 `s4r7-live.toml` (copia sin cambios de `s4r6-live.toml`, SHA-256 `b6e9e8e8c9cec2324fa218645de1843bce58148c2cefe384f32956f76fe4e74c`). Autorización del responsable del 14 de octubre de 2026: un piloto excluido y diez corridas puntuadas de E2E-03; E2E-02 sólo si el modo Estándar se alcanzaba de forma segura; como máximo 150 solicitudes reales en la ronda; el responsable no usó el equipo.

**Resultado:** el piloto de E2E-03 (excluido de la puntuación) pasó: el descenso por la isla halló «Editar» en los dos clics (`ok`, profundidad 3, `MenuItem`, `AutomationId` `Edit`), no hubo anclajes erróneos y el menú pasó de Colapsado a Expandido y otra vez a Colapsado. La serie puntuada completó nueve de sus diez corridas, todas exitosas, con 0 falsos éxitos y 0 anclajes erróneos. La corrida 10 no se inició: su primera solicitud superó el cupo de la serie (`--max-live 20`), que no reservaba solicitudes para los reintentos tras `STALE_SNAPSHOT`; dos corridas los necesitaron. El cliente creó `stop.flag` antes de enviar esa solicitud. Como el archivo existe, no se ejecutaron `--calc-standard` ni E2E-02. Las matrices no cambian.

### 1. Verificación previa a la entrada en vivo

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: ya al día, sin avisos.
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: 351 pasadas, 0 fallidas, 1 ignorada.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 113 pruebas correctas.
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48 correctas; `validar_paquete.py`, 211 comprobaciones y 0 fallidas. Cuatro archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y el fin de línea; se restauraron con `git restore`.
- Manifiesto float16 `models\manifest.rnf05.json`: SHA-256 `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`, comprobado en disco.
- Cuaderno en 401 (`--round-start 401`), `stop.flag` ausente, sin procesos `ommatix.exe`, `CalculatorApp.exe` ni `ommatix-testapp.exe`.
- Escritorio: dos ventanas del Bloc de notas, ambas minimizadas (`showCmd` 2). Su colocación se guardó antes del piloto con `notepad_placement.ps1`, un auxiliar fuera del repositorio que sólo lee y restaura posiciones. Dos procesos `notepad.exe` (PID 8960 y 23164). El PID 40380 de la sexta ronda ya no existía al empezar esta ronda; el cliente no inicia ni cierra procesos de Notepad.

### 2. Ejecución en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| E2E-03, piloto (excluido) | `--app notepad --mode run --runs 1 --max-live 2 --round-start 401` | 2 | Aprobado |
| E2E-03, serie | `--app notepad --mode run --runs 10 --max-live 20 --round-start 401` | 20 | Nueve corridas puntuadas, nueve exitosas; la corrida 10 no se inició |
| `--calc-standard` | no se ejecutó | 0 | Bloqueado por `stop.flag` |
| E2E-02 | no se ejecutó | 0 | Bloqueado por `stop.flag` |

Cuaderno: 401 → 423, es decir, 22 solicitudes reales de 150 en la ronda (2 del piloto, 20 de la serie). Ninguna solicitud se envió después de que el cliente creara `stop.flag`.

### 3. Resultados

#### 3.1 Piloto (excluido de la puntuación)

- Corrida 1: dos clics entregados, ambos `effect_observed` y verificados por el motor. El descenso desde el anfitrión de la isla halló «Editar» (`MenuItem`, `AutomationId` `Edit`, profundidad 3, 11 elementos visitados) en los dos clics; la identidad fue `ok` en los dos, `grounding_wrong` 0 y falsos éxitos 0.
- Menú «Editar» (UI Automation): Colapsado al empezar, Expandido tras el clic 1 y Colapsado tras el clic 2. Las dos comprobaciones del sistema operativo pasaron (2 de 2).
- La ventana reutilizada se minimizó al terminar, como hace el cliente con una ventana que estaba minimizada.
- Inactividad del propietario al empezar: 788,5 s.

#### 3.2 E2E-03, serie puntuada (corridas 1 a 9)

| Corrida | Solicitudes | Entregadas | Rechazadas (`STALE_SNAPSHOT`) | Éxito |
|---|---|---|---|---|
| 1 | 2 | 2 | 0 | sí |
| 2 | 3 | 2 | 1 (paso 2; el reintento se entregó) | sí |
| 3 a 7 | 2 cada una | 2 cada una | 0 | sí |
| 8 | 3 | 2 | 1 (paso 2; el reintento se entregó) | sí |
| 9 | 2 | 2 | 0 | sí |
| 10 | 0 | 0 | — | no iniciada |

Totales: 9 corridas puntuadas y 9 exitosas. 20 solicitudes reales, 18 entregadas (todas `effect_observed`, verificadas) y 2 rechazadas antes de la entrega (`STALE_SNAPSHOT` en el paso de cierre de las corridas 2 y 8; no entregaron nada, porque el motor revalida la captura antes de cada entrega). `indeterminate`: 0. Restauraciones del menú: 0. Falsos éxitos: 0. `grounding_wrong`: 0. Comprobaciones del sistema operativo: 18 de 18.

Anclaje por identidad: 20 de 20 intentos hallaron «Editar» como único candidato con su `RuntimeId`, incluidos los dos rechazados. Descenso: 18 de 18 `ok`, profundidad 3, `MenuItem`, `AutomationId` `Edit`, con 20 elementos visitados en cada uno; las cotas (500 y 16) no se alcanzaron. Clics anclados: 18 de 18. Menú Colapsado al empezar en las nueve corridas.

Inactividad al empezar: corrida 1, 28,9 s (tras el piloto); corridas 2 a 9, entre 2,6 y 2,8 s, que corresponden a los clics del propio cliente. Ningún dato indica entradas de origen desconocido.

#### 3.3 Corte del cupo y `stop.flag`

- La corrida 10 empezó y su primera solicitud superó el cupo de la serie (20 de 20). El cliente registró `operator_stop_engaged` con el motivo «live action budget exhausted», creó `stop.flag` (18:02:21, hora local, según la fecha de modificación del archivo) y abortó. No se envió ninguna entrada para la corrida 10.
- **Causa.** El cupo se calculó como dos solicitudes por corrida. Cada intento repetido tras `STALE_SNAPSHOT` también consume una solicitud del cuaderno. Dos corridas necesitaron un reintento, y eso agotó el cupo antes de la décima. Es el hallazgo 1 de la [revisión](../reviews/2026-10-14-s4-seventh-round.md).
- **Vigilancia.** Un script auxiliar, fuera del repositorio, vigilaba el archivo de resultados y detenía el proceso del cliente sólo ante un evento de seguridad (`grounding_wrong` o `engine_false_success`) o si existía `stop.flag`. La parte de `stop.flag` sobraba, porque el cliente ya se detiene con él. Se disparó justo cuando el cliente creaba el archivo, mientras aún cerraba: el proceso terminó antes de escribir su registro `end` y antes de cerrar el servidor. Comprobado después: ningún `ommatix.exe`, ventanas minimizadas en su colocación y menú Colapsado. Hallazgo 2 de la revisión (de operación; no es un defecto del cliente).
- **`stop.flag` sigue presente.** Lo creó el cliente, no el agente, y no se ha retirado. Retirarlo es decisión del responsable.

#### 3.4 `--calc-standard` y E2E-02

No se ejecutaron, porque el protocolo ordena detener todo el trabajo en vivo mientras exista `stop.flag`. No hay datos de la transición a Estándar ni de la Calculadora en esta ronda. Los `targets` de E2E-03 sí se registraron; los campos `targets` y `stopped_by` de `--calc-standard` no se han probado en vivo.

### 4. Latencias (`_meta`)

| Medida (ms) | Piloto (2 clics) | Serie: mediana / p95 (18 clics entregados) |
|---|---|---|
| `action_total` | 67,5 y 75,8 | 86,0 / 97,3 |
| `action_verification` | 58,4 y 67,5 | 67,6 / 69,7 |
| `action_freshness` | 5,8 y 5,9 | 16,2 / 25,8 |
| `action_delivery` | 2,8 y 1,9 | 2,2 / 5,3 |
| `action_gate` | 0,5 y 0,5 | 0,2 / 0,5 |
| rtt del cliente | 67,9 y 76,1 | 86,2 / 97,6 |

Observaciones de la serie (29, mediana / p95): `total` 130,0 / 144,9; `detect` 83,8 / 87,0; `recognize` 44,1 / 56,1; `accessibility` 114,3 / 136,7; `capture` 11,3 / 23,7; `plan` 2,0 / 2,8; `track` 0,7 / 1,2; `fuse` 0,1 / 0,1. Duración de cada corrida (nueve): mediana 8,9 s, p95 11,1 s. RNF-05 no se evalúa con estas corridas de flujo, que no son la medición de S0 §10.

### 5. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor, planificación | El cupo de la serie (`--max-live 20` para diez corridas) no reservaba solicitudes para los reintentos tras `STALE_SNAPSHOT`. Dos corridas los usaron y el cupo se agotó antes de la corrida 10 (§3.3). | Abierto, decisión del responsable (§8). Sin cambio de código: el cliente se detuvo como está diseñado, antes de enviar la solicitud 21 de la serie (la 23 de la ronda). |
| 2 | Menor, operación | El script de vigilancia detuvo el proceso del cliente mientras cerraba (§3.3); su registro `end` y el cierre del servidor no quedaron escritos. | Cerrado para esta ronda: se comprobó el estado del escritorio y ningún proceso quedó vivo. El script no se reutiliza en esta forma: no detiene el proceso por `stop.flag`. Sin cambio en el repositorio. |
| 3 | Observación | `STALE_SNAPSHOT` en el paso de cierre de las corridas 2 y 8, antes de la entrega; el reintento volvió a anclarse y se entregó. | Sin cambio. Causa no verificada: el registro no muestra qué cambió en la pantalla entre la observación y la acción. |
| 4 | Observación | El descenso visitó 11 elementos en el piloto y 20 en la serie. | Sin cambio. Causa no verificada. Las cotas no se alcanzaron y el resultado fue el mismo (profundidad 3, `Edit`). |
| 5 | Documental (preexistente) | `plan_pruebas.csv` describe E2E-02 como «Editor» y E2E-03 como «Explorador»; el protocolo S4 ejecuta la Calculadora y el Bloc de notas bajo esos identificadores. | Sin cambio. Decisión del responsable sobre la correspondencia. |

### 6. Estado en la matriz

Sin cambio. `docs/thesis/matrices/plan_pruebas.csv` no se modifica: E2E-03 tiene nueve de las diez corridas puntuadas que exige cada serie, E2E-02 no se ejecutó, y la matriz describe E2E-02 y E2E-03 como «Editor» y «Explorador», no como la Calculadora y el Bloc de notas (hallazgo 5). Las columnas siguen igual: `NO_EJECUTADO` en la de referencia y `PENDIENTE_PRODUCTO` en la de aceptación. E2E-01 mantiene `EJECUTADO_SUPERVISADO_PARCIAL`.

### 7. Estado final del escritorio y del cuaderno

- **Bloc de notas:** procesos 8960 y 23164, los mismos que al empezar. Las dos ventanas del propietario están minimizadas, con la colocación guardada antes del piloto; la restauración y su verificación coinciden con `notepad_placement_before_r7.json`. El menú «Editar» está Colapsado en las dos (lectura de UI Automation, sólo lectura).
- **Procesos:** ningún `ommatix.exe`, `CalculatorApp.exe` ni `ommatix-testapp.exe`, y ningún proceso del cliente.
- **`stop.flag`:** presente (creado por el cliente a las 18:02:21). No se ha retirado.
- **Cuaderno:** 423 (22 de 150 solicitudes de la ronda).
- **Capturas y texto de pantalla:** ninguno guardado. Los registros contienen estados, códigos, contadores, tiempos, tipos de control, `AutomationId` y clases de ventana.

### 8. Qué queda y cómo reanudar

1. **Responsable:** decidir si la ronda continúa. Si continúa, retirar `stop.flag` a mano. El agente no lo crea ni lo retira.
2. **Responsable:** fijar el cupo contando los reintentos. Propuesta, con 128 solicitudes restantes de 150 y `--round-start 401`: la corrida 10 de E2E-03 en su propia invocación (`--runs 1 --first-run 10 --max-live 6`); `--calc-standard` con su cupo de 4; E2E-02, piloto `--max-live 10` y serie `--max-live 75`. Son 95 solicitudes como máximo; el caso típico, sin reintentos, son 59. El cupo de la serie admite 25 reintentos sobre sus 50 clics típicos. El peor caso, con cuatro intentos por clic, supera las 128, así que el corte puede repetirse: el responsable decide si lo acepta.
3. **Cliente:** sin cambios en esta ronda. Contar los reintentos de forma explícita en el cupo es un cambio de protocolo y de cliente: necesita su prueba y una ronda nueva con cambio de hash.
4. **Vigilancia:** la siguiente ronda no usa el script de esta forma. Si se vigila el proceso, sólo se informa: el cliente se detiene solo.
5. **Cuaderno:** 423. Cada ronda usa su propio `--round-start`.

### 9. Ejecución

El cliente se usó sin cambios y sin envoltorios que envíen entrada. Cada invocación en vivo comprobó antes el SHA-256 del cliente. Las únicas entradas fueron las 22 solicitudes de `execute_action` sobre «Editar» en el Bloc de notas (`MenuItem`, `AutomationId` `Edit`); no se escribió nada en el Bloc de notas ni se usó ninguna otra aplicación. Guardar y restaurar la colocación de las ventanas es gestión de ventanas, sin entradas, y las lecturas de UI Automation son de sólo lectura.

## Séptima ronda (continuación): corrida 10 de E2E-03 y `--calc-standard`; E2E-02 no se ejecutó

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-seventh-round-continuation`, desde `main` (`885cda7`, fusión de #16). No cambia ningún código: ni el cliente ni las crates de Rust. Se usaron el cliente comprometido (`tools/s4/e2e_driver.py`, SHA-256 `60486d87…c214a`, comprobado antes de cada invocación en vivo), el binario de `main` (`target/release/ommatix.exe`, SHA-256 `11a3d792…aca9da`, el mismo de la séptima ronda) y la configuración `s4r7-live.toml` (SHA-256 `b6e9e8e8…e74c`, sin cambios). Autorización del responsable del 14 de octubre de 2026: la corrida 10 de E2E-03 en su propia invocación (`--runs 1 --first-run 10 --max-live 6`); `--calc-standard` una sola vez, con un máximo de 4 solicitudes; E2E-02 sólo si el modo Estándar se alcanzaba de forma segura (un piloto con `--max-live 10` y una serie con `--max-live 75`); como máximo 150 solicitudes en la ronda, con `--round-start 401`; el responsable no usó el equipo.

**Resultado:** la corrida 10 de E2E-03 pasó. La serie puntuada de E2E-03 queda completa: diez corridas puntuadas, todas exitosas, con 0 falsos éxitos y 0 anclajes erróneos. `--calc-standard` **no alcanzó el modo Estándar**: el clic sobre «Abrir navegación» se entregó y quedó anclado por identidad, pero «Estándar» no apareció en UI Automation y el paso se detuvo sin segundo clic. Por eso E2E-02 no se ejecutó, ni piloto ni serie. No queda ningún proceso del cliente, `stop.flag` no existe y el cuaderno está en 426 (25 de 150 solicitudes de la ronda).

### 1. Verificación previa a la entrada en vivo

Antes de la entrada en vivo se repitieron las comprobaciones de la séptima ronda sobre `885cda7`, sin cambios de código:

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: ya al día; el binario conserva el SHA-256 `11a3d792…aca9da`.
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: 351 pasadas, 0 fallidas, 1 ignorada.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 113 pruebas correctas.
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48 correctas; `validar_paquete.py`, 211 comprobaciones y 0 fallidas. Cuatro archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y el fin de línea; se restauraron con `git restore`.
- `stop.flag`: lo creó el cliente en la primera parte de la ronda. El responsable autorizó retirarlo para esta ronda y se retiró antes de la corrida 10; la copia queda como `stop.flag.r7-removed-by-coordinator-with-owner-authorization`. El archivo no existía al empezar esta continuación.
- Antes de cada invocación en vivo se comprobaron por SHA-256 el cliente, la configuración, el binario y el manifiesto float16 (`AppData\Local\Ommatix\models\manifest.rnf05.json`, `f8b04be9…fe47`), y además el cuaderno (423 al empezar), la ausencia de `stop.flag` y la de procesos `ommatix.exe`, `CalculatorApp.exe` y `ommatix-testapp.exe`.
- Escritorio: dos ventanas del Bloc de notas (procesos 8960 y 23164), ambas minimizadas (`showCmd` 2). Su colocación se guardó antes de la corrida con `notepad_placement.ps1`, un auxiliar fuera del repositorio que sólo lee y restaura posiciones, y coincidió con el estado guardado en la séptima ronda.

### 2. Ejecución en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| E2E-03, corrida 10 | `--app notepad --mode run --runs 1 --first-run 10 --max-live 6 --round-start 401` | 2 | Aprobada |
| `--calc-standard` | `--app calculator --mode run --calc-standard --max-live 4 --round-start 401` | 1 | No alcanzó Estándar; detenido en el paso «Estándar», sin segundo clic |
| E2E-02, piloto y serie | no se ejecutaron | 0 | Sin modo Estándar (regla de la autorización) |

Cuaderno: 423 → 426, es decir, 3 solicitudes reales (2 de la corrida 10 y 1 de `--calc-standard`). La ronda lleva 25 de 150. El cliente no creó `stop.flag` en esta continuación.

### 3. Resultados

#### 3.1 E2E-03, corrida 10 (aprobada)

- Dos clics entregados, ambos `effect_observed` y verificados por el motor. Identidad `ok` en los dos; descenso por la isla `ok` en los dos (profundidad 3, `MenuItem`, `AutomationId` `Edit`, 20 elementos visitados en cada uno). `grounding_wrong` 0, falsos éxitos 0, rechazos 0 y reintentos 0.
- Menú «Editar» (UI Automation, comprobación del sistema operativo): Colapsado al empezar; las dos comprobaciones pasaron (2 de 2), con el menú Expandido tras el primer clic y Colapsado tras el segundo. No hizo falta ningún clic de restauración.
- Inactividad del propietario al empezar: 295,6 s. Duración de la corrida: 9,2 s.
- La ventana usada se minimizó al terminar, como hace el cliente con una ventana que estaba minimizada, y la colocación coincide con la guardada.

#### 3.2 E2E-03, serie puntuada completa (corridas 1 a 10)

| Corrida | Solicitudes | Entregadas | Rechazadas (`STALE_SNAPSHOT`) | Éxito |
|---|---|---|---|---|
| 1 | 2 | 2 | 0 | sí |
| 2 | 3 | 2 | 1 (paso 2; el reintento se entregó) | sí |
| 3 a 7 | 2 cada una | 2 cada una | 0 | sí |
| 8 | 3 | 2 | 1 (paso 2; el reintento se entregó) | sí |
| 9 | 2 | 2 | 0 | sí |
| 10 | 2 | 2 | 0 | sí |

Totales: 10 corridas puntuadas y 10 exitosas. 22 solicitudes reales (20 de las corridas 1 a 9 y 2 de la corrida 10), 20 entregadas (todas `effect_observed` y verificadas) y 2 rechazadas antes de la entrega (`STALE_SNAPSHOT` en las corridas 2 y 8; el motor revalida la captura antes de cada entrega). `indeterminate`: 0. Restauraciones del menú: 0. Falsos éxitos: 0. `grounding_wrong`: 0. Comprobaciones del sistema operativo: 20 de 20. Identidad: 22 de 22 intentos hallaron «Editar» como único candidato con su `RuntimeId`. Descenso: 20 de 20 `ok`, profundidad 3, `MenuItem`, `AutomationId` `Edit`. Clics anclados: 20 de 20. Menú Colapsado al empezar en las diez corridas.

#### 3.3 Latencias (`_meta`)

| Medida (ms) | Corrida 10 (2 clics): mediana / p95 | Serie completa (20 clics entregados): mediana / p95 |
|---|---|---|
| `action_total` | 85,8 / 87,1 | 86,0 / 95,0 |
| `action_verification` | 62,8 / 67,5 | 67,5 / 69,0 |
| `action_freshness` | 20,6 / 26,2 | 16,2 / 25,8 |
| `action_delivery` | 2,2 / 2,6 | 2,2 / 4,7 |
| `action_gate` | 0,3 / 0,3 | 0,2 / 0,4 |
| rtt del cliente | 86,1 / 87,4 | 86,2 / 95,2 |

Observaciones (corrida 10: 3 observaciones; serie: 32; mediana / p95): `total` 141,6 / 148,8 y 130,2 / 148,2; `detect` 84,7 / 85,8 y 83,9 / 87,0; `recognize` 47,5 / 48,2 y 44,3 / 56,1; `accessibility` 122,4 / 141,8 y 115,7 / 140,0; `capture` 5,9 / 18,4 y 10,9 / 23,7; `plan` 2,0 / 2,0 y 2,0 / 2,8; `track` 0,7 / 0,7 y 0,7 / 1,2; `fuse` 0,1 / 0,1 y 0,1 / 0,1. Duración de las diez corridas: mediana 8,9 s, p95 11,1 s. La lectura de diagnóstico posterior al clic de «Editar» tardó 1,14 s y 1,19 s en la corrida 10; no es una latencia de acción ni forma parte de las puntuadas. RNF-05 no se evalúa con estas corridas de flujo, que no son la medición de S0 §10.

#### 3.4 `--calc-standard`: no alcanzó el modo Estándar

Lo que registró el paso, en orden:

- Modo al empezar: `other`, es decir, no Estándar.
- Clic 1, «Abrir navegación» (la etiqueta exacta del conmutador): un candidato, `TogglePaneButton` (tipo `Button`), con la identidad confirmada antes del clic. Entregado, `effect_observed`, y anclado tras el clic.
- Paso 2, «Estándar»: el paso volvió a observar y buscó la etiqueta hasta tres veces, con 0,3 s de espera entre búsquedas (unos 1,5 s en total). En ninguna búsqueda hubo un candidato con ese nombre, ni en la observación del motor ni en UI Automation (0 en pantalla y 0 fuera de pantalla): `stopped_by` `zero_candidates`. El paso se detuvo sin segundo clic, y el modo no se volvió a leer (`mode_after` nulo).
- El paso cerró la Calculadora que había lanzado. Duración total: 170 s. Unos 165 s transcurrieron antes del primer clic, mientras `wait_foreground` esperaba a que la Calculadora llegara al primer plano maximizada: la ventana se traía al frente cada 2 s, sin enviar entrada. El resumen del paso no registra esa espera; el registro de las corridas sí (`foreground_wait_s`).

Causa no verificada. El registro no dice si el panel de navegación se abrió tras el clic. Hay dos explicaciones que no se distinguen: (a) el clic no abrió el panel, y el motor informó un cambio visual genérico (`effect_observed`), que no comprueba el modo; (b) el panel se abrió después de la ventana de búsqueda del cliente, de unos 1,5 s. El efecto del clic no se comprobó por el lado del sistema operativo, porque el paso se detuvo antes de leer el modo: no cuenta como falso éxito, porque ninguna comprobación lo refuta, y tampoco queda confirmado.

#### 3.5 E2E-02, Calculadora: no ejecutada

La autorización sólo permite el piloto y la serie si el modo Estándar se alcanza de forma segura. No se alcanzó, así que no hubo piloto ni serie. `--calc-standard` se ejecutó una vez, como se autorizó, y no se repitió.

### 4. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor, Calculadora | `--calc-standard` no halló «Estándar» en la observación del motor ni en UI Automation en unos 1,5 s después del clic de navegación, y el paso se detuvo sin segundo clic (§3.4). El registro no distingue si el panel no se abrió o tardó más. | Abierto. Decisión del responsable: cómo llegar al modo Estándar. El cliente no lee el estado del panel tras el clic; cambiarlo exige una versión nueva del cliente, con su prueba, en una ronda aparte. |
| 2 | Menor, operación | Unos 165 s entre el arranque de la Calculadora y su primer clic; el resumen de `--calc-standard` no registra la espera de primer plano (§3.4). El primer clic se envió sólo con la Calculadora en primer plano, maximizada y anclada. | Abierto. Causa no verificada; hipótesis: el bloqueo de primer plano de Windows, que la espera intenta sortear. |
| 3 | Menor, registro del cliente | La línea `calc_standard` del registro contiene un byte no UTF-8 (`ó`, 0xF3): la salida se escribe en la página de códigos de la consola cuando se redirige a un archivo. Los JSONL de resultados son UTF-8 y no se ven afectados. | Abierto, sin efecto en las puntuaciones. Corrección: escribir la salida como UTF-8, con su prueba, en una versión nueva del cliente. |
| 4 | Observación | La lectura de diagnóstico posterior al clic de «Editar» tardó 1,14 s y 1,19 s en la corrida 10 (no puntuada). | Sin cambio. |
| 5 | Observación (de la primera parte) | Los rechazos `STALE_SNAPSHOT` de las corridas 2 y 8 siguen sin causa verificada. | Sin cambio. |
| 6 | Documental (preexistente) | `plan_pruebas.csv` describe E2E-02 como «Editor» y E2E-03 como «Explorador»; el protocolo ejecuta la Calculadora y el Bloc de notas bajo esos identificadores. | Sin cambio. Decisión del responsable (S4R-02 y S4R-03, en un cambio documental aparte). |

El hallazgo 1 de la primera parte (cupo sin reservar reintentos) queda resuelto para E2E-03: la corrida 10 se ejecutó en su propia invocación, con su propio cupo, y la serie completó diez de diez. Sigue abierto para E2E-02, donde el cupo debe contar los reintentos (decisión del responsable).

### 5. Estado en la matriz

Sin cambio. `docs/thesis/matrices/plan_pruebas.csv` no se modifica. E2E-03 tiene diez corridas puntuadas de diez, todas exitosas, pero la matriz describe E2E-03 como «Explorador», y el responsable decidió que el protocolo reducido tenga sus propios identificadores (S4R-03) en un cambio documental aparte. E2E-02 no se ejecutó. E2E-01 mantiene `EJECUTADO_SUPERVISADO_PARCIAL`.

### 6. Estado final del escritorio, del cuaderno y de `stop.flag`

- **Bloc de notas:** procesos 8960 y 23164, los mismos que al empezar. Las dos ventanas del propietario están minimizadas, con la colocación guardada antes de la corrida 10; la verificación, después de la corrida y después de `--calc-standard`, coincide con el estado guardado.
- **Procesos:** ningún `ommatix.exe`, `CalculatorApp.exe` ni `ommatix-testapp.exe`. El paso `--calc-standard` cerró la Calculadora que lanzó.
- **`stop.flag`:** ausente. El cliente no lo creó en esta continuación, y el agente no lo creó ni lo retiró.
- **Cuaderno:** 426 (25 de 150 solicitudes de la ronda; 125 disponibles).
- **Capturas y texto de pantalla:** ninguno guardado. Los registros contienen estados, códigos, contadores, tiempos, tipos de control, `AutomationId`, identificadores de elemento y las etiquetas fijas del protocolo («Abrir navegación», «Estándar», «Editar»).

### 7. Qué queda y cómo reanudar

1. **Responsable:** decidir cómo llegar al modo Estándar. Cambiar de modo a mano queda fuera del protocolo. Las dos vías son una observación de sólo lectura con el responsable presente, que registre el panel tras el clic (hallazgo 1), o una versión del cliente que lea el estado del panel y espere más, con su prueba de regresión y un hash nuevo, en una ronda aparte.
2. **Responsable:** E2E-02 sigue bloqueado hasta que `--calc-standard` alcance Estándar. La autorización de «una vez» no permite repetir el paso en esta ronda.
3. **Responsable:** el cupo de E2E-02 debe contar los reintentos. Propuesta, sin ejecutar: la serie en invocaciones de una corrida cada una, con `--max-live` igual a lo que quede del cupo de la serie, para que una parada de seguridad impida la corrida siguiente sin interrumpir ningún proceso; y contar a mano las tres corridas fallidas seguidas, porque la regla de `stop.flag` del cliente sólo cuenta dentro de una invocación.
4. **Cliente (ronda aparte, con cambio de hash):** hallazgo 1 (estado del panel y espera), hallazgo 2 (espera de primer plano en el resumen) y hallazgo 3 (codificación), cada uno con su prueba que falle sin el cambio (AGENTS.md, regla 10).
5. **Cuaderno:** 426. Cada ronda usa su propio `--round-start`.

### 8. Ejecución

Dos invocaciones en vivo (la corrida 10 y `--calc-standard`), con 3 solicitudes reales en total. Las únicas entradas fueron dos clics sobre «Editar» en el Bloc de notas (`MenuItem`, `AutomationId` `Edit`) y un clic sobre «Abrir navegación» en la Calculadora (`Button`, `AutomationId` `TogglePaneButton`). No se escribió nada, no se pulsó ninguna pestaña ni botón de ventana, y no se tocó ningún otro elemento. Mover y restaurar la ventana del propietario es gestión de ventanas, sin entradas, y las lecturas de UI Automation son de sólo lectura.


## Preparación de la octava ronda: el cliente busca el elemento «Estándar» por `AutomationId`, registra el modo al detenerse, y los flujos reducidos pasan a S4R-02 y S4R-03

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-calc-standard-and-ids`, sobre `main` en `eef459e`. Cambian el cliente (`tools/s4/e2e_driver.py`, SHA-256 **`a16c53afca63e21f629140ab33951f481b75dfd113866720ab947aa8272ae22d`**, antes `60486d87…c214a`), sus pruebas, las matrices y la documentación. No cambia ninguna crate de Rust ni se ejecutó entrada en vivo: las pruebas usan una Calculadora falsa. Sólo se comprobó, de sólo lectura, que el nuevo script de UI Automation se ejecuta y devuelve el elemento con un `AutomationId` dado en una ventana abierta del escritorio (no la Calculadora). Que el elemento «Estándar» del panel tenga `AutomationId` `Standard` es una hipótesis, **no verificada en vivo**: viene de las convenciones de WinUI, no de una lectura del panel.

### Decisiones del responsable (14 de octubre de 2026)

1. Los flujos reducidos tienen identificadores propios: **S4R-02** (Calculadora) y **S4R-03** (Bloc de notas). E2E-02 y E2E-03 del dossier no cambian.
2. Hallazgos C1, C2 y C3 de la [revisión](../reviews/2026-10-14-s4-seventh-round.md): se corrigen en el cliente con pruebas que fallan sin el cambio.

### La regla de `--calc-standard` (hallazgo C1)

1. **Primer clic, como antes.** «Abrir navegación», seleccionado por su etiqueta exacta, con identidad y anclaje. Después el cliente **no** busca «Estándar» en la observación del motor de inmediato.
2. **Sondeo de sólo lectura.** Durante `STANDARD_POLL_WINDOW_S` = 3 s, en pasos de `STANDARD_POLL_STEP_S` = 0,15 s, lee por UI Automation los elementos de la ventana con `AutomationId` `Standard`. Cada lectura cuesta un arranque de PowerShell, así que el paso real es mayor que 150 ms: lo que se acota es la ventana, y `polls` y `waited_ms` dicen cuántas lecturas hubo y cuánto se esperó.
3. **Candidato.** Un elemento en pantalla con ese `AutomationId` y un tipo de control de la lista cerrada `ListItem`, `TreeItem`, `TabItem`, `MenuItem`, `RadioButton`. Se necesita exactamente uno y con RuntimeId.
4. **Nombre.** Se acepta sólo si, tras NFC, es exactamente `Estándar` o `Calculadora estándar` (lista cerrada `CALC_STANDARD_NAMES`). Cualquier otro nombre (otra capitalización, con espacios, otro texto) es `unknown_name`, y **no se registra**: `name` es nulo y `name_accepted` falso.
5. **Sin clic** (y `standard_item.stopped_by`): `timeout` (nada, o nada legible, en la ventana), `several_candidates`, `unknown_name` o `unidentifiable` (sin RuntimeId). Los tres últimos paran de inmediato; esperar no los mejora. Un `timeout` registra además `by_name`: cuántos elementos ve UI Automation con los dos nombres aceptados, de cualquier tipo y `AutomationId`, con recuentos y tipos y sin nombres. Así se distingue un `AutomationId` distinto de `Standard` de un panel que no mostró el elemento. Nunca se hace clic por el nombre solo.
6. **Elemento del motor.** Con el elemento sondeado hallado, el cliente observa de nuevo, selecciona el elemento del motor por ese nombre exacto (único y confirmado, como en cualquier paso: `targets` y `stopped_by` como hasta ahora) y busca su identidad por UI Automation con el tipo de control de su rol. Esa identidad debe ser **el mismo RuntimeId** que el elemento sondeado (`pinned_runtime_id`); otro RuntimeId bajo el mismo nombre es `not_the_polled_item` y no se envía el clic. El anclaje tras el clic (RuntimeId, y el descenso por la isla si el punto cae en un anfitrión) no cambia.
7. **Modo.** Se lee antes (sin clic si ya es Estándar o no se puede leer) y después. `mode_after` se registra **en todo desenlace**, también cuando el paso se detiene (antes quedaba nulo): se lee, de sólo lectura, antes de cerrar la Calculadora que el paso lanzó; sin ventana que leer es `unknown`.

### Hallazgos C2 y C3

- **C2.** El resumen de `calc_standard` registra `foreground_wait_s`, `activation_requests` y `window_placements`, como las corridas.
- **C3.** `main()` configura la salida estándar en UTF-8 (`use_utf8_stdout`) cualquiera que sea la página de códigos de la consola; el registro redirigido a un archivo ya no mezcla bytes de la página de códigos.

### Registro (sin nombres)

El resumen añade `flow_id` (`S4R-02`), `standard_item` (`status`, `polls`, `waited_ms`, `window_s`, `step_s`, `candidates`, `offscreen_ignored`, `control_type`, `automation_id`, `name_accepted`, `name` sólo si está en la lista cerrada, `stopped_by` y, al detenerse, `by_name`), `foreground_wait_s`, `activation_requests` y `window_placements`; `mode_after` ya no es nulo. Las corridas de todos los flujos añaden `flow_id` (`E2E-01`, `S4R-02`, `S4R-03`) y conservan `e2e` (`E2E-01`, `E2E-02`, `E2E-03`), porque la evidencia registrada se refiere a él.

### Pruebas

`python -m unittest discover -s tools/s4 -p "test_*.py"`: **143 pruebas correctas** (113 anteriores, con las que cambian por el nuevo flujo ajustadas, y 30 nuevas). Con el cliente de la séptima ronda las 30 nuevas fallan (6 fallos y 24 errores). Cubren: el panel que aparece tarde dentro de la ventana → hallado; `AutomationId` `Standard` con el nombre «Calculadora estándar» → seleccionado y anclado; dos candidatos → sin clic; nombre desconocido → sin clic y el nombre no aparece en el resumen, en el archivo de resultados ni en el registro; tiempo agotado → sin clic; lectura ilegible → sondeo hasta el tiempo y sin clic; otro `AutomationId` con el mismo nombre → `timeout` con `by_name` y sin clic; elemento que el motor no ve → se detiene en el registro del lado del motor; otro RuntimeId bajo el mismo nombre → sin clic; `mode_after` registrado al detenerse (cinco causas) y antes de cerrar la Calculadora; espera de primer plano en el resumen; registro en UTF-8; `flow_id`.

### Identificadores y matrices

`matrices/plan_pruebas.csv` añade **S4R-02** (`PENDIENTE_PRODUCTO`: no ejecutada) y **S4R-03** (`EJECUTADO_SUPERVISADO_PARCIAL`: diez de diez corridas puntuadas, 0 falsos éxitos, séptima ronda y su continuación; parcial por una tarea, sólo clics, un DPI y un tema). E2E-02 y E2E-03 siguen `PENDIENTE_PRODUCTO`. `requisitos_trazabilidad.csv` liga RF-18 también a S4R-02 y S4R-03. `validar_paquete.py` no valida patrones de identificador (sólo unicidad y trazabilidad de los escenarios), así que no cambia.

**RF-18 y §6.7, sin casillas marcadas.** Hay evidencia puntuada en dos aplicaciones: la aplicación de prueba (E2E-01, parcial) y el Bloc de notas (S4R-03). Falta la Calculadora (S4R-02), el editor con escritura y guardado (E2E-02), el explorador (E2E-03), los DPI de 125 y 150 %, otro tema y las 450 corridas. RF-18 y la Definition of Done siguen abiertos.

### Qué debe mostrar la próxima ronda en vivo

`--calc-standard` con su cupo de 4 y `--round-start 426`, y el hash de arriba comprobado antes de cada invocación. Una ejecución que alcance Estándar muestra `standard_item.status` `ok`, el `control_type` y `name_accepted` verdadero, dos clics anclados y `mode_after` `standard`. Si no lo alcanza, el registro dirá por qué (`timeout` con `by_name`, `several_candidates`, `unknown_name`, un objetivo del motor ausente o ambiguo), y eso es un hallazgo, no un fallo del cliente. Seguirá abierto el cupo de S4R-02, que debe contar los reintentos (decisión del responsable).

### Estado

Hallazgos C1, C2 y C3: corregidos en el cliente y probados con dobles, **sin confirmar en vivo**. S4R-02 no se ejecutó. Cuaderno: 426, sin cambio.
