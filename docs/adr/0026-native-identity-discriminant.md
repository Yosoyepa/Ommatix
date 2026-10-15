# 0026. Native accessibility identity as the discriminant, with abstention and a recorded reason

- **Status:** Accepted
- **Date:** 2026-10-15
- **Approval:** Project owner, pre-approved on 2026-10-15 for the Paper's next-cut increment INC-04, provided the change follows the Paper plan exactly (no safeguard relaxed, no change to the association rules).

## Context

Algorithm 3B (thesis chapter 04 §4.5) matches an observation to a track by role, normalised label and discriminant, by IoU, and only when the match is unique in both directions. The domain already supports a discriminant (`Observation.discriminant`), but the pipeline built every observation with `discriminant: None` (audit findings R07 and R10). Two visual boxes or two accessibility elements with the same text and box therefore matched by place alone. The virtualised-row test covered the domain path only, with a discriminant set by the test, so it did not show that the real pipeline carries one.

The Windows evidence says the runtime ID is worth using and has limits. The S4 client reads UI Automation's `RuntimeId` to ground its clicks. A control that is destroyed and recreated gets another one (the breadcrumbs of File Explorer), while an element that persists keeps the same one (the expanded menu of Notepad). The ID is scoped to the provider that issued it: it is not universal, and it is not persistent across processes or sessions. Visual detections (contours, OCR) have no such evidence.

## Decision

1. **The native identity of an accessibility element is its discriminant.** The UI Automation adapter reads the cached `RuntimeId` and the `NativeWindowHandle` of the window that hosts the provider, with the process id of the element. The application turns them into a scoped key, `uia:<process>:<window>:<runtime id>`, and gives it to the observation of the element. A runtime ID without a known process or window is not used at all. The adapter makes no call beyond the cache and no `unsafe` (ADR-0017).
2. **Visual elements carry no discriminant.** Their continuity is decided as before, by role, text, appearance and box.
3. **Uncertain continuity abstains with a new ID.** The association rules do not change: an element keeps its ID only when its scoped key, role, label and box agree with exactly one track, and that track is the only one compatible with it. Otherwise the old track retires and the element gets a new ID. Nothing redirects an action to the most similar neighbour.
4. **Each retirement records its reason as a code.** `Retirement` is `vanished`, `moved`, `substituted`, `ambiguous` or `geometry_changed`. It is computed for retired tracks only, kept with the published snapshot and read through `Engine::last_identity_changes`. It holds no label, discriminant, box or user identifier, and it never reaches the wire.
5. **Scope and lifetime.** A key is compared only with the track of the immediately preceding committed observation. A new process, a new window or a lapse in which the element is not seen is a new scope, and the registry already retires the track of an element that is not seen.

The contract, the wire DTOs and the action gate (`act.rs`) do not change.

## Alternatives considered

- **The runtime ID alone.** Rejected: it is not unique across providers, processes or windows.
- **Reading the process creation time to close process-id reuse.** It needs `OpenProcess` and `GetProcessTimes`, which would be `unsafe` in the accessibility adapter, and ADR-0017 forbids that. The residual risk is declared below.
- **A `Continuity` enum in `Observation` beside the discriminant.** Rejected: the discriminant already carries the comparison, and a second field would change every literal of the domain for no new behaviour.
- **Reading the accessibility tree of the whole window on every observation.** Rejected for this increment: Algorithm 1 reads only the regions that changed, and a full read costs far more than the perception it would protect. The identity of an unchanged region is not re-verified until its pixels change; the revalidation before delivery belongs to the TTL increment (INC-03).
- **Putting the reasons on the wire** (`_meta` or the metrics). Rejected: it changes the contract and the JSON schemas, and the reasons are diagnostics.

## Consequences and cost

- Accessibility elements whose runtime ID changes (a recreated control, a navigation, a recycled container) get a new target ID. Agents must observe again, which the contract already requires.
- Two identical twins that swap places lose both IDs instead of exchanging them. That is the conservative outcome the dossier asks for.
- Visual-only elements and elements without a runtime ID behave exactly as before.
- Cost: the association is unchanged. The reasons are computed only for retired tracks, and a track without candidates scans the observations that share its role and label. PERF-01 (`core_latency`) is the measurement. The recorded baseline in `docs/thesis/S0_RESULTADOS.md` §11 is not changed by this increment; the regression check made for this change, run under load and interleaved with `origin/main`, is reported in the pull request.

**Residual risks, declared.**

- A provider that gives the same runtime ID to a different object in the same window, between two consecutive observations, with the same role, text and box, keeps the ID. That is a limit of UI Automation shared by every client (audit R10); the engine cannot see past it.
- A process id and a window handle reused between two consecutive observations, with the same runtime ID, can match an old element. Process creation time would close this, and it is not read (above).
- A substitution that changes no pixel of its region is not seen until the region changes (Algorithm 1). The element keeps its ID until then, and the agent's action on it still passes the pixel re-capture. INC-03 must revalidate the native identity of the target before delivery.

## Required validation

- Domain (`src/domain/src/identity.rs`): `a_substituted_element_with_the_same_text_and_box_gets_a_new_id`, `the_same_native_identity_with_jitter_keeps_its_id`, `twin_controls_that_swap_places_never_exchange_their_ids`, `stacked_twins_keep_their_own_ids`, `retirements_record_why_an_id_was_lost`, and the pairwise check that the reasons cover exactly the retired tracks.
- Engine, end to end (`src/application/src/engine/tests/identity.rs`): UT-37 (`ut_37_a_substituted_element_with_the_same_text_and_box_gets_a_new_id`), jitter, the virtualised row, the swapped twins, the stacked twins, the navigation to another window or process, the recycled container, the element that comes back after a lapse, and `a_native_identity_is_the_discriminant_of_its_observation`, which fails if the pipeline drops the discriminant.
- Acceptance: BDD-40 (`docs/thesis/features/03_identidad.feature`), with its steps in `src/acceptance/tests/thesis.rs`.
- Adapter: `a_runtime_id_is_scoped_by_its_process_and_window` (pure); the `uia` feature compiles and is clippy-clean on Windows.
- Performance: `core_latency` (PERF-01), a regression check interleaved with `origin/main` and reported with the machine, the load and the commits.

## Reversibility

Remove the assignment of `discriminant` in `control_candidate` (`src/application/src/engine/observe.rs`) and the conversion in `accessible_control`: elements go back to visual rules. The retirement reasons are additive and can stay. No data migrates.

## Provenance

Paper, next-cut plan, INC-04 (`docs/02_Plan_Incremental_Critico.md`), and audit findings R07 and R10. It refines ADR-0005 (persistent tracking, which left the discriminant to the caller) and ADR-0017 (UI Automation evidence, whose identity was reserved for a later phase). It supersedes neither.

## Resumen (ES)

El `RuntimeId` de UI Automation de un elemento de accesibilidad se usa como discriminante de su track, acotado por proveedor, proceso y ventana; los elementos visuales no llevan ninguno. Si la continuidad no está probada, el objeto recibe un ID nuevo y nunca el del vecino más parecido, y cada retirada registra su motivo como código, sin valores ni identificadores de usuario. Motivo: una sustitución con el mismo texto y la misma caja no debe heredar el ID del objeto anterior, y los gemelos no deben intercambiarse. Quedan declarados los límites de UI Automation y la revalidación previa a la entrega, que corresponde a INC-03.
