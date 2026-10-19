# Getting started

[Español](getting-started.es.md) · [Documentation](../README.md)

Requires Rust 1.99.0 (the release pinned in `rust-toolchain.toml`) and Python 3.11 or later for the
thesis checks.

```bash
# Thesis checks (from docs/thesis/; the validator needs jsonschema)
pip install -r docs/thesis/requirements-validacion.txt
cd docs/thesis && python referencia_ejecutable/run_tests.py && python validar_paquete.py && cd -

# The engine
cargo run -- demo                                   # observations and a dry-run click on the simulated desktop, printed
cargo run -- demo --observations 5 --theme dark     # more observations, another theme
cargo run -- serve                                  # MCP server on stdio over the simulated desktop
cargo run -- serve --config ommatix.toml            # same, with a configuration file
cargo run -- serve --source images --images frames/ # a directory of PNG frames
cargo run --features screen -- serve --source screen   # the primary monitor (interim, xcap)
cargo run --features dxgi -- serve --source dxgi        # Windows: DXGI capture of output 0, dry-run input
```

## Connect an MCP client

Build first with `cargo build --locked`. In your client’s MCP configuration, use the absolute path of the binary and the following arguments:

```json
{"mcpServers":{"ommatix":{"command":"ommatix","args":["serve"]}}}
```

The example assumes the binary is on `PATH`; otherwise replace `command` with its absolute path. The client launches the process and exchanges JSON-RPC over stdio; diagnostics go to stderr. Compatibility depends on the profile: see [the contract](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md) and [installation](../thesis/S4_INSTALACION_Y_REVERSION.md).

## Before using a real screen

The default demo and server use the simulated desktop. For sources and models, read [configuration](configuration.md); for live input, first read [operator control](live-input.md). Installation on a clean machine remains pending: [session A](../thesis/S4_SESIONES_PENDIENTES.md).
