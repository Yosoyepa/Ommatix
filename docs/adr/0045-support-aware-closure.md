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
  The ONNX detector and every fake declare none: their **plan** is today's, bit for bit. What
  R0 promises is the plan; R5 below changes the **read** of every regional plan, for every
  detector.
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
- **R5, the crop-edge defence (added by unit O3e, decision D59; (b) made stricter by unit
  O3g, decision D65).** For **every** regional
  observation, classic plans included, the detector and the recogniser read each region grown by
  a margin `m` = 32 px, clipped to the frame (the *crop*; overlapping crops are read as their
  union). The accessibility provider reads its regions as before, never grown: it returns every
  element that meets them whole (R1). What the crop returns is judged against the region:
  (a) a word, or a detection that is not a surface, that meets a region and crosses its edge was
  seen whole: the region widens to contain it, and the widening goes to a fixed point that closes
  again over the held words and non-surface detections (and the relations between them) and over
  every fresh one the widened region meets, without reading again; (b) a final region that
  comes within a band `t` = 16 px of a crop edge that is not the frame's edge, or leaves its
  crop, or meets a fresh word or non-surface detection within that band, may hold something the
  crop clipped or dropped: the observation reads once more, with crops grown from the widened
  regions. The first condition (O3g) implies the other two (an item a final region meets lies
  inside it after (a), so at least as far from the crop's edges as the region), which are kept
  as reviewed in O3e and counted on their own; it alone covers a held word or detection (or the
  other member of a held relation) that the widening brought into the band and that the crop
  **dropped** there: absent from the fresh read, it would otherwise have been replaced by
  nothing in the merge. A second (b), or a closure out of rounds, makes the
  observation a full refresh with the existing reason `closure_budget`, whose meaning widens: the
  closure over the evidence did not settle within its bound, **at planning or at the read**. The
  budgets (regions, area, the adaptive rule) are checked again on the final regions, with their
  own reasons. The observation keeps the fresh words and non-surface detections that meet a final
  region (each then lies inside one) and the surfaces a final region contains. The invariant it
  ends with, when the observation stays regional: every final region lies at least `t` inside the
  crop it was read from on every side that is not the frame's edge; every committed fresh word or
  non-surface detection, and every held one a final region meets, lies wholly inside a final
  region, so each was read with at least `t` of context on every such side; and no held word or
  non-surface detection is met and not contained by a final region. `m` and `t` are constants (no
  configuration key): `t` is about one
  glyph at 150 % scale, `m = 2t` so that a word lying on a region's edge is a band's width clear
  of the crop's edge.
- **Merge.** A held detector box is replaced when it lies **inside** a re-analysed region
  (before: when it overlapped one). For a classic plan the two are the same; under the
  support-aware closure the only controls a region overlaps without containing are spanned
  surfaces, which are kept. Words and accessibility elements keep the overlap rule.

The plan trace (`demo --plan-trace`, schema `ommatix.plan-trace/3`) says which closure ran
(`chosen`), what the support-aware closure did (surfaces declared and spanned, accessibility
boxes a region cut, relations ignored, the attempt's reason and the classic plan R4 ran), the
class of each component's and region's largest support, and what R5 did (`crop_edge`); its
`decided_by` is the check that decided the work that ran (the classic plan's under R4, R5's when
the read made a regional plan full). `demo --plan-check` (schema `ommatix.plan-check/3`) compares
each regional observation with a full read of the same frame, after the commit, as counts, and
places each differing word against the final regions and the crops. Neither reaches an MCP
client; `_meta`'s fields and the schemas do not change and no configuration key is added, but
**the meaning of one reported value widens**: `closure_budget` can now be decided at the read
(chapter 04 and the guides say so).

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
- **R5 changes the read for every detector, the ONNX detector included.** R0 still promises the
  plan, bit for bit; what changes for a detector that declares no surfaces is the pixels its
  stages read (each region grown by 32 px a side), and a possible widening, second read or full
  refresh. The ONNX adapter never magnifies a crop: a region of up to 1216 px a side still goes
  through the fixed 1280×1280 input at scale 1 (the inference costs the same; with a model of
  several input shapes, the grown crop may need a larger shape), and a larger one is reduced a
  little more. The detector sees 32 px of context around every crop, and S0 showed that its
  detections depend on that context (S0_RESULTADOS §9.5); S0's measured figures were taken on
  exact crops and are not re-measured here. Two regions whose crops overlap are read as one crop
  (one inference instead of two). Not measured with the ONNX detector.
- **What R5 costs and what it bought, on the lab's S1e pixels** (offline replay of the lab's own
  screenshot of the scene, placed so that the planner's region is the lab's to the pixel,
  12 288 → 20 400 px², with the real Windows OCR; agent-run, exploratory): with no margin, every
  regional observation has 2 other-text pairs (one word with a letter misread, one without its
  first letter) and 2 same-text pairs 2 px apart, as the lab measured (L6c). With a margin of 16,
  32 or 48 px, 0 differing words in 4 of 4 regional observations: a margin of pixels around
  the words is the context the recogniser needed, not the whole line. With the design (32 and
  16), on S1e and S1 (4 regional observations each) neither (a) nor (b) fired, and the pixels
  each of the two stages reads per regional observation grew from 20 400 to 36 400 px² (S1e)
  and from 45 312 to 76 800 px² (S1), about 1.7 to 1.8 times; S1's word beside the caret still
  differs (one other-text pair per observation, as before). The stricter (b) of O3g fired 0
  times on the same frames (4 and 4 regional observations, no region widened), so it added no
  read there. In UT-89's generated scenes it fired on 27 of 662 regional observations of the
  O3f kinds and on 31 of 320 of a recogniser that reads words larger on all four sides and
  drops what comes near a crop's edge (O3h; 20 in O3g, with the reading larger to the right only).
- **What a regional read does to text near a change** (lab, observation only, agent-run,
  exploratory, commit `a014f1c`, before R5): in L6b, 70 regional observations, every differing
  word inside a region (`never_l3` 0): one other-text pair at the caret per observation in S1 and
  S4 (the caret read into the adjacent word), two same-text pairs within 2 px and two other-text
  pairs in S1e. In L6c (accessibility off, so that the words reach the page), S1e's crop reading
  was wrong and the full read right 6 of 6, and in S1 both readings of the number beside the caret
  were wrong, differently, 3 of 3. Which reading is right is established only for those scenes:
  in general it is not.
- **What remains with R5.** A word that the change itself touches (S1's number beside the caret)
  is read from pixels that include the change; a full read reads it in another context, so the
  two can still differ, and both can be wrong. And **a regional reading of text persists until
  the next full refresh**: every later regional observation keeps it (L6c: 19 of 19 and 17 of 17
  observations), and a later plan check whose regions do not meet it counts it outside every
  region (L6c: `never_l3` 8 on a clock tick, before R5).
- **E, what the closure buys, as it stands.** With I1 (closure), I2 (merge) and I3 (every dirty
  tile inside a region), and perception that is local, the merged evidence of a regional
  observation equals a full read's of the same frame, except the age of kept accessibility
  evidence. ADR-0045 keeps that, with four stated limits:
  - **L1.** A change strictly inside a surface that disconnects it (cutting the only link
    between two parts) makes a full read report two boxes; the planner keeps the old box until
    its band changes or a full refresh. A caret, a status text or a clock cannot do this to a
    window's backdrop. Pinned by `ut_86_l1_a_change_that_splits_a_surface_changes_the_full_read`.
  - **L2.** UI Automation enters a child only if the child and every ancestor overlap a region:
    a child sticking out of its parent, met by a small region only outside the parent, is not
    re-read and is dropped by the merge. Before, a closed region that contained it usually met
    the parent too.
  - **L3.** OCR locality is approximate (WinRT reads lines in context); pre-existing on every
    regional plan. R5's margin gives each crop context; it does not make OCR local.
  - **L4.** `t` = 16 px is about one glyph of a 9–10 pt font at 150 % scale. A recogniser that
    drops a cut glyph wider than the band (a higher scale, a larger font) can still return a word
    short of the band, so a cut is made unlikely, not impossible. Pinned as a limit, not solved.
    Likewise, if a recogniser drops a fresh word altogether because it read it joined with a
    neighbour that reaches the band or crop edge (returning nothing, while the final region stays
    at least t clear of the crop's edge), neither (a) nor (b) sees the omission and the merge
    replaces the held word with nothing until the next full refresh.

## Required validation

- **Done (simulated and generated):** UT-83 (the closure with spanning: R0 to R4, the band to
  the pixel, the lab's boxes at 1920×1200, a property over generated inputs), UT-84 (the
  merge), UT-85 (regional against full on generated scenes and on the lab's boxes, with fakes
  that cut every box but a surface at a crop's edge), UT-86 (the flat-region contract with the
  real detector, L1 pinned), UT-89 (R5: a word held tighter than its pixels widens the region
  instead of being cut; O3g: a held word, and a held input pulled in by its relation, that the
  widening brings into the band and the crop drops are read again, not lost; O3h: each of the
  four sides of the band enforced on its own, a side on the frame's edge exempt, and the limit L4
  of a word dropped on a reading past the band pinned as what the engine does; and a property: on
  generated scenes, with fakes that clip at the edge of
  what they are asked to read and an OCR that reads every crop a pixel tight, no committed word or
  detection is ever a clipped one), and the mutations listed in the units' reports (O3, O3e).
- **Not validated:** the real screen. The lab's protocol (unit O3 §8): scenes S1 (caret,
  maximised Notepad), S2 (clock), S3 (Orca's terminals behind, two configurations) and S4
  (default size, Orca minimised), with `--timings --plan-trace` and one `--plan-check` run.
  **The criterion (accepted, decision D58):** on every regional observation, `--plan-check`'s
  `controls` and `accessibility` counts that overlap a spanned surface or an R1 box are 0, and
  `differing_words.never_l3` is 0 (no differing word crosses a region's edge or lies outside
  every region); words that differ inside a region (the limit L3) are reported with their
  pairing and are not a failure. An independent UI Automation watcher must see no element lost
  or cut. With R5, on the lab's rerun: the regional share holds, `never_l3` stays 0, and S1e's
  crop reading of the unchanged words equals the full read's. The ONNX detector's plan is
  unchanged by R0; its read changes under R5, and it was not run.

## Reversibility

Declare no band in the flat-region detector (one line) and the planner is today's everywhere;
or revert the branch. A real-screen `--plan-check` `controls` or `accessibility` count above 0 at
a spanned or accessibility box, a `never_l3` above 0, or an element the watcher finds lost or cut,
is the trigger. R5 alone is reverted by reading the regions ungrown (one constant), which brings
back the cut UT-89 pins.

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
plan de siempre. En toda observación regional, el detector y el OCR leen cada región ampliada en
32 px (R5): lo que cruza el borde de una región la amplía, y una región final que queda a menos
de 16 px de un borde del recorte que no es el del fotograma (o algo leído en esa franja) obliga a
una lectura más, y si vuelve a pasar hay refresco completo (`closure_budget`,
cuyo sentido se amplía a la lectura). Quedan declarados los límites L1 a L4 (L4 incluye la palabra nueva que un reconocedor descarta
por completo al leerla unida a una vecina que llega a la banda: la fusión sustituye la palabra
retenida por nada hasta el siguiente refresco completo); falta la validación
sobre la pantalla real.
