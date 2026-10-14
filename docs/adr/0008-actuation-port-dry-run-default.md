# 0008. Actuation through an InputController port, dry-run by default

- **Status:** Accepted, amended by [0012](0012-settled-actions-and-wire-contract.md)
- **Date:** 2026-10-14

## Context

A computer-use agent must act, not only see. Acting on a real desktop is risky: a wrong
coordinate clicks the wrong thing, and an agent cannot undo it. Agents also waste a round
trip if every action has to be followed by a separate `perceive` call.

## Decision

1. Input is a driven port, `InputController` (`perform(&Action)`, `is_live()`), with the
   domain value object `Action` (`Click`, `TypeText`, `PressKey`).
2. A dedicated use case, `ActuationService`, validates every request **before** it reaches
   the port: a frame must have been perceived, click targets must lie on the last frame,
   element ids must exist in the scene, and text length is capped. It reads the scene
   through the read-only `ScreenModel` trait, not through the perception engine itself
   (dependency inversion, interface segregation).
3. Agents should click **by element id** (`click {element_id}`); raw coordinates are allowed.
4. Each MCP action tool perceives right after acting (`observe`, default `true`) and returns
   the change set in the same response.
5. Adapters: `DryRunInput` (records, never touches the OS) is the default; `NativeInput`
   (`enigo`) is compiled only with `--features native-input` and selected with
   `--input native`. Every receipt says whether the action was `live`.

## Consequences

- Moving the real mouse needs an explicit build feature and flag: no accidental live input.
- Validation is unit-tested without any OS dependency.
- Wayland sessions may restrict synthetic input; `enigo`'s backend choice is an adapter
  concern and can change without touching the core.

## Alternatives considered

- **Leave actuation to another MCP server** — the agent would then mix two coordinate
  sources and lose the validated, id-based targeting.
- **Live input by default** — rejected for safety.

## Resumen (ES)

La actuación es un puerto (`InputController`) con un caso de uso propio (`ActuationService`)
que valida cada acción contra la última pantalla percibida antes de ejecutarla. Se recomienda
hacer clic por id de elemento. Por defecto las acciones son de prueba (dry-run); la entrada
real requiere compilar con `native-input` y pedirla explícitamente. Cada acción puede
devolver lo que cambió en pantalla en la misma respuesta.
