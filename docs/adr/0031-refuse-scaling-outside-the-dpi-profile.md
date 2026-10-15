# 0031. Refuse a monitor scaling outside the dossier's DPI profile with UNSUPPORTED_GEOMETRY

- **Status:** Accepted (2026-10-15)
- **Date:** 2026-10-15
- **Approval:** Accepted by the project owner on 2026-10-15 at about 11:45 ("Fix now"), when the
  owner asked for the explicit refusal that RNF-07 requires to be implemented in this cut. The live
  check at 175 % is run by the coordinator and is not part of this record.

## Context

RNF-07 (dossier chapter 03) asks for the profile Windows 11 x64, the selected monitor and a DPI of
100, 125 and 150 %, with an explicit refusal outside the profile. Its evidence is IT-07. The engine
did not read the scaling. The DXGI capturer already reads the effective DPI of the monitor
(`GetDpiForMonitor` with `MDT_EFFECTIVE_DPI`, `src/adapters/capture/src/dxgi/win.rs`) and keeps it in
its `Geometry`, but the value never reached the frame. At 175 % the engine would have served the
screen without a flag. S4 §C and chapter 06 §6.7 decision 3 record that outcome (`served_unflagged`)
as a finding against RNF-07.

The engine already refuses a rotated output and a format other than BGRA8. It does so inside the
capture adapter, which surfaces the failure as `CAPTURE_UNAVAILABLE`, a retryable code with the
`observe_again` next step. That is not an explicit refusal of the scaling, and it is not changed here.

## Decision

1. **The profile is a constant.** `ommatix_domain::SUPPORTED_DPI` is `[96, 120, 144]`, which are
   100, 125 and 150 % (96 DPI is 100 %). It cites RNF-07 and S4 §C. It is not configurable: the
   dossier names this one profile and no configuration of it.
2. **The scaling travels with the frame.** `CapturedFrame`, the existing output type of the
   `FrameCapturer` port, gains `scaling: Scaling`. `Scaling` is either `Dpi(u32)` or `Unknown`. The
   DXGI capturer reports `Dpi` from the effective DPI it already reads. The simulated, fixture and
   scripted sources report `Dpi(96)`. The interim `screen` source (xcap) and the image-sequence source
   report `Unknown`. No port trait is added, no adapter crate is added, and no `unsafe` is added
   (ADR-0016 is unchanged).
3. **The refusal.** `Engine::observe` refuses a frame whose scaling is known and outside
   `SUPPORTED_DPI` with `UNSUPPORTED_GEOMETRY`, retryable `false` and next step `stop`. The check runs
   after the capture and before planning. No inference runs, the identity registry is not touched and
   nothing is committed, so no snapshot is published. The refusal does what the epoch-0 refusal
   already does: it calls `record_failure` with no dirty tiles. The store becomes degraded, so actions
   on the last snapshot are refused with `STALE_SNAPSHOT` until an observation succeeds again. The
   message is a fixed English string with no screen content (ADR-0021).
4. **A scaling the adapter cannot tell.** `Unknown` is neither refused nor counted as in the profile.
   The engine makes no claim about it: no output says a scaling was verified. RNF-07 evidence cannot
   come from an `Unknown` source. The IT-07 evidence comes from sessions that read a known DPI, and
   `tools/perf/dpi_probe.py` reads the DPI itself.
5. **Actions after a scaling change.** Every capturer starts a new geometry epoch when the scaling
   changes: the DXGI `Geometry` includes the DPI, and the fixture's `set_dpi` bumps the epoch. An
   action on a snapshot taken in the profile is therefore stopped by the existing freshness check. A
   live action captures again, `dirty_tiles` reports `GeometryChanged`, and the result is
   `STALE_SNAPSHOT` with nothing delivered. The dry-run path does not capture again, by design (it
   sends nothing and never claims delivery, ADR-0007). It still refuses after a scaling refusal,
   because the store is degraded.
6. **The probe reads the error where the wire puts it.** `tools/perf/dpi_probe.py` read the code at
   the top level of the structured content. A tool error carries it under `error.code`
   (`tool_error.output`). Without this fix a real refusal would have been recorded as
   `refused_with_another_code`. The probe now reads `error.code`, and its fake engine uses the real
   envelope.

## Alternatives considered

- **A configurable profile.** Rejected. The dossier names one profile. A configurable list would let an
  operator accept a scaling that the evidence does not cover.
- **Refuse `Unknown` too.** Rejected. It would refuse the interim `screen` source and recorded images,
  which RNF-07 does not cover, and it would change their behaviour without evidence. The rule for an
  adapter that cannot know the DPI is fixed in decision 4.
- **Refuse inside the capture adapter, with a `PortError`.** Rejected. The engine maps a capture
  failure to `CAPTURE_UNAVAILABLE`, which is retryable and points the agent at a new observation. That
  is the wrong code and the wrong next step for a scaling the operator has not approved.
- **Treat `Unknown` as in the profile.** Rejected. It would claim a verification that nobody made.
- **Leave DPI-only changes without an epoch change.** Rejected. The DPI is already part of the geometry
  that the epoch tracks, and a scaling change moves the pixels the agent's element IDs refer to.

## Consequences and cost

- From the DXGI source, a display outside 100, 125 and 150 % gets an explicit refusal instead of a
  snapshot. An operator who needs another scaling needs a new ADR and a dossier change first.
- A rotated output and a non-BGRA8 format still surface as `CAPTURE_UNAVAILABLE`. Changing them to
  `UNSUPPORTED_GEOMETRY` is a separate decision.
- After a refusal the last snapshot's pages remain readable through their cursor, marked degraded, as
  after any failed observation. Its actions are refused.
- RNF-07 keeps the status `ACEPTADO_POR_ALCANCE` (ADR-0029). It changes only when IT-07 has evidence at
  100, 125 and 150 % and the owner decides.
- The hot path gains one comparison of a value that is already read. It adds no dependency and no
  allocation, so AGENTS.md §5 rule 8 does not ask for a measurement.

## Required validation

- Engine tests in `src/application/src/engine/tests.rs`:
  `a_scaling_outside_the_dpi_profile_is_refused_and_publishes_nothing`,
  `the_dpi_profile_and_an_unknown_scaling_are_served`,
  `an_action_on_the_last_snapshot_is_refused_after_a_scaling_refusal` and
  `a_live_action_after_a_scaling_change_is_stale_and_sends_nothing`.
- Unit tests: `only_a_known_scaling_outside_the_profile_is_refused` (`src/domain/src/display.rs`),
  `the_captured_frame_carries_the_monitor_scaling` (`src/adapters/capture/src/dxgi.rs`),
  `the_scaling_given_to_the_stamper_travels_with_the_frame` (`src/adapters/capture/src/stamp.rs`) and
  `the_fixture_reports_100_percent_scaling` (`src/adapters/sim/src/fixture.rs`).
- BDD-54, `docs/thesis/features/05_mcp.feature`, with its steps in `src/acceptance/tests/thesis.rs`:
  the refusal through the MCP contract, and a click on the old snapshot refused without delivery.
- Probe tests `test_the_refusal_code_is_read_where_the_wire_puts_it` and
  `test_outside_the_profile_an_explicit_refusal_is_flagged` (`tools/perf/test_dpi_probe.py`).
- IT-07 at 100, 125 and 150 % and the live change of scaling (chapter 06 §6.7 decision 3,
  `S4_SESIONES_PENDIENTES.md` §C). This is evidence for RNF-07, not a test of this record.

## Reversibility

Remove the check in `Engine::observe` and the `scaling` field in `CapturedFrame`. The error code
`UNSUPPORTED_GEOMETRY` already exists, so no wire, schema or stored data changes. Changing the profile
needs a new ADR and a dossier change first.

## Provenance

Dossier chapter 03 (RNF-07), chapter 06 §6.7 decision 3, `S4_SESIONES_PENDIENTES.md` §C, and chapter 07
§7.7 (the code and its next step). Related: ADR-0007 (honest simulation), ADR-0016 (bounded `unsafe`),
ADR-0021 (no screen text in errors), ADR-0023 (IT-07 outside the reduced MVP scope), and ADR-0029 (the
status by scope).

## Resumen (ES)

El motor no comprobaba la escala de la pantalla: a 175 % habría servido la captura sin marca, un
hallazgo contra RNF-07 («rechazo explícito fuera de perfil»). Esta ADR fija el perfil como constante
(`SUPPORTED_DPI` = 96, 120 y 144 DPI, es decir, 100, 125 y 150 %), lleva la escala del monitor en el
`CapturedFrame` del puerto de captura y hace que el motor rechace una escala conocida fuera de perfil
con `UNSUPPORTED_GEOMETRY` (no reintentable, siguiente paso `stop`), antes de planificar, sin inferir,
sin tocar el registro de identidades y sin publicar. Una escala que el adaptador no puede leer
(`Unknown`) no se rechaza y tampoco se cuenta como dentro de perfil. Un cambio de escala cambia la
época geométrica, así que una acción sobre un snapshot anterior se rechaza con `STALE_SNAPSHOT`. El
requisito sigue como `ACEPTADO_POR_ALCANCE` hasta que IT-07 tenga evidencia.
