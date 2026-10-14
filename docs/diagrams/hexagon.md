# Hexagonal architecture / Arquitectura hexagonal

*Arrows point in the direction of the code dependency. Nothing points out of the domain.*
*Las flechas indican la dirección de la dependencia. Nada sale del dominio.*

```mermaid
flowchart TB
    subgraph app["ommatix (src/app) — composition root · SystemClock"]
      direction TB
      subgraph driving["Driving adapters"]
        mcp["ommatix-adapter-mcp<br/>MCP stdio server"]
      end
      subgraph core["Core"]
        appl["ommatix-application<br/>PerceptionEngine · Observer · ActuationService · AgentSession<br/>ports: FrameSource, ElementDetector,<br/>InputController, Clock"]
        dom["ommatix-domain<br/>Frame · ChangeTracker · Scene · SpatialIndex<br/>Action · DisplayMapping"]
      end
      subgraph driven["Driven adapters"]
        cap["ommatix-adapter-capture<br/>Scripted · ImageSequence · Screen(xcap)"]
        vis["ommatix-adapter-vision<br/>FlatRegionDetector"]
        inp["ommatix-adapter-input<br/>DryRunInput · NativeInput(enigo)"]
        sim["ommatix-adapter-sim<br/>SimulatedDesktop (themes)"]
      end
    end
    mcp --> appl
    appl --> dom
    cap -->|implements FrameSource| appl
    vis -->|implements ElementDetector| appl
    inp -->|implements InputController| appl
    sim -->|implements FrameSource + InputController| appl
    cap --> dom
    vis --> dom
    inp --> dom
    sim --> dom
```
