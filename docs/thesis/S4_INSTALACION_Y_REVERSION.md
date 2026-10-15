# REL-01: instalación, verificación y reversión

**Fecha:** 15 de octubre de 2026 (`CHANGELOG.md`, 1.0.47). **Alcance:** cómo dejar Ommatix funcionando en una máquina Windows 11 x64, cómo comprobar que lo instalado es lo que el repositorio fija, cómo conectar un cliente MCP y cómo volver atrás hasta una versión anterior sin dejar nada. Es la parte de «instalación, rollback y límites de compatibilidad» de la casilla de la Definition of Done (capítulo 06 §6.7) que [`S4_REPRODUCIBILIDAD.md`](S4_REPRODUCIBILIDAD.md) §6 daba por faltante.

**Estado: escrita, no ejecutada de principio a fin.** Cada orden y cada ruta que nombra existe en el repositorio (lo comprueba una prueba, ver §9), y los digests son los de [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §2. Pero nadie ha seguido esta guía en una máquina limpia. Esa ejecución, con su `environment.json`, es la evidencia que falta y es del responsable ([`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md), sesión A). La casilla sigue sin marcar.

Nada de lo que sigue necesita entrada real. El motor no instala nada fuera de lo que el operador copia (§7): no escribe en el registro, no crea servicios ni tareas programadas y no arranca solo.

## 1. Requisitos previos

| Qué | Versión | Para qué |
|---|---|---|
| Windows 11 x64 | el perfil de [ADR-0008](../adr/0008-one-real-platform-first.md); medido en la build 26300.9550 | única plataforma real |
| Rust vía `rustup` | **1.99.0** exacta, fijada en `rust-toolchain.toml` (`rustup` la instala sola con `rustfmt` y `clippy` al entrar en el repositorio) | compilar |
| Herramientas de compilación de C++ de Visual Studio | las que pide `rustup` para el destino `x86_64-pc-windows-msvc` | enlazar |
| Git | cualquiera reciente | obtener y volver a un commit |
| Python | 3.11 o posterior | comprobaciones de la tesis, `tools/repro`, `tools/perf` |
| Idioma de OCR de Windows `es-ES` | instalado en Windows | `[models] ocr = "winrt"`; si falta, el motor se detiene antes de capturar |
| Una GPU con DirectX 12 | sólo para `directml` | el detector en la GPU; sin ella, la configuración de CPU |
| Una cuenta sin privilegios elevados | — | el actuador `SendInput` rechaza un primer plano elevado |

Para las comprobaciones de la tesis (`docs/thesis/`): `pip install -r docs/thesis/requirements-validacion.txt` (sólo `jsonschema==4.26.0`).

## 2. Obtener el código

```powershell
git clone https://github.com/Yosoyepa/Ommatix.git
cd Ommatix
git checkout <commit-de-entrega>
rustup show
```

Mientras el responsable no etiquete la entrega (RNF-08 pide un commit congelado, [`S4_REPRODUCIBILIDAD.md`](S4_REPRODUCIBILIDAD.md) §6), `<commit-de-entrega>` es el commit que el responsable indique, y se anota en `environment.json` (§6). El repositorio no tiene etiquetas todavía. `rustup show` debe nombrar `1.99.0`.

## 3. Compilar cada conjunto de rasgos

Siempre con `--locked`: `Cargo.lock` es parte de lo fijado y un cambio en él es un fallo, no una actualización.

| Para | Orden | Salida |
|---|---|---|
| Probar el motor sobre el escritorio simulado | `cargo build --release --locked` y `cargo run -- demo` | `target\release\ommatix.exe`; no necesita modelos |
| Capturar la pantalla con el detector en CPU | `cargo build --release --locked -p ommatix --features dxgi,onnx,winrt-ocr` | idem |
| El detector en la GPU (DirectML) | `cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr` | idem |
| Con evidencia de accesibilidad (UI Automation, [ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)) | añadir `uia` a cualquiera de las dos anteriores | idem |
| Con entrada real, **sólo en una sesión supervisada con el consentimiento del responsable** | añadir `sendinput` (implica `dxgi`) | idem; sigue necesitando el §5.2 |

El conjunto de las mediciones de latencia y de las sesiones de [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md) es `dxgi,directml,winrt-ocr,uia`. Las pruebas de CI de Windows usan `dxgi,sendinput,onnx,winrt-ocr`.

La aplicación de prueba propia (necesaria para `tools/perf/measure_latency.py`, `tools/perf/soak.py` y `tools/perf/dpi_probe.py`) es un crate aparte con su propio `Cargo.lock`:

```powershell
cd tools\s4\testapp
cargo build --release --locked
cd ..\..\..
```

Deja `tools\s4\testapp\target\release\ommatix-testapp.exe`. El editor propio (`tools\s4\testeditor`, sólo para E2E-02) se compila igual.

Comprobación de lo compilado, sin pantalla:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 4. Los archivos del operador

El repositorio no contiene pesos, runtimes ni capturas (regla 7 de `AGENTS.md`). Todo lo que carga el motor vive en `%LOCALAPPDATA%\Ommatix` y está fijado por SHA-256 en la configuración del operador: el motor lo comprueba **antes de cargar nada** y rechaza cualquier diferencia ([ADR-0021](../adr/0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md), pruebas `sec_06_*`). No hay descarga automática.

Estructura que esperan las tres configuraciones de `tools/s0/`:

```text
%LOCALAPPDATA%\Ommatix\
├── ommatix.s0.toml | ommatix.s0.directml.toml | ommatix.rnf05.directml.toml   (copias de tools\s0\)
├── models\
│   ├── manifest.json                       (CPU y DirectML de S0)  o  manifest.rnf05.json (fp16)
│   └── omniparser-icon-detect-v3\
│       ├── icon_detect_v3.onnx             (S0)
│       └── icon_detect_v3_fp16.onnx        (RNF-05)
└── runtime\
    ├── onnxruntime-1.28.0\lib\onnxruntime.dll
    └── onnxruntime-directml-1.24.4\{onnxruntime.dll, DirectML.dll}
```

Qué colocar y con qué digest (los mismos de [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §2, donde están con su origen y licencia):

| Archivo | De dónde | SHA-256 |
|---|---|---|
| `models\manifest.json` | copia de `docs/thesis/plantillas/model_manifest.s0.json` | `63d9df24109308d1e2b354648139fee0c1d4e2563f8cb2a83497d4e26b7d728e` |
| `models\manifest.rnf05.json` | copia de `docs/thesis/plantillas/model_manifest.rnf05.json` | `f8b04be90e3259ac2ca877d3e6928af155adf250a6d6d6c5292b1c4b0ae7fe47` |
| `icon_detect_v3.onnx` | exportado con `tools/s0/export_omniparser_v3.py` | `bcbec780f2b3e5fcbc13c26a73a73541c84ea3482a7696540eb258a076c9d968` |
| `icon_detect_v3_fp16.onnx` | el mismo script con `--fp16` | `d7a8901db0ae650616ee43f53e6ee8f57a829de54ab8daae431be77556cc0805` |
| `onnxruntime.dll` 1.28.0 (CPU) | `onnxruntime-win-x64-1.28.0.zip` de la release v1.28.0 de `microsoft/onnxruntime` | `18370c375f07357fa5874344a9d9ac17e6b6fe1eb18b1dd209d79483b4470257` |
| `onnxruntime.dll` 1.24.4 (DirectML) | paquete NuGet `Microsoft.ML.OnnxRuntime.DirectML` 1.24.4, `runtimes/win-x64/native/` | `e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40` |
| `DirectML.dll` 1.15.4 | paquete NuGet `Microsoft.AI.DirectML` 1.15.4, `bin/x64-win/` | `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1` |

Los pesos de partida (`icon_detect_v3/model.pt`, MIT) se descargan de `microsoft/OmniParser-v2.0`, revisión `f55d0750e5b94db2125ef0b45b0fa4a85ddc59b4`, y su digest (`11c6cbb77f22569fab22d86c76407a83ec81ab89dbfe28279854822d6e3fb00c`) lo comprueba el script de exportación antes de exportar; **no se usan los pesos `icon_detect` AGPL-3.0.** El entorno de exportación (torch, onnx, onnxruntime) es un `venv` temporal fuera del repositorio; el encabezado del script da las versiones y las órdenes exactas.

`DirectML.dll` se distribuye con los términos de Microsoft para DirectML (no MIT) y el repositorio no la incluye; usarla es una decisión de licencia del responsable ([`S4_REPRODUCIBILIDAD.md`](S4_REPRODUCIBILIDAD.md) §3).

Para copiar la configuración y comprobar un archivo a mano:

```powershell
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Ommatix" | Out-Null
Copy-Item tools\s0\ommatix.s0.directml.toml "$env:LOCALAPPDATA\Ommatix\"
Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\Ommatix\models\manifest.json"
```

Las rutas de la configuración son relativas a su archivo. Una ruta mal puesta, un digest distinto, un runtime parcheado o un idioma de OCR sin instalar detienen el motor antes de abrir la captura; nada recurre a otro modelo o idioma.

## 5. La configuración del operador

El archivo tiene la forma de `docs/thesis/plantillas/configuracion_mvp.toml` (se valida con rigor: una clave desconocida lo rechaza). Sin archivo, el motor niega toda actuación. Guárdelo en un directorio que sólo el operador pueda escribir.

### 5.1 Por defecto: sólo observar

Las tres configuraciones de `tools/s0/` ya son así: `actuation_enabled = false` y `allowed_applications = []`. Con ellas ninguna acción llega al sistema aunque el agente pida `dry_run: false`. Las sesiones de medición (`tools/perf`) las exigen y se niegan a arrancar con otra.

### 5.2 Para una sesión supervisada con entrada real

Sólo cuando el responsable la autoriza, y siempre con el motor compilado con `sendinput` y `--input sendinput --source dxgi`. Hace falta **todo** esto en la configuración:

```toml
[safety]
actuation_enabled = true
allowed_applications = ["notepad.exe"]   # minúsculas; nunca applicationframehost.exe
stop_file = "STOP"                        # relativo a la configuración
allowed_window_classes = {}               # {"explorer.exe" = ["CabinetWClass"]} si se permite el explorador
window_pin_file = ""                      # ADR-0020: ventana única registrada por el operador

[models]
accessibility = "uia"                     # evidencia de UI Automation (ADR-0017); nunca lee el valor de un campo
```

- **Parada.** Mientras exista el archivo `STOP` junto a la configuración, el motor no entrega entrada nueva (`POLICY_DENIED`); borrarlo la reanuda. Ningún tool de MCP puede crearlo ni borrarlo. Terminar el proceso también detiene.
- **Alcance por ventana.** Un proceso como `explorer.exe` es dueño también del escritorio y de la barra de tareas: `allowed_window_classes` y `window_pin_file` lo acotan ([ADR-0020](../adr/0020-operator-scope-on-windows.md)). El motor avisa en `stderr` si se permite `explorer.exe` sin ninguno de los dos.
- **Accesibilidad.** Requiere la compilación con `uia` y una fuente en vivo; se fusiona con la visión y nunca la sustituye.

El detalle de cada clave y sus rangos está en `README.md` (sección «Quick start»). Esta guía no los repite.

## 6. Verificar lo instalado

Desde la raíz del repositorio, sólo lectura y sin red:

```powershell
python tools/repro/environment.py --operator-dir "$env:LOCALAPPDATA\Ommatix" --out environment.json
```

Termina con código 1 si algún archivo existe con un digest distinto del fijado (`status: mismatch`); un archivo de otra variante que no se instaló aparece como `absent`, que no es un fallo. Escribe el commit, si el árbol estaba limpio, `rustc`, los digests de los tres `Cargo.lock`, la máquina y el estado de cada archivo fijado. No imprime el contenido de ningún archivo. Guarde `environment.json` junto a la entrega: es el registro de lo que se instaló.

Después, el motor de punta a punta sobre el escritorio simulado y sobre la pantalla real (esto último captura fotogramas en memoria, nunca se guardan, y no envía entrada):

```powershell
cargo run -- demo
target\release\ommatix.exe demo --source dxgi --config "$env:LOCALAPPDATA\Ommatix\ommatix.s0.directml.toml" --session-id verificacion --observations 3 --timings
```

La segunda debe imprimir tres observaciones con elementos y, en `stderr`, sus tiempos por etapa. Si falla antes de capturar, el mensaje nombra el artefacto o la clave, nunca su contenido.

## 7. Conectar un cliente MCP

El motor habla MCP por `stdio` (perfil 2026-07-28; `--legacy` añade el 2025-11-25). `stdout` lleva sólo mensajes del protocolo; los diagnósticos van a `stderr`. En un cliente que lea una lista de servidores con `command` y `args`:

```json
{
  "mcpServers": {
    "ommatix": {
      "command": "C:\\ruta\\al\\repositorio\\target\\release\\ommatix.exe",
      "args": ["serve", "--source", "dxgi",
               "--config", "C:\\Users\\<usuario>\\AppData\\Local\\Ommatix\\ommatix.s0.directml.toml"]
    }
  }
}
```

Expone tres herramientas con los esquemas de `docs/thesis/contratos/esquemas/`: `observe_screen`, `execute_action` y `get_engine_metrics`. El agente actúa por identificador y revisión de elemento, nunca por coordenadas, y toda acción es un ensayo en seco salvo que se cumpla el §5.2. Con las configuraciones de `tools/s0/` el cliente sólo puede observar.

**Compatibilidad comprobada:** los clientes de `tools/s3/mcp_driver.py` y `tools/s4/` (propios) y los 10 contratos CT-01..10 del adaptador. Ningún cliente MCP de terceros se ha probado (`README.md`). Un cliente que no hable el perfil 2026-07-28 necesita `--legacy`.

### Qué deja el motor en la máquina

Nada propio: ninguna crate de producción abre un socket, escribe un archivo ni lanza un proceso (prueba `sec_04_no_production_code_opens_a_socket_writes_a_file_or_starts_a_process`). Los únicos archivos son los que el operador colocó (§4), los que crea para la parada y la ventana registrada (§5.2) y los informes que las herramientas de `tools/` escriben donde se les indica con `--out`.

## 8. Reversión

Volver atrás son tres cosas independientes: parar, volver a una versión anterior y, si se quiere, retirar los archivos del operador. Ninguna deshace una acción ya entregada al sistema (el motor no las revierte; capítulo 10).

### 8.1 Parar

Cree el archivo de parada (`New-Item "$env:LOCALAPPDATA\Ommatix\STOP"`) si el motor corre con entrada real, y termine el proceso. Compruebe que no queda ninguno de los tres:

```powershell
tasklist /FI "IMAGENAME eq ommatix.exe"
tasklist /FI "IMAGENAME eq ommatix-testapp.exe"
tasklist /FI "IMAGENAME eq ommatix-testeditor.exe"
```

### 8.2 Volver a un commit y a un binario anteriores

El binario, la configuración y el manifiesto van juntos: una configuración se valida con rigor y fija el digest del manifiesto, así que un binario viejo con la configuración nueva puede rechazarse (clave desconocida) y uno nuevo con un manifiesto viejo, también (digest distinto). Es lo esperado.

```powershell
git fetch --all --tags
git worktree add ..\Ommatix-anterior <etiqueta-o-commit-anterior>
cd ..\Ommatix-anterior
cargo build --release --locked -p ommatix --features dxgi,directml,winrt-ocr,uia
```

1. Use el `environment.json` de esa entrega para comprobar que el toolchain (`rustc`) y los digests de los tres `Cargo.lock` coinciden con los de la compilación que acaba de hacer.
2. Copie a `%LOCALAPPDATA%\Ommatix` la configuración **de ese commit** (`tools\s0\*.toml` y el manifiesto de `docs\thesis\plantillas\` de ese commit), sustituyendo las actuales. Conserve antes las actuales si piensa volver a ellas.
3. Si los pesos o los runtimes de esa entrega eran otros, sus digests están en la configuración y en el manifiesto de ese commit; colóquelos (§4) antes de arrancar.
4. Verifique con el §6, ahora con el árbol de trabajo de la versión anterior.

Convención recomendada (no la exige el motor): guardar cada entrega como `%LOCALAPPDATA%\Ommatix\releases\<commit>\` con su `ommatix.exe`, su `environment.json` y las configuraciones que usó, para volver sin recompilar. Un binario guardado se verifica comparando su SHA-256 con el anotado.

### 8.3 Retirar los archivos del operador

Primero mire qué hay; los pesos pesan cientos de megabytes y tardan en volver a obtenerse:

```powershell
Get-ChildItem -Recurse "$env:LOCALAPPDATA\Ommatix" | Measure-Object -Property Length -Sum
```

- **Retirada parcial** (dejar la máquina sin configuración, conservar los modelos): borre los `*.toml`, `STOP`, el archivo de ventana registrada y `releases\`.
- **Retirada completa:** `Remove-Item -Recurse "$env:LOCALAPPDATA\Ommatix"`. Es la única orden destructiva de esta guía y no tiene vuelta atrás: los pesos exportados se regeneran con `tools/s0/export_omniparser_v3.py`, y los runtimes se vuelven a bajar de su origen y se comprueban contra los digests del §4.
- La salida de compilación se retira con `cargo clean` en la raíz y en `tools\s4\testapp` y `tools\s4\testeditor`; el árbol de trabajo extra, con `git worktree remove ..\Ommatix-anterior`.

### 8.4 Comprobar que no queda nada

```powershell
tasklist /FI "IMAGENAME eq ommatix.exe"
Test-Path "$env:LOCALAPPDATA\Ommatix"
git worktree list
git status --porcelain
```

Esperado tras una retirada completa: ningún proceso, `False`, sólo el árbol de trabajo principal y un árbol limpio. No hay nada más que mirar fuera de esas rutas (§7): ni claves del registro, ni servicios, ni tareas programadas, ni entradas de inicio. Los informes de `tools/` (`environment.json`, `soak-*.json`, `dpi-probe-*.json`) están donde el responsable los dejó con `--out`; no contienen datos de pantalla, pero son suyos y se borran a mano.

## 9. Lo que comprueba el repositorio de esta guía

`rel_01_the_install_guide_points_at_files_that_exist` (`src/acceptance/tests/repository.rs`) comprueba que cada ruta del repositorio que nombra este documento y [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md) existe, y `tools/repro/test_install_guide.py` que cada opción de `ommatix`, cada rasgo de compilación, cada opción de las herramientas de Python y cada digest de la guía existe en el código o en [`S0_RESULTADOS.md`](S0_RESULTADOS.md). Si el CLI o una herramienta cambian de nombre, las pruebas fallan hasta que la guía lo recoja. No prueban que las órdenes funcionen en una máquina limpia: eso es la sesión A.

## 10. Límites de compatibilidad

| Límite | Valor | Fuente |
|---|---|---|
| Sistema | Windows 11 x64; un monitor (la salida 0 de DXGI, `--output N`) | ADR-0008 |
| Pantalla | sin rotar, formato BGRA8; una pantalla rotada se rechaza | `README.md` |
| Escala | 100, 125 y 150 %; **IT-07 sin ejecutar** y el motor no comprueba la escala fuera del perfil | RNF-07, [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md) sesión C |
| OCR | Windows OCR `es-ES`; sin recurso a otro idioma | `README.md` |
| Detector | `icon_detect_v3` con una sola clase (rol `unknown`) | `S0_RESULTADOS.md` §4 |
| Entrada | sólo `SendInput`; rechaza primer plano elevado, punto fuera de las ventanas, modificadores pulsados; sin `applicationframehost.exe` | ADR-0007, ADR-0020 |
| Otras plataformas | `screen` y `native-input` (macOS, Linux) son interinos; el motor nunca los deja actuar en vivo | `AGENTS.md` regla 6 |

## 11. Lo que sigue siendo del responsable

1. **Commit de entrega congelado** y su etiqueta (RNF-08).
2. **Corpus de aceptación** (≥ 300 capturas etiquetadas y las 450 corridas E2E, capítulo 03 §3.5): no existe.
3. **Ejecutar esta guía en una máquina limpia** y archivar el `environment.json` resultante.
4. **Licencias:** la del código y la de los documentos quedaron decididas el 15 de octubre de 2026 (`MIT OR Apache-2.0` y CC BY 4.0; [`S4_REPRODUCIBILIDAD.md`](S4_REPRODUCIBILIDAD.md) §3). Queda la de `DirectML.dll`, que el repositorio no contiene ni redistribuye.
