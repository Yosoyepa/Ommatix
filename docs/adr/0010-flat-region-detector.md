# 0010. Detect elements as flat-colour regions, without assuming a background colour

- **Status:** Accepted (supersedes [0006](0006-deterministic-classical-detector.md))
- **Date:** 2026-10-14

## Context

The first detector (`ContrastBlobDetector`, ADR-0006) took the most frequent colour of a
region as its background. The [MVP review](../reviews/2026-10-14-mvp-review.md) showed
three consequences:

- When one surface covers most of a region, it becomes the "background" and whatever
  surrounds it becomes the foreground. A white window on a contrasting wallpaper turned
  the wallpaper into a frame-sized phantom element, and since every later change touched
  that phantom, every frame was re-analysed in full (no incrementality left).
- An element that fills its region (a solid panel a glyph appears on) vanished.
- Grouping boxes compared every pair until a fixpoint: cubic, 185 s for a 1080p page of
  text.

GUIs are drawn with flat colours, and the regions the engine hands to a detector are built
so that every element they touch is either fully inside or a container fully around them
(ADR-0011). The detector can use both facts.

## Decision

Ship `FlatRegionDetector` (`ommatix-adapter-vision`) and remove `ContrastBlobDetector`.

1. **Segment.** Grow 8-connected regions of pixels whose neighbours differ by at most
   `tolerance` (max RGB channel difference, default 24), with a union-find over the area:
   near-linear in its pixels.
2. **Background.** A segment is background when an edge pixel blends into its neighbour
   just outside the analysed area (it continues beyond the area: backdrop or container),
   or, at the frame's own border where there is no outside, when it looks like the segment
   covering most of that border (wallpaper, maximised window). No colour is special.
3. **Group.** Drop segments under `min_pixels`; drop boxes that only repeat a bigger box
   (all edges within `nested_inset` = 3 px: a border and its fill, the hole of an "o");
   group glyph-sized boxes (≤ 48 px) at most `merge_gap` = 3 px apart into words, unless
   nested. Neighbour searches use the domain's `SpatialIndex`.
4. The port becomes fallible (`detect -> Result<Vec<Detection>, PortError>`) and the
   engine clips every detection to its region, whatever the detector returns.

All elements are still emitted with kind `blob`.

## Consequences

- Light, dark and contrasting themes behave the same; the Gherkin feature
  `appearance.feature` runs the form under each of them.
- A change on a container's surface re-detects only what changed; the container survives.
- Cost: 1920×1080 text page ≈ 0.1 s, 3840×2160 ≈ 0.4 s for a **full** analysis (release
  build, one core); later frames only pay for changed regions.
- Smooth gradients are one segment and therefore background; low-contrast controls
  (within `tolerance` of their surroundings) are invisible, as before.
- Anti-aliased edges may add a 1 px ring to a box; text is still geometry only.

## Alternatives considered

- **Keep the dominant colour, add heuristics** (ignore segments touching the border) —
  fails on the same inputs from another angle (a maximised window, a panel under a
  change); the background question is about continuity, not frequency.
- **Edge detection (Sobel/Canny) + contours** — more tunables, still needs a notion of
  "outside", and gradients produce spurious edges.
- **Learned detector** — still planned as an additional adapter (OCR, widget classes).

## Resumen (ES)

El detector por contraste tomaba el color más frecuente como fondo: una ventana grande
sobre un fondo de escritorio contrastado convertía el fondo en un elemento fantasma del
tamaño de la pantalla y anulaba la incrementalidad; además agrupaba cajas en tiempo
cúbico. Lo reemplaza `FlatRegionDetector`: crece regiones de color plano con union-find,
considera fondo lo que continúa fuera del área analizada (o domina el borde de la pantalla)
y agrupa glifos en palabras con un índice espacial. Funciona igual con tema claro, oscuro o
contrastado, y una página de texto 1080p se analiza en ~0,1 s.
