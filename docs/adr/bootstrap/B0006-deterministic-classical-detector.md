# B0006. Deterministic classical detector for the MVP

- **Status:** Superseded by [ADR-0009](../0009-models-and-licences-decided-in-the-spike.md) on 2026-10-14
- **Previous status:** Superseded by [B0010](B0010-flat-region-detector.md)
- **Date:** 2026-10-14

> **Archived.** Written while bootstrapping the repository, before the thesis dossier was
> consolidated into it. It describes the prototype in `src/` that is being realigned with the
> thesis; it is not a current decision. See the [bootstrap index](README.md).

## Context

The MVP must prove the incremental architecture end to end. Learned detectors (OCR, widget
classifiers) bring model files, runtimes and non-determinism across hardware, which would
blur the evaluation of the architecture itself.

## Decision

Ship one `ElementDetector`: **`ContrastBlobDetector`** (`ommatix-adapter-vision`).

1. Background = dominant colour of the region (deterministic tie-break).
2. Foreground = pixels whose max RGB channel distance to the background exceeds a tolerance.
3. 8-connected components → bounding boxes; components below `min_pixels` are noise.
4. Boxes closer than `merge_gap` are merged (glyphs → words) unless one contains the other
   (a button inside a dialog stays a separate element).

All elements are emitted with kind `blob`.

## Consequences

- Fully deterministic and dependency-free; good baseline for the evaluation chapter.
- Known limitations: no text content, no semantic kind, an element that fills its whole
  region is treated as background, gradients/antialiasing may fragment elements.
- Learned or OCR detectors arrive as new adapters behind the same port, possibly chained
  (blob proposals → classifier).

## Alternatives considered

- **OCR first (Tesseract/PaddleOCR)** — valuable but heavy; scheduled after benchmarks.
- **VLM grounding per region** — contradicts the low-latency goal for the core loop; may be
  an optional, explicitly slow adapter later.

## Resumen (ES)

El MVP usa un detector clásico y determinista por contraste: color de fondo dominante,
máscara de primer plano, componentes conexas y fusión de cajas cercanas que no estén
anidadas. No reconoce texto ni tipos de control todavía; esos detectores llegarán como
nuevos adaptadores detrás del mismo puerto.
