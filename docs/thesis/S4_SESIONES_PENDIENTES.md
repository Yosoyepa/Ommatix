# S4: sesiones supervisadas pendientes de la Definition of Done

**Fecha:** 15 de octubre de 2026 (`CHANGELOG.md`, 1.0.47). **Alcance:** lo que el responsable debe poner (tiempo, ajustes, órdenes) para producir, cada una en una sola sesión, la evidencia que falta de REL-01, PERF-03 (RNF-06) e IT-07 (RNF-07). Este documento y las herramientas que describe fueron **preparación** (1.0.47): entonces no se ejecutó ninguna sesión. La sesión B se ejecutó (1.0.55, 1.0.66 y 1.0.89) y la sesión C se ejecutó el 15 de octubre de 2026 (1.0.73, §C.1). La sesión A sigue sin ejecutar.

**Alcance del MVP (15 de octubre de 2026):** la sesión A (REL-01, instalación y reversión) y la sesión B (PERF-03, soak de 2 h) siguen en el MVP; la sesión C (IT-07, DPI) pasó al próximo corte, como EV-01, EV-02 y el corpus de REL-01 (capítulo 06 §6.7, [ADR-0023](../adr/0023-licence-and-reduced-mvp-scope.md)), y el responsable la adelantó y la ejecutó el 15 de octubre de 2026 (§C.1).

Ninguna de las tres sesiones envía entrada (ni de teclado, ni de ratón) ni usa `execute_action`. Las herramientas de `tools/perf/` observan, y están escritas para negarse a empezar si la configuración del operador pudiera entregar entrada ([`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md) §5.1).

**Estado (1.0.66):** la sesión B se ejecutó dos veces, supervisada y sin entrada. La primera, sobre `fe35d80` y sin accesibilidad, fue completa (§12; veredicto del umbral en 1.0.62). La segunda, sobre `745caff` con `uia` y la pantalla encendida, también fue completa (§15): con los criterios del [capítulo 09 §9.7](09_EVALUACION_EXPERIMENTAL.md), el conjunto de trabajo cumple (+1,25 MiB/h) y los manejadores **no cumplen** (pendiente positiva: un manejador más en el minuto 69). La cola no tiene veredicto porque no tiene cota (decisión 9 de [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)). Un intento anterior de repetirla con `uia` quedó bloqueado antes de la corrida (§14). Seguía sin ejecutar la sesión A (REL-01). **Estado (1.0.73):** la sesión C (IT-07) se ejecutó el 15 de octubre de 2026 a 100, 125 y 150 %, con el cambio de escala en vivo y una escala fuera de perfil (§C.1); la sesión A sigue sin ejecutar. **Estado (1.0.89):** la sesión B se ejecutó otra vez el 17 de octubre de 2026 sobre `1c46b87` con `uia` (§B y [S0 §18](S0_RESULTADOS.md)); M1 se ejecutó el 17 de octubre de 2026 (§D.1.1); M2 e IT-08 siguen sin ejecutar.

## 0. Condiciones comunes

- **Una sola cosa a la vez.** No debe haber una serie de E2E en marcha ni otro `ommatix.exe`: las herramientas se niegan a arrancar si ven `ommatix.exe` u `ommatix-testapp.exe` ya en ejecución, para que los números sean del motor solo.
- **Manos fuera durante B y durante cada observación de C.** El motor sólo observa mientras la aplicación de prueba está en primer plano (lo que muestre otra ventana no está autorizado), y la herramienta nunca la trae al frente: no usa entrada. Cualquier clic, tecla o ventana nueva que le quite el primer plano se cuenta como una pérdida (`foreground_lost`) y tres seguidas terminan la corrida.
- **Pantalla.** Un solo monitor capturado (la salida 0, la principal); sesión de Windows desbloqueada y sin suspensión durante toda la sesión; bloquear el equipo hace perder el acceso DXGI. Cierre lo que no haga falta.
- **Compilación.** Desde la raíz del repositorio, con Rust 1.99.0 y el árbol limpio (el informe registra el commit y si lo estaba):

  ```powershell
  cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia
  cd tools\s4\testapp
  cargo build --release --locked
  cd ..\..\..
  ```

- **Configuración del operador.** `%LOCALAPPDATA%\Ommatix\ommatix.rnf05.directml.toml` (detector fp16 en DirectML, el que se midió en RNF-05) con sus archivos fijados, según [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md) §4. Compruébelo antes con `python tools/repro/environment.py --operator-dir "$env:LOCALAPPDATA\Ommatix" --out environment.json`: sin `mismatch`.
- **Qué se guarda.** Estados, códigos, recuentos, tiempos y tamaños en JSON. Ningún fotograma, ninguna etiqueta de pantalla, ninguna coordenada (ADR-0012). Las herramientas escriben sólo donde se les da `--out`, en un directorio **fuera del repositorio** (los informes de mediciones se archivan con el commit y la máquina, no se versionan en bruto).
- **Después.** Las cifras van a [`S0_RESULTADOS.md`](S0_RESULTADOS.md) con la máquina y el commit, y sólo entonces el responsable decide el estado de la fila de la matriz (`EJECUTADO_SUPERVISADO` o `EJECUTADO_SUPERVISADO_PARCIAL`). Capítulo 09 §9.7: los umbrales se fijan **antes** de medir y un resultado negativo no se reemplaza por un umbral retrospectivo.

## A. REL-01: instalación y reversión en una máquina limpia

| | |
|---|---|
| **Tiempo** | Sin medir. Dos descargas grandes (el paquete NuGet de DirectML pesa unos 200 MB) y una compilación completa de Rust; reserve media jornada la primera vez. |
| **Necesita del responsable** | Una máquina Windows 11 x64 sin el repositorio ni `%LOCALAPPDATA%\Ommatix`; el commit de entrega y su etiqueta (RNF-08); decidir la licencia de `DirectML.dll`; el visto bueno para borrar al final. |
| **Ajustes** | Ninguno de pantalla. |

Pasos: seguir [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md) de §1 a §6 tal como está escrita, sin atajos, anotando cada discrepancia. Después, §7 con un cliente MCP (el propio de `tools/s3/mcp_driver.py` basta para la conexión; uno de terceros amplía la evidencia). Luego §8: volver a un commit anterior con su binario y su manifiesto, verificar (§6) y retirar todo (§8.3, §8.4).

**Se registra:** `environment.json` de la instalación y otro tras la reversión; el commit; cada paso que no salió como la guía dice (la guía se corrige en el mismo PR); la salida de §8.4.
**No se cierra aquí:** el corpus de aceptación congelado ni el commit de entrega (son suyos); el estado de REL-01 lo decide el responsable con ese registro.

## B. PERF-03 / RNF-06: recursos y soak de 2 h

**Ejecutada el 17 de octubre de 2026 sobre `1c46b87` con `uia`** ([S0 §18](S0_RESULTADOS.md)), con `--max-pending-regions 510` y `--foreground-wait 300`. Cumplen el conjunto de trabajo (+1,39 MiB/h) y la cola (máximo 0); los manejadores no cumplen en sentido estricto con el criterio del 15 de octubre (un paso de 998 a 999 en el minuto 13). Con el criterio de manejadores fijado el 17 de octubre de 2026 (decisión 10 de [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)) cumplen también: un paso de +1 y el recuento constante durante la última hora ([S0 §18](S0_RESULTADOS.md), relectura). Hubo dos monitores de 1920 × 1080, no uno: la desviación queda registrada en S0 §18.

| | |
|---|---|
| **Tiempo** | 125 minutos ininterrumpidos (5 de preparación y 120 de corrida), más 5 minutos antes para el ensayo corto. |
| **Necesita del responsable** | Dejar el equipo solo; sin suspensión ni bloqueo de pantalla durante 2 h. Los criterios de conjunto de trabajo (no más de 10 MiB/h) y de manejadores (un único cambio de +1 tras el calentamiento y el recuento constante durante la última hora) ya están fijados ([capítulo 09 §9.7](09_EVALUACION_EXPERIMENTAL.md)). La cota de la cola de regiones pendientes está fijada desde el 17 de octubre de 2026 (decisión 9 de [06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)): 510 teselas en 1920 × 1080. |
| **Ajustes** | Plan de energía «Alto rendimiento» (el de S0); un solo monitor; 100 % de escala (como en RNF-05); notificaciones desactivadas. |

1. Ensayo corto, con la corrida real entera por delante: confirma que la aplicación de prueba toma el primer plano sola, que el contador de GPU responde y que el informe se escribe (la herramienta se negará con código 2 si no puede mantener el primer plano sin entrada).

   ```powershell
   python tools/perf/soak.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.rnf05.directml.toml" --minutes 1 --interval 2 --out C:\ruta\fuera\soak-ensayo.json
   ```

2. La corrida de 2 h (por defecto `--minutes 120` y una muestra cada 5 s; los criterios son los de 15 y 17 de octubre de 2026):

   ```powershell
   python tools/perf/soak.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.rnf05.directml.toml" --minutes 120 --out C:\ruta\fuera\soak-120.json
   ```

   Los criterios son los valores por defecto: `--max-rss-growth-mib-per-hour 10` para el conjunto de trabajo, y la regla de manejadores del 17 de octubre de 2026, que no tiene opción. `--max-handle-growth-per-hour` sólo añade el criterio de no crecimiento del 15 de octubre si se pasa (con `0`), y no tiene valor por defecto. Con la cota de la cola ya fijada (decisión 9), para el monitor del MVP (1920 × 1080) se añade `--max-pending-regions 510`; sin esa opción la herramienta no juzga la cola. Cambiar un criterio sólo por decisión del responsable, y se anota en el informe.

**Qué hace** (sin entrada, sin `execute_action`): lanza sólo la aplicación de prueba con `--animate` (dos etiquetas que cambian cada 100 ms, para que haya regiones incrementales continuas) y `ommatix serve --source dxgi` con la configuración, y cada 5 s observa y muestrea. Termina sólo sus procesos.

**Se registra** (todo en `soak-120.json`):

- `samples`: por muestra, el instante, el conjunto de trabajo, los bytes privados y los manejadores de `ommatix.exe`, la memoria dedicada de GPU si el contador de rendimiento de Windows la da (si no, `gpu_counter.reason`), `pending_regions`, `observations`, `rejected_actions` y `delivered_actions` de `get_engine_metrics`, y la latencia de `observe_screen` (ida y vuelta y total del motor, de `_meta`).
- `resources`: de cada recurso, mínimo, máximo, mediana del primer y del último décimo tras el calentamiento y la pendiente por hora; `pending_regions`: máximo, último y número de muestras en cero (cola acotada); `latency_ms`: p50/p95/p99/máximo global y por ventana de 10 minutos (cómo cambia la cola con el tiempo).
- `threads` y `handle_steps`: el número de hilos de `ommatix.exe` en cada muestra, con el mismo resumen que los manejadores, y cada cambio del número de manejadores tras el calentamiento con los hilos antes y después; es un diagnóstico, y la regla de manejadores lo juzga en `criteria.handle_count`.
- `counts` (ticks, observaciones, pérdidas de primer plano, errores), `ended` (`completed` es la única terminación completa), `criteria` (cada criterio con su valor medido y si se cumple: conjunto de trabajo y `handle_count` siempre, este último con los pasos, sus deltas, el tiempo plano final y el motivo si no se cumple; la pendiente de manejadores sólo si se pasa su opción; la cola sólo si se da su cota), el commit, si el árbol estaba limpio, y los SHA-256 del ejecutable y de la configuración.

Qué decide: RNF-06 pide «RSS/VRAM totales, cola acotada y sin crecimiento sostenido en soak de 2 h». La herramienta entrega la serie, las pendientes y, para cada criterio fijado, si se cumple; no declara por sí sola «sin crecimiento sostenido» para RNF-06. Esa lectura es del responsable, con los criterios que fijó: la memoria de trabajo y los manejadores cumplen en la corrida de §12, y la repetición de §18 se juzgó con los de 15 de octubre y se relee con el de 17 de octubre (decisión 10); la cola necesita su cota y una corrida en la que alguna observación falle, porque con observaciones correctas vale cero por construcción; la memoria de GPU se publica sin criterio. Una corrida que termina con `foreground_lost` o `observe_errors` no es un soak de 2 h: se anota como corrida truncada con su motivo y se repite.

## C. IT-07 / RNF-07: DPI 100, 125 y 150 %

| | |
|---|---|
| **Tiempo** | Unos 10 minutos por escala (3), unos 10 minutos más para el cambio de escala en vivo, y 10 si quiere el caso fuera de perfil. Unos 60 minutos en total. |
| **Necesita del responsable** | Cambiar la escala **a mano** entre sesiones (Configuración > Sistema > Pantalla > Escala); la herramienta nunca la cambia. Anotar la escala que tenía para devolverla al terminar. |
| **Ajustes** | Un monitor capturado: el principal, con origen (0, 0); resolución sin cambiar; el cambio de escala puede pedir cerrar sesión en algunas aplicaciones, pero la herramienta lanza procesos nuevos en cada sesión. |

Para cada escala de 100, 125 y 150 %: póngala, deje que Windows la aplique y ejecute

```powershell
python tools/perf/dpi_probe.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.s0.directml.toml" --out C:\ruta\fuera\dpi-probe-<escala>.json
```

Cada sesión: lee la escala del monitor principal (con el proceso consciente de DPI por monitor, como el motor), lanza sólo la aplicación de prueba, ejecuta la prueba manual de captura de un fotograma (`dxgi_captures_one_frame_of_output_0`, un fotograma en memoria que no se guarda; necesita las dependencias del crate de captura ya compiladas, o `--skip-capture-test`), y hace 5 observaciones de sólo lectura.

**Criterios que comprueba** (el dossier fija el perfil —100/125/150 %, epoch, fuera de perfil— y no cifras):

| Criterio | Cómo se lee |
|---|---|
| `scaling_in_profile` | la escala leída es 100, 125 o 150 |
| `frame_matches_monitor`, `mapping_is_identity` | el fotograma tiene el tamaño físico del monitor y el `DisplayMapping` del motor es la identidad (fotogramas y entrada comparten píxeles físicos) |
| `epoch_stable_without_change` | `geometry_epoch` no se mueve mientras nada cambia |
| `observations_succeeded`, `buttons_found` | las 5 observaciones salieron bien y aparecen los 6 botones fijos de la aplicación de prueba (`Sumar`, `Restar`, `Reiniciar`, `Activar`, `Siguiente`, `Anterior`) |
| `epoch_follows_scaling` | (sólo en la sesión de cambio en vivo) el epoch sube tras cada cambio de escala, no baja y no sube sin cambio |
| `out_of_profile` | `not_applicable` dentro del perfil; fuera de él, `flagged` sólo si toda observación se rechazó con `UNSUPPORTED_GEOMETRY` (otro rechazo es `refused_with_another_code`) |

**Cambio de epoch en vivo.** Una vez, con la escala en 100 %:

```powershell
python tools/perf/dpi_probe.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.s0.directml.toml" --skip-capture-test --watch-minutes 6 --out C:\ruta\fuera\dpi-probe-cambio.json
```

Mientras corre, el responsable cambia la escala a 125 % a mano y vuelve a poner la aplicación de prueba al frente (Alt+Tab o su botón de la barra); la herramienta sólo observa cuando está al frente y registra la serie (escala, epoch). Es la única parte de la sesión C con entrada: la suya, no la de la herramienta.

**Fuera de perfil (opcional, una vez).** Ponga una escala personalizada que no sea 100, 125 ni 150 (por ejemplo 175 %) y ejecute la misma orden. **Estado del motor (1.0.70, [ADR-0031](../adr/0031-refuse-scaling-outside-the-dpi-profile.md)):** rechaza una escala conocida fuera de 100, 125 y 150 % con `UNSUPPORTED_GEOMETRY`, así que lo esperable es `flagged`. Una pantalla rotada y un formato que no sea BGRA8 siguen como `CAPTURE_UNAVAILABLE`. Si la herramienta registra `served_unflagged`, eso sería un hallazgo contra RNF-07 («rechazo explícito fuera de perfil»), no un fallo de la herramienta.

Al terminar: `python tools/perf/dpi_probe.py --combine dpi-probe-100.json dpi-probe-125.json dpi-probe-150.json dpi-probe-cambio.json --out dpi-probe-resumen.json` indica qué escalas del perfil tienen una sesión completa y si se comprobó una transición de epoch. Devuelva la escala a la que tenía.

**Se registra** (por sesión): la escala y el DPI, el tamaño y el origen del monitor principal, la forma del fotograma (ancho, alto, paso, epoch y `DisplayMapping`), por observación el epoch, el recuento de elementos y **cuántos elementos llevan cada una de las 7 etiquetas fijas de la aplicación de prueba** (ninguna otra etiqueta), y los criterios. No hay fotogramas ni coordenadas.

### C.1 Resultado de la sesión del 2026-10-15

PC de referencia, monitor principal 1920×1080 con origen (0, 0), motor con las características `dxgi,directml,winrt-ocr` y la configuración del operador (sin entrada: la herramienta sólo observa; el responsable cambió la escala a mano). Las salidas JSON quedan fuera del repositorio (ADR-0012). Los cinco informes se resumieron con `--combine`.

| Sesión | Commit | DPI | Fotograma y `DisplayMapping` | Observaciones | 6 botones | Resultado |
|---|---|---|---|---|---|---|
| 100 % | `6278b6f` | 96 | 1920×1080, paso 7680, epoch 1, identidad | 5/5 | sí | `met_all` |
| 125 % | `6278b6f` | 120 | 1920×1080, paso 7680, epoch 1, identidad | 5/5 | sí | `met_all` |
| 150 % | `69d8aeb` (`main` más el ajuste de la aplicación de prueba, PR #63) | 144 | 1920×1080, paso 7680, epoch 1, identidad | 5/5 | sí | `met_all` |
| Cambio en vivo 100 → 125 % (`--watch-minutes 6`) | `6278b6f` | 96 → 120 | (sin prueba de captura) | 5/5, más una serie de 178 entradas | sí | sí (`epoch_follows_scaling` cumple) |
| 175 % personalizada (fuera de perfil) | `6278b6f` | 168 | prueba de captura correcta | 0/5: las 5 rechazadas con `UNSUPPORTED_GEOMETRY` | no aplica | `out_of_profile`: `flagged` |

**Resumen combinado.** Las escalas del perfil 100, 125 y 150 % tienen una sesión completa, no falta ninguna, la transición de epoch se comprobó, y el resultado fuera de perfil es `flagged`.

**Cambio en vivo.** El epoch es 1 a 100 % y pasa a 2 en la primera observación tras el cambio a 125 %, con 76 elementos antes y después. Hubo 3 muestras omitidas mientras la aplicación de prueba no estaba al frente. `epoch_follows_scaling`: 1 cambio respondido, 0 sin respuesta, 0 bajadas, 0 subidas sin explicar.

**Fuera de perfil.** A 175 % (168 dpi) las cinco observaciones se rechazaron con `UNSUPPORTED_GEOMETRY` y la herramienta lo registró como `flagged`: el comportamiento de [ADR-0031](../adr/0031-refuse-scaling-outside-the-dpi-profile.md) queda confirmado en vivo. `served_unflagged`, que habría sido un hallazgo contra RNF-07, no ocurrió.

**Hallazgo y corrección (aplicación de prueba, [PR #63](https://github.com/Yosoyepa/Ommatix/pull/63)).** La primera sesión a 150 % con la aplicación de prueba anterior falló `buttons_found` en 10 de 10 observaciones: «Reiniciar» quedaba cortado al pie de la ventana (el responsable lo confirmó a la vista). No es un fallo del motor: la aplicación de prueba ahora encoge sus separaciones en una ventana corta y no cambia la disposición a 100 y 125 %. Las lecturas hechas con la aplicación anterior (100 y 125 %, también `met_all`) no cuentan para esta tabla.

**Procedimiento.** Dos intentos a 125 % terminaron `refused` porque la aplicación de prueba se abrió detrás de otras ventanas y la herramienta nunca usa entrada para traerla al frente. El tercero, con `--foreground-wait 180` y la aplicación traída al frente por el responsable, se completó.

**Qué no cubre.** Un solo monitor, la aplicación de prueba propia, ninguna acción en vivo a 125 o 150 % y un solo tema. La fila IT-07 de `plan_pruebas.csv` y el estado de RNF-07 no cambian: los decide el responsable.

## D. Mediciones de frescura M1 y M2 / IT-08 (ADR-0032): M1 ejecutada; M1 con los dos grupos y M2 / IT-08 ejecutadas de forma descriptiva el 17 de octubre de 2026 (§D.1.3 y §D.2.1); pendientes: IT-03 con `target_window`, un objetivo no maximizado con opacidad atestiguada, y un D0

**Preparación de herramientas (1.0.47).** M1 se ejecutó el 17 de octubre de 2026 (§D.1.1), sin entrada ni actuación. Al escribirse esto (1.0.47), M2 e IT-08 no se habían ejecutado; se ejecutaron de forma descriptiva el 17 de octubre de 2026 (§D.2.1).
El responsable registra antes de cada sesión el commit, configuración, N, aplicaciones,
estado del menú y protocolo descriptivo (M1) o criterio de aceptación (M2), fuera del
repositorio. IT-08 estaba planificado entonces;
ningún estado de requisito ni casilla de §6.7 cambia. El valor por defecto sigue siendo
`action_freshness = "screen"`.

### D.1 M1, sólo lectura y sin motor ni entrada (ejecutada el 17 de octubre de 2026: §D.1.1)

Compilar `cargo build --release -p ommatix --features sendinput`. Esa característica permite
reutilizar las consultas Win32 del adaptador; el subcomando construye **únicamente**
`WindowReader`, que declara `is_live = false`, rechaza toda entrega y no construye el
actuador vivo. No inicia MCP, detector, OCR ni accesibilidad. La configuración de M1 debe
tener `[engine] mode = "dry_run"`, `[safety] actuation_enabled = false`, la aplicación en
`allowed_applications` y su clase exacta en `allowed_window_classes`. Se respetan el stop
y el anclaje existentes; la herramienta no crea, borra ni modifica ninguno de ellos.

El responsable pone al frente la ventana **a mano** y selecciona una caja objetivo que esté
completamente dentro de ella. Las cuatro magnitudes de la caja son píxeles físicos locales
del monitor seleccionado (`--output`, cero por defecto): sólo son argumentos de una
herramienta del operador, nunca campos MCP ni datos de salida. No se leen títulos ni texto.
Para Bloc de notas se usa exclusivamente la caja de «Editar», sin pulsarlo mediante la
herramienta. **Decisión del responsable: M1 es descriptiva, con N = 60 pares en total,
30 con el menú cerrado y 30 con el menú abierto, sin umbral de aprobación.** Cambia el
menú a mano entre las dos invocaciones; cualquier otra ventana se registra por separado.

```powershell
# Asignar previamente x, y, width y height en el cuaderno de sesión.
# La carpeta de salida ya debe existir fuera de cualquier repositorio.
python tools/perf/freshness_probe.py --exe target\release\ommatix.exe `
  --config "$env:LOCALAPPDATA\Ommatix\m1.toml" --pairs 30 --interval-ms 150 `
  --descriptive --window-class Notepad `
  --target-x $x --target-y $y --target-width $width --target-height $height `
  --out "$env:LOCALAPPDATA\Ommatix\runs\m1-menu-cerrado.json"

# El responsable abre el menú a mano y verifica de nuevo la caja antes de continuar.
python tools/perf/freshness_probe.py --exe target\release\ommatix.exe `
  --config "$env:LOCALAPPDATA\Ommatix\m1.toml" --pairs 30 --interval-ms 150 `
  --descriptive --window-class Notepad `
  --target-x $x --target-y $y --target-width $width --target-height $height `
  --out "$env:LOCALAPPDATA\Ommatix\runs\m1-menu-abierto.json"
```

`--descriptive` registra los recuentos y `outside_pairs` (pares con
`total_tiles > group_tiles`), **sin** `min_outside_pairs` ni `criterion_met` en la salida.
No convierte la medición en aprobada o rechazada, ni decide activar el alcance o acreditar
IT-08. Para otro protocolo futuro puede elegirse `--min-outside-pairs`, mínimo de pares
con cambios fuera del grupo entre cero y N: no tiene valor por defecto y es incompatible
con `--descriptive`. El responsable no eligió ese criterio de aprobación para esta M1.
N tampoco tiene valor por defecto en la herramienta: se pasa 30 en cada invocación del
plan de 60. Ningún recuento es una tolerancia del motor o autorización de entrada.

La salida contiene sólo números y booleanos. Por par: `total_tiles`, `group_tiles`,
`touches_target`, `group_window_count`, `group_frame_fraction`,
`group_covers_whole_frame` y `capture_gap_ms` (intervalo real entre sellos de captura;
`interval_ms` es la pausa solicitada). `touches_target` indica intersección del **tile**
distinto con la caja, no demuestra que el píxel distinto esté dentro de ella. El tamaño de
tile es siempre 64×64, incluidos los bordes parciales; la comparación reutiliza
`dirty_tiles` y `dirty_tiles_in_regions`. Cada par tiene base propia, no una observación
publicada. Se descarta además un fotograma inicial para establecer `DisplayMapping`.

El grupo usa `WindowRead` y la misma selección de ADR-0032 que el motor: ventana bajo el
centro de la caja, primer plano y las ventanas que solapan cualquiera de esas dos semillas,
sin cierre transitivo. La cobertura es el área de la **unión** de los rectángulos, recortados
al fotograma y ampliados por `action_freshness_margin_px` de la configuración; las
intersecciones no se suman dos veces. Ventanas de fondo pueden producir cobertura 1 y
`group_covers_whole_frame = true`: en ese caso no se presume ninguna ganancia. No se oculta
el cursor de texto ni ninguna animación.

Lecturas iguales rodean cada captura y ambos miembros del par deben conservar el grupo.
Cambio de grupo, geometría/época, caja fuera del fotograma, ventana fuera de la política,
stop o evidencia incompleta abortan la invocación; no se fabrica un cero ni se publica
un informe parcial como si fueran N pares válidos. El binario emite sólo el JSON numérico
por stderr, sin escribir archivos (SEC-04).
El controlador `tools/perf/freshness_probe.py` exige salida exitosa, stdout vacío y un
informe completo con claves y tipos permitidos antes de guardarlo. Nunca guarda el stderr
bruto de un fallo. El archivo no se sobrescribe. No se
guardan fotogramas, clases, handles, coordenadas, etiquetas, títulos ni texto de pantalla.
La comparación DWM/GetWindowRect de ADR-0032 permanece pendiente: M1 usa los límites DWM
que atestigua actualmente el motor y no afirma resolver aquella medición.

### D.1.1 Resultados de M1 (17 de octubre de 2026)

**Qué es.** La primera ejecución de M1, con el responsable presente y en su escritorio tal como estaba. Es descriptiva: no hay umbral de aprobación y no decide nada. No activa `action_freshness = "target_window"`, no acredita IT-08 y no cambia ningún estado de requisito ni casilla de §6.7. Al escribirse esta sección, M2 e IT-08 no se habían ejecutado; se ejecutaron de forma descriptiva el 17 de octubre de 2026, después de M1 (§D.2.1).

**Binario y configuración.** Compilado desde el commit `8587f4e` de `main` con `--features sendinput`; el SHA-256 empieza por `1D363C36A9637F33`. Herramienta `tools/perf/freshness_probe.py`, en modo descriptivo, con N = 60 pares (30 con el menú cerrado y 30 con el menú abierto), intervalo de 150 ms y margen 0.

**Condiciones.**

- Escritorio del responsable tal como estaba, con las ventanas sin minimizar. Dos monitores de 1920 × 1080; se midió la salida 0. El perfil del MVP es de un monitor: es una desviación, que se declara aquí.
- DPI de 96 (100 %).
- El responsable trajo al frente el Bloc de notas a mano. La caja objetivo fue un elemento de menú del Bloc de notas de 56×32 px físicos, leída con UI Automation en solo lectura; no se registran ni su etiqueta ni sus coordenadas (ADR-0012 y ADR-0032). El responsable abrió y cerró el menú a mano.
- Ninguna entrada de herramienta, ninguna actuación del motor y nada guardado de la pantalla. Los informes JSON en bruto quedan fuera del repositorio, en `%LOCALAPPDATA%\Ommatix\runs\m1-menu-cerrado.json` y `%LOCALAPPDATA%\Ommatix\runs\m1-menu-abierto.json`.
- El primer intento falló antes de medir, por un error de configuración del operador: `allowed_window_classes` estaba escrito como lista y no como el mapa por aplicación. Se corrigió y la medición se repitió completa.

**Resultados.** Los dos grupos tienen 30 de 30 pares válidos.

- **Menú cerrado (30 pares):** 5 ventanas en el grupo en todos los pares; `group_frame_fraction` 1,0 y `group_covers_whole_frame` verdadero en 30 de 30; `total_tiles` 0 en todos los pares; `touches_target` 0; `outside_pairs` 0; mediana de `capture_gap_ms` de 166,1 ms.
- **Menú abierto (30 pares):** 6 ventanas en el grupo: probablemente la ventana emergente del menú (la sonda no registra identidades); cobertura 1,0 en 30 de 30; `total_tiles` 0 en todos los pares; `outside_pairs` 0; mediana de `capture_gap_ms` de 166,8 ms.

**Lectura.** En este escritorio, el grupo de ADR-0032 cubre toda la salida en cada par, así que `target_window` no puede reducir el área comparada. La salida no identifica las ventanas que forman esa cobertura. El propio ADR-0032 advierte que un fondo que cubre la salida puede extender el grupo a toda la pantalla; M1 confirma el efecto sin establecer la causa. La pantalla estuvo quieta, así que tampoco se observó ningún cambio: M1 no dice nada sobre los rechazos de S3 §9.6 y §10. El 16 de octubre de 2026 el responsable fijó M1 como medición descriptiva, sin umbral de aprobación, y acordó que, si M1 mostraba el grupo cubriendo siempre toda la salida, el arquitecto propondría enmendar ADR-0032 y el responsable decidiría. Es ese caso. **Decisión del responsable (17 de octubre de 2026): proponer la enmienda**, [ADR-0035](../adr/0035-freshness-group-limited-to-occluders.md), en estado *Proposed*. No se implementa. Antes de cualquier uso en vivo, la enmienda necesita repetir M1 en el escritorio del responsable.

### D.1.2 Preparación de la repetición de M1 con los dos grupos (ADR-0035, aceptada; histórico: la repetición se ejecutó después, en §D.1.3)

**Estado histórico: preparada y no ejecutada al escribirse; la repetición se ejecutó el 17 de octubre de 2026, con su resultado en §D.1.3.** Es la repetición que pide la validación de [ADR-0035](../adr/0035-freshness-group-limited-to-occluders.md) (estado *Accepted*) antes de cualquier uso en vivo. No se había hecho ninguna corrida al escribirse esto; nada de lo que sigue marca una casilla de §6.7, cambia un estado de requisito ni activa `action_freshness = "target_window"`. M2 e IT-08 estaban pendientes al escribirse esto (ejecutados el 17 de octubre de 2026, §D.2.1).

**Qué mide de más.** La sonda sigue registrando, por par, los campos de ADR-0032 sin cambios (§D.1) y añade, bajo la clave `occluder_group`, el grupo enmendado calculado con el **mismo código de selección del motor** sobre las mismas lecturas, capturas y margen: la ventana bajo la caja más las ventanas por encima que solapan su rectángulo ampliado por el margen. Así la reducción se mide sobre los mismos pares y no se infiere. Por par, `occluder_group` lleva `group_tiles`, `group_window_count`, `group_frame_fraction`, `group_covers_whole_frame` (nulos si el grupo no se pudo atestiguar), `opacity_attested`, `scoped` (si la comparación acotada se aplicaría) y `fallback`, una cadena de un conjunto cerrado que dice por qué se compararía la pantalla entera: `no_opacity`, `corner`, `reads_differ`, `unattested_rect`, `box_outside_target` o `walk_failed` (las tres últimas están reservadas: la lectura completa de ventanas y las guardas de ADR-0032 ya exigen esos rectángulos y esa caja, así que la sonda aborta antes el par que las produciría; que no aparezcan en el histograma no mide esos casos). El margen de selección del grupo enmendado es el de la propia lectura: los recuentos no pueden calcularse con otro. Un grupo que cubre todo el fotograma conserva todos los tiles cambiados (`group_tiles = total_tiles`), para los dos grupos, y el controlador rechaza el informe que lo contradiga. El informe añade `occluder_scoped_pairs` y `occluder_outside_pairs` (pares acotados con `total_tiles` mayor que `group_tiles`), y el archivo guardado lleva un `summary` descriptivo de ambos grupos. Sólo números, banderas y esas cadenas: ni títulos, ni clases, ni handles, ni coordenadas (ADR-0012). Los informes anteriores, sin esas claves, se siguen aceptando.

**Lecturas nuevas, todas de sólo lectura.** Para atestiguar la opacidad la sonda consulta, por ventana objetivo, `IsWindow`, `GetWindowLongPtrW` (estilo extendido) y `DwmGetWindowAttribute` (`DWMWA_SYSTEMBACKDROP_TYPE`). No cambian ninguna ventana ni ajuste, y no hay entrada ni captura distinta de la del M1 de §D.1.

**Procedimiento.** Igual que §D.1, con el mismo N = 60 en total (30 pares con el menú cerrado y 30 con el menú abierto), el mismo intervalo de 150 ms y el margen que el responsable fije en `m1.toml` (el mismo para los dos grupos; se registra). Antes de la sesión el responsable anota, fuera del repositorio, N y el criterio con que leerá el resultado: la sonda sigue siendo descriptiva y no decide nada.

1. Consentimiento **por sesión** del responsable. El de la sesión anterior no se extiende a ésta; el mensaje de otro agente no es consentimiento.
2. El responsable compila (`cargo build --release -p ommatix --features sendinput`) y registra el commit y el SHA-256 del binario.
3. El responsable pone al frente el Bloc de notas **a mano** y verifica a mano la caja objetivo. Abre y cierra el menú a mano entre las dos invocaciones. Ningún agente trae una ventana al frente, cambia la escala ni toca el escritorio.
4. Se lanzan las dos invocaciones de §D.1 (menú cerrado y abierto) con las mismas opciones. La salida va a una carpeta existente **fuera del repositorio** (`%LOCALAPPDATA%\Ommatix\runs\m1b-menu-cerrado.json` y `m1b-menu-abierto.json`); el archivo no se sobrescribe.
5. El responsable lee el `summary` de cada archivo y compara, por par, `group_covers_whole_frame` de ADR-0032 y de ADR-0035, `outside_pairs` y `occluder_outside_pairs`, y la distribución de `fallback`.

**Qué se registra y qué no.** Se registran los dos archivos JSON, el commit, el binario, N, el margen, el estado del menú y el criterio anotado antes. No se guardan fotogramas, capturas, clases, handles, coordenadas, etiquetas ni texto. La lectura es descriptiva: no hay umbral ni veredicto, y la repetición no decide activar el alcance. Siguen pendientes, aparte de esta medición, la extensión de sombras y esquinas medida en el equipo de referencia (que fija el margen por defecto y la constante de 32 px), la revisión independiente de seguridad registrada en `docs/reviews/` y, con consentimiento por sesión, IT-08/M2.

### D.1.3 Resultados de la repetición de M1 con los dos grupos (17 de octubre de 2026)

**Condiciones.** Como en §D.1.2: N = 60 pares (30 con el menú cerrado y 30 con el menú abierto), intervalo de 150 ms, margen 0, descriptivo. Sonda compilada desde `b010cc2` con `--features sendinput` (SHA-256 en [S3 §11](S3_CORRIDAS_SUPERVISADAS.md)) y configuración `m1.toml`, la misma que la primera M1. Objetivo: el Bloc de notas, maximizado por el responsable; dos monitores de 1920 × 1080 a 100 %. Sin entrada. No se guardó ningún fotograma ni texto de pantalla; los registros quedan fuera del repositorio.

| Menú | Grupo | Ventanas | Cubre todo el fotograma | Opacidad atestiguada | Acotado | Respaldo `no_opacity` | Teselas totales | Pares fuera |
|---|---|---|---|---|---|---|---|---|
| Cerrado | ADR-0032 | 5 | 30/30 | — | — | — | 0 | 0 |
| Cerrado | ADR-0035 | 2 | 30/30 | 0/30 | 0/30 | 30/30 | 0 | 0 |
| Abierto | ADR-0032 | 6 | 30/30 | — | — | — | 0 | 0 |
| Abierto | ADR-0035 | 3 | 30/30 | 0/30 | 0/30 | 30/30 | 0 | 0 |

La mediana del intervalo de captura fue de unos 166 ms con el menú cerrado y de 168,1 ms con el menú abierto.

**Lectura del arquitecto** (a partir de una lista de ventanas de sólo lectura, sin nada guardado). El Bloc de notas estaba maximizado, así que la ventana objetivo sola cubre la salida; por encima sólo estaban la barra de tareas y, con el menú abierto, el menú emergente. La opacidad de la ventana del Bloc de notas en Windows 11 no se atestigua (`no_opacity`), lo que es coherente con un material de fondo del sistema distinto de NONE/AUTO; por diseño, la comparación es entonces de todo el fotograma. ADR-0035 no reduce nada para este objetivo. Una reducción sólo puede aparecer con un objetivo no maximizado cuya opacidad se atestigüe, y esta repetición no lo ejercita.

### D.2 M2 supervisada, IT-08: copia de configuración y diagnóstico D0

Protocolo previo a la ejecución descriptiva del 17 de octubre de 2026, que está en §D.2.1.

**Sólo en una sesión con consentimiento explícito del responsable**, con criterios y
presupuesto escritos antes de ejecutarla. Se mantienen las aplicaciones, las puertas,
el ledger, el stop exterior y el anclaje que requiere cada protocolo. Primero se lee M1;
la herramienta no decide activar la opción a partir de su resultado.

`--target-window-freshness` está disponible en `tools/s4/e2e_driver.py` y
`tools/s4/e2e_dossier.py`. Con el archivo del operador pasado por `--config`, escribe una
copia única en una subcarpeta `m2-*` junto a `--results`, obligatoriamente fuera del
repositorio. Sólo cambia `[engine] action_freshness = "target_window"`; conserva los
permisos y conserva el significado de las rutas de modelos, caché, stop, purga y anclaje.
Por decisión del arquitecto, la copia admite únicamente estas formas de ruta Windows:

- **Absolutas estándar**, que empiezan por letra, dos puntos y barra inversa
  (`C:\x\STOP`) o por un UNC con servidor y recurso (`\\servidor\recurso\x\STOP`). Se
  conservan **carácter por carácter**; `C:/x/STOP` no pertenece a esta lista cerrada.
- **Relativas simples**, con uno o más componentes separados por `\` o `/`, sin unidad
  ni separador inicial (`STOP`, `anclaje/ventana.pin`). La copia se construye sólo como
  texto: `directorio_suministrado + "\\" + ruta_relativa`. Ni el directorio **tal como se
  suministró** ni el valor relativo se reescriben con una biblioteca de rutas.

**Cada componente de ambas formas**, incluidos servidor y recurso UNC, debe cumplir
`[A-Za-z0-9_\-][A-Za-z0-9_\-. ]*`: empieza por letra ASCII, dígito, guion bajo o guion;
después admite esos caracteres, punto y espacio. Ningún componente puede terminar en
punto o espacio, ser `.` o `..`, ni quedar vacío: se rechazan separadores dobles y un
separador final, también en un valor de anclaje como `pin\`. No se admite `:` salvo el
de la unidad, ni `?`, `*`, `<`, `>`, `|` o `"`.

Se rechazan **antes de crear la carpeta o copia** las rutas relativas a una unidad
(`C:STOP`), relativas a la raíz (`\STOP`), con prefijos verbatim/dispositivo (`\\?\`,
`\\.\`), con componentes `..` (por ejemplo `..\STOP`) o vacías. La misma restricción de
componentes se aplica al argumento de configuración antes de convertirlo en un objeto de
ruta. `--config` debe suministrarse en forma absoluta con un directorio también absoluto
y con componentes nombrados; una configuración relativa o directamente en la raíz de la
unidad se rechaza. Además debe ser un archivo regular y no un enlace simbólico. La
validación y construcción de los valores copiados usan exclusivamente texto, sin
normalización ni resolución mediante `pathlib`; los objetos de ruta sólo sirven para
leer el TOML y escribir la copia. No se intenta emular las reglas especiales de Rust o
Win32. El cliente muestra el motivo fijo del rechazo, sin volcar la ruta.

Si una ruta opcional no se usa, **se omite su clave**; una clave de ruta presente con `""`
no se acepta en la copia M2. Una ruta cuyo significado no pueda verificarse o cuyo campo
no tenga correspondencia conocida también se rechaza. Un stop o anclaje relativo admitido
sigue apuntando al mismo archivo absoluto, aunque el archivo aún no exista. El original
nunca se modifica. La copia y el
hash del inicio permiten auditar la variante; sin la opción se usa la configuración
entregada, que también podría declarar un alcance explícito.

```powershell
# IT-03: misma sesión de menú de S3, apertura y cierre por «Editar»; sin escribir.
python tools/s4/e2e_driver.py --exe target\release\ommatix.exe `
  --config "$env:LOCALAPPDATA\Ommatix\live-notepad.toml" --app notepad --mode run `
  --runs $N --max-live $budget --round-start $roundStart `
  --ledger "$env:LOCALAPPDATA\Ommatix\runs\ledger.txt" `
  --results "$env:LOCALAPPDATA\Ommatix\runs\m2-it03.jsonl" --target-window-freshness

# D1 / BDD-35: mismo caso señuelo y guarda de foco; sólo el editor de pruebas permitido.
python tools/s4/e2e_dossier.py --exe target\release\ommatix.exe `
  --config "$env:LOCALAPPDATA\Ommatix\live-decoy.toml" --app decoy --mode run `
  --runs $N --max-live $budget --round-start $roundStart --operator-focus `
  --ledger "$env:LOCALAPPDATA\Ommatix\runs\ledger.txt" `
  --results "$env:LOCALAPPDATA\Ommatix\runs\m2-d1.jsonl" --target-window-freshness
```

El ejecutable de M2 se compila con las características de la configuración de la sesión
(por ejemplo `dxgi,sendinput,onnx,winrt-ocr,uia`, o `directml` si procede), no con la
compilación mínima de M1. Las variables de los ejemplos son valores del plan del
responsable, no criterios ni presupuestos propuestos por esta herramienta.

Cada rechazo que trae `_meta["io.ommatix/action_timings"].freshness_diff` conserva
`total_tiles`, `group_tiles` cuando está atestiguado, y `touches_target` en el registro de
la acción; también el intento de cierre de menú tras un fallo. Un campo de grupo ausente
permanece ausente, nunca se sustituye por cero. Los tiempos `*_us` siguen en `stages_ms`;
el objeto D0 no se divide entre mil. Ausencia de D0 en una respuesta no permite inferir
cero cambios. No se amplían las operaciones de los clientes ni se introduce una repetición
del mismo snapshot tras un rechazo. Las lecturas y cambios de ventanas los supervisa el
responsable; toda decisión sobre IT-08 exige la evidencia y criterios de esa sesión.

### D.2.1 Resultado de M2 / IT-08 (17 de octubre de 2026)

Ejecutada en la misma ronda que [S3 §11](S3_CORRIDAS_SUPERVISADAS.md): la D1 con `--target-window-freshness`, una vez, con `--max-live 4` y criterio descriptivo. La configuración es una copia de la de D1 en una carpeta `m2-*` fuera del repositorio, con sólo `action_freshness = "target_window"` cambiado.

- **Resultado:** el mismo que el intento 1-bis de D1: `indeterminate` con `INDETERMINATE_EFFECT`; clic de enfoque sobre Campo auxiliar; foco de teclado en Campo de destino; repetición con el resultado guardado; ninguna tecla; carpeta vacía. Libro 2 → 4.
- **Límite:** no hubo rechazo por frescura, así que no se emitió ningún D0 (el diagnóstico de §D.2). Si la comparación fue acotada a la ventana, el cliente no lo observa, y se declara así.
- **IT-03 con `target_window`:** no se ejecutó. Por decisión del responsable, el Bloc de notas recurre siempre al refresco de pantalla completa (§D.1.3).
- **Estado:** IT-08 queda ejecutado de forma supervisada y descriptiva, según la descripción de su fila en [`matrices/plan_pruebas.csv`](matrices/plan_pruebas.csv); el esquema de la matriz no cambia, y el campo de referencia de oráculo de esa fila sigue `NO_EJECUTADO`. Su estado de aceptación de producto sigue `PENDIENTE_PRODUCTO`, por decisión del responsable.

## Lo que no entra en estas tres sesiones

- **E2E-03 y las demás series con entrada real** (la serie de la undécima ronda sigue su propio protocolo y su cuaderno).
- **El corpus de aceptación** (≥ 300 capturas, 450 corridas) y la **etiqueta de entrega**.
- **EV-01 y la comparación B0–B3** (capítulo 09): necesitan el corpus.
