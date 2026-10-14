# Hexagonal architecture / Arquitectura hexagonal

*Target layout from ADR-0014. Arrows point in the direction of the code dependency; nothing
points out of the domain.*
*Disposición objetivo según ADR-0014. Las flechas indican la dirección de la dependencia;
nada sale del dominio.*

```mermaid
flowchart TB
    subgraph app["ommatix (src/app): composition root and CLI"]
      direction TB
      subgraph driving["Driving adapter"]
        mcp["ommatix-adapter-mcp<br/>MCP profiles · wire DTOs"]
      end
      subgraph core["Core"]
        appl["ommatix-application<br/>Observation coordinator · Policy and action gate · Effect verifier<br/>ports: FrameCapturer, VisionDetector, TextRecognizer,<br/>OsActuator, Clock, PolicyStore"]
        dom["ommatix-domain<br/>Tiles and regions (Alg. 1) · Spatial fusion (Alg. 2)<br/>Icon descriptor (Alg. 3A) · Temporal registry (Alg. 3B) · Snapshots"]
      end
      subgraph driven["Driven adapters"]
        cap["ommatix-adapter-capture<br/>DXGI Desktop Duplication"]
        vis["ommatix-adapter-vision<br/>ONNX Runtime: detector and OCR"]
        inp["ommatix-adapter-input<br/>SendInput"]
        sim["ommatix-adapter-sim<br/>deterministic fixtures"]
      end
    end
    mcp --> appl
    appl --> dom
    cap -->|implements FrameCapturer| appl
    vis -->|implements VisionDetector, TextRecognizer| appl
    inp -->|implements OsActuator| appl
    sim -->|implements fixtures for the ports| appl
    cap --> dom
    vis --> dom
    inp --> dom
    sim --> dom
```

Today these crates still hold the bootstrap prototype: capture with `xcap`, a flat-region
detector, input with `enigo`, and the ports `FrameSource`, `ElementDetector` and
`InputController`. Stages S1–S3 of the thesis plan move them to this shape.

Hoy estos crates contienen todavía el prototipo del arranque: captura con `xcap`, un detector
de regiones planas, entrada con `enigo` y los puertos `FrameSource`, `ElementDetector` e
`InputController`. Las etapas S1–S3 del plan de la tesis los llevan a esta forma.
