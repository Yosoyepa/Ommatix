# S3: corridas supervisadas con captura y entrada reales

**Fecha:** 14 de octubre de 2026. **Autorización:** el responsable del proyecto autorizó el 14 de octubre de 2026 corridas supervisadas en la máquina de referencia con captura real y entrada real (`SendInput`) **limitadas a `notepad.exe` con datos desechables**, con parada local por `stop_file`. **Alcance:** una aplicación, un monitor (salida 0), DPI 100 %, una máquina. Este informe registra lo que se ejecutó, con qué resultado y qué no se ejecutó; no cierra ningún requisito RF/RNF ni ninguna casilla DoD. Las secciones 1 a 8 son las corridas del 14 de octubre; la sección 9 (15 de octubre de 2026, 1.0.74) registra una ronda de regresión en vivo sobre el `main` de ese día, con SEC-02 ejecutada, y los estados de §5 son los de las corridas iniciales: el vigente está en `matrices/plan_pruebas.csv`.

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
- **IT-07** (DPI 125/150 %, cambio de epoch, fuera de perfil): no se ejecutó en estas corridas; la máquina se usó sólo a 100 % y con un monitor capturado. Se ejecutó después, el 15 de octubre de 2026 (1.0.73, [S4_SESIONES_PENDIENTES.md §C.1](S4_SESIONES_PENDIENTES.md)).

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

## 9. Ronda de regresión en vivo sobre el `main` actual (1.0.74)

**Fecha:** 15 de octubre de 2026, con el responsable presente en la máquina de referencia (las marcas de tiempo de los registros van de las 14:53 a las 15:02, hora local; DPI 100 %). **Autorización y reglas:** las de §1 y §8: `SendInput` real sólo sobre `notepad.exe`, maximizado en primer plano en la salida 0, **sólo clics sobre `Editar`** (`--menu-labels Editar,Edit`), ningún `type`, ningún `scroll`, parada local por `stop_file`. **Objetivo:** ejercer en vivo lo que las rondas de §3 y §8 no ejercieron y que desde entonces entró en el motor: la parada del operador a mitad de una secuencia, con repetición y entrada nueva (SEC-02, RF-16); el borrado bajo demanda (`[safety] purge_file`, [ADR-0028](../adr/0028-operator-purge-file.md)); la vigencia del snapshot de 10 000 ms ([ADR-0025](../adr/0025-bound-authorisations-by-snapshot-and-evidence-age.md)); el cambio de primer plano que hace el responsable a mano (SEC-05, RF-11); la denegación por defecto (SEC-01); y el plazo de verificación (IT-04) con el motor de hoy. Es la ronda que la decisión 6 de [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md) dejaba programada. Los pasos del cliente son los que `tools/s3/mcp_driver.py` ganó en la 1.0.71 (`sec02r`, `purge`, `ttl`, `sec05m`). No se guardó ningún frame ni ninguna etiqueta. Los registros crudos de la ronda (JSONL del cliente, sin etiquetas ni coordenadas) quedaron fuera del repositorio, y las cifras de abajo salen de ellos.

### 9.1 Entorno y configuraciones

| Campo | Valor |
|---|---|
| Binario | Compilado desde `6278b6f` (su código es idéntico al de `7a91354`: la PR #65 sólo cambió documentación) con `--features dxgi,sendinput,directml,winrt-ocr,uia`. SHA-256 `bda691245fb2d6315fa3b7fcb84f7068a8e0f4604bdf22715397e8850925429f` |
| Modelos y accesibilidad | Los de la configuración viva de S4: `icon_detect_v3` en float16 sobre DirectML (manifiesto `f8b04be9…`), OCR `winrt` es-ES y `[models] accessibility = "uia"` |
| Políticas comunes | `allowed_applications = ["notepad.exe"]`, `include_values = false`, `stop_file` y `purge_file` propios en una carpeta nueva fuera del repositorio, `verify_poll_ms = 50`. Ninguna configuración contiene secretos; los hashes sólo identifican el archivo usado |
| `dry.toml` | `actuation_enabled = false`. SHA-256 `e98760cec3b92d00c208538f12839ac1bc5bc79c8c4b9b62d3af87089de815a6` |
| `live_a.toml` | `actuation_enabled = true`, `verify_timeout_ms = 500`, `max_snapshot_age_ms = 10000`. SHA-256 `7d57bdbad9ef7de888eec5e911ee2f88ab7ee8124233bb5a7e586fb7e9bbab08` |
| `live_b.toml` | Como `live_a` con `max_snapshot_age_ms = 120000`, para que la espera del responsable en la Corrida 3 no venza el snapshot. SHA-256 `f56d79fb7ac34a7063fb14d62592a8350f7bda66e6dfe443b0be929a98428e23` |
| `live_c.toml` | Como `live_a` con `verify_timeout_ms = 0` (IT-04). SHA-256 `f62823db6039d2b9366754ba828b3b2a1d51b1dcbf66491dbc2058d8176e3310` |

Cada sesión imprimió su aviso de arranque con `values redacted`; las que tenían actuación, además, el aviso `ACTIVE CONTROL` con la aplicación permitida y el modo de parar. Cada corrida terminó con `ommatix: session ended; the engine's state and cache were deleted` (§9.5).

### 9.2 Corridas 0 y 0b: SEC-01 con `dry.toml`

| Solicitud | Resultado | Entrega |
|---|---|---|
| `s3-dry-1`, `dry_run: true` | `simulated`, 0,2 ms | ninguna |
| `s3-sec01-1`, `dry_run: false` | `rejected`, `POLICY_DENIED` ("live input is not allowed by the local policy or no live actuator is available"), `retryable: false`, `next_step: operator_review`, 0,2 ms | `input_delivered: false`; `delivered_actions` 0; `rejected_actions` 1 |

La Corrida 0b repite la 0 después de corregir el envoltorio (§9.3) con el mismo resultado; las dos salieron con código 0. La sesión anuncia `input=dry-run (never live)` y `actuation disabled`.

### 9.3 Incidencia de procedimiento: la primera Corrida 1 la abortó el envoltorio

La primera Corrida 1 (`live_a.toml`, pasos `sec02r,purge,ttl`) terminó a los pocos segundos de empezar su primer paso. **No fue el cliente ni el motor:** el envoltorio de PowerShell que lanzaba el cliente tenía `$ErrorActionPreference = 'Stop'` y convirtió la primera línea que el cliente escribió en stderr (`sec02r: this run created the operator stop file and will remove it itself`) en un error terminante, que mató la corrida. Hasta ese punto el registro muestra: la apertura `s3-sec02r-open-1` entregada con `effect_observed` (76,9 ms) y la parada creada por la corrida (`operator_stop`, `created_by_this_run: true`). **No se entregó nada más.** El archivo de parada quedó activo, que es el estado seguro; el **responsable cerró el menú y retiró el archivo de parada a mano**. El registro de esa corrida no tiene evento `end` ni las líneas de stderr del motor, así que no se puede afirmar cómo terminó su borrado de fin de sesión. El envoltorio se corrigió (`Start-Process` con la salida de errores redirigida a un archivo) y la Corrida 1 se repitió entera; la repetición es la que se puntúa (§9.4). Se registra como incidencia de procedimiento porque cuenta una entrega real y una parada que el operador tuvo que retirar. Lo que muestra de paso: el fallo del orquestador dejó el sistema en el estado cerrado (parada activa, ninguna entrada más), no en uno abierto.

### 9.4 Resultados por corrida

**Corrida 1 (`live_a`, `--steps sec02r,purge,ttl`, `--max-live 8`).** 16 comprobaciones `check`, las 16 en `ok`, código de salida 0. Seis solicitudes con `dry_run: false`: dos entregas, una repetición y tres rechazos.

| Paso | Qué ocurrió | Resultado medido |
|---|---|---|
| `sec02r` (SEC-02) | `s3-sec02r-open-1` abre el menú | `effect_observed`, `input_delivered: true`, 70,2 ms |
| | el cliente crea el archivo de parada y repite el `request_id` de la apertura | el resultado guardado (`effect_observed`, `input_delivered: true`) en 0,2 ms, sin captura ni entrega nueva |
| | solicitud nueva con la parada activa | `rejected`, `POLICY_DENIED` ("the operator stopped live input; nothing was delivered"), `retryable: false`, `next_step: operator_review`, 0,2 ms; sin captura (`frame_seq` 7 → 8 sólo por la observación siguiente) |
| | contadores | `delivered` +0, `rejected` +1 |
| | stderr del motor | `STOP engaged by the operator (…): no new input will be sent; …` y, al retirarlo el cliente, `stop cleared by the operator: [safety] governs live input again` |
| | cierre del menú, ya sin parada | `s3-sec02r-close-1` entregado, `effect_observed`, 93,8 ms |
| `purge` (ADR-0028) | el cliente crea `purge_file`; acción de prueba y acción viva sobre el snapshot anterior | las dos `rejected`, `STALE_SNAPSHOT` ("the action does not reference the published snapshot; observe again"), `observe_again`, 0,3 ms cada una, sin entrega ni captura |
| | observación siguiente | snapshot nuevo (`s8`), 0 identificadores reutilizados (`ids_fresh`) |
| | stderr del motor | `the engine's state and cache were deleted on the operator's request`, una sola vez; el cliente retiró su archivo |
| | contadores | `delivered` +0, `rejected` +2 |
| `ttl` (ADR-0025) | observación y acción de prueba dentro de la ventana | `simulated`, 0,2 ms |
| | espera de 12,0 s (`max_snapshot_age_ms` 10 000 ms más 2000 ms de margen), acción de prueba y acción viva | las dos `rejected`, `STALE_SNAPSHOT` ("the snapshot is older than [engine] max_snapshot_age_ms; observe again"), sin entrega |
| Final | contadores | `observations` 9, `delivered_actions` 2 (los dos clics reales), `rejected_actions` 5 |

**Corrida 2 (`live_a`, `--steps it03,it05`, `--max-live 4`).** Una apertura entregada y **dos cierres rechazados** (hallazgo de §9.6).

| `request_id` | Resultado | Ida y vuelta |
|---|---|---|
| `s3-it03-open-1` | `effect_observed`, `input_delivered: true` | 83,6 ms |
| `s3-it03-close-1` (observación de `frame_seq` 7) | `rejected`, `STALE_SNAPSHOT` ("the screen changed since the observation; nothing was delivered; observe again"), `input_delivered: false` | 5,3 ms |
| `s3-it03-close-2` (observación nueva, `frame_seq` 9) | lo mismo | 5,6 ms |
| repetición de `s3-it03-close-2` (IT-05) | el resultado guardado (`rejected`, `STALE_SNAPSHOT`), sin entrega | 0,2 ms |
| el mismo `request_id` con `dry_run: true` | `rejected`, `REQUEST_CONFLICT` ("this request_id was already used with other arguments"), `operator_review` | 0,1 ms |

Final: `delivered_actions` 1, `rejected_actions` 3. No se sabe si el menú `Editar` quedó abierto al terminar: la Corrida 2b empezó con una observación normal y el cliente no registra el estado del menú.

**Corrida 2b (`live_c`, `--steps it04`, `--max-live 5`, `verify_timeout_ms = 0`).**

| Solicitud | Resultado | Ida y vuelta | `delivered_actions` |
|---|---|---|---|
| apertura `s3-it04-open-1` | `indeterminate`, `input_delivered: true`, `verified: false`, `observed_effect: unknown`, `INDETERMINATE_EFFECT` ("input was delivered but no visual change was observed within the verification window; do not retry"), `retryable: false`, `next_step: reconcile_effect` | 24,6 ms | 0 → 1 |
| repetición exacta | el mismo resultado guardado, sin captura ni entrega | 0,2 ms | 1 → 1 |
| cierre `s3-it04-close-1` (observación nueva) | `indeterminate`, el mismo error | 34,5 ms | 1 → 2 |

Final: `delivered_actions` 2, `rejected_actions` 0.

**Corrida 3 (`live_b`, `--steps sec05m`, `--owner-wait-s 90`, `--max-live 1`; SEC-05).** El cliente observó con el Bloc de notas en primer plano y no envió nada hasta que **el responsable trajo la Calculadora al primer plano a mano**. La solicitud de clic salió unos 26 s después de la observación, dentro de la ventana de 120 000 ms de `live_b`, de modo que la causa del rechazo no es la vigencia.

| Solicitud | Resultado | Ida y vuelta |
|---|---|---|
| `s3-sec05m-1`, `dry_run: false` | `rejected`, `POLICY_DENIED` ("foreground application is not on the local allowlist"), `retryable: false`, `next_step: operator_review`, `input_delivered: false` | 0,7 ms |

Las comprobaciones `delivered_nothing`, `denied` y `counters` (`delivered` +0, `rejected` +1) pasaron. El responsable devolvió el Bloc de notas al primer plano (`notepad_back: true`). **El cliente no registra qué ejecutable vio en primer plano** (`calculatorapp.exe` o `applicationframehost.exe`; `--switch-to` acepta ambos como el cambio del responsable). Lo que se afirma es que el primer plano atestiguado por el actuador no estaba en la lista. Es la parte de cambio de primer plano que §3.6 no pudo provocar.

### 9.5 Recuento y estado

- Solicitudes con `dry_run: false` de la ronda: **17** (Corrida 0: 1; 0b: 1; Corrida 1 abortada: 1; Corrida 1: 6; Corrida 2: 4; 2b: 3; Corrida 3: 1). **Clics entregados al SO: 6**, todos sobre `Editar` en el Bloc de notas maximizado (1 en la Corrida 1 abortada, 2 en la 1, 1 en la 2 y 2 en la 2b). Repeticiones sin entrega: 3 (Corridas 1, 2 y 2b). Ninguna escritura, ningún desplazamiento y ningún clic fuera de `Editar`. Todos los rechazos fueron sin entrega: SEC-01 (2), SEC-02 (1), purga (2, una de ellas de prueba), vigencia (2, una de prueba), cierres obsoletos de la Corrida 2 (2), conflicto de `request_id` (1, de prueba) y SEC-05 (1).
- **Borrado al terminar la sesión (SEC-07), visto en vivo.** Las seis sesiones que terminaron con normalidad (Corridas 0, 0b, 1, 2, 2b y 3) imprimieron `session ended; the engine's state and cache were deleted`. Esa línea es el aviso del propio motor: la ronda no inspeccionó su memoria. Lo que sí se comprobó desde fuera es el efecto del borrado bajo demanda (`STALE_SNAPSHOT` sobre el snapshot anterior e identificadores no reutilizados).
- **Matriz.** **SEC-02 pasa a `EJECUTADO_SUPERVISADO`:** la parada por archivo a mitad de una secuencia en vivo deja pasar la repetición de una entrega anterior (resultado guardado, sin entrega nueva), rechaza la entrada nueva con `POLICY_DENIED` sin captura ni evento, los contadores no se mueven, el motor avisa de la parada y de su retirada, y la entrega posterior a retirarla funciona. **Límites declarados, que no bloquean ese estado:** el motor no tiene manejador de Ctrl+C (la parada en proceso, `StopSwitch`, se pierde con el proceso; desde 1.0.67 un `--session-id` fijo con actuación viva se rechaza al arrancar) y un cierre forzado no borra nada. Lo ejercido es la parada por archivo, no la parada en proceso. **SEC-05 sigue `EJECUTADO_SUPERVISADO_PARCIAL`:** ahora se vieron en vivo el cambio de primer plano (Corrida 3), la vigencia (Corrida 1, `ttl`) y la frescura de pantalla (Corrida 2), pero no un foco de teclado equivocado para `type` en el actuador real (el lado del actuador de BDD-28). **IT-03, IT-04 e IT-05 siguen `EJECUTADO_SUPERVISADO_PARCIAL`:** IT-04 (Corrida 2b) e IT-05 (repetición y conflicto, Corridas 1, 2 y 2b) se repiten con el motor de hoy; IT-03 suma un hallazgo (§9.6); la caída ambigua de IT-05, el recibo de `SendInput`, la integridad, los modificadores, el punto fuera de ventana y el foco de `type` siguen sin ejercerse. SEC-01 se ejerció dos veces más con el mismo resultado. La fila SEC-07 no se cambia en esta ronda.
- **Sobre la vigencia de 10 000 ms.** Se ejerció el lado vencido (12 s de espera: dos rechazos con el mensaje de la vigencia del snapshot) y el lado vigente (acciones dentro de la ventana). No se ejerció el borde exacto (eso es UT-46, con reloj inyectado), no se aisló la cota de la evidencia de accesibilidad de la del snapshot y no se midió la relectura de accesibilidad que calibra la ventana.

### 9.6 Hallazgo de IT-03: el cierre del menú se rechazó dos veces con `STALE_SNAPSHOT`

**Qué se vio.** En la Corrida 2 la apertura se entregó (`effect_observed`) y el cierre se rechazó **dos veces seguidas** con `STALE_SNAPSHOT` ("the screen changed since the observation; nothing was delivered; observe again"), cada vez con una observación nueva (`frame_seq` 7 y 9), con 5,3 y 5,6 ms de ida y vuelta y sin entrega. Difiere de §8.2, donde el cierre se entregó, y de otras dos partes de esta ronda, donde el cierre se entregó (Corrida 1, después del paso `sec02r`; Corrida 2b, con `verify_timeout_ms = 0`). **No es un fallo de seguridad:** es el comportamiento especificado y a prueba de fallos (no se entregó nada y el motor pidió observar de nuevo). Es un dato de utilidad: el flujo de dos clics de IT-03 no se completó en esa corrida.

**Qué hace el motor (lectura del código; no se ejecutó nada nuevo).** Antes de cualquier entrega viva, `fresh_frame` (`src/application/src/engine/act.rs`) vuelve a capturar la pantalla y la compara con el cuadro base confirmado mediante `dirty_tiles` (`src/domain/src/incremental.rs`). Si **un solo tile** es distinto (o hay un refresco completo por cambio de geometría), responde `STALE_SNAPSHOT` con ese mensaje y marca el snapshot como gastado. Tres propiedades de la comparación importan aquí. (1) Es **sobre el fotograma entero** (1920×1080 de la salida 0), no sobre la región del objetivo. (2) Es **exacta por píxel**: `tile_changed` compara los tres canales de color de cada píxel de cada tile de 64×64 (`[engine] tile_size`), sin tolerancia; sólo el alfa y el relleno de fila se ignoran. (3) El cuadro base es la **captura con que empezó la observación** (`captured_at` es el instante de esa captura), no la del final: entre ella y la captura de frescura pasa casi todo el tiempo de la observación, que el campo `age_ms` de la respuesta informa.

**Lo que dicen las cifras de la ronda** (`age_ms` de `observe_screen`, en ms; «ligera» es una observación sin regiones que analizar, y «pesada» la que analiza regiones cambiadas con detector, OCR y accesibilidad):

| Observación previa a una acción viva | `age_ms` | Tipo | Resultado de la acción |
|---|---|---|---|
| Aperturas (Corridas 1 abortada, 1, 2 y 2b; 4 de 4) | 1–2 | ligera | entregada |
| Cierre de la Corrida 1 (`frame_seq` 8) | 155 | pesada | entregada |
| Cierre de la Corrida 2b (`frame_seq` 6) | 121 | pesada | entregada (`indeterminate` por el plazo de 0 ms) |
| Cierre de la Corrida 2, intento 1 (`frame_seq` 7) | 138 | pesada | **`STALE_SNAPSHOT`** |
| Cierre de la Corrida 2, intento 2 (`frame_seq` 9) | 133 | pesada | **`STALE_SNAPSHOT`** |

La pantalla debe quedar idéntica entre la captura base y la de frescura: unos pocos ms con una observación ligera (la comprobación pasó 4 de 4) y de 120 a 155 ms con una pesada (pasó 2 de 4). Un cierre va siempre después de un cambio visual (el menú abierto), así que sigue a una observación pesada; una apertura, tras una observación ligera. Además, **la pantalla cambia sola**. En la Corrida 2, la observación de `frame_seq` 11 (pesada, 1,2 s después de la de 9, sin entrega ni captura entre ellas salvo la de frescura rechazada) volvió a analizar regiones, y lo mismo ocurrió en la Corrida 1 entre `frame_seq` 7 y 8 (1,7 s; lo único intermedio fue un rechazo sin captura). En las corridas sin actuación, de 3 pares de observaciones consecutivas separados por unos 125 ms, 2 analizaron regiones con detector y OCR con el menú cerrado y sin entrada (Corrida 0, `obs3`; Corrida 0b, `obs2`); los 9 pares a 15–30 ms (8 de las corridas vivas y 1 de la 0b) no analizaron ninguna.

**Hipótesis (no verificada).** Algo de la salida 0 cambia de forma periódica o intermitente, aunque sea un solo píxel; una observación pesada deja abierta una ventana de unos 140 ms en la que ese cambio hace fallar la comprobación exacta de frescura, y una ligera casi nunca la sufre. El candidato principal es el **cursor de texto parpadeante** del área de edición del Bloc de notas: en la décima ronda el cursor parpadeante del editor de prueba causó `STALE_SNAPSHOT` tras escribir y se corrigió en el editor ([`reviews/2026-10-14-s4-tenth-round.md`](../reviews/2026-10-14-s4-tenth-round.md), `CHANGELOG.md` 1.0.39). Otros candidatos: la animación o el resalte del menú abierto, y cualquier otro contenido de la salida 0 (el reloj de la barra de tareas cambia cada minuto). Con cuatro observaciones pesadas (dos pasaron y dos no) no se pueden separar esas causas ni descartar el azar, y la ronda no registró qué tiles diferían. La hipótesis es coherente con tres hechos: el motor compara la pantalla entera con exactitud, los rechazos aparecen sólo con ventanas largas y la pantalla cambia sola a escala de cientos de milisegundos.

**Seguimiento propuesto (ningún cambio de código en esta PR).**

1. *Cliente (`tools/s3/mcp_driver.py`, PR aparte):* antes de buscar el objetivo del cierre, una observación de reposo extra, para que la observación sobre la que se actúa sea ligera (como en §8.2, que usaba `--observations`). Un reintento más tras `STALE_SNAPSHOT` sólo ayuda si la observación de reintento es ligera. No cambia la seguridad: la frescura sigue siendo la del motor.
2. *Medición sin entrada, con el responsable presente:* con el menú cerrado y luego abierto, N pares de observaciones separados unos 150 ms con el escenario `observe`, contando los que analizan regiones; separa el parpadeo de la animación del menú.
3. *Diagnóstico del motor (PR aparte, con prueba):* al rechazar por frescura, contar (sólo números, nunca píxeles ni etiquetas) cuántos tiles difieren y si alguno toca la caja del objetivo, en stderr o en `_meta`. Distinguiría un parpadeo lejano de un cambio real bajo el objetivo.
4. *Decisión de diseño, no tomada aquí:* [02 §2.6](02_SDD_ARQUITECTURA_MVP.md) prevé validar «por región inalterada, tras demostrar su corrección» como mejora posterior. Si el parpadeo se confirma como causa, esa validación por región (con una prueba de que no deja pasar un cambio bajo el objetivo) es la salida de diseño. Hasta entonces el motor rechaza más de lo necesario, no menos.

Registro del hallazgo con las pruebas que cubren el rechazo: [`reviews/2026-10-15-live-round-close-refused.md`](../reviews/2026-10-15-live-round-close-refused.md).

### 9.7 Qué no cubre

Un monitor y DPI 100 %; una aplicación (Bloc de notas) y un solo elemento de menú; ningún `type` ni `scroll`; el foco de teclado de `type` en el actuador real, la caída ambigua de IT-05, el recibo de `SendInput`, la integridad, los modificadores y un punto fuera de ventana siguen sin ejercerse; ninguna medición de la relectura de accesibilidad que calibra la ventana de 10 000 ms; Ctrl+C no se probó (el motor no tiene manejador); el cierre forzado no se probó y no borra nada; el cliente no registra qué ejecutable atestiguó el actuador en la Corrida 3; no se sabe en qué estado quedó el menú tras la Corrida 2.

Para repetirlo (desde la raíz del repositorio, con el Bloc de notas maximizado en primer plano, las configuraciones de §9.1 y un envoltorio que **no** convierta stderr en error):

```text
python tools\s3\mcp_driver.py --exe <ommatix.exe> --config dry.toml --scenario dry --max-live 1 --menu-labels Editar,Edit
python tools\s3\mcp_driver.py --exe <ommatix.exe> --config live_a.toml --scenario live --steps sec02r,purge,ttl --max-live 8 --observations 2 --menu-labels Editar,Edit
python tools\s3\mcp_driver.py --exe <ommatix.exe> --config live_a.toml --scenario live --steps it03,it05 --max-live 4 --observations 2 --menu-labels Editar,Edit
python tools\s3\mcp_driver.py --exe <ommatix.exe> --config live_c.toml --scenario live --steps it04 --max-live 5 --observations 2 --menu-labels Editar,Edit
python tools\s3\mcp_driver.py --exe <ommatix.exe> --config live_b.toml --scenario live --steps sec05m --max-live 1 --observations 2 --owner-wait-s 90 --menu-labels Editar,Edit
```

## 10. Ronda de box 4: caso señuelo (BDD-35), caída ambigua (IT-05) y primer plano (15 de octubre de 2026)

**Fecha y autorización.** 15 de octubre de 2026, con el responsable presente en la máquina de referencia (DPI 100 %). El responsable autorizó entrada real para esta sesión y sólo para ella: clics sobre el campo «Campo auxiliar» de `ommatix-testeditor.exe` (BDD-35, con `--decoy-focus`) y clics sobre «Sumar» de `ommatix-testapp.exe` (la medición C0 y la caída de IT-05 en C1). El responsable hizo a mano todo lo que no es lectura: traer las ventanas al primer plano, minimizarlas y dar el foco. El libro de la ronda empezó en 0 y terminó en 17 (`ledger.txt`). No se guardó ningún fotograma, ninguna etiqueta ni texto de pantalla; los registros crudos quedaron fuera del repositorio.

**Objetivo y criterio.** Ejercer en vivo tres casos que el dossier dejaba sin ejecutar: BDD-35 (el foco de teclado de `type` tiene que quedar en Campo de destino tras el clic de un campo que lo cede), la caída ambigua de IT-05 (el motor muere a mitad de una petición) y SEC-05 con un cambio de primer plano. El criterio de BDD-35 es el de `decoy_problems` en `tools/s4/e2e_dossier.py`, fijado antes de las corridas: `indeterminate` con `INDETERMINATE_EFFECT`; clic de enfoque entregado y no verificado, con `reconcile_effect`; el clic cayó en Campo auxiliar por identidad; UI Automation sitúa el foco en Campo de destino; la repetición devuelve el resultado guardado sin entrega nueva; ninguna tecla en ningún campo; carpeta de la corrida vacía.

### 10.1 Entorno

| Campo | Valor |
|---|---|
| Motor | `ommatix.exe`, SHA-256 `3617b33d…`, compilado desde `main` a6b67ab con `--features dxgi,sendinput,directml,winrt-ocr,uia`. No cambió durante la ronda |
| Aplicación de prueba | `ommatix-testapp.exe`, SHA-256 `5860750e…`, botón «Sumar» (C0 y C1) |
| Editor de prueba | `ommatix-testeditor.exe` con `--decoy-focus`. Primera corrida D1: `bb51f195…`. Segunda: `af1e0924…`, recompilado desde `main` 65c94e6 con PR #70. Tercera: `be48e071…`, desde `main` 1d8d31d con PR #72. Las corridas cuarta a sexta no constan en este documento |
| Configuraciones | `decoy.toml` y `testapp.toml` son copias de `live_a.toml` con una sola línea cambiada (`allowed_applications`); `max_snapshot_age_ms = 10000`; `stop_file` y `purge_file` en una carpeta fuera del repositorio. SHA-256 `883f81b3…` y `8b64aaec…` |
| Modelos | Los de la configuración viva de S4: manifiesto `f8b04be9…`, ONNX Runtime `e7eedec6…`, DirectML `9c9e6d82…` |

### 10.2 Corridas

Orden por el libro. «Peticiones» son las que llevan `dry_run: false` y se gastan en el libro, la repetición incluida.

| Corrida (registro) | Libro | Resultado | Entrega y nota |
|---|---|---|---|
| D1 primera (`d1-first`) | 0 → 2 | `indeterminate`, `INDETERMINATE_EFFECT`; repetición con resultado guardado; UI Automation sitúa el foco fuera de la ventana (`elsewhere`) | Clic de enfoque entregado sobre Campo auxiliar, identidad correcta. No cumple: la ventana del editor no tenía el foco del sistema (`activation_requests` 0). Motivo corregido en PR #70 |
| C0 primera (`c0-stale1`) | 2 → 3 | `rejected`, `STALE_SNAPSHOT`, sin entrega | El registro calculó una medición de un clic que no se entregó. Corregido en PR #71 |
| C0 (`c0`) | 3 → 4 | `effect_observed`, valor 0 → 1 en «Sumar» | Clic entregado: 9,875 ms de entrega y 68,9 ms de ida y vuelta, con la ventana atestiguada. Cumple |
| C1 (`c1`) | 4 → 7 | Motor muerto a mitad de la petición (`crashed_mid_request`), entrega única (`delivered_once`, valor 0 → 1). Repetición tras el reinicio: `SESSION_MISMATCH`, sin entrega ni cambio del contador. Clic nuevo entregado (valor final 2) | Cumple (IT-05, caída ambigua). El motor muerto no escribió su fin de sesión; el reiniciado terminó con código 0. El registro, escrito antes de PR #71, contaba 2 peticiones y se gastaron 3 |
| D1 segunda (`d1-second`) | 7 → 9 | `rejected`, `STALE_SNAPSHOT`, sin entrega (el clic de enfoque no se entregó) | Tras la espera, UI Automation situó el foco en el campo principal (`editor`). No cumple |
| D1 tercera (`d1-third`) | 9 → 11 | `rejected`, `POLICY_DENIED` («foreground application is not on the local allowlist») | Sin entrega. El primer plano ya no era el editor (`outside`). Según el responsable, minimizó el editor con un segundo clic: BDD-28 apareció de forma no planificada y no es el caso planificado. Observación a acción: 4.610 ms. No cumple |
| D1 cuarta (`d1-fourth`) | 11 → 14 | Tres `STALE_SNAPSHOT` seguidos, cada uno con observación nueva | Sin entrega. Observación a acción: 4.156, 4.219 y 4.171 ms. El tercer rechazo dice que la evidencia de accesibilidad es más antigua que `max_snapshot_age_ms`. Primer uso del reintento acotado (PR #72). No cumple |
| D1 quinta (`d1-fifth`) | 14 → 14 | Parada antes de la primera petición: «no accessibility evidence for the decoy fields» | Sin libro gastado. Según el coordinador, la terminal de Orca, minimizada, dejó otra ventana delante; el registro no identifica esa ventana. No cumple (caso no ejecutado) |
| D1 sexta (`d1`) | 14 → 17 | Intento 1: `STALE_SNAPSHOT` (4.000 ms). Intento 2: `indeterminate`, `INDETERMINATE_EFFECT`, clic de enfoque entregado sobre Campo auxiliar con identidad correcta; repetición con resultado guardado | Sin teclas. UI Automation situó el foco en la ventana del editor (`in_editor`, `Window`), no en Campo de destino. Es el único criterio que falla. Observación a acción del intento 2: 3.937 ms. No cumple |

### 10.3 Recuento y lectura

- **Peticiones y entregas.** 17 peticiones con `dry_run: false` (1 + 1 + 3 + 2 + 2 + 2 + 3 + 0 + 3). Cinco clics entregados al sistema operativo: uno del caso señuelo en la primera D1, uno sobre «Sumar» en C0, dos en C1 (el del motor que murió y el nuevo) y uno del caso señuelo en la sexta D1. **Ninguna tecla**: los títulos leídos del caso señuelo marcan cero caracteres en los tres campos, y la carpeta de cada corrida que llegó a leerla quedó vacía.
- **Rechazos sin entrega.** `STALE_SNAPSHOT` en C0 primera, D1 segunda, D1 cuarta (tres veces) y D1 sexta (un intento); `POLICY_DENIED` en D1 tercera; `SESSION_MISMATCH` en la repetición tras el reinicio de C1.
- **BDD-35 en vivo: seis corridas y ninguna cumple el criterio.** Las cinco primeras no llegan a cumplirlo: foco fuera de la ventana (primera), rechazo por frescura (segunda y cuarta), primer plano perdido (tercera) y sin evidencia de accesibilidad (quinta). La sexta llega al clic de enfoque, entrega la identidad correcta, no envía teclas y la repetición devuelve lo guardado, pero el foco de UI Automation queda en la ventana.

### 10.4 Causa del foco en la ventana (análisis de código, sin corrida viva)

Las fuentes instaladas (egui 0.36.2, accesskit_consumer 0.35.0, accesskit_windows 0.32.1) explican la secuencia de la sexta corrida. (1) egui publica en cada pasada el foco de su memoria (`TreeUpdate.focus`, `egui/src/context.rs`), así que no hay retraso de un fotograma. (2) El campo del señuelo toma el foco al pulsar, y el redirect a Campo de destino ocurría en ese mismo fotograma, antes de que el clic terminara. (3) Al soltar, un clic fuera de un widget con foco se lo quita: el comportamiento por defecto es `SurrenderFocusOn::Clicks` (`context.rs`, líneas 1550 a 1565). Campo de destino perdió el foco, el árbol publicó la raíz y UI Automation leyó la ventana.

Lo reproduce una prueba headless de `testeditor` que falla antes del cambio (el foco de memoria queda en `None` tras la liberación) y pasa después. La corrección de PR #73 traslada la entrega del foco al momento en que el botón del señuelo está levantado. **Ninguna corrida viva la ha ejercido.** El registro de la sexta corrida no guardaba estos datos: la explicación es del código y de la prueba, no de la ronda.

### 10.4.1 Preparación del primer plano sin entrada (1.0.78, sin corrida viva)

Ocultar y volver a mostrar el editor hasta tres veces (`FocusSeeker`) no garantiza que Windows le dé el foco de teclado. Para `--app decoy` únicamente, el cliente llama ahora a [`AllowSetForegroundWindow`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow) con el PID que acaba de lanzar y registrar, y solicita [`SetForegroundWindow`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow) sólo para una ventana de ese proceso que no sea la auxiliar de winit. Reutiliza la verificación acotada de primer plano de `e2e_driver.py` con una lista explícita que contiene sólo esa llamada: no usa sus métodos antiguos con `AttachThreadInput`. Pide activación una vez incluso si la ventana ya está al frente, porque primer plano y foco de teclado son estados distintos. No usa el comando de foco del toolkit: winit 0.30.13 puede enviar Alt sintético por ese camino.

Los campos `foreground_granted` y `startup_focus_requested` del registro indican sólo la aceptación de las API, no que haya foco. Windows puede denegar la cesión si el cliente no tiene derechos de primer plano y puede retirar los derechos del hijo tras entrada del usuario. Si no llega el primer plano, `--operator-focus` conserva la espera acotada y de sólo lectura para que el responsable actúe a mano; si tampoco llega así, no se envía ninguna petición viva. Las lecturas de UI Automation y las guardas de cada petición siguen siendo la evidencia del foco real. El editor normal y el flujo de Explorer no activan esta ruta nueva.

Pruebas con dobles: `test_startup_grants_only_the_started_editor_pid`, `test_startup_foreground_request_uses_no_input_queue_attachment`, `test_startup_never_requests_another_process_or_a_helper_window`, `test_startup_activation_never_uses_the_drivers_attachment_methods`, `test_startup_requests_are_opt_in_for_the_decoy`, `test_startup_denied_rights_do_not_replace_the_real_focus_check`, `test_startup_foreground_denial_keeps_the_operator_focus_fallback` y `test_startup_os_denials_and_errors_are_best_effort`. No se abrió ninguna ventana, no se capturó pantalla y no se envió entrada. **Esta preparación no se ha ejercido en vivo y no garantiza resolver el foco de D1; ninguna casilla ni estado de validación cambia.**

### 10.5 Tiempos y primer plano

- **Tiempos.** Entre la observación y la petición (`observe_to_action_ms`) hubo 4.610 ms en la tercera corrida, 4.156, 4.219 y 4.171 ms en la cuarta, y 4.000 y 3.937 ms en la sexta. La comprobación de foco del precondicionamiento va antes de la observación, así que la distancia desde ella es mayor. El registro no la guardaba; desde PR #73 se registra como `since_focus_ms`. En ese intervalo el cliente lee la identidad y el anclaje con UI Automation, por PowerShell.
- **Tercera corrida.** El motor la rechazó en su puerta de política (`action_gate` 0,17 ms) y no ejecutó la comprobación de frescura (`action_freshness` 0,0 ms). El primer plano ya había salido del editor antes de que la petición llegara al motor, dentro de esos 4,6 s. El registro no dice qué lo sacó. Según el responsable, minimizó el editor con un segundo clic. La guarda de PR #73 lee el estado justo antes de cada petición y lo detectaría si ocurre antes de esa lectura. Un cambio en los milisegundos entre la guarda y la puerta del motor sigue llegando al motor, que lo rechaza sin entregar nada.
- **Quinta corrida.** Se paró antes de su primera petición sin evidencia de accesibilidad de los dos campos: no se envió nada y no se gastó libro.

### 10.6 Hallazgo de diseño: la frescura de pantalla completa

El motor compara el fotograma entero (1920×1080) con la captura base, con exactitud por píxel (§9.6). La frescura rechazó la acción en cuatro corridas: la primera C0, la segunda D1, la cuarta D1 (tres veces) y el primer intento de la sexta D1. Los candidatos a cambio que no es del objetivo son la terminal de Orca visible en la pantalla, el reloj de la barra de tareas y el marco de foco al cambiar de ventana. **Son hipótesis.** El registro no tiene datos por tile, así que no se atribuye ningún rechazo a un cambio concreto. Lo que sí se ve es el patrón: cuando algo se mueve en la pantalla, la comprobación completa falla aunque la acción sea correcta, y la ventana de observación a acción de unos 4 s es lo bastante ancha para que eso ocurra.

### 10.7 Propuesta de diseño: ADR de frescura por región (sin número, sin decidir)

La comprobación actual protege contra actuar sobre una pantalla que ya no es la observada, y eso es correcto. El problema es el alcance: cualquier píxel cuenta. [02 §2.6](02_SDD_ARQUITECTURA_MVP.md) prevé validar «por región inalterada, tras demostrar su corrección» como mejora posterior. Alternativas: (a) comparar sólo los tiles de la región del objetivo, con una prueba que demuestre que un cambio bajo el objetivo sigue rechazando la acción; (b) mantener la comparación completa y acortar la ventana de observación a acción, hoy de unos 4 s, sobre todo en lecturas de UI Automation por PowerShell; (c) en el cliente, una observación de reposo antes de actuar, que no cambia la seguridad. La recomendación de esta sección es medir (b) primero y decidir (a) después, con la medición de tiles de §9.6. **La decisión corresponde al responsable y al arquitecto**; aquí no se escribe ningún ADR.

### 10.8 Estado en la matriz y en §6.7

| Prueba | Estado | Justificación |
|---|---|---|
| IT-05 | `EJECUTADO_SUPERVISADO` (`plan_pruebas.csv`) | Repetición y conflicto de `request_id` en vivo (§9, 1.0.74); caída ambigua en C1 de esta ronda: una entrega, repetición rechazada con `SESSION_MISMATCH`, petición nueva correcta |
| BDD-35 (actuador real) | en parte; ningún caso cumplido | Seis corridas sin cumplir el criterio (§10.3) |
| BDD-28 (actuador real) | en parte | Visto de forma no planificada en la tercera corrida (`POLICY_DENIED` sin entrega); no es el caso planificado |
| SEC-05 | `EJECUTADO_SUPERVISADO_PARCIAL`, sin cambio | El cambio de primer plano apareció dos veces sin plan (tercera y quinta corridas); el caso con foco de `type` no se ha ejercido |

En [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md): la **casilla 4 queda sin marcar** (decisión del responsable: el criterio pre-registrado de BDD-35 no se cumplió). La **casilla 10** deja de listar IT-05 y sigue con IT-01 a IT-04. Por decisión del arquitecto, BDD-35 y el lado real de BDD-28 figuran en esa lista, dentro de SEC-05.

### 10.9 Qué no cubre esta ronda

- BDD-35 no cumplió: no hay evidencia viva de que UI Automation sitúe el foco en Campo de destino tras el clic del señuelo.
- La corrección de PR #73 y la guarda del cliente no se han ejercido en vivo.
- El registro no dice qué ejecutable estaba en primer plano en las corridas que lo perdieron, ni qué tile cambió en los rechazos por frescura.
- El motor no registra el intervalo entre su observación y la acción: sólo se conoce el del cliente.

### 10.10 Cómo repetirlo

Desde la raíz del repositorio, con las configuraciones de §10.1, `--round-start 0` y un envoltorio que **no** convierta stderr en error. `<run>` es la carpeta de la ronda, fuera del repositorio.

```text
python tools\s4\e2e_driver.py --exe <ommatix.exe> --config <run>\testapp.toml --app testapp --mode run --runs 1 --max-live 1 --ledger <run>\ledger.txt --round-start 0 --results <run>\c0.jsonl --measure-click --testapp-exe <ommatix-testapp.exe> --settle 1.5
python tools\s4\e2e_driver.py --exe <ommatix.exe> --config <run>\testapp.toml --app testapp --mode run --runs 1 --max-live 3 --ledger <run>\ledger.txt --round-start 0 --results <run>\c1.jsonl --ambiguous-crash --crash-after-from <run>\c0.jsonl --testapp-exe <ommatix-testapp.exe> --settle 1.5
python tools\s4\e2e_dossier.py --exe <ommatix.exe> --config <run>\decoy.toml --app decoy --mode run --runs 1 --max-live 4 --ledger <run>\ledger.txt --round-start 0 --results <run>\d1.jsonl --editor-exe <ommatix-testeditor.exe> --settle 1.5 --pause 1 --operator-focus
```

Con `--operator-focus` el responsable da el foco a mano, con un clic en el botón del editor en la barra de tareas y sin escribir. `--max-live 4` es el mínimo del caso señuelo: tres intentos y la repetición (PR #72). Si la ventana sale del primer plano, la guarda no envía nada y espera al responsable (tres veces por caso como máximo).
