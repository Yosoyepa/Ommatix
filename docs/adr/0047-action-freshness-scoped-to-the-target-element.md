# 0047. Scope the freshness check of a live action to the target element, with a last-look identity query, behind a switch that stays off

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Drafted by an AI agent (the architect's specifier, decision D50 of the
  coordinator's plan) for the owner to accept, reject or accept with changes. Nothing is
  implemented, nothing in this record was measured with the proposed scope, and no test status,
  requirement status or checkbox changes with it. Merging the branch that carries it is not its
  acceptance: the owner accepts it in writing, as for ADR-0032 and ADR-0035.

## Context

**The rule today.** Before every live delivery the engine captures the screen again and compares
it with the committed base, exactly, tile by tile (ADR-0032; chapter 04, *Frescura antes de una
acción*). With `action_freshness = "screen"`, the default, any changed pixel anywhere refuses with
`STALE_SNAPSHOT`, nothing is delivered and the snapshot is spent. With `"target_window"` (ADR-0032,
amended by ADR-0035) the comparison covers the target's top-level window and the windows above it
that overlap it, provided the operating system attests the window list and the target window's
opacity; otherwise the whole frame. The diagnostic D0 reports, on a refusal, `total_tiles` and
`touches_target` ("algún tile distinto interseca la caja objetivo", chapter 07 §7.7), in `_meta`
only; `touches_target` is never an input to the decision.

**The evidence.** It comes from agent-run, exploratory sessions on a laptop (1920 × 1200 at 150 %)
on 2026-11-26, under the owner's live-lab protocol. They are lab engineering, **not the dossier's
evidence**. The reports are kept outside the repository, in the owner's workspace. Quoted exactly:

1. "At Notepad's default size, with Orca's terminals visible behind it, **5 of 5** live clicks were
   refused (6 changed tiles, `touches_target: false`), so every later session ran maximized.
   Maximized: 1 of 13 (Notepad menus), 6 of 30 (Calculator) and 1 of 2 (typing) attempts were
   refused. … Of the 13 refusals, **11 had `touches_target: false`**: they were refused for pixels
   outside the target" (L1, failure 3).
2. "`STALE_SNAPSHOT` before typing is frequent in Notepad: 40 of 104 attempts were refused. 20 of
   them had `touches_target: true` with 2 tiles, which is the document's own blinking caret. That
   blocked E7's `i4` run entirely, with 4 refusals in a row" (L3). The report does not say what
   caused the other 20.
3. With the ONNX detector on CPU, where an observation takes about 4.5–5 s: the `Editar` flow had
   "3 `STALE_SNAPSHOT` (2 tiles, `touches_target: false`)"; the Calculator keys "8
   `STALE_SNAPSHOT`: 5 pixel (2–17 tiles, all `touches_target: false`), **3 age**" (L4). The age
   refusals are ADR-0025 working as written with a slow observation and are not this ADR's subject.

Every refusal delivered nothing: this is availability, not a safety failure. But each refusal spends
the snapshot, and each new observation costs 100–185 ms without the detector and seconds with it on
CPU.

**Why `target_window` does not answer it.** The audit of the freshness rule (A7) found:

- "`touches_target` is about the target's **element box**, while `target_window` compares the
  target's **whole window**", so the 11 are not shown to be what `target_window` would accept
  (A7 §0). A caret, a status bar or an animation inside the target's window refuses under
  `target_window` too: ADR-0032 keeps the caret refused and forbids a mask without its own ADR.
- On the owner's desktop, the repeat of M1 with both groups found the target window's opacity
  "`opacity attested` 0/30 and fallback `no_opacity` 30/30 in both menu states" (A7 §3, reading
  `S4_SESIONES_PENDIENTES.md` §D.1.3), so the scope never applied; the audit infers that Notepad
  11's Mica backdrop is what ADR-0035 point 4 refuses by design. On the laptop the M1 probe stopped
  before its first pair with "M1 window group cannot be attested" (L2 §4b), and the probe does not
  say which condition failed.

**What the audit proposes.** Its option 5.2: compare the target element's own box, "plus anything
that came to cover it", keep the whole-frame geometry guard and the identity of the windows above
the target, and add "a last-look identity query (the element under the click point is the observed
one, as ADR-0022 already requires of the client driver before each retry), which the engine does
not do today", "the strongest protection offered by any option here" (A7 §5.2). ADR-0022 makes
that query a rule of the supervised test clients, not of the engine.

## Decision

Proposed: a third value of `action_freshness`, `"target_element"`, chosen by the operator. Each
point is written so that an implementation can be checked against it.

1. **Switch.** `[engine] action_freshness = "target_element"` selects this ADR. `"screen"` stays
   the default and `"target_window"` keeps ADR-0035's meaning. The value is validated at start,
   and start refuses it unless (a) `[models] accessibility = "uia"`, because point 6 needs an
   accessibility identity, and (b) `[engine] action_freshness_element_margin_px` is set
   explicitly, in 0..=256: this key has **no default**, because its value is the owner's
   (point 4). The agent cannot choose the value; it is not a tool argument (chapter 07 §7.7).
2. **What is compared.** The fresh capture is compared exactly with the committed base on the
   tiles that meet the **target element's box** (the box of the element the request names, as the
   committed snapshot holds it), grown by `action_freshness_element_margin_px` and clipped to the
   frame. Half-open boxes, as chapter 04 says: touching only an edge is not an intersection, and a
   changed pixel in a tile that intersects refuses even when the pixel itself lies outside the
   grown box. No tolerance, no mask, no per-application exemption.
3. **What is kept, unchanged.**
   - the whole-frame geometry and epoch guard (ADR-0032 point 4);
   - the identity of the foreground window: every change of the foreground handle refuses with
     `STALE_SNAPSHOT` (ADR-0035 point 3);
   - the identity of **everything that came to cover the target**: the ordered list of ADR-0035
     point 1 (the target's top-level window and every visible window above it whose rectangle
     overlaps the target window's rectangle, grown by `action_freshness_margin_px`) is read at the
     observation and again at the last look, and any difference refuses. The list is taken over the
     whole target window, not only over the box, so that a new window over any part of the target
     window still refuses; it costs no pixel comparison;
   - the window under the delivery point at the last look is the target's top-level window;
   - the two window reads around the freshness capture must be equal, and the z-order walk keeps
     its bounds, as in ADR-0032 point 6 and ADR-0035 point 5;
   - the gate and the last look with every check they have (stop, consent, policy, foreground
     application, ADR-0020's window scope) and the age bound of ADR-0025 at every real-input
     boundary;
   - a dry run returns before the freshness capture; a refusal spends the snapshot; the visual
     effect is measured over the whole frame; D0 stays in `_meta` only.
4. **Margin.** `action_freshness_element_margin_px` grows the compared box so that a change next
   to the element (its label, its focus ring, a hover state, an icon beside it) refuses. A larger
   margin can only compare more tiles, so it can only refuse more. Its value is not decided here;
   the owner sets it from a measurement of those extents on the targets in use.
5. **Opacity.** ADR-0035 point 4 requires the target window to be opaque over the box because,
   under `target_window`, windows below the target are not compared. For this value the backdrop
   material condition (blur, acrylic, Mica) is dropped, and only for this value: [D] the box's own
   pixels are compared exactly, so whatever a material shows through them is compared, and where
   the click lands is attested by two hit tests (the window under the point, point 3, and the
   element under the point, point 6), not inferred from opacity. A target window that is layered
   or click-through, or a box that touches the 32-pixel corner square, still gives the whole frame,
   as in ADR-0035.
6. **The last-look identity query.** Immediately before delivery, after the last look and the
   window checks, the engine asks the accessibility provider which element is under the delivery
   point (the point the actuator will use, in the capture's physical pixels and DPI awareness,
   SDD §2.8). New port method:
   `AccessibilityProvider::element_at(&mut self, point) -> Result<Option<HitElement>, PortError>`,
   where `HitElement` carries the native identity of the element hit and of its ancestors up to its
   top-level window (at most 32 levels), and whether each is enabled and on screen. It reads no
   name and no value (ADR-0017), it acts on nothing, and it is bounded in time by a limit the
   implementation states and tests. Its default implementation answers that it cannot tell.
7. **"The same element".** The query passes when the observed target's native identity (ADR-0026:
   provider, process, window and runtime ID, compared within that scope and nowhere else) equals
   the identity of the element hit or of one of its ancestors, and that element is enabled and on
   screen. Labels, roles and boxes are never used as identity. An element hit whose ancestors do
   not include the target (the target's parent, a neighbour, an overlay drawn in the same window)
   does not pass.
8. **Outcomes, fail closed.**
   - The query answers and the element is the same: delivery may go ahead.
   - The query answers and the element is not the same: `STALE_SNAPSHOT`, nothing delivered, the
     snapshot spent. A positive difference is never turned into a fall-back.
   - The query cannot tell (provider error, time limit, no element, or a target that has no native
     identity because the snapshot confirmed it only visually): the same captured frame is compared
     over **the whole frame**, as `"screen"` does, with no second capture; any changed tile refuses.
   - Any condition of point 3 or 5 that cannot be attested: the whole frame, as in ADR-0032
     point 6.

   The fall-back is always the whole frame, never `target_window`, so that it is the strictest
   comparison and its result does not depend on ADR-0035's opacity attestation.
9. **Order at delivery.** Gate → fresh capture → comparison of point 2 → last look → window and
   occluder identity → identity query → age check → delivery. What can change between the query and
   the event is the residual risk chapter 08 §8.3 already names ("El SO puede cambiar entre
   precheck y evento").
10. **`type`.** The scope applies to the check before the focusing click, as ADR-0032 point 7 says;
    the keys still wait for element-level keyboard focus (ADR-0018). The target of a `type` is the
    text field, so a caret that blinks inside that field changes tiles of the compared box and still
    refuses: this ADR does not accept the 20 caret refusals of L3.
11. **What the agent is told.** On a refusal, D0 keeps its fields and their meaning. Two optional
    fields are proposed for the owner's decision (D0's shape is the owner's, decision of
    2026-10-15): `scope_tiles`, the differing tiles that meet the compared box (present only when
    the element scope applied), and `identity`, one closed name of `same`, `different` or
    `unavailable` (present only when the query ran). The query's time goes in a new `identity_us`
    of `io.ommatix/action_timings`. No pixel, label, coordinate, handle or runtime ID is reported.
12. **Several clients and the lease.** If ADR-0039, ADR-0040 and ADR-0043 (proposed on branches not
    yet merged) are accepted, the value is engine-wide and the operator's; no client and no grant
    selects or loosens it, and the query runs for the lease holder's action like every other
    last-look check.
13. **Determinism.** The compared tiles, the window list and the ancestor chain are ordered
    explicitly; nothing is iterated in hash order. Tests drive the query through port doubles.

## Alternatives considered

- **A. The whole screen, as today.** No gap; on the lab's laptop it made live input impossible at
  Notepad's default size (5 of 5 refused) and refused 8 of 45 attempts maximised. It stays the
  default.
- **B. ADR-0035's window scope.** It helps only where the window list and the opacity are attested
  and the change lies outside the target window. It does not help the caret or any change inside
  the target window, which it keeps refused by design; and a window with a Mica backdrop is never
  attested opaque, so it falls back to the whole frame (0 of 30 attested in the repeat of M1).
  Kept, unchanged.
- **C. This ADR: the element's box, the identity of the windows over it and of the element under
  the point** (recommended). It accepts a change that touches neither the box, nor the windows over
  the target, nor the element under the point, and it adds a check that no scope has today.
- **D. An exception for the target application's own caret** (A7 §5.3): ignore a changed tile only
  when every changed pixel lies in an operating-system-attested caret rectangle that does not meet
  the target box. It would matter most for `type`. It is not decided here and needs its own ADR,
  as ADR-0032 requires of "a caret mask, a blink exclusion or any per-application pixel exemption";
  the lab would first have to show that the operating system attests where Notepad 11's caret is.
- **E. A retry inside the engine after a refusal.** Rejected, in the contract's own words: "Una
  petición vieja no se redirige a otro target cercano. Tras una nueva observación, el agente
  selecciona explícitamente de nuevo" (chapter 07 §7.4); "el agente recibe `STALE_SNAPSHOT`, no un
  reintento automático" (chapter 07 §7.7); and ADR-0032 keeps forbidden "A retry on the same
  snapshot after a refusal". An engine that acts on its own new observation executes an intent the
  agent formed on the old one.

## What it stops protecting against, in plain words

Under `"target_element"` the guarantee narrows from "the screen is as observed" (`screen`) or "the
target window and the windows over it are as observed" (`target_window`) to: **the target's box
and its margin look exactly as observed, no window came over the target window, the foreground did
not change, and the element under the point is the observed one.** A change that leaves all of
that equal is accepted. Examples:

- a banner, an information bar, an error or validation message, or a toast drawn inside the
  application's own window, away from the target;
- the content of another part of the same window replaced: another pane, the document behind a
  menu, a tab's title, a status bar;
- an in-application overlay or dimming layer that does not cover the box and does not take the
  hit test at the point;
- a neighbour moved next to the target while the box's pixels stayed equal and the point still hits
  the target.

The agent then acts on the target as observed, on a window part of which it has not seen in its
current state. That is a reasoning risk on the agent's side, as ADR-0032 and ADR-0035 state, and
here it covers more of the screen than under either of them.

**What the identity query gives back**, which no pixel scope gives: an overlay or a popup drawn
inside the same window (without a window of its own) that takes the point; an element replaced at
the point by another that looks the same, where the application gives the new element a new
runtime ID; a target that became disabled or went off screen while its pixels stayed equal. It does
not catch an application that recycles an element and its runtime ID for different content, and it
answers with the accessibility tree's view of the point, which can differ from where Windows routes
the input: on the laptop, while a WinUI flyout of Notepad 11.2609 was open, a point query
"descended only to `MenuBar`" and not to the item (L1, failure 2). Under point 7 that case refuses:
a false refusal, never a false acceptance.

**What the lab's numbers allow, and no more.** An upper bound of what the pixel part would accept is
the count of refusals with `touches_target: false` at margin 0: 11 of 13 in L1; 3 of 3 for
`Editar` and 5 of 5 pixel refusals on Calculator in L4. Whether those actions would also have passed
the window attestation and the identity query is not known, and the window attestation failed on
the laptop for a reason not yet named. A positive margin can only lower the bound.

## Consequences and cost

- **Easier.** Fewer refusals for changes that do not concern the target, by at most the bound
  above; each avoided refusal saves one observation.
- **Harder.** A new port method and its Windows adapter (a UI Automation point query, its
  ancestor walk and its time limit, in the accessibility adapter, with no `unsafe` of its own as
  ADR-0017 requires), its test doubles, and one more read at the last look. Its cost goes in
  `identity_us` and is reported before and after (ADR-0010).
- **The dossier changes with it,** in the same pull request as the code. The sentences:
  - chapter 04, *Frescura antes de una acción*: "por defecto (`screen`) también mira la pantalla
    entera; con `target_window` aplica la misma comparación exacta sólo a los tiles que intersecan
    la unión de rectángulos del grupo" (a third value), and "una opacidad no atestiguada … devuelve
    a la pantalla entera" (not for the material, under this value);
  - chapter 04 §4.5, "La revalidación de la identidad justo antes de la entrega corresponde al
    incremento de vigencia (INC-03)" (this value is such a revalidation, in the engine);
  - chapter 07 §7.7, the D0 paragraphs ("`group_tiles` … sólo aparece cuando se pudo acreditar el
    grupo de ventanas"; "La forma de D0 no cambia") if the owner accepts the fields of point 11,
    and "sólo el operador puede escoger el alcance";
  - chapter 08, *Frescura por grupo de ventanas* ("El alcance opcional `target_window` acota la
    garantía …"), and §8.3, whose residual risks gain the list above;
  - chapter 02 §2.6, "ADR-0032 diseña esa validación opcional por grupo de ventanas";
  - outside the dossier: `action_freshness_diff.meta.schema.json` and its example (if point 11 is
    accepted), the configuration guide and the template, and the CHANGELOG.
- **Watch.** The margin is unmeasured. The query's view of the point can refuse correct actions on
  some controls (the flyout case above). The fall-back to the whole frame means that a target the
  snapshot confirmed only visually (no accessibility identity) gains nothing.

## Required validation

The implementer plans the new tests, not this ADR; they get their identifiers when they are
planned, each fails first where it tests new behaviour (AGENTS.md §5.10), and they run on
synthetic frames and port doubles. They must show at least:

- a change in the target window beyond the grown box is accepted, while `"screen"` and
  `"target_window"` refuse on the same frames;
- a one-pixel change inside the box refuses, and the margin refuses exactly the band it reaches;
- a caret blinking inside the target field refuses (no mask);
- a window above the target that appears, moves or changes order between the looks refuses, even
  away from the box; a foreground handle change refuses on identical pixels;
- the identity query answering with a different element refuses with nothing sent, on identical
  pixels; answering with a descendant of the target passes; answering with the target's parent
  (the flyout case) refuses;
- a query that cannot tell (error, time limit, no element, a visual-only target) compares the
  whole captured frame and does not capture again;
- a layered or click-through target window and a box at a corner give the whole frame; a window
  attested with a backdrop material does not;
- start refuses `"target_element"` without `uia` and without an explicit margin; the default stays
  `"screen"`;
- a geometry or epoch change refuses; a dry run returns before the capture; a refusal spends the
  snapshot; the visual effect is measured over the whole frame;
- over generated frames, layouts and query answers: with the identity the same, the element scope
  refuses on pixels exactly when a changed tile meets the grown box, and it never accepts a changed
  tile inside it;
- the existing freshness tests keep passing unchanged under `"screen"` and `"target_window"`, in
  particular UT-38 and UT-55..UT-75.

Then, before any live use, mirroring ADR-0032 and ADR-0035:

- an independent safety review of the compared region, the kept guards, the dropped material
  condition, the identity query and its fall-backs, recorded under `docs/reviews/` with each
  finding and the test that covers it;
- the condition that stops the window list from being attested on the machine where it will run
  named and resolved (the audit's first unit, A7-1): without the window list this value falls back
  to the whole frame and gains nothing;
- the margin's extents (focus ring, hover, label) measured without input on the targets in use, and
  the margin set by the owner from that result;
- preferably first, a counterfactual: on real refusals under `"screen"`, what this value would have
  decided, recorded as a diagnostic behind a key that is off by default, with no change of
  behaviour (the audit's A7-3; the owner decides its shape);
- then, with the owner's consent for each session and only on the test applications the owner
  allows, a live measurement with N and the criterion written by the owner before the run. A
  proposal for the owner to adopt, change or replace: the flow S4R-03 at Notepad's default size in
  front of live terminals, ten runs; no delivery that an independent watcher judges wrong; every
  refusal and every fall-back reported with its reason; and the share of refusals with
  `touches_target: false` reported against `"screen"` on the same scene;
- `cargo test --workspace`, the clippy runs with the Windows features and the thesis checks, and
  `freshness_us` and `identity_us` reported before and after.

## Reversibility

Setting `action_freshness = "screen"` (or `"target_window"`) restores the earlier behaviour at
once. Nothing is persisted and no tool argument changes; the optional D0 fields, if accepted, can be
ignored by a client. The trigger for reverting is a live result in which this value accepted an
action that the observed screen no longer justified, or a review finding the rules do not cover;
either one returns the switch to `"screen"` and starts a superseding ADR.

## Provenance

[D] Written from the lab reports L1, L3 and L4 of 2026-11-26 (agent-run, exploratory, on a laptop;
kept outside the repository; not the dossier's evidence), from the audit of the freshness rule
(A7: the threats of its §2, `target_window` in its §3, its options in §5, its order and questions
in §6), and from ADR-0022, ADR-0025, ADR-0026, ADR-0032 and ADR-0035. [P] The value, its kept
guards, the dropped material condition and the identity query are a new proposal. It refines ADR-0032
(it adds a value to its switch) and changes nothing in ADR-0035 for `"target_window"`. ADR-0022
stays a rule of the supervised clients. It supersedes nothing. ADR-0039, ADR-0040 and ADR-0043 are
cited as proposals on branches not yet merged.

## What the owner is asked

1. **Accept, reject, or accept with changes** the element scope as written.
2. **The margin.** Its value, or the measurement from which the owner will set it; until then the
   value cannot be used, because the key has no default.
3. **The material condition.** Whether to drop ADR-0035's backdrop condition for this value only
   (point 5), or to keep it, in which case a Mica window such as Notepad 11's never gets this scope.
4. **The covering windows.** The identity list over the whole target window (point 3, recommended),
   or only over the grown box (fewer refusals, and a new window over the target window away from
   the box is then accepted).
5. **The fall-back.** The whole frame (point 8, recommended), or ADR-0035's window scope when it is
   attested.
6. **D0.** Whether to add `scope_tiles` and `identity` to `freshness_diff` (point 11).
7. **The gate.** Whether the counterfactual diagnostic comes first; who writes N and the criterion
   of the live measurement and on which machine; whether the proposed criterion is adopted; which
   review counts as independent; and how consent for each session is given.
8. **Not decided here, kept for later:** the caret exception (option D, its own ADR), and an
   identity source other than accessibility for targets the snapshot confirmed only visually.

Status: Proposed

## Resumen (ES)

Propuesta, no decidida: un tercer valor de `[engine] action_freshness`, `"target_element"`, que
compara de forma exacta sólo la caja del elemento objetivo ampliada por un margen que fija el
responsable (la clave no tiene valor por defecto), conserva la guarda global de geometría y época,
la identidad de la ventana de primer plano y de las ventanas que cubren la ventana objetivo y las
edades de ADR-0025, y añade una consulta de identidad justo antes de entregar: el elemento bajo el
punto debe ser el observado, por su identidad de accesibilidad (ADR-0026). Si la consulta no puede
responder se compara el fotograma entero; si responde otro elemento, `STALE_SNAPSHOT`. El motivo son
las sesiones exploratorias del laboratorio del 2026-11-26 (no evidencia del dossier): 5 de 5 clics
rechazados con Notepad sin maximizar y 11 de 13 rechazos sin tocar la caja del objetivo; el cursor
de texto dentro del propio campo sigue rechazando. Deja de proteger contra cambios dentro de la
ventana que no tocan la caja ni el punto (un aviso, otro panel), queda apagada por defecto y exige
revisión, margen medido y consentimiento por sesión antes de cualquier uso en vivo.
