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

The SDD defines twelve components ([diagram](../diagrams/components.md)).
[ADR-0014](../adr/0014-hexagonal-crate-layout.md) places them in crates:

| Component | Responsibility | Contract | Crate |
|---|---|---|---|
| MCP Adapter | Validate and route calls; describe the tools | Versioned tool inputs and outputs | `ommatix-adapter-mcp` |
| Observation Coordinator | Capture, region planning and commit | SnapshotRequest → Snapshot | `ommatix-application` (`Engine::observe_screen`) |
| Capture Provider | Pixels, stride, timing, dirty and move rectangles | FrameLease | `ommatix-adapter-capture` |
| Perception Provider | Detector, OCR and image transforms | Region → RawEvidence | `ommatix-adapter-vision` |
| Accessibility Provider | The foreground window's UI Automation tree; optional evidence fused with the visual path | Region → AccessibleElement | `ommatix-adapter-accessibility` |
| Spatial Fusion | Containment, lines, labels and relations; absorbs detections that match an accessible element | RawEvidence + AccessibleElement → Candidates | `ommatix-domain` |
| Icon Resolver | Contextual lookup with rejection | CropDescriptor → LabelEvidence | `ommatix-domain` |
| Temporal Registry | IDs, revisions and retirement | Candidates + Snapshot → Tracks | `ommatix-domain` |
| Policy & Action Gate | Freshness, permissions and preconditions | ActionRequest → Permit or Reject | `ommatix-application` (`Engine::execute_action`) |
| OS Actuator | Event injection and delivery count | ValidatedAction → DeliveryReceipt | `ommatix-adapter-input` |
| Effect Verifier | Check an expected effect, or report uncertainty | Before and after → Verification | `ommatix-application`: re-capture after delivery (partial; S3) |
| Telemetry | Metrics and audit without sensitive text | Events and spans | `get_engine_metrics` and stage timings in MCP `_meta` (partial; S3) |
| Evidence tooling | Resources and 2 h soak (PERF-03), DPI 100/125/150 % (IT-07), install and rollback (REL-01) | Soak run once (PERF-03: [S0 §12](../thesis/S0_RESULTADOS.md), without accessibility and on an earlier commit): its working-set and handle thresholds of 1.0.62 pass there, the queue has no bound yet, and the repeat on current code is scheduled. DPI and install written, not run; the read-only `tools/perf/soak.py` and `tools/perf/dpi_probe.py` (no input, no `execute_action`) and the [install and rollback guide](../thesis/S4_INSTALACION_Y_REVERSION.md); what each supervised session needs is in [`S4_SESIONES_PENDIENTES.md`](../thesis/S4_SESIONES_PENDIENTES.md) | S4 (sessions with the owner) |

The core reaches the outside world only through the ports `FrameCapturer`, `VisionDetector`,
`TextRecognizer`, `AccessibilityProvider`, `OsActuator`, `Clock` and `PolicyStore`, so every adapter can be replaced by
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
3. **Perceive** the planned regions with the detector and the OCR, and read the
   accessibility tree inside them. The three stages read the same frame and do not
   depend on each other, so they run concurrently on scoped threads and are joined before
   anything is fused ([ADR-0019](../adr/0019-concurrent-observation-stages.md)); the
   coordinator stays the only writer and collects the results by stage, not by arrival.
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
   guessing. An accessibility element's UI Automation runtime ID, scoped by its process and
   window, is part of its identity; when continuity is not proven the element gets a new ID,
   and the reason of each retirement is recorded as a code (ADR-0026).
7. **Commit.** Registry, index and base are published together as an immutable snapshot. If
   any step fails, nothing is published, pending dirty regions are kept and the engine is
   degraded, which blocks actions ([snapshot lifecycle](../diagrams/snapshot-state.md)). A
   change of monitor, DPI or model configuration bumps the geometry epoch and forces a rebuild.

## One action

[Sequence diagram](../diagrams/action-sequence.md). `execute_action` names the session, the
snapshot, the geometry epoch, the target and its expected revision. The engine validates the
schema, deduplicates the `request_id`, applies the local policy, checks that snapshot,
revision and epoch are current, confirms window, focus, freshness and age (ADR-0025) right before each real input, records its intent,
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
  JSON-RPC errors. Messages are limited to 64 KiB in both directions. Run-dependent values
  go in the result's `_meta`, never in `structuredContent`: an observation's stage timings
  (`io.ommatix/timings`) and whether its accessibility evidence is complete
  (`io.ommatix/accessibility`), and an action's stage timings (`io.ommatix/action_timings`).

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

**Windows adapters (S3, first part).** Every one is behind a build feature that is off by
default, and the defaults stay fail-closed: no model, no OCR, dry-run input, deny-all policy.

- *Capture* (`dxgi`, `--source dxgi --output N`): DXGI Desktop Duplication of one output in
  physical pixels (the process is per-monitor-v2 DPI aware). The desktop texture is copied
  to a staging texture and read row by row with its own stride; frames stay in memory. A
  rotated output is refused; a change of size, DPI or origin bumps the geometry epoch; when
  no desktop update arrives the last frame is returned again; the first capture after
  opening (or recreating) the duplication waits up to 500 ms for an update that carries a
  presented desktop image (`LastPresentTime != 0`) and fails rather than return a blank
  frame; a lost duplication
  (`DXGI_ERROR_ACCESS_LOST`, device removed or reset: a mode change, the secure desktop) is
  recreated once within the capture, a second loss is a capture error, and the next frame
  starts a new geometry epoch. Dirty and move rectangles are only counted; the engine's own
  differ decides what changed. The capturer shares its live display mapping with the input
  adapter, which refuses to aim while it is lost or out of profile.
- *Input* (`sendinput`, `--input sendinput`, only with `--source dxgi`): Win32 `SendInput`
  aimed through the capturer's mapping, one call per action, with a receipt of submitted and
  inserted events. It attests the foreground application (the lowercase executable name of
  the foreground window's process) to the engine's allowlist check, and before sending it
  refuses a different foreground process, a target point outside that process's windows, a
  higher integrity level (UIPI would drop the input silently) and held modifiers or mouse
  buttons. `type` sends its focusing click first and its keys only after
  `GetGUIThreadInfo` shows keyboard focus, in the same foreground process, on the window at
  the target point or a window containing it (polled for up to 200 ms while the click is
  handled); otherwise no key is sent. With accessibility evidence the engine owns that
  pause instead (ADR-0018): `deliver_focus_click` sends the click, the engine polls the
  provider's `keyboard_focus` on its `Clock` until the target element has focus, and only
  then calls `deliver_keys`, which still checks the foreground application and held
  modifiers. A refusal is an actuator error, which the engine reports `indeterminate` and
  never retries: after a refused `type` the click may have been delivered. No test sends
  real input.
- *Detector* (`onnx`, `[models] detector_manifest`): a generic YOLO-style detector on ONNX
  Runtime loaded at run time (`load-dynamic`) from a library pinned by SHA-256. The model
  manifest is pinned by SHA-256 and names the weights, their SHA-256 and each class's role
  and affordance; class names never become roles. Each planned region is letterboxed into
  the input, decoded, suppressed and mapped back to the frame, one inference per region.
  An inference costs about as much as its input has pixels, and smaller inputs for small
  regions change the detections (S0 §10.2), so every region goes through the full input. The
  DirectML manifest of RNF-05 points at a float16 export of the network with a float32
  decoding (S0 §10.3).
  It runs on the CPU (`detector_threads` intra-op threads) or,
  with the `directml` feature, on DirectML with a pinned `DirectML.dll`, sequential
  execution, no memory pattern and no CPU fallback; the manifest lists the providers the
  model was validated on.
- *Accessibility* (`uia`, `[models] accessibility = "uia"`,
  [ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)): the UI Automation tree
  of the foreground window the actuator attests (the `ApplicationFrameHost` frame resolves to
  its hosted app), read once per observation inside the planned regions, bounded by elements,
  depth and time (the window with the keyboard focus first, each window shallowest level
  first, so a walk cut short loses the deepest nodes), through the safe `uiautomation` crate (no `unsafe` of our own; the crate's
  input and pattern calls are forbidden by the adapter's `clippy.toml`). Evidence, not a
  replacement: an enabled, on-screen element with an action pattern is `confirmed` with its
  role and name (`label_source: accessibility`), the detector boxes that are the same control
  merge into it, and a box centred on another accessible control stops being confirmed. A walk
  that ends on its budget says so (`truncated`): those regions are read again on the next
  observation even if no pixel changed, evidence the partial read does not repeat is kept while
  its pixels are unchanged, and a visual box inside such a gap is not confirmed. It
  never reads a field's value, and a password field is no input. Allowed only with `dxgi` or
  `screen`; off by default.
- *OCR* (`winrt-ocr`, `[models] ocr = "winrt"`): `Windows.Media.Ocr` for exactly
  `ocr_language`, never another language, without invented confidences; recognised text is
  never logged.
- *Configuration*: a configured detector or OCR that the build lacks, whose files do not
  match their SHA-256 or whose language is not installed stops the engine before any capture
  opens. `images`, `screen` and `dxgi` all take the `[models]` detector and OCR.
- *Bounded `unsafe`*: DXGI and `SendInput` are COM and Win32 calls.
  [ADR-0016](../adr/0016-bounded-unsafe-for-windows-capture-and-input.md) (accepted on 2026-10-14) allows `unsafe` only in the `#[cfg(windows)] mod win` of
  the capture and input adapters: those crates deny `unsafe_code` and
  `clippy::undocumented_unsafe_blocks`, every block wraps one call with a `// SAFETY:`
  comment, and raw pointers never leave the module. The OCR uses the safe WinRT projection
  and the vision crate keeps `unsafe_code = "forbid"`. All of it uses one pinned `windows`
  crate (`=0.62.2`), and CI builds, lints and tests the features on `windows-latest`.

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
with nothing delivered. Only the `SendInput` actuator attests the foreground application;
the interim `enigo` input cannot, so its live actions are always denied. Before delivering, a
freshness check captures the screen again: if it differs from the committed base, the action
is `STALE_SNAPSHOT` and nothing is delivered. Then the engine takes a last look: it reads
the policy again (the operator's stop, consent, allowlist) and asks the actuator for the
foreground application again; a stop, or a foreground that is not the one attested at the
gate, is `POLICY_DENIED` with nothing delivered. The operator's stop comes from `[safety]
stop_file` (a file whose existence stops new input, read on every action; an unreadable
path counts as present) or an in-process `StopSwitch` in the composition root; no MCP tool
reaches either, and results already delivered stay recorded as reported. While live input is
possible, `serve` prints an active-control indicator on stderr, and a line whenever the stop
is engaged or cleared. A delivery receipt whose inserted events match the submitted ones is
`input_delivered`; anything else is `indeterminate`. An expected `visual_change` is verified
against the pre-delivery frame within a bounded settle window: the engine captures right
after delivery and, while nothing changed, again every `[engine] verify_poll_ms` (50 ms) on
its injected clock until `verify_timeout_ms` (500 ms; 0 is a single check) has elapsed. A
change is `effect_observed`; none is `indeterminate`, and the input is never sent again; the other effects (focus, value, scroll) are reported `indeterminate`
until they can be checked.
Any delivery, whatever its outcome, spends the snapshot: the next action needs a new
observation (`STALE_SNAPSHOT`), while journal replays still return the stored result.

The MVP closes with the reduced scope that the owner decided on 15 October 2026 ([CHANGELOG 1.0.52](../thesis/CHANGELOG.md), [ADR-0023](../adr/0023-licence-and-reduced-mvp-scope.md)): RNF-05 is accepted as a documented near-miss and is not met (two regions, p95 153 ms against 150 ms); the reduced S4 protocol replaces the 450 runs of EV-02 for this MVP; EV-01, EV-02, IT-07 and the REL-01 corpus move to the next cut. Three of the ten Definition of Done boxes of [chapter 06 §6.7](../thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md) are ticked, each with its evidence. Since 1.0.53 the engine deletes its state and cache when the MCP session ends, and the optimised tile comparison is checked against a scalar reference on generated frames ([CHANGELOG 1.0.53](../thesis/CHANGELOG.md); SEC-07, BDD-39, UT-08). Deletion on demand, without ending the session, is proposed in [ADR-0028](../adr/0028-operator-purge-file.md) (Proposed, off by default) and waits for the owner's decision. On `main` the five flows of the reduced protocol have ten of ten each (E2E-03 since 1.0.46). Re-evaluated on 1.0.58, the boxes keep the same three ticks, and eight decisions for the owner are listed at the end of §6.7 ([CHANGELOG 1.0.58](../thesis/CHANGELOG.md)).

| Area | Thesis | Today | Completed in |
|---|---|---|---|
| Change detection, fusion, identity, snapshots | Algorithms 1, 2, 3B; transactional commit | Implemented in `ommatix-domain` and run by the `Engine` | S1–S2 (done) |
| Icon cache | Algorithm 3A in the observation | Run by the `Engine` on icon candidates of the planned regions with the operator's curated prototypes; off without them; context configured, thresholds not calibrated | S3 (context), S4 (calibration) |
| Perception | Detector and OCR on ONNX Runtime, roles from the S0 spike | By default, flat-colour region detector (roles `unknown`) and no OCR (`NullTextRecognizer`); the simulated desktops report ground truth. Opt-in adapters exist for `images`, `screen` and `dxgi`, but no model is selected: a fail-closed ONNX detector driven by a SHA-256-pinned manifest (feature `onnx`) and Windows OCR for one language (feature `winrt-ocr`). The supervised and reduced S4 runs select `icon_detect_v3` on DirectML and Windows OCR (es-ES) through the operator's `[models]` section | S0 (models), S3 |
| Agent contract | Ommatix 1.0 over MCP 2026-07-28; 2025-11-25 opt-in | Implemented (CT-01..10); driven live by the minimal clients `tools/s3/mcp_driver.py` and `tools/s4/e2e_driver.py` (the reduced S4 protocol, with ten of ten scored runs in each of E2E-01, E2E-02, S4R-02 and S4R-03 and three of ten in E2E-03, [S4 report](../thesis/S4_ACEPTACION_REDUCIDA.md)), not yet by a third-party client | S0/S3 for the client |
| Action gate | Freshness, policy, allowlist, focus, stop | Snapshot freshness (also re-checked by a capture before delivery), policy and the allowlist against the foreground application `SendInput` attests, all checked again right before delivery; the operator's stop file (`[safety] stop_file`) and, optionally, the operator's scope on windows (ADR-0020, accepted: window classes and a pinned window, checked on the foreground window and the window under the point); the actuator also checks the process under the target point, integrity level and held modifiers, and for `type` keyboard focus after the focusing click. Run under supervision on `notepad.exe` ([S3 runs](../thesis/S3_CORRIDAS_SUPERVISADAS.md)): the stop file, the freshness check and the policy refused live clicks without sending anything; a change of foreground application could not be provoked (SEC-02, SEC-05 in part) | S3 (supervised runs) |
| Effect verification | Focus, value, scroll and visual postconditions | Visual change only, re-captured within a bounded settle window (`verify_timeout_ms`, `verify_poll_ms`); added after the S3 runs, where a single capture right after delivery never saw the effect (4 of 4 `indeterminate`); confirmed live in the second run (2 of 2 clicks `effect_observed`; with a 0 ms window, `indeterminate` and no retry: IT-04 in part) | S3 |
| Telemetry | Metrics and audit without sensitive text | `get_engine_metrics`; stage timings in `_meta` | S3 |
| Platform | Windows 11 x64: DXGI and `SendInput` | DXGI capture and `SendInput` input behind the `dxgi` and `sendinput` features (ADR-0016), built and tested in CI on Windows run in part under supervision ([S3 runs](../thesis/S3_CORRIDAS_SUPERVISADAS.md): IT-01, IT-03 and IT-04, eight real clicks on `notepad.exe` over two runs; the first DXGI frame shows the desktop live; IT-07 not run and moved to the next cut) and in the reduced S4 protocol (E2E-01, E2E-02, S4R-02 and S4R-03 with ten of ten scored runs each, E2E-03 with three of ten; `SendInput` attests a hosted UWP app, not `ApplicationFrameHost.exe`); simulated and fixture desktops for tests; interim `xcap` and `enigo` behind `screen` and `native-input` | S3 (supervised runs) |

The thesis scenarios (BDD-01..37) stay in [`docs/thesis/features/`](../thesis/features),
where the dossier's traceability matrix expects them; `src/acceptance/tests/thesis.rs` runs
all of them: BDD-01..10 and BDD-13..21 on the domain core, BDD-11, 12 and 22..27 through
the engine and the MCP adapter over the simulated and fixture desktops, and BDD-28..32
(another application taking the foreground before typing, indeterminate verification
without retry, a repeated `request_id`, on-screen text asking for more permission, the
operator's stop file during a sequence) the same way over a test double of the live
actuator that records what it is handed and sends nothing. They show the engine's side of
SEC-05, IT-04, IT-05, SEC-03 and SEC-02, which still need supervised runs with the real
actuator; the keyboard-focus check inside the allowed application is the `SendInput`
actuator's, and no test drives it. The two-phase `type` of ADR-0018 (focus confirmed on the
target element by the engine, BDD-34 and BDD-35) runs over doubles of the actuator and of
the accessibility provider. The operator's scope on windows ([ADR-0020](../adr/0020-operator-scope-on-windows.md),
accepted: window classes and a pinned window, so that `explorer.exe` can be allowed without the
desktop and the taskbar) is BDD-36 and BDD-37, over the same actuator double; its Win32 half is
only compiled and linted, not run live.

The tests that need no live input and no screen are Rust tests too (CHANGELOG 1.0.35): IT-06
(`_meta` carries per-stage times as whole numbers and nothing from the screen; `ommatix serve`
writes replies to stdout, its own `ommatix:` lines to stderr and nothing to disk), SEC-03 (OCR and
accessibility text never changes the policy, the allowlist, the gate or actionability), SEC-04
(no result carries pixels, a file or a key the schemas lack; no production code opens a socket
or writes a file), SEC-06 (manifest, runtime library, weights and the curated-icon file are each
checked against a pinned SHA-256 before anything is loaded; every artefact in the shipped
manifests records origin, digest and licence), PERF-01 (`core_latency`, release build: the core on
300 controls and 600 text spans), REL-01 and DOC-01 (`src/acceptance/tests/repository.rs` and
`docs/thesis/validar_paquete.py`). They run over doubles, the simulated desktop and temporary
files; the same cases with the real OCR, UI Automation tree and operator still need supervised runs.
