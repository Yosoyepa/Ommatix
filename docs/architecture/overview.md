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
| `ommatix-domain` | Geometry, grid signatures, change tracking, region merging, scene with identity tracking, spatial index, display mapping | `Frame`, `ChangeTracker`, `Scene`, `SpatialIndex`, `Element`, `DisplayMapping` |
| `ommatix-application` | Use cases and ports to the outside | `PerceptionEngine`, `Observer`, `ActuationService`, `AgentSession`; ports `FrameSource`, `ElementDetector`, `InputController`, `Clock` |
| `ommatix-adapter-capture` | Frame sources | `ScriptedSource`, `ImageSequenceSource`, `ScreenSource` |
| `ommatix-adapter-vision` | Element detection | `FlatRegionDetector` |
| `ommatix-adapter-input` | Desktop input | `DryRunInput`, `NativeInput` |
| `ommatix-adapter-sim` | Simulated desktop for tests and demos (both ports), themes, recorded session | `SimulatedDesktop`, `Theme`, `scripted_session` |
| `ommatix-adapter-mcp` | Agent-facing protocol: JSON-RPC envelope, MCP routing, tool registry, wire DTOs | `McpServer` |
| `ommatix` | Composition root, CLI (`serve`, `demo`) | `main.rs` |

## One tick

See the [pipeline sequence](../diagrams/perception-pipeline.md),
[ADR-0005](../adr/0005-grid-incremental-perception.md) and
[ADR-0011](../adr/0011-incremental-perception-invariants.md).

1. Capture a frame.
2. Hash each cell (8 bytes at a time); compare with the last committed signature. A new
   frame size means a new scene.
3. Merge dirty cells into regions; grow them to whole elements until nothing changes,
   leaving out containers that strictly enclose a change (a window around a button).
4. Run the detector inside regions only ([ADR-0010](../adr/0010-flat-region-detector.md))
   and clip what it returns to the region.
5. Update the scene: pair old and new elements by best IoU, add new ones, remove vanished
   ones. Only now is the frame committed, so a failed detection is retried, not lost.
6. Report the delta, the pixels analysed and per-stage timings.

## Acting

See [ADR-0008](../adr/0008-actuation-port-dry-run-default.md) and
[ADR-0012](../adr/0012-settled-actions-and-wire-contract.md). `ActuationService` checks
each request against the last perceived frame (target on screen, element id exists, text
length, no control characters) and only then calls the injected `InputController`. The
`Observer` then perceives until the screen settles, so the agent receives the settled
effect of its action in the same response; if watching fails, the response still says
the action was performed. Clicks are converted from frame pixels to OS coordinates
(HiDPI, multi-monitor). Input is dry-run unless the binary is built with `native-input`
and started with `--source screen --input native`.

## Acceptance specification

See [ADR-0009](../adr/0009-gherkin-acceptance-through-mcp.md). The MVP is specified in
Gherkin (`src/acceptance/features/`) and executed with `cucumber` through the MCP adapter
against a simulated desktop that reacts to input.

## Quality attributes

| Attribute | How it is addressed |
|---|---|
| Latency | Work proportional to changed area (`analysed_px` in every report); near-linear algorithms; per-stage timings in `_meta` |
| Determinism | Deterministic hashing, ordered collections, explicit sorting, injected clock; identical conversations give byte-identical answers |
| Modifiability | Ports and adapters; boundaries enforced by crate dependencies |
| Testability | Injected ports (including time); simulated desktop with themes; end-to-end MCP tests without stdio |
| Privacy | Frames never persisted by default; captures git-ignored |

## Current limitations (MVP)

- Detector reports geometry only (`kind = blob`), no text; low-contrast controls are not
  seen.
- Live input has only been exercised through the dry-run and simulated adapters.
- Display settings changed during a session need a restart.
- Single-threaded; a full 1080p analysis takes ~0.1 s, later frames only pay for changes.

The [MVP review](../reviews/2026-10-14-mvp-review.md) lists the defects found and how each
is now tested.
