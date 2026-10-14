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
| Observation Coordinator | Capture, region planning and commit | SnapshotRequest → Snapshot | `ommatix-application` (`Engine::observe_screen`) |
| Capture Provider | Pixels, stride, timing, dirty and move rectangles | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR and image transforms | Region → RawEvidence | `ommatix-adapter-vision` |
| Spatial Fusion | Containment, lines, labels and relations | RawEvidence → Candidates | `ommatix-domain` |
| Icon Resolver | Contextual lookup with rejection | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisions and retirement | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Freshness, permissions and preconditions | ActionRequest → Permit or Reject | `ommatix-application` (`Engine::execute_action`) |
| OS Actuator | Event injection and delivery count | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Check an expected effect, or report uncertainty | Before and after → Verification | `ommatix-application`: re-capture after delivery (partial; S3) |
| Telemetry | Metrics and audit without sensitive text | Events and spans | `get_engine_metrics` and stage timings in MCP `_meta` (partial; S3) |

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

- **Profiles:** MCP 2026-07-28 (`server/discover`, no `initialize`, version and capabilities
  in `_meta`); 2025-11-25 is opt-in
  ([ADR-0002](../adr/0002-mcp-stdio-with-explicit-profiles.md)). A connection locks onto
  the profile of its first selecting request and never mixes the two rule sets.
- **Transport:** `rmcp` 3.5.1 wire types with the adapter's own bounded message loop, because
  `rmcp`'s service loop has no size limit, closes the connection on early notifications, lets
  profiles mix and drops unparsable input silently
  ([ADR-0015](../adr/0015-mcp-transport-on-rmcp-wire-types.md)).
- **Tools:** `observe_screen` (query, paging, no coordinates), `execute_action` (`click`,
  `type`, `scroll`) and `get_engine_metrics`, described with the dossier's schemas.
- **Errors** carry a code (`STALE_SNAPSHOT`, `STALE_TARGET`, `AMBIGUOUS_TARGET`,
  `POLICY_DENIED`, `REQUEST_CONFLICT` and others), `retryable` and a `next_step` such as
  `observe_again` or `operator_review`; they are tool results with `isError: true`.
  Malformed messages, unknown methods or tools and schema-invalid arguments (`-32602`) are
  JSON-RPC errors. Messages are limited to 64 KiB in both directions. Stage timings go in
  the result's `_meta` (`io.ommatix/timings`), never in `structuredContent`.

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

Since S2b the running binary is the thesis engine. `serve` and `demo` build one `Engine`
(`ommatix-application`) over the SDD ports and, for `serve`, the MCP adapter in front of it.
The bootstrap prototype's use cases, its ports (`FrameSource`, `ElementDetector`,
`InputController`), its seven coordinate-based MCP tools and its acceptance suite (the
`src/acceptance/features/` directory and `tests/acceptance.rs`) are retired. Of its domain
modules, `ommatix-domain` keeps only the support types the interim adapters still use (the
RGBA `Frame`, `Rect`/`Point`, `Detection`, the simulated user's `Action`, `DisplayMapping`).
Its archived decisions stay in [`docs/adr/bootstrap/`](../adr/bootstrap/README.md). The
[S2b review](../reviews/2026-10-14-s2b-engine-and-mcp.md) records the findings fixed before
this stage closed, each with its regression test.

**`observe_screen`** captures a frame, plans regions against the last committed base
(Algorithm 1), hands only the planned regions to the detector and the recogniser, keeps the
evidence outside them, fuses (Algorithm 2), resolves icons (Algorithm 3A), tracks identities
(Algorithm 3B) and stages and commits the snapshot. A failed stage publishes nothing, keeps
the planned tiles pending and blocks actions. Pages are filtered by `query`, redacted unless
the local policy allows values, and bound to their snapshot by the cursor.

**Icons (Algorithm 3A).** A control is an icon candidate when it is an `icon_button`, or when
it has no inner text and its role takes no external label. Its box is cropped from the frame
(no inner margin), converted to luma with the BT.601 weights (0.299 R + 0.587 G + 0.114 B),
resampled to the 9×8 grid by integer block means and described with the masked gradient
descriptor; the curated cache answers a label for the configured context only when the
match has enough support, is close enough and beats the next different label by the margin.
A match is labelled `curated_icon`; anything else keeps an empty label, never a guess. The
label never changes actionability, which comes from the detector. Only candidates inside the
planned regions are looked up again; the others keep the label of the committed snapshot.
The resolver is off unless the operator configures prototypes and thresholds (`[cache]`),
because the thesis calibrates them (H4); the context is configured too until S3 can identify
the foreground application.

**`execute_action`** checks the arguments, looks the `request_id` up in a bounded
deduplication journal (FNV-1a hash of the canonical arguments, so typed text is never stored;
same ID with other arguments is `REQUEST_CONFLICT`), then checks session, geometry epoch,
snapshot, target, revision and actionability strictly against the published snapshot. A dry
run (the default) is `simulated`. Live input needs the operator's policy
(`actuation_enabled`, not stopped), a live actuator **and** a foreground application on the
non-empty `allowed_applications` list, as the actuator attests it; otherwise `POLICY_DENIED`
with nothing delivered. No actuator can attest the foreground application before S3, so
today live input is always denied. Before delivering, a freshness check captures the screen
again: if it differs from the committed base, the action is `STALE_SNAPSHOT` and nothing is
delivered. A delivery receipt whose inserted events match the submitted ones is
`input_delivered`; anything else is `indeterminate`. An expected `visual_change` is verified
by a capture after delivery compared with the pre-delivery frame (`effect_observed` or
`indeterminate`); the other effects are reported `indeterminate` until S3 can check them.
Any delivery, whatever its outcome, spends the snapshot: the next action needs a new
observation (`STALE_SNAPSHOT`), while journal replays still return the stored result.

| Area | Thesis | Today | Completed in |
|---|---|---|---|
| Change detection, fusion, identity, snapshots | Algorithms 1, 2, 3B; transactional commit | Implemented in `ommatix-domain` and run by the `Engine` | S1–S2 (done) |
| Icon cache | Algorithm 3A in the observation | Run by the `Engine` on icon candidates of the planned regions with the operator's curated prototypes; off without them; context configured, thresholds not calibrated | S3 (context), S4 (calibration) |
| Perception | Detector and OCR on ONNX Runtime, roles from the S0 spike | Flat-colour region detector (roles `unknown`), no OCR (`NullTextRecognizer`); the simulated desktops report ground truth | S0, S3 |
| Agent contract | Ommatix 1.0 over MCP 2026-07-28; 2025-11-25 opt-in | Implemented (CT-01..10); not yet tested against a real client | S0/S3 for the client |
| Action gate | Freshness, policy, allowlist, focus, stop | Snapshot freshness (also re-checked by a capture before delivery), policy, stop and the allowlist, which denies live input until an actuator attests the foreground application; no focus check | S3 |
| Effect verification | Focus, value, scroll and visual postconditions | Visual change by re-capture only | S3 |
| Telemetry | Metrics and audit without sensitive text | `get_engine_metrics`; stage timings in `_meta` | S3 |
| Platform | Windows 11 x64: DXGI and `SendInput` | Simulated and fixture desktops; `xcap` screen capture and `enigo` input behind build features | S3 |

The thesis scenarios (BDD-01..32) stay in [`docs/thesis/features/`](../thesis/features),
where the dossier's traceability matrix expects them; `src/acceptance/tests/thesis.rs` runs
BDD-01..27: BDD-01..10 and BDD-13..21 on the domain core, BDD-11, 12 and 22..27 through the
engine and the MCP adapter over the simulated and fixture desktops. BDD-28..32 wait for S3.
