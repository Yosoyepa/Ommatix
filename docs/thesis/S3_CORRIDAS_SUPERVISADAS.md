# S3: corridas supervisadas con captura y entrada reales

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable del proyecto autorizó el 14 de octubre de 2026 corridas supervisadas en la máquina de referencia con captura real y entrada real (`SendInput`) **limitadas a `notepad.exe` con datos desechables**, con parada local por `stop_file`. **Alcance:** una aplicación, un monitor (salida 0), DPI 100 %, una máquina. Este informe registra lo que se ejecutó, con qué resultado y qué no se ejecutó; no cierra ningún requisito RF/RNF ni ninguna casilla DoD.

Todas las cifras son **mediciones [V]** con el `n` indicado. No se guardó ningún frame ni ninguna etiqueta: la pantalla y el OCR (es-ES) se procesaron sólo en memoria (ADR-0012). El cliente imprime sólo estados, códigos, recuentos, números de frame y tiempos; las etiquetas se usan en memoria para elegir el objetivo y no se escriben en ningún sitio.

## 1. Entorno

| Campo | Valor |
|---|---|
| Hardware y sistema | Los de [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §1: Windows 11 Pro 26H2 (build 26300.9550), Ryzen 7 5700X, 32 GiB, 1920×1080, dos monitores, se captura la salida 0, DPI 100 % |
| Binario | `cargo build --release -p ommatix --features dxgi,sendinput,onnx,winrt-ocr`, `rustc 1.99.0` |
| Código | Corridas A, B y la observación de control: rama `feat/s3-supervised-runs` en `a47700f` (PR #8, sin cambios). Corrida C: la misma rama con las correcciones de §6 (commit de este PR) |
| Modelos | Los de S0: `icon_detect_v3` en ONNX Runtime 1.28.0 (CPU, 1 hilo), manifiesto `8e1c7ad6…b7014f1`, `onnxruntime.dll` `18370c37…4470257`; OCR `winrt` es-ES |
| Cliente MCP | [`tools/s3/mcp_driver.py`](../../tools/s3/mcp_driver.py): JSON-RPC 2026-07-28 sobre stdin/stdout de `ommatix serve --source dxgi --output 0`, sin `initialize`, `_meta` por solicitud. No es un cliente MCP de terceros (capítulo 07 §7.1 sigue pendiente) |
| Configuración sin actuación | `actuation_enabled = false`, `allowed_applications = ["notepad.exe"]`, `include_values = false`, `stop_file` en el directorio temporal de la sesión, `[models]` como S0 con rutas absolutas. SHA-256 `0a5f3f20dacc85d3b834a0e257dc8f387903f69554bb61b07dc4ae73d12a8d4d` |
| Configuración con actuación | Idéntica salvo `actuation_enabled = true`. SHA-256 `5755217c1bf88695102af9ccca909d9cb9972f8710846231071a20b69d86e941`. Ninguna contiene secretos; los hashes sólo identifican el archivo usado |

**Reglas de la corrida.** Entrada real sólo a través de la compuerta del motor (`execute_action` con `dry_run: false`, `--input sendinput`). Antes de cada acción real el cliente comprueba, sólo leyendo, que el proceso en primer plano es `notepad.exe` y que su ventana está maximizada en el monitor principal; si no, crea el archivo de parada y termina. El único objetivo permitido es un elemento `confirmed`, único en su snapshot, cuya etiqueta OCR recortada es exactamente `Archivo`/`File`, `Editar`/`Edit` o `Ver`/`View`. Sólo se envían clics; un menú abierto se cierra con un clic sobre la misma etiqueta en una observación nueva; nunca `type` ni clics en pestañas, en elementos de menú ni en los botones de la ventana. `scroll` no se usó: no había forma de saber sin mirar que un elemento sin etiqueta fuese el área de texto. Máximo: 10 solicitudes `dry_run: false` en total.

## 2. Preparación e incidencias

- `Start-Process notepad` abrió ventanas nuevas del Bloc de notas pero **no** les dio el primer plano: Windows lo niega a un proceso lanzado en segundo plano. Se pidió la activación de la ventana (`WScript.Shell.AppActivate`, que no envía entrada) cada 20 s; el Bloc de notas quedó en primer plano unos tres minutos y medio después. No se puede atribuir con certeza a la petición, porque mientras tanto el responsable usaba otra aplicación en el segundo monitor.
- **Dos barras de menú visibles.** Las dos primeras corridas sin actuación encontraron dos elementos `Archivo`, dos `Editar` y dos `Ver`: una ventana del Bloc de notas sin maximizar quedaba encima de otra maximizada. El cliente no eligió ninguno, creó el archivo de parada y terminó sin enviar nada (se retiró el archivo después). Desde entonces el cliente exige que la ventana en primer plano esté maximizada en el monitor principal, y se lanzó una ventana maximizada. Esas dos corridas sólo aportan el dato del primer frame (§3.1).
- El Bloc de notas de Windows 11 restaura pestañas de sesiones anteriores; sus títulos pasaron por el OCR en memoria y no se registran. Quedan abiertas tres ventanas vacías del Bloc de notas creadas para estas corridas; no se cerraron porque cerrarlas exigía tocar ventanas o botones fuera de lo autorizado.

## 3. Resultados

### 3.1 IT-01, captura DXGI

| Comprobación | Resultado |
|---|---|
| Frame real de la salida 0 (`cargo test --release -p ommatix-adapter-capture --features dxgi -- --ignored`, sin guardar píxeles) | 1920×1080, stride 7680 B (= 4·1920), epoch 1, `DisplayMapping` con origen (0, 0) y tamaños de frame y SO iguales; primer frame con 0 rectángulos sucios y 0 de movimiento |
| Rotación | Identidad: `check_rotation` aceptó la salida en todas las sesiones (una salida rotada es un error del adaptador). No se probó una salida rotada |
| `geometry_epoch` | 1 en todas las observaciones de todas las sesiones (n = 21 observaciones registradas en A, B y C): ni la geometría ni la duplicación cambiaron |
| `frame_seq` | Crece de uno en uno por captura. Una acción real entregada añade dos capturas (la de vigencia y la de verificación: de `s3`/frame 3 a `s4`/frame 6 en C), un `STALE_SNAPSHOT` por vigencia añade una, y una denegación por política o una repetición de `request_id`, ninguna (de frame 15 a 16 alrededor de la repetición de IT-05) |
| Primer frame en blanco (S0 §7) | Reproducido en **6 de 6** sesiones `serve` (las dos abortadas, A, la observación de control, B y C): la primera observación da 1 elemento y un OCR de 23–30 ms; la segunda ve el Bloc de notas (94–141 elementos según las ventanas visibles). Corregido después de estas corridas, pendiente de una corrida nueva (§6, hallazgo 4) |
| Pérdida de acceso | No se provocó (no se cambió de modo ni se pasó al escritorio seguro) |

### 3.2 SEC-01, el agente no puede habilitar la actuación

- `ommatix serve --source dxgi --input sendinput` con la configuración sin actuación **no arranca**: `sendinput input requires [safety] actuation_enabled = true in the --config file; the agent can never enable actuation` (salida 1). No se abrió la captura ni el actuador.
- Corrida A (`--input dry-run` por defecto, configuración sin actuación, sesión `s3-dry`): tras observar, el objetivo fue `Archivo` (único, `confirmed`, rol `unknown`, etiqueta OCR). `dry_run: true` → `simulated`, `input_delivered: false`. La misma intención con `dry_run: false` (`s3-sec01-1`) → `rejected`, `POLICY_DENIED`, `retryable: false`, `next_step: operator_review`, 0,2 ms. Métricas al final: `mode: simulation`, `rejected_actions` 1, `delivered_actions` 0. La política no tiene ninguna vía de escritura desde MCP: `tools/list` sólo ofrece las tres herramientas del contrato.

### 3.3 IT-03, actuador `SendInput` (clics sobre `Archivo`)

Corrida B (código de PR #8, sesión `s3-live`): `ACTIVE CONTROL` en stderr con `notepad.exe` y el archivo de parada. Corrida C (con las correcciones, sesión `s3-live2`). En las dos el objetivo fue `Archivo`, `target_id` `e32`.

| Corrida | `request_id` | Estado | `input_delivered` | `delivered_actions` | Evidencia posterior |
|---|---|---|---|---|---|
| B | `s3-it03-open-1` | `indeterminate`, `INDETERMINATE_EFFECT`, `observed_effect: unknown`, `reconcile_effect`; 19,6 ms | **false (defecto, §6.1)** | 0 → 0 | La observación siguiente con detección pasa de 94 elementos/15 etiquetas OCR a 116/33: el menú se abrió |
| C | `s3-close-1` | `indeterminate`, "input was delivered but no visual change was observed"; 32,0 ms | true | 0 → 1 | 137/33 → 101/15: cerró el menú que B dejó abierto |
| C | `s3-it03-open-1` | `indeterminate`, mismo mensaje; 18,0 ms | true | 1 → 2 | 101/15 → 132/35: menú abierto |
| C | `s3-it03-close-1` | `rejected`, `STALE_SNAPSHOT`, "the screen changed since the observation; nothing was delivered"; 5,7 ms | false | sin cambio | Nada enviado; se observó de nuevo |
| C | `s3-it03-close-2` | `indeterminate`; 33,1 ms | true | 2 → 3 | 126/36 → 97/16: menú cerrado |

- **Clics que llegaron al SO: 4**, todos sobre `Archivo` en la ventana maximizada del Bloc de notas en primer plano (atestiguada por el motor y por el actuador). Ningún efecto inesperado.
- **`expected_effect: visual_change` nunca se vio** (4 de 4): la captura de verificación se hace justo después de `SendInput`, antes de que el Bloc de notas dibuje el menú; el cambio sí aparece en la observación siguiente, unos 7 s después. El motor lo informó como `indeterminate`, sin reintentar, que es lo honesto; pero con esta aplicación `effect_observed` era inalcanzable (§6, hallazgo 3, corregido después de estas corridas).
- **Recibo de `SendInput`**: no es visible. Ni el resultado MCP ni stderr publican `submitted`/`inserted`; sólo se sabe que el motor lo encontró completo (si no, el mensaje habría sido "the OS did not accept every input event").
- No se ejercieron: un primer plano elevado, modificadores pulsados, un punto fuera de la ventana ni el foco de teclado de `type`.

### 3.4 IT-05, idempotencia por `request_id`

En C, después de cerrar el menú: repetir exactamente `s3-it03-close-2` devolvió el resultado guardado (mismo `indeterminate`, `input_delivered: true`) en 0,1 ms, sin capturas (`frame_seq` siguiente: 16 tras 15) y con `delivered_actions` en 3. La observación posterior mantuvo el menú cerrado (110 elementos/16 etiquetas OCR). El mismo `request_id` con `dry_run: true` → `rejected`, `REQUEST_CONFLICT`, `operator_review`. La caída ambigua (el proceso muere tras enviar) no se probó.

### 3.5 SEC-02, parada del operador

En B, tras una observación nueva (`s5`): el cliente creó el archivo de parada, esperó 0,5 s y pidió un clic real (`s3-sec02-1`) → `rejected`, `POLICY_DENIED`, `retryable: false`, `next_step: operator_review`, 0,4 ms; `delivered_actions` 0 → 0, `frame_seq` 7 → 8 sólo por la observación siguiente (ni captura de vigencia ni entrega). stderr mostró `STOP engaged by the operator (the stop file … exists): no new input will be sent; …` con una racha de espacios dentro de la línea (§6, hallazgo 2). Se retiró el archivo. No se probó Ctrl+C ni repetir bajo la parada el `request_id` de una entrega anterior.

### 3.6 SEC-05 / análogo de BDD-28: otra aplicación en primer plano

**No se ejecutó.** Tras observar con el Bloc de notas en primer plano, el cliente lanzó la Calculadora (`calc.exe`, fuera de la lista permitida; no estaba abierta antes). Windows no le dio el primer plano: ni en 15 s (B) ni en 30 s con 23 peticiones de activación (C). Como la Calculadora no llegó al primer plano, el cliente no envió la acción (habría caído sobre el Bloc de notas, que no era el caso de prueba) y cerró la Calculadora con `Stop-Process -Name CalculatorApp`. La parte de **vigencia** del precheck sí actuó de verdad: `s3-it03-close-1` fue `STALE_SNAPSHOT` sin enviar nada porque la pantalla cambió entre la observación y la acción.

### 3.7 IT-04 e IT-07

- **IT-04** (postcondición y plazo sin reintento): no se ejecutó como caso propio; provocar sin riesgo un clic que no cambie nada no era trivial. Los cuatro `indeterminate` de §3.3 son evidencia relacionada (sin reintento automático), pero de un efecto que sí ocurrió. Se ejecutó en la segunda corrida (§8.3).
- **IT-07** (DPI 125/150 %, cambio de epoch, fuera de perfil): no se ejecutó; la máquina se usó sólo a 100 % y con un monitor capturado.

### 3.8 Latencias (CPU, 1 hilo intra-op)

Tiempos por etapa del `_meta` (`io.ommatix/timings`) de las corridas A, B y C, en milisegundos; mediana (mín.–máx.). Las observaciones de una sesión no son independientes.

| Caso | n | capture | plan | detect | recognize | track | total |
|---|---|---|---|---|---|---|---|
| Primera observación (frame en blanco) | 3 | 7,3 | 0,02 | 7 369 | 24,3 | 0,03 | 7 401 (7 400–7 460) |
| Con regiones o refresco completo | 10 | 3,7 | 2,0 | 7 183 | 13,4 | 1,5 | 7 207 (7 150–7 588) |
| Sin regiones planificadas | 8 | 15,8 | 2,0 | 0 | 0 | 1,1 | 19,2 (6,9–29,1) |

Ida y vuelta de `execute_action` medida en el cliente: 18–33 ms con entrega (capturas de vigencia y verificación incluidas), 5,7 ms con `STALE_SNAPSHOT`, 0,2–0,4 ms con `POLICY_DENIED` y 0,1 ms para una repetición. Cada observación tras un clic cuesta una inferencia completa (unos 7,2 s), así que cada paso con menú tarda unos 8 s.

## 4. Recuento de acciones

Solicitudes `dry_run: false`: **8 de 10** permitidas (A: 1; B: 2; C: 5). Clics entregados al SO: **4**. Denegadas sin enviar nada: 3 (`POLICY_DENIED` ×2, `STALE_SNAPSHOT` ×1). Repetición sin entrega: 1. Además, sin entrada real: 1 `simulated` y 1 `REQUEST_CONFLICT`. Ninguna escritura, desplazamiento ni clic fuera de `Archivo`. El estado final del Bloc de notas es el inicial: menú cerrado.

## 5. Estado en la matriz

| Prueba | Estado nuevo | Justificación |
|---|---|---|
| IT-01 | `EJECUTADO_SUPERVISADO_PARCIAL` | Stride, epoch, `frame_seq`, rotación identidad y primer frame en blanco con captura real; sin pérdida de acceso ni salida rotada |
| IT-03 | `EJECUTADO_SUPERVISADO_PARCIAL` | Cuatro clics reales con aplicación atestiguada; recibo no visible; integridad, modificadores, punto fuera de ventana y foco de `type` sin ejercer |
| IT-05 | `EJECUTADO_SUPERVISADO_PARCIAL` | Repetición y conflicto de `request_id` en vivo; caída ambigua sin probar |
| SEC-01 | `EJECUTADO_SUPERVISADO` | Arranque rechazado sin consentimiento y `POLICY_DENIED` con `dry_run: false`; el contrato no ofrece ninguna vía para cambiar la política |
| SEC-02 | `EJECUTADO_SUPERVISADO_PARCIAL` | Parada por archivo a mitad de secuencia → `POLICY_DENIED` sin eventos; Ctrl+C y el informe de entregas previas bajo parada sin probar; ejecutada con el código de PR #8 |
| SEC-05 | `EJECUTADO_SUPERVISADO_PARCIAL` | Sólo la vigencia (`STALE_SNAPSHOT` real, sin envío); el cambio de primer plano no se pudo provocar |
| IT-04, IT-07 | sin cambio | No ejecutadas (§3.7) |

## 6. Hallazgos

1. **Corregido: `indeterminate` ocultaba una entrega.** En B el clic llegó al SO (el menú se abrió) y el resultado decía `input_delivered: false`, y `delivered_actions` seguía en 0, porque ambos se derivaban sólo del estado. El capítulo 07 §7.5 define `input_delivered` como "el actuador reportó entrega" y el SDD §2.9 separa entrega, cambio visual y postcondición. Ahora un resultado `indeterminate` informa si el actuador reportó la entrega completa (`true` cuando sólo falta verificar el efecto; `false` si el actuador falló o el SO no aceptó todos los eventos), y esas entregas cuentan en `delivered_actions`. `verified` sigue siendo `false`. El schema ya lo admitía; §7.5 lo aclara. Pruebas: `an_unverified_effect_still_reports_the_delivered_input`, `an_indeterminate_result_reports_whether_its_input_was_delivered` y BDD-29, que ahora exige `input_delivered: true`.
2. **Corregido: línea de parada con espacios.** Una continuación de línea perdida metía 18 espacios en el aviso `STOP engaged` de stderr. Prueba: `the_stop_notice_reads_as_one_sentence`.
3. **Corregido después de estas corridas (pendiente de una corrida nueva): la verificación visual llegaba demasiado pronto.** La única captura de verificación seguía inmediatamente a `SendInput`, así que una aplicación real todavía no había redibujado (4 de 4). Ahora el motor vuelve a capturar cada `[engine] verify_poll_ms` (50 ms) con el puerto `Clock` hasta ver el cambio o hasta que vence `verify_timeout_ms` (500 ms), sin reenviar la entrada (SDD §2.9, capítulo 07 §7.4, "plazo" de BDD-29; `CHANGELOG.md` 1.0.10). Pruebas: `visual_change_after_a_redraw_delay_is_observed`, `no_change_within_the_timeout_is_indeterminate`, `verification_never_resends_input`.
4. **Corregido después de estas corridas (pendiente de una corrida nueva): primer frame DXGI en blanco** (S0 §7), 6 de 6 sesiones. La primera actualización tras crear la duplicación no traía imagen presentada (`LastPresentTime == 0`) y su textura en blanco se entregaba como frame 1. Ahora el capturador la descarta y espera una con imagen dentro de los 500 ms de la primera captura, o falla. La prueba manual de un frame (sin guardar píxeles) falló con el código de estas corridas y pasa con la corrección. Prueba: `a_first_update_without_a_desktop_image_is_never_a_frame`.
5. **Abierto: el agente no ve el recibo de `SendInput`.** Los recuentos `submitted`/`inserted` no salen del motor; IT-03 pide contarlos.
6. **Limitaciones del entorno.** Un monitor capturado (salida 0) y DPI 100 % (IT-07 sin ejecutar); unos 7,2 s por inferencia, así que cada observación tras un cambio es lenta; el Bloc de notas restaura pestañas; el objetivo es un elemento `confirmed` de rol `unknown`, porque el detector de S0 tiene una sola clase y su affordance viene de su ficha (S0 §4); Windows no da el primer plano a ventanas lanzadas en segundo plano, lo que impidió SEC-05.

**Pendientes** (la segunda corrida, §8, resolvió los tres primeros: IT-03 con `effect_observed`, el primer frame con imagen e IT-04). Repetir IT-03 sobre `Archivo` con el código corregido para comprobar que un clic entregado pasa a `effect_observed` dentro del plazo y que la primera observación de una sesión `dxgi` ya ve el Bloc de notas; ejecutar IT-04 (un clic sin efecto que agote el plazo); el recibo de `SendInput` (hallazgo 5); SEC-05 con un cambio real de primer plano; IT-07.

## 7. Cómo repetirlo

Desde la raíz del repositorio, con el Bloc de notas maximizado en primer plano en el monitor principal y dos configuraciones locales como las de §1 (nunca en el repositorio):

```text
python tools\s3\mcp_driver.py --exe target\release\ommatix.exe --config <sin_actuacion>.toml --scenario dry --max-live 1
python tools\s3\mcp_driver.py --exe target\release\ommatix.exe --config <con_actuacion>.toml --scenario live --steps it03,it05,sec02,sec05 --max-live 8
```

El cliente termina, crea el archivo de parada y no envía nada si el Bloc de notas no está en primer plano y maximizado o si la etiqueta del menú no es única. Su salida puede incluir rutas locales (el archivo de parada) y nunca texto de la pantalla.

## 8. Segunda corrida (correcciones 1.0.10, DirectML)

**Fecha:** 14 de octubre de 2026, con la misma autorización, máquina, monitor y reglas que §1: sólo clics sobre una etiqueta de menú única y `confirmed`; ningún `type` ni `scroll`; el Bloc de notas en primer plano y maximizado en la salida 0, comprobado por el cliente antes de cada acción. Máximo de esta corrida: 8 solicitudes `dry_run: false`. Objetivo: comprobar en vivo las dos correcciones de 1.0.10 (plazo de verificación y primer frame DXGI) y ejecutar IT-04. No se guardó ningún frame ni ninguna etiqueta.

| Campo | Valor |
|---|---|
| Binario | `cargo build --release -p ommatix --features dxgi,sendinput,directml,winrt-ocr`, `rustc 1.99.0` |
| Código | Rama `feat/s3-supervised-runs` en `7ba14d3` (incluye 1.0.10 y 1.0.11), con el paso `it04` añadido a `tools/s3/mcp_driver.py` (commit de esta entrada); el motor no cambió |
| Detector | `icon_detect_v3` en DirectML (ONNX Runtime 1.24.4, `Microsoft.AI.DirectML` 1.15.4), como en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §9; OCR `winrt` es-ES |
| Configuración A (IT-01, IT-03) | `actuation_enabled = true`, `allowed_applications = ["notepad.exe"]`, `include_values = false`, `stop_file` en el directorio temporal de la sesión, `[models]` de `tools/s0/ommatix.s0.directml.toml` con rutas absolutas, `[engine] verify_timeout_ms = 500` y `verify_poll_ms = 50` (los valores por defecto). SHA-256 `026daa46f996c5da62d64846f1625d489c542a84c0b1bd0b5ce3a08a651d94ae` |
| Configuración B (IT-04) | Idéntica salvo `verify_timeout_ms = 0` (una sola comprobación inmediata). SHA-256 `9a963f3f0d37fe4e33de52c5e134b93583902e1ddb68f27f707bead9c3e9a32d` |

**Preparación.** El Bloc de notas ya estaba en primer plano y maximizado en la salida 0 (una de las ventanas vacías de la primera corrida). No hizo falta pedir la activación, no se lanzó ni se cerró ninguna ventana, y las tres etiquetas de menú fueron únicas en todas las observaciones. El archivo de parada no existía antes de ninguna sesión y no se activó.

### 8.1 IT-01: primer frame

- Prueba manual de un frame (`cargo test --release -p ommatix-adapter-capture --features dxgi -- --ignored`, sin guardar píxeles): pasa, con la comprobación de 1.0.10 de que el primer frame no es uniforme. 1920×1080, stride 7680 B, epoch 1, `DisplayMapping` con origen (0, 0) y tamaños iguales; 1 frame adquirido con 1 rectángulo sucio (en la primera corrida, 0) y ninguna pérdida de acceso.
- **La primera observación de cada sesión `serve` ya ve el Bloc de notas: 2 de 2 sesiones** (en la primera corrida, 0 de 6). Sesión A: 94 elementos, 15 etiquetas OCR y las tres etiquetas de menú; sesión B: 96 y 15. La segunda observación de cada sesión no tuvo regiones planificadas y dio los mismos recuentos: el primer frame ya era la pantalla y no hizo falta una observación de más.
- `geometry_epoch` 1 en las 10 observaciones; `frame_seq` crece de uno en uno por captura (§8.2 y §8.3 cuentan las capturas de cada acción).

### 8.2 IT-03: `effect_observed` dentro del plazo (configuración A, sesión `s3-run2-a`)

| `request_id` | Estado | `input_delivered` | `verified` / `observed_effect` | Ida y vuelta | `delivered_actions` | Evidencia posterior |
|---|---|---|---|---|---|---|
| `s3-it03-open-1` | `effect_observed` | true | true / `visual_change` | 74,9 ms | 0 → 1 | 94 elementos/15 OCR → 116/33: menú abierto |
| `s3-it03-close-1` | `effect_observed` | true | true / `visual_change` | 83,0 ms | 1 → 2 | 116/33 → 93/15: menú cerrado |

- **El hallazgo 3 queda confirmado en vivo: 2 de 2 clics con `effect_observed`** (en la primera corrida, 0 de 4). Cada acción entregada consumió tres capturas (`frame_seq` 3 → 7 y 7 → 11): la de vigencia, la verificación inmediata, que todavía no ve el menú, y una segunda tras esperar `verify_poll_ms` (50 ms), que sí lo ve. El Bloc de notas redibuja, pues, en menos de unos 50 ms, y la ventana de 500 ms sobra en este equipo.
- Ninguna `STALE_SNAPSHOT`: con DirectML la observación previa sin regiones cuesta unos 14 ms, y la pantalla no cambió antes de la captura de vigencia.

### 8.3 IT-04: plazo agotado sin reintento (configuración B, sesión `s3-run2-b`)

Dentro de las reglas no hay un clic sin efecto visual que pueda provocarse sin riesgo: el único clic permitido abre o cierra un menú, y las dos cosas cambian la pantalla. Por eso el operador acortó el plazo a 0 ms en su configuración: el motor hace entonces una sola comprobación inmediata, que llega antes del redibujado (primera corrida y §8.2). El caso comprueba el lado del motor: plazo agotado, informe honesto y ningún reintento.

| `request_id` | Estado | `input_delivered` | Error | Ida y vuelta | `delivered_actions` | Evidencia posterior |
|---|---|---|---|---|---|---|
| `s3-it04-open-1` | `indeterminate`, `verified: false`, `observed_effect: unknown` | true | `INDETERMINATE_EFFECT`, "input was delivered but no visual change was observed within the verification window; do not retry", `retryable: false`, `next_step: reconcile_effect` | 19,5 ms | 0 → **1** | 96/15 → 122/35: el menú se abrió después del plazo |
| `s3-it04-open-1` (repetición exacta) | el mismo resultado guardado | true | el mismo | 0,1 ms | 1 → **1** | Sin capturas: tras el frame 5, la observación siguiente es el frame 6 |
| `s3-it04-close-1` | `indeterminate`, mismo error | true | el mismo | 25,6 ms | 1 → 2 | 122/35 → 93/15: menú cerrado |

- El clic se entregó una sola vez: `delivered_actions` subió exactamente 1 y la entrega consumió dos capturas (`frame_seq` 3 → 5: vigencia y una verificación). El efecto ocurrió después del plazo, y el motor lo informó como incierto sin reintentar.
- La repetición del `request_id` devolvió el resultado guardado sin capturar ni entregar nada. `rejected_actions` siguió en 0 durante toda la sesión.
- El menú se cerró con un clic en una observación nueva, también `indeterminate` por el mismo plazo de 0 ms.

### 8.4 Latencias con DirectML

Tiempos por etapa del `_meta` (`io.ommatix/timings`) de las dos sesiones, en milisegundos; mediana (mín.–máx.). Las observaciones de una sesión no son independientes.

| Caso | n | capture | plan | detect | recognize | track | total |
|---|---|---|---|---|---|---|---|
| Primera observación (calentamiento de DirectML) | 2 | 7,8 | 0,05 | 262,2 | 34,0 | 0,09 | 304,2 (303,3–305,1) |
| Tras un clic, con una inferencia (menú abierto o cerrado) | 4 | 4,9 | 2,4 | 114,7 | 8,3 | 1,7 | 137,7 (127,5–143,3) |
| Sin regiones planificadas | 4 | 14,4 | 2,4 | 0 | 0 | 1,3 | 18,3 (13,6–27,2) |

Ida y vuelta de `execute_action` medida en el cliente: 75–83 ms con `effect_observed` (tres capturas y una espera de 50 ms), 19,5–25,6 ms con `indeterminate` y plazo 0, y 0,1 ms en la repetición. Cada paso con menú tarda ahora unos 0,2 s, frente a unos 8 s en CPU con un hilo (§3.8).

### 8.5 Recuento, estado y hallazgos

- Solicitudes `dry_run: false`: **5 de 8** permitidas (A: 2; B: 3). Clics entregados al SO: **4**, todos sobre `Archivo` en la ventana maximizada del Bloc de notas en primer plano. Repeticiones sin entrega: 1. Ningún rechazo, ninguna escritura, ningún desplazamiento y ningún clic en otro elemento. Estado final: menú cerrado, como al principio.
- **Matriz:** IT-04 pasa a `EJECUTADO_SUPERVISADO_PARCIAL`: con el actuador real, el plazo se agota, el resultado es `indeterminate` sin reintento y la repetición devuelve el resultado guardado. Es parcial porque sólo se comprueba la postcondición de cambio visual (las de foco, valor y desplazamiento no están implementadas) y porque el plazo se forzó con `verify_timeout_ms = 0`, no con una acción sin efecto. IT-01 e IT-03 siguen en `EJECUTADO_SUPERVISADO_PARCIAL`. IT-01 confirma el primer frame, pero sigue sin pérdida de acceso ni salida rotada. IT-03 confirma `effect_observed`, pero el recibo de `SendInput` sigue sin ser visible (hallazgo 5), y la integridad, los modificadores, el punto fuera de la ventana y el foco de `type` siguen sin ejercerse. Los requisitos RF/RNF, los escenarios y las casillas DoD no cambian.
- **Hallazgos:** ningún defecto nuevo. Los hallazgos 3 y 4 de §6 quedan confirmados en vivo; el 5 sigue abierto.
- **Pendientes:** el recibo de `SendInput`; SEC-05 con un cambio real de primer plano; IT-07; un clic sin efecto genuino para IT-04, cuando haya una aplicación de prueba controlada que lo permita.

Para repetirlo (la configuración B es la A con `verify_timeout_ms = 0`):

```text
python tools\s3\mcp_driver.py --exe target\release\ommatix.exe --config <A>.toml --scenario live --steps it03 --max-live 4 --observations 2
python tools\s3\mcp_driver.py --exe target\release\ommatix.exe --config <B>.toml --scenario live --steps it04 --max-live 5 --observations 2
```
