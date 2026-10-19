# Primeros pasos

[English](getting-started.md) · [Documentación](../README.md)

Requiere Rust 1.99.0 (la versión fijada en `rust-toolchain.toml`) y Python 3.11 o posterior para las
comprobaciones de la tesis.

```bash
# Thesis checks (from docs/thesis/; the validator needs jsonschema)
pip install -r docs/thesis/requirements-validacion.txt
cd docs/thesis && python referencia_ejecutable/run_tests.py && python validar_paquete.py && cd -

# The engine
cargo run -- demo                                   # simulated observations and a dry-run click
cargo run -- demo --observations 5 --theme dark     # more observations, another theme
cargo run -- serve                                  # MCP over stdio on the simulated desktop
cargo run -- serve --config ommatix.toml            # with a configuration file
cargo run -- serve --source images --images frames/ # a PNG frame directory
cargo run --features screen -- serve --source screen   # primary monitor (interim, xcap)
cargo run --features dxgi -- serve --source dxgi        # Windows: DXGI output 0, dry-run input
```

## Conectar un cliente MCP

Compile primero con `cargo build --locked`. En la configuración MCP de su cliente, use el
binario en `PATH`, o cambie `command` por su ruta absoluta:

```json
{"mcpServers":{"ommatix":{"command":"ommatix","args":["serve"]}}}
```

El cliente crea el proceso y habla JSON-RPC por stdio; los diagnósticos van a stderr.
La compatibilidad depende del perfil: consulte [el contrato](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md)
y [la instalación](../thesis/S4_INSTALACION_Y_REVERSION.md).

## Antes de usar una pantalla real

La demo y el servidor predeterminados usan el escritorio simulado. Para configurar fuentes
y modelos, lea [configuración](configuration.es.md); para entrada real, lea primero
[el control del operador](live-input.es.md). La instalación en una máquina limpia sigue
pendiente: [sesión A](../thesis/S4_SESIONES_PENDIENTES.md).
