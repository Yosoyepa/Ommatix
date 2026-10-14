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

Its [verification pass](../reviews/2026-10-14-mvp-review.md#verification-pass) added:

- on Linux X11 with a display scale (`Xft.dpi` 192), clicks landed at half the distance
  from the screen origin: the capture library reports the monitor in scaled units, while
  capture and input use physical pixels;
- the screen source served the frame captured at start-up forever;
- a failure in the middle of an observation lost the changes of the frames already seen;
- a `tools/call` without an id was performed and never answered, and array arguments were
  read positionally.

## Decision

1. **Time is a port.** `Clock` (`now`, `sleep`) is injected into the perception engine and
   the observer. Production uses the system clock; tests and simulations use
   `ManualClock`, where sleeping advances virtual time instantly.
2. **Settled observation.** `Observer` perceives until the screen shows no changed cell for
   `stable_frames` (2) consecutive frames and at least `min_wait` (100 ms) has passed, or
   until `timeout` (1 s), polling every 16 ms. The result is the net change
   (`SceneDelta::then`) and whether the screen `settled`. Policies that could spin forever
   are rejected, and so is a `timeout` above 60 s.
3. **A performed action is never a failure.** `AgentSession` performs, then observes. If
   observing fails, the MCP result still succeeds with `performed` and an
   `observe_error` field, plus the `observation` of the frames seen before the failure:
   their changes are already in the scene and would otherwise never be reported. Invalid
   requests are rejected before anything is performed.
4. **Coordinate mapping.** `DisplayMapping` (domain) maps frame pixels to OS input
   coordinates: `origin + p · os_size / frame_size`. The screen source derives it from the
   monitor it captures, per platform, because capture and input disagree on units:
   - macOS: input is in points, frames are in pixels; the monitor's reported geometry is
     in points.
   - Linux X11: input (XTest) is in physical pixels, the frame's own units; the reported
     origin is divided by `Xft.dpi / 96`, so it is scaled back.
   - Windows: input is virtualised by the display scale for a process that is not
     DPI-aware, so only 100% is supported.
   - Wayland sessions and Windows at another scale: not verified, so `--input native`
     fails at start-up and says why, instead of guessing where clicks land.

   The native input adapter applies the mapping. `--input native` is refused unless
   `--source screen`, so clicks always land on the screen the agent sees. The source
   captures a fresh frame on every tick and stops with an error if the monitor's size
   changes, since the mapping was measured at start-up.
5. **Wire contract.** Every MCP payload is a DTO of the MCP adapter (no `serde` in domain or
   application). Run-dependent measurements (`ommatix/timings_us`, `ommatix/elapsed_us`)
   travel in the result's `_meta`. `structuredContent` is a function of the frames seen:
   the same frame sequence gives byte-identical payloads. With the real clock, the number
   of frames an observation takes depends on timing, so frame counters (`frames`,
   `frame_no`, `first_seen`) can differ between live runs. The adapter is split into
   JSON-RPC envelope handling (batches, invalid requests, invalid UTF-8 answered with
   errors instead of stopping), MCP method routing, and a registry of tools.
6. **Strict input.** A request without an id (a notification) is never executed: it
   cannot be answered, and an action no one hears about must not happen. Tool arguments
   are JSON objects, never positional, and an element id has one form (`e12`, not
   `e012`). `type_text` rejects control characters and invisible layout characters (line
   and paragraph separators, bidirectional marks, overrides and isolates); Enter, Tab and
   Backspace are `press_key`.

## Consequences

- One tool call gives the agent the settled result of its action; deterministic tests stay
  instant thanks to the virtual clock.
- A spinner or video never settles: the observation ends at the timeout with
  `settled: false`, and the agent can decide.
- Changing the resolution or the display scale during a session stops capture with an
  error asking for a restart.
- Native input works on macOS, Linux X11 (clicks checked under Xvfb with `Xft.dpi`
  unset, 144 and 192) and Windows at 100% scale. Wayland and scaled Windows need their own
  verified mapping. On X11 HiDPI, a secondary monitor's origin is recovered from scaled
  units, so it can be off by less than the scale factor (one pixel at 2×) when it is not a
  multiple of it.

## Alternatives considered

- **Fixed sleep after each action** — either too slow or too short; settling adapts.
- **Report observation failures as tool errors** — makes agents retry actions that already
  happened (double submits).
- **Let each input adapter guess the scale** — the scale is a property of the captured
  monitor, known only where the frame is captured.

## Resumen (ES)

Tras cada acción, el `Observer` percibe hasta que la pantalla se estabiliza (sin celdas
cambiadas durante 2 frames y al menos 100 ms, con límite de 1 s y nunca más de 60 s),
usando un reloj inyectado que en pruebas es virtual. Una acción realizada nunca se informa
como fallo: si observar falla, la respuesta incluye `performed`, `observe_error` y lo que
se alcanzó a observar. Las coordenadas del frame se convierten a coordenadas del sistema
según la plataforma (puntos en macOS, píxeles físicos en X11, Windows solo al 100%); en
Wayland y en Windows escalado la entrada nativa se rechaza al arrancar en lugar de
adivinar dónde cae el clic. La fuente de pantalla captura un frame nuevo en cada tick y se
detiene con un error si cambia la resolución. El adaptador MCP define sus propios DTOs y
envía las medidas de tiempo en `_meta`: la misma secuencia de frames da respuestas
idénticas. Una petición sin id nunca se ejecuta, los argumentos deben ser objetos y
`type_text` rechaza caracteres de control y marcas invisibles de dirección o salto de
línea.
