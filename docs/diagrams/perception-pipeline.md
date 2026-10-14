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

## Act and observe / Actuar y observar

```mermaid
sequenceDiagram
    autonumber
    participant A as Agent
    participant M as adapter-mcp
    participant U as ActuationService
    participant E as PerceptionEngine (ScreenModel)
    participant I as InputController

    A->>M: tools/call click {element_id: "e4"}
    M->>U: click(screen = E, Element(e4), Left)
    U->>E: screen_bounds(), element(e4)
    E-->>U: last frame bounds, element bounds
    U->>U: validate (perceived? on screen? exists?)
    U->>I: perform(Click at center)
    I-->>U: ok
    U-->>M: ActionReceipt {live}
    M->>E: tick()  (observe = true)
    E-->>M: PerceptionReport (what changed)
    M-->>A: {performed, observed}
```
