# Architecture overview

🇪🇸 [Versión en español](overview.es.md)

This page summarises the design of the thesis and maps it onto the repository. The normative
text is in Spanish in [`docs/thesis/`](../thesis/README.md): the SDD (chapter 02), the
algorithms (chapter 04) and the MCP contract (chapter 07). Where this page and those chapters
differ, the chapters win.

## Problem

Computer-use agents usually send a screenshot to a vision-language model at every step and
act on the coordinates it guesses. That is slow and costly, and it is unsafe: the screen can
change between observation and action, and nothing checks that the click still lands on what
the agent chose.

## Approach

Ommatix separates perception from reasoning.

- **Perception (Ommatix, local, Rust):** turns screen frames into GUI elements with stable
  session identifiers, re-analyses only what changed, and publishes immutable snapshots.
- **Reasoning (the agent, an MCP client):** chooses an element by its identifier and
  revision. It never sends coordinates.

Before acting, Ommatix checks that the agent's observation is still current and that a local
policy allows the action. It then injects the input and reports delivery and verification as
separate facts. Screen content is untrusted data: text on screen never changes permissions.

## Trust boundaries

A local operator authorises one agent to use one engine instance, bound to one user session
and one allowed monitor. A local policy outside the agent's control decides which
applications and actions are enabled, and the operator can stop the engine at any time. The
MVP opens no network port; MCP runs over stdio. See [context](../diagrams/context.md) and
[thesis chapter 08](../thesis/08_SEGURIDAD_Y_PRIVACIDAD.md).

## Components

The SDD defines eleven components ([diagram](../diagrams/components.md)).
[ADR-0014](../adr/0014-hexagonal-crate-layout.md) places them in crates:

| Component | Responsibility | Contract | Crate |
|---|---|---|---|
| MCP Adapter | Validate and route calls; describe the tools | Versioned tool inputs and outputs | `ommatix-adapter-mcp` |
| Observation Coordinator | Capture, region planning and commit | SnapshotRequest → Snapshot | `ommatix-application` |
| Capture Provider | Pixels, stride, timing, dirty and move rectangles | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR and image transforms | Region → RawEvidence | `ommatix-adapter-vision` |
| Spatial Fusion | Containment, lines, labels and relations | RawEvidence → Candidates | `ommatix-domain` |
| Icon Resolver | Contextual lookup with rejection | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisions and retirement | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Freshness, permissions and preconditions | ActionRequest → Permit or Reject | `ommatix-application` |
| OS Actuator | Event injection and delivery count | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Check an expected effect, or report uncertainty | Before and after → Verification | `ommatix-application` |
| Telemetry | Metrics and audit without sensitive text | Events and spans | Port in `ommatix-application`, plus an adapter |

The core reaches the outside world only through the ports `FrameCapturer`, `VisionDetector`,
`TextRecognizer`, `OsActuator`, `Clock` and `PolicyStore`, so every adapter can be replaced by
a deterministic fixture. On Windows 11 ([ADR-0008](../adr/0008-one-real-platform-first.md)),
capture uses DXGI Desktop Duplication and input uses `SendInput`. Inference runs on ONNX
Runtime, with the CPU as the baseline and DirectML as an option; the models are chosen in the
S0 spike ([ADR-0009](../adr/0009-models-and-licences-decided-in-the-spike.md)).

## Data model

- **Frame:** physical size, row stride in bytes, BGRA8, monotonic capture time, monitor,
  rotation, geometry epoch and where its change regions came from. Sizes are checked with
  overflow-safe arithmetic, and `row_stride == width * 4` is never assumed.
- **RawEvidence:** a box, the kind of evidence, optional text, the provider's score, the
  transform from tensor to crop, the model and its revision, and the source frame.
- **Element:** an opaque `target_id`, a monotonic revision, a role or `unknown`, a label with
  its provenance, inner content with `text_kind`, visibility, enablement and
  `actionability`. Geometry stays inside the engine.
- **Snapshot:** `session_id`, `snapshot_id`, `geometry_epoch`, capture and publication
  times, elements, relations, degradation and metrics, in a stable order.

## One observation

1. **Capture** a frame and the native dirty and move rectangles.
2. **Plan regions (Algorithm 1).** Compare 64×64 tiles exactly, pixel by pixel, against the
   last *committed* base, and add the dirty tiles still pending. Group dirty tiles into
   8-connected components, pad each with a 32 px halo, and close it over the evidence it
   touches until nothing changes. A geometry change, a closure that runs out of budget, or a
   plan above the initial thresholds (40 % of the area, or more than four regions) falls back
   to a full refresh.
3. **Perceive** the planned regions with the detector and the OCR.
4. **Fuse (Algorithm 2).** A text belongs to the most specific control that contains it,
   measured by IoC = area(text ∩ control) / area(text) ≥ 0.75; ties abstain. Texts are grouped
   into lines before the reading order is sorted. External labels are matched by an
   anisotropic cost inside a corridor between facing edges, with abstention when the margin
   is small. Text without an owner becomes a `text_node` of unknown actionability.
5. **Resolve icons (Algorithm 3A).** Icons without text are looked up in a contextual cache
   with a masked 128-bit gradient descriptor; a match needs enough support and a margin over
   the runner-up. Unknown stays unknown.
6. **Track identity (Algorithm 3B).** One-to-one matching keeps each element's session ID,
   which is monotonic and never reused; the revision grows when something an action depends
   on changes; retired IDs leave tombstones. Ambiguity creates a new track instead of
   guessing.
7. **Commit.** Registry, index and base are published together as an immutable snapshot. If
   any step fails, nothing is published, pending dirty regions are kept and the engine is
   degraded, which blocks actions ([snapshot lifecycle](../diagrams/snapshot-state.md)). A
   change of monitor, DPI or model configuration bumps the geometry epoch and forces a rebuild.

## One action

[Sequence diagram](../diagrams/action-sequence.md). `execute_action` names the session, the
snapshot, the geometry epoch, the target and its expected revision. The engine validates the
schema, deduplicates the `request_id`, applies the local policy, checks that snapshot,
revision and epoch are current, confirms window, focus and freshness, records its intent,
injects the input, checks delivery and looks for the expected effect.

The answer is one of `simulated`, `rejected`, `input_delivered`, `effect_observed` or
`indeterminate`, and none claims more than it shows: delivered input does not prove that the
interface obeyed, and an uncertain effect is never retried automatically. `dry_run` is true
unless the agent sets it to false, and even then the local consent and allowlist apply
([ADR-0007](../adr/0007-fail-closed-actuation-and-honest-simulation.md)).

## Agent contract

[Thesis chapter 07](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md) and the JSON schemas in
[`docs/thesis/contratos/`](../thesis/contratos/README.md) define it:

- **Profiles:** MCP 2026-07-28 through the `rmcp` SDK (`server/discover`, no `initialize`,
  version and capabilities in `_meta`); 2025-11-25 is opt-in
  ([ADR-0002](../adr/0002-mcp-stdio-with-explicit-profiles.md)).
- **Tools:** `observe_screen` (query, paging, no coordinates), `execute_action` (`click`,
  `type`, `scroll`) and `get_engine_metrics`.
- **Errors** carry a code (`STALE_SNAPSHOT`, `STALE_TARGET`, `AMBIGUOUS_TARGET`,
  `POLICY_DENIED`, `REQUEST_CONFLICT` and others) and a `next_step` such as `observe_again` or
  `operator_review`. Messages are limited to 64 KiB.

## Quality attributes

| Attribute | How it is addressed |
|---|---|
| Correctness | Each algorithm has invariants and planned tests (UT-01..43); a Python oracle (48 tests) is the differential reference |
| Safety | Fail-closed gate, strict freshness, separate delivery and verification statuses, local stop |
| Determinism | Ordered collections, explicit sorting, injected clock; run-dependent values stay out of the structured result |
| Privacy | No screenshots or values in logs by default; screen data never raises permissions ([ADR-0012](../adr/0012-data-minimisation-and-evidence-traceability.md)) |
| Latency | Budgets per stage, measured on reference hardware before they become commitments ([ADR-0010](../adr/0010-performance-budgets-subject-to-measurement.md)) |
| Modifiability | Ports and adapters, with boundaries enforced by crate dependencies |

## Where the code is today

The code in `src/` was written while the repository was bootstrapped, before the thesis was
consolidated. It already follows the crate layout, the injected clock, stdout reserved for
MCP and dry-run input by default, but not the thesis pipeline:

| Area | Thesis | Prototype in `src/` | Replaced in |
|---|---|---|---|
| Change detection | Exact 64×64 tiles against the committed base, halo closure with budget | Hashed 32 px cells against the previous frame, context-margin expansion | S1 |
| Perception | Detector and OCR on ONNX Runtime, roles from the S0 spike | Flat-colour region detector, no text, every element a `blob` | S0, S3 |
| State | Immutable snapshots, staging, atomic commit, geometry epoch | One mutable scene | S1 |
| Identity | Session IDs, revisions, tombstones, one-to-one matching with abstention | Monotonic IDs matched by IoU | S2 |
| Agent contract | `observe_screen`, `execute_action`, `get_engine_metrics`; no coordinates | Seven tools that expose coordinates; MCP 2025-06-18 | S2 |
| Platform | Windows 11 x64: DXGI and `SendInput` | `xcap` and `enigo` on macOS, Linux X11 and Windows at 100 % | S3 |

The prototype's Gherkin features in `src/acceptance/features/` and its archived decisions in
[`docs/adr/bootstrap/`](../adr/bootstrap/README.md) describe it until it is replaced. The
thesis scenarios (BDD-01..32, in [`docs/thesis/features/`](../thesis/features)) move into the
acceptance crate as each stage makes them executable.
