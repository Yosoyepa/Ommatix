# Hexagonal architecture / Arquitectura hexagonal

*Arrows point in the direction of the code dependency. Nothing points out of the domain.*
*Las flechas indican la dirección de la dependencia. Nada sale del dominio.*

```mermaid
flowchart TB
    subgraph app["ommatix (src/app) — composition root"]
      direction TB
      subgraph driving["Driving adapters"]
        mcp["ommatix-adapter-mcp<br/>MCP stdio server"]
      end
      subgraph core["Core"]
        appl["ommatix-application<br/>PerceptionEngine<br/>ports: FrameSource, ElementDetector"]
        dom["ommatix-domain<br/>Frame · GridSpec · Scene · SpatialIndex"]
      end
      subgraph driven["Driven adapters"]
        cap["ommatix-adapter-capture<br/>Scripted · ImageSequence · Screen(xcap)"]
        vis["ommatix-adapter-vision<br/>ContrastBlobDetector"]
      end
    end
    mcp --> appl
    appl --> dom
    cap -->|implements FrameSource| appl
    vis -->|implements ElementDetector| appl
    cap --> dom
    vis --> dom
```
