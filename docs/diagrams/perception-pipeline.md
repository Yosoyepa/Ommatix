# Perception pipeline / Pipeline de percepción

*One `perceive` tool call. Only steps 5–6 scale with the changed area.*
*Una llamada a `perceive`. Solo los pasos 5–6 escalan con el área cambiada.*

```mermaid
sequenceDiagram
    autonumber
    participant A as Agent (MCP client)
    participant M as adapter-mcp
    participant E as PerceptionEngine
    participant S as FrameSource
    participant G as Domain (grid, scene)
    participant D as ElementDetector

    A->>M: tools/call perceive
    M->>E: tick()
    E->>S: next_frame()
    S-->>E: Frame (RGBA8)
    E->>G: signature(frame) + diff(previous)
    G-->>E: dirty cells → merged regions → expanded regions
    loop each dirty region
        E->>D: detect(frame, region)
        D-->>E: detections
    end
    E->>G: scene.apply(regions, detections)
    G-->>E: SceneDelta (added / updated / removed, stable ids)
    E-->>M: PerceptionReport (+ per-stage timings)
    M-->>A: structuredContent JSON
    A->>M: tools/call element_at {x, y}
    M->>E: elements_at(point)
    E-->>M: elements (smallest first)
    M-->>A: bounds + click center
```
