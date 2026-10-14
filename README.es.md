# Ommatix

**Indexación Espacial y Percepción Incremental para Agentes de Interfaz Gráfica**

🇬🇧 [Read in English](README.md)

Ommatix es un motor de percepción determinista y de baja latencia que permite a agentes
basados en LLM operar un escritorio gráfico. En lugar de enviar cada captura de pantalla a
un modelo de visión lento, Ommatix divide el framebuffer en una cuadrícula de celdas
independientes, vuelve a analizar **solo las celdas que cambiaron**, mantiene cada elemento
detectado en un índice espacial y responde al agente con **coordenadas exactas en píxeles**
a través del [Model Context Protocol](https://modelcontextprotocol.io).

> **¿Por qué "Ommatix"?** El sistema recibe el nombre de Ommatix en referencia a los
> omatidios (*ommatidia*), las unidades fotorreceptoras independientes que conforman el ojo
> compuesto de insectos de vuelo rápido. De forma análoga a cómo cada omatidio procesa
> cambios locales de luminancia en paralelo sin saturar el sistema nervioso central, Ommatix
> divide el framebuffer de la pantalla en una matriz espacial de celdas independientes,
> procesando únicamente las regiones con actividad visual y entregando coordenadas
> deterministas al agente.

## Estado

`v0.1.0`: **bootstrap del MVP.** El pipeline completo funciona de extremo a extremo con
frames sintéticos y grabados; la captura de pantalla en vivo está detrás de un feature flag.
Ver la [hoja de ruta](#hoja-de-ruta).

## Cómo funciona

```mermaid
flowchart LR
    A[Fuente de frames] --> B[Firma de cuadrícula<br/>hash por celda]
    B -->|diff vs anterior| C[Celdas sucias]
    C --> D[Regiones fusionadas<br/>+ expandidas a elementos tocados]
    D --> E[Detector de elementos<br/>solo dentro de regiones]
    E --> F[Escena + índice espacial<br/>ids estables]
    F -->|herramientas MCP| G[Agente LLM]
```

Una pantalla estática cuesta una pasada de hashing y nada más; escribir en un campo de
texto vuelve a analizar 3 de 260 celdas en la sesión de demo. Más detalle en
[docs/architecture](docs/architecture/overview.es.md).

## Inicio rápido

Requiere Rust estable (fijado en `rust-toolchain.toml`).

```bash
cargo run -- demo                       # sesión de escritorio sintética, una línea por frame
cargo run -- serve                      # servidor MCP por stdio (fuente sintética)
cargo run -- serve --source images --images ./captures/sesion-01    # reproducir PNGs
cargo run --features screen -- serve --source screen                # monitor principal en vivo
```

La captura en vivo en Linux necesita las librerías de sistema listadas en
[AGENTS.md](AGENTS.md#toolchain).

### Uso desde un cliente MCP

```json
{
  "mcpServers": {
    "ommatix": { "command": "/ruta/a/ommatix", "args": ["serve", "--source", "screen"] }
  }
}
```

| Herramienta | Propósito |
|---|---|
| `perceive` | Captura un frame, re-analiza regiones cambiadas y devuelve elementos añadidos / actualizados / eliminados |
| `list_elements` | Todos los elementos en orden de lectura, con punto de clic |
| `element_at` | Elementos bajo `(x, y)`, el más específico primero |
| `elements_in_rect` | Elementos que intersectan un rectángulo |

## Estructura del repositorio

```text
src/                       Miembros del workspace de Cargo (monolito modular)
├── domain/                ommatix-domain        núcleo puro: frame, cuadrícula, regiones, escena, índice
├── application/           ommatix-application   puertos (traits) + caso de uso de percepción
├── adapters/
│   ├── capture/           ommatix-adapter-capture  FrameSource: guion, secuencia PNG, pantalla
│   ├── vision/            ommatix-adapter-vision   ElementDetector: detector por contraste
│   └── mcp/               ommatix-adapter-mcp      servidor MCP stdio (adaptador conductor)
└── app/                   ommatix               raíz de composición + binario CLI
docs/
├── adr/                   registros de decisiones de arquitectura
├── architecture/          visión general (EN/ES)
└── diagrams/              diagramas Mermaid (contexto, hexágono, pipeline)
```

## Desarrollo

```bash
git config core.hooksPath .githooks     # una vez: fmt + clippy + tests antes de commit/push
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Las reglas de contribución (para personas y agentes de IA) están en [AGENTS.md](AGENTS.md).

## Hoja de ruta

- [x] Diff por cuadrícula, fusión de regiones, índice espacial, identidad estable de elementos
- [x] Detector determinista por contraste
- [x] Servidor MCP (stdio) con herramientas de percepción
- [x] Fuentes sintética, secuencia PNG y pantalla en vivo
- [ ] Benchmarks (criterion) y presupuesto de latencia por etapa
- [ ] Adaptador de detección de texto / OCR, clasificación de elementos (botón, texto, icono)
- [ ] Herramientas de actuación (clic, escribir) detrás de un puerto `InputController`
- [ ] Detección paralela por región y hashing SIMD
- [ ] Banco de evaluación con sesiones GUI grabadas

## Documentación

- [Visión general de la arquitectura](docs/architecture/overview.es.md) · [Architecture overview](docs/architecture/overview.md)
- [Registros de decisiones (ADR)](docs/adr/README.md)
- [Diagramas](docs/diagrams/README.md)
- [Glosario / Glossary](docs/glossary.md)
