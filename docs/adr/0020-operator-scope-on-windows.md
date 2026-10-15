# 0020. The operator's scope on windows: window classes and a pinned window

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Accepted by the project owner on 2026-10-14 (E2E-03 may run in `explorer.exe`
  only in a window the client opened)

## Context

`[safety] allowed_applications` names a process by executable file name (ADR-0007, chapter
08). A process can own more than the window the operator means. The S4 reduced acceptance
(`docs/thesis/S4_ACEPTACION_REDUCIDA.md` §6, finding 1) found the first case: the Windows 11
Calculator is drawn inside a frame owned by `ApplicationFrameHost.exe`, so allowing the
application meant allowing every hosted application until the actuator attested the hosted
process instead.

`explorer.exe` is the second case, and a worse one. It owns File Explorer windows
(`CabinetWClass`), but also the desktop (`Progman`, `WorkerW`), the taskbar (`Shell_TrayWnd`,
`Shell_SecondaryTrayWnd`) and the shell's pop-up menus. The dossier's E2E-03 (navigate test
folders without destroying anything, `matrices/plan_pruebas.csv`) needs `explorer.exe` on the
allowlist. With a process name alone, a click aimed at an Explorer window that is covered by
the taskbar, or an Explorer window the owner opened, or the desktop behind it, would be
delivered: the engine checks the foreground *application* and the actuator the process of the
window under the point (`process_at`), and for `explorer.exe` that is the same process for
all of them.

The dossier already asks for more than a process. Chapter 08 §8.1 lists an "allowlist de
aplicaciones y ventana", and RF-11 asks the gate to validate "ventana/foco". The code
implements the application half only. The agent cannot repair this itself: the operator's
policy is outside its reach (ADR-0007), and window titles and screen text are untrusted data
that never change a permission (ADR-0012, `AGENTS.md` rule 7).

## Decision

Add an optional scope on windows, off by default, enforced by the engine at the action gate
and attested by the actuator.

1. **Port.** `OsActuator` gains `foreground_window()` and `window_at(point)`, each returning
   `Option<TopLevelWindow>`: the class name and the handle of a top-level window, never its
   title or any text. The default is `None` ("cannot attest"), so an actuator that does not
   implement them is unchanged.
2. **Policy.** `LocalPolicy` gains two fields, both empty by default:
   - `window_classes`: per application (the lowercase executable name the actuator
     attests), the only window classes live input may go to. Configured as
     `[safety] allowed_window_classes = { "explorer.exe" = ["CabinetWClass"] }`. An
     application without an entry is not restricted by class.
   - `window_pin`: the operator's pin on one window, configured as `[safety]
     window_pin_file = "<path>"`. The file holds the handle of the only window live input may
     go to, in decimal. It is read on every action, like the stop file. A missing, empty,
     malformed or oversized file is a pin that names no window, and a pin that names no window
     allows no input at all. The pin applies to every allowed application.
3. **Rule** (the pure function `window_scope_problem`, in `ommatix-application`). When a scope
   applies to the attested foreground application, live input is allowed only if the
   foreground window and the window under the action's point are both attested and
   - for a class list: both have a listed class (exact, case-sensitive match), and they are
     the same window (a click may not land in another window of the same class);
   - for a pin: both are the pinned window.
   Whatever the actuator cannot attest is outside the scope. The refusal is `POLICY_DENIED`
   with a fixed message that carries no class, handle or title.
4. **Where.** At the gate, after the application allowlist, and again in the last look right
   before delivery. For `type` in two phases (ADR-0018) the last look also runs between the
   focusing click and the keys, so a window that takes the foreground after the click gets no
   key. The actuator is asked only when a scope applies, so the default policy costs no
   Win32 call. Nothing is ever rolled back.
5. **Actuator.** The `SendInput` actuator attests the root window (`GetAncestor(GA_ROOT)`) of
   the foreground window and of the window at the mapped point: class name and handle. It
   decides nothing. It adds no unsafe block: the `GetAncestor` call that `process_at` already
   made moved into a helper that both use, inside the `#[cfg(windows)] mod win` of ADR-0016,
   and the handle is read from the `HWND` without a cast of a raw pointer.
6. **Configuration is validated, not trusted.** An entry that could never match would leave
   its application unrestricted without saying so, so it is an error: a key that is not a
   lowercase executable name, a key not listed in `allowed_applications`, an empty class
   list or an empty class, a class longer than 256 characters, `applicationframehost.exe`
   (its frames stand for every hosted application and are never allowed), and the classes
   of the desktop and the taskbar (`Progman`, `WorkerW`, `Shell_TrayWnd`,
   `Shell_SecondaryTrayWnd`, `#32769`). At startup, with live input on, `explorer.exe` allowed
   with neither a class list nor a pin gets a warning on stderr, and the active-control
   indicator (chapter 08 §8.2) names the scope.
7. **Who writes the pin.** The operator, or the harness the operator runs (the S4 E2E client
   writes the handle of the one Explorer window it opened and deletes the file when it
   closes that window). The agent has no tool that writes it, exactly like the stop file.
8. **Not decided here.** `type` and `scroll` are not widened or narrowed beyond the rule
   above; `ApplicationFrameWindow` (UWP) scopes are not supported; pop-up windows (menus,
   XAML flyouts, the breadcrumb's drop-down list) have their own classes and are outside any
   scope unless the operator lists them, which the E2E-03 configuration does not.

## Alternatives considered

- **Process name only (status quo).** It lets `explorer.exe` reach the desktop and the
  taskbar. Rejected.
- **Match the window title.** Titles are screen text: untrusted and attacker-influenced, and
  text on screen never changes permissions (ADR-0012). Rejected.
- **Classes only, no pin.** A second `CabinetWClass` window, such as the owner's own, would
  receive clicks. The pin narrows the class to the one window the harness recorded.
- **A pin only, no classes.** A reused handle (Windows recycles window handles after a window
  closes) could name a window of another class. Both together are the E2E-03 configuration.
- **Check the window in the accessibility provider** (UI Automation `FromPoint`). It would
  tie a safety check to an optional feature (`[models] accessibility`) and to a slower read.
  The actuator already resolves the window under the point for the process check.
- **Add a double click to the contract so that the file list can be used.** Out of scope:
  the contract has `click`, `type` and `scroll` only, and a new action is a contract change.
- **Restrict by process and kill or hide the other `explorer.exe` windows.** The client
  never ends a process it did not start and `explorer.exe` owns the user's shell.

## Consequences and cost

- E2E-03 can be allowed for exactly one Explorer window. The same mechanism gives any
  application with several windows a way to be allowed for one of them.
- The window check and the delivery are not atomic: between the last look and the OS a
  window can still change (chapter 08 §8.3). The actuator's process check narrows it, and the
  scope does not claim more. Handle reuse is bounded by the harness deleting the pin file when
  it closes its window; an operator who leaves a stale pin in place has pinned whatever window
  gets that handle next, and the class list limits that to the listed classes.
- A pop-up opened by a click (the breadcrumb's drop-down) is refused by design. The navigation
  pane and the breadcrumb buttons are inside the Explorer window itself.
- Two more calls per live action (the foreground window and the window at the point) only when a
  scope is configured, on a path that already captures a frame (ADR-0010: measured with the
  rest of the action path, not promised).
- `LocalPolicy` grows two fields. Every constructor that spelled all fields out had to change;
  none of the existing tests did.

## Required validation

- Unit tests of the pure rule (`window_scope::tests`): no scope changes nothing; another
  application is not restricted; the listed class is allowed; the desktop, the taskbar and
  shell pop-ups are refused; exact class match; another window of the class under the point is
  refused; unattestable windows are refused; the pin; an unset pin; messages without class,
  handle or title.
- Engine tests (`engine::tests::window_scope`): the same at the gate and in the last look,
  `type` between its click and its keys, and no window query without a scope.
- Policy and configuration tests: the pin file follows the file and fails closed; the
  validation errors above; the active-control indicator and the `explorer.exe` warning.
- Acceptance scenarios BDD-36 (outside the scope) and BDD-37 (unrecorded window) over the
  actuator double, traced to RF-11 and UT-45.
- Pending and **not done**: a supervised run of E2E-03 on the reference machine, which is also
  the only check of what `GetAncestor` and the class names return on the real shell. Until
  then the Win32 half is verified by compilation and clippy only.

## Reversibility

Remove the two configuration keys (the defaults are empty) and the scope is off. To remove the
code, delete `window_scope.rs`, the two `OsActuator` methods and the two `LocalPolicy` fields;
the gate's extra calls are the only places that read them. Revisit if the supervised run shows
that the real shell's classes differ from `CabinetWClass`, or if a second application needs a
rule that is not a class list or a single window.

## Provenance

Chapter 08 §8.1 ("allowlist de aplicaciones y ventana") and RF-11 ("ventana/foco"); SDD §2.9;
the S4 reduced acceptance finding 1 (frame host) and the owner's decision of 2026-10-14 on
E2E-03. Refines [ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md) and stays inside
[ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md).

## Resumen (ES)

La lista de aplicaciones permitidas nombra un proceso, y `explorer.exe` es dueño también del
escritorio y de la barra de tareas. Aprobada por el responsable el 14 de octubre de 2026: un
alcance opcional por ventana, apagado por defecto. `[safety] allowed_window_classes` lista, por
aplicación, las únicas clases de ventana que reciben entrada real (para el explorador,
`CabinetWClass`), y `[safety] window_pin_file` apunta a un archivo con el identificador de la
única ventana permitida, que escribe el operador o su arnés. El motor comprueba la ventana en
primer plano y la ventana bajo el punto de la acción en la puerta y de nuevo justo antes de
entregar (y entre el clic y las teclas de `type`), y rechaza con `POLICY_DENIED` todo lo que el
actuador no pueda atestiguar. El actuador `SendInput` sólo atestigua clase e identificador, sin
leer títulos. Nunca se permite `applicationframehost.exe`. La parte de Windows aún no se ha
ejecutado en vivo.
