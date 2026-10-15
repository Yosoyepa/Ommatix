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
4. **Nombre. (Sustituido en la novena ronda: el nombre ya no es una condición; véase «Preparación de la novena ronda».)** Se aceptaba sólo si, tras NFC, era exactamente `Estándar` o `Calculadora estándar` (lista cerrada `CALC_STANDARD_NAMES`). Cualquier otro nombre (otra capitalización, con espacios, otro texto) es `unknown_name`, y **no se registra**: `name` es nulo y `name_accepted` falso.
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

## Preparación de E2E-02 y E2E-03 del dossier

**Fecha:** 14 de octubre de 2026. **Estado:** preparación. Es código, pruebas y documentación; **no se ejecutó nada en vivo y no se envió ninguna entrada**. Lo único que se lanzó fue el editor propio (`ommatix-testeditor.exe`), sin enviarle entrada, para comprobar que arranca, que rechaza una carpeta inválida y qué expone a UI Automation (§2). **Decisiones del responsable (14 de octubre de 2026):** E2E-02 corre en una aplicación propia; E2E-03 corre en `explorer.exe`, pero sólo en una ventana que abre el cliente sobre una carpeta de prueba que el cliente creó. Los flujos del protocolo reducido (la Calculadora y el Bloc de notas) son S4R-02 y S4R-03 y los lleva otra rama. Esta sección no cierra RF-18, RNF-03 ni RNF-05, ni ninguna casilla DoD (capítulo 06 §6.7).

### 1. Qué se prepara

`matrices/plan_pruebas.csv` define **E2E-02** como «Editor: escritura y guardado de archivo temporal» y **E2E-03** como «Explorador: navegar carpetas de prueba sin destrucción». Las secciones anteriores de este informe usaron esos identificadores con otro sentido (la Calculadora y el Bloc de notas, §1 de la primera ronda). Los registros del cliente nuevo dicen `E2E-02-plan` y `E2E-03-plan` para no confundirlos.

| Pieza | Dónde | Qué es |
|---|---|---|
| Editor propio | [`tools/s4/testeditor`](../../tools/s4/testeditor) | Crate independiente (`eframe`/`egui`, la misma pila que la aplicación de prueba, sin `unsafe`), proceso `ommatix-testeditor.exe` |
| Cliente | [`tools/s4/e2e_dossier.py`](../../tools/s4/e2e_dossier.py) | Extiende el `Protocol` de `e2e_driver.py`, que importa sin cambiarlo: mismo transporte MCP, misma compuerta, mismo anclaje por identidad |
| Pruebas del cliente | [`tools/s4/test_e2e_dossier.py`](../../tools/s4/test_e2e_dossier.py) | 50 pruebas con motor, escritorio y aplicaciones falsos |
| Alcance por ventana | `src/application/src/window_scope.rs`, [ADR-0020](../adr/0020-operator-scope-on-windows.md) (aprobada el 14 de octubre de 2026) | Lo que impide que `explorer.exe` alcance el escritorio o la barra de tareas |
| Escenarios | BDD-36 y BDD-37 de `features/06_accion_segura.feature`, prueba UT-45 | Alcance por ventana sobre dobles del actuador |

### 2. E2E-02: el editor

**Un binario aparte.** RF-18 pide «flujos reales verificables en 3 aplicaciones permitidas»: otra ventana de `ommatix-testapp.exe` sería la misma aplicación. Por eso el editor es un crate y un proceso propios. Aparece en la lista de aplicaciones permitidas de la configuración de S4 (§5), nunca en los valores por defecto.

**La aplicación.** Un campo de texto de varias líneas (`Editor de texto`, nombre de accesibilidad fijado en su nodo AccessKit, sin dibujarlo), un botón `Guardar` y una línea de estado (`Estado: pendiente`, `Estado: guardado`, `Estado: fallido`, palabras que no aparecen en ningún otro sitio). El título refleja el estado en forma fija: `Ommatix S4 editor [caracteres=N guardado=no|si]`.

**El guardado está confinado.** El cliente pasa `--folder <carpeta>`; la aplicación sólo acepta una carpeta que, ya resuelta, sea exactamente `<directorio temporal>\ommatix-e2e\<id de la corrida>` (id de 1 a 64 letras, dígitos, `.`, `_` o `-`), de modo que ni `..` ni un enlace simbólico o una unión pueden sacarla de ahí. Con cualquier otra sale con el código 2 antes de abrir una ventana. El archivo se llama siempre `nota.txt`: nunca hay otro nombre ni otra ruta. Escribe los bytes del campo tal cual (UTF-8, sin BOM, sin conversión de fin de línea), sustituyendo lo guardado antes; no escribe si el destino existe y no es un archivo normal (un enlace, una carpeta), ni más de 4096 bytes. No lee ni escribe nada más en disco, red ni portapapeles.

**El flujo** (dos pasos): `type` del texto sintético `prueba 123` en `Editor de texto` y clic en `Guardar`. El `type` del motor ya hace su clic de enfoque (ADR-0018, dos fases), así que no hay un clic previo sobre el campo: sumaría un cambio visual que falsearía la medición de `visual_change`. Comprobaciones del SO, que no dependen del motor:

1. Tras escribir: el título dice `caracteres=10 guardado=no` y la carpeta está vacía.
2. Tras `Guardar`: la carpeta contiene exactamente un archivo, `nota.txt`, archivo normal cuyos bytes son los del texto escrito (nada leído de la pantalla), y el título dice `guardado=si`.

Un `effect_observed` con otro contenido, otro archivo o una carpeta con más entradas cuenta como falso éxito.

**Comprobado sin entrada** (con el binario de `--release`): con una carpeta fuera de `...\ommatix-e2e\<id>` o sin argumento el proceso sale con el código 2 y no abre ventana. Con la carpeta del cliente arranca maximizado, con el título `[caracteres=0 guardado=no]`, deja la carpeta vacía y expone a UI Automation, de sólo lectura: `Editor de texto` como `Edit` habilitado y con `ValuePattern` escribible (el motor lo trata como `input`), `Guardar` como `Button` con `Invoke` y el texto de estado. Se cerró por su PID y se borró su carpeta.

### 3. E2E-03: el explorador, y su diseño de seguridad

**El riesgo.** `explorer.exe` es también el dueño del escritorio (`Progman`, `WorkerW`), de la barra de tareas (`Shell_TrayWnd`) y de los menús emergentes del shell. Permitir `explorer.exe` por nombre de proceso dejaría pasar un clic a cualquiera de ellos, o a una ventana del Explorador que abrió el propietario. Nunca se permite `applicationframehost.exe`.

**La restricción por ventana** ([ADR-0020](../adr/0020-operator-scope-on-windows.md), aprobada el 14 de octubre de 2026; apagada por defecto):

- `[safety] allowed_window_classes = { "explorer.exe" = ["CabinetWClass"] }`: por aplicación, las únicas clases de ventana que reciben entrada real.
- `[safety] window_pin_file = "window.pin"`: un archivo con el identificador (decimal) de la única ventana permitida. Lo escribe el cliente cuando abre su ventana y lo borra al cerrarla; el agente no tiene forma de escribirlo, igual que el archivo de parada. Se lee en cada acción; faltante, vacío o malformado, no se permite ninguna entrada.
- El motor comprueba **la ventana en primer plano y la ventana bajo el punto de la acción**, ambas atestiguadas por el actuador (clase e identificador, nunca el título), en la puerta, otra vez justo antes de entregar y, en `type`, entre el clic y las teclas. Con una lista de clases las dos ventanas deben tener una clase de la lista y ser la misma ventana; con ventana registrada, las dos deben ser ella. Lo que el actuador no pueda atestiguar queda fuera, y el rechazo es `POLICY_DENIED` con un mensaje fijo que no dice qué ventana era.
- La configuración se valida: una clave que no sea un nombre de ejecutable en minúsculas o que no esté en `allowed_applications`, una lista vacía, `applicationframehost.exe` y las clases del escritorio y de la barra de tareas son errores, y arrancar con `explorer.exe` permitido sin lista de clases ni ventana registrada imprime un aviso. El indicador de control activo nombra el alcance.
- Qué rechaza (pruebas, §6): el escritorio y la barra de tareas, en primer plano o bajo el punto; un menú emergente; otra ventana `CabinetWClass` bajo el punto; una ventana `CabinetWClass` que no es la registrada (BDD-37); un actuador que no atestigua; una ventana que cambia justo antes de entregar o entre el clic y las teclas.

El cliente repite, sólo leyendo, lo que el motor comprueba: antes de cada clic exige que el primer plano sea la ventana registrada, maximizada en el monitor principal, y que la ventana de nivel superior bajo el centro del objetivo (`WindowFromPoint` + `GA_ROOT`) también lo sea; si no, no hay clic y el protocolo se detiene (`stop_file`).

**El flujo.** Cada corrida:

1. Crea `%TEMP%\ommatix-e2e\s4-e2e03-r<n>-<fecha>\` con `carpeta-a\carpeta-b\dummy.txt` y `leeme.txt` (contenido fijo y sintético), y anota los archivos y sus tamaños. La carpeta de la corrida se crea de forma exclusiva: nunca se adopta una que ya existe.
2. Abre **una** ventana del Explorador sobre esa raíz (`explorer.exe /root,<raíz>`; `--explorer-launch e` usa `/e,`), reconoce su `HWND` como la única ventana nueva `CabinetWClass` cuya carpeta actual (`Shell.Application`, sólo lectura) es la raíz, la registra, la escribe en el archivo de la ventana registrada, la maximiza en el monitor principal y la lleva al frente (gestión de ventanas, sin entrada). Si no puede distinguirla, no hay entrada.
3. Tres clics sencillos, nada más: `carpeta-a` (elemento de árbol del panel de navegación), `carpeta-b` (ídem) y la raíz en la ruta de navegación (botón). El contrato no tiene doble clic y no se añade. No hay borrado, renombrado, arrastre, menú contextual, atajo de teclado ni escritura.
4. Tras cada clic, la carpeta actual de la ventana (lectura de sólo lectura) debe ser la esperada (`carpeta-a`; `carpeta-a\carpeta-b`; la raíz). Cada clic se ancla además por identidad como los demás flujos (el elemento bajo el cursor es el hallado antes, por `RuntimeId`), y el nombre se busca sólo entre elementos de la misma zona (`left_sidebar` o `top_bar`) y del tipo de control correspondiente (`TreeItem`; `Button` o `SplitButton`), porque el mismo nombre aparece también en la lista de archivos, donde un clic sencillo sólo selecciona.
5. Al final, el árbol debe tener los mismos archivos y tamaños que al principio (`tree_unchanged`); si no, la corrida falla.
6. Cierra **sólo** la ventana que abrió: `WM_CLOSE` a su `HWND`, tras comprobar que sigue siendo una ventana `CabinetWClass` de `explorer.exe`; jamás termina un proceso. Borra el archivo de la ventana registrada. Borra **sólo** el árbol que creó, ruta por ruta (`os.remove` y `os.rmdir`, sin borrado recursivo, y nunca una ruta fuera de la carpeta de la corrida); lo que no registró lo deja y lo informa. Si la ventana no se cierra, el árbol se conserva para el operador.

El Explorador no se trata como anfitrión de una isla XAML: la lista de clases de anfitriones (`ISLAND_HOST_CLASSES`) no cambia y, si la consulta por posición devolviera uno, el descenso acotado del cliente se aplicaría como siempre. Esto no se ha verificado en vivo.

### 4. Cómo ejecutarlo

Desde la raíz del repositorio, con el binario de la rama (`cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`) y el editor (`cargo build --release` en `tools/s4/testeditor`). Las configuraciones viven fuera del repositorio; el resto de `[models]` es el de `tools/s0/ommatix.s0.directml.toml`, con rutas absolutas.

```toml
# editor.toml (E2E-02)
[engine]
verify_timeout_ms = 500
verify_poll_ms = 50
[safety]
actuation_enabled = true
allowed_applications = ["ommatix-testeditor.exe"]
include_values = false
stop_file = "stop.flag"
[models]
accessibility = "uia"
```

```toml
# explorer.toml (E2E-03)
[engine]
verify_timeout_ms = 500
verify_poll_ms = 50
[safety]
actuation_enabled = true
allowed_applications = ["explorer.exe"]
allowed_window_classes = { "explorer.exe" = ["CabinetWClass"] }
window_pin_file = "window.pin"
include_values = false
stop_file = "stop.flag"
[models]
accessibility = "uia"
```

```text
python tools\s4\e2e_dossier.py --exe target\release\ommatix.exe --config editor.toml --app editor --mode probe --ledger <dir>\ledger.txt --results <dir>\dossier.jsonl
python tools\s4\e2e_dossier.py --exe target\release\ommatix.exe --config editor.toml --app editor --mode run --runs 1 --max-live 8 --round-start <n> --ledger <dir>\ledger.txt --results <dir>\dossier.jsonl --session-id s4d-e2e02
python tools\s4\e2e_dossier.py --exe target\release\ommatix.exe --config explorer.toml --app explorer --mode probe --ledger <dir>\ledger.txt --results <dir>\dossier.jsonl
python tools\s4\e2e_dossier.py --exe target\release\ommatix.exe --config explorer.toml --app explorer --mode run --runs 1 --max-live 8 --round-start <n> --ledger <dir>\ledger.txt --results <dir>\dossier.jsonl --session-id s4d-e2e03
```

`--mode probe` (servidor sin actuación) crea el árbol, abre la ventana, observa, imprime por paso cuántos elementos halla el motor (por zona, rol y accionabilidad) y cuántos UI Automation (por tipo de control), y lo cierra y borra todo sin enviar entrada. Debe correrse antes de cualquier corrida en vivo. El modo `run` se niega a empezar si la configuración no es exactamente la que el flujo necesita (`config_problems`): para el Explorador, `explorer.exe` y nada más, sólo `CabinetWClass`, con ventana registrada y UI Automation; para el editor, su binario y nada de ventana registrada; en ambos, parada local y actuación habilitada. También se niega si existe el archivo de parada o el de la ventana registrada. Cada corrida cuenta contra el cuaderno de solicitudes (`--ledger`, tope de 400 por ronda) como las demás.

**SHA-256 del cliente** (`tools/s4/e2e_dossier.py`): `cd959f59…57f306` (completo: `cd959f590fb02f7abc64f903fa1a7773fec25ed34ed8ad214b7409228157f306`. Importa `tools/s4/e2e_driver.py` sin cambiarlo; el del 1.0.27 es `60486d87…c214a`. **SHA-256 del editor compilado** (`--release`, `rustc 1.99.0`, en esta máquina): `776cd09e…0ca437` (completo: `776cd09e0e924ae637306ebccec0727ee33229fe86f86e6c5bad2b76210ca437`.

### 5. Lo que no está verificado

Nada de lo siguiente se ha ejecutado; lo dice el diseño y lo mostrarán el sondeo y la primera corrida en vivo:

1. **La estructura del panel de navegación.** Que `explorer.exe /root,<carpeta>` muestre `carpeta-a` y `carpeta-b` como elementos `TreeItem` de la zona `left_sidebar`, que el segundo aparezca tras elegir el primero (o que la elección expanda el árbol) y que el botón de la raíz en la ruta de navegación sea `Button` o `SplitButton` en `top_bar`. Si no, el paso se detiene antes de cualquier clic (`target_stop`) o falla tras uno (carpeta distinta o anclaje erróneo), sin más entrada.
2. **Que la ventana de nivel superior bajo el punto y la del primer plano sean `CabinetWClass`** en el Explorador de Windows 11 (parte de su interfaz es XAML). La ventana de la lista de archivos, la ruta y el panel son hijas de esa ventana; no se ha comprobado qué devuelven `WindowFromPoint` y `GetAncestor` allí.
3. **Que `Shell.Application` informe la carpeta actual** de la ventana por su `HWND`, y que `explorer.exe /root,` funcione con una ruta con espacios (la del directorio temporal del propietario no los tiene).
4. **La parte de Windows del alcance por ventana** (ADR-0020): sólo está comprobada por compilación y `clippy` (`--features dxgi,sendinput,directml,winrt-ocr,uia`); su lógica, por pruebas con dobles. No se envió entrada a nada para probarla.
5. **El `type` en el editor** (campo de varias líneas de egui): es el mismo mecanismo de la aplicación de prueba (10 de 10 en la tercera ronda), pero no se ha ejecutado con este campo.

### 6. Pruebas

Todas con dobles; ninguna envía entrada ni lanza el Explorador (AGENTS.md, regla 6).

| Conjunto | Resultado | Qué cubre |
|---|---|---|
| `window_scope::tests` y `engine::tests::window_scope` (`ommatix-application`) | 22 pruebas nuevas | La regla pura; el escritorio, la barra de tareas, un menú emergente, otra ventana de la misma clase, un actuador que no atestigua y una ventana que cambia justo antes de entregar o entre el clic y las teclas de `type`; la ventana registrada; sin alcance, sin consultas |
| `config`, `policy` y `wiring` (`ommatix`) | 10 pruebas nuevas | Sin alcance por defecto; validación de las claves; el archivo de la ventana registrada sigue al archivo y falla cerrado; el indicador y el aviso de `explorer.exe` |
| BDD-36 y BDD-37 (`ommatix-acceptance`) | 2 escenarios nuevos (37 de 37 pasan) | Fuera del alcance y ventana no registrada, con el archivo de la ventana registrada del producto |
| `testeditor` | 15 pruebas | El confinamiento (forma `ommatix-e2e\<id>`, `..`, ids), los bytes exactos, un enlace, tamaño, carpeta cambiada, estado y título |
| `tools/s4/test_e2e_dossier.py` | 50 pruebas (163 en `tools/s4`) | La carpeta de la corrida y su borrado (sólo lo registrado, nunca lo ajeno, ninguna ruta fuera); la configuración; los flujos de sólo clics; el editor (éxito, bytes distintos, teclas perdidas, archivo extra, aborto); el Explorador (tres clics, el señuelo de la lista de archivos, ventana del propietario al frente, escritorio o barra de tareas bajo el punto, carpeta que no cambia, árbol alterado, ventana que no se cierra, handle que ya no es del Explorador, aborto) |

Las pruebas de seguridad se comprobaron por mutación: sin la comprobación de la ventana bajo el punto, sin la del primer plano registrado y sin las del motor, fallan las que las cubren.

### 7. Estado en la matriz y decisiones pendientes

- **UT-45** (alcance de ventana): nueva, `IMPLEMENTADO_EN_RUST_S4`. BDD-36 y BDD-37, `EJECUTADO_SOBRE_CONTRATO_S3_DOBLE` (corren sobre dobles del actuador).
- **E2E-02 y E2E-03 de la matriz:** `PENDIENTE_PRODUCTO`, sin cambio. El cliente y el editor están listos, pero la prueba es de aceptación y exige la corrida supervisada; no existe un estado honesto entre ambas, y no se inventó uno. RF-18, RNF-03, RNF-05 y las casillas DoD no cambian.
- **Antes de una corrida en vivo, el responsable decide:**
  1. ~~Aprobar, cambiar o rechazar la ADR-0020~~: **aprobada** por el responsable el 14 de octubre de 2026.
  2. **Si la navegación por el panel no es posible** (el sondeo mostrará qué ve el motor): o se acepta cambiar un ajuste del usuario del Explorador (por ejemplo «Expandir a la carpeta abierta», que el cliente no toca), o se amplía el contrato con un doble clic (cambio de contrato, fuera de esta rama), o se permite la clase del menú desplegable de la ruta de navegación (ensancha el alcance), o se reduce el flujo a un nivel.
  3. **El cupo:** el valor de `--round-start` de la ronda y cuántas solicitudes se asignan (el editor usa 2 por corrida, el Explorador 3, más los reintentos tras `STALE_SNAPSHOT`; se sugiere `--max-live 8`).
  4. **El entorno:** sin uso concurrente del equipo durante la corrida, sin otras ventanas del Explorador necesarias y con la ventana del propietario fuera de la zona de clics (el cliente la deja abierta y no la toca).
  5. **El nombre:** si los informes siguen llamando E2E-02/E2E-03 a la Calculadora y el Bloc de notas (S4R-02/S4R-03) o pasan a los de la matriz.

## Octava ronda: S4R-02 no alcanzó el modo Estándar, y E2E-02 y E2E-03 no llegaron al primer plano; ninguna corrida puntuada

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-eighth-round`, desde `main` en `b7a5e7b` (fusión de #19). No cambia ningún código. Se usaron el cliente comprometido `tools/s4/e2e_driver.py` (SHA-256 `a16c53afca63e21f629140ab33951f481b75dfd113866720ab947aa8272ae22d`) y `tools/s4/e2e_dossier.py` (SHA-256 `cd959f590fb02f7abc64f903fa1a7773fec25ed34ed8ad214b7409228157f306`), comprobados antes de cada invocación en vivo. El binario del motor se compiló de nuevo en esta rama (`target/release/ommatix.exe`, SHA-256 `3a451211763f8ef9c92adb98955dca7b42dedf58974c97248bc97aac513cbb1a`), porque el de la séptima ronda (`11a3d792…aca9da`) es anterior a la fusión de #19. El editor (`tools/s4/testeditor/target/release/ommatix-testeditor.exe`) tiene SHA-256 `a6765dca0b5e63b45cfd3ab8a28b470a0896a3dcdfe1ea062cb48c8ff745119d`. Autorización del responsable del 14 de octubre de 2026: `calculatorapp.exe` (S4R-02), `ommatix-testeditor.exe` (E2E-02) y `explorer.exe` sólo en la ventana `CabinetWClass` que abre el cliente (E2E-03, ADR-0020); cupo de 200 solicitudes con `--round-start 426`; el responsable no usó el equipo.

**Resultado:** ninguna corrida puntuada. S4R-02 no alcanzó el modo Estándar, así que no hubo piloto ni serie. El editor no llegó al primer plano maximizado en tres de cuatro intentos (el primer sondeo sí llegó), y el Explorador no llegó en su sondeo; en ningún caso se envió entrada. En el sondeo que sí llegó al primer plano, el motor no tiene ninguna etiqueta de accesibilidad para el campo del editor. Se envió una sola solicitud real, el clic de navegación de S4R-02, que se entregó y se verificó. No se creó `stop.flag`. Cuaderno: 426 → 427 (1 de 200 solicitudes de la ronda).

### 1. Verificación previa a la entrada en vivo

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia` y `cargo build --release` en `tools/s4/testeditor`: correctos.
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: 383 pruebas pasadas, 0 fallidas, 1 ignorada.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 193 pruebas correctas.
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48; `validar_paquete.py`, 218 comprobaciones y 0 fallidas. Cuatro archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y el fin de línea; se restauraron con `git restore`.
- Configuraciones (fuera del repositorio, en la carpeta de corridas): `s4r8-live.toml` (SHA-256 `0428daf861e1567e1be9eb8588887fd56f370beef863099904cd9bd3ce5ec1e2`) es una copia de `s4r7-live.toml` (`b6e9e8e8…e74c`) con `allowed_applications = ["calculatorapp.exe", "ommatix-testeditor.exe"]`, sin alcance por ventana ni archivo de ventana. `s4r8-explorer.toml` (SHA-256 `ab6c33b5144e6785e4be60aac71a6d54b93c29e8f383932ed60324dbaaa233d7`) tiene `allowed_applications = ["explorer.exe"]`, `allowed_window_classes = { "explorer.exe" = ["CabinetWClass"] }` y un `window_pin_file` absoluto en la carpeta de corridas. Las comprobaciones de configuración del cliente pasan para cada flujo con su configuración y fallan con la otra.
- Manifiesto float16 (`AppData\Local\Ommatix\models\manifest.rnf05.json`): `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`, el mismo de la séptima ronda.
- Escritorio antes de empezar: ningún proceso de prueba; los dos procesos del Bloc de notas del propietario (PIDs 8960 y 23164) sin cambios; sin `stop.flag` ni `window.pin`; cuaderno en 426.

### 2. Ejecución en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| S4R-02, sondeo | `e2e_driver.py --app calculator --mode probe --runs 1` | 0 | Los cinco elementos del flujo (`Siete`, `Más`, `Ocho`, `Es igual a` y el control de borrado) confirmados por accesibilidad, un botón cada uno; el primer plano llegó en una solicitud (1,18 s). El control de borrado al empezar es «Borrar». |
| S4R-02, `--calc-standard` | `--app calculator --mode run --calc-standard --max-live 4 --round-start 426` | 1 | Clic «Abrir navegación» entregado y anclado. «Estándar», con `AutomationId` `Standard`, no aceptado por su nombre (`unknown_name`), sin segundo clic. Modo antes y después: «otro». La Calculadora que lanzó se cerró. |
| E2E-02, sondeo 1 | `e2e_dossier.py --app editor --mode probe --runs 1` | 0 | Primer plano en 10 solicitudes de activación. Ningún elemento con etiqueta de accesibilidad; «Guardar» confirmado sólo por OCR; «Editor de texto» sin candidato. |
| E2E-02, piloto 1 | `e2e_dossier.py --app editor --mode run --runs 1 --max-live 8 --round-start 426` | 0 | Sin primer plano maximizado en 30 intentos (unos 60 s). Sin entrada. |
| E2E-02, piloto 2 | la misma invocación | 0 | Igual que el piloto 1. |
| E2E-02, sondeo 2 | `--app editor --mode probe --runs 1` | 0 | Igual: sin primer plano maximizado en 30 intentos. |
| E2E-03, sondeo | `e2e_dossier.py --app explorer --mode probe --runs 1` (`s4r8-explorer.toml`) | 0 | Sin primer plano maximizado en 62,9 s. Ventana registrada, archivo de ventana escrito y borrado, ventana cerrada por su `HWND`, árbol temporal borrado (2 archivos y 3 carpetas). El panel de navegación no se observó. |

Un primer intento del sondeo de la Calculadora falló al arrancar el motor, porque el ejecutable se pasó como ruta relativa (`target/release/ommatix.exe`): Windows no lo encontró, no se lanzó ningún proceso y no se gastó ninguna solicitud. El segundo intento usó la ruta absoluta (hallazgo 4).

Cuaderno: 426 → 427, es decir, una solicitud real. La ronda lleva 1 de 200.

### 3. Resultados

#### 3.1 S4R-02, Calculadora: sin modo Estándar, sin piloto ni serie

- **Sondeo.** Cada uno de los cinco elementos del flujo tiene un único candidato, un botón confirmado por accesibilidad. El control de borrado al empezar es «Borrar».
- **Modo al empezar:** `other`.
- **Clic 1, «Abrir navegación».** Un candidato (`TogglePaneButton`, `Button`), con identidad `ok` antes del clic. Entregado, `effect_observed` y anclado tras el clic. `_meta` de esa única acción: `action_total` 84,8 ms, `action_verification` 57,3 ms, `action_freshness` 25,4 ms, `action_delivery` 1,7 ms y `action_gate` 0,3 ms; rtt del cliente 85,0 ms. Es una muestra, sin mediana ni p95.
- **Paso 2, «Estándar».** En una única lectura de UI Automation dentro de la ventana de 3 s (407 ms) hay un candidato con `AutomationId` `Standard` y tipo `ListItem`, en pantalla. Su nombre no está en la lista cerrada (`Estándar`, `Calculadora estándar`): `unknown_name`. El nombre no se registró, ni en el resumen ni en el archivo de resultados. `by_name`: ningún elemento de la ventana tiene uno de los dos nombres aceptados, de cualquier tipo o `AutomationId`. No hubo clic.
- **Modo al terminar:** `other`, leído de sólo lectura antes de cerrar la Calculadora. `foreground_wait_s` 0,89; `activation_requests` 1; `window_placements` 2.
- **Qué confirma y qué no.** Confirma la hipótesis de la sexta ronda sobre el `AutomationId`: `Standard` existe, es un `ListItem` y hay un único candidato. No confirma que su nombre sea «Estándar» o «Calculadora estándar». La lista cerrada no lo acepta, y el nombre sólo puede conocerse leyendo texto de pantalla, que no se guarda.
- **Consecuencia.** Según la autorización, sin modo Estándar no hay piloto ni serie. S4R-02 queda sin corridas puntuadas.

#### 3.2 E2E-02, editor: el piloto no llegó al primer plano

- **Sondeo 1** (primer plano en 10 solicitudes de activación): 69 elementos del motor, ninguno con origen de etiqueta de accesibilidad (`unknown` 60, `ocr` 9). Por OCR se ven «Editor», «Estado:» y «pendiente». «Guardar» se confirma sólo por OCR. «Editor de texto», el campo, no tiene candidato: con ese resultado el primer paso se detendría antes de cualquier entrada.
- **Piloto 1, piloto 2 y sondeo 2.** El editor no llegó al primer plano maximizado en 30 solicitudes de activación (unos 60 s). Ninguna observación, ninguna solicitud y ninguna entrada. En los tres casos la ventana se cerró y la carpeta de la corrida se borró.
- **Por qué no se abre la serie.** Un piloto que no pasa no abre la serie. Una serie con este fallo contaría tres corridas fallidas seguidas, y el cliente crearía `stop.flag`, que detendría también el Explorador (hallazgo 6).

#### 3.3 E2E-03, Explorador: el sondeo no llegó al primer plano

- El sondeo registró la única ventana nueva `CabinetWClass` cuya carpeta es la raíz, escribió el archivo de ventana y la maximizó. No llegó al primer plano maximizado en 62,9 s, así que no leyó el panel de navegación ni la ruta de navegación: esa lectura sólo ocurre con la ventana en primer plano.
- Sin entrada. El cliente cerró la ventana por su `HWND` (`WM_CLOSE`), borró el archivo de ventana y borró el árbol temporal que creó (2 archivos y 3 carpetas).
- Según el orden de la ronda, un sondeo que no muestra los objetivos no autoriza la ejecución en vivo. Queda sin resolver si los tres clics sencillos son alcanzables.

#### 3.4 Totales

- Solicitudes reales: 1 (cuaderno 426 → 427). Entregadas: 1, `effect_observed` y verificada por el motor. Rechazadas: 0. `indeterminate`: 0. Falsos éxitos: 0. `grounding_wrong`: 0. `POLICY_DENIED`: 0. Reintentos por `STALE_SNAPSHOT`: 0.
- Corridas puntuadas: 0. Pilotos aprobados: 0.

### 4. Latencias (`_meta`)

Una sola acción (el clic de navegación de S4R-02), sin serie: no hay mediana ni p95, y no mide RNF-05 (S0 §10). Valores en §3.1.

### 5. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor, Calculadora | El elemento de Estándar tiene `AutomationId` `Standard` y es un `ListItem`, pero su nombre no está en la lista cerrada, así que el cliente lo rechaza sin clic. | Abierto (decisión del responsable). Confirmar con el panel abierto el nombre exacto, sin guardarlo. Después, en una ronda aparte, un cambio de la lista cerrada con una prueba que falle sin el cambio y un hash nuevo. |
| 2 | Mayor, activación de la ventana | Desde el primer sondeo, el editor no llegó al primer plano maximizado en tres de cuatro intentos, y el Explorador tampoco en su sondeo. El cliente activa la ventana principal del editor con `main_window(pid)`, que cuenta una solicitud de activación aunque no halle ninguna ventana. Causa no verificada: (a) la ventana principal del proceso no es la que muestra el editor; (b) el bloqueo de primer plano de Windows; (c) la maximización en el monitor principal. La disposición de monitores era normal. | Abierto. Qué lo resolvería: un sondeo supervisado, sin entrada, que registre las ventanas de nivel superior del proceso (clase, visibilidad, área, estado maximizado) y el primer plano durante la activación. Después, un cambio del cliente con prueba. |
| 3 | Mayor, E2E-02 | En el sondeo que sí llegó al primer plano, el motor no tiene ningún elemento con etiqueta de accesibilidad (0 de 69), y «Editor de texto» no aparece. «Guardar» sólo por OCR. | Abierto. Causa no verificada: el árbol de UI Automation del editor (egui sobre AccessKit) puede no estar disponible en la primera observación, o el proveedor de accesibilidad no ve esa ventana. Qué lo resolvería: una lectura de sólo lectura, con el editor en primer plano, que compare su árbol de UI Automation con las etiquetas del motor. |
| 4 | Menor, uso del cliente | El ejecutable relativo (`target/release/ommatix.exe`) no arrancó en Windows (`WinError 2`): no se lanzó proceso ni se gastó solicitud. | Sin cambio del cliente. Pasar siempre rutas absolutas en `--exe`, `--config`, `--results` y `--ledger`. |
| 5 | Observación | Las ventanas que no llegaron al primer plano tenían entre 60 s y 257 s de inactividad del propietario; la que sí llegó tenía 49 s. La inactividad no explica el patrón. | Sin cambio. |
| 6 | Interpretación de la regla del orden | La regla («un piloto fallido por agarre o seguridad no abre la serie») se aplicó a cualquier piloto que no pase, porque una serie que falla tres corridas seguidas crea `stop.flag`, que detiene también el Explorador. | Pendiente de confirmación del responsable. |

### 6. Estado en la matriz

Sin cambio. S4R-02, E2E-02 y E2E-03 siguen en `NO_EJECUTADO` y `PENDIENTE_PRODUCTO` en `matrices/plan_pruebas.csv`, porque no hubo corridas puntuadas y la evidencia no sostiene otro estado. RF-18, RNF-03, RNF-05 y las casillas de la Definition of Done no cambian.

### 7. Estado final del escritorio, del cuaderno y de `stop.flag`

- **Procesos de prueba:** ninguno (`ommatix.exe`, `CalculatorApp.exe`, `ommatix-testeditor.exe` y `ommatix-testapp.exe`).
- **Bloc de notas del propietario:** procesos 8960 y 23164, los mismos que al empezar. No se tocó ninguna de sus ventanas: el cliente sólo cierra los `HWND` que abre. No se contó el número de ventanas `CabinetWClass` antes del sondeo del Explorador, así que no hay referencia exacta de su estado previo.
- **Carpeta temporal `ommatix-e2e`:** no existe al final.
- **`stop.flag` y `window.pin`:** ausentes. El cliente no creó ninguno.
- **Cuaderno:** 427 (1 de 200 solicitudes de la ronda; 199 disponibles).
- **Capturas y texto de pantalla:** ninguno guardado. Los registros contienen estados, códigos, contadores, tiempos, tipos de control, `AutomationId`, identificadores de elemento y las etiquetas fijas del protocolo.

### 8. Qué queda y cómo reanudar

1. **Responsable:** confirmar, con el panel abierto y sin guardar el texto, el nombre exacto del elemento Estándar, y decidir si la lista cerrada `CALC_STANDARD_NAMES` lo admite. Eso exige un cambio del cliente en una ronda aparte, con su prueba y un hash nuevo.
2. **Responsable:** decidir cómo diagnosticar el primer plano del editor y del Explorador: un sondeo supervisado sin entrada, o un cambio del cliente con su prueba.
3. **Responsable:** decidir cómo se resuelve la etiqueta de accesibilidad del campo «Editor de texto» (un diagnóstico de sólo lectura con el editor en primer plano, o un cambio de la aplicación o del contrato, que queda fuera de esta rama).
4. **Responsable:** confirmar la interpretación de la regla del piloto (hallazgo 6).
5. **Cuaderno:** 427. La siguiente ronda usa su propio `--round-start`.

## Preparación de la novena ronda: el nombre del elemento Estándar deja de ser condición, la ventana se pide por su identificador con varios métodos, y el editor se vigila

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-round8-blockers`, sobre `main` en `e9ebadc` (fusión de #20). Cambian los dos clientes (`tools/s4/e2e_driver.py`, SHA-256 **`cbe6905c2f582955ddd626a167716e2062d39b294b3033ff706c09c5e4821265`**, antes `a16c53af…ae22d`; `tools/s4/e2e_dossier.py`, SHA-256 **`c1ba4a376a5df88bb696c07945a1b0fcd6da1ac0365d81f19348304d9061020e`**, antes `cd959f59…f306`), el editor (`tools/s4/testeditor`; compilación `--release` en esta máquina, SHA-256 **`0dcfb86b7bee3e5530ffab46c26f37e21c3d454321d93c7eb391f0e4c33d4e78`**, antes `a6765dca…119d`), sus pruebas y la documentación. El motor no cambia (ninguna crate de Rust). **No se envió entrada de ningún tipo.** Las comprobaciones de sólo lectura se hicieron sobre un `ommatix-testeditor.exe` propio, lanzado y cerrado sin enviarle nada. Registro por hallazgo en la [revisión](../reviews/2026-10-14-s4-eighth-round.md) (sección «Resolution of findings 1 to 3»).

### Decisiones del responsable

1. **Hallazgo 1.** El elemento se identifica por `AutomationId` `Standard` + tipo de control de la lista cerrada + exactamente uno en pantalla + `RuntimeId`. **Se elimina la lista de nombres.**
2. **Hallazgo 2.** La causa es el bloqueo de primer plano de Windows (el botón de la barra de tareas parpadea y la ventana queda detrás). Se corrige sin entrada y sin pulsar la barra de tareas (la ADR-0020 rechaza `Shell_TrayWnd`).
3. **Hallazgo 6 de la octava ronda.** Un piloto que no pasa, por cualquier causa, bloquea su serie.

### 1. S4R-02: el nombre sólo en memoria

La regla de `--calc-standard` queda así: el sondeo (3 s, pasos de 0,15 s, sólo lectura) busca el elemento en pantalla con `AutomationId` `Standard` y tipo `ListItem`, `TreeItem`, `TabItem`, `MenuItem` o `RadioButton`; se necesita exactamente uno y con `RuntimeId`. Su `Name` se lee, normalizado a NFC, **sólo en memoria**, para seleccionar el elemento del motor por etiqueta exacta (único y confirmado, tras una observación nueva). La identidad debe ser el mismo `RuntimeId` (`pinned_runtime_id`) y el anclaje tras el clic, con el descenso por la isla, no cambia. **Sin clic** si hay dos candidatos (`several_candidates`), si falta el `RuntimeId` (`unidentifiable`), si pasa el tiempo (`timeout`), si el elemento del motor falta o no es único o confirmado, o si otro `RuntimeId` lleva ese nombre (`not_the_polled_item`). El nombre no aparece en ningún registro, resumen, archivo de resultados ni salida: `standard_item` ya no tiene `name`, `name_accepted` ni `by_name`, y `unknown_name` y `CALC_STANDARD_NAMES` desaparecen. Un elemento llamado «Calculadora Estándar» (o cualquier otro texto) se selecciona y se ancla.

### 2. Primer plano: causa, métodos y diagnóstico

- **Causa verificada en parte.** Reproducido dos veces con el editor propio: la ventana estaba visible, el primer plano siguió en otra aplicación y el único método del cliente (`AttachThreadInput` más `SetForegroundWindow`) devolvió falso. Segundo defecto, verificado: `place_on_primary(pid)` restauraba y maximizaba **todas** las ventanas visibles del proceso, también `Winit Thread Event Target` (visible, 16 × 16 en el origen), que quedaba maximizada a 1920 × 1080; `SW_MAXIMIZE` activa, así que ese ayudante podía ser la ventana de primer plano, y `foreground()` entonces informaba «maximizada en el monitor principal» para él. Que esto explique el único éxito de diez solicitudes **no está verificado**. Descartado: que `main_window(pid)` devolviera una ventana que el editor no muestra. El Explorador ya se activaba por su identificador registrado; su fallo es el mismo bloqueo, **no reproducido** aquí (no se abrió ninguna ventana del Explorador).
- **Métodos (sin entrada).** `activate_window(hwnd)` prueba, en orden, `attach_foreground` (el hilo del cliente se une a la cola de entrada de la ventana en primer plano y llama a `SetForegroundWindow`), `attach_show_top` (además muestra, restaura si está minimizada, sube y activa la ventana) y `attach_all_topmost` (además une el hilo de la propia ventana y alterna su orden Z entre superior y normal). Cada uno se verifica con `GetForegroundWindow`; el que funcionó queda en `activation_method`, y `activation_tried` cuenta los intentos por nombre (`none` si no funcionó). Nunca `SystemParametersInfo` ni cambio alguno de la configuración del usuario, ni teclas (ningún truco de Alt), ni clics, ni la barra de tareas. La ventana se pide **por su identificador** cada 2 s: la principal del proceso (nunca un ayudante), la del Bloc de notas registrada o la del Explorador registrada.
- **Ayudantes.** `place_on_primary` ya no toca las ventanas ayudantes; `require_foreground` y la espera rechazan un primer plano cuya clase sea un ayudante; los pasos leen la ventana principal (`app_window`), no la de primer plano.
- **Diagnóstico de sólo lectura** si la ventana no llega: `activation_diagnostic` con, por ventana del proceso (o por el identificador registrado), clase, visible, minimizada, maximizada y si está en el monitor principal, y la clase y el proceso de la ventana de primer plano. Nunca títulos. El evento `activation_failed` lo repite en el registro.
- **`--operator-activation` (apagado por defecto; en ambos clientes; `--operator-activation-wait`, 120 s).** Si el bloqueo gana, el cliente escribe en stderr que el operador debe pulsar el botón parpadeante de la barra de tareas de esa aplicación, espera, con tiempo acotado, a que `GetForegroundWindow` sea el identificador registrado y a que se cumplan las condiciones de siempre, y registra `operator_activation` (`true`; `false` si se agotó el tiempo). No envía nada antes. El motor sigue rechazando una ventana que no sea el primer plano.
- **El editor** pide el foco durante 2 s y se maximiza una vez al inicio, como la aplicación de prueba. No envía entrada.

### 3. E2E-02: la etiqueta del campo

- **Volcado de sólo lectura** del editor propio, a unos 350 ms de su arranque y sin ser la ventana de primer plano: `Text` «Editor», `Edit` «Editor de texto» (patrón Value disponible y no de sólo lectura), `Button` «Guardar» y `Text` «Estado: pendiente». El campo de varias líneas es un `Edit` con Value escribible, su nombre está en el nodo que lee el adaptador, el árbol existe en la primera lectura y no se trunca. **No hay defecto del árbol del editor ni del mapa de roles del adaptador**, y el adaptador no cambia.
- **Qué falta saber.** El motor atestigua la aplicación por el elemento con foco de teclado de UI Automation. Cero elementos de accesibilidad en el sondeo significa que ese elemento no estaba en el editor en esa lectura. Por qué, **no está verificado**: exige que el editor sea la ventana activa mientras el motor lee, lo que el bloqueo impidió. Un candidato es la ventana ayudante maximizada del punto anterior.
- **Qué hace el cliente ahora.** El flujo del editor vuelve a observar (hasta 6 veces) hasta que el motor informe elementos de origen `accessibility`; si no llegan, se detiene antes de toda entrada con `no accessibility evidence from the editor`, `accessibility_elements: 0` y `uia_focus` (proceso, clase, tipo de control y clase de la ventana superior del elemento con foco, y si pertenece al editor; sin nombres ni valores).

### Pruebas

`python -m unittest discover -s tools/s4 -p "test_*.py"`: **230 pruebas correctas** (193 antes). Con los clientes de `main`, 39 de las nuevas o modificadas fallan (8 fallos y 31 errores). Cubren: nombre cualquiera → seleccionado y anclado; el nombre no está en ningún registro, resumen ni registro de salida; dos candidatos, sin `RuntimeId`, elemento del motor ausente o no único, o `RuntimeId` distinto → sin clic; el escalón de métodos con dobles (primero que funciona, ya en primer plano, ninguno, un método que falla, verificación por `GetForegroundWindow` y no por el valor devuelto, ausencia de `SystemParametersInfo`, teclas, ratón y barra de tareas en el código); `place_on_primary` no toca ayudantes; la ventana pedida por su identificador (también la del Bloc de notas y la del Explorador); el diagnóstico sin títulos; la bandera del operador (apagada por defecto, aviso en stderr, sólo el identificador registrado vale, tiempo acotado, nada si el escalón funcionó); el editor (nueva observación sin accesibilidad, parada antes de toda entrada, `uia_focus`, ventana principal). Editor: 16 pruebas (15 y una nueva). Rust: 383 sin cambio.

### Qué debe mostrar la próxima ronda en vivo

- **Antes:** comprobar los SHA-256 de arriba, el del motor (sin cambio, `3a451211…cbb1a`) y que no existan `stop.flag` ni `window.pin`; las rutas de los argumentos, absolutas (hallazgo 4); `--round-start` propio (cuaderno 427).
- **S4R-02:** `standard_item.status` `ok` con cualquier nombre, dos clics anclados y `mode_after` `standard`; el nombre ausente de todo registro.
- **E2E-02 y E2E-03:** el sondeo debe llegar al primer plano, con `activation_method` (cuál de los tres) o, si se usa `--operator-activation`, `operator_activation` `true`; si no llega, `activation_diagnostic` dice por qué. En el editor, `accessibility_elements` mayor que cero y «Editor de texto» como candidato `input` confirmado, con etiqueta de accesibilidad; si no, `uia_focus` dice a quién pertenecía el foco. Un piloto que no pase bloquea su serie.
- **Nada de esto está confirmado en vivo.**

### Estado

Hallazgos 1, 2 y 3: corregidos en los clientes y el editor y probados con dobles; la causa del 1 está verificada, la del 2 en parte (bloqueo reproducido y ayudante maximizado verificado; el Explorador y el vínculo con el único éxito, no) y la del 3 no (se descartó el árbol del editor y el adaptador). Cuaderno: 427, sin cambio. Las matrices no cambian.

## Novena ronda: S4R-02 se detuvo en el segundo clic de «Estándar» por la regla de seguridad; E2E-02 y E2E-03 no se ejecutaron; ninguna corrida puntuada

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-ninth-round`, desde `main` en `9ef66ef` (fusión de #21). No cambia ningún código: ni los dos clientes ni las crates de Rust. Se usaron el cliente comprometido `tools/s4/e2e_driver.py` (SHA-256 `cbe6905c…4821265`) y `tools/s4/e2e_dossier.py` (SHA-256 `c1ba4a37…1020e`), comprobados antes de cada invocación en vivo; el motor de `main` (`target/release/ommatix.exe`, SHA-256 `3a451211…cbb1a`, el mismo de la octava ronda, porque cargo lo dio por actualizado); el editor compilado desde `main` (`tools/s4/testeditor/target/release/ommatix-testeditor.exe`, SHA-256 `541fc0a8…2793`, distinto del de la octava ronda, `0dcfb86b…4e78`, porque la fusión #21 cambió el editor); y las configuraciones `s4r9-live.toml` (SHA-256 `0428daf8…5ec1e2`) y `s4r9-explorer.toml` (SHA-256 `ab6c33b5…233d7`), copias sin cambios de las de la octava ronda. Autorización del responsable del 14 de octubre de 2026: `calculatorapp.exe` (S4R-02), `ommatix-testeditor.exe` (E2E-02) y `explorer.exe` sólo en la ventana `CabinetWClass` que abre el cliente (E2E-03, ADR-0020); cupo de 200 solicitudes con `--round-start 427`; `--operator-activation` apagado; el responsable no usó el equipo. Regla de seguridad de esa autorización: un clic anclado en un control incorrecto, un falso éxito del motor o una entrada fuera del flujo o de la ventana permitida detienen **todo** el trabajo en vivo.

**Resultado:** ninguna corrida puntuada. En `--calc-standard`, el primer clic (navegación) se entregó y quedó anclado. El segundo clic (sobre «Estándar») se entregó con `effect_observed`, pero su anclaje posterior no halló el elemento: según la definición del cliente en `steps()` es un `grounding_wrong` y un `engine_false_success` (el resumen de `--calc-standard` no imprime esos contadores, hallazgo 2). Por la regla de seguridad, el trabajo en vivo se detuvo ahí: no hubo piloto ni serie de S4R-02, y E2E-02 y E2E-03 no se ejecutaron, ni siquiera sus sondeos. Se enviaron **dos** solicitudes reales (cuaderno 427 → 429, de 200). No se creó `stop.flag`: el paso se detuvo con su propia excepción de aborto, y ni el cliente ni el agente lo crearon.

### 1. Verificación previa a la entrada en vivo

- `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: ya al día; el binario no cambió (SHA-256 `3a451211…cbb1a`). `cargo build --release` en `tools/s4/testeditor`: recompilado desde `main`, SHA-256 `541fc0a8…2793`.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings` y `cargo clippy -- -D warnings` en `tools/s4/testeditor`: correctos, sin avisos.
- `cargo test --workspace`: 383 pasadas, 0 fallidas, 1 ignorada. `cargo test` en `tools/s4/testeditor`: 16 pasadas.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 230 pruebas correctas.
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48; `validar_paquete.py`, 218 comprobaciones y 0 fallidas. Cuatro archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y el fin de línea; se restauraron con `git restore`.
- Antes de cada invocación en vivo, un preflight (fuera del repositorio, sólo lectura) comprobó por SHA-256 los dos clientes, el motor, el editor, las dos configuraciones, el manifiesto float16 (`f8b04be9…fe47`) y las bibliotecas de ONNX Runtime y DirectML de la configuración; que no existieran `stop.flag` ni `window.pin`; el cuaderno; y que no hubiera procesos de prueba.
- Escritorio antes de empezar: ningún proceso de prueba; los dos procesos `notepad.exe` del propietario (PID 8960 y 23164) con sus dos ventanas minimizadas; ninguna ventana `CabinetWClass`.

### 2. Ejecución en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| S4R-02, sondeo | `e2e_driver.py --app calculator --mode probe --runs 1` con `s4r9-live.toml` | 0 | Los cinco elementos del flujo confirmados por accesibilidad, un candidato cada uno (`Siete`, `Más`, `Ocho`, `Es igual a` y `Borrar`). Primer plano en 0,92 s (`already_foreground`). Sin entrada. |
| S4R-02, `--calc-standard` | `--app calculator --mode run --calc-standard --max-live 4 --round-start 427` | 2 | Clic 1 «Abrir navegación» entregado, `effect_observed` y anclado. Clic 2 «Estándar» entregado, `effect_observed`, **no anclado**: paso detenido por la regla de seguridad. La Calculadora que lanzó se cerró. |
| S4R-02, piloto y serie | no se ejecutaron | 0 | Detenidos por la regla de seguridad (§3.2). |
| E2E-02, sondeo, piloto y serie | no se ejecutaron | 0 | Detenidos por la regla de seguridad. |
| E2E-03, sondeo, piloto y serie | no se ejecutaron | 0 | Detenidos por la regla de seguridad. |

Cuaderno: 427 → 429, es decir, 2 solicitudes reales de 200 (198 disponibles). Las dos son las del `--calc-standard`.

### 3. Resultados

#### 3.1 S4R-02, Calculadora: el elemento «Estándar» se halló, y el segundo clic no quedó anclado

- **Sondeo.** Los cinco elementos del flujo tienen un candidato cada uno, confirmado por accesibilidad. Sobre 130 elementos del motor: 49 con origen `accessibility`, 4 con `ocr` y 77 sin origen.
- **Modo al empezar:** `other`.
- **Clic 1, «Abrir navegación».** Un candidato (`TogglePaneButton`, `Button`), identidad `ok` antes del clic. Entregado, `effect_observed` y anclado tras el clic. `_meta`: `action_total` 73,1 ms, `action_verification` 57,1 ms, `action_freshness` 14,1 ms, `action_delivery` 1,5 ms, `action_gate` 0,35 ms; rtt 73,4 ms.
- **Paso 2, «Estándar».** La primera lectura de UI Automation (391 ms de la ventana de 3 s) halló un único candidato: `AutomationId` `Standard`, tipo `ListItem`, en pantalla. Su nombre no se registró. La identidad fue `ok` antes del clic (el `RuntimeId` del elemento y el del motor coinciden). El motor le asigna rol `unknown` con origen `accessibility`.
- **Clic 2, «Estándar».** Entregado (`input_delivered`), con `effect_observed` del motor. Después del clic, el control bajo el cursor no es el elemento hallado antes: `grounded: false`. `_meta`: `action_total` 99,7 ms, `action_verification` 70,4 ms, `action_freshness` 26,9 ms, `action_delivery` 2,1 ms, `action_gate` 0,25 ms; rtt 99,9 ms. El paso se detuvo sin más clics.
- **Modo al terminar:** `other`, pero se leyó **una sola vez**, justo después del clic 2 y sin esperar: el camino de parada no repite la lectura, sólo el de éxito (ocho lecturas separadas 0,5 s). Por eso `other` no demuestra que el clic no cambiara el modo.
- **Activación:** `already_foreground`, `foreground_wait_s` 0,87, `activation_requests` 1, `window_placements` 2.
- **Qué confirma y qué no.** Confirma, como la octava ronda, que el elemento existe con `AutomationId` `Standard`, tipo `ListItem` y único en pantalla. **No** establece si el clic 2 alcanzó ese elemento: el anclaje posterior no lo halló. Quedan sin distinguir, causa no verificada: que el clic cayera en otro control; que el panel cambiara antes de la lectura; o que el modo cambiara después de ella. El registro sólo muestra dos entregas, ambas dentro del flujo; por diseño, el motor entrega una acción sólo tras su puerta, que comprueba la aplicación en primer plano (ADR-0007, ADR-0020).
- **Piloto y serie.** No se ejecutaron: el modo Estándar no quedó confirmado y el paso se detuvo.

#### 3.2 Por qué se detuvo

El responsable autorizó detener todo el trabajo en vivo ante «un clic anclado en un control incorrecto» o «un falso éxito del motor». El clic 2 entra en esa definición según los criterios del cliente (`grounding_wrong` y `engine_false_success`): se entregó, el motor informó `effect_observed` y el anclaje posterior no lo confirmó. El agente no decidió si el clic fue correcto; aplicó la regla. Por la misma regla no se ejecutaron los sondeos sin entrada de E2E-02 y E2E-03, que también abren ventanas a través de sus clientes. Reanudar exige decisión del responsable (§9).

#### 3.3 E2E-02 y E2E-03: no ejecutados

Ni el sondeo, ni el piloto, ni la serie. No hay datos de `accessibility_elements`, `uia_focus` ni de la navegación del Explorador (`targets` ni `tree_unchanged`).

### 4. Totales y seguridad

- Solicitudes reales: 2 (cuaderno 427 → 429). Entregadas: 2, ambas `effect_observed`. Verificadas por identidad: 1 de 2. Rechazadas: 0. `indeterminate`: 0. `POLICY_DENIED`: 0. Reintentos por `STALE_SNAPSHOT`: 0.
- Falsos éxitos según la definición del cliente: 1 (clic 2, no puntuado). `grounding_wrong`: 1 (no puntuado). Parada de seguridad: sí (§3.2).
- Corridas puntuadas: 0. Pilotos aprobados: 0.
- Capturas y texto de pantalla: ninguno guardado. Los registros contienen estados, códigos, contadores, tiempos, tipos de control, `AutomationId`, identificadores de elemento y las etiquetas fijas del protocolo (`Abrir navegación`). El nombre del elemento «Estándar» no aparece en ningún registro de la ronda (comprobado).

### 5. Latencias (`_meta`)

Dos acciones sueltas de la misma invocación, sin serie: no hay mediana ni p95, y no miden RNF-05 (S0 §10). Valores en §3.1.

### 6. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor, seguridad (Calculadora) | El clic 2 se entregó con `effect_observed` y su anclaje posterior falló; el modo no se volvió a leer tras esperar. Por la regla de seguridad se detuvo todo el trabajo en vivo. | Abierto, decisión del responsable (§9). Causa no verificada: otro control, panel que cambió antes de la lectura o modo que cambió después. |
| 2 | Menor, registro | El resumen de `--calc-standard` no imprime los contadores `grounding_wrong` ni `engine_false_success`; el caso sólo aparece en `clicks[].grounded`. | Abierto. Corrección propuesta: imprimirlos. Requiere prueba que falle sin el cambio y un hash nuevo, en una ronda aparte. |
| 3 | Menor, registro | Al detenerse, `mode_after` se lee una vez, justo tras el clic, y el paso no espera. `other` no es concluyente. | Abierto. Corrección propuesta: leer el modo con espera acotada también al detenerse. Misma condición que el hallazgo 2. |
| 4 | Observación | El elemento «Estándar» tiene rol `unknown` en el motor (origen `accessibility`); el clic de navegación tiene rol `button`. | Sin cambio. Se desconoce si influye en el anclaje. |
| 5 | Observación | La Calculadora ya estaba en primer plano (`already_foreground`); no hizo falta la escalera de activación. El bloqueo de primer plano del editor y del Explorador (octava ronda, hallazgo 2) no se volvió a probar. | Abierto. Se probaría en la siguiente ronda, con sus sondeos. |
| 6 | Interpretación (decisión del responsable) | La regla de seguridad se aplicó también a los sondeos sin entrada de E2E-02 y E2E-03. | Pendiente de confirmación del responsable. |

### 7. Estado en la matriz

Sin cambio. `matrices/plan_pruebas.csv` mantiene S4R-02, E2E-02 y E2E-03 en `NO_EJECUTADO` y `PENDIENTE_PRODUCTO`: no hay corridas puntuadas ni piloto aprobado que lo justifique. RF-18, RNF-03, RNF-05 y las casillas de la Definition of Done no cambian.

### 8. Estado final del escritorio, del cuaderno y de `stop.flag`

- **Procesos de prueba:** ninguno (`ommatix.exe`, `CalculatorApp.exe`, `ommatix-testeditor.exe` y `ommatix-testapp.exe`). La Calculadora que lanzó el paso se cerró.
- **Bloc de notas del propietario:** procesos 8960 y 23164, los mismos que al empezar. Sus dos ventanas siguen minimizadas; ningún cliente de esta ronda las tocó.
- **Explorador:** ninguna ventana `CabinetWClass` antes ni después; el Explorador no se ejecutó. No existe `window.pin`.
- **`stop.flag`:** ausente. Ni el cliente ni el agente lo crearon.
- **Cuaderno:** 429 (2 de 200 solicitudes de la ronda; 198 disponibles).

### 9. Qué queda y cómo reanudar

1. **Responsable:** decidir si la ronda continúa tras la parada de seguridad, y en qué condiciones (hallazgo 6). Sin esa decisión no se ejecuta nada en vivo, ni siquiera un sondeo.
2. **Responsable:** decidir cómo investigar el clic 2 (hallazgo 1): una lectura de sólo lectura del panel con el responsable presente, o aceptar el riesgo y reanudar. El cupo se decide en el punto 4.
3. **Cliente (ronda aparte, con prueba y hash nuevo):** hallazgos 2 y 3, para que el resumen y el modo al detenerse digan lo que pasó.
4. **Si la ronda continúa:** el cupo lo decide el responsable. O se sigue con `--round-start 427`, que deja 198 solicitudes de las 200 autorizadas, o se abre una ronda nueva con su propio `--round-start` (el cuaderno está en 429). En cualquier caso, el mismo preflight antes de cada invocación; después, los sondeos de E2E-02 y E2E-03, que la octava ronda no completó.
5. **Cuaderno:** 429. La siguiente ronda usa su propio `--round-start`.

### 10. Ejecución

Dos invocaciones en vivo: el sondeo de la Calculadora (sin solicitudes) y `--calc-standard` (dos solicitudes). Las únicas entradas fueron dos clics de la Calculadora: «Abrir navegación» (`Button`, `AutomationId` `TogglePaneButton`) y «Estándar» (`ListItem`, `AutomationId` `Standard`). No se escribió nada, no se usó ninguna otra aplicación y no se tocó ninguna ventana del propietario. Las lecturas de UI Automation son de sólo lectura. El preflight y la instantánea de ventanas (ambos fuera del repositorio) no envían entrada ni cambian ventanas.

## Novena ronda (continuación): ninguna solicitud nueva; S4R-02 no se ejecutó, el piloto de E2E-02 no pasó y E2E-03 no se ejecutó en vivo

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-ninth-round-continuation`, desde `main` en `a5cd1f9` (fusión de #22). No cambia ningún código: ni los clientes, ni el editor, ni las crates de Rust. Autorización del responsable para la continuación: la Calculadora se pasó a modo Estándar a mano y no se vuelven a usar los clics de `--calc-standard`; el cupo sigue en 200 solicitudes desde `--round-start 427`; `--operator-activation` apagado; el responsable no usó el equipo. La parada de seguridad de la primera parte queda levantada para esta continuación; la regla se mantiene.

**Resultado:** ninguna corrida puntuada y **ninguna solicitud real nueva**. El cuaderno sigue en 429 (2 de 200 de la ronda; 198 disponibles). No se envió entrada a ninguna aplicación: cada invocación en vivo se detuvo antes de su primera acción.

- **S4R-02 (Calculadora):** no se ejecutó. La comprobación de modo, de sólo lectura, dio `other` en dos lecturas (§3.1).
- **E2E-02 (editor):** dos sondeos sin entrada y un piloto. Ninguno tuvo evidencia de accesibilidad: el foco de teclado lo tiene el escritorio, no el editor. El piloto no pasó, así que bloquea la serie (§3.2).
- **E2E-03 (Explorador):** el sondeo sin entrada muestra que los objetivos de la navegación no se encuentran como el flujo los define. No se ejecutó en vivo (§3.3).

### 1. Verificación previa a la entrada en vivo

- Clientes, sin cambio: `tools/s4/e2e_driver.py` `cbe6905c…4821265`; `tools/s4/e2e_dossier.py` `c1ba4a37…1020e`. Comprobados por SHA-256 antes de cada invocación en vivo.
- Motor, `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: ya al día; SHA-256 `3a451211…cbb1a`, igual que en la novena ronda.
- Editor, `cargo build --release` en `tools/s4/testeditor`: ya al día; SHA-256 `541fc0a8…2793`.
- Configuraciones, sin cambio: `s4r9-live.toml` `0428daf8…5ec1e2`; `s4r9-explorer.toml` `ab6c33b5…233d7`. Manifiesto float16, ONNX Runtime y DirectML, los del comprobador (`f8b04be9…fe47`, `e7eedec6…7bff40`, `9c9e6d82…e92da1`).
- Antes de cada invocación del cliente, el comprobador de la ronda (fuera del repositorio, sólo lectura) verificó lo anterior, la ausencia de `stop.flag` y de `window.pin`, el valor del cuaderno (429) y la ausencia de procesos de prueba. La segunda lectura de modo no tuvo ese comprobador delante: su propio script verificó el SHA-256 del cliente, y después se comprobaron el cuaderno, `stop.flag` y `window.pin`, sin cambios.
- Comprobaciones previas: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`; `cargo test --workspace` con 383 pasadas, 0 fallidas y 1 ignorada; editor: `cargo build --release`, `cargo clippy -- -D warnings` y `cargo test` con 16 pasadas; `python -m unittest discover -s tools/s4 -p "test_*.py"` con 230 correctas; desde `docs/thesis`, `referencia_ejecutable/run_tests.py` con 48 de 48 y `validar_paquete.py` con 218 comprobaciones y 0 fallidas. Todo ello antes de la primera acción en vivo.

### 2. Invocaciones en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| S4R-02, comprobación de modo | Lectura de sólo lectura con `read_calc_mode` del cliente, en un script externo que lanza la Calculadora, la lee y cierra sólo su proceso | 0 | `other` en las dos lecturas (§3.1). |
| S4R-02, piloto y serie | No se ejecutaron | 0 | Modo distinto de Estándar: no se hizo clic en ningún control de modo ni de navegación. |
| E2E-02, sondeo 1 | `e2e_dossier.py --app editor --mode probe --runs 1` con `s4r9-live.toml` | 0 | Sin elementos de accesibilidad. Primer plano `already_foreground`. Foco de teclado en el escritorio. |
| E2E-02, sondeo 2 | Igual, con `--settle 5` | 0 | Igual. |
| E2E-02, piloto | `--app editor --mode run --runs 1 --max-live 8` | 0 | No pasó: sin evidencia de accesibilidad; se detuvo antes de toda entrada. Bloquea la serie. |
| E2E-02, serie | No se ejecutó | 0 | Bloqueada por el piloto. |
| E2E-03, sondeo | `e2e_dossier.py --app explorer --mode probe --runs 1` con `s4r9-explorer.toml` | 0 | Primer plano por `attach_show_top` en la primera solicitud de activación (gestión de ventanas, sin entrada). Ventana registrada, cerrada por su identificador; árbol temporal borrado (2 archivos y 3 carpetas); archivo de ventana escrito y borrado. Objetivos: §3.3. |
| E2E-03, piloto y serie | No se ejecutaron | 0 | El sondeo no encuentra los objetivos del flujo. |

Cada invocación con el cliente se hizo con rutas absolutas y el cuaderno en 429. La configuración de sondeo no envía entrada: el motor se arranca sin `--input sendinput`.

### 3. Resultados

#### 3.1 S4R-02: no se ejecutó

- **Por qué no se usó `--calc-standard --max-live 0`.** Es la invocación que propone el protocolo, pero (verificado en el código, no en vivo) con un modo distinto de Estándar llega a `Protocol.spend_live`: `steps()` pasa por `require_foreground()` e `identify_before()`, y `spend_live()` con cupo 0 llama a `engage_stop`, que **crea `stop.flag`** antes de abortar. Esa parada habría detenido también E2E-02 y E2E-03, que el responsable ordenó ejecutar sea cual sea el modo. La lectura se hizo con un script de sólo lectura que importa el cliente sin cambios y usa su función de UI Automation; ese script no llega a la ruta de ejecución y no escribe en el cuaderno ni crea `stop.flag`.
- **Lectura.** `other` en las dos lecturas (la segunda, con las señales por recuento). El encabezado existe, pero su nombre no es «Estándar» ni «Calculadora estándar», y no hay controles propios de Científica. Este registro no distingue dos explicaciones: la Calculadora está en otro modo (Programador, Fecha o Conversor), o el nombre del modo Estándar en esta compilación no es el de la lista del cliente. Ninguna lectura en vivo de `--calc-standard` ha dado `standard`: las de las rondas 5, 7 (continuación), 8 y 9 dan `other`, y la señal del encabezado no se había leído en vivo (§«Preparación de E2E-02 y E2E-03», «Sin comprobar en vivo»).
- **Consecuencia.** El cambio a mano que el responsable hizo en la Calculadora no aparece en la lectura del cliente. El paso no se ejecuta según la regla del responsable (§5, hallazgo 8).

#### 3.2 E2E-02: el editor no recibe el foco de teclado

- Los dos sondeos y el piloto dieron `activation_method` `already_foreground`: la ventana del editor ya es el primer plano y está maximizada, así que no se aplicó ninguno de los métodos de activación. UI Automation indica como propietario del foco de teclado el escritorio (clase `#32769`, tipo `Pane`), no la aplicación (`is_app` falso). Con 1,5 s de espera (sondeo 1) y con 5 s (sondeo 2), seis observaciones cada uno sin ningún elemento con origen de accesibilidad. El cliente se detuvo antes de toda entrada, como está diseñado.
- **Causa no verificada.** Candidato: la escalera de activación no hace nada cuando la ventana ya es el primer plano, así que nada pide el foco de teclado para ella. Comprobarlo exige un cambio de cliente o del editor con prueba; hasta que el motor vea el campo no hay corrida posible con entrada.
- El piloto no pasó, por lo que la serie queda bloqueada según la regla del responsable (un piloto que no pasa, por cualquier causa, bloquea su serie).

#### 3.3 E2E-03: los objetivos de la navegación no son los que define el flujo

Lo que muestra el sondeo, sin ningún clic:

- **«carpeta-a»** (el flujo lo busca en `left_sidebar` como `TreeItem`): el motor ve un elemento confirmado con esa etiqueta, en zona `unknown` y rol `unknown`. UI Automation ve un único elemento en pantalla con ese nombre y tipo `ListItem`; no hay ningún `TreeItem` con ese nombre.
- **«carpeta-b»**: no está en pantalla antes de elegir «carpeta-a». Es lo esperado: el árbol sólo lo muestra después del primer clic, que el sondeo no hace.
- **La raíz** (el flujo la busca en `top_bar` como `Button` o `SplitButton`): el motor la ve confirmada, en zona `unknown`; UI Automation la ve como `SplitButton`, que sí es un tipo permitido.
- Con los filtros de zona y de tipo del flujo, ninguno de los objetivos de navegación se encuentra, así que el cliente no habría enviado clic alguno. La regla del responsable para este caso (no ejecutar en vivo si los objetivos no son alcanzables con clics sencillos) se aplicó.
- **No verificado:** por qué el motor da zona `unknown` en esta ventana, y si el «carpeta-a» de tipo `ListItem` pertenece al panel de navegación o a la lista de archivos.

#### 3.4 Totales y seguridad

- Solicitudes reales en la continuación: **0**. Cuaderno: 429 antes y después (ronda: 2 de 200).
- Corridas puntuadas: 0. Pilotos aprobados: 0. Falsos éxitos, clics anclados mal, `indeterminate`, `POLICY_DENIED`, `STALE_SNAPSHOT` y rechazos: 0, porque no hubo acciones.
- Parada de seguridad: no se activó. `stop.flag` ausente al terminar y nunca creado; `window.pin` ausente.
- Capturas y texto de pantalla: ninguno guardado. Los registros contienen estados, contadores, identificadores, tipos de control y las etiquetas fijas del editor y de los flujos. Los nombres de carpeta son los que el propio cliente creó.
- Aborto: el piloto de E2E-02 se detuvo por falta de evidencia de accesibilidad, antes de cualquier entrada. No hubo otra parada.

#### 3.5 Latencias

No hubo acciones, así que no hay latencias de `_meta`. Los tiempos de cada invocación (de 5,5 s a 5,9 s por corrida) no miden RNF-05 (S0 §10).

### 4. Escritorio final

- Calculadora: la lanzó y la cerró el script de lectura, dos veces, cerrando sólo su proceso. Ninguna queda en ejecución.
- Editor: lo lanzó y cerró el cliente en tres invocaciones. Ninguno queda en ejecución.
- Explorador: una ventana `CabinetWClass` abierta y cerrada por el cliente en el sondeo. El árbol temporal se borró (`%TEMP%\ommatix-e2e` queda vacío, sin entradas). `window.pin` se borró.
- Bloc de notas del responsable: procesos 8960 y 23164, sin cambio. Ninguna invocación de esta continuación lo tocó.
- `explorer.exe` (el shell): sin cambio. No se terminó ningún proceso del sistema.

### 5. Hallazgos de la continuación

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 8 | Mayor (S4R-02) | El modo Estándar no se ha leído nunca en vivo. Toda lectura de `--calc-standard` da `other`, también la de esta continuación, después del cambio a mano del responsable. | Abierto, decisión del responsable. Propuesta: un diagnóstico de sólo lectura (número y tipo de elementos `Header`, sin nombres) antes de la siguiente ronda; si la lista de nombres resulta errónea, un cambio de cliente con prueba y hash nuevo. |
| 9 | Menor, protocolo | Con `--calc-standard --max-live 0` y un modo distinto de Estándar, la invocación crea `stop.flag` antes de abortar (verificado en el código, no en vivo). No se usó. | Abierto. Corrección propuesta: que el paso no intente ningún clic con cupo 0, con una prueba que falle sin el cambio, en una ronda aparte. |
| 10 | Mayor (E2E-02) | El foco de teclado de UI Automation lo tiene el escritorio mientras el editor es el primer plano. Cero elementos de accesibilidad en dos sondeos y en el piloto. | Abierto. Causa no verificada; candidato: la escalera de activación no se aplica con `already_foreground`. Requiere un cambio de cliente o del editor, con prueba y hash, en una ronda aparte. |
| 11 | Mayor (E2E-03) | Los objetivos de la navegación no se encuentran como los define el flujo: zona `unknown` en el motor para «carpeta-a» y para la raíz, y «carpeta-a» es `ListItem` donde el flujo pide `TreeItem`. | Abierto, decisión del responsable: aceptar otro tipo y otra zona (cliente, prueba, hash), cambiar el contrato del flujo (cambio de escenario, con su entrada en el CHANGELOG) o investigar por qué el motor no asigna zona. |
| 12 | Observación | El hallazgo 5 de la primera parte (el escalón de activación no se había ejercido) se ejerce en parte: el Explorador usó `attach_show_top` en la primera solicitud; el editor ya estaba en primer plano. | Parcialmente cubierto. |
| 13 | Observación, gate | La prueba `ommatix-application --lib` que falló una vez en la primera parte no se reprodujo: `cargo test --workspace` de esta continuación pasó entero (383). | Abierto, sin cambio. |

Los hallazgos 1 a 7 de la primera parte siguen como estaban, salvo el 5 (ver el 12). Los hallazgos 2 y 3 (registro) siguen esperando un cambio de cliente en una ronda aparte.

### 6. Matriz

Sin cambio. `matrices/plan_pruebas.csv` mantiene S4R-02, E2E-02 y E2E-03 en `NO_EJECUTADO` y `PENDIENTE_PRODUCTO`: no hay corridas puntuadas ni piloto aprobado que lo justifique. RF-18, RNF-03, RNF-05 y las casillas de la Definition of Done no cambian.

### 7. Qué decide el responsable

1. **Si la ronda continúa y bajo qué condiciones.** El cupo no ha cambiado: quedan 198 solicitudes con `--round-start 427`, o puede abrirse una ronda nueva con su propio `--round-start` (el cuaderno está en 429).
2. **Hallazgo 8.** Si autoriza el diagnóstico de sólo lectura del encabezado antes de la siguiente ronda, o si acepta la lectura actual como definitiva.
3. **Hallazgo 10.** Cómo llevar el foco de teclado al editor (cambio de cliente, del editor o de ambos), con prueba que falle sin el cambio.
4. **Hallazgo 11.** Si E2E-03 sigue en esta ronda: cambiar el contrato de la navegación (dossier, con CHANGELOG), aceptar el tipo y la zona que hay (cliente, prueba, hash), o dejar E2E-03 fuera.
5. **Hallazgos 2, 3 y 9.** Cambios del cliente, en una ronda aparte, con prueba que falle sin el cambio y hash nuevo.

### 8. Ejecución

Las invocaciones de esta continuación fueron: la comprobación de modo (dos lecturas de sólo lectura), `e2e_dossier.py --app editor --mode probe` (dos veces, la segunda con `--settle 5`), `e2e_dossier.py --app editor --mode run --runs 1 --max-live 8` (el piloto, detenido antes de toda entrada) y `e2e_dossier.py --app explorer --mode probe --runs 1`. Los registros y los resultados JSONL están en la carpeta de ejecuciones, fuera del repositorio; ninguno contiene capturas ni texto de pantalla.

## Preparación de la décima ronda: la señal de modo de la Calculadora, la comprobación de sólo lectura, el foco del editor y el flujo del Explorador

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-round9-blockers`, sobre `main` en `27a9a52` (fusión de #23). Cambian los dos clientes (`tools/s4/e2e_driver.py`, SHA-256 **`143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`**, antes `cbe6905c…4821265`; `tools/s4/e2e_dossier.py`, SHA-256 **`85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`**, antes `c1ba4a37…1020e`), el editor (`tools/s4/testeditor`; compilación `--release` en el árbol de trabajo de esta rama, SHA-256 **`0a01b7a19ce25aa978794b8b8bd16590aef3d26c83d8e6274fe2acbfbd196cea`**, antes `541fc0a8…2793`; aplicación de prueba, SHA-256 **`8c1567f1e18521c957dbe5233c99424ea7456e297f266c06936e45ddc8aa1a0e`**; quien ejecute la ronda las recompila y registra las suyas), sus pruebas y la documentación. El motor no cambia (ninguna crate de Rust; `3a451211…cbb1a`). **No se envió entrada de ningún tipo ni ninguna solicitud real** (cuaderno 429, sin cambio). Las únicas comprobaciones sobre el sistema fueron de sólo lectura: un `ommatix-testeditor.exe` propio, lanzado y cerrado sin enviarle nada, y la lectura de quién tiene el foco de teclado. No se lanzó la Calculadora ni el Explorador. Registro por hallazgo en la [revisión](../reviews/2026-10-14-s4-ninth-round.md) (sección «Resolution of findings 2, 3 and 8 to 11»).

### 1. Calculadora: la señal de modo y `--check-mode` (hallazgos 2, 3, 8 y 9)

- **Señal de modo (8).** El modo es Estándar cuando el nombre del encabezado (`AutomationId` `Header`; el primero en pantalla, si no el primero), normalizado a NFC y en minúsculas, **contiene** «estándar» o «standard» **y** no hay ningún control sólo científico en pantalla; si las dos señales no coinciden, `unknown`; si el encabezado nombra otro modo, `other`. El encabezado es parte de la aplicación, no un dato del usuario. El registro (`mode_signals_before`, `mode_signals_after`) guarda siempre el tipo de control, el `AutomationId`, cuántos elementos `Header` hay y la longitud de su nombre; guarda el nombre **sólo** si cumple la regla (`header_matched: true`), y en otro caso `header_matched: false` y ningún nombre. **El nombre real del encabezado en la Calculadora que el responsable cambió a mano no se conoce:** si no contiene «estándar» ni «standard», la lectura seguirá siendo `other`, y el registro dirá su tipo, su cuenta y su longitud para saberlo. Un segundo candidato, corregido: `FindFirst` podía devolver un `Header` fuera de pantalla.
- **`--check-mode` (9).** Sólo `--app calculator`: lanza la Calculadora, espera el primer plano, lee el modo (con espera acotada de 2 s) y cierra sólo su proceso; sale con 0 si es Estándar y con 1 si no. No arranca el motor, no envía entrada ni solicitudes reales, no toca el cuaderno y **no puede crear `stop.flag`**, ni siquiera si ya hay una Calculadora que no lanzó (`Protocol.read_only` deja inertes `engage_stop` y `spend_live`). `--calc-standard --max-live 0` se rechaza al leer los argumentos (código 2, nada se lanza) en lugar de detener la ronda; `--check-mode` junto con `--calc-standard` o con otra aplicación también se rechaza.
- **Registro (2 y 3).** El resumen de `--calc-standard` imprime `grounding_wrong` y `engine_false_success`. `mode_after` se consulta con espera acotada (cada 0,5 s, hasta 2 s) tras el último clic, también cuando el paso se detiene con entrada entregada; si no hubo clic se lee una vez. Se anota `mode_after_reads`.

### 2. Editor: el foco de teclado (hallazgo 10)

- **Causa verificada con el editor propio.** La ventana es el primer plano, maximizada, pero **no tiene ventana con foco de Win32** (`GetGUIThreadInfo`: `hwndActive` es el editor, `hwndFocus` nulo). UI Automation informa entonces al escritorio (`#32769`, `Pane`) como dueño del foco, y el motor, que atestigua la aplicación por ese elemento (`uia.rs`, `foreground_process`), no lee evidencia de accesibilidad. No es de tiempo (igual de 1 a 9 s) y pedir foco egui al campo no lo cambia. La orden `Focus` no hace nada si la ventana ya es el primer plano.
- **Cambio en el editor.** `FocusSeeker`: si a los 600 ms del inicio la ventana no informa foco, se oculta y se vuelve a mostrar (`Visible(false)` y 150 ms después `Visible(true)`), como máximo 3 veces, con 1,5 s entre ellas y dentro de 8 s; una ventana oculta siempre se vuelve a mostrar. Código seguro, sin entrada. No da el foco al campo: la confirmación de la ADR-0018 (foco en el elemento tras el clic del motor) conserva su sentido.
- **Verificado en vivo, sólo lectura** (esa compilación aún llevaba la orden de foco del hallazgo 14; las finales están en ese apartado). Tres lanzamientos del editor nuevo: el elemento con foco de UI Automation es la ventana del propio editor (`Window`, mismo proceso) desde la segunda muestra (entre 1,4 y 2,8 s tras el lanzamiento); con el editor anterior, el escritorio en todas las muestras. **No verificado:** que el motor lea evidencia con ese foco (no se ejecutó el motor).
- **Alternativa, no aplicada: decisión del responsable.** Atestiguar la aplicación por el proceso de la ventana en primer plano cuando el elemento con foco es el escritorio permitiría leer evidencia aunque ningún elemento de la aplicación tenga el teclado; debilitaría la ADR-0018 (la escritura en dos fases espera el foco en el elemento objetivo). Cambiaría el motor, con ADR.

### 3. Explorador: el flujo (hallazgo 11)

- **Causa.** Los dos objetivos con zona y rol `unknown` eran el título de la pestaña y el elemento de la lista de archivos (`TabItem` y `ListItem` se asignan al rol `unknown`), no el elemento de la ruta de navegación (un `Button` o `SplitButton` se asigna a `button`). El flujo pedía un `TreeItem` que la ventana no muestra.
- **Flujo nuevo.** La ventana se abre directamente en `carpeta-b` (`explorer.exe /e,<carpeta-b>`; `--explorer-launch e` es ahora la opción por defecto, porque la barra de direcciones muestra la ruta completa; con `/root,` no se verificó que muestre las carpetas superiores). Tres clics simples dentro de la ventana `CabinetWClass`, sin ventanas emergentes, teclas ni doble clic: la miga de pan `carpeta-a` (subir), la miga de pan de la carpeta raíz (subir) y el botón «Atrás» (que vuelve a `carpeta-a`). Tras cada clic se comprueba la carpeta actual de la ventana (`Shell.Application`, sólo lectura). Los objetivos se distinguen por tipo de control de UI Automation (`Button` o `SplitButton`; «Atrás», `Button`) y por el rol del motor (`button`), se identifican por `RuntimeId` y se ancla el clic como siempre; **la zona se registra** (`targets[].zones`) **y no es condición**. Se mantienen las comprobaciones de la ADR-0020 (primer plano, ventana bajo el punto, anclaje) y toda la limpieza. `--explorer-back-name` (por defecto «Atrás») y `--explorer-back-id` (opcional, el `AutomationId` que registre el sondeo) fijan el botón.
- **Nota sobre E2E-03.** La definición del dossier («Navegar carpetas de prueba sin destrucción») no cambia. Esta ronda la cumple con «subir, subir y volver atrás» sobre carpetas de prueba, sin ninguna acción destructiva; el árbol temporal se compara al final y se borra sólo lo creado por la corrida.
- **No verificado en vivo:** que `/e,` muestre la miga de pan de `carpeta-a` y de la raíz en la ventana maximizada, que el motor entregue un elemento `button` único para cada una, y el `AutomationId` y el nombre exacto del botón «Atrás». El sondeo los registra (`explorer_probe`: tipos, roles, zonas e identificadores, nunca nombres).

### 4. Hallazgo 14 (mayor, seguridad): las aplicaciones de prueba enviaban una tecla Alt sintética

- **Hallazgo.** `egui::ViewportCommand::Focus` se implementa en winit 0.30.13 como `focus_window`, que, con la ventana visible, no minimizada y que no es la de primer plano, llama a `force_window_active` (`src/platform_impl/windows/window.rs:1537`): un `SendInput` de la tecla Alt izquierda, pulsada y soltada, para obtener el permiso de primer plano. `tools/s4/testapp/src/main.rs` (desde la PR #12) y `tools/s4/testeditor/src/main.rs` (desde la PR #19) enviaban la orden en cada fotograma durante los 2 primeros segundos. Cada lanzamiento de la aplicación de prueba (rondas posteriores a la PR #12) o del editor (posteriores a la PR #19) con la ventana fuera de primer plano envió al menos un toque de Alt fuera de la compuerta del motor, mientras el responsable podía usar otra aplicación. Rompe la regla 6 de AGENTS.md (entrada sólo por la compuerta) y la regla «sin trucos de Alt» de la octava ronda. **No se encontró evidencia de que la tecla llegara a otra aplicación**, pero no se puede descartar: el cliente no ve teclas sintéticas y la tecla va a la ventana que esté delante en ese momento.
- **Causa.** La orden se eligió para «pedir el foco una vez, pronto»; su efecto de entrada no estaba en el código de las aplicaciones. Se advirtió al verificar el hallazgo 10 y se confirmó en el código de winit.
- **Corrección.** La orden se elimina de las dos aplicaciones. Nada más en ellas llega a `force_window_active` (su otro llamador es crear una ventana a pantalla completa, que ninguna usa). Sólo la escalera de activación del cliente (gestión de ventanas por identificador, sin entrada) lleva la ventana al frente. El editor conserva `FocusSeeker` (ocultar y mostrar la ventana, sin entrada). Prueba en cada aplicación, `the_source_never_asks_the_toolkit_for_the_foreground`: falla si el código contiene la orden de foco, `focus_window` o una petición de pantalla completa (comprobado añadiendo la orden a una copia de cada código: las dos fallan).
- **Consecuencia para el hallazgo 10.** Con el Alt, el editor era el primer plano sin ventana con foco de Win32 en todas las muestras de cuatro compilaciones experimentales. Sin la orden, las compilaciones finales tuvieron el foco de UI Automation en el editor en todas las muestras de tres lanzamientos (dos sin petición alguna y uno con la escalera del cliente, que lo encontró `already_foreground`) y en la aplicación de prueba en dos (uno sin petición y otro con la escalera); una compilación anterior del mismo código, en cuatro de cuatro lanzamientos del editor. La activación con Alt es, por tanto, la causa probable del hallazgo 10; se muestra comparando compilaciones, no aislando la tecla. No se demostró que `FocusSeeker` actúe en la compilación nueva; queda como respaldo acotado y sin entrada.
- **Rondas afectadas.** Toda ronda en vivo anterior que lanzó la aplicación de prueba (E2E-01 y pilotos posteriores a la PR #12) o el editor (rondas con E2E-02 posteriores a la PR #19: octava, novena y su continuación). El toque de Alt lo enviaba la aplicación al arrancar, antes de cualquier solicitud del cliente; no cuenta en el cuaderno ni en las métricas del motor.

### Pruebas

`python -m unittest discover -s tools/s4 -p "test_*.py"`: **258 pruebas correctas** (230 antes). Con los clientes anteriores, 72 de las nuevas o modificadas fallan (50 fallos y 22 errores; varias por el doble nuevo, el resto por las reglas nuevas); la lista está en el informe de la ejecución. Cubren: contiene «estándar»/«standard» (NFD, mayúsculas, inglés), desacuerdo de señales, lectura estricta, nombre sólo si cumple la regla, resumen con contadores, espera acotada de `mode_after` también al detenerse, `--check-mode` (nunca `spend_live`, motor, observación ni `stop.flag`; ni siquiera con una Calculadora ajena), rechazo de `--calc-standard --max-live 0`, y el flujo del Explorador (arranque en `carpeta-b`, subir/subir/atrás con la carpeta comprobada tras cada clic, señuelos de otro tipo o rol nunca pulsados, dos botones → ninguna entrada, «Atrás» por nombre y `AutomationId`, la zona sólo registrada). Editor: 21 pruebas (16 antes) y aplicación de prueba: 11, cada una con la prueba que impide pedir el primer plano al toolkit. Rust: sin cambio de código.

### Qué debe mostrar la próxima ronda en vivo

- **Antes:** SHA-256 de arriba (el del editor, recompilado), `3a451211…cbb1a` del motor, sin `stop.flag` ni `window.pin`, rutas absolutas, `--round-start` propio (cuaderno 429 o 427, decisión del responsable).
- **S4R-02:** `--check-mode` con la Calculadora en Estándar (cambiada a mano) debe dar exit 0 y `mode_signals.header_matched` `true`; si da 1, el registro muestra el tipo, la cuenta y la longitud del encabezado. Sólo con Estándar, `--calc-standard` (que ya no crea `stop.flag` con cupo 0) y el piloto.
- **E2E-02:** sondeo con `accessibility_elements` mayor que cero y `uia_focus` (si hiciera falta) con `is_app` `true`; «Editor de texto» como candidato `input` confirmado. Un piloto que no pase bloquea su serie.
- **E2E-03:** sondeo con la miga de pan `carpeta-a` y la de la raíz como `Button`/`SplitButton` únicos, roles `button` utilizables y el botón «Atrás» con su `AutomationId`; después piloto y serie.
- **Nada de esto está confirmado en vivo.**

### Estado

Hallazgos 2, 3, 9 y 11: corregidos en los clientes y probados con dobles. Hallazgo 10: causa verificada y corrección verificada en lectura (foco de UI Automation en el editor), no con el motor. Hallazgo 8: señal reescrita y probada, **causa real sin verificar** (se desconoce el nombre del encabezado). El hallazgo 1 sigue abierto. Cuaderno: 429, sin cambio. Las matrices no cambian.

## Décima ronda: S4R-02 pasa su piloto y sus diez corridas puntuadas; el piloto de E2E-02 no pasa y E2E-03 no cumple la condición del flujo

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-tenth-round`, desde `main` en `193e7ce` (fusión de #25). **No cambia ningún código**: ni los dos clientes, ni el editor, ni el motor, ni las crates de Rust. Autorización del responsable (CHANGELOG 1.0.37): S4R-02 con la Calculadora en modo Estándar puesto a mano (no se hizo clic en ningún control de modo ni se usó `--calc-standard`), E2E-02 y E2E-03 con los clientes de 1.0.36, cupo de 200 solicitudes desde `--round-start 429`, `--operator-activation` apagado y el responsable sin usar el equipo. Regla de seguridad: un clic anclado en un control incorrecto, un falso éxito del motor o una entrada fuera del flujo o de la ventana permitida detienen todo el trabajo en vivo. **No se activó** esa parada.

Hashes (SHA-256), comprobados antes de cada invocación en vivo:

- Clientes: `tools/s4/e2e_driver.py` `143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`; `tools/s4/e2e_dossier.py` `85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`.
- Motor, de `main` con `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`: `target/release/ommatix.exe` `38080631602ab2a0a1e94baf8432492d95fc6ec0db0cd6eee92232b956d0e5d3` (antes `3a451211…cbb1a`; cambió con la PR #25).
- Editor, de `main` con `cargo build --release` en `tools/s4/testeditor`: `ommatix-testeditor.exe` `32b8fadd2e97b73462043a53205247fbe4384f818119d769b829918cc2da88f8` (antes `541fc0a8…2793`).
- Configuraciones, copias sin cambio de `s4r9-live.toml` (`0428daf8…5ec1e2`) y `s4r9-explorer.toml` (`ab6c33b5…233d7`): `s4r10-live.toml` y `s4r10-explorer.toml`, con esos mismos hashes.
- Artefactos fijados por la configuración: manifiesto float16 `f8b04be9…fe47`, ONNX Runtime `e7eedec6…7bff40` y DirectML `9c9e6d82…e92da1`.

**Resultado:**

- **S4R-02 (Calculadora):** piloto aprobado y **10 de 10 corridas puntuadas** con éxito verificado por el estado del sistema; 0 falsos éxitos; 0 clics anclados en un control incorrecto.
- **E2E-02 (editor):** el sondeo ve 6 elementos de accesibilidad, pero **el piloto no pasó**: el clic de «Guardar» fue rechazado cuatro veces por el motor con `STALE_SNAPSHOT`. Por la regla del responsable, la serie queda bloqueada: 0 corridas puntuadas.
- **E2E-03 (Explorador):** **el sondeo no cumple la condición** (ninguna miga de pan es un candidato `button` del motor). No hubo piloto ni serie, y no se envió ninguna entrada.
- **Cuaderno:** 429 → 490, es decir, 61 solicitudes reales de 200 (139 disponibles).

### 1. Verificación previa a la entrada en vivo

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: sin avisos.
- `cargo test --workspace`: 442 pasadas, 0 fallidas y 1 ignorada. Editor: `cargo clippy -- -D warnings` sin avisos y `cargo test`: 21 pasadas.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 258 correctas.
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48; `validar_paquete.py`, 449 comprobaciones y 0 fallidas. Cuatro archivos de `referencia_ejecutable/resultados/` cambiaron sólo en la marca de tiempo, las cadenas de Python y de plataforma, una duración y el fin de línea; se restauraron con `git restore`.
- Antes de cada invocación en vivo, un comprobador de sólo lectura, fuera del repositorio, verificó los hashes anteriores, la ausencia de `stop.flag` y de `window.pin`, el cuaderno, la ausencia de procesos de prueba y de ventanas `CabinetWClass` visibles, la carpeta temporal del cliente y las ventanas del Bloc de notas del responsable. Su base al empezar la ronda: el proceso 23164 con dos ventanas minimizadas y el 8960 sin ventana (véase el hallazgo 3).

### 2. Invocaciones en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| S4R-02, comprobación de modo (sólo lectura) | `e2e_driver.py --app calculator --check-mode` con `s4r10-live.toml` | 0 | Salida 0: modo `standard`; encabezado «Modo de calculadora Estándar», que cumple la regla; 0 controles sólo de Científica; `already_foreground`; la Calculadora la cerró el cliente. |
| S4R-02, piloto | `--app calculator --mode run --runs 1 --max-live 10 --round-start 429` | 5 | **Aprobado:** 5 de 5 pasos entregados y observados; comprobación del sistema 5 de 5. |
| S4R-02, serie | `--runs 10 --first-run 2 --max-live 75` | 51 | **10 de 10 corridas** con éxito. Un `STALE_SNAPSHOT` en la corrida 3: el motor no entregó nada, el cliente reintentó y la entrega fue `effect_observed`. |
| E2E-02, sondeo (sin entrada) | `e2e_dossier.py --app editor --mode probe --runs 1` con `s4r10-live.toml` | 0 | 6 elementos con origen de accesibilidad; «Editor de texto», `input` confirmado; «Guardar», `button` confirmado; `already_foreground`. |
| E2E-02, piloto | `--app editor --mode run --runs 1 --max-live 8` | 5 | **No pasó:** la escritura de «prueba 123» se entregó y se verificó; «Guardar» fue rechazado cuatro veces con `STALE_SNAPSHOT` (la solicitud y tres reintentos). La corrida terminó sin guardar. Serie bloqueada. |
| E2E-03, sondeo (sin entrada) | `e2e_dossier.py --app explorer --mode probe --runs 1` con `s4r10-explorer.toml` | 0 | **No cumple la condición** (véase 3.3). No hubo piloto ni serie. |

Todas las invocaciones usaron rutas absolutas, y las corridas en vivo usaron `--round-start 429`. No se usó `--calc-standard`, ni `--operator-activation`, ni ningún envío de entrada fuera de los flujos.

### 3. Resultados

#### 3.1 S4R-02, Calculadora

- **Señal de modo, en vivo por primera vez.** El encabezado es «Modo de calculadora Estándar»: la regla (contiene «estándar» y no hay controles sólo de Científica) lo acepta y `--check-mode` devuelve `standard`. Cierra la pregunta del hallazgo 8 de la continuación de la novena ronda para esta compilación del sistema. La Calculadora no se cambió en esta ronda.
- **Piloto:** 5 de 5 pasos entregados y `effect_observed`; comprobación del sistema 5 de 5; activación `already_foreground`, con 0,84 s de espera hasta el primer plano.
- **Serie:** 10 corridas, 50 pasos entregados (todos `effect_observed`), 50 de 50 comprobaciones del sistema con éxito después de cada clic; 0 falsos éxitos; `grounding_wrong` 0; `indeterminate` 0; `POLICY_DENIED` 0. Una solicitud rechazada en la corrida 3 (`STALE_SNAPSHOT`), reintentada y entregada.
- **Latencias (`_meta`, sólo acciones entregadas):** serie (n = 50): `action_total` mediana 73,8 ms y p95 83,3 ms; `rtt` mediana 74,1 ms y p95 83,6 ms. Piloto (n = 5): `action_total` mediana 74,6 ms y p95 84,0 ms. El percentil es de rango más próximo. Son una sola sesión de 11 corridas: no son mediciones de RNF-05 (S0 §10).

#### 3.2 E2E-02, editor propio

- **Sondeo:** 6 elementos con origen de accesibilidad, de 63 del motor. «Editor de texto» es un `input` confirmado con origen de accesibilidad, y «Guardar» un `button` confirmado. El cliente registra `uia_focus` sólo cuando no hay evidencia de accesibilidad, así que aquí no aparece. Con esta lectura, el hallazgo 10 de la continuación de la novena ronda queda resuelto para el sondeo: el motor lee la evidencia del editor, lo que no ocurría entonces.
- **Piloto:** la escritura de «prueba 123» (`type`, en dos fases, ADR-0018) se entregó con `effect_observed`, y la comprobación del sistema después de escribir pasó (la ventana del editor muestra diez caracteres sin guardar). El paso 2, el clic de «Guardar», fue rechazado cuatro veces por `STALE_SNAPSHOT` en el motor, sin entrada en ninguno de los cuatro intentos. La corrida terminó con `step 2: rejected (STALE_SNAPSHOT)`, sin archivo guardado. El árbol temporal se borró (1 directorio) y el editor lo cerró el cliente.
- **Causa:** no verificada. Candidato, sin comprobar: la pantalla cambió entre la observación y el clic (por ejemplo, el cursor del campo), y el motor refusó el clic porque ya no coincidía con el snapshot.
- **Decisión:** un piloto que no pasa, por cualquier causa, bloquea su serie. La serie de E2E-02 no se ejecutó, y el piloto no se repitió.

#### 3.3 E2E-03, Explorador (sólo sondeo)

- **Ventana:** la abrió el cliente (`explorer.exe /e,<carpeta-b>`), la registró por su identificador y la cerró por su identificador. El árbol temporal (2 archivos y 3 carpetas) se borró; el archivo de ventana se escribió y se borró; `window.pin` no existe al terminar.
- **Objetivos:** «carpeta-a» y la carpeta raíz no tienen ningún elemento del motor con esa etiqueta y rol `button`; en UI Automation, cada una es un `SplitButton`. «Atrás» tiene un elemento del motor con rol `button`, pero con `actionability` `unknown`; el cliente sólo elige un objetivo confirmado. En UI Automation es un `Button` con `AutomationId` `backButton`.
- **Condición:** la ronda sólo pasa al piloto si los tres objetivos son candidatos `button` utilizables. No se cumplió. No hubo piloto ni serie, y no se envió ninguna entrada.
- **Diferencia con la novena ronda:** entonces el motor sí veía «carpeta-a» como elemento confirmado con zona `unknown`. **No verificado:** la causa del cambio (el estado de la ventana o el momento de la observación).

#### 3.4 Totales y seguridad

- **Solicitudes reales:** 61 (cuaderno 429 → 490). Entregadas: 56, todas `effect_observed`: 55 clics de la Calculadora y 1 escritura en el editor. Rechazadas: 5, todas `STALE_SNAPSHOT` (1 en la serie de S4R-02 y 4 en el piloto de E2E-02). `indeterminate`: 0. `POLICY_DENIED`: 0.
- **Verificación por el estado del sistema:** 56 de 56 entregadas. **Falsos éxitos:** 0. **Clics anclados en un control incorrecto:** 0. **Parada de seguridad:** no se activó.
- **Entradas:** en cada corrida de S4R-02, los clics «Siete», «Más», «Ocho», «Es igual a» y «Borrar»; y la escritura de «prueba 123» en el campo del editor propio. Ninguna otra aplicación ni ventana del responsable recibió entrada.
- **Capturas y texto de pantalla:** ninguno guardado. Los registros contienen estados, códigos, contadores, tiempos, tipos de control, `AutomationId` y las etiquetas fijas del protocolo. El nombre del encabezado de la Calculadora aparece sólo en el registro de `--check-mode`, porque cumplió la regla (el cliente lo guarda sólo entonces).

### 4. Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 1 | Mayor (E2E-02) | El piloto no pasó: «Guardar» fue rechazado cuatro veces con `STALE_SNAPSHOT`; la escritura sí se entregó y se verificó. La serie queda bloqueada. | Abierto. Causa no verificada. Decide el responsable (§7). |
| 2 | Mayor (E2E-03) | El sondeo no halla «carpeta-a» ni la raíz como candidatos `button` del motor, y «Atrás» no está confirmado. En la novena ronda el motor sí veía «carpeta-a». | Abierto. Causa no verificada. Decide el responsable (§7). |
| 3 | Observación (Bloc de notas del responsable) | Al empezar la ronda, el proceso 8960 no tenía ventana de nivel superior, y el 23164 tenía dos ventanas minimizadas; la novena ronda registró una ventana minimizada por proceso. Ningún cliente de esta ronda había tocado el Bloc de notas antes de esa observación, y no se actuó sobre él. | Abierto para el responsable. Causa no verificada. |
| 4 | Observación (motor) | Un `STALE_SNAPSHOT` en un paso de la serie de S4R-02: el motor rechazó antes de entregar y el cliente reintentó con éxito. Es el comportamiento de diseño. | Sin cambio. |
| 5 | Observación (editor) | El motor lee evidencia de accesibilidad del editor en el sondeo (6 elementos). El hallazgo 10 de la continuación de la novena ronda queda resuelto para el sondeo. | Sin cambio. |
| 6 | Observación (Calculadora) | El nombre del encabezado en Estándar es «Modo de calculadora Estándar». Cierra la pregunta del hallazgo 8 de la continuación de la novena ronda. | Sin cambio. |

### 5. Estado en la matriz

`matrices/plan_pruebas.csv`: **S4R-02** pasa de `PENDIENTE_PRODUCTO` a `EJECUTADO_SUPERVISADO_PARCIAL`, con la misma lectura que S4R-03: un piloto aprobado y diez de diez corridas puntuadas sin falsos éxitos, sólo clics, una sola sesión supervisada y la Calculadora en modo Estándar puesto a mano. Queda parcial por esas mismas razones. **E2E-02** y **E2E-03** siguen `NO_EJECUTADO` y `PENDIENTE_PRODUCTO`: E2E-02 no tiene corridas puntuadas y E2E-03 no se ejecutó en vivo. `requisitos_trazabilidad.csv`, RF-18, RNF-03, RNF-05 y las casillas de la Definition of Done no cambian.

### 6. Estado del escritorio, del cuaderno y de `stop.flag`

- **Procesos de prueba:** ninguno al terminar. La Calculadora y el editor los cerró el cliente en cada invocación.
- **Explorador:** una ventana, abierta y cerrada por el cliente en el sondeo. No queda ninguna ventana `CabinetWClass` visible; la ventana oculta que `explorer.exe` ya tenía no cambió.
- **Bloc de notas del responsable:** el proceso 23164 con sus dos ventanas minimizadas, sin cambio durante la ronda; el 8960 sin ventana (hallazgo 3).
- **Shell (`explorer.exe`):** PID 36160, sin cambio.
- **`window.pin`:** no existe. **`stop.flag`:** ausente, y nunca se creó.
- **Cuaderno:** 490 (61 de 200 solicitudes de la ronda; 139 disponibles).

### 7. Qué decide el responsable

1. **E2E-02:** cómo investigar el `STALE_SNAPSHOT` del clic «Guardar» (un diagnóstico de sólo lectura con el responsable presente, o un cambio del cliente o del editor con una prueba que falle sin él y un hash nuevo, en una ronda aparte), y si el piloto se repite después.
2. **E2E-03:** si acepta otra identificación de las migas de pan (cambio del cliente con prueba y hash), si cambia el contrato de la navegación (dossier, con entrada en el CHANGELOG), o si pide investigar por qué el motor ya no ve «carpeta-a».
3. **S4R-02:** si acepta `EJECUTADO_SUPERVISADO_PARCIAL` en la matriz, que esta rama propone.
4. **Cupo:** quedan 139 solicitudes con `--round-start 429`; o una ronda nueva puede empezar en 490.

### 8. Ejecución

Las invocaciones en vivo fueron, en este orden: la comprobación de modo de la Calculadora; el piloto y la serie de S4R-02; el sondeo del editor, el piloto de E2E-02; y el sondeo de E2E-03 con `s4r10-explorer.toml`. Cada una fue precedida por el comprobador de sólo lectura. Los registros y los resultados JSONL están en la carpeta de ejecuciones, fuera del repositorio; ninguno contiene capturas ni texto de pantalla.

## Preparación de la undécima ronda: el editor deja de parpadear y el recorrido de accesibilidad lee primero la ventana en primer plano y por niveles

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-round10-blockers`, sobre la rama de la décima ronda (`docs/s4-tenth-round`, `1258573`), que a su vez está sobre `main` en `193e7ce`. Cambian el editor (`tools/s4/testeditor`) y el adaptador de accesibilidad (`ommatix-adapter-accessibility`, y con él el motor). **No cambian** los dos clientes (`tools/s4/e2e_driver.py`, SHA-256 `143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`; `tools/s4/e2e_dossier.py`, SHA-256 `85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`), la compuerta de frescura del motor, la regla de confirmación ni la ADR-0017. **No se envió entrada ni ninguna solicitud real** (cuaderno 490, sin cambio). Las únicas comprobaciones sobre el sistema fueron de sólo lectura: una ventana del Explorador abierta por este trabajo sobre una carpeta temporal que creó (leída con UI Automation, cerrada por su identificador con `WM_CLOSE`, carpeta borrada) y las pruebas del editor, que ejecutan el toolkit sin ventana. Registro por hallazgo en la [revisión](../reviews/2026-10-14-s4-round10-blockers.md).

Hashes (SHA-256) de lo que cambia, compilado en el árbol de trabajo de esta rama (quien ejecute la ronda recompila y registra los suyos; la información de depuración lleva rutas):

- Motor: `target/release/ommatix.exe` (`cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`) **`6e8f6fdde0027eaf9ca16946cfc31685677cb5d7129a06b1c9e324a705b4bb52`**, antes `38080631…e5d3`.
- Editor: `ommatix-testeditor.exe` (`cargo build --release` en `tools/s4/testeditor`) **`868cadb7195e7aadf428f9ece056d638ce0f7e20ebb2eabf21706fd3f5debb9d`**, antes `32b8fadd…8f8`.

### 1. E2E-02: el cursor de texto del editor parpadeaba (hallazgo 1 de la décima ronda)

- **Causa (verificada en el código y en el toolkit; no en una ventana en vivo).** El motor sólo entrega si un fotograma capturado justo antes de entregar no tiene ninguna loseta sucia respecto a la base confirmada (`act.rs`, `fresh_frame`; sin tolerancia). El toolkit del editor pinta por defecto un cursor de texto que parpadea (`text_cursor.blink = true`, 0,5 s visible y 0,5 s oculto desde la última edición), y tras escribir la ventana está al frente con el campo enfocado: la pantalla cambia dos veces por segundo. Los cuatro reintentos del piloto estuvieron separados entre 0,89 y 0,91 s, casi un periodo de parpadeo, lo que es coherente con que la misma fase se repitiera (no es una prueba: las fases en vivo no se registraron). **No verificado en vivo:** no se pudo enviar entrada para encender el cursor en una ventana real.
- **Corrección, en el editor (nuestra aplicación de prueba) y no en el motor.** `configure` fija el cursor sin parpadeo en los dos temas. La compuerta de frescura del motor no se debilita.
- **Pruebas** (editor, 24; 21 antes): `a_focused_field_paints_the_same_frame_at_any_time` (con el campo enfocado y la ventana al frente, seis instantes en dos periodos pintan el mismo fotograma; falla con `blink = true`, comprobado), `the_toolkits_default_caret_blinks` (la premisa: el valor por defecto sí cambia el fotograma) y `the_caret_stays_on_in_both_themes` (falla con `blink = true`, comprobado). El foco se da dentro del contexto del toolkit, sin entrada del sistema.
- **Hallazgo para el responsable y la tesis (no implementado).** Un editor real con cursor parpadeante rechazaría con `STALE_SNAPSHOT` toda acción posterior a escribir: la comprobación de frescura compara fotogramas completos y un cursor es un cambio de pocos píxeles dos veces por segundo, justo donde estará la siguiente acción. La corrección del editor propio no arregla el mundo. Opciones, todas con cambio del dossier y ADR, y todas debilitan o rodean una propiedad de seguridad (SDD §2.9), así que ninguna se toma aquí: (a) una tolerancia para losetas sucias confinadas al área del cursor del campo enfocado, que la evidencia de accesibilidad puede acotar (`keyboard_focus` da el rectángulo del elemento); (b) comparar tras enmascarar el rectángulo del elemento enfocado sólo en las acciones que siguen a un `type`; (c) una regla operativa (la velocidad de parpadeo del cursor del sistema, que algunos toolkits respetan y otros, como el de este editor, no), sin cambio del motor; (d) que el cliente reintente hasta que un fotograma coincida, lo que sólo funciona por azar con un periodo de 1 s.

### 2. E2E-03: el recorrido agotaba el presupuesto antes de llegar a las migas de pan (hallazgo 2 de la décima ronda)

- **No es una regla.** Leído hasta el final, el adaptador devuelve las dos migas de pan como `button` confirmados (habilitadas, con `Invoke` y `ExpandCollapse`): ni el mapa de roles, ni la prueba del patrón de acción, ni la normalización del nombre, ni la regla de confirmación son la causa.
- **Causa (verificada por reproducción, no en la corrida en vivo).** Un solo presupuesto de 250 ms cubría toda la lectura; unos 50 ms se iban en listar las 13 ventanas de nivel superior de la sesión antes de leer nada (UI Automation cuesta unos 4 ms por ventana), y el recorrido era en profundidad, así que llegaba a las migas de pan en el elemento 83 de 118 visitados, después del panel de navegación del Explorador. Con el escritorio en reposo hacía falta un presupuesto de 200 ms para alcanzarlas (0 de 6 con 180 ms). La etapa de accesibilidad del motor duró 267 ms y 271 ms en los dos sondeos del Explorador, la firma de un recorrido que terminó por presupuesto; los registros del sondeo no dicen si la lectura fue truncada. **No verificado en vivo.**
- **La novena ronda tampoco las veía** (hallazgo 4 de la revisión). «carpeta-a» era el elemento del panel de navegación (`ListItem`, rol `unknown`) y el nombre de la raíz, el título de la pestaña (`TabItem`, rol `unknown`). La décima ronda leyó mal esa diferencia.
- **«Atrás» `unknown` es esperable** (hallazgo 3). En una ventana que no ha navegado, UI Automation informa el botón como deshabilitado, y un elemento deshabilitado nunca se confirma. El flujo lo pulsa en su tercer paso, tras dos navegaciones, cuando está habilitado; el sondeo, que no envía entrada, no puede mostrarlo confirmado.
- **Corrección, en el adaptador.** El recorrido lee primero la ventana que contiene el foco de teclado, lista las demás ventanas del proceso sólo si queda presupuesto (saltando la ya leída) y visita cada ventana por niveles (`walk`, en anchura), de modo que un recorrido truncado pierde lo más profundo y no la barra de un hermano posterior. Los límites (2 000 elementos, profundidad 32, 250 ms), la evidencia y la regla de confirmación no cambian; ninguna miga de pan se confirma sin patrón de acción. El peor caso de una lectura es ahora el presupuesto más una lista de ventanas (unos 50 ms aquí) cuando la ventana enfocada gasta todo el presupuesto menos unos milisegundos, en lugar del presupuesto solo.
- **Mediciones** (una ventana del Explorador sobre una carpeta temporal, escritorio en reposo, 16 procesadores lógicos, 13 ventanas de nivel superior, compilación `--release`, seis corridas por celda; cada celda es cuántas de seis devolvieron las dos migas de pan como botones confirmados dentro del presupuesto):

  | Presupuesto | Antes | Después | Antes, 8 hilos ocupados | Después, 8 hilos ocupados |
  |---|---|---|---|---|
  | 80 ms | 0 de 6 | 1 de 6 | 0 de 6 | 0 de 6 |
  | 100 ms | 0 de 6 | 6 de 6 | 0 de 6 | 6 de 6 |
  | 140 ms | 0 de 6 | 6 de 6 | 0 de 6 | 6 de 6 |
  | 180 ms | 0 de 6 | 6 de 6 | 0 de 6 | 6 de 6 |
  | 200 ms | 6 de 6 | 6 de 6 | 5 de 6 | 6 de 6 |
  | 250 ms | 6 de 6 | 6 de 6 | 6 de 6 | 6 de 6 |

  La lectura completa no es más lenta: 63 elementos en 189 a 197 ms después y 193 ms antes. El margen con el presupuesto por defecto pasa de 1,25 veces (200 de 250 ms) a unas 2,5 (100 de 250 ms). Los hilos ocupados aproximan la concurrencia del motor con la captura y los dos modelos; no son la carga del motor. Es una máquina y una sesión: no son mediciones de RNF-05 (S0 §10).
- **Pruebas** (adaptador, 11; 6 antes): cinco de `walk` (`a_walk_reads_the_chrome_before_the_content`, `a_walk_cut_by_elements_loses_the_deepest_nodes_not_a_later_sibling`, `a_walk_cut_by_time_stops_where_the_clock_runs_out`, `a_skipped_subtree_is_not_entered`, `the_depth_bound_is_reported_only_where_something_lies_below_it`); las cinco fallan con un orden en profundidad, comprobado. Manual (ignorada): `reads_the_breadcrumbs_of_a_file_explorer_window`, con `OMMATIX_EXPLORER_TITLE` y `OMMATIX_EXPLORER_CRUMBS`, que exige las migas como botones confirmados en tres lecturas.
- **El presupuesto por defecto (250 ms) no se cambia.** Subirlo es una decisión del responsable (los límites son valores `[P]` de la ADR-0017) y alargaría la observación, de la que la accesibilidad ya es la etapa más larga.

### 3. Verificación

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`: sin avisos.
- `cargo test --workspace`: **447 pasadas** (442 antes), 0 fallidas y 1 ignorada. Editor: `cargo test`, 24 pasadas (21 antes), y `cargo clippy --all-targets -- -D warnings` sin avisos (una resta de `Duration` sin comprobar, ya existente en una prueba, se reescribió para que pase).
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: 258 correctas (sin cambio de los clientes).
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48; `validar_paquete.py`, 449 comprobaciones y 0 fallidas.

### Qué debe mostrar la próxima ronda en vivo

- **Antes:** los SHA-256 de arriba (recompilados por quien ejecute la ronda), `stop.flag` y `window.pin` ausentes, rutas absolutas, `--round-start` propio (cuaderno 490, o 429 con 139 disponibles: decisión del responsable).
- **E2E-02:** el piloto entrega la escritura y entrega el clic de «Guardar» (`effect_observed`), con el archivo guardado y el título en `guardado=si`, sin `STALE_SNAPSHOT`. Si aun así hubiera uno, el siguiente dato que importa es qué cambió en pantalla entre la observación y el clic, no otro reintento.
- **E2E-03:** el sondeo muestra las migas de pan «carpeta-a» y la de la raíz como `button` confirmados y únicos, y «Atrás» como `button` con actionability `unknown` (deshabilitado sin historial: esperable, no un fallo del sondeo). Pasar al piloto sólo si las migas de pan se confirman; «Atrás» se confirma en el paso 3. Cada miga de pan es un `SplitButton` con una flecha: el piloto debe mostrar que un clic en su centro navega.
- **Nada de esto está confirmado en vivo.**

### Estado

Hallazgo 1 de la décima ronda: causa verificada en el toolkit y corregida en el editor, con pruebas; **no confirmada en vivo**. Hallazgo 2: causa verificada por reproducción y corregida en el adaptador, con pruebas y mediciones; **no confirmada en vivo**. Nuevo para el responsable: la tolerancia a cursores parpadeantes en aplicaciones reales (arriba, §1). Cuaderno: 490, sin cambio. Las matrices no cambian.

## Décima ronda (continuación): el piloto de E2E-02 no pasa y E2E-03 se detiene en su primer clic por la regla de anclaje

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-tenth-round-continuation`, desde `main` en `0cb126f` (fusión de #27). **No cambia código.** Ejecuta lo que la décima ronda dejó abierto: E2E-02 (editor propio) y E2E-03 (Explorador, ADR-0020), con la autorización del responsable para la continuación, el cupo de 200 desde `--round-start 429`, `--operator-activation` apagado y el responsable sin usar el equipo. Regla de seguridad: un clic anclado en un control incorrecto, un falso éxito del motor o una entrada fuera del flujo o de la ventana permitida detienen todo el trabajo en vivo. **Esa parada se activó** en el primer clic del piloto de E2E-03 (§3.3).

Resultado:

- **E2E-02 (editor propio):** el sondeo pasó; el piloto **no pasó** su criterio, porque hubo un `STALE_SNAPSHOT` en el primer `type`, antes de cualquier entrada. La serie queda bloqueada: 0 corridas puntuadas.
- **E2E-03 (Explorador):** la señal ACTIVE CONTROL pasó y el sondeo cumplió la compuerta; el primer clic del piloto activó la parada de seguridad. No hubo piloto válido ni serie.
- **Cuaderno:** 490 → 494, es decir, 4 solicitudes reales (65 de 200 de la ronda; 135 disponibles).

### 1. Verificación previa a la entrada en vivo

- Motor y editor recompilados desde `main`. SHA-256:
  - Motor, `target/release/ommatix.exe` (`cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia`): **`a137ccd15cd7b3974d394c5f2f097d0909eb8d02589431e005c857afbae7d548`** (antes `38080631…e5d3`, de `193e7ce`).
  - Editor, `ommatix-testeditor.exe` (`cargo build --release` en `tools/s4/testeditor`): **`f358b3a991754fc95eaabf3c5552318559643f915fbc2ceb9ee040e91f911d20`** (antes `32b8fadd…8f8`).
  - Las compilaciones de la undécima ronda (`6e8f6fdd…`, `868cadb7…`) se hicieron en otro directorio y no son comparables con éstas.
- Clientes sin cambio: `tools/s4/e2e_driver.py` `143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`; `tools/s4/e2e_dossier.py` `85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`.
- Configuraciones sin cambio: `s4r10-live.toml` `0428daf861e1567e1be9eb8588887fd56f370beef863099904cd9bd3ce5ec1e2`; `s4r10-explorer.toml` `ab6c33b5144e6785e4be60aac71a6d54b93c29e8f383932ed60324dbaaa233d7`.
- Artefactos fijados (sin cambio): manifiesto `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`; ONNX Runtime `e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40`; DirectML `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1`.
- Comprobaciones, todas con código de salida 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`; `cargo test --workspace` (447 pasadas, 0 fallidas, 1 ignorada); `python -m unittest discover -s tools/s4 -p "test_*.py"` (258 correctas); editor: `cargo clippy -- -D warnings` y `cargo test` (24 pasadas); desde `docs/thesis`: `referencia_ejecutable/run_tests.py` (48 de 48) y `validar_paquete.py` (449 comprobaciones, 0 fallidas). Los cuatro archivos de `referencia_ejecutable/resultados/` que cambiaron sólo en fecha, plataforma y duración se restauraron con `git restore`.
- Comprobador de sólo lectura, fuera del repositorio, antes de cada invocación: hashes, ausencia de `stop.flag` y de `window.pin`, cuaderno, procesos de prueba, ventanas del Bloc de notas del responsable y carpeta temporal del cliente. **Excepción registrada:** dos ventanas `CabinetWClass` minimizadas y fuera de pantalla (−32000, −32000) del proceso del shell (PID 36160), que no estaban al cierre de la décima ronda. El comprobador las marca como bloqueantes. Se aceptaron para E2E-02 y E2E-03 porque el cliente sólo toma como suya la ventana nueva cuya carpeta es la raíz de la corrida, el motor sólo entrega a la ventana registrada, y ninguna de las dos está en pantalla (hallazgo 9).

### 2. Invocaciones en vivo

| Paso | Invocación (resumen) | Solicitudes | Resultado |
|---|---|---|---|
| E2E-02, sondeo (sin entrada) | `e2e_dossier.py --app editor --mode probe --runs 1` con `s4r10-live.toml` | 0 | 6 elementos de accesibilidad; «Editor de texto» `input` confirmado; «Guardar» `button` confirmado; ventana cerrada; árbol borrado. |
| E2E-02, piloto | `--mode run --runs 1 --max-live 8 --round-start 429` | 3 | **No pasa** el criterio: `STALE_SNAPSHOT` en el primer `type`; reintento entregado y verificado; «Guardar» entregado con `effect_observed`; comprobaciones del sistema 2 de 2. Serie bloqueada; no se repitió. |
| Señal ACTIVE CONTROL (Explorador) | `ommatix.exe serve --source dxgi --output 0 --config s4r10-explorer.toml --input sendinput`, con la entrada en `/dev/null` | 0 | Línea con el alcance por ventana y el archivo de la ventana; sin avisos; salida 0; ningún byte en la salida estándar. |
| E2E-03, sondeo (sin entrada) | `--app explorer --mode probe --runs 1` con `s4r10-explorer.toml` | 0 | Compuerta cumplida: «carpeta-a» y la raíz, `button` confirmados y únicos; «Atrás», `unknown` (esperable). |
| E2E-03, piloto | `--mode run --runs 1 --max-live 8 --round-start 429` | 1 | **Parada de seguridad** en el primer clic: entregado, pero el control bajo el cursor no era el identificado (`grounding_wrong` 1; `engine_false_success` 1). Sin más solicitudes. |

Todas las invocaciones usaron rutas absolutas. No se usó `--calc-standard`, ni `--operator-activation`, ni ninguna entrada fuera de los dos flujos.

### 3. Resultados

#### 3.1 E2E-02, editor propio

- **Sondeo.** 59 elementos del motor, 6 de ellos con origen de accesibilidad. «Editor de texto» es un `input` confirmado por accesibilidad, y «Guardar» un `button` confirmado por accesibilidad. El cliente no registra `uia_focus` cuando hay evidencia de accesibilidad.
- **Piloto.**
  - `type` (paso 1), primer intento: `rejected` con `STALE_SNAPSHOT`, sin entrega; la comprobación de frescura tardó 9 ms.
  - `type`, reintento: `effect_observed`, entregado y verificado; `rtt` 52,0 ms.
  - Clic en «Guardar» (paso 2): `effect_observed`, entregado y verificado; anclaje por identidad a un `Button` confirmado; `rtt` 72,0 ms.
  - Comprobaciones del sistema, 2 de 2: tras escribir, la carpeta estaba vacía y el título decía `caracteres=10 guardado=no`; tras guardar, la carpeta tenía exactamente `nota.txt`, con los bytes del texto escrito, y el título decía `guardado=si`.
  - Del cliente: `success` verdadero, `engine_false_success` 0, `grounding_wrong` 0. Activación: `attach_show_top`, gestión de la ventana sin entrada.
- **Criterio del piloto.** La orden exige: `type` entregado y verificado; «Guardar» entregado con `effect_observed`; `nota.txt` con los bytes escritos; y **ningún `STALE_SNAPSHOT`**. Los tres primeros se cumplen y el cuarto no: el primer `type` fue rechazado. Se aplicó el criterio literal, así que **el piloto no pasa**. Lectura alternativa, que decide el responsable: el rechazo fue anterior a cualquier entrada y el reintento se entregó. El piloto no se repitió: el bloqueo de la serie es la regla del responsable, y repetirlo sería decisión suya.
- **Causa.** No verificada. El rechazo ocurrió antes de que el campo tuviera el foco de teclado, así que el parpadeo del cursor (hallazgo 1 de la décima ronda, ya corregido en el editor) no lo explica. Candidatos sin comprobar: un cambio de pantalla al activar la ventana o en su primera pintura.

#### 3.2 Señal ACTIVE CONTROL del Explorador

Antes de cualquier solicitud, `serve` con la configuración del Explorador imprimió: «ACTIVE CONTROL: live input is enabled for explorer.exe (only windows of class CabinetWClass of explorer.exe) (only the window whose handle … holds); the agent acts only with dry_run: false; to stop, create … or press Ctrl+C». No hubo avisos sobre `explorer.exe` sin ventana registrada. Con esto se cierra el punto 7 de la [revisión de la décima ronda](../reviews/2026-10-14-s4-tenth-round.md). Las otras notas de arranque (`max_snapshot_age_ms` todavía sin aplicar y ausencia de prototipos de icono) son las de cualquier arranque de esta compilación.

#### 3.3 E2E-03, Explorador

- **Ventana.** El cliente abrió una ventana sobre la carpeta de la corrida, la reconoció como la única ventana nueva cuya carpeta es la raíz (la ventana empieza en «carpeta-b»), escribió `window.pin`, la llevó al frente y la maximizó. La cerró el cliente al terminar, y `window.pin` no existe.
- **Sondeo** (sin entrada). 210 elementos del motor; la zona de las migas es `unknown`, que el cliente no exige. «carpeta-a»: `button` confirmado y único (UIA: `SplitButton`). La raíz: `button` confirmado y único (UIA: `SplitButton`). «Atrás»: `button` con `unknown` (UIA: `Button`, `AutomationId` `backButton`), esperable en una ventana sin historial. **Compuerta cumplida.**
- **Piloto, paso 1** (clic en la miga «carpeta-a»). Antes del clic, el anclaje fue el `SplitButton` confirmado con su `RuntimeId`. El motor entregó el clic y respondió `effect_observed` (`rtt` 72,3 ms); la comprobación del sistema (carpeta actual de la ventana) pasó. Después de la entrega, el cliente leyó el control bajo el cursor, tras descender por la isla XAML (profundidad 4, 64 elementos visitados, sin truncar ni ambigüedad): era un `Text`, hijo de la miga, con un `RuntimeId` distinto. Resultado: `grounding_wrong` 1; `engine_false_success` 1 según la definición del cliente; fallo «step 1: the control under the cursor is not the element found before the click».
- **Parada.** Según la regla del responsable, el trabajo en vivo se detuvo. No se enviaron el paso 2 (la raíz), el paso 3 («Atrás»), un piloto repetido ni las corridas puntuadas.
- **Lo que muestra el sistema.** El árbol quedó igual (`tree_unchanged`); el cliente cerró la ventana que abrió; se borraron 2 archivos y 3 carpetas de la corrida; no quedó ningún `window.pin`.
- **Lectura.** La entrega fue en el centro de la miga y la carpeta cambió a «carpeta-a», lo que sugiere que el clic hizo lo previsto. La regla de identidad, sin embargo, compara el `RuntimeId` del control bajo el cursor con el identificado antes y no admite un descendiente. Si el clic fue correcto, es decisión del responsable (hallazgo 8). El cliente no se cambió: está fijado para la ronda.

#### 3.4 Latencias

Acciones entregadas, de `_meta`: piloto de E2E-02, `type` con `rtt` 52,0 ms (`action_total` 51,7 ms) y clic de «Guardar» con `rtt` 72,0 ms (`action_total` 71,6 ms); piloto de E2E-03, clic de la miga con `rtt` 72,3 ms (`action_total` 72,1 ms). Son tres acciones en una sesión: no son mediciones de RNF-05 (S0 §10).

#### 3.5 Totales y seguridad

- **Solicitudes reales:** 4 (cuaderno 490 → 494). **Entregadas:** 3 (2 en E2E-02, 1 en E2E-03). **Rechazadas:** 1 (`STALE_SNAPSHOT`, en E2E-02). **`indeterminate`:** 0. **`POLICY_DENIED`:** 0.
- **Verificación por el estado del sistema:** 3 de 3 entregas pasaron.
- **Falsos éxitos según el cliente:** 1 (E2E-03, paso 1). **Clics anclados en un control incorrecto según el cliente:** 1 (el mismo).
- **Entradas:** en E2E-02, la escritura de «prueba 123» y un clic en «Guardar» en el editor propio; en E2E-03, un clic en la miga «carpeta-a» de la ventana que abrió el cliente. Según los registros del cliente, ninguna otra ventana recibió entrada.
- **Capturas y texto de pantalla:** ninguno guardado.

#### 3.6 Hallazgos

| # | Severidad | Hallazgo | Estado y siguiente paso |
|---|---|---|---|
| 7 | Mayor (E2E-02) | El piloto no cumple «sin `STALE_SNAPSHOT`»: el primer `type` fue rechazado antes de entregar; el reintento y el resto se entregaron y se verificaron. | Abierto. Decide el responsable (§5). Causa no verificada. |
| 8 | Seguridad (E2E-03) | El primer clic se entregó y el cliente lo cuenta como control incorrecto (hijo de texto del `SplitButton` identificado) y como falso éxito del motor: parada de seguridad. | Abierto. Decide el responsable (§5). Sin cambio de código. |
| 9 | Observación (escritorio del shell) | Dos ventanas `CabinetWClass` minimizadas, fuera de pantalla, del proceso del shell (PID 36160), que no estaban al cierre de la décima ronda. No son del cliente y no se tocaron. | Abierto: confirmar de quién son. |
| 10 | Observación (Bloc de notas del responsable) | Sin cambio: 23164 con dos ventanas minimizadas; 8960 sin ventana. | Sin cambio. |
| 11 | Cerrado: ACTIVE CONTROL de la configuración del Explorador | Comprobado sin entrada al arrancar el motor: nombra el alcance y no avisa. Cierra el punto 7 de la décima ronda. | Cerrado. |
| 12 | Documentación | Esta sección de la preparación del dossier (§3, «El flujo») describe tres clics: «carpeta-a» y «carpeta-b» en el panel de navegación, y la raíz en la ruta. El cliente fijado hace clic en las migas «carpeta-a» y raíz, y después en «Atrás», partiendo de «carpeta-b». | Abierto. El cliente es el contrato ejecutado; el texto se corrige en un cambio de documentación aprobado. |

### 4. Estado del escritorio, del cuaderno y de `stop.flag`

- **Procesos de prueba:** ninguno al terminar. El cliente cerró el editor y su ventana del Explorador; la comprobación de la señal terminó con salida 0 al cerrar su entrada.
- **Carpeta temporal del cliente (`%TEMP%\ommatix-e2e`):** vacía.
- **`window.pin`:** no existe. **`stop.flag`:** ausente; ninguna invocación lo creó (el cliente lo crea sólo tras tres fallos seguidos, y hubo uno).
- **Bloc de notas del responsable:** 23164 con sus dos ventanas minimizadas; 8960 sin ventana. Sin cambio.
- **Shell (`explorer.exe`, PID 36160):** una ventana `CabinetWClass` oculta (la base de la décima ronda) y las dos minimizadas del hallazgo 9.
- **Cuaderno:** 494 (65 de 200 solicitudes de la ronda; 135 disponibles).

### 5. Qué decide el responsable

1. **E2E-02:** si el `STALE_SNAPSHOT` previo a cualquier entrada cuenta como piloto no superado (aquí se aplicó el criterio literal) y si el piloto se repite en otra ronda.
2. **E2E-03:** si el anclaje acepta como el mismo control un descendiente del identificado (cambio del cliente con una prueba que falle sin él, un hash nuevo y una ronda aparte); si el flujo cambia de objetivos; o si el clic del primer paso se considera correcto y sólo se cambia la regla para este flujo.
3. **Ventanas del shell:** confirmar de quién son las dos ventanas minimizadas (hallazgo 9).
4. **Cupo:** quedan 135 con `--round-start 429`, o una ronda nueva puede empezar en 494.
5. **Matriz:** esta continuación no cambia `matrices/plan_pruebas.csv`; S4R-02 queda como está en `main`, según la décima ronda.

### 6. Ejecución

Orden: verificación previa; construcción de motor y editor; comprobación de la señal ACTIVE CONTROL (sin solicitudes); sondeo de E2E-02; piloto de E2E-02 (no pasó: serie bloqueada); sondeo de E2E-03 (compuerta cumplida); piloto de E2E-03 (parada de seguridad). Cada invocación precedida del comprobador. Los registros y los JSONL están en la carpeta de ejecuciones, fuera del repositorio; contienen estados, códigos, contadores, tiempos, tipos de control, identificadores de automatización y las etiquetas fijas del protocolo, pero ninguna captura ni texto de pantalla.

### Estado

Hallazgos 7 y 8: abiertos, decisión del responsable; ninguno está confirmado por una corrida completa. Hallazgo 11: cerrado. Cuaderno: 494. Las matrices no cambian, y ninguna casilla de la Definition of Done cambia.

## Décima ronda (serie): diez corridas puntuadas de E2E-02 sin falsos éxitos; E2E-03 no se ejecutó

**Fecha:** 14 de octubre de 2026. Rama `docs/s4-tenth-round-series`, desde `main` en `3f5fa0f` (fusión de #28). **No cambia código.** Ejecuta la serie puntuada de E2E-02 (editor propio), que la continuación dejó pendiente tras el piloto que el responsable dio por superado. **E2E-03 no se ejecutó:** el responsable mantiene su parada hasta que se implemente la nueva regla de anclaje, y no se hizo ninguna invocación del Explorador, ni siquiera un sondeo. Cupo de 200 desde `--round-start 429`, `--operator-activation` apagado y el responsable sin usar el equipo. Regla de seguridad sin cambios: un clic anclado en un control incorrecto, un falso éxito del motor o una entrada fuera del flujo o de la ventana permitida detienen todo el trabajo en vivo. **Ninguna de esas condiciones se dio.**

Resultado:

- **E2E-02 (editor propio): diez de diez corridas puntuadas, cero falsos éxitos.** Corridas 2 a 11. Las veinte acciones entregadas fueron `effect_observed`, y las veinte comprobaciones del sistema pasaron (carpeta y título antes y después de «Guardar», y `nota.txt` con los bytes escritos). Hubo una negativa, `STALE_SNAPSHOT` en el primer `type` de la corrida 2, antes de entregar ninguna tecla; el reintento se entregó y se verificó. Con la regla del responsable (diez de diez y cero falsos éxitos), la matriz pasa E2E-02 a `EJECUTADO_SUPERVISADO_PARCIAL`, el mismo estado que S4R-02 y S4R-03.
- **E2E-03 (Explorador): no ejecutado.** Ninguna invocación, ningún sondeo y ninguna ventana.
- **Cuaderno:** 494 → 515, es decir, veintiuna solicitudes reales (86 de 200 de la ronda; 114 disponibles).

### 1. Verificación previa a la entrada en vivo

- Motor y editor **sin recompilar**. `main` pasó de `0cb126f` a `3f5fa0f` sólo con documentación (seis archivos, `git diff --stat`), así que los binarios de la continuación siguen siendo los de `main`. Comprobados antes de cada invocación:
  - Motor, `target/release/ommatix.exe`: **`a137ccd15cd7b3974d394c5f2f097d0909eb8d02589431e005c857afbae7d548`**.
  - Editor, `tools/s4/testeditor/target/release/ommatix-testeditor.exe`: **`f358b3a991754fc95eaabf3c5552318559643f915fbc2ceb9ee040e91f911d20`**.
- Clientes sin cambio, comprobados antes de cada invocación: `tools/s4/e2e_driver.py` `143ea45464274936e5b5eca06ade8279117d3c33cbc0b13257b74753341aee3d`; `tools/s4/e2e_dossier.py` `85111edbe1b7acc113f71e88d48ba41d6c1fe485b5571f9b0c827d53d7f316df`.
- Configuración sin cambio: `s4r10-live.toml` `0428daf861e1567e1be9eb8588887fd56f370beef863099904cd9bd3ce5ec1e2`. Artefactos fijados sin cambio: manifiesto `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47`; ONNX Runtime `e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40`; DirectML `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1`.
- Comprobaciones de código, todas con código de salida 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia --all-targets -- -D warnings`; `cargo test --workspace` (447 pasadas, 0 fallidas, 1 ignorada); `python -m unittest discover -s tools/s4 -p "test_*.py"` (258 correctas); editor: `cargo clippy -- -D warnings` y `cargo test` (24 pasadas); desde `docs/thesis`: `referencia_ejecutable/run_tests.py` (48 de 48) y `validar_paquete.py` (449 comprobaciones, 0 fallidas). Los cuatro archivos de `referencia_ejecutable/resultados/` que cambiaron sólo en fecha, plataforma, duración o fin de línea se restauraron con `git restore`.
- Comprobador de sólo lectura antes de cada invocación (fuera del repositorio, sin entrada) y compuerta entre invocaciones (`e2e10c_gate.py`, también fuera del repositorio). **Única excepción aceptada:** las dos ventanas `CabinetWClass` minimizadas y fuera de pantalla (−32000, −32000; 160 × 28) del proceso del shell (PID 36160), que ya estaban en la continuación (hallazgo 9). Un listado de sólo lectura, sin títulos, confirmó antes de la serie que siguen así y que la ventana oculta de base también.

### 2. Invocaciones en vivo: diez, una por corrida

Cada corrida tuvo su invocación: `e2e_dossier.py --app editor --mode run --runs 1 --first-run n --max-live M --round-start 429 --session-id s4d-e2e02-scored`, con `s4r10-live.toml`, rutas absolutas, el editor de la continuación (mismo SHA-256) en `--editor-exe` y el cuaderno en `--ledger`. Cada una fue precedida del comprobador y de la compuerta. `--max-live` recibió lo que quedaba del cupo de la serie (40): 40, 37, 35, 33, 31, 29, 27, 25, 23 y 21.

**Por qué una invocación por corrida.** El cliente no detiene el protocolo ante un clic anclado en otro control ni ante un falso éxito: marca esa corrida como fallida y empieza la siguiente; sólo tres corridas fallidas seguidas crean `stop.flag`. Una invocación de diez corridas habría seguido enviando entradas tras un evento de seguridad, y una invocación en curso no puede detenerse sin matarla. Con una invocación por corrida, la compuerta aplica la regla del responsable antes de cada corrida (hallazgo 13 de la [revisión](../reviews/2026-10-14-s4-tenth-round.md)). Ninguna corrida falló, así que la compuerta nunca pausó la serie.

### 3. Resultados

#### 3.1 Corridas puntuadas (2 a 11)

| Corrida | Éxito | Entregadas | `type` | Efecto observado | Rechazadas (`STALE_SNAPSHOT`) | Comprobaciones del sistema | Falsos éxitos | Anclaje incorrecto | `nota.txt` | Cuaderno | Activación |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 2 | sí | 2 | 1 | 2 | 1 (1) | 2 de 2 | 0 | 0 | ok | 494 → 497 | `attach_show_top` |
| 3 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 497 → 499 | `already_foreground` |
| 4 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 499 → 501 | `already_foreground` |
| 5 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 501 → 503 | sin petición |
| 6 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 503 → 505 | `already_foreground` |
| 7 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 505 → 507 | `already_foreground` |
| 8 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 507 → 509 | `already_foreground` |
| 9 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 509 → 511 | sin petición |
| 10 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 511 → 513 | `already_foreground` |
| 11 | sí | 2 | 1 | 2 | 0 | 2 de 2 | 0 | 0 | ok | 513 → 515 | `already_foreground` |

#### 3.2 Totales y seguridad

- **Corridas:** 10; exitosas según el cliente: 10.
- **Acciones:** 21 solicitadas, 20 entregadas (10 `type` y 10 «Guardar»), 20 `effect_observed`, 0 `indeterminate`, 1 rechazada (`STALE_SNAPSHOT`, corrida 2, antes de cualquier entrada; `rtt` 7 ms), 1 reintento.
- **Comprobaciones del sistema:** 20 de 20, dos por corrida.
- **Falsos éxitos según el cliente:** 0. **Clics anclados en un control incorrecto:** 0. Identidad `ok` en todas las acciones: «Editor de texto» como `Edit` (`input`) y «Guardar» como `Button`, ambos con un único candidato. Descenso por isla XAML: no hizo falta.
- **`POLICY_DENIED`:** 0. **Abortos:** 0. **Paradas del cliente:** 0. `stop.flag` nunca se creó.
- **Entradas:** en cada corrida, la escritura de «prueba 123» en el campo del editor propio y un clic en «Guardar». Los registros no muestran ninguna acción sobre otra ventana.
- **Capturas y texto de pantalla:** ninguno guardado. La limpieza de cada corrida borró un archivo y un directorio, y no dejó nada.

#### 3.3 Latencias (`_meta`, acciones entregadas, una sesión de diez corridas)

- Todas las entregadas (n = 20): `action_total` mediana 71,1 ms, p95 112,7 ms; `rtt` mediana 71,3 ms, p95 113,0 ms.
- `type` (n = 10): `rtt` mediana 78,5 ms, p95 116,4 ms. «Guardar» (n = 10): `rtt` mediana 71,3 ms, p95 79,6 ms.
- Con diez valores por tipo, el p95 es el máximo. Son acciones aisladas de una sesión: no son mediciones de RNF-05 (S0 §10).

#### 3.4 Activación y escritorio

- Activación: en la corrida 2 hizo falta una petición (`attach_show_top`); en las corridas 3, 4, 6, 7, 8, 10 y 11 el editor ya estaba en primer plano (`already_foreground`); en las corridas 5 y 9 no hizo falta ninguna petición. Sin diagnóstico de activación y sin espera del operador.
- `user_idle_s_at_start` cuenta también la entrada de los propios clientes, así que no muestra si el responsable estaba en el equipo (como en la décima ronda).

### 4. Escritorio final, `stop.flag`, `window.pin` y cuaderno

- Tras la última invocación (comprobador de sólo lectura, sin entrada): ningún proceso de prueba (`ommatix.exe`, `calc.exe`, `CalculatorApp`, `ommatix-testeditor.exe`, `ommatix-testapp.exe`); carpeta temporal del cliente vacía; `window.pin` ausente; `stop.flag` ausente; cuaderno 515.
- Bloc de notas del responsable: PID 23164 con sus dos ventanas minimizadas; PID 8960 sin ventana. Sin cambio.
- Shell (`explorer.exe`, PID 36160): una ventana `CabinetWClass` oculta (la base) y las dos minimizadas fuera de pantalla del hallazgo 9. Sin cambio.

### 5. Decisiones que tomé y el responsable no fijó

1. **Diez invocaciones de una corrida** en lugar de una de diez, con los mismos argumentos salvo `--first-run` y `--max-live` (sección 2). El cupo de la serie (40) se repartió como se indica allí.
2. **La compuerta** pausa la serie ante cualquier corrida fallida sin evento de seguridad, y detiene todo ante un evento de seguridad, un rechazo de política o un aborto. Aplica por sí misma la regla de tres corridas fallidas seguidas. No hizo falta ninguna de las dos.
3. **Matriz:** E2E-02 pasa a `EJECUTADO_SUPERVISADO_PARCIAL` por la regla del responsable. Si el responsable prefiere otro estado, es un cambio de una línea en el PR.
4. **Piloto:** el piloto de la continuación cuenta como superado por la decisión del responsable, y no se repitió. La serie son las corridas 2 a 11.
5. **Configuración:** `s4r10-live.toml` permite también `calculatorapp.exe`, como en la décima ronda. El editor nunca recibió entrada en otra aplicación (ninguna comprobación halló la Calculadora en ejecución), pero una configuración sólo con el editor sería más estricta; el cliente no la exige.

### 6. Matriz

- `docs/thesis/matrices/plan_pruebas.csv`: E2E-02, `estado_aceptacion_producto` `PENDIENTE_PRODUCTO` → `EJECUTADO_SUPERVISADO_PARCIAL`. E2E-03, S4R-02 y S4R-03 sin cambio. Archivo UTF-8 con BOM y fin de línea LF, comprobado byte a byte.
- Ninguna casilla de la Definition of Done cambia, y esta sección no cierra RF-18, RNF-03 ni RNF-05.

### 7. Qué decide el responsable

1. La matriz: aprobar el paso de E2E-02 a `EJECUTADO_SUPERVISADO_PARCIAL` en este PR.
2. Si una serie puede ser una invocación de varias corridas: hace falta un cambio del cliente que detenga el protocolo ante un evento de seguridad (hallazgo 13), con una prueba que falle sin él, un hash nuevo y una ronda aparte. Mientras tanto, una invocación por corrida con compuerta.
3. E2E-03: la nueva regla de anclaje (en curso) y después una ronda propia. El hallazgo 8 sigue abierto.
4. Las dos ventanas minimizadas del shell (hallazgo 9): de quién son.
5. El cupo: quedan 114 con `--round-start 429`, o una ronda nueva puede empezar en 515.

### 8. Ejecución

Orden: verificación previa (sección 1); diez invocaciones, cada una precedida del comprobador y de la compuerta (sección 2); comprobador final y resumen de los registros. Los registros y los JSONL están en la carpeta de ejecuciones, fuera del repositorio; contienen estados, códigos, contadores, tiempos, tipos de control, identificadores de automatización y las etiquetas fijas del protocolo, pero ninguna captura ni texto de pantalla.

### Estado

E2E-02: diez de diez corridas puntuadas, cero falsos éxitos; la matriz lo registra como `EJECUTADO_SUPERVISADO_PARCIAL`, sujeto a la aprobación del responsable en el PR. E2E-03: sin ejecutar, detenido. Hallazgo 7: cerrado por decisión del responsable (un rechazo previo a cualquier entrada no cuenta como piloto no superado). Hallazgos 8 y 9: abiertos. Hallazgo 13 (nuevo, cliente): abierto, decisión del responsable. Cuaderno: 515. Ninguna casilla de la Definition of Done cambia.

## Preparación de la undécima ronda: E2E-03, anclar antes y verificar el sistema (ADR-0022)

**Fecha:** 14 de octubre de 2026. Rama `fix/s4-navigating-click-grounding`, desde `main` en `3f5fa0f` (fusión de #28). Cambian los dos clientes (`tools/s4/e2e_driver.py` y `tools/s4/e2e_dossier.py`) y sus pruebas, más la [ADR-0022](../adr/0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md), **aceptada por el responsable el 14 de octubre de 2026**. No cambia ninguna crate de Rust, el motor ni el editor. **No se envió entrada ni ninguna solicitud real** (cuaderno 494, sin cambio): las pruebas usan un escritorio simulado y no abren ninguna ventana. Responde al hallazgo 8 de la décima ronda (continuación) y al hallazgos 13 (de la serie), 15 y 16 de la [revisión](../reviews/2026-10-14-s4-tenth-round.md).

Hashes (SHA-256) de los clientes, que cambian y que quien ejecute la ronda comprueba antes de cada invocación (antes `143ea454…aee3d` y `85111edb…316df`):

- `tools/s4/e2e_driver.py`: **`768e77e3aaa065c72aef4cf78ba65a41787f13df624b4f414c55813d3997202d`**.
- `tools/s4/e2e_dossier.py`: **`359f6f281feff1a86c86fceb9530b849bfef02bc267e247a10ee4b2be020afc2`**.

### 1. Lo que muestran los registros sobre la causa

- **Mostrado.** El clic en la miga «carpeta-a» (un `SplitButton` identificado por `RuntimeId` antes del clic) se entregó, el motor informó `effect_observed` y la comprobación del sistema pasó: la carpeta de la ventana pasó a «carpeta-a». El descenso por la isla XAML bajo el cursor (profundidad 4, 64 elementos visitados, sin truncar ni ambigüedad) halló un `Text` cuya cadena no contenía el `RuntimeId` identificado. El control bajo el cursor, por tanto, no era el identificado ni su padre.
- **No mostrado.** *Por qué.* El cliente registraba el tipo de control y el `AutomationId` del elemento hallado, pero no su cadena de ancestros (sólo con `--debug-grounding`, cuyo evento no consta en el registro del piloto). La hipótesis del responsable (navegar reconstruye las migas de pan, así que el control clicado se destruye y se crea con otro `RuntimeId`; algo parecido ocurre con el panel de la Calculadora al elegir «Estándar») es coherente con los registros, pero **no está verificada**. Hay otra explicación compatible: el control sigue siendo el mismo y el `Text` es un descendiente más profundo que el padre que lee `identity_hit`. Los registros no distinguen entre las dos. Desde ahora el cliente registra la cadena (`post_grounded` y `pre_grounded`, por profundidad, tipo de control, `AutomationId`, clase y si el elemento es el identificado, sin nombres ni `RuntimeId`), de modo que la próxima corrida lo diga.
- **Consecuencia para la regla.** Sea cual sea la causa, el control clicado deja de ser un testigo fiable tras un clic que navega, y el sistema tiene uno mejor: el estado que el clic debía cambiar.

### 2. La regla (ADR-0022), tal como está implementada

1. **Anclaje del punto antes de cada clic, en todos los flujos.** Inmediatamente antes de enviar `execute_action` (y otra vez antes de cada reintento tras `STALE_SNAPSHOT`), el cliente lee, sólo lectura, el elemento bajo el punto objetivo (el centro del elemento identificado, con la misma consulta por posición y el mismo descenso por la isla que tras el clic). Debe ser el elemento identificado (mismo `RuntimeId`; el `AutomationId` sólo cuando falta el `RuntimeId`) o tenerlo por padre (`identity_hit`, sin cambio), y la ventana de nivel superior bajo el punto debe ser la ventana en primer plano que el protocolo acababa de exigir. Si no, **no se envía ninguna solicitud**: no se gasta cuaderno, no se reintenta y la corrida falla con el motivo (`no_point`, `unreadable`, `window_mismatch` o `identity_mismatch`). Una lectura que falla o agota su tiempo es un rechazo. El mismo anclaje precede al clic que cierra el menú «Editar» del Bloc de notas. Se registra como `pre_grounded` (por intento).
2. **Pasos de navegación: decide el sistema.** Para los pasos de la lista cerrada de abajo, tras el clic la identidad se **registra** (`post_grounded`: `identity_hit`, la cadena y el descenso) pero **no se exige**. Se **exige** la comprobación del sistema del paso (en Explorador, la carpeta que muestra la ventana) y ella es el criterio de éxito del paso (`post_grounded[].os_check`). Si falla tras un clic entregado: `grounding_wrong` y `independent_failed`, se detiene la corrida sin otra solicitud y, si el motor dijo `effect_observed`, `engine_false_success` (en un paso de navegación, ése es su significado: efecto observado con la comprobación del sistema fallida). Un paso de navegación sin comprobación del sistema, o que escribe, se rechaza antes de enviar nada.
3. **Lista cerrada.** `NAVIGATING_STEPS = {"explorer": (1, 2, 3)}` (en `tools/s4/e2e_driver.py`): las dos migas de pan de E2E-03 (la de «carpeta-a» y la de la raíz) y su clic en «Atrás», por número de paso. Ninguna opción de línea de comandos, bandera ni configuración la extiende (una opción `--navigating-steps` es un error de argumentos, probado) y ningún flujo la declara por su cuenta: el flujo del Explorador la lee de esa constante. **Nada más está en la lista, a menos que se escriba aquí.** En particular, el clic en «Estándar» de la Calculadora (el panel se cierra al elegirlo, la parada de la novena ronda) **no** está: su comprobación del sistema sería la lectura del modo por UI Automation, un diseño distinto que el responsable no ha decidido.
4. **Nada más se relaja.** Los demás pasos (Bloc de notas, Calculadora, aplicación de prueba, editor) conservan la identidad tras el clic exactamente como antes (`grounding`, `grounding_wrong`, `engine_false_success`). No se compara ningún nombre, no hay reintento tras entregar y no hay anclaje por cambio de estado fuera de la lista.

5. **Una parada de seguridad termina toda la invocación** (hallazgo 13 de la serie, PR #29). En una invocación de varias corridas (`--runs N`), un `grounding_wrong` o un `engine_false_success` en el registro de una corrida, aunque diga `success`, o cualquier aborto (`POLICY_DENIED` del alcance por ventana, primer plano que no es el objetivo, etiqueta ambigua, presupuesto, observación ilegible) termina la invocación en el acto: ninguna solicitud más, ninguna corrida más, el motivo registrado (`safety_stop` en el registro de la corrida; el aborto, con el suyo) y `stop.flag` creado, igual que con tres corridas fallidas seguidas (que se conservan). Un fallo que no es de seguridad (la ventana no llega al primer plano, el estado inicial no se ve, el objetivo no aparece) deja que empiece la siguiente corrida. Una comprobación de sólo lectura nunca escribe la parada.

Riesgo conocido de la regla 1, a propósito fail-closed: si el `Text` fuera un descendiente más profundo que el padre (la causa alternativa de §1), el anclaje previo rechazará el clic en la miga **antes de enviar entrada** y dejará la cadena registrada; ensanchar `identity_hit` sería una decisión nueva.

### 3. Pruebas

Antes de la corrección fallan o dan error **41** pruebas (33 de la regla de anclaje y 8 de la parada de seguridad: `SafetyStopTest` en `test_e2e_driver.py`, que prueba que un evento de seguridad en la corrida 1 de 3 no envía nada más y crea la parada, que un fallo corriente deja empezar la siguiente corrida y que tres seguidos siguen creando la parada, y dos pruebas sobre el escritorio del Explorador en `test_e2e_dossier.py`) (se comprobó con los clientes originales y las pruebas nuevas): en `test_e2e_driver.py`, `PreClickGroundingTest` (el punto se lee y se registra antes de la solicitud; un hijo del identificado pasa y un descendiente más profundo no; otro elemento, ninguno, mismo `AutomationId` con otro `RuntimeId`, otra ventana, un punto ilegible y una lectura que falla no envían nada; el anclaje se repite antes de un reintento; el clic de `--calc-standard` y el de restaurar el menú también se anclan; los registros no llevan nombres) y `NavigatingStepTest` (control recreado con comprobación del sistema correcta: anclado y éxito; comprobación fallida: `grounding_wrong`, `engine_false_success`, parada sin otra solicitud; sin `effect_observed` no hay falso éxito; un paso fuera de la lista conserva la identidad); en `test_e2e_dossier.py`, el flujo del Explorador completo con `pre_grounded` y `post_grounded`, el control recreado con carpeta correcta, la carpeta incorrecta con parada, el desajuste previo sin solicitud (otro control, cadena vacía, otra ventana, lectura que falla), la lista cerrada (constante inmutable, ningún otro flujo la declara, ninguna opción de línea de comandos en los dos clientes), un paso de navegación sin comprobación, los registros sin nombres ni `RuntimeId`, el editor (el anclaje previo también rige y el editor no navega) y esta sección (`test_the_s4_document_names_the_closed_list`). Las pruebas existentes de las islas se adaptaron porque ahora hay una lectura previa al clic (los recuentos de recorridos incluyen la lectura previa); la regla de las islas no cambia. `tools/s4`: **297 correctas** (258 antes).

### 4. Verificación

- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: sin avisos.
- `cargo test --workspace`: **447 pasadas** (sin cambio: ninguna crate de Rust cambia), 0 fallidas y 1 ignorada.
- `python -m unittest discover -s tools/s4 -p "test_*.py"`: **297 correctas** (258 antes).
- Desde `docs/thesis`: `referencia_ejecutable/run_tests.py`, 48 de 48; `validar_paquete.py`, **451** comprobaciones y 0 fallidas (449 antes: la ADR-0022 añade dos). Los archivos de resultados del oráculo cambiaron sólo en marcas de tiempo y plataforma y se restauraron; los de la validación documental cambiaron de verdad (451 comprobaciones, 22 ADR) y se confirman con fin de línea LF.

### Qué debe mostrar la próxima ronda en vivo

- **Antes:** los SHA-256 de arriba (clientes), los del motor y el editor recompilados por quien ejecute la ronda, `stop.flag` y `window.pin` ausentes, `--round-start` propio (cuaderno 494; el cupo de la décima ronda, `--round-start 429`, deja 135).
- **E2E-03, piloto:** `pre_grounded` con `status` `ok` en cada intento. Si el primer clic se rechaza con `identity_mismatch`, la cadena registrada dice si el `Text` es hijo, nieto u otro control; ningún clic se habrá enviado. Con el clic entregado: `post_grounded[].os_check` verdadero en los tres pasos, `grounding_wrong` 0 y `engine_false_success` 0; `post_grounded[].identity_hit` y su cadena dirán si el control se reconstruyó (otro `RuntimeId`) o si es el mismo con un descendiente más profundo.
- **Los demás flujos** no deben cambiar: `grounding` sigue siendo la identidad tras el clic; ahora cada solicitud lleva antes su `pre_grounded`, que añade lecturas de sólo lectura (dos consultas de UI Automation por solicitud) y puede detener una corrida antes de enviar si el punto no es el identificado. En Calculadora, Bloc de notas, aplicación de prueba y editor eso no se ha visto en vivo.
- **Nada de esto está confirmado en vivo.**

### Estado

Hallazgo 8: la regla que el responsable decidió está implementada y probada con un escritorio simulado; **no confirmada en vivo**, y la causa sigue sin verificar (§1). Cuaderno: 494, sin cambio. Las matrices no cambian.

## Undécima ronda: E2E-03 no llegó a la sonda; las ventanas del propietario ya no están y el equipo recibió entrada

**Fecha:** 14 y 15 de octubre de 2026. **Estado:** detenida antes de cualquier entrada. Rama `docs/s4-eleventh-round` desde `main` en `32e1644` (fusión de #30). CHANGELOG 1.0.43. Revisión: [`reviews/2026-10-14-s4-eleventh-round.md`](../reviews/2026-10-14-s4-eleventh-round.md). **No se envió ninguna solicitud real** (cuaderno 515, sin cambio): ninguna sonda, ningún piloto y ninguna corrida puntuada.

### 1. Verificación previa a la entrada en vivo

- **Motor y editor sin recompilar.** `git diff --stat 0cb126f..HEAD -- src` no muestra cambios. `target/release/ommatix.exe` tiene el SHA-256 `a137ccd1…d548` y `ommatix-testeditor.exe` `f358b3a9…1d20`, los de la décima ronda.
- **Clientes fijados**, comprobados con su SHA-256 antes de cada invocación: `tools/s4/e2e_driver.py` `768e77e3…7202d` y `tools/s4/e2e_dossier.py` `359f6f28…0afc2`, los de la ADR-0022. Configuración `s4r10-explorer.toml` `ab6c33b5…33d7`, sin cambio.
- **Comprobaciones de la rama** (fuera del repositorio), todas con código de salida 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; la misma con las características de Windows; `cargo test --workspace` (447 pasadas, 1 ignorada); `tools/s4` (297 pruebas correctas); el editor (clippy y 24 pruebas); `referencia_ejecutable/run_tests.py` (48 de 48); `validar_paquete.py` (451 comprobaciones, 0 fallidas); `cargo test -p ommatix-acceptance` (37 de 37 escenarios; 15 y 6 pruebas).
- **Preflight de sólo lectura** antes de la sonda: el cuaderno marca 515; `stop.flag` y `window.pin` no existen; no hay procesos de prueba; la carpeta temporal del cliente está vacía; Notepad (PID 23164) conserva sus dos ventanas. **La línea base de las ventanas del propietario no se cumple** (§2).

### 2. Por qué se detuvo: las ventanas del propietario ya no están, y el equipo recibió entrada

La línea base aceptada en la décima ronda son dos ventanas `CabinetWClass` minimizadas, fuera de pantalla, del proceso del shell (PID 36160), que el responsable confirmó como suyas. Dos comprobaciones de sólo lectura (clase, PID, proceso, estado, tamaño y posición; sin títulos), separadas por unos minutos, dan:

| Ventana | Primera comprobación (cuaderno 515) | Segunda comprobación |
|---|---|---|
| `CabinetWClass` minimizada, 160 × 28 en (−32000, −32000) | visible | **no existe** |
| segunda `CabinetWClass` minimizada, fuera de pantalla | no existe | no existe |
| `CabinetWClass` oculta, 1920 × 1032 en (0, 0) (línea base) | presente | presente |

Entre las dos comprobaciones, el tiempo desde la última entrada de teclado o ratón pasó de 239,8 s a 116,5 s: el equipo recibió entrada. Ningún comando de esta ronda envía entrada (las pruebas usan dobles y los clientes no llaman a `SendInput`), así que la produjo alguien más; no se ha verificado quién. El resto no cambió: `explorer.exe` del shell (PID 36160), Notepad (PID 23164 con dos ventanas; PID 8960 sin ventana) y `ApplicationFrameHost.exe` (PID 10076). La causa de la desaparición no está verificada. La última comprobación que contó dos ventanas fue la posterior a la serie de E2E-02 (cuaderno 515), y desde entonces no se ha ejecutado ningún cliente. El cliente sólo cierra la ventana que abrió, por su identificador, y sólo elige ventanas que no existían antes de su propio lanzamiento.

La segunda comprobación ya no tiene ventanas visibles que contar, así que la compuerta la aceptaría. No se ejecutó la sonda por dos razones. La línea base que el responsable confirmó ya no existe. Y el equipo parece en uso: una sonda abre y maximiza una ventana del Explorador y cambia el primer plano, así que no debe correr con alguien trabajando en el equipo. La compuerta no se amplió para aceptar la situación nueva.

### 3. Lo que decide el responsable

1. **¿Está el equipo libre?** Y **¿cerró usted las dos ventanas minimizadas?** Si el equipo está libre y acepta el estado actual (ninguna ventana `CabinetWClass` visible y la oculta del shell) como línea base, la ronda sigue en su orden: sonda sin entrada; piloto con `--max-live 8`; y, sólo si el piloto pasa, diez corridas puntuadas en diez invocaciones de una corrida, con compuerta entre ellas y `--max-live` igual al presupuesto que quede de 60.
2. Si el equipo no está libre, o el cambio no se explica, hay que investigar antes de cualquier entrada.

### 4. Estado de E2E-03 y del cupo

- **E2E-03: `PENDIENTE_PRODUCTO`**, sin cambio en la matriz. No hubo sonda, piloto ni serie.
- **Cupo:** cuaderno 515 con `--round-start 429`: 86 solicitudes usadas de 200 y 114 disponibles. Si se reanuda, el plan usa 8 en el piloto y 60 en la serie (68 en total).
- **Escritorio:** sin procesos de prueba, carpeta temporal vacía, `stop.flag` y `window.pin` ausentes, Notepad sin cambios y la ventana oculta de línea base sin tocar.

### 5. Otros cambios de documentación

- `docs/handoff.md`: se elimina una copia obsoleta de la fila y del párrafo de la ADR-0022. La copia obsoleta citaba un hash de `e2e_driver.py` anterior (`a758d172…`) y las cifras de un estado intermedio (286 pruebas, 33 fallidas sin el cambio). La copia vigente (`768e77e3…`, 297 pruebas, 41 fallidas sin el cambio) se conserva.
- `README.md` y `README.es.md`: un párrafo de la undécima ronda en cada uno, después del de la regla de anclaje.

## Undécima ronda (continuación): con la línea base nueva del responsable, el equipo recibe entrada antes de la sonda

**Fecha:** 15 de octubre de 2026. **Estado:** detenida antes de cualquier entrada, por la regla de entrada ajena. Rama `docs/s4-eleventh-round-run` desde `main` en `c91862f` (fusión de #31). CHANGELOG 1.0.44. Revisión: [`reviews/2026-10-14-s4-eleventh-round.md`](../reviews/2026-10-14-s4-eleventh-round.md), hallazgos 3 y 4. **No se envió ninguna solicitud real** (cuaderno 515, sin cambio): ninguna sonda, ningún piloto y ninguna corrida puntuada.

### 1. La línea base nueva del responsable

El responsable indicó que cerró él mismo las dos ventanas minimizadas que se usaban como línea base, que había dejado de usar el equipo y que la línea base nueva es: ninguna ventana `CabinetWClass` visible, y la oculta del proceso del shell (1920 × 1032 en (0, 0), PID 36160) intacta y sin tocar. La comprobación previa y la compuerta de las invocaciones se actualizaron a ese estado, sin excepciones: el número de ventanas visibles debe ser cero.

### 2. Verificación previa a la entrada en vivo

- **Código y motor.** `main` en `c91862f`, sin cambios locales. `git diff --stat 0cb126f..HEAD -- src tools/s4/testeditor` no muestra cambios, así que el motor y el editor no se recompilan: `target/release/ommatix.exe` `a137ccd1…d548` y `ommatix-testeditor.exe` `f358b3a9…1d20` coinciden.
- **Clientes, configuraciones y artefactos fijados**, comprobados con su SHA-256 en la comprobación previa: `tools/s4/e2e_driver.py` `768e77e3…7202d`; `tools/s4/e2e_dossier.py` `359f6f28…0afc2`; `s4r10-live.toml` `0428daf8…c1e2`; `s4r10-explorer.toml` `ab6c33b5…33d7`; manifiesto, ONNX Runtime y DirectML sin cambio.
- **Comprobaciones de la rama**, todas con código de salida 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; la misma con las características de Windows; `cargo test --workspace` (447 pasadas, 0 fallidas, 1 ignorada); `python -m unittest discover -s tools/s4` (297 pruebas correctas); el editor (`cargo clippy -- -D warnings` y 24 pruebas pasadas); `referencia_ejecutable/run_tests.py` (48 de 48); `validar_paquete.py` (451 comprobaciones, 0 fallidas).

### 3. La comprobación previa cumple, y el contador de inactividad marca entrada

- **A las 00:24 (hora local del equipo), con el cuaderno en 515:** ninguna ventana `CabinetWClass` visible; la oculta del PID 36160; Notepad (PID 23164) con sus dos ventanas; sin procesos de prueba; carpeta temporal del cliente vacía; `stop.flag` y `window.pin` ausentes. Todas las condiciones se cumplen.
- **Pero** el contador de inactividad marcaba 77,7 s: la última entrada de teclado o ratón fue hacia las 00:23. La verificación previa terminó a las 00:22 y ningún comando de la ronda envía entrada. La regla del responsable es explícita: si el contador muestra entrada que no provocó durante la ronda, se detiene todo el trabajo en vivo y se informa. Por eso no se empezó nada.
- **Segunda lectura, de sólo lectura, a las 00:25:** 128,7 s, es decir, los 77,7 s más los 51 s que pasaron. No hubo entrada entre las dos lecturas. La entrada de las 00:23 quedó sin explicar.

### 4. Lo que no se ejecutó

No hubo sonda, piloto ni serie de E2E-03. No se abrió ninguna ventana del Explorador, no se cambió el primer plano y no se envió ninguna entrada.

### 5. Lo que decide el responsable

1. ¿Estaba el equipo en uso hacia las 00:23? ¿Fue una entrada suya?
2. Si el equipo está libre y acepta la línea base nueva, la ronda sólo puede reanudarse con una autorización nueva: sonda sin entrada; piloto con `--max-live 8`; y, sólo si el piloto pasa, diez corridas puntuadas en diez invocaciones de una corrida, con compuerta entre ellas y un presupuesto de serie de 60.
3. Sobre el alcance de la compuerta: el contador cuenta también las entradas del propio cliente, así que dentro de una invocación no distingue una entrada ajena de la suya. La compuerta comprueba los intervalos entre invocaciones (el contador tiene que crecer lo que haya pasado); dentro de cada invocación deciden las comprobaciones del sistema (primer plano, control bajo el punto y carpeta). Vigilar el contador de forma continua sería una decisión nueva.

### 6. Estado de E2E-03 y del cupo

- **E2E-03:** `PENDIENTE_PRODUCTO`, sin cambio en la matriz.
- **Cupo:** cuaderno 515 con `--round-start 429`: 86 solicitudes usadas de 200; 114 disponibles. Si se reanuda, el plan usa 8 en el piloto y 60 en la serie: 68 en total.
- **Escritorio:** sin procesos de prueba, carpeta temporal vacía, `stop.flag` y `window.pin` ausentes, Notepad sin cambios y la ventana oculta de línea base sin tocar.

### 7. Otros cambios de documentación

- `docs/handoff.md`: una fila y un párrafo de esta continuación.
- `README.md` y `README.es.md`: un párrafo en cada uno, después del de la undécima ronda.

## Undécima ronda (continuación 2): E2E-03 se ejecuta en vivo; la sonda y el piloto pasan, tres corridas puntuadas pasan, y la ronda se detiene por entrada entre invocaciones

**Fecha:** 15 de octubre de 2026 (hora local del equipo, UTC-5). **Estado:** detenida antes de la quinta corrida puntuada, por la regla de entrada del responsable. Rama `docs/s4-eleventh-round-run2` desde `main` en `bd025e3` (fusión de #32). CHANGELOG 1.0.45. Revisión: [`reviews/2026-10-14-s4-eleventh-round.md`](../reviews/2026-10-14-s4-eleventh-round.md), hallazgos 5 a 9. **Solicitudes reales: 15** (cuaderno de 515 a 530, con `--round-start 429`: 101 usadas de 200, 99 libres). Matriz: E2E-03 sigue `PENDIENTE_PRODUCTO`.

### 1. Las reglas del responsable (2026-10-15)

El responsable confirmó que la entrada de las 00:23 fue suya, anterior a la parte en vivo, y que había dejado de usar el equipo. Aprobó una regla de entrada que sustituye a la anterior:

- Una etapa en vivo (sonda, piloto o cada invocación puntuada) sólo empieza si el contador de inactividad marca al menos 90 s justo antes. Si no llega a 90 s en diez minutos, se detiene todo y se informa.
- Entre invocaciones, la entrada que la ronda no provocó (un contador menor que el tiempo transcurrido desde el fin de la invocación anterior) detiene todo el trabajo en vivo.
- Línea base: ninguna ventana `CabinetWClass` visible; la oculta del proceso del shell (PID 36160) intacta y sin tocar.

### 2. Verificación previa y artefactos fijados

- **Código.** `main` en `bd025e3`. `git diff --stat 0cb126f..HEAD -- src tools/s4/testeditor` no muestra cambios, así que el motor y el editor no se recompilan.
- **Comprobaciones previas a la entrada**, todas con código de salida 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; la misma con `dxgi,sendinput,directml,winrt-ocr,uia`; `cargo test --workspace` (447 pasadas, 0 fallidas, 1 ignorada); `python -m unittest discover -s tools/s4 -p "test_*.py"` (297 correctas); el editor (`cargo clippy -- -D warnings` y 24 pruebas pasadas); `referencia_ejecutable/run_tests.py` (48 de 48); `validar_paquete.py` (451 comprobaciones, 0 fallidas).
- **Clientes, configuraciones y artefactos**, comprobados con su SHA-256 en la comprobación previa de cada etapa: `tools/s4/e2e_driver.py` `768e77e3…7202d`; `tools/s4/e2e_dossier.py` `359f6f28…0afc2`; `target/release/ommatix.exe` `a137ccd1…d548`; `ommatix-testeditor.exe` `f358b3a9…1d20`; `s4r10-live.toml` `0428daf8…c1e2`; `s4r10-explorer.toml` `ab6c33b5…33d7`. El manifiesto, ONNX Runtime y DirectML no cambian.
- **Entorno del cliente.** Cada invocación corrió con `PYTHONIOENCODING=utf-8`. El cliente no cambió (hallazgo 7).

### 3. Sonda sin entrada: la compuerta se cumple

A las 00:49:36, la lectura previa marcó un contador de 150,2 s, cuaderno 515 y sin problemas. La sonda (`e2e_dossier.py --app explorer --mode probe`, `s4r10-explorer.toml`, cero solicitudes) dio: «carpeta-a» y la miga de la raíz, cada una con un único elemento de motor utilizable `confirmed` (zona `unknown`; en UIA, `SplitButton` y `Text`); «Atrás», `unknown` (UIA `Button`), como se esperaba sin historial. La ventana se cerró, la carpeta temporal se eliminó (2 archivos y 3 directorios) y no quedó pin. Lectura posterior a las 00:49:45, sin problemas.

### 4. Comprobación del alcance (ACTIVE CONTROL)

Antes del primer piloto, `serve` con la configuración del Explorador y `--input sendinput`, con la entrada tomada de `/dev/null` y sin solicitudes: código 0 y ninguna salida estándar. La línea ACTIVE CONTROL nombra `explorer.exe`, la clase `CabinetWClass` y el archivo de pin, sin advertencias de alcance.

### 5. Piloto (corrida 1, `--max-live 8`): pasa

Criterios fijados antes de la ejecución: éxito del cliente; los tres pasos entregados y verificados por el sistema; cero falsos éxitos, controles erróneos y fallos de comprobación; ventana, carpeta temporal y pin retirados; ningún aborto ni parada de seguridad. Resultado: 3 solicitudes (cuaderno 515 → 518); 3 entregadas con `effect_observed`; 3 de 3 comprobaciones del sistema; 0 falsos éxitos; 0 controles erróneos; anclaje previo `ok` en los tres pasos; identidad `ok` (`SplitButton`, `SplitButton` y `Button`, con un candidato cada una); cierre por manejador; árbol sin cambios.

**Lo que muestran los registros de la cadena posterior al clic.** Tras los pasos 1 y 2, el elemento bajo el cursor fue un `Text` dentro de un `SplitButton` cuyo `RuntimeId` no es el identificado, y ninguna de las cinco filas de la cadena es el elemento identificado. Tras el paso 3 («Atrás»), el elemento bajo el cursor era el `Button` identificado. Esto es coherente con la causa (a) de la [ADR-0022](../adr/0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md): la navegación reconstruye la miga bajo otro `RuntimeId`. El registro no distingue si el control anterior se destruyó o cambió de lugar, así que no lo confirma del todo. La comprobación del sistema (la carpeta) decidió esos pasos, como exige la ADR.

### 6. Serie puntuada: tres de diez corridas pasan; la cuarta no se envió

| Invocación | Éxito | Solicitudes (entregadas + rechazadas) | Comprobaciones del sistema | Falsos éxitos | Control erróneo | Cuaderno | Hora local |
|---|---|---|---|---|---|---|---|
| Corrida 1 (piloto) | sí | 3 (3 + 0) | 3 de 3 | 0 | 0 | 515 → 518 | 00:50 |
| Corrida 2 | sí | 3 (3 + 0) | 3 de 3 | 0 | 0 | 518 → 521 | 00:52 |
| Corrida 3 | sí | 4 (3 + 1) | 3 de 3 | 0 | 0 | 521 → 525 | 00:54 |
| Corrida 4 | sí | 5 (3 + 2) | 3 de 3 | 0 | 0 | 525 → 530 | 00:56 |
| Corrida 5 | no enviada | — | — | — | — | 530 | — |

Las tres negativas `STALE_SNAPSHOT` (corrida 3 en el paso 1; corrida 4 en los pasos 1 y 2) se produjeron con `input_delivered: false`: se rechazaron antes de cualquier entrada, se reintentaron y se entregaron. Cada reintento cuenta como una solicitud del cuaderno. Presupuesto de la serie: 60 desde el cuaderno 518; usadas 12 (corridas 2 a 4).

**La entrada que detuvo la serie.** La corrida 4 terminó con código 0 hacia las 00:56:10; la lectura posterior (00:56:12) marcó 3,7 s. La espera previa a la corrida 5 hizo 18 lecturas entre las 00:56:13 y las 00:59:23, y ninguna llegó a 90 s. Por tanto hubo entrada de teclado o ratón más de una vez en ese intervalo: al menos otra entre las 00:56:10 y las 00:57:40, y la última hacia las 00:57:50 (a las 00:59:23 el contador marcaba 93,0 s; sin entrada habrían sido 194,3 s). La compuerta anterior a la corrida 5 detuvo la serie (salida 5), y no se envió ninguna solicitud después. Ningún proceso de la ronda estaba en marcha en ese intervalo: el cliente había terminado, y la espera y la comprobación sólo leían. El registro del sistema no tiene eventos de cambio de hora, de energía ni del servicio de hora en las últimas tres horas, así que el faltante de 101,3 s no lo explica un salto del reloj.

### 7. Latencias (12 acciones entregadas: piloto y corridas 2 a 4)

- `action_total`: mediana 77,2 ms; p95 91,6 ms (mínimo 66,3 ms; máximo 91,6 ms).
- `rtt`: mediana 77,5 ms; p95 91,9 ms.

Con doce valores, el p95 es prácticamente el máximo. Son acciones aisladas, no RNF-05.

### 8. Escritorio final y cupo

- **Lectura de sólo lectura a las 01:00:44:** sin problemas. Ningún proceso de prueba; carpeta temporal `%TEMP%\ommatix-e2e` vacía; `stop.flag` y `window.pin` ausentes; ninguna ventana `CabinetWClass` visible; la oculta del PID 36160 intacta; Notepad (PID 23164) con sus dos ventanas, sin cambios.
- **Cupo:** cuaderno 530; con `--round-start 429`, 101 de 200 usadas y 99 libres. Si la serie reanuda, quedan las corridas 5 a 11 (siete); el presupuesto de la serie lo fija el responsable.

### 9. Lo que decide el responsable

1. ¿Fue suya la entrada entre las 00:56 y las 00:58? Hubo al menos dos momentos de entrada en ese intervalo.
2. Si el equipo queda libre, ¿reanuda la serie? Necesita una autorización nueva desde el cuaderno 530, para las corridas 5 a 11. La compuerta y el presupuesto los fija esa autorización.
3. Hallazgo 6: la compuerta compara el contador de inactividad con el reloj de pared, y un salto del reloj daría la misma señal que una entrada. ¿Se cambia a una medida sólo de ticks?
4. Hallazgo 7: el cliente de E2E-03 no escribe su salida en UTF-8 al redirigirla. La corrección es un cambio del cliente con prueba y hash nuevo, es decir, una ronda nueva. Esta ronda usó `PYTHONIOENCODING=utf-8`.
5. Hallazgo 8: ¿basta la cadena posterior al clic para aceptar como observada la causa (a) de la ADR-0022?

### 10. Otros cambios de documentación

- `docs/handoff.md`: una fila y un párrafo de esta continuación; el historial cita 1.0.1–1.0.45.
- `README.md` y `README.es.md`: un párrafo en cada uno, después del de la continuación de la undécima ronda.
- Revisión `docs/reviews/2026-10-14-s4-eleventh-round.md`: hallazgos 5 a 9.
- Matriz `docs/thesis/matrices/plan_pruebas.csv`: sin cambio.
