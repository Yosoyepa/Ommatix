# 0011. Invariants of the incremental pipeline

- **Status:** Accepted (amends [0005](0005-grid-incremental-perception.md))
- **Date:** 2026-10-14

## Context

ADR-0005 defined the grid, the diff and the scene update. The
[MVP review](../reviews/2026-10-14-mvp-review.md) found inputs where the implementation
broke its own promise that the scene matches the screen:

- Region expansion ran once, so an element touched only by the *grown* region was
  invalidated but re-detected as a fragment.
- A change inside a window re-analysed the whole window.
- Re-identification was greedy in detection order: a focus ring could take a button's id.
- A resize that kept the same number of grid cells left ghost elements off-screen.
- A failing detector after the diff lost the change forever.
- Hashing byte by byte and overflowing `u32` arithmetic on extreme coordinates.

Its [verification pass](../reviews/2026-10-14-mvp-review.md#verification-pass) added:

- Skipping every container *strictly around* a region also skipped a panel whose own edge
  was the change: a panel that shrank kept its old bounds.
- Glyphs typed one by one stayed separate when the next glyph's cell did not touch the
  previous one, though a full analysis groups them into a word.
- Merging regions needed one pass per merge on a staircase of rectangles (1.5 s for
  2,000), and the cell hash let changes in two words cancel out.

## Decision

The engine guarantees the following, each covered by a regression test:

1. **Expansion fixpoint, containers excluded.** Regions absorb every element within the
   detector's context margin (`ElementDetector::context_margin`; touching counts), and
   regions within that margin of each other merge, until nothing changes. An element that
   **encloses** a region, with the region in its interior and sharing none of its edges
   (the window around a changed button), is not absorbed: it cannot have changed as a
   whole. A container whose edge is part of the change (a panel that shrinks) is absorbed
   and re-detected. Afterwards every element within the margin of a region is either inside
   it or encloses it (`Scene::expand_regions`), so incremental analysis groups glyphs
   exactly like a full analysis.
2. **Global identity matching.** Invalidated elements and new detections of the same kind
   are paired best IoU first (ties: older id, then detection order); IoU ≥ 0.5 keeps the
   id. Ids are never reused, not even after the scene is cleared.
3. **Geometry change = new scene.** The first frame or any change of frame size (not of
   grid size) clears the scene; every previous element is reported as `removed`.
4. **Atomic ticks.** `ChangeTracker::compare` does not remember the frame; the engine
   detects first and only then commits the signature and updates the scene. A failing
   detector leaves everything unchanged and the same change is processed next tick.
5. **Bounded work.** Detections are clipped to their region. Merging regions is a single
   sweep (`merge_nearby`): each rectangle absorbs the neighbours the spatial index finds in
   reach until none is left, so a staircase of 20,000 rectangles merges in one pass. A
   rectangle query never costs more than the number of occupied buckets, whatever its
   size.
6. **Total arithmetic.** `Rect` edges saturate at `u32::MAX`; a rectangle covering no pixel
   is empty for every method; frame sizes whose buffer would not fit in memory are an
   error.
7. **Cell hashing.** Cells are hashed two pixels (8 bytes) at a time, refining ADR-0005's
   FNV-1a. Each step `h = (h ^ word) · K; h ^= h >> 32` is a bijection of the state for a
   fixed word, so a change confined to two neighbouring pixels always changes the hash;
   the shift carries high bits down, so changes spread over several words collide only by
   chance (none in 2²⁰ random trials, where plain multiply-xor missed one in 2¹⁶). Alpha
   is not hashed: a screen is opaque and the detector never reads it. A collision would
   hide a change in that cell until the cell changes again.

Reports expose `analysed_regions` and `analysed_px`, the detector's actual workload.

## Consequences

- Elements keep their ids across unrelated changes, including inside large containers.
- Determinism holds by construction (ordered maps, explicit sorting) and is tested
  end to end: the same MCP conversation produces byte-identical answers.
- A container is re-detected only when a change touches its edges.
- Hashing costs ≈ 1.9 ms per 1080p frame and ≈ 7.7 ms per 4K frame (release build), up
  from 1.3 and 6.8 ms with the weaker mixing.

## Alternatives considered

- **Always re-detect containers** — simpler, but a keystroke in a full-screen editor would
  re-analyse the full screen.
- **Hungarian assignment for identities** — optimal globally, O(n³); greedy best-first on
  sorted IoU is deterministic and adequate for GUI elements.

## Resumen (ES)

El motor garantiza: expansión de regiones hasta un punto fijo, que absorbe los elementos a
menos del margen de contexto del detector (así las letras tecleadas una a una se agrupan
igual que en un análisis completo) sin absorber contenedores que encierran el cambio sin
compartir un borde (un cambio dentro de una ventana no re-analiza la ventana, pero un
panel que se encoge sí se re-detecta); emparejamiento global de identidades por mejor IoU
sin reutilizar ids; escena nueva ante cualquier cambio de tamaño de pantalla; ticks
atómicos (si el detector falla no se pierde el cambio); fusión de regiones en una sola
pasada; aritmética que no desborda; y un hash de celdas que procesa dos píxeles a la vez,
detecta siempre un cambio dentro de una misma palabra de 8 bytes e ignora el canal alfa.
Cada garantía tiene su prueba de regresión.
