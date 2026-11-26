# 0047. Scope the freshness check of a live action to the target element, with a last-look identity query, behind a switch that stays off

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Drafted by an AI agent (the architect's specifier, decision D50 of the
  coordinator's plan) for the owner to accept, reject or accept with changes, and revised the same
  day after an independent review (recorded in the coordinator's workspace). Nothing is
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
opacity; otherwise the whole frame. After the extra window walk, the engine re-runs its whole last
look (stop, consent, application, window scope, age) before it compares identity or delivers
(`src/application/src/engine/freshness.rs:644-646`; chapter 02 §2.9: "Tras ese recorrido se
revalidan parada, consentimiento, aplicación, alcance por ventana y edad"). The diagnostic D0
reports, on a refusal, `total_tiles` and `touches_target` ("algún tile distinto interseca la caja
objetivo", chapter 07 §7.8), in `_meta` only; `touches_target` is never an input to the decision.

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
  11's Mica backdrop is what ADR-0035 point 4 refuses by design.
- **On the laptop the window list itself could not be attested.** The M1 probe stopped before its
  first pair with "M1 window group cannot be attested" (L2 §4b), and the probe does not say which
  condition failed. The audit notes that the probe is stricter than the action path, so this does
  not prove the action path would fail too, and it proposes a read-only diagnostic that names the
  failing condition (A7-1). This proposal needs the same window list (point 3): until that
  condition is named and resolved, on that laptop it would fall back to the whole frame and gain
  nothing.

**What the audit proposes.** Its option 5.2: compare the target element's own box, "plus anything
that came to cover it", keep the whole-frame geometry guard and the identity of the windows above
the target, and add "a last-look identity query (the element under the click point is the observed
one, as ADR-0022 already requires of the client driver before each retry), which the engine does
not do today", "the strongest protection offered by any option here" (A7 §5.2). ADR-0022 makes
that query a rule of the supervised test clients, not of the engine, and it fixes what "the
observed one" means: "the identified element (the same RuntimeId …) or have it as parent". It
records that the owner rejected the wider rule: "Accept any descendant of the identified control
as the same control. It would also accept a click on a different child of a composite control (a
`SplitButton`'s arrow instead of its body). The owner chose not to weaken the identity rule."

**What ADR-0032 rejected, and how this proposal relates to it.** ADR-0032 compared four options and
rejected two that look like this one:

- *Its option B*, "The target window's rectangle plus a margin; everything outside ignored", was
  rejected because "It drops the geometry guard" and because "it compares pixels only, so a
  transparent window that starts to receive clicks, or a change of z-order that leaves the pixels
  unchanged, passes". This proposal compares even less than B (the element's box, not the window's
  rectangle), so both objections apply to its pixel part. They are answered by what it keeps, not
  by the pixels: the whole-frame geometry and epoch guard (point 3), the ordered list of the
  windows over the target window, which refuses a z-order change or a new window on identical
  pixels (point 3), the window under the delivery point (point 3), and the element under the
  point (points 6 and 7).
- *Its option C*, a region from Algorithm 1's closure, was rejected because "safety would depend on
  the detector finding every support: a missed support is a missed region", and because "A popup
  elsewhere in the same window (SDD §2.6) would not be covered". ADR-0032 then lists, under "What
  stays forbidden", "A perception-derived region (Algorithm 1's closure, detector boxes) as the
  freshness scope. Option C needs its own ADR." The compared box here is derived from perception,
  so this is such an ADR, and the objections stand in part:
  - the dependence on perception is **narrowed, not removed**: the box is the target's own box,
    not a closure of supports, and point 2 requires the target to have been confirmed by
    accessibility, so the box includes the rectangle the application itself reported. A box that
    is too small, from the detector or from an application that reports a wrong rectangle, makes
    the comparison cover less; the identity query still checks the point;
  - **an in-window popup away from the box is still not covered** (SDD §2.6). If it is a window of
    its own, the list of covering windows catches it. If it is drawn inside the target window and
    does not take the point, nothing in this proposal catches it. That objection of ADR-0032
    stands, and the owner accepts or rejects it with this ADR.

## Decision

Proposed: a third value of `action_freshness`, `"target_element"`, chosen by the operator. Each
point is written so that an implementation can be checked against it.

1. **Switch.** `[engine] action_freshness = "target_element"` selects this ADR. `"screen"` stays
   the default and `"target_window"` keeps ADR-0035's meaning. The value is validated at start,
   and start refuses it unless (a) `[models] accessibility = "uia"`, because points 2 and 6 need
   accessibility evidence, and (b) `[engine] action_freshness_element_margin_px` is set
   explicitly, in 0..=256: this key has **no default**, because its value is the owner's
   (point 4). The agent cannot choose the value; it is not a tool argument (chapter 07 §7.8).
2. **What is compared.** The fresh capture is compared exactly with the committed base on the
   tiles that meet the **target's compared box**, grown by `action_freshness_element_margin_px`
   and clipped to the frame. The compared box is the union of the target's box as the committed
   snapshot holds it and the rectangle of the accessibility element that confirmed it at the
   observation. A target that the snapshot did not confirm by accessibility (no native identity)
   gets the whole frame (point 8). Half-open boxes, as chapter 04 says: touching only an edge is
   not an intersection, and a changed pixel in a tile that intersects refuses even when the pixel
   itself lies outside the grown box. No tolerance, no mask, no per-application exemption.
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
   - the window under the delivery point at the last look is the target's top-level window; a
     different window refuses;
   - the two window reads around the freshness capture must be equal, and the z-order walk keeps
     its bounds, as in ADR-0032 point 6 and ADR-0035 point 5;
   - the gate and the last look with every check they have (stop, consent, policy, foreground
     application, ADR-0020's window scope) and the age bound of ADR-0025 at every real-input
     boundary, re-run after every window walk and after the identity query (point 9);
   - a dry run returns before the freshness capture; a refusal spends the snapshot; the visual
     effect is measured over the whole frame; D0 stays in `_meta` only.
4. **Margin.** `action_freshness_element_margin_px` grows the compared box so that a change next
   to the element (its label, its focus ring, a hover state, an icon beside it) refuses. A larger
   margin can only compare more tiles, so it can only refuse more. Its value is not decided here;
   the owner sets it from a measurement of those extents on the targets in use.
5. **Opacity.** ADR-0035 point 4 requires the target window to be opaque over the box because,
   under `target_window`, windows below the target are not compared. For this value, and only for
   it, one of its conditions is dropped: **the system backdrop material** (blur, acrylic, Mica).
   [D] The box's own pixels are compared exactly, so whatever a material shows through them is
   compared, and where the click lands is attested by hit tests (the window under the point,
   point 3, and the element under the point, points 6 and 7), not inferred from opacity. Every
   other exclusion of ADR-0035 is kept and gives the whole frame: a target window that is layered,
   click-through, or has no redirection bitmap (`WS_EX_NOREDIRECTIONBITMAP`, which DirectComposition
   can compose with per-pixel alpha without layering, ADR-0035 implementation notes and chapter 02
   §2.9); a style query that failed or left an error; and a compared box that touches the 32-pixel
   corner square of the target window.
6. **The last-look identity query.** After the window checks of the last look, the engine asks the
   accessibility provider which element is under the delivery point (the point the actuator will
   use, in the capture's physical pixels and DPI awareness, SDD §2.8). New port method:
   `AccessibilityProvider::element_at(&mut self, point) -> Result<Option<HitElement>, PortError>`,
   where `HitElement` carries the native identity of the element hit and of its parent, and
   whether each is enabled and on screen. It reads no name and no value (ADR-0017), it acts on
   nothing, and it is bounded in time: it answers within the accessibility read's existing time
   budget (250 ms, `DEFAULT_MAX_DURATION` in `src/adapters/accessibility/src/lib.rs`) plus that
   module's reply margin, or it counts as "cannot tell". Its default implementation answers that it
   cannot tell. The Windows implementation lives in the accessibility adapter, with no `unsafe` of
   its own (ADR-0017).
7. **"The same element", as ADR-0022 accepted it.** The query passes when the element hit **is**
   the observed target, or **has it as parent**: the observed target's native identity (ADR-0026:
   provider, process, window and runtime ID, compared within that scope and nowhere else) equals
   the identity of the element hit or of its parent. Both the element hit and the target must be
   enabled and on screen. Nothing further up counts: an element hit whose parent is not the target
   (the target's own parent, a neighbour, a deeper descendant, an overlay drawn in the same window)
   does not pass. There is no fall-back to an AutomationId: without a runtime ID the query cannot
   tell. Labels, roles and boxes are never used as identity. Whether the rule should be wider or
   narrower is a question for the owner, not part of this proposal.
8. **Outcomes, fail closed, each with its reason.**
   - The query answers and the element is the same: delivery may go ahead, after point 9's last
     re-validation.
   - The query answers and the element is not the same: `STALE_SNAPSHOT`, nothing delivered, the
     snapshot spent. A positive difference is never turned into a fall-back.
   - Every condition that cannot be attested gives the **whole frame**, compared on the frame
     already captured, with no second capture; any changed tile refuses. Each such case has one
     closed reason name: `window_read_unstable` (the two window reads differ),
     `window_read_incomplete` (a member without a rectangle, a failed or bounded walk),
     `target_layered`, `target_click_through`, `target_no_redirection_bitmap`,
     `target_style_unattested` (the style query failed), `box_at_corner`,
     `box_outside_target_window`, `target_without_native_identity`, `identity_provider_error`,
     `identity_timeout`, `identity_no_element`, and `final_attestation_missing` (a read of point 9
     after the query could not be made). When several hold, the first in the order of point 9 is
     reported.
   - The fall-back is always the whole frame, never `target_window`, so that it is the strictest
     comparison and does not depend on ADR-0035's opacity attestation.
9. **Order at delivery, never weaker than today's.** Gate (stop, consent, policy, foreground
   application, window scope, age) → two window reads around the fresh capture → comparison of
   point 2 → the existing last look → window identity (foreground handle, the list of covering
   windows, the window under the point) → identity query → **a full re-validation after the
   query**: stop, consent, policy, foreground application, ADR-0020's window scope, the foreground
   handle and the window under the point read again, and the age bound → delivery. This is the
   existing rule of re-running the last look after an OS walk (`freshness.rs:644-646`), extended
   to the query, so that the query's time never prolongs an authorisation. A change found by the
   re-validation refuses with the code the existing last look uses; a read it cannot make falls
   back to the whole frame (`final_attestation_missing`), as the existing code does when final
   attestation is lost. What can change between the last re-validation and the event is the
   residual risk chapter 08 §8.3 already names ("El SO puede cambiar entre precheck y evento").
10. **`type`.** The scope applies to the check before the focusing click, as ADR-0032 point 7 says;
    the keys still wait for element-level keyboard focus (ADR-0018). The target of a `type` is the
    text field, so a caret that blinks inside that field changes tiles of the compared box and still
    refuses: this ADR does not accept the 20 caret refusals of L3.
11. **What the agent and the operator are told.** Every live action under this value reports, in
    `_meta["io.ommatix/action_timings"]` and nowhere else, `freshness_scope` with `configured`
    (`target_element`), `effective` (`target_element` or `screen`) and `fallback_reason` (`null`
    or one name of point 8), on a delivery and on a refusal alike, so that every fall-back is
    reported with its reason. On a refusal, D0 keeps its fields and their meaning; two optional
    fields are proposed for the owner's decision (D0's shape is the owner's, decision of
    2026-10-15): `scope_tiles`, the differing tiles that meet the compared box (present only when
    the element scope applied), and `identity`, one closed name of `same`, `different` or
    `unavailable` (present only when the query ran). The query's time goes in a new `identity_us`.
    No pixel, label, coordinate, handle or runtime ID is reported. Under `"screen"` and
    `"target_window"` nothing new appears.
12. **Several clients and the lease.** If ADR-0039, ADR-0040 and ADR-0043 (proposed on branches not
    yet merged) are accepted, the value is engine-wide and the operator's; no client and no grant
    selects or loosens it, and the query runs for the lease holder's action like every other
    last-look check.
13. **Determinism.** The compared tiles and the window list are ordered explicitly; nothing is
    iterated in hash order. Tests drive the query through port doubles.

## Alternatives considered

- **A. The whole screen, as today.** No gap; on the lab's laptop it made live input impossible at
  Notepad's default size (5 of 5 refused) and refused 8 of 45 attempts maximised. It stays the
  default.
- **B. ADR-0035's window scope.** It helps only where the window list and the opacity are attested
  and the change lies outside the target window. It does not help the caret or any change inside
  the target window, which it keeps refused by design; and a window with a Mica backdrop is never
  attested opaque, so it falls back to the whole frame (0 of 30 attested in each menu state in the
  repeat of M1). Kept, unchanged.
- **C. ADR-0032's option B, the target window's rectangle alone.** Rejected there for dropping the
  geometry guard and for comparing pixels only; see the Context for how this proposal keeps both
  guards and why its pixel part is narrower still.
- **D. This ADR: the element's box, the identity of the windows over the target window and of the
  element under the point** (recommended). It accepts a change that touches neither the box, nor
  the windows over the target window, nor the foreground, nor the element under the point, and it
  adds a check that no scope has today. ADR-0032's objection that an in-window popup away from the
  target is not covered still stands.
- **E. An exception for the target application's own caret** (A7 §5.3): ignore a changed tile only
  when every changed pixel lies in an operating-system-attested caret rectangle that does not meet
  the target box. It would matter most for `type`. It is not decided here and needs its own ADR,
  as ADR-0032 requires of "a caret mask, a blink exclusion or any per-application pixel exemption";
  the lab would first have to show that the operating system attests where Notepad 11's caret is.
- **F. A retry inside the engine after a refusal.** Rejected, in the contract's own words: "Una
  petición vieja no se redirige a otro target cercano. Tras una nueva observación, el agente
  selecciona explícitamente de nuevo" (chapter 07 §7.4); "el agente recibe `STALE_SNAPSHOT`, no un
  reintento automático" (chapter 07 §7.8); and ADR-0032 keeps forbidden "A retry on the same
  snapshot after a refusal". An engine that acts on its own new observation executes an intent the
  agent formed on the old one.

## What it stops protecting against, in plain words

Under `"target_element"` the guarantee narrows from "the screen is as observed" (`screen`) or "the
target window and the windows over it are as observed" (`target_window`) to: **the target's box
and its margin look exactly as observed, no window came over the target window, the foreground did
not change, the window under the point is the target's, and the element under the point is the
observed one or its direct child.** A change that leaves all of that equal is accepted. Each case
below says what still catches it, or that nothing does.

| Change | What still catches it |
|---|---|
| A banner, information bar, error or validation message, or toast drawn inside the target window, away from the box | Nothing, unless it is a window of its own over the target window (the covering list) or it takes the point (the query) |
| A confirmation dialog in the target's own window whose prompt text changed while its button stayed identical | Nothing, if the prompt lies outside the box and its margin. A dialog that is a new window of its own refuses (covering list); under `target_window` the prompt's pixels would have refused |
| A setting elsewhere in the window that changes what the click means (another checkbox, the selected row whose action the target is) | Nothing |
| Another pane, the document behind a menu, a tab title or a status bar replaced | Nothing |
| A virtualised list scrolled so that another row's identical-looking button lies under the point, its label outside the box changed | The query, when the application gives the new row's button a new runtime ID; nothing, when it recycles the element and its runtime ID |
| A neighbour moved next to the target while the box's pixels stayed equal and the point still hits the target | Nothing; the click lands on the target as observed |
| An overlay drawn in the same window (no window of its own) that covers the point and is visible | The box's pixels, and the query if the overlay is an accessibility element |
| An overlay in the same window that is invisible and not an accessibility element, but intercepts input at the point, with a visible change elsewhere in the window | Nothing: pixels, window list and query are all unchanged. `screen` and `target_window` would refuse on the visible change. With no visible change at all, nothing catches it today under any value |
| A direct child of a composite target hit instead of the part the agent meant (a split button's arrow, if the platform reports it as a direct child) | Nothing beyond ADR-0022's accepted rule, which accepts a direct child |
| The target disabled, or moved off screen, while its pixels stayed equal | The query (enabled and on screen are required) |
| A window that the operating system reports late, after the last read | Nothing at that action; the residual of chapter 08 §8.3 |
| A stop, a consent withdrawn, a foreground or window-scope change during the query | The re-validation after the query (point 9) |
| The caret blinking inside the target field | Still refused (point 10): not an availability gain |

The agent then acts on the target as observed, on a window part of which it has not seen in its
current state. That is a reasoning risk on the agent's side, as ADR-0032 and ADR-0035 state, and
here it covers more of the screen than under either of them.

**What the identity query gives back**, which no pixel scope gives: an overlay or a popup drawn
inside the same window, as an accessibility element, that takes the point; an element replaced at
the point by another that looks the same, where the application gives the new element a new
runtime ID; a target that became disabled or went off screen while its pixels stayed equal. Its
limits: it answers with the accessibility tree's view of the point, which can differ from where
Windows routes the input. On the laptop, while a WinUI flyout of Notepad 11.2609 was open, a point
query "descended only to `MenuBar`" and not to the item (L1, failure 2); under point 7 that case
refuses, a false refusal. Where the tree and the input routing differ the other way (the tree
reports the target, the input goes elsewhere, as with the invisible overlay above), the query
accepts and nothing else catches it.

**What the lab's numbers allow, and no more.** An upper bound of what the pixel part would accept is
the count of refusals with `touches_target: false` at margin 0: 11 of 13 in L1; 3 of 3 for
`Editar` and 5 of 5 pixel refusals on Calculator in L4. Whether those actions would also have passed
the window attestation and the identity query is not known, and the window attestation failed on
the laptop for a reason not yet named. A positive margin can only lower the bound.

## Consequences and cost

- **Easier.** Fewer refusals for changes that do not concern the target, by at most the bound
  above; each avoided refusal saves one observation.
- **Harder.** A new port method and its Windows adapter (a UI Automation point query, its parent
  and its time limit, in the accessibility adapter, with no `unsafe` of its own as ADR-0017
  requires), its test doubles, one more read and one more re-validation at the last look. The
  window reads are the existing ones of ADR-0032 and ADR-0035 in the input adapter's `win` module
  (ADR-0016); this value adds no Win32 call there. Its costs go in `identity_us` and are reported
  before and after (ADR-0010).
- **The dossier changes with it,** in the same pull request as the code. The sentences:
  - chapter 02 §2.4, the list of what `OsActuator` and `AccessibilityProvider` attest (the new
    `element_at`);
  - chapter 02 §2.9, the paragraph *Alcance de frescura opcional (ADR-0032)* ("`[engine]
    action_freshness = "screen"` conserva la pantalla entera por defecto. `"target_window"`
    compara …") and the paragraph *Grupo enmendado (ADR-0035 …)* on what `window_opacity` must
    attest (not the material, under this value);
  - chapter 02 §2.6, "ADR-0032 diseña esa validación opcional por grupo de ventanas";
  - chapter 04, *Frescura antes de una acción*: "por defecto (`screen`) también mira la pantalla
    entera; con `target_window` aplica la misma comparación exacta sólo a los tiles que intersecan
    la unión de rectángulos del grupo" (a third value), and "una opacidad no atestiguada … devuelve
    a la pantalla entera" (not for the material, under this value);
  - chapter 04 §4.5, "La revalidación de la identidad justo antes de la entrega corresponde al
    incremento de vigencia (INC-03)" (this value is such a revalidation, in the engine);
  - chapter 07 §7.8, the D0 paragraphs ("`group_tiles` … sólo aparece cuando se pudo acreditar el
    grupo de ventanas"; "La forma de D0 no cambia") if the owner accepts the fields of point 11,
    the new `freshness_scope` and `identity_us`, and "sólo el operador puede escoger el alcance";
  - chapter 08, *Frescura por grupo de ventanas* ("El alcance opcional `target_window` acota la
    garantía …"), and §8.3, whose residual risks gain the table above;
  - outside the dossier: `action_freshness_diff.meta.schema.json` and its example (if point 11 is
    accepted), the configuration guide and the template, and the CHANGELOG.
- **Watch.** The margin is unmeasured. The query's view of the point can refuse correct actions on
  some controls (the flyout case above). A target the snapshot confirmed only visually gains
  nothing (whole frame).

## Required validation

The implementer plans the new tests, not this ADR; they get their identifiers when they are
planned, each fails first where it tests new behaviour (AGENTS.md §5.10), and they run on
synthetic frames and port doubles. They must show at least:

- a change in the target window beyond the grown box is accepted, while `"screen"` and
  `"target_window"` refuse on the same frames;
- a one-pixel change inside the compared box refuses, the margin refuses exactly the band it
  reaches, and a change inside the accessibility rectangle but outside the snapshot's box refuses;
- a caret blinking inside the target field refuses (no mask);
- a window above the target that appears, moves or changes order between the looks refuses, even
  away from the box; a foreground handle change refuses on identical pixels; a different window
  under the point refuses;
- the identity query answering with a different element refuses with nothing sent, on identical
  pixels; answering with the target passes; answering with a direct child of the target passes;
  answering with a grandchild, with the target's parent (the flyout case) or with a disabled or
  off-screen element refuses;
- every fall-back of point 8 compares the whole captured frame without a second capture and
  reports its reason; with several conditions, the first in the order of point 9 is reported;
- a layered, click-through or no-redirection-bitmap target window, a failed style query and a box
  at a corner give the whole frame; a window attested with only a backdrop material does not;
- a stop engaged, a consent withdrawn, a foreground change and an age expiry during the identity
  query each refuse before delivery (the re-validation of point 9);
- start refuses `"target_element"` without `uia` and without an explicit margin; the default stays
  `"screen"`; nothing new appears in `_meta` under the other two values;
- a geometry or epoch change refuses; a dry run returns before the capture; a refusal spends the
  snapshot; the visual effect is measured over the whole frame;
- over generated frames, layouts and query answers: with the identity the same, the element scope
  refuses on pixels exactly when a changed tile meets the grown box, and it never accepts a changed
  tile inside it;
- the existing freshness tests keep passing unchanged under `"screen"` and `"target_window"`, in
  particular UT-38 and UT-55..UT-75.

Then, before any live use, mirroring ADR-0032 and ADR-0035:

- the condition that stops the window list from being attested on the machine where it will run
  named and resolved (the audit's first unit, A7-1): without the window list this value falls back
  to the whole frame and gains nothing;
- an independent safety review of the compared box, the kept guards, the dropped material
  condition, the identity rule, the re-validation and every fall-back, recorded under
  `docs/reviews/` with each finding and the test that covers it;
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
once. Nothing is persisted and no tool argument changes; the new `_meta` fields can be ignored by a
client. The trigger for reverting is a live result in which this value accepted an action that the
observed screen no longer justified, or a review finding the rules do not cover; either one returns
the switch to `"screen"` and starts a superseding ADR.

## Provenance

[D] Written from the lab reports L1, L3 and L4 of 2026-11-26 (agent-run, exploratory, on a laptop;
kept outside the repository; not the dossier's evidence), from the audit of the freshness rule
(A7: the threats of its §2, `target_window` in its §3, its options in §5, its order and questions
in §6), and from ADR-0022, ADR-0025, ADR-0026, ADR-0032 and ADR-0035. [P] The value, its kept
guards, the dropped material condition, the re-validation after the query and the identity query
are a new proposal; the identity rule is ADR-0022's, unchanged. It refines ADR-0032 (it adds a value
to its switch, and it is the separate ADR that ADR-0032 requires for a perception-derived region)
and changes nothing in ADR-0035 for `"target_window"`. ADR-0022 stays a rule of the supervised
clients. It supersedes nothing. ADR-0039, ADR-0040 and ADR-0043 are cited as proposals on branches
not yet merged.

## What the owner is asked

1. **Accept, reject, or accept with changes** the element scope as written, including the
   standing objection of ADR-0032 that an in-window popup away from the target is not covered.
2. **The window attestation on the laptop.** The window list could not be attested there (L2 §4b),
   and without it this value is the whole frame. Does the owner authorise the read-only diagnostic
   A7-1 first, and does the owner (a) decide on this ADR only after A7-1 has named the failing
   condition, or (b) decide now, knowing that on that laptop the value gains nothing until the
   condition is fixed?
3. **The caret.** Under this ADR the 20 caret refusals of L3 stay refused, so typing into Notepad
   keeps being refused while its caret blinks. Does the owner (a) accept that and defer a caret
   exception to its own ADR, or (b) ask first for a no-input check of whether the operating system
   attests where Notepad 11's caret is (the precondition A7 §5.3 names), before deciding?
4. **The identity rule.** Keep ADR-0022's accepted rule (the element hit is the target or has it as
   parent; recommended), narrow it to the target itself (no direct child), or widen it to any
   ancestor (the wider rule the owner rejected in ADR-0022)?
5. **The margin.** Its value, or the measurement from which the owner will set it; until then the
   value cannot be used, because the key has no default.
6. **The material condition.** Drop ADR-0035's backdrop condition for this value only (point 5), or
   keep it, in which case a Mica window such as Notepad 11's never gets this scope?
7. **No redirection bitmap.** Keep the fall-back to the whole frame for `WS_EX_NOREDIRECTIONBITMAP`
   windows (point 5; recommended), or treat them like the material?
8. **The covering windows.** The identity list over the whole target window (point 3, recommended),
   or only over the grown box (fewer refusals, and a new window over the target window away from
   the box is then accepted)?
9. **The fall-back.** The whole frame (point 8, recommended), or ADR-0035's window scope when it is
   attested?
10. **Reporting.** `freshness_scope` with its reason in `_meta` on every live action under this
    value (point 11; recommended), or only on refusals; and whether D0 gains `scope_tiles` and
    `identity`.
11. **The query's time limit.** 250 ms, the accessibility read's existing budget (point 6), or
    another value?
12. **The gate.** Whether the counterfactual diagnostic comes first; who writes N and the criterion
    of the live measurement, and on which machine; whether the proposed criterion is adopted; which
    review counts as independent; and how consent for each session is given.
13. **A source of identity other than accessibility** for targets the snapshot confirmed only
    visually: leave them on the whole frame (recommended), or ask for a separate proposal?

Status: Proposed

## Resumen (ES)

Propuesta, no decidida: un tercer valor de `[engine] action_freshness`, `"target_element"`, que
compara de forma exacta sólo la caja del elemento objetivo (unida al rectángulo de accesibilidad que
lo confirmó) ampliada por un margen que fija el responsable (la clave no tiene valor por defecto),
conserva la guarda global de geometría y época, la identidad de la ventana de primer plano, de la
ventana bajo el punto y de las ventanas que cubren la ventana objetivo y las edades de ADR-0025, y
añade una consulta de identidad antes de entregar: el elemento bajo el punto debe ser el observado o
tenerlo como padre, la regla que ADR-0022 ya aceptó; tras la consulta se revalidan parada,
consentimiento, política, primer plano, alcance por ventana y edad. Si algo no se puede atestiguar se
compara el fotograma entero y se informa el motivo; si responde otro elemento, `STALE_SNAPSHOT`. El
motivo son las sesiones exploratorias del laboratorio del 2026-11-26 (no evidencia del dossier): 5 de
5 clics rechazados con Notepad sin maximizar y 11 de 13 rechazos sin tocar la caja del objetivo; el
cursor de texto dentro del propio campo sigue rechazando. Deja de proteger contra cambios dentro de
la ventana que no tocan la caja ni el punto (un aviso, otro panel, un ajuste que cambia el sentido
del clic), mantiene la objeción de ADR-0032 sobre un emergente dentro de la ventana, queda apagada
por defecto y exige el diagnóstico de atestación de ventanas, revisión, margen medido y
consentimiento por sesión antes de cualquier uso en vivo.
