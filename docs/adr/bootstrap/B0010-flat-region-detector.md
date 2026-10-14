# B0010. Detect elements as flat-colour regions, without assuming a background colour

- **Status:** Superseded by [ADR-0009](../0009-models-and-licences-decided-in-the-spike.md) on 2026-10-14
- **Previous status:** Accepted (supersedes [B0006](B0006-deterministic-classical-detector.md))
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

The first detector (`ContrastBlobDetector`, B0006) took the most frequent colour of a
region as its background. The [MVP review](../../reviews/2026-10-14-mvp-review.md) showed
three consequences:

- When one surface covers most of a region, it becomes the "background" and whatever
  surrounds it becomes the foreground. A white window on a contrasting wallpaper turned
  the wallpaper into a frame-sized phantom element, and since every later change touched
  that phantom, every frame was re-analysed in full (no incrementality left).
- An element that fills its region (a solid panel a glyph appears on) vanished.
- Grouping boxes compared every pair until a fixpoint: cubic, 185 s for a 1080p page of
  text.

A [verification pass](../../reviews/2026-10-14-mvp-review.md#verification-pass) of the first
version of this decision then showed that judging the screen edge from the analysed
area's slice of it made incremental and full analysis disagree, and that a screen packed
with tiny segments still took 15 s to group.

GUIs are drawn with flat colours, and the regions the engine hands to a detector are built
so that every element they touch is either fully inside or a container fully around them
(B0011). The detector can use both facts.

## Decision

Ship `FlatRegionDetector` (`ommatix-adapter-vision`) and remove `ContrastBlobDetector`.

1. **Segment.** Grow 8-connected regions of pixels whose neighbours differ by at most
   `tolerance` (max RGB channel difference, default 24), with a union-find over the area:
   near-linear in its pixels.
2. **Background.** No colour is special. A segment is background when:
   - it continues beyond the analysed area: one of its edge pixels blends into any of its
     8 neighbours just outside the area (a backdrop or a container); or
   - it lies on a **surface** of the screen's border, a flat stretch covering at least half
     of a screen edge (the wallpaper, a maximised or snapped window, a taskbar), or touches
     the screen edge with the colour of such a surface.

   Surfaces are measured on the whole frame for every call, so whether a segment is
   background never depends on which part of the frame is being analysed.
3. **Group.** Drop segments under `min_pixels`; drop boxes that only repeat a bigger box
   (all edges within `nested_inset` = 3 px: a border and its fill, the hole of an "o");
   group glyph-sized boxes (≤ 48 px) at most `merge_gap` = 3 px apart into words, unless
   nested. Each pass finds neighbours through a grid built for it (4 px cells, coarser
   when the boxes are sparse), so a screen packed with tiny segments stays near-linear.
4. **Context margin.** Since glyphs up to `merge_gap` apart form one word, the detector
   declares that margin (`ElementDetector::context_margin`) and the engine hands it every
   element within that distance of a change (B0011): typing a word glyph by glyph gives
   the same element as analysing the finished word.
5. The port becomes fallible (`detect -> Result<Vec<Detection>, PortError>`) and the
   engine clips every detection to its region, whatever the detector returns.

All elements are still emitted with kind `blob`.

## Consequences

- Light, dark and contrasting themes behave the same; the Gherkin feature
  `appearance.feature` runs the form under each of them.
- A change on a container's surface re-detects only what changed; the container survives.
- Cost of a **full** analysis (release build, one core): 1920×1080 text page ≈ 0.08 s,
  3840×2160 ≈ 0.3 s, a 1080p screen packed with 2×2 dots ≈ 0.15 s (it took 15 s before
  the per-pass grid). Later frames only pay for changed regions.
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
cúbico. Lo reemplaza `FlatRegionDetector`: crece regiones de color plano con union-find y
considera fondo lo que continúa fuera del área analizada (mirando los 8 vecinos) o forma
parte de una superficie del borde de la pantalla, un tramo plano que cubre al menos la
mitad de un borde (fondo de escritorio, ventana maximizada o acoplada, barra de tareas),
medido siempre sobre el frame completo para que el análisis incremental y el completo
coincidan. Agrupa glifos en palabras con una cuadrícula propia de cada pasada y declara su
margen de contexto (la distancia entre glifos) para que el motor le entregue las letras
vecinas de un cambio. Funciona igual con tema claro, oscuro o contrastado; una página de
texto 1080p se analiza en ~0,08 s y una pantalla llena de puntos diminutos en ~0,15 s.
