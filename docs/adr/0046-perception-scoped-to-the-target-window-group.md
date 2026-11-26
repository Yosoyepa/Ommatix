# 0046. Scope the observation to the target's window group, behind a switch that stays off

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Drafted by an AI agent (the architect's specifier, decision D50 of the
  coordinator's plan) for the owner to accept, reject or accept with changes. Nothing is
  implemented, nothing in this record was measured with the proposed scope, and no test status,
  requirement status or checkbox changes with it. Merging the branch that carries it is not its
  acceptance: the owner accepts it in writing, as for ADR-0032 and ADR-0035.

## Context

**What an observation covers today.** `observe_screen` captures the whole monitor and compares it,
tile by tile and exactly, with the committed base (Algorithm 1, chapter 04 §4.2: "for every valid
tile: dirty[tile] = any_color_pixel_diff(current, committed_base)"). Every changed tile is planned,
closed over the supports of the last snapshot and re-analysed, and the page holds every element of
the screen. The tool describes itself as "the screen as a full snapshot"
(`src/adapters/mcp/src/tools.rs:60`). Three things already narrow by window, but none narrows
perception: UI Automation reads only the process of the foreground window (ADR-0017;
`src/adapters/accessibility/src/uia.rs`), and the application and window allowlist and the window
pin (ADR-0020) and `action_freshness = "target_window"` (ADR-0032, amended by ADR-0035) restrict
**actions**.

**The evidence.** It comes from agent-run, exploratory sessions on a laptop (1920 × 1200 at 150 %,
one monitor) on 2026-11-26, under the owner's live-lab protocol. They are lab engineering, **not
the dossier's evidence**: no supervised run, no reference PC, no criterion written before the run
by the owner. The reports are kept outside the repository, in the owner's workspace. Quoted
exactly:

1. *Every observation is a full refresh while anything else on the screen changes.* "after the
   first, every observation is `work: full`, `reason: area_budget`, `region_area_px = 2 304 000`
   (the whole screen), 100–185 ms" (L0, finding 1), against 14–28 ms with Notepad maximised and
   still.
2. *Changes outside the target are enough.* With Notepad at its default size in front of a
   maximised Orca window whose worker terminals were live: "full (`area_budget`) ×9", median
   163 ms. The same scene with Orca minimised: "**idle ×9**", "**12.6** (12.1–17.1)" ms (L2 §3).
   The plan trace of the same scene, three runs pooled: full refreshes with a median `total` of
   150.5 ms (L2b).
3. *Small changes, closed over a window-sized support.* "**H1, and only H1, as far as the data can
   tell**": 47 full refreshes, each from 2–28 changed tiles "closed over one evidence box of about
   49–82% of the frame" (L2b). "Each source is enough, alone, to force a full refresh: Orca's
   terminals (whenever they are visible); the taskbar clock (once a minute); Notepad's own
   blinking caret" (L2 §3).
4. *Configuration does not help.* "No setting of `adaptive_min_regions` or
   `full_refresh_area_ratio` below 1.0 turned a single observation regional"; at 1.0 "every
   observation became one region of 69.5%, and it cost the same as a full refresh" (L2).
5. *What the support-aware closure can and cannot do.* The unit that proposes ADR-0045 splits the
   47: 43 come from Orca's terminals behind the target, in 4–6 components each, and stay full
   refreshes after that closure under the product configuration; 4 (the clock, the caret, Notepad
   maximised) become regional. "**option C** (perception scoped to the target's window group, an
   ADR for the owner) for the largest gain on the 43 (Orca minimised: 12.6 ms median against
   150–163 ms). C does not replace O3: with perception scoped to Notepad, its own caret still
   closes over Notepad's window-sized support and re-reads the window" (unit O3, opening section).

The audit of the full refresh (A5, §4) lists what ties an observation to the whole screen: the
whole-frame capture and diff (`dirty_tiles_in_regions` exists but only the freshness check uses
it), a registry and a page that hold every element, supports taken from all of them, and a
screen-wide pending set. It lists five conditions that perception scoped to a window would have to
meet, and the risks: a dialog that does not yet overlap the target, the availability problem moving
from the observation to the action when freshness still compares the whole screen, and the meaning
of the contract.

**What the 12.6 ms is and is not.** It is the cost of an observation when nothing changes on the
screen, measured by minimising Orca. It is the best that the proposed scope could reach on that
scene, not a measurement of the scope. It holds only while nothing inside the target changes; a
change there still costs what ADR-0045 and Algorithm 1 make it cost.

## Decision

Proposed: a second **observation scope**, chosen by the operator, under which the engine's
observation (the diff, the plan, the evidence and the page) covers the window group of an anchor
window and ignores changes outside it. The default stays the whole screen. Each point below is
written so that an implementation can be checked against it.

1. **Switch.** `[engine] observation_scope = "screen"` is the default and is exactly today's
   observation. `"target_group"` selects this ADR. The value is validated at start like the other
   `[engine]` keys; an unknown value fails at start. It is the operator's: no tool argument, cursor
   or grant selects, widens or narrows it, and the agent cannot set it.
2. **The anchor.** The anchor is the window the operator pinned (ADR-0020, `[safety]
   window_pin_file`) when a pin is set; otherwise it is the foreground window. In both cases the
   operating system attests it, through the actuator's window reads (`OsActuator::foreground_window`
   and `top_level_windows`, the reads ADR-0032 and ADR-0035 use), never perception and never a
   window title (ADR-0020). The anchor must be visible, not minimised, not cloaked, and its
   rectangle must meet the captured output.
3. **The group.** The group of the anchor `A` is:
   - `A` itself;
   - every window owned by `A`, by the operating system's owner relation, wherever it lies on the
     output (an owned dialog or popup that extends beyond `A`);
   - every visible top-level window above `A` in the z-order whose rectangle overlaps `A`'s
     rectangle (a menu, a toast, a dialog of another application drawn over `A`).

   Windows below `A` are not members. The **scoped region** is the union of the members'
   rectangles, in physical pixels of the captured output, clipped to the frame. [D] No opacity
   attestation is needed for perception, unlike ADR-0035 point 4: whatever shows through a
   translucent `A` is a pixel inside the region and is compared like any other pixel there.
4. **Attestation, read around the capture.** The window list is read before and after the frame
   capture, as ADR-0032 point 6 requires for actions. The scope applies only when both reads are
   complete and equal (z-order, handle, rectangle; every member with an attested rectangle; the
   z-order walk within its bounds of no cycle and at most 4,096 handles), and when the foreground
   window is a member of the group.
5. **Fail closed to the whole screen, with the reason named.** Otherwise the observation is
   today's whole-screen observation, and its `_meta` names why, with one closed name:
   `anchor_unattested` (no anchor, or the pinned window is gone, minimised or cloaked),
   `window_read_unstable` (the two reads differ), `window_read_incomplete` (a member without a
   rectangle, or a failed or bounded walk), `foreground_outside_group` (the foreground window is not
   a member: another window, of this application or of another, has taken the foreground), or
   `no_window_attestation` (the configured actuator attests no windows). A geometry or epoch change
   is today's whole-frame full refresh (`geometry_changed`), under both values.
6. **The diff and the plan.** Algorithm 1 runs unchanged on the tiles that meet the scoped region:
   a tile outside it is neither dirty nor pending. The committed base stays the whole captured
   frame, so the geometry, the epoch and an `action_freshness = "screen"` comparison keep working
   on the whole frame. Two transitions force a full refresh **of the region**, with new closed
   reasons in `io.ommatix/plan`: `scope_changed` when the group's ordered list differs from the one
   of the last committed observation (the anchor changed, moved or resized; a member entered, left,
   moved or changed order), and `scope_widened` when the observation falls back to the whole screen
   after a scoped one (a full refresh of the whole frame, because the evidence outside the region
   was dropped). The budgets, ADR-0030's adaptive rule and ADR-0045's closure, if accepted, apply
   inside the region as they do today on the frame.
7. **Evidence outside the region: dropped, never served, never kept with an age.** At the commit
   of a scoped observation, every detector box, word and accessibility element that does not lie
   wholly inside the scoped region is dropped from the held evidence; it is not a support of the
   closure and it is not in the page. It is not kept with its age, because visual evidence has no
   age bound (ADR-0025 bounds only the accessibility read, A5 §4 condition 4), so a kept box could
   reappear as current when the scope widens. The accessibility read asks only for the regions
   inside the scoped region.
8. **Identity continuity.** A track whose evidence is dropped by point 7 is retired with a new
   closed reason, `out_of_scope`, recorded with the other reasons of chapter 04 §4.5 and never sent
   on the contract. A track inside the region keeps its ID and revision when the scope narrows. An
   element that leaves the region and comes back gets a new ID, as an element that leaves its
   window and returns already does (ADR-0026); IDs are never reused. The native identity's scope
   (provider, process, window) does not change.
9. **What the page says.** The page of a scoped engine carries the scope it got, so that an agent
   that reads only `structuredContent` does not assume it sees the whole screen, as the page's
   truncation already does for missing controls (chapter 07 §7.3). An optional `scope` field
   (`"screen"` or `"target_group"`) is added to the output of `observe_screen`; it is present on
   every page when the operator configured `"target_group"`, including the pages that fell back,
   and absent otherwise, so that existing payloads stay valid. A new `_meta` key,
   `io.ommatix/scope`, carries `configured`, `effective`, `fallback_reason` (`null` or a name of
   point 5), `group_windows` (a count) and `region_area_ratio` (the region's share of the frame,
   three decimals). It never carries a handle, a class, a title, a process name or a coordinate
   (ADR-0012, ADR-0020). The tool's description stops saying "the screen" and says that the
   snapshot covers the scope the page reports.
10. **The window pin.** The pin keeps doing exactly what ADR-0020 says for actions. The scope never
    widens what may be acted on and never chooses a window the pin excludes. With a pin, a
    foreground window outside the pinned group gives the whole screen (point 5), so the agent sees
    what took the foreground.
11. **Freshness.** Nothing in the freshness check changes. Every value of `action_freshness`
    keeps its own rule and its own window reads at the last look. The combination
    `observation_scope = "target_group"` with `action_freshness = "screen"` is allowed and is safe,
    but it moves the availability problem from the observation to the action: every change outside
    the group still refuses the action (A5 §4). The scope is meant to be paired with ADR-0035's
    `target_window` or with ADR-0047's `target_element` (proposed). No combination lets an action
    compare less than its own value requires.
12. **Several clients and the lease.** If ADR-0039, ADR-0040 and ADR-0043 (proposed on branches not
    yet merged) are accepted: the scope is engine-wide, every client gets the same scoped page,
    and no client and no grant can choose its own scope (a per-client scope is not decided here).
    When one client's action moves the foreground, the next page of every client reflects it,
    with the reason in `_meta`; the lease already serialises the observe-and-act cycle.
13. **Determinism.** The group is ordered by z-order and then by handle; the region is a union of
    rectangles with the existing `BBox` operations; no collection is iterated in hash order. The
    window reads become an input of the observation, so tests drive them through port doubles:
    the same frames and the same window reads give the same page.

## Alternatives considered

- **A. The whole screen, as today.** Safe and simple. On an ordinary desktop it costs a full
  refresh on almost every observation (L0, L2), and it serves elements of windows the agent does
  not work in (in L0 a page held 99 OCR words that, "by their text, … are from the windows
  behind" Notepad). It stays the default.
- **B. Several regions counted by area** (A5 option A; ADR-0030 counts regions because each
  region is a whole inference of the ONNX detector). By configuration it does not help on the
  measured scenes (Context, item 4). Making ADR-0030's rule depend on the detector's cost model is
  worth doing for detectors whose cost grows with pixels, but it still processes every change on
  the screen, including the terminals that the agent never looks at. Complementary, not a
  replacement.
- **C. The mosaic of ADR-0041** (proposed: one detector inference for several regions). It lowers
  the per-region cost of an ONNX detector only. It touches neither OCR nor UI Automation, it does
  nothing with the flat-region detector, and it still processes changes outside the target. It
  does not answer this problem.
- **D. The support-aware closure of ADR-0045** (proposed). Necessary for the target's own small
  changes, and by its unit's own analysis it makes 4 of the 47 measured refreshes regional; the
  other 43 come from outside the target. Complementary: this ADR removes the outside changes, and
  ADR-0045 keeps the inside changes small.
- **E. This ADR: the anchor's window group, attested by the operating system** (recommended). It
  is the only option that makes the observation idle when only other windows change, and its
  machinery (window reads, rectangles, z-order, fall-backs) already exists for freshness. Its cost
  is the meaning of the page, which is why it is opt-in and the owner's decision.
- **F. A region derived from perception** (for example the target application's boxes). Rejected
  for the same reason ADR-0032 rejected its option C: the scope would rest on the detector finding
  every support, which the operating system does not confirm.

## What the scope stops covering, in plain words

Under `"target_group"` the guarantee of a page narrows from "these are the elements on the screen"
to "these are the elements of the anchor's window group". The agent no longer sees:

- **a window that appears outside the group without taking the foreground**: a dialog or an
  error of another application, or a second window of the same application, that does not overlap
  the anchor and is not owned by it; a notification in a corner of the screen; the taskbar and its
  clock. If such a window takes the foreground, the scope falls back to the whole screen
  (point 5) and the agent sees it on that observation;
- **the content of other windows**: an agent that needs to read one window and act on another
  cannot do it under this scope. That is a loss of capability, not of safety;
- **absence as information**: an agent can no longer conclude "there is no dialog on the screen"
  from a scoped page; the page says its scope (point 9).

Two threats stay covered, and not by perception:

- **a window that comes to cover the target.** If it is above the anchor and overlaps it, it is a
  member of the group, so the next observation sees it. Between the observation and the action, it
  is the freshness rule that must catch it, whatever the observation scope: the whole-frame
  comparison (`screen`), the identity of the windows above the target (ADR-0035) or the same
  identity plus the element under the point (ADR-0047). This ADR does not rely on perception to
  refuse an action;
- **a dialog of another application that takes the foreground**: whole screen, `foreground_outside_group`.

The residual risk is a reasoning risk on the agent's side, as ADR-0032 and ADR-0035 state for
actions: if what happens outside the group matters to the task, an agent working from scoped pages
will not know it. The owner accepts or rejects that knowingly.

## Consequences and cost

- **Easier.** On the measured scene the observation can become idle while only other windows
  change: the lab's proxy is 12.6 ms against 150–163 ms (Context, item 2), not a measurement of
  this scope. Pages get shorter, because elements of other windows are not served.
- **Harder.** Two window reads per observation, as the freshness check does today; their cost goes
  in a new `scope_us` of `io.ommatix/timings` and must be reported before and after (ADR-0010).
  The actuator port is the only one that attests windows today, so a configuration whose actuator
  attests none gets the whole screen (`no_window_attestation`); whether the window reads move to a
  port of their own is not decided here. The test doubles and the simulated desktop need the
  window reads at observation.
- **It changes what an observation means**, so the dossier changes with it, in the same pull
  request as the code (AGENTS.md §1). The sentences that become false or incomplete:
  - chapter 03, RF-02: "Detectar todos los cambios RGB en fixtures" (under the scope, the changes
    outside the region are not detected, by design);
  - chapter 04 §4.2, *Entradas, salidas y precondiciones*: "Entradas: frame actual, último frame
    base confirmado, regiones sucias pendientes y soportes visuales del último snapshot" (the scoped
    region becomes an input);
  - chapter 04 §4.2, the reference pseudocode: "for every valid tile:" (every valid tile inside the
    scope);
  - chapter 04 §4.2, *Regiones y halo*: "El fallback también aplica por TTL, cambio de ventana o
    demasiadas regiones" (and the new reasons `scope_changed` and `scope_widened`);
  - chapter 04, *Frescura antes de una acción*: "La planificación incremental del Algoritmo 1 sigue
    usando todos los tiles frente a la base comprometida";
  - chapter 04 §4.5, the list of retirement reasons (`vanished`, `moved`, `substituted`,
    `ambiguous`, `geometry_changed`), which gains `out_of_scope`;
  - chapter 05 §5.4: "UT-01 a UT-15 cubren incrementalidad y commit" and the list that follows,
    which gains the range of the scope's tests;
  - chapter 07 §7.3: "La observación inicial retorna `session_id`, `snapshot_id`,
    `geometry_epoch`, frame_seq, age_ms, degraded, elementos y truncamiento explícito" (and the
    scope); "Se informa truncamiento para que el agente no asuma que ve todos los controles" (the
    scope is reported for the same reason); the list of `io.ommatix/plan` reasons and the new
    `io.ommatix/scope` and `scope_us`;
  - chapter 07 §7.7: "La configuración no aparece en argumentos de las herramientas: sólo el
    operador puede escoger el alcance" (it now names both scopes);
  - chapter 08 §8.2, *Consentimiento y alcance*, which describes the window allowlist as a scope of
    actions only, and §8.3, whose residual risks gain the paragraph above;
  - chapter 02 §2.6, which names ADR-0032 as the only window scope;
  - outside the dossier: `observe_screen.output.schema.json` and its example, the tool's
    description (`tools.rs:60`), the configuration guide and the template, and the CHANGELOG.
- **Contract.** The `structuredContent` schema gains one optional field (point 9). Whether the
  contract stays "Ommatix 1.0" with an additive optional field is the owner's decision.
- **Detectors.** With the ONNX detector, a full refresh of the region is one inference of a crop,
  seen at another scale than the whole frame; ADR-0030's measurements found that crops change what
  the detector publishes (§13.10 of `S0_RESULTADOS.md`). The element counts under the scope must be
  measured, not assumed equal.

## Required validation

The implementer plans the new tests, not this ADR; they get their identifiers when they are
planned, each fails first where it tests new behaviour (AGENTS.md §5.10), and they run on
synthetic frames and port doubles. They must show at least:

- a change outside the group leaves the observation `idle`, and no element of a window outside the
  group is in the page;
- a change inside the anchor is planned exactly as `screen` plans it on the same frames, for every
  generated layout in which no support crosses the region's boundary (a differential test against
  the whole-screen plan, in the manner of UT-08 for tiles);
- an element wholly outside the region is dropped and retired as `out_of_scope`; an element that
  leaves and returns gets a new ID; no ID is reused;
- a window above the anchor that overlaps it, and an owned popup outside the anchor's rectangle,
  are members and their elements are served; a window below the anchor is not a member;
- a change of the group's ordered list gives a full refresh of the region with `scope_changed`;
- a foreground window outside the group, an unattested anchor, two window reads that differ, a
  member without a rectangle and a failed walk each give the whole screen, with the reason of
  point 5 and `scope_widened` when the previous observation was scoped;
- a geometry or epoch change gives today's whole-frame full refresh under both values;
- the page carries `scope` exactly when the operator configured `"target_group"`, and
  `io.ommatix/scope` carries only the fields of point 9, with no handle, class, title or
  coordinate (in the manner of IT-06);
- the default is `"screen"`, an unknown value fails at start, and no tool argument changes the
  scope;
- under `"screen"` every existing test passes unchanged, in particular UT-01..UT-12, UT-38 and
  UT-55..UT-75.

Then, before any live use, mirroring ADR-0032 and ADR-0035:

- an independent safety review of the group, the fall-backs, the dropped evidence, the
  retirements and the page's wording, recorded under `docs/reviews/` with each finding and the
  test that covers it;
- a no-input measurement whose N and criterion the owner writes before the run, on a desktop the
  owner names. A proposal for the owner to adopt, change or replace: the lab's scene (a) of L2
  (Notepad at its default size in front of live terminals), the same binary with `"screen"` and
  `"target_group"`, three runs of ten observations each; the scope is reported effective in every
  observation after the first, or each fallback is reported with its reason; the median `total` of
  observations 2–10 under the scope is at most 30 ms while nothing inside the target changes; an
  operator-side check finds no served element outside the attested region; and a scripted
  foreground change gives the whole screen with `foreground_outside_group` in every trial;
- with the owner's consent for each session, a flow with input under the scope (for example
  S4R-03), with the freshness value the owner chooses;
- `cargo test --workspace`, the clippy runs with the Windows features and the thesis checks, and
  the observation's stage timings reported before and after.

## Reversibility

Setting `observation_scope = "screen"` restores today's observation at once. Nothing is persisted
and nothing in the wire changes except the optional `scope` field and the new `_meta` key, which a
client that does not know them can ignore. The trigger for reverting is a live result in which a
scoped page hid something the agent needed and the fall-backs did not catch, or a review finding
that the rules do not cover; either one returns the switch to `"screen"` and starts a superseding
ADR.

## Provenance

[D] Written from the lab reports L0–L4 of 2026-11-26 (agent-run, exploratory, on a laptop; kept
outside the repository; not the dossier's evidence), the audit of the full refresh (A5, its §4 and
option C of its §5), the opening section of the unit that proposes ADR-0045, and the code and
records cited above. [P] The scope, its group, its fall-backs and its page fields are a new
proposal. It uses ADR-0012 (minimisation), ADR-0020 (the pin and the ban on titles as evidence),
ADR-0025 (ages), ADR-0026 (native identity and retirement), ADR-0030 (the adaptive rule) and the
window reads of ADR-0032 and ADR-0035, and it changes none of them. It supersedes nothing. ADR-0039,
ADR-0040, ADR-0041, ADR-0043, ADR-0044 and ADR-0045 are cited as proposals on branches not yet
merged.

## What the owner is asked

1. **Accept, reject, or accept with changes** the scope of perception as written.
2. **The anchor.** The pinned window when a pin is set, otherwise the foreground window (point 2);
   or the pinned window only, so that an unpinned engine always observes the whole screen.
3. **The group.** Owned windows and windows above the anchor that overlap it (point 3); or the
   anchor and the windows above it only, without owned windows elsewhere on the output.
4. **Evidence outside: dropped** (point 7), or kept and marked as not refreshed. This ADR
   recommends dropping it, because visual evidence has no age bound.
5. **The page.** An optional `scope` field in `structuredContent` plus `io.ommatix/scope` in
   `_meta` (point 9), or `_meta` only; and whether the contract keeps the version Ommatix 1.0.
6. **The gate.** Who writes N and the criterion of the no-input measurement, on which desktop (the
   owner's, the reference PC or the lab's laptop), whether the proposed criterion is adopted, which
   review counts as independent, and whether observation-only lab sessions under the protocol of
   2026-11-26 count as consented or need consent by name.
7. **The pairing.** Whether to allow `"target_group"` with `action_freshness = "screen"` (point 11;
   safe, but the outside changes then refuse actions), or to refuse that combination at start.

Status: Proposed

## Resumen (ES)

Propuesta, no decidida: un alcance de observación opcional, `[engine] observation_scope =
"target_group"`, bajo el cual la diferencia de teselas, el plan, la evidencia y la página cubren
sólo el grupo de ventanas de un ancla (la ventana registrada si hay una, si no la de primer plano):
el ancla, sus ventanas propias y las ventanas por encima que la solapan, todo atestiguado por el
sistema operativo. Lo de fuera se descarta, no se sirve y sus pistas se retiran con el motivo
`out_of_scope`; sin atestación completa, o si el primer plano sale del grupo, se observa la pantalla
entera y `_meta` dice por qué. El motivo son las sesiones exploratorias del laboratorio del
2026-11-26 (no evidencia del dossier): con terminales vivas detrás de Notepad cada observación era
un refresco completo de 150–163 ms, y la misma escena con Orca minimizado costaba 12,6 ms. Cambia lo
que significa una observación (capítulos 03, 04, 05, 07 y 08), deja de ver ventanas que aparecen
fuera del grupo sin tomar el primer plano, no cambia ninguna regla de frescura y queda apagado por
defecto hasta que el responsable la acepte y se cumplan la revisión y la medición previas.
