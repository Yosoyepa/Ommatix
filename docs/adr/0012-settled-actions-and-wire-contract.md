# 0012. Actions wait for the screen to settle; the wire contract is owned by the adapter

- **Status:** Accepted (amends [0004](0004-mcp-stdio-agent-interface.md) and
  [0008](0008-actuation-port-dry-run-default.md))
- **Date:** 2026-10-14

## Context

The [MVP review](../reviews/2026-10-14-mvp-review.md) found that, after acting:

- the engine perceived exactly one frame, so an application that redraws a few
  milliseconds later looked unchanged to the agent;
- if that perception failed, the tool reported an error although the click had happened,
  inviting the agent to click again;
- frame pixels were sent to the OS as-is, which is wrong on HiDPI screens (macOS input is
  in points) and on secondary monitors (they have an origin);
- domain types were serialised directly as the MCP payload, and per-stage timings made
  every payload different between runs.

## Decision

1. **Time is a port.** `Clock` (`now`, `sleep`) is injected into the perception engine and
   the observer. Production uses the system clock; tests and simulations use
   `ManualClock`, where sleeping advances virtual time instantly.
2. **Settled observation.** `Observer` perceives until the screen shows no changed cell for
   `stable_frames` (2) consecutive frames and at least `min_wait` (100 ms) has passed, or
   until `timeout` (1 s), polling every 16 ms. The result is the net change
   (`SceneDelta::then`) and whether the screen `settled`. Policies that could spin forever
   are rejected.
3. **A performed action is never a failure.** `AgentSession` performs, then observes. If
   observing fails, the MCP result still succeeds with `performed` and an
   `observe_error` field. Invalid requests are rejected before anything is performed.
4. **Coordinate mapping.** `DisplayMapping` (domain) maps frame pixels to OS input
   coordinates: `origin + p · os_size / frame_size`. The screen source measures it on the
   monitor it captures; the native input adapter applies it. `--input native` is refused
   unless `--source screen`, so clicks always land on the screen the agent sees.
5. **Wire contract.** Every MCP payload is a DTO of the MCP adapter (no `serde` in domain or
   application). Run-dependent measurements (`ommatix/timings_us`, `ommatix/elapsed_us`)
   travel in the result's `_meta`; `structuredContent` is reproducible. The adapter is split
   into JSON-RPC envelope handling (batches, invalid requests, invalid UTF-8 answered with
   errors instead of stopping), MCP method routing, and a registry of tools.
6. `type_text` rejects control characters; Enter, Tab and Backspace are `press_key`.

## Consequences

- One tool call gives the agent the settled result of its action; deterministic tests stay
  instant thanks to the virtual clock.
- A spinner or video never settles: the observation ends at the timeout with
  `settled: false`, and the agent can decide.
- Display settings changed during a session are not picked up until restart.

## Alternatives considered

- **Fixed sleep after each action** — either too slow or too short; settling adapts.
- **Report observation failures as tool errors** — makes agents retry actions that already
  happened (double submits).
- **Let each input adapter guess the scale** — the scale is a property of the captured
  monitor, known only where the frame is captured.

## Resumen (ES)

Tras cada acción, el `Observer` percibe hasta que la pantalla se estabiliza (sin celdas
cambiadas durante 2 frames y al menos 100 ms, con límite de 1 s), usando un reloj
inyectado que en pruebas es virtual. Una acción realizada nunca se informa como fallo: si
observar falla, la respuesta incluye `performed` y `observe_error`. Las coordenadas del
frame se convierten a coordenadas del sistema (HiDPI, multi-monitor) y la entrada nativa
solo se permite capturando la pantalla real. El adaptador MCP define sus propios DTOs y
envía las medidas de tiempo en `_meta`, para que las respuestas sean reproducibles.
