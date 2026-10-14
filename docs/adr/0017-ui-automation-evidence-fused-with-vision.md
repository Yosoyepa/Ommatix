# 0017. UI Automation evidence, fused with the visual path

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Project owner, 2026-10-14

## Context

The reduced S4 acceptance run (`docs/thesis/S4_ACEPTACION_REDUCIDA.md`, review
`docs/reviews/2026-10-14-s4-reduced-acceptance.md`) showed three limits of vision-only
perception on the reference machine:

- The S0 detector (OmniParser) has one class: "interactable". Every role is `unknown`, so
  `type`, which needs a confirmed `Role::Input` (SDD §2.9), is impossible, and the rule must
  not be relaxed.
- Windows OCR cannot label the single-character Calculator keys.
- A detector box labelled "Función" was centred on the x² key, so a click aimed at that box
  by ID hit the wrong control (open finding 2: four false effects in the pilot).

Windows applications that implement UI Automation already know their controls' roles,
names, rectangles, enabled state and supported patterns. That is positive evidence that the
visual path cannot produce. The thesis already reserves it: the contract's `label_source`
enum has `accessibility`, SDD §2.5 gives an element a role, a label with provenance and an
actionability, and chapter 06 §6.8 lists a UI Automation adapter for v1.1. Algorithm 2 fuses
evidence by containment (IoC) and never invents an affordance [ADR-0003](0003-conservative-spatial-fusion-with-ioc.md).
Chapter 08 keeps values and passwords private and treats screen text as untrusted data
[ADR-0012](0012-data-minimisation-and-evidence-traceability.md).

## Decision

Add accessibility evidence as one more driven port, **fused with** the visual evidence, never
replacing it.

1. **Port.** `AccessibilityProvider` in `application/src/ports.rs`:
   `snapshot(regions) -> Vec<AccessibleElement>`, plus a no-op `NoAccessibility`, the default.
   Scope: the elements of the attested foreground window only, whose boxes overlap the
   planned regions (the whole window on a full refresh). Each element carries a box in
   physical pixels of the selected monitor (clipped), a role, its accessible name,
   `enabled`, `offscreen`, `password` and which patterns it supports. A new `accessibility`
   stage is timed in `_meta` (`accessibility_us`), never in `structuredContent`.
2. **Role mapping** (UIA ControlType → contract role, in the adapter): `Button` and
   `SplitButton` → `button`; `Edit` and `Document` → `input` only when the element has a
   writable `Value` pattern and is not a password field; `CheckBox` → `checkbox`; `ComboBox`
   → `dropdown`. `RadioButton`, `MenuItem`, `TabItem`, `ListItem`, `TreeItem`, `DataItem`,
   `Hyperlink`, `HeaderItem` and read-only or password edits are reported as `unknown`: the
   contract has no role for them and calling them buttons would invent one (their affordance
   still comes from their patterns). Every other type (panes, groups, text, images, lists,
   menus, bars, windows, custom) is skipped.
3. **Affordance.** An accessibility element is `Actionability::Confirmed` only if it is
   enabled, on screen and acts through a pattern: `Invoke`, `Toggle`, `ExpandCollapse` or
   `SelectionItem` for a click; for an input, a writable `Value`. `Scroll` is not a click
   affordance. Off-screen elements are dropped (the screen does not show them). Role and name
   never confirm anything: only the state and the patterns do.
4. **Fusion** (an extension of Algorithm 2, in `ommatix-domain`). Every accessibility element
   becomes a control. A visual detection that is the same control (IoU ≥ 0.5, `[P]`, or
   contained by IoC ≥ the fusion threshold) is absorbed: the element's box, role and
   affordance win, and its name is the label with `label_source: accessibility`. Two equally
   good candidates abstain. A visual detection no element absorbs stays as it was, except that
   it loses a confirmed affordance when its click point (its centre) falls on an accessibility
   element: that click would act on the element, not on what the box shows (the "Función"
   over x² case). Evidence only ever removes actionability from a visual box, never adds it.
   OCR words then fuse by IoC as before, and the order stays deterministic (the evidence is
   sorted before it is fused).
5. **`type`.** Allowed on an accessibility-confirmed `Input`, like on any confirmed input.
   The SendInput actuator's keyboard-focus check is unchanged, and so is fail-closed
   actuation [ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md).
6. **Privacy.** The provider never reads `ValuePattern.Value` (only whether the pattern is
   available and writable), so the engine's `include_values` and the policy keep governing
   values, which still come from OCR. A password field is never an `input`, publishes no
   label or value from OCR, and is marked sensitive. Accessible names are untrusted screen
   data, like OCR text: control characters are removed and whitespace collapsed; a name
   labels an element and never instructs or changes a permission. No accessibility data is
   logged.
7. **Adapter.** New crate `ommatix-adapter-accessibility` (feature `uia`, Windows only) on
   the safe `uiautomation` crate (0.25.1, Apache-2.0, edition 2024, builds with the MSRV
   1.88), pinned exactly. It has **no `unsafe` of its own** and needs no change to
   [ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md). It reads cached
   properties on a worker thread in the COM multithreaded apartment. The attested
   application is the one the SendInput actuator attests, including the
   `ApplicationFrameHost` resolution. The walk is bounded by elements (2 000), depth (32) and
   time (250 ms); on exhaustion it reports what it found, since the visual path covers the
   rest. The crate's default features include input modules that cannot be switched off, so
   the adapter's `clippy.toml` forbids every call that sends input, moves focus or acts
   through a pattern (`click`, `send_keys`, `set_focus`, `get_pattern`, `Keyboard`, `Mouse`,
   ...), and CI runs clippy with `-D warnings`.
8. **Configuration.** `[models] accessibility = "none" | "uia"` (default `none`), allowed only
   with the live sources `dxgi` or `screen`; with `images` it is refused (the tree would not
   describe the recorded frames), and the simulated desktop ignores it with a note. A build
   without the `uia` feature refuses `"uia"` at startup.

## Alternatives considered

- **Replace vision with accessibility.** Many applications expose a poor or empty tree
  (games, canvases, custom-drawn controls, remote desktops), and the thesis is about
  perception from pixels. Rejected: the tree is evidence, not the source of truth.
- **A role-classifying detector or a second model.** Needs weights, labels and a licence
  decision (S0) and still cannot say whether a control is enabled or writable.
- **Hand-written COM bindings for UI Automation.** More `unsafe` in the tree for no gain over
  a maintained safe wrapper.
- **Allow `type` on any visual input guess.** Violates SDD §2.9; only positive evidence
  confirms an input.
- **Read the value through `ValuePattern`.** Leaks content the policy may not allow, and
  passwords. The provider asks only for availability.
- **Query the tree per element on demand.** Cross-process calls are slow; one cached walk per
  observation, bounded, keeps latency measurable (ADR-0010).

## Consequences and cost

What becomes possible: roles, names and exact rectangles for applications that expose them;
`type` on real inputs; labels for the Calculator's keys; clicks that land on the control the
agent asked for. What becomes harder: another stage on the hot path (it runs only when regions
are planned, and the walk is capped at 250 ms; its real cost is to be measured on the
reference machine, not assumed); the engine now depends on what a third-party application
chooses to expose, so a wrong or hostile tree can mislabel a control or declare one
actionable, bounded by the same gates (enabled, on screen, a pattern, the foreground
allow-list, stale-snapshot and policy re-checks) and by the visual evidence it is fused with.
The IoU threshold 0.5 and the walk bounds are `[P]` values to calibrate.

## Required validation

- UT-44 and BDD-33 (`docs/thesis/features/02_fusion.feature`): role and name from
  accessibility, detection absorbed, wrong-centre box not confirmed, click at the accessible
  rectangle's centre, `type` allowed on the accessibility input, password never published.
- Unit tests: the ControlType → role table, the affordance rules, name cleaning, fusion
  precedence and determinism, region-limited reads, the stage timing, failure publishes
  nothing, config and wiring checks.
- The contract tests still pass, with `accessibility` a schema-valid `label_source`.
- `cargo clippy -p ommatix --features dxgi,sendinput,directml,winrt-ocr,uia -- -D warnings`
  (the `windows` CI job) with the adapter's forbidden-call table.
- Manual: the ignored test that reads the live tree of the test application reports counts and
  roles only. A supervised E2E run with `type` remains owner-only (S4).

## Reversibility

Set `[models] accessibility = "none"` (the default) and the engine behaves as before; the
port, its no-op and the fusion branch cost nothing when no element arrives. To remove it
entirely, drop the crate, the port and the `reconcile_accessibility` step; no wire format
changes, since `accessibility` was already a `label_source` value. Reconsider if the tree
proves unreliable on the reference applications.

## Provenance

Roadmap v1.1 (thesis chapter 06 §6.8) and the proposal at the end of the reduced S4
acceptance report. Extends Algorithm 2 [ADR-0003](0003-conservative-spatial-fusion-with-ioc.md);
keeps [ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md),
[ADR-0012](0012-data-minimisation-and-evidence-traceability.md) and
[ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md); the crate layout follows
[ADR-0014](0014-hexagonal-crate-layout.md).

## Resumen (ES)

Se añade la evidencia de UI Automation como un puerto más (`AccessibilityProvider`,
`[models] accessibility = "uia"`), fusionada con la evidencia visual y nunca en su lugar. Un
elemento accesible habilitado, en pantalla y con un patrón de acción es `confirmed`; absorbe
las detecciones visuales que son el mismo control (su rol, rectángulo y nombre ganan, con
`label_source: accessibility`), y una caja visual cuyo centro cae sobre otro control accesible
pierde la confirmación. `type` queda permitido sobre una entrada confirmada por accesibilidad.
Nunca se lee el valor de un campo ni se publica una contraseña, y los nombres son datos no
fiables. El adaptador usa el crate seguro `uiautomation`, sin `unsafe` propio, con un recorrido
acotado por elementos, profundidad y tiempo.
