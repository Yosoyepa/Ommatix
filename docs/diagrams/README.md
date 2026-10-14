# Diagrams

All diagrams are [Mermaid](https://mermaid.js.org) inside Markdown, so GitHub renders them and
diffs stay reviewable. Labels are in English and captions are bilingual. The first four are
the thesis views of [`docs/thesis/diagramas/`](../thesis/diagramas/README.md), whose Spanish
sources stay canonical; the hexagon shows how ADR-0014 lays those components out in crates,
and the engine view shows the code as of S2b.

| Diagram | Shows |
|---|---|
| [context.md](context.md) | System context: operator, local policy, agent, engine, desktop |
| [components.md](components.md) | The engine's components, from capture to effect verification |
| [action-sequence.md](action-sequence.md) | `observe_screen` followed by `execute_action`, with its three outcomes |
| [snapshot-state.md](snapshot-state.md) | Lifecycle of the published snapshot, including the degraded state |
| [hexagon.md](hexagon.md) | Crates, ports and allowed dependencies |
| [engine.md](engine.md) | What the implemented `Engine` does for `observe_screen` and `execute_action`, port by port |
