# 0046. Scope the observation to the target's window group, behind a switch that stays off

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Drafted by an AI agent (the architect's specifier, decision D50 of the
  coordinator's plan) for the owner to accept, reject or accept with changes, and revised the same
  day after an independent review (recorded in the coordinator's workspace). Nothing is
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
   maximised) are predicted to become regional (that unit's §8; not measured). "**option C**
   (perception scoped to the target's window group, an ADR for the owner) for the largest gain on
   the 43 (Orca minimised: 12.6 ms median against 150–163 ms). C does not replace O3: with perception scoped to Notepad, its own caret still
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
2. **The anchor.** When the operator configured a window pin (ADR-0020, `[safety]
   window_pin_file`), the anchor is the pinned window, and only it: a pin that names no window now
   (`WindowPin::Unset`: the file is missing, empty or not a handle) gives the whole screen with
   `pin_unset`, and never falls through to the foreground window. With no pin configured, the
   anchor is the foreground window. In both cases the operating system attests it, through the
   actuator's window reads (`OsActuator::foreground_window` and `top_level_windows`, the reads
   ADR-0032 and ADR-0035 use), never perception and never a window title (ADR-0020). The anchor
   must be visible, not minimised, not cloaked, its rectangle must meet the captured output, and it
   must pass the same policy an action's window passes: its application is in
   `allowed_applications` and, when `allowed_window_classes` is configured for that application,
   its class is listed. Otherwise the whole screen, with `anchor_unattested` or
   `anchor_not_allowed`. [D] This bounds what a stale pin file (a handle reused by another window)
   or a foreground taken by another application can make the engine observe narrowly: only a
   window the operator already allows. An observation-only configuration that wants the scope must
   therefore list the target application, with `actuation_enabled = false`.
3. **The group.** The group of the anchor `A` is built to a fixed point:
   - it starts with `A` and every window owned by `A`, directly or through a chain of owners,
     wherever it lies on the output (an owned dialog or popup that extends beyond `A`);
   - it adds every visible top-level window that lies above some member in the z-order and whose
     rectangle overlaps that member's rectangle (a menu, a toast, a dialog of another application
     drawn over `A` or over one of `A`'s owned windows), and every window owned by a window it
     adds;
   - it repeats until nothing is added; the walk keeps the bounds of ADR-0032 (no cycle, at most
     4,096 handles), and a walk that exceeds them gives the whole screen.

   Windows that are below every member they overlap are not members. The **scoped region** is the
   union of the members' rectangles, in physical pixels of the captured output, clipped to the
   frame. A group that grows to most of the output gains nothing and harms nothing;
   `region_area_ratio` (point 10) shows it. [D] No opacity attestation is needed for perception,
   unlike ADR-0035 point 4: whatever shows through a translucent member is a pixel inside the region
   and is compared like any other pixel there.
4. **What the port must attest, and where the Win32 calls go.** The owner relation is not attested
   today: `TopLevelWindow` (`src/application/src/ports.rs`) carries a rectangle, a z-order, a class
   and a handle, and the input adapter's window walk (`src/adapters/input/src/sendinput/win.rs`)
   reads no owner. This ADR adds to `TopLevelWindow` an `owner` field (the owner's handle, or none),
   read with `GetWindow(GW_OWNER)` inside the `#[cfg(windows)] mod win` of the input adapter, under
   ADR-0016's rules: one Win32 call per `unsafe` block, each with a `// SAFETY:` comment, no raw
   pointer outside the module, and the thread's last error checked so that a failed query is not
   read as "no owner". The complete window list says whether it attested the owners; an actuator
   whose list does not (every test double and adapter written before this ADR) gives the whole
   screen with `owners_unattested`, never a group without its owned windows. The test doubles and
   the simulated desktop gain owners.
5. **Attestation, read around the capture.** The window list is read before and after the frame
   capture, as ADR-0032 point 6 requires for actions. The scope applies only when both reads are
   complete and equal (z-order, handle, rectangle, owner; every member with an attested rectangle;
   the walk within its bounds), and when the foreground window is a member of the group.
6. **Fail closed to the whole screen, with the reason named.** Otherwise the observation is
   today's whole-screen observation, and its `_meta` names why with one closed name. When several
   hold, the first in this order is reported: `no_window_attestation` (the configured actuator
   attests no windows), `pin_unset`, `anchor_unattested` (no anchor, or the pinned window is gone,
   minimised, cloaked or off the output), `anchor_not_allowed`, `window_read_incomplete` (a member
   without a rectangle, a failed or bounded walk), `owners_unattested`, `window_read_unstable` (the
   two reads differ), `foreground_outside_group` (the foreground window is not a member: with a
   pin, another window has taken the foreground). A geometry or epoch change is today's
   whole-frame full refresh (`geometry_changed`), under both values, and is reported as the plan's
   reason, not as a scope fall-back.
7. **The diff and the plan.** Algorithm 1 runs unchanged on the tiles that meet the scoped region:
   a tile outside it is neither dirty nor pending. The committed base stays the whole captured
   frame, so the geometry, the epoch and an `action_freshness = "screen"` comparison keep working
   on the whole frame. Two transitions force a full refresh **of the region**, with new closed
   reasons in `io.ommatix/plan`: `scope_changed` when the group's ordered list differs from the one
   of the last committed observation (the anchor changed, moved or resized; a member entered, left,
   moved or changed order), and `scope_widened` when the observation falls back to the whole screen
   after a scoped one (a full refresh of the whole frame, because the evidence outside the region
   was dropped). The budgets, ADR-0030's adaptive rule and ADR-0045's closure, if accepted, apply
   inside the region as they do today on the frame.
8. **Evidence outside the region: dropped, never served, never kept with an age.** At the commit
   of a scoped observation, every detector box, word and accessibility element that does not lie
   wholly inside the scoped region is dropped from the held evidence; it is not a support of the
   closure and it is not in the page. It is not kept with its age: ADR-0025 bounds the age of the
   snapshot's capture and of the accessibility read that confirms a target, but a box held from an
   earlier observation has no age of its own (A5 §4 condition 4), so a kept box could reappear as
   current when the scope widens. An element whose box crosses the region's boundary is dropped
   too, and counted (point 10). The accessibility read asks only for the regions inside the scoped
   region.
9. **Identity continuity.** A track whose evidence is dropped by point 8 is retired with a new
   closed reason, `out_of_scope`, recorded with the other reasons of chapter 04 §4.5 and never sent
   on the contract. A track inside the region keeps its ID and revision when the scope narrows. An
   element that leaves the region and comes back gets a new ID, as an element that leaves its
   window and returns already does (ADR-0026); IDs are never reused. The native identity's scope
   (provider, process, window) does not change.
10. **What the page says.** The page of a scoped engine carries the scope it got, so that an agent
    that reads only `structuredContent` does not assume it sees the whole screen, as the page's
    truncation already does for missing controls (chapter 07 §7.3). An optional `scope` field
    (`"screen"` or `"target_group"`) is added to the output of `observe_screen`; it is present on
    every page when the operator configured `"target_group"`, including the pages that fell back,
    and absent otherwise. A new `_meta` key, `io.ommatix/scope`, carries `configured`,
    `effective`, `fallback_reason` (`null` or a name of point 6), `scope_changed` (whether the
    group's list differs from the previous observation's, so that an agent sees that the anchor or
    the group moved, for example when another application took the foreground of an unpinned
    engine), `group_windows` (a count), `region_area_ratio` (the region's share of the frame, three
    decimals) and `dropped_at_boundary` (the count of elements dropped because their box crossed
    the boundary). It never carries a handle, a class, a title, a process name or a coordinate
    (ADR-0012, ADR-0020). **Under the default `"screen"` neither the `scope` field nor the
    `io.ommatix/scope` key appears**, so the payloads, the `_meta` of today and IT-06's checks are
    unchanged. The tool's description stops saying "the screen" and says that the snapshot covers
    the scope the page reports.
11. **The window pin.** The pin keeps doing exactly what ADR-0020 says for actions. The scope never
    widens what may be acted on and never chooses a window the pin excludes. With a pin, a
    foreground window outside the pinned group gives the whole screen (point 6), so the agent sees
    what took the foreground.
12. **Freshness.** Nothing in the freshness check changes. Every value of `action_freshness`
    keeps its own rule and its own window reads at the last look. The combination
    `observation_scope = "target_group"` with `action_freshness = "screen"` is allowed and is safe,
    but it moves the availability problem from the observation to the action: every change outside
    the group still refuses the action (A5 §4). The scope is meant to be paired with ADR-0035's
    `target_window` or with ADR-0047's `target_element` (proposed). No combination lets an action
    compare less than its own value requires.
13. **Several clients and the lease.** If ADR-0039, ADR-0040 and ADR-0043 (proposed on branches not
    yet merged) are accepted: the scope is engine-wide, every client gets the same scoped page,
    and no client and no grant can choose its own scope (a per-client scope is not decided here).
    When one client's action moves the foreground, the next page of every client reflects it,
    with the reason in `_meta`; the lease already serialises the observe-and-act cycle.
14. **Determinism.** The group is ordered by z-order and then by handle; the region is a union of
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
  is the only option that makes the observation idle when only other windows change. Most of its
  machinery (window reads, rectangles, z-order, fall-backs) already exists for freshness; the owner
  relation does not, and the port and the input adapter's `win` module must grow for it (point 4).
  Its cost is the meaning of the page, which is why it is opt-in and the owner's decision.
- **F. A region derived from perception** (for example the target application's boxes). Rejected
  for the same reason ADR-0032 rejected its option C: the scope would rest on the detector finding
  every support, which the operating system does not confirm.

## What the scope stops covering, in plain words

Under `"target_group"` the guarantee of a page narrows from "these are the elements on the screen"
to "these are the elements of the anchor's window group". Each case below says what still catches
it, or that nothing does.

| Change or situation | What still catches it |
|---|---|
| A window of another application, or a second window of the same application, appears outside the group (not owned by a member, not above a member and overlapping it) without taking the foreground: a dialog, an error, a notification in a corner, the taskbar and its clock | Nothing at observation: the agent does not see it. If it takes the foreground, point 6 or point 2 applies (below) |
| With a pin, any other window takes the foreground | Whole screen, `foreground_outside_group` (point 6) |
| Without a pin, another application takes the foreground | It becomes the anchor if the policy allows it: the page then shows only the new window's group and the original target's elements are retired (`scope_changed` in `_meta`, point 10). If the policy does not allow it, the whole screen, `anchor_not_allowed`. Nothing makes the engine keep observing the original window; a pin does |
| A stale pin file whose handle another window has reused | Only a window the policy allows can be the anchor (point 2); a reused handle of an allowed application's other window is observed as if pinned, the same residual ADR-0020 has for actions |
| A popup of the anchor's own application that is not an owned window and neither overlaps a member nor lies above one | Nothing at observation; if it takes the foreground, as above |
| An element of a member whose box crosses the region's boundary (a flyout of the anchor drawn partly outside the members' rectangles) | Nothing serves it: it is dropped (point 8) and counted in `dropped_at_boundary` (point 10). A flyout that is a window of its own above the anchor and overlapping it is a member, and its whole rectangle is in the region |
| A window that the operating system reports late, after the second read | Nothing at that observation; the next one sees it if it is a member, or if it takes the foreground. Between the observation and an action, the freshness rule (below) |
| The content of other windows the task needs (read one window, act on another) | Nothing: a loss of capability, not of safety |
| Absence as information ("there is no dialog on the screen") | The page says its scope (point 10), so the agent can know it cannot conclude it |
| A window that comes to cover the target between the observation and the action | Not perception: the freshness rule, whatever the observation scope (the whole-frame comparison of `screen`, the identity of the windows above the target of ADR-0035, the same plus the element under the point of ADR-0047). This ADR never relies on perception to refuse an action |
| A window that comes to cover the anchor before an observation | It is a member if it lies above the anchor and overlaps it, so that observation sees it, with `scope_changed` |

The residual risk is a reasoning risk on the agent's side, as ADR-0032 and ADR-0035 state for
actions: if what happens outside the group matters to the task, an agent working from scoped pages
will not know it. The owner accepts or rejects that knowingly.

## Consequences and cost

- **Easier.** On the measured scene the observation can become idle while only other windows
  change: the lab's proxy is 12.6 ms against 150–163 ms (Context, item 2), not a measurement of
  this scope. Pages get shorter, because elements of other windows are not served.
- **Harder.** Two window reads per observation, as the freshness check does today; their cost goes
  in a new `scope_us` of `io.ommatix/timings` (present only under `"target_group"`) and must be
  reported before and after (ADR-0010). The port and the input adapter's `win` module grow by the
  owner relation (point 4), under ADR-0016. The actuator port is the only one that attests windows
  today, so a configuration whose actuator attests none gets the whole screen
  (`no_window_attestation`); whether the window reads move to a port of their own is not decided
  here. The test doubles and the simulated desktop need the window reads, with owners, at
  observation.
- **The window attestation on the lab's laptop.** The window list this scope needs is the one the
  M1 probe could not attest there ("M1 window group cannot be attested", L2 §4b), for a condition
  the probe does not name. The audit of freshness proposes a read-only diagnostic that names it
  (A7-1). Until it is named and resolved, on that laptop this scope may fall back to the whole
  screen on every observation and gain nothing; the measurement below would show it as
  `window_read_incomplete` or `window_read_unstable`.
- **It changes what an observation means**, so the dossier changes with it, in the same pull
  request as the code (AGENTS.md §1). The sentences that become false or incomplete:
  - chapter 03, RF-02: "Detectar todos los cambios RGB en fixtures" (under the scope, the changes
    outside the region are not detected, by design);
  - chapter 03, RF-10: "Snapshot completo versionado" (complete within the scope the page
    reports);
  - chapter 02 §2.4, on what `OsActuator` attests in `TopLevelWindow` ("`OsActuator` puede
    atestiguar una lista completa y ordenada por z-order/handle de ventanas visibles con
    rectángulos físicos opcionales"): the owner, and the reads at observation;
  - chapter 02 §2.6, which names ADR-0032 as the only window scope;
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
  - chapter 07 §7.8: "La configuración no aparece en argumentos de las herramientas: sólo el
    operador puede escoger el alcance" (it now names both scopes);
  - chapter 08 §8.2, *Consentimiento y alcance*, which describes the window allowlist as a scope of
    actions only, and §8.3, whose residual risks gain the table above;
  - outside the dossier: `observe_screen.output.schema.json` and its example, the tool's
    description (`tools.rs:60`), the configuration guide and the template, and the CHANGELOG.
- **Contract.** The `structuredContent` schema gains one optional field (point 10). Whether the
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
- an element wholly outside the region, and one whose box crosses the boundary, are dropped,
  retired as `out_of_scope` and counted; an element that leaves and returns gets a new ID; no ID is
  reused;
- a window above the anchor that overlaps it, a window owned by the anchor (directly or through a
  chain) outside the anchor's rectangle, and a window above an owned window that overlaps it
  outside the anchor's rectangle are members and their elements are served; a window below every
  member it overlaps is not a member;
- a change of the group's ordered list gives a full refresh of the region with `scope_changed`;
- every fall-back of point 6 gives the whole screen with its reason, the first in point 6's order
  when several hold, and `scope_widened` when the previous observation was scoped; in particular a
  configured pin that names no window gives `pin_unset` and never the foreground window, an anchor
  whose application or class the policy does not allow gives `anchor_not_allowed`, and a window
  list that does not attest owners gives `owners_unattested`;
- without a pin, a foreground switch to an allowed application moves the anchor with
  `scope_changed`, and to an application the policy does not allow gives the whole screen;
- a geometry or epoch change gives today's whole-frame full refresh under both values;
- the page carries `scope` exactly when the operator configured `"target_group"`;
  `io.ommatix/scope` carries only the fields of point 10, with no handle, class, title or
  coordinate (in the manner of IT-06); under `"screen"` neither appears, nor `scope_us`;
- the owner query in the Windows adapter treats a failed query as unattested, not as "no owner"
  (a pure helper, tested as the adapter's other helpers are);
- the default is `"screen"`, an unknown value fails at start, and no tool argument changes the
  scope;
- under `"screen"` every existing test passes unchanged, in particular UT-01..UT-12, UT-38 and
  UT-55..UT-75.

Then, before any live use, mirroring ADR-0032 and ADR-0035:

- the condition that stops the window list from being attested on the machine where it will run
  named and resolved (the audit's A7-1), since without it this scope gains nothing there;
- an independent safety review of the group, the owner query and its `unsafe` blocks, the
  fall-backs, the dropped evidence, the retirements and the page's wording, recorded under
  `docs/reviews/` with each finding and the test that covers it;
- a no-input measurement whose N and criterion the owner writes before the run, on a desktop the
  owner names. A proposal for the owner to adopt, change or replace: the lab's scene (a) of L2
  (Notepad at its default size in front of live terminals, Notepad pinned and allowed), the same
  binary with `"screen"` and `"target_group"`, three runs of ten observations each; the scope is
  reported effective in every observation after the first, or each fall-back is reported with its
  reason; the median `total` of observations 2–10 under the scope is at most 30 ms while nothing
  inside the target changes; an operator-side check finds no served element outside the attested
  region; and a scripted foreground change gives the whole screen with `foreground_outside_group`
  in every trial;
- with the owner's consent for each session, a flow with input under the scope (for example
  S4R-03), with the freshness value the owner chooses;
- `cargo test --workspace`, the clippy runs with the Windows features and the thesis checks, and
  the observation's stage timings reported before and after.

## Reversibility

Setting `observation_scope = "screen"` restores today's observation at once. Nothing is persisted
and nothing in the wire changes except the optional `scope` field and the new `_meta` key, which
appear only under `"target_group"`. The trigger for reverting is a live result in which a scoped
page hid something the agent needed and the fall-backs did not catch, or a review finding that the
rules do not cover; either one returns the switch to `"screen"` and starts a superseding ADR.

## Provenance

[D] Written from the lab reports L0–L4 of 2026-11-26 (agent-run, exploratory, on a laptop; kept
outside the repository; not the dossier's evidence), the audits of the full refresh (A5, its §4 and
option C of its §5) and of freshness (A7, for the window attestation on the laptop), the opening
section of the unit that proposes ADR-0045, and the code and records cited above; revised the same
day after an independent review (recorded in the coordinator's workspace). [P] The scope, its group,
its fall-backs, the owner field and its page fields are a new proposal. It uses ADR-0012
(minimisation), ADR-0016 (where the owner query's `unsafe` goes), ADR-0020 (the pin, the window
policy and the ban on titles as evidence), ADR-0025 (ages), ADR-0026 (native identity and
retirement), ADR-0030 (the adaptive rule) and the window reads of ADR-0032 and ADR-0035, and it
changes none of them. It supersedes nothing. ADR-0039, ADR-0040, ADR-0041, ADR-0043, ADR-0044 and
ADR-0045 are cited as proposals on branches not yet merged.

## What the owner is asked

1. **Accept, reject, or accept with changes** the scope of perception as written.
2. **The window attestation on the laptop.** The window list could not be attested there
   (L2 §4b). Does the owner authorise the read-only diagnostic A7-1 first, and does the owner
   (a) decide on this ADR only after A7-1 has named the failing condition, or (b) decide now,
   knowing that on that laptop the scope may gain nothing until the condition is fixed?
3. **The anchor.** The pinned window when a pin is set, otherwise the foreground window (point 2);
   or the pinned window only, so that an unpinned engine always observes the whole screen (and a
   foreground switch never moves the scope)?
4. **The anchor's policy check.** Require the anchor to pass `allowed_applications` and
   `allowed_window_classes` (point 2; recommended), or not?
5. **The group.** Owned windows and the windows above any member that overlap it, to a fixed point
   (point 3; recommended), or the anchor and the windows above it only, without owned windows?
6. **The owner relation and ADR-0016.** Does the owner accept the `GetWindow(GW_OWNER)` call in the
   input adapter's `win` module as within ADR-0016's bounds (point 4), or does it need its own
   amendment of ADR-0016? (Moot if question 5 drops owned windows.)
7. **Evidence outside: dropped** (point 8; recommended, because a held box has no age of its own),
   or kept and marked as not refreshed?
8. **The page.** An optional `scope` field in `structuredContent` plus `io.ommatix/scope` in
   `_meta`, both only under `"target_group"` (point 10), or `_meta` only; and does the contract keep
   the version Ommatix 1.0?
9. **The gate.** Who writes N and the criterion of the no-input measurement, and on which desktop
   (the owner's, the reference PC or the lab's laptop); whether the proposed criterion is adopted;
   which review counts as independent; and whether observation-only lab sessions under the protocol
   of 2026-11-26 count as consented or need consent by name.
10. **The pairing.** Allow `"target_group"` with `action_freshness = "screen"` (point 12; safe, but
    the outside changes then refuse actions), or refuse that combination at start?

Status: Proposed

## Resumen (ES)

Propuesta, no decidida: un alcance de observación opcional, `[engine] observation_scope =
"target_group"`, bajo el cual la diferencia de teselas, el plan, la evidencia y la página cubren
sólo el grupo de ventanas de un ancla (la ventana registrada si hay registro, que nunca cae a la de
primer plano; si no la de primer plano), que además debe permitir la política: el ancla, sus
ventanas propias y las ventanas por encima de algún miembro que lo solapan, todo atestiguado por el
sistema operativo; la relación de propiedad exige ampliar el puerto y el módulo `unsafe` de ADR-0016.
Lo de fuera se descarta, no se sirve y sus pistas se retiran con el motivo `out_of_scope`; sin
atestación completa, o si el primer plano sale del grupo, se observa la pantalla entera y `_meta`
dice por qué; con el alcance por defecto no aparece nada nuevo. El motivo son las sesiones
exploratorias del laboratorio del 2026-11-26 (no evidencia del dossier): con terminales vivas detrás
de Notepad cada observación era un refresco completo de 150–163 ms, y la misma escena con Orca
minimizado costaba 12,6 ms; en ese portátil la lista de ventanas aún no se pudo atestiguar. Cambia lo
que significa una observación (capítulos 02, 03, 04, 05, 07 y 08), deja de ver ventanas que aparecen
fuera del grupo sin tomar el primer plano, no cambia ninguna regla de frescura y queda apagado por
defecto hasta que el responsable la acepte y se cumplan el diagnóstico, la revisión y la medición
previos.
