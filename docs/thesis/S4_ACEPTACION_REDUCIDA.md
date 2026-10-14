# S4: protocolo de aceptación reducido (corridas E2E supervisadas)

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

### 6. Estado en la matriz

- **E2E-01:** `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambio. Esta ronda confirma los clics y los controles por accesibilidad, pero la escritura no se verificó en la serie puntuada (0 de 3).
- **E2E-02 y E2E-03** de la matriz (editor con escritura y guardado; explorador de archivos): sin cambio, no ejecutadas.
- **PERF-02:** sin cambio, parcial. La etapa `accessibility` ya está medida en vivo.
- **UT-44, IT-01 a IT-07, SEC y EV:** sin cambio.
- **RF-18, RNF-03 y RNF-05:** sin cambio. RNF-05 sigue por encima de 150 ms.

### 7. Cómo repetirlo y reanudar

El cuaderno de solicitudes (`ledger.txt`) está en 160 y el tope es 400. El archivo de parada, `stop.flag`, existe y **lo retira sólo el responsable**. Antes de reanudar conviene un sondeo y el piloto de `--focus-click`, que no se ha ejecutado.

```text
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app testapp --mode probe --runs 1 --ledger <dir>\ledger.txt --results <dir>\probe.jsonl
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app testapp --mode run --runs 10 --focus-click --max-live 100 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e01
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app calculator --mode run --runs 10 --max-live 60 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e02
python tools\s4\e2e_driver.py --exe target\release\ommatix.exe --config <local>.toml --app notepad --mode run --runs 10 --max-live 30 --ledger <dir>\ledger.txt --results <dir>\s4r2.results.jsonl --session-id s4r2-e2e03
```

La configuración local es la de §1 de esta ronda. Nunca se guarda en el repositorio.
