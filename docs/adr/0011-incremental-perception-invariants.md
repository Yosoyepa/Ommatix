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

## Decision

The engine guarantees the following, each covered by a regression test:

1. **Expansion fixpoint, containers excluded.** Regions absorb every element they
   intersect, and overlapping regions merge, until nothing changes. An element that
   *strictly contains* a region (the window around a changed button) is not absorbed: it
   cannot have changed as a whole. Afterwards every element intersecting a region is
   either inside it or strictly contains it (`Scene::expand_regions`).
2. **Global identity matching.** Invalidated elements and new detections of the same kind
   are paired best IoU first (ties: older id, then detection order); IoU ≥ 0.5 keeps the
   id. Ids are never reused, not even after the scene is cleared.
3. **Geometry change = new scene.** The first frame or any change of frame size (not of
   grid size) clears the scene; every previous element is reported as `removed`.
4. **Atomic ticks.** `ChangeTracker::compare` does not remember the frame; the engine
   detects first and only then commits the signature and updates the scene. A failing
   detector leaves everything unchanged and the same change is processed next tick.
5. **Bounded work.** Detections are clipped to their region. Region merging and spatial
   queries are near-linear (spatial hash + union-find); a rectangle query never costs more
   than the number of occupied buckets, whatever its size.
6. **Total arithmetic.** `Rect` edges saturate at `u32::MAX`; a rectangle covering no pixel
   is empty for every method. Cells are hashed 8 bytes at a time (FNV-style mixing,
   deterministic), not byte by byte.

Reports expose `analysed_regions` and `analysed_px`, the detector's actual workload.

## Consequences

- Elements keep their ids across unrelated changes, including inside large containers.
- Determinism holds by construction (ordered maps, explicit sorting) and is tested
  end to end: the same MCP conversation produces byte-identical answers.
- A container is re-detected only when a change touches its edges.

## Alternatives considered

- **Always re-detect containers** — simpler, but a keystroke in a full-screen editor would
  re-analyse the full screen.
- **Hungarian assignment for identities** — optimal globally, O(n³); greedy best-first on
  sorted IoU is deterministic and adequate for GUI elements.

## Resumen (ES)

El motor garantiza: expansión de regiones hasta un punto fijo sin absorber contenedores
(un cambio dentro de una ventana no re-analiza la ventana), emparejamiento global de
identidades por mejor IoU sin reutilizar ids, escena nueva ante cualquier cambio de tamaño
de pantalla (los elementos anteriores se reportan como eliminados), ticks atómicos (si el
detector falla no se pierde el cambio) y aritmética que no desborda. Cada garantía tiene
su prueba de regresión.
