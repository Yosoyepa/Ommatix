# 0005. Grid-based incremental perception and uniform-grid spatial index

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Between two consecutive frames of a desktop session, most pixels are unchanged. Re-running
detection on the full frame wastes almost all of the budget. Agents also need fast
point/rectangle queries ("what is under (540, 316)?") with stable element identities.

## Decision

1. **Grid signature.** Split the frame into square cells (default 32 px). Fingerprint each
   cell with 64-bit **FNV-1a** over its RGBA rows: deterministic across runs and platforms,
   no dependency.
2. **Diff.** Dirty cells = cells whose hash changed. First frame or resolution change ⇒ all
   cells dirty and the scene is reset.
3. **Regions.** Merge 4-connected dirty cells into bounding rectangles, then **expand** each
   region to fully include any existing element it touches (merging overlaps to a
   fixpoint), so detectors always see whole elements.
4. **Scene update.** Elements intersecting a region are invalidated; new detections inherit
   the id of an invalidated element of the same kind with IoU ≥ 0.5, otherwise get a new
   monotonically increasing id. Elements outside regions are untouched by construction.
5. **Spatial index.** A **uniform grid hash** (default 64 px buckets, `BTreeMap` for
   deterministic order). Point queries return smallest-first; rect queries return reading
   order.

## Consequences

- Cost of a static screen = one hashing pass; detection cost ∝ changed area.
- Hash collisions can hide a change in one cell for one frame (probability ≈ 2⁻⁶⁴ per
  changed cell): accepted.
- Cell size trades sensitivity vs overhead; exposed as `--cell-size` for experiments.
- Animated content (video, spinners) keeps its cells permanently dirty; a future
  "volatility mask" can down-sample such cells.

## Alternatives considered

- **Pixel-wise diff** — exact regions, but O(pixels) comparisons with the previous frame
  kept in memory; the hash grid needs only `cols × rows × 8` bytes of state.
- **OS damage events (X11 Damage, DXGI dirty rects)** — cheaper when available, but
  platform-specific; can become an alternative `FrameSource` that pre-computes dirtiness.
- **R-tree (`rstar`)** — better for highly varied sizes; GUI elements are small and bounded
  and updates are frequent, where the uniform grid is simpler and O(1) per query bucket.

## Resumen (ES)

La pantalla se divide en celdas (omatidios) con un hash FNV-1a cada una; solo se re-analizan
las celdas cuyo hash cambió, agrupadas en regiones y expandidas a los elementos que tocan.
Los elementos conservan su id si la nueva detección se solapa (IoU ≥ 0,5). Un índice
espacial de cuadrícula uniforme responde consultas por punto o rectángulo de forma
determinista. Una pantalla estática solo cuesta una pasada de hashing.
