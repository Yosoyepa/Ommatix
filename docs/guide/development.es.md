# Desarrollo y estructura

[English](development.md) · [Documentación](../README.md)

## Estructura

```text
src/                       workspace de Cargo (monolito modular, ADR-0014)
├── domain/                ommatix-domain        núcleo de la tesis, sin I/O: incremental,
│                                                fusion, appearance, identity, snapshot
├── application/           ommatix-application   puertos del SDD, caso de uso Engine, tipos del contrato
├── adapters/
│   ├── accessibility/     ommatix-adapter-accessibility  AccessibilityProvider: UI Automation (feature `uia`)
│   ├── capture/           ommatix-adapter-capture  FrameCapturer: guion, frames PNG, pantalla, DXGI
│   ├── vision/            ommatix-adapter-vision   VisionDetector, TextRecognizer (color plano, ONNX, OCR de Windows)
│   ├── input/             ommatix-adapter-input    OsActuator: dry run, entrada nativa, SendInput
│   ├── sim/               ommatix-adapter-sim      escritorios simulado y de fixtures (los cuatro
│   │                                               puertos de píxeles y entrada) para pruebas deterministas
│   └── mcp/               ommatix-adapter-mcp      servidor MCP por stdio (adaptador conductor),
│                                                   CT-01..10 en tests/
├── app/                   ommatix               raíz de composición y binario CLI (serve, demo)
└── acceptance/            ommatix-acceptance    ejecuta los escenarios de la tesis (tests/thesis.rs)
docs/
├── thesis/                el dossier: capítulos 00–12 en su raíz, contratos/, features/,
│                          matrices/, plantillas/, diagramas/, fuentes/, oráculo
├── adr/                   registros de decisiones (los del arranque en adr/bootstrap/)
├── architecture/          visión general (EN/ES)
├── diagrams/              diagramas Mermaid, etiquetas en inglés
└── reviews/               revisiones de código: hallazgo, corrección, prueba de regresión
tools/                     herramientas de medición y aceptación fuera del workspace (nunca
├── s0/                    enlazadas al binario): exportación del modelo (`--fp16`, `--dynamic`),
│                          frames sintéticos, configuraciones del operador
├── repro/                 `environment.py`: commit, toolchain, lockfiles, máquina y artefactos fijados (REL-01)
├── s3/, s4/               clientes MCP, la aplicación de prueba controlada (`s4/testapp`, con
│                          las banderas ocultas `--animate` de las mediciones de RNF-05) y el
│                          editor (`s4/testeditor`) y el cliente (`s4/e2e_dossier.py`) del
│                          E2E-02 y E2E-03 del dossier
└── perf/                  `measure_latency.py`: latencia por etapa contra la aplicación de prueba;
                           `soak.py` (PERF-03: recursos y soak de 2 h) y `dpi_probe.py` (IT-07: DPI
                           100/125/150 %), de sólo lectura, sin entrada; IT-07 se ejecutó el 15 de octubre de 2026, REL-01 aún no
```

Cómo se reparten los componentes del SDD del dossier
([capítulo 02 §2.3](../thesis/02_SDD_ARQUITECTURA_MVP.md)) entre los crates:

| Componente del SDD | Dónde |
|---|---|
| MCP Adapter | `ommatix-adapter-mcp` |
| Observation Coordinator, Policy & Action Gate | `Engine` en `ommatix-application` |
| Spatial Fusion, Temporal Registry | `ommatix-domain` (`fusion`, `identity`), dirigidos por el `Engine` |
| Icon Resolver | `ommatix-domain` (`appearance`), ejecutado por el `Engine` sobre los controles sin texto de las regiones planificadas con los prototipos curados del operador |
| Capture Provider | `ommatix-adapter-capture` (`FrameCapturer`) |
| Perception Provider | `ommatix-adapter-vision` (`VisionDetector`, `TextRecognizer`) |
| Accessibility Provider | `ommatix-adapter-accessibility` (`AccessibilityProvider`; opcional, apagado por defecto) |
| OS Actuator | `ommatix-adapter-input` (`OsActuator`) |
| Effect Verifier | la nueva captura del `Engine` tras la entrega (sólo el cambio visual); ver la arquitectura para sus límites |
| Telemetry | tiempos por etapa en el `_meta` de MCP; ver la arquitectura para sus límites |

Y cómo se reparte el paquete del dossier en el repositorio: capítulos, contratos, matrices,
plantillas (`plantillas/`: configuración y puertos), features, diagramas y oráculo están en
`docs/thesis/`; sus ADR, en `docs/adr/` en inglés; la edición de lectura (PDF y Word), el
manifiesto y la conversación original [F0] se conservan fuera del repositorio
([CHANGELOG 1.0.1](../thesis/CHANGELOG.md)).

## Comprobaciones

```bash
git config core.hooksPath .githooks     # once: fmt + clippy + tests before commit/push
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # unit, integration and Gherkin acceptance tests
cargo clippy -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia --all-targets -- -D warnings   # on Windows
cargo test -p ommatix --features dxgi,sendinput,onnx,winrt-ocr,uia                              # on Windows
```

Ninguna prueba envía entrada real: el actuador `SendInput` sólo se prueba con sus funciones
puras.

Las reglas de contribución, para personas y agentes de IA, están en [AGENTS.md](../../AGENTS.md).

## Investigación e historial

El nombre alude a los omatidios, unidades del ojo compuesto: la analogía del proyecto es
dividir la imagen en celdas y atender cambios locales. La tesis es *Percepción visual
incremental y grounding temporal seguro para agentes de escritorio: fusión espacial,
caché perceptual y control por identificadores mediante MCP*.

Los capítulos, requisitos, escenarios y pruebas planificadas se consultan en el
[índice del dossier](../thesis/README.md); el inventario vigente está en las
[matrices de trazabilidad](../thesis/matrices). El estado
de cada casilla está en [§6.7](../thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md#67-definition-of-done-del-producto),
y cada decisión o corrida anterior está en el [CHANGELOG](../thesis/CHANGELOG.md),
[S0](../thesis/S0_RESULTADOS.md), [S3](../thesis/S3_CORRIDAS_SUPERVISADAS.md) y
[S4](../thesis/S4_ACEPTACION_REDUCIDA.md). La lista de bloqueos actual está en §6.7;
las rondas antiguas no describen el estado actual.
