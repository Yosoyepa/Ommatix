# Live input and operator control

[Español](live-input.es.md) · [Configuration](configuration.md)

> [!CAUTION]
> Live input is off by default. These controls belong to the operator; screen text and the agent never grant permissions.

Everything is fail-closed by default: without a configuration file the template denies
actuation, redacts values and loads no model; every action is a dry run unless the agent
sets `dry_run: false`; and no CLI flag loosens `[safety]`. Live input needs **all** of:

1. `--input sendinput` with `--source dxgi`, on a Windows build with `sendinput`;
2. `[safety] actuation_enabled = true` and a non-empty `[safety] allowed_applications` of
   lowercase executable names (`"notepad.exe"`), both in the operator's configuration;
3. `dry_run: false` from the agent, on a target of the current snapshot.

The CLI refuses a live input adapter before any adapter opens when 1 or 2 does not hold.
For each action the engine then checks the foreground application the actuator attests
against `allowed_applications`, captures the screen again and refuses (`STALE_SNAPSHOT`) if
it no longer matches the snapshot, reads the operator's stop and attests the foreground
application once more (refusing with `POLICY_DENIED` if the stop is engaged or the
application changed), and only then delivers. The `SendInput` actuator also refuses a
foreground process with a higher integrity level (UIPI would drop the input silently), a
target point outside that process's windows and held modifier keys or mouse buttons; for
`type`, after the click that focuses the field it checks that keyboard focus is on the
window at the target point (or a window containing it) of the same foreground process, and
otherwise sends no key. With `[models] accessibility = "uia"` the engine itself delivers
`type` in two phases ([ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md)):
it sends the click, polls on its clock until UI Automation reports keyboard focus on the
target element, and only then sends the keys; no confirmation in the verification window
means no key. Such a refusal is reported `indeterminate`, never retried: for
`type` the click may already have been delivered. `--input native`
(or `[engine] mode = "native"`) is still accepted with `--source screen` and
`actuation_enabled = true`, but `enigo` cannot attest
the foreground application, so its live actions are always `POLICY_DENIED`.

**Operator stop.** While live input is possible, `serve` prints an `ACTIVE CONTROL` line on
stderr naming the allowed applications and how to stop. With `[safety] stop_file =
"STOP"` (relative to the configuration file), creating that file stops live input: the
engine reads it before each action and again right before delivery, refuses new actions
with `POLICY_DENIED` (`next_step: operator_review`) without sending any event, and prints a
line on stderr when the stop is engaged or cleared; removing the file clears it. Actions
already delivered keep their reported status (a replay of their `request_id` still says
`input_delivered`); nothing is rolled back. No MCP tool can set or clear the stop. Ending
the process (Ctrl+C) also stops it; without a `stop_file`, that is the only local stop.

**Operator purge** ([ADR-0028](../adr/0028-operator-purge-file.md), accepted; off by
default). With `[safety] purge_file = "PURGE"` (relative to the configuration file), creating
that file makes the engine delete its state and cache at its next request, without ending the
session; no identifier is issued again afterwards. The engine honours each instance of the file
once (a new modification time or size is a new instance), writes a line on stderr and never
deletes the file: to ask again, the operator removes it, lets one request pass and creates it
again. No MCP tool can request a purge, and the agent sees only the usual `STALE_SNAPSHOT` on its
next action on the previous snapshot.

**Operator scope on windows** ([ADR-0020](../adr/0020-operator-scope-on-windows.md),
accepted; off by default). `allowed_applications` names a process, and `explorer.exe` also
owns the desktop and the taskbar. `[safety] allowed_window_classes = { "explorer.exe" =
["CabinetWClass"] }` limits an application to the window classes listed, and `[safety]
window_pin_file` names a file, written by the operator or the harness it runs, that holds
the handle of the only window allowed. The engine checks the foreground window and the
window under the action's point (class and handle, as the actuator attests them, never a
title) at the gate and again right before delivery; whatever cannot be attested is refused
with `POLICY_DENIED`. `applicationframehost.exe` is never allowed, and `serve` warns when
`explorer.exe` is allowed with neither.



## Delivery and effect

`simulated` claims no delivery; `input_delivered` claims no effect; `indeterminate` never starts an automatic retry. Current verification checks `visual_change` within `verify_timeout_ms`, polling every `verify_poll_ms`; effects it cannot check retain their uncertainty. Any delivery spends the snapshot; a new action needs a new observation. Sources: [ADR-0007](../adr/0007-fail-closed-actuation-and-honest-simulation.md), [architecture](../architecture/overview.md#one-action) and [contract](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md).
