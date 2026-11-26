# 0045. Keep a small change small inside a window-sized support (support-aware closure)

- **Status:** Proposed
- **Date:** 2026-11-26
- **Approval:** Pending. Merging the branch that carries it is the owner's acceptance (as for
  ADR-0039, decision D25 of the coordinator's plan); the architect confirmed the four readings
  of the unit (decision D48).

## Context

Algorithm 1 (dossier chapter 04 §4.2) pads each component of dirty tiles with a halo and closes
the region over every box of the last snapshot's evidence that it touches, until a fixed point:
a region never cuts a box, so evidence that is kept is never partly re-analysed. Since ADR-0017
(fusion with UI Automation, `fd47ab0`) the closure's supports are the detector's boxes, the OCR
words **and the accessibility elements**; neither the commit nor ADR-0017 gives a reason for the
last ones.

The first sessions of the live lab on the owner's laptop (2026-11-26; L2b, the plan trace of
`demo --plan-trace`) classified **all 47 full refreshes** of the measured scenes as one cause:
a padded change of 0.80 to 1.07 % of the frame closed to 62.7 to 91.75 % through **one** support
of 49.32, 49.81 or 81.92 % of the frame. By arithmetic on the boxes (the trace does not name a
support's source): at the window's default size the support is a flat-region **surface** of
the window's backdrop (99.2 % of Notepad's 1426×811 window; no accessibility role maps a whole
window); maximised, it is the **UI Automation Document** (1920 × 983 rows). A blinking caret,
a status text or the clock therefore re-reads the window, at 121 to 166 ms instead of a small
region.

Two kinds of box need not be closed over:

- An **accessibility box** is always re-read whole. The provider returns every element whose box
  overlaps the regions it reads (`application/src/ports.rs`), and the merge replaces every held
  element that overlaps them (`engine/observe.rs`). No region can cut one.
- A flat-region **surface** (a box with a side above `glyph_max_side`, which grouping passes
  through unchanged) is reported **whole or not at all** for any area: a segment that continues
  past the analysed area is background and is dropped (`segment.rs`, `continues_outside`). Its
  box is set by its extreme pixels, so while no pixel within `nested_inset + 1` of its edge
  changes, a change strictly inside it cannot move the box, join it to something outside or
  make a duplicate appear (`grouping.rs`: a duplicate lies within `nested_inset` of the edges),
  except by splitting it (L1 below).

## Decision

The planner gets an optional split of the evidence, `Spanning { cut_prone, surfaces,
whole_on_read, band_px }`, passed beside `Evidence` to `plan_regions_with` (the three lists are
`Evidence::supports` as multisets). The rules:

- **R0, gate.** The engine passes the split only when the detector declares a band
  (`VisionDetector::surface_band_px`, default `None`). The flat-region detector declares
  `nested_inset + 1` = 4 and lists its surfaces of the last call (`VisionDetector::surfaces`).
  The ONNX detector and every fake declare none: their plan is today's, bit for bit.
- **R1.** Accessibility boxes are not closed over.
- **R2.** A surface `s` is **spanned** (not closed over) iff (i) it does not touch the frame's
  edge (whether it is background depends on its mean colour then); (ii) no pixel of the band
  `s.pad(b) \ s.shrink(b)`, clipped to the frame, differs from the committed base (exact BGR,
  as the tile diff); (iii) no pending tile meets `s.pad(b)` without lying inside `s.shrink(b)`;
  (iv) `s.shrink(b)` is not empty; and (v) no final region covers the pixels of `s.shrink(b)`.
  Rule (v) was added during implementation: a region that covers the inside of a surface
  without its border could read a box the full read drops as a duplicate (a fill inside its
  border; UT-86 met one on generated screens). The planner closes over every surface (v)
  names and plans again; closing over more only grows the regions, so this ends at a fixed
  point that does not depend on order. A surface that is also a cut-prone box is closed over.
- **R3.** A relation whose control is a box R1 or R2 leaves out (and no cut-prone box) is
  ignored by the closure; its label is a word, closed over as one. Fusion pairs labels again
  over the whole merged evidence on every observation.
- **R4, never worse.** When the support-aware plan is a full refresh, the classic plan (today's
  closure over every support) is computed and runs. A support-aware regional plan's regions
  each lie inside a classic region, so its covered area is never larger, and its full
  refreshes are a subset of today's, with today's reasons.
- **Merge.** A held detector box is replaced when it lies **inside** a re-analysed region
  (before: when it overlapped one). For a classic plan the two are the same; under the
  support-aware closure the only controls a region overlaps without containing are spanned
  surfaces, which are kept. Words and accessibility elements keep the overlap rule.

The plan trace (`demo --plan-trace`, schema `ommatix.plan-trace/2`) says which closure ran
(`chosen`), what the support-aware closure did (surfaces declared and spanned, accessibility
boxes a region cut, relations ignored, the attempt's reason and the classic plan R4 ran) and the
class of each component's and region's largest support. `demo --plan-check` compares each
regional observation with a full read of the same frame, after the commit, as counts. Neither
reaches an MCP client; `_meta`, the schemas and chapter 07 do not change, and no configuration
key is added.

## Alternatives considered

- **Skip a support that strictly contains the region:** misses both maximised cases (the
  clock's and the caret's halos meet the Document only partly) and breaks E for a flat box that
  is a glyph group (a crop inside it reports pieces).
- **Close only over leaves by kind, never over flat surfaces:** misses the Document (a leaf of
  role `Input`) and keeps a surface whose edge changed with a stale box.
- **Let a region grow at most k times:** a tunable; the measured growth is 65 to 115 times, and
  skipping a word or a group still cuts it.
- **Have the detector stop reporting window-sized boxes:** changes what a full read publishes,
  and does nothing for the Document.
- **Per-stage regions** (the detector over its boxes, OCR over words, UIA none): keeps E, but
  carries three region lists through inference, merge and the record; the detector would still
  read the window.
- **Unit A** (the adaptive rule must not count regions for a detector whose cost is per pixel,
  audit A5 §5) and **option C** (perception scoped to the target's window group, an ADR for the
  owner) address the 43 full refreshes of the scenes with Orca's terminals behind the target
  (4 to 6 components each). They are complementary, not alternatives: with perception scoped
  to Notepad, its own caret still closes over Notepad's window-sized support.

## Consequences and cost

- A caret, a status text or a clock tick in front of a window-sized surface or a UI Automation
  Document becomes one small region (predicted, not measured: see validation). On the measured
  scenes O3 alone makes at most 4 of the 47 full refreshes regional; the other 43 need unit A
  or option C.
- The plan costs a band comparison per declared surface (its perimeter times `2b` pixels) and,
  when R4 fires, a second closure.
- **E, what the closure buys, as it stands.** With I1 (closure), I2 (merge) and I3 (every dirty
  tile inside a region), and perception that is local, the merged evidence of a regional
  observation equals a full read's of the same frame, except the age of kept accessibility
  evidence. ADR-0045 keeps that, with three stated limits:
  - **L1.** A change strictly inside a surface that disconnects it (cutting the only link
    between two parts) makes a full read report two boxes; the planner keeps the old box until
    its band changes or a full refresh. A caret, a status text or a clock cannot do this to a
    window's backdrop. Pinned by `ut_86_l1_a_change_that_splits_a_surface_changes_the_full_read`.
  - **L2.** UI Automation enters a child only if the child and every ancestor overlap a region:
    a child sticking out of its parent, met by a small region only outside the parent, is not
    re-read and is dropped by the merge. Before, a closed region that contained it usually met
    the parent too.
  - **L3.** OCR locality is approximate (WinRT reads lines in context); pre-existing on every
    regional plan.
- Chapter 04 §4.2 says that "detecciones que toquen el borde del crop" trigger a widening; the
  engine has no such widening (none found in `engine/observe.rs`). ADR-0045 does not change
  that text; it is recorded here for whoever revises it.

## Required validation

- **Done (simulated and generated):** UT-83 (the closure with spanning: R0 to R4, the band to
  the pixel, the lab's boxes at 1920×1200, a property over generated inputs), UT-84 (the
  merge), UT-85 (regional against full on generated scenes and on the lab's boxes, with fakes
  that cut every box but a surface at a crop's edge), UT-86 (the flat-region contract with the
  real detector, L1 pinned), and the mutations listed in the unit's report.
- **Not validated:** the real screen. The lab's protocol (unit O3 §8): scenes S1 (caret,
  maximised Notepad), S2 (clock), S3 (Orca's terminals behind, two configurations) and S4
  (default size, Orca minimised), with `--timings --plan-trace` and one `--plan-check` run;
  `--plan-check`'s counts at spanned or accessibility boxes must be 0 on every regional
  observation, and an independent UI Automation watcher must see no element lost or cut. The
  ONNX detector is unchanged by R0 and was not run.

## Reversibility

Declare no band in the flat-region detector (one line) and the planner is today's everywhere;
or revert the branch. A real-screen `--plan-check` count above 0 at a spanned or accessibility
box, or an element the watcher finds lost or cut, is the trigger.

## Provenance

Unit O3 of the architect's plan (2026-11-26), from audit A5 (`planes/audits/A5-full-refresh.md`)
and the lab report L2 (`evidencias/2026-11-26/live/L2-REPORT.md`), decision D48. It refines
ADR-0011 (adaptive incrementality) and ADR-0017 (which made accessibility boxes supports), and
amends dossier chapter 04 §4.2. It does not change ADR-0030.

## Resumen (ES)

El cierre del halo dejaba que un cambio pequeño (un caret, el reloj) creciera hasta una caja del
tamaño de la ventana: la superficie plana de su fondo o el documento de UI Automation, y así se
explican los 47 refrescos completos medidos en el laboratorio. Con un detector que declara una
banda, el planificador deja de cerrar sobre las cajas de accesibilidad (se releen enteras) y
sobre las superficies cuyo borde no cambió (el detector las da enteras o no las da), ignora las
relaciones cuyo control es una de ellas, y si el plan resultante es un refresco completo usa el
plan de siempre. Quedan declarados los límites L1 a L3; falta la validación sobre la pantalla
real.
