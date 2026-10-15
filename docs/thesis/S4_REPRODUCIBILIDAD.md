# REL-01: reproducibilidad de las comprobaciones (RNF-08)

**Fecha:** 14 de octubre de 2026 (`CHANGELOG.md`, 1.0.35). **Alcance:** qué se congela para que otra persona repita las comprobaciones del repositorio y obtenga lo mismo, dónde está cada dato y qué falta. Este documento no cierra RNF-08 ni la casilla de la Definition of Done que le corresponde (capítulo 06 §6.7: «Lockfiles, hashes, licencias, instalación, rollback y límites de compatibilidad están documentados»): falta ejecutar la instalación y el rollback en una máquina limpia (hay guía escrita desde 1.0.47) y el corpus de aceptación (§6).

Cada afirmación de las tablas la comprueba una prueba de Rust (`src/acceptance/tests/repository.rs`, funciones `rel_01_*`) o el script `tools/repro/environment.py`; lo que sólo describe, lo dice.

## 1. Qué se congela

| Qué | Dónde | Cómo se comprueba |
|---|---|---|
| Código | el commit del PR; el registro de cada medición lleva su `commit` y si el árbol estaba limpio | `tools/repro/environment.py` (`commit`, `tree_clean`) y el informe de `core_latency` |
| Dependencias de Rust | `Cargo.lock` (raíz), `tools/s4/testapp/Cargo.lock`, `tools/s4/testeditor/Cargo.lock`; ninguno está en `.gitignore` | `rel_01_the_lockfiles_are_committed_and_current`: existen, no se ignoran y `cargo metadata --locked --offline` resuelve el espacio de trabajo sin modificar `Cargo.lock` |
| Toolchain | `rust-toolchain.toml`: la versión exacta `1.99.0` (antes `stable`), con `rustfmt` y `clippy`; la CI instala esa misma versión | `rel_01_the_toolchain_is_pinned_to_one_release`: versión de tres números, no menor que `rust-version` del espacio de trabajo |
| Python de las comprobaciones | `docs/thesis/requirements-validacion.txt`: `jsonschema==4.26.0`; el oráculo sólo usa la biblioteca estándar | `rel_01_the_python_checks_are_pinned` |
| Modelos y runtimes | `docs/thesis/plantillas/model_manifest.s0.json` y `model_manifest.rnf05.json`; las configuraciones del operador `tools/s0/ommatix.s0.toml`, `ommatix.s0.directml.toml` y `ommatix.rnf05.directml.toml` fijan los SHA-256 de los manifiestos, de ONNX Runtime y de DirectML; los pesos los fija cada manifiesto | `rel_01_every_pinned_digest_is_listed_with_its_provenance` (cada digest fijado está en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §2 con su origen, y cada manifiesto fijado es el que el repositorio distribuye) y `sec_06_*` (el motor rechaza cualquier archivo que no coincida) |
| Licencias de los pesos | el campo `license` de cada manifiesto, y la tabla de [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §2 | `sec_06_the_shipped_manifests_record_origin_digest_and_licence` |
| Licencias de las dependencias | §3 de este documento | `rel_01_every_dependency_has_a_licence_and_none_is_copyleft` |
| Hardware y software de las mediciones | `docs/thesis/plantillas/hardware_profile.s0.json` (formato de `hardware_profile.json`, completo) y el registro de cada medición | `rel_01_the_hardware_of_the_measurements_is_recorded` y `tools/repro/environment.py` |
| Corpus | §5 | descripción; no hay corpus de aceptación todavía |

## 2. Artefactos fijados, comprobados en la máquina de referencia

Con `python tools/repro/environment.py` (sólo lectura, sin red), el 14 de octubre de 2026, sobre `%LOCALAPPDATA%\Ommatix`, **los 11 archivos que fijan las tres configuraciones de `tools/s0/` tienen el digest fijado**: los tres manifiestos, los dos pesos (`icon_detect_v3.onnx`, `icon_detect_v3_fp16.onnx`; cada manifiesto declara los suyos y coinciden), ONNX Runtime 1.28.0 (CPU), ONNX Runtime 1.24.4 con DirectML y `DirectML.dll` 1.15.4. Ningún mensaje de error ni el informe imprimen contenido de un archivo. El registro de esa comprobación, con el commit, el toolchain, los digests de los tres `Cargo.lock` y la máquina, es `plantillas/environment.rel01.json`.

Las pruebas `sec_06_*` hacen lo contrario con archivos temporales falsos, nunca con los pesos reales: un bit cambiado, un archivo truncado, ampliado, vacío o sustituido por otro, un manifiesto editado después de fijarlo, un runtime parcheado y un archivo de prototipos de íconos con una línea añadida se rechazan antes de cargar nada.

## 3. Licencias del grafo de dependencias

`cargo metadata --locked` lista 243 paquetes para todas las plataformas: los 10 del espacio de trabajo (`UNLICENSED`, más abajo) y **233 de terceros**. Todos los de terceros declaran licencia; ninguno es copyleft fuerte. Las expresiones distintas y cuántos paquetes tiene cada una:

| Expresión | Paquetes |
|---|---|
| `MIT OR Apache-2.0` | 126 |
| `MIT` | 42 |
| `Unicode-3.0` | 18 |
| `Apache-2.0 OR MIT` | 12 |
| `Unlicense OR MIT` | 6 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 4 |
| `MIT/Apache-2.0` | 3 |
| `BlueOak-1.0.0` | 3 |
| `Zlib OR Apache-2.0 OR MIT` | 2 |
| `Zlib` | 2 |
| `MIT OR Zlib OR Apache-2.0` | 2 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `Apache-2.0` | 2 |
| `Unlicense/MIT` | 2 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `MIT-0` | 1 |
| `Apache-2.0/MIT` | 1 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |

El paquete con `LGPL-2.1-or-later` la ofrece como alternativa (`OR`): se usa bajo MIT. El código del repositorio es `UNLICENSED` (`Cargo.toml`: `license = "UNLICENSED"`, `publish = false`): el responsable no ha elegido licencia para distribuirlo. Los pesos son MIT; `DirectML.dll` se distribuye con los términos de Microsoft para DirectML, no es MIT y el repositorio no la incluye ([`S0_RESULTADOS.md`](S0_RESULTADOS.md) §2). Los pesos `icon_detect` AGPL-3.0 no se usan. Si cambia el grafo, la prueba falla hasta que esta tabla lo recoja.

## 4. Cómo repetir las comprobaciones

Desde la raíz del repositorio, con el toolchain de `rust-toolchain.toml` (`rustup` lo instala solo):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m unittest discover -s tools/repro -p "test_*.py"
python tools/repro/environment.py --out environment.json
```

Desde `docs/thesis/` (necesita `pip install -r requirements-validacion.txt`):

```bash
python referencia_ejecutable/run_tests.py
python validar_paquete.py
```

Medición de PERF-01 (compilación `release`; no abre la pantalla ni envía entrada; tarda unos dos minutos de un solo hilo):

```bash
cargo run --release -p ommatix-acceptance --example core_latency -- --out perf01.json
```

El informe `perf01.json` nombra la compilación, la máquina, el commit y los argumentos. Sus cifras, tal como se midieron, están en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §11 y el informe de esa corrida es `plantillas/perf01_core_latency.s0.json`. En una máquina distinta las cifras cambian; la prueba que fija las reglas del método es `perf_01_*` en `src/acceptance/examples/core_latency.rs`, que corre con `cargo test`.

## 5. Corpus

| Corpus | Qué es | Cómo se reproduce |
|---|---|---|
| Escritorio simulado | `SimulatedDesktop` y `FixtureDesktop` (`src/adapters/sim`), determinista; `ommatix_adapter_sim::scripted_session(theme)` | `cargo run -- demo`; mismas entradas, mismos elementos, IDs y JSON |
| Fotogramas sintéticos del spike | `tools/s0/synthetic_frames.py` (una ventana de 1920×1080 dibujada con formas fijas y una semilla fija, y sus variantes; ninguna es una captura) | `python tools/s0/synthetic_frames.py --out-dir <directorio fuera del repositorio>` (necesita numpy y Pillow, que no son dependencias del binario) |
| Escena de PERF-01 | 300 controles y 600 fragmentos de texto en 1920×1080, generada por `src/acceptance/examples/core_latency.rs` (sin aleatoriedad) | el ejemplo; `perf_01_the_synthetic_scene_has_300_controls_and_600_spans` fija su tamaño |
| Aplicaciones de prueba de S4 | `tools/s4/testapp` y `tools/s4/testeditor`, con su `Cargo.lock` | los clientes de `tools/s4` y [`S4_ACEPTACION_REDUCIDA.md`](S4_ACEPTACION_REDUCIDA.md) |

El **corpus de aceptación** del capítulo 03 §3.5 (al menos 300 capturas etiquetadas para percepción y las 450 corridas E2E planificadas) **no existe**: capítulo 09 §9.3 lo declara por construir. Hasta que exista no hay corpus que congelar, y este documento no inventa uno.

## 6. Lo que falta para cerrar REL-01 y RNF-08

1. **Corpus de aceptación congelado** (§5): construirlo, etiquetarlo y fijarlo por hash. Es trabajo del responsable y de corridas supervisadas.
2. **Commit de entrega**: RNF-08 pide «commit … congelados». Un commit se congela en S4 al etiquetar la entrega; este PR sólo hace que cada medición lo registre.
3. **Instalación, rollback y límites de compatibilidad** (casilla de la Definition of Done): desde 1.0.47 hay una guía escrita, [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md), con la compilación de cada conjunto de rasgos, los archivos del operador con su digest, la verificación, la conexión de un cliente MCP, la reversión y los límites de compatibilidad, y pruebas que comprueban que lo que nombra existe (`rel_01_the_install_guide_points_at_files_that_exist` y `tools/repro/test_install_guide.py`). **Nadie la ha seguido en una máquina limpia**: esa ejecución y su `environment.json` son la sesión A de [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md). Lo que dice sobre la escala (100, 125 y 150 %) tampoco está medido: IT-07 sigue sin ejecutar (sesión C).
4. **Proveniencia más allá del hash**: S0 comprobó a mano la firma Authenticode de `onnxruntime.dll` (DirectML) y de `DirectML.dll`, y que los digests de los zips coinciden con los publicados; no hay una comprobación automática de firma.
5. **Aprobación de licencias por el responsable**: el detector del producto (ADR-0009) y la distribución de `DirectML.dll` son decisiones suyas; la licencia del código del repositorio, también.

Por eso `matrices/plan_pruebas.csv` mantiene REL-01 en `PENDIENTE_PRODUCTO`.
