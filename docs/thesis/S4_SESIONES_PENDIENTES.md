# S4: sesiones supervisadas pendientes de la Definition of Done

**Fecha:** 15 de octubre de 2026 (`CHANGELOG.md`, 1.0.47). **Alcance:** lo que el responsable debe poner (tiempo, ajustes, órdenes) para producir, cada una en una sola sesión, la evidencia que falta de REL-01, PERF-03 (RNF-06) e IT-07 (RNF-07). Este documento y las herramientas que describe son **preparación**: no se ejecutó ninguna sesión, ninguna fila de las matrices cambia de estado (`PENDIENTE_PRODUCTO` y `NO_EJECUTADO` siguen como están) y no hay cifra nueva.

**Alcance del MVP (15 de octubre de 2026):** la sesión A (REL-01, instalación y reversión) y la sesión B (PERF-03, soak de 2 h) siguen en el MVP; la sesión C (IT-07, DPI) pasa al próximo corte, como EV-01, EV-02 y el corpus de REL-01 (capítulo 06 §6.7, [ADR-0023](../adr/0023-licence-and-reduced-mvp-scope.md)).

Ninguna de las tres sesiones envía entrada (ni de teclado, ni de ratón) ni usa `execute_action`. Las herramientas de `tools/perf/` observan, y están escritas para negarse a empezar si la configuración del operador pudiera entregar entrada ([`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md) §5.1).

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

| | |
|---|---|
| **Tiempo** | 125 minutos ininterrumpidos (5 de preparación y 120 de corrida), más 5 minutos antes para el ensayo corto. |
| **Necesita del responsable** | Dejar el equipo solo; sin suspensión ni bloqueo de pantalla durante 2 h; **fijar los umbrales antes** (el dossier no da ninguno para RNF-06; capítulo 09 §9.7): crecimiento máximo de bytes privados por hora y cola máxima de regiones pendientes. |
| **Ajustes** | Plan de energía «Alto rendimiento» (el de S0); un solo monitor; 100 % de escala (como en RNF-05); notificaciones desactivadas. |

1. Ensayo corto, con la corrida real entera por delante: confirma que la aplicación de prueba toma el primer plano sola, que el contador de GPU responde y que el informe se escribe (la herramienta se negará con código 2 si no puede mantener el primer plano sin entrada).

   ```powershell
   python tools/perf/soak.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.rnf05.directml.toml" --minutes 1 --interval 2 --out C:\ruta\fuera\soak-ensayo.json
   ```

2. La corrida de 2 h (por defecto `--minutes 120`, una muestra cada 5 s; los umbrales sólo si el responsable los fijó):

   ```powershell
   python tools/perf/soak.py --config "$env:LOCALAPPDATA\Ommatix\ommatix.rnf05.directml.toml" --minutes 120 --max-rss-growth-mib-per-hour <MiB/h> --max-pending-regions <n> --out C:\ruta\fuera\soak-120.json
   ```

**Qué hace** (sin entrada, sin `execute_action`): lanza sólo la aplicación de prueba con `--animate` (dos etiquetas que cambian cada 100 ms, para que haya regiones incrementales continuas) y `ommatix serve --source dxgi` con la configuración, y cada 5 s observa y muestrea. Termina sólo sus procesos.

**Se registra** (todo en `soak-120.json`):

- `samples`: por muestra, el instante, el conjunto de trabajo, los bytes privados y los manejadores de `ommatix.exe`, la memoria dedicada de GPU si el contador de rendimiento de Windows la da (si no, `gpu_counter.reason`), `pending_regions`, `observations`, `rejected_actions` y `delivered_actions` de `get_engine_metrics`, y la latencia de `observe_screen` (ida y vuelta y total del motor, de `_meta`).
- `resources`: de cada recurso, mínimo, máximo, mediana del primer y del último décimo tras el calentamiento y la pendiente por hora; `pending_regions`: máximo, último y número de muestras en cero (cola acotada); `latency_ms`: p50/p95/p99/máximo global y por ventana de 10 minutos (cómo cambia la cola con el tiempo).
- `counts` (ticks, observaciones, pérdidas de primer plano, errores), `ended` (`completed` es la única terminación completa), `criteria` (sólo si se dieron umbrales), el commit, si el árbol estaba limpio, y los SHA-256 del ejecutable y de la configuración.

Qué decide: RNF-06 pide «RSS/VRAM totales, cola acotada y sin crecimiento sostenido en soak de 2 h». La herramienta entrega la serie y las pendientes; no declara «sin crecimiento sostenido». Esa lectura, con los umbrales que el responsable fijó antes, es suya. Una corrida que termina con `foreground_lost` o `observe_errors` no es un soak de 2 h: se anota como corrida truncada con su motivo y se repite.

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

**Fuera de perfil (opcional, una vez).** Ponga una escala personalizada que no sea 100, 125 ni 150 (por ejemplo 175 %) y ejecute la misma orden. **Estado del motor:** rechaza una pantalla rotada y un formato que no sea BGRA8 (como `CAPTURE_UNAVAILABLE`), pero no comprueba la escala; lo esperable hoy es `served_unflagged`. Eso sería un hallazgo contra RNF-07 («rechazo explícito fuera de perfil»), no un fallo de la herramienta, y arreglarlo (con su ADR, contrato y prueba) es un cambio aparte que el responsable decide.

Al terminar: `python tools/perf/dpi_probe.py --combine dpi-probe-100.json dpi-probe-125.json dpi-probe-150.json dpi-probe-cambio.json --out dpi-probe-resumen.json` indica qué escalas del perfil tienen una sesión completa y si se comprobó una transición de epoch. Devuelva la escala a la que tenía.

**Se registra** (por sesión): la escala y el DPI, el tamaño y el origen del monitor principal, la forma del fotograma (ancho, alto, paso, epoch y `DisplayMapping`), por observación el epoch, el recuento de elementos y **cuántos elementos llevan cada una de las 7 etiquetas fijas de la aplicación de prueba** (ninguna otra etiqueta), y los criterios. No hay fotogramas ni coordenadas.

## Lo que no entra en estas tres sesiones

- **E2E-03 y las demás series con entrada real** (la serie de la undécima ronda sigue su propio protocolo y su cuaderno).
- **El corpus de aceptación** (≥ 300 capturas, 450 corridas) y la **etiqueta de entrega**.
- **EV-01 y la comparación B0–B3** (capítulo 09): necesitan el corpus.
