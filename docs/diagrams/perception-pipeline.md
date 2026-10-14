# Perception pipeline / Pipeline de percepción

*One `perceive` tool call. Hashing (step 5) reads each pixel once; only detection (steps
7–8) scales with the changed area. Nothing is committed until detection succeeds.*
*Una llamada a `perceive`. El hashing (paso 5) lee cada píxel una vez; solo la detección
(pasos 7–8) escala con el área cambiada. Nada se confirma hasta que la detección termina
bien.*

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
    E->>G: compare(frame) with the committed signature
    G-->>E: dirty cells → merged regions → expanded to a fixpoint
    loop each region
        E->>D: detect(frame, region)
        D-->>E: detections (clipped to the region)
    end
    E->>G: commit signature + scene.apply(regions, detections)
    G-->>E: SceneDelta (added / updated / removed, stable ids)
    E-->>M: PerceptionReport (+ per-stage timings)
    M-->>A: structuredContent JSON (timings in _meta)
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
    participant X as AgentSession
    participant U as ActuationService
    participant I as InputController
    participant O as Observer
    participant E as PerceptionEngine (ScreenModel)

    A->>M: tools/call click {element_id: "e4"}
    M->>X: click(Element(e4), Left, observe = true)
    X->>U: click(screen = E, Element(e4), Left)
    U->>E: screen_bounds(), element(e4)
    E-->>U: last frame bounds, element bounds
    U->>U: validate (perceived? exists? on screen?)
    U->>I: perform(Click at center)
    Note right of I: native input maps frame pixels<br/>to OS coordinates (DisplayMapping)
    I-->>U: ok
    U-->>X: ActionReceipt (performed)
    X->>O: observe(E)
    loop every 16 ms until settled (2 unchanged frames, ≥ 100 ms) or 1 s
        O->>E: tick()
        E-->>O: PerceptionReport
    end
    O-->>X: Observation (net change, settled)
    X-->>M: ActionOutcome
    M-->>A: {performed, observation} or {performed, observe_error}
```
