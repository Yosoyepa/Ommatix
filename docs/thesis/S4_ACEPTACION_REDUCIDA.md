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
