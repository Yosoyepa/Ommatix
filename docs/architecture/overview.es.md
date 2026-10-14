# Visión general de la arquitectura

🇬🇧 [English version](overview.md)

## Problema

Los agentes de uso de computador suelen enviar capturas completas de pantalla a un modelo
de visión-lenguaje en cada paso. Eso es lento (segundos), costoso y produce coordenadas
aproximadas. Sin embargo, entre dos pasos la mayor parte de la pantalla no cambia.

## Enfoque

Ommatix separa la **percepción** del **razonamiento**:

- **Percepción (Ommatix, Rust):** incremental, determinista, en milisegundos. Mantiene un
  modelo actualizado e indexado espacialmente de lo que hay en pantalla.
- **Razonamiento (el agente, vía MCP):** decide qué hacer y le pregunta a Ommatix *dónde*
  están las cosas.

Como un ojo compuesto, la pantalla se divide en celdas independientes (*omatidios*). En cada
tick se calcula la huella de cada celda y solo se re-analizan las celdas cuya huella cambió.

## Componentes

Ver el [diagrama hexagonal](../diagrams/hexagon.md).

| Crate | Responsabilidad | Tipos clave |
|---|---|---|
| `ommatix-domain` | Geometría, firmas de cuadrícula, fusión de regiones, escena con seguimiento de identidad, índice espacial | `Frame`, `GridSpec`, `GridSignature`, `Scene`, `SpatialIndex`, `Element` |
| `ommatix-application` | Orquestación del caso de uso; puertos hacia el exterior | `PerceptionEngine`, `FrameSource`, `ElementDetector`, `PerceptionReport` |
| `ommatix-adapter-capture` | Fuentes de frames | `ScriptedSource`, `ImageSequenceSource`, `ScreenSource` |
| `ommatix-adapter-vision` | Detección de elementos | `ContrastBlobDetector` |
| `ommatix-adapter-mcp` | Protocolo hacia el agente | `McpServer`, DTOs de herramientas |
| `ommatix` | Raíz de composición, CLI (`serve`, `demo`) | `main.rs` |

## Un tick

Ver la [secuencia del pipeline](../diagrams/perception-pipeline.md) y el
[ADR-0005](../adr/0005-grid-incremental-perception.md).

1. Capturar un frame.
2. Calcular el hash de cada celda (FNV-1a) y compararlo con la firma anterior.
3. Fusionar celdas sucias en regiones; expandir regiones a elementos conocidos completos.
4. Ejecutar el detector solo dentro de las regiones.
5. Actualizar la escena: re-identificar elementos por IoU, añadir nuevos, eliminar los que
   desaparecieron.
6. Reportar el delta y los tiempos por etapa.

## Atributos de calidad

| Atributo | Cómo se aborda |
|---|---|
| Latencia | Trabajo proporcional al área cambiada; nada costoso fuera de las regiones; tiempos por etapa en cada reporte |
| Determinismo | Hash FNV, colecciones ordenadas, ordenamiento explícito, detector determinista |
| Modificabilidad | Puertos y adaptadores; límites impuestos por las dependencias entre crates |
| Testabilidad | Frames sintéticos; pruebas MCP de extremo a extremo sin stdio |
| Privacidad | Los frames no se guardan por defecto; las capturas están en `.gitignore` |

## Limitaciones actuales (MVP)

- El detector reporta solo geometría (`kind = blob`), sin texto.
- Aún no hay actuación (clic, escribir): el agente debe usar otra herramienta para actuar.
- Un solo hilo; la detección por región es el punto natural de paralelización.
