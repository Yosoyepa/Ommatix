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
| `ommatix-domain` | Geometría, firmas de cuadrícula, seguimiento de cambios, fusión de regiones, escena con seguimiento de identidad, índice espacial, mapeo de pantalla | `Frame`, `ChangeTracker`, `Scene`, `SpatialIndex`, `Element`, `DisplayMapping` |
| `ommatix-application` | Casos de uso y puertos hacia el exterior | `PerceptionEngine`, `Observer`, `ActuationService`, `AgentSession`; puertos `FrameSource`, `ElementDetector`, `InputController`, `Clock` |
| `ommatix-adapter-capture` | Fuentes de frames | `ScriptedSource`, `ImageSequenceSource`, `ScreenSource` |
| `ommatix-adapter-vision` | Detección de elementos | `FlatRegionDetector` |
| `ommatix-adapter-input` | Entrada al escritorio | `DryRunInput`, `NativeInput` |
| `ommatix-adapter-sim` | Escritorio simulado para pruebas y demos (ambos puertos), temas, sesión grabada | `SimulatedDesktop`, `Theme`, `scripted_session` |
| `ommatix-adapter-mcp` | Protocolo hacia el agente: sobre JSON-RPC, enrutamiento MCP, registro de herramientas, DTOs | `McpServer` |
| `ommatix` | Raíz de composición, CLI (`serve`, `demo`) | `main.rs` |

## Un tick

Ver la [secuencia del pipeline](../diagrams/perception-pipeline.md), el
[ADR-0005](../adr/0005-grid-incremental-perception.md) y el
[ADR-0011](../adr/0011-incremental-perception-invariants.md).

1. Capturar un frame.
2. Calcular el hash de cada celda (8 bytes a la vez) y compararlo con la última firma
   confirmada. Un tamaño de frame nuevo implica una escena nueva.
3. Fusionar celdas sucias en regiones y crecerlas hasta cubrir elementos completos hasta
   que nada cambie, sin absorber contenedores que encierran el cambio (la ventana alrededor
   de un botón).
4. Ejecutar el detector solo dentro de las regiones
   ([ADR-0010](../adr/0010-flat-region-detector.md)) y recortar lo que devuelva a la región.
5. Actualizar la escena: emparejar elementos viejos y nuevos por mejor IoU, añadir los
   nuevos y eliminar los que desaparecieron. Solo entonces se confirma el frame, así que una
   detección fallida se reintenta en lugar de perderse.
6. Reportar el delta, los píxeles analizados y los tiempos por etapa.

## Actuar

Ver el [ADR-0008](../adr/0008-actuation-port-dry-run-default.md) y el
[ADR-0012](../adr/0012-settled-actions-and-wire-contract.md). `ActuationService` valida
cada petición contra el último frame percibido (objetivo dentro de la pantalla, id de
elemento existente, longitud del texto, sin caracteres de control) y solo entonces llama
al `InputController` inyectado. Después el `Observer` percibe hasta que la pantalla se
estabiliza, así que el agente recibe el efecto ya asentado de su acción en la misma
respuesta; si observar falla, la respuesta igual indica que la acción se realizó. Los
clics se convierten de píxeles del frame a coordenadas del sistema (HiDPI, varios
monitores). La entrada es dry-run salvo que el binario se compile con `native-input` y se
inicie con `--source screen --input native`.

## Especificación de aceptación

Ver el [ADR-0009](../adr/0009-gherkin-acceptance-through-mcp.md). El MVP está especificado
en Gherkin (`src/acceptance/features/`) y se ejecuta con `cucumber` a través del adaptador
MCP sobre un escritorio simulado que reacciona a la entrada.

## Atributos de calidad

| Atributo | Cómo se aborda |
|---|---|
| Latencia | Trabajo proporcional al área cambiada (`analysed_px` en cada reporte); algoritmos casi lineales; tiempos por etapa en `_meta` |
| Determinismo | Hash determinista, colecciones ordenadas, ordenamiento explícito, reloj inyectado; conversaciones idénticas dan respuestas idénticas byte a byte |
| Modificabilidad | Puertos y adaptadores; límites impuestos por las dependencias entre crates |
| Testabilidad | Puertos inyectados (incluido el tiempo); escritorio simulado con temas; pruebas MCP de extremo a extremo sin stdio |
| Privacidad | Los frames no se guardan por defecto; las capturas están en `.gitignore` |

## Limitaciones actuales (MVP)

- El detector reporta solo geometría (`kind = blob`), sin texto; los controles de bajo
  contraste no se ven.
- La entrada real solo se ha ejercitado con los adaptadores dry-run y simulado.
- Cambiar la configuración de pantalla durante una sesión requiere reiniciar.
- Un solo hilo; un análisis completo 1080p toma ~0,1 s, los frames siguientes solo pagan
  por lo que cambia.

La [revisión del MVP](../reviews/2026-10-14-mvp-review.md) lista los defectos encontrados y
cómo se prueba cada uno.
