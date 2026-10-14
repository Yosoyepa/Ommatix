# Architecture overview

🇪🇸 [Versión en español](overview.es.md)

## Problem

Computer-use agents typically send full screenshots to a vision-language model on every
step. That is slow (seconds), expensive, and yields approximate coordinates. Yet between
two steps, most of the screen does not change.

## Approach

Ommatix separates **perception** from **reasoning**:

- **Perception (Ommatix, Rust):** incremental, deterministic, millisecond-scale. Maintains
  an up-to-date, spatially indexed model of what is on screen.
- **Reasoning (the agent, via MCP):** decides what to do, asks Ommatix *where* things are.

Like a compound eye, the screen is divided into independent cells (*ommatidia*). Each tick
fingerprints every cell, and only cells whose fingerprint changed are re-analysed.

## Components

See the [hexagon diagram](../diagrams/hexagon.md).

| Crate | Responsibility | Key types |
|---|---|---|
| `ommatix-domain` | Geometry, grid signatures, region merging, scene with identity tracking, spatial index | `Frame`, `GridSpec`, `GridSignature`, `Scene`, `SpatialIndex`, `Element` |
| `ommatix-application` | Use case orchestration; ports to the outside | `PerceptionEngine`, `FrameSource`, `ElementDetector`, `PerceptionReport` |
| `ommatix-adapter-capture` | Frame sources | `ScriptedSource`, `ImageSequenceSource`, `ScreenSource` |
| `ommatix-adapter-vision` | Element detection | `ContrastBlobDetector` |
| `ommatix-adapter-mcp` | Agent-facing protocol | `McpServer`, tool DTOs |
| `ommatix` | Composition root, CLI (`serve`, `demo`) | `main.rs` |

## One tick

See the [pipeline sequence](../diagrams/perception-pipeline.md) and
[ADR-0005](../adr/0005-grid-incremental-perception.md).

1. Capture a frame.
2. Hash each cell (FNV-1a); diff against the previous signature.
3. Merge dirty cells into regions; expand regions to whole known elements.
4. Run the detector inside regions only.
5. Update the scene: re-identify elements by IoU, add new ones, remove vanished ones.
6. Report the delta and per-stage timings.

## Quality attributes

| Attribute | How it is addressed |
|---|---|
| Latency | Work proportional to changed area; no I/O or allocation-heavy work outside regions; per-stage timings in every report |
| Determinism | FNV hashing, ordered collections, explicit sorting, deterministic detector |
| Modifiability | Ports and adapters; boundaries enforced by crate dependencies |
| Testability | Synthetic frames; end-to-end MCP tests without stdio |
| Privacy | Frames never persisted by default; captures git-ignored |

## Current limitations (MVP)

- Detector reports geometry only (`kind = blob`), no text.
- No input actuation yet: the agent must click through other tooling.
- Single-threaded; detection per region is a natural parallelisation point.
