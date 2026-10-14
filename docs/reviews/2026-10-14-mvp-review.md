# MVP code review (2026-10-14)

Independent review of the MVP (PR #2 at `8f0415b`) against the ADRs, the Gherkin
specification and general engineering practice (single responsibility, dependency
injection, determinism, robustness on external input). Every finding was reproduced with a
scratch test or a benchmark before being fixed; each fix ships with a regression test.

Decisions taken as a result: [ADR-0010](../adr/0010-flat-region-detector.md) (detector),
[ADR-0011](../adr/0011-incremental-perception-invariants.md) (pipeline invariants),
[ADR-0012](../adr/0012-settled-actions-and-wire-contract.md) (settled actions, wire
contract).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Blocker | The detector's background was the region's most frequent colour: a white window on a contrasting wallpaper made the wallpaper a frame-sized phantom element, and every later change re-analysed the full frame | `FlatRegionDetector`: background = what continues outside the analysed area or dominates the frame border | `a_contrasting_wallpaper_yields_no_phantom_and_stays_incremental`, `a_window_dominating_a_contrasting_wallpaper_is_an_element`, `appearance.feature` |
| 2 | Blocker | Box merging compared all pairs to a fixpoint: 185 s on a 1080p text page | Union-find + spatial index for segments, nested duplicates and glyph grouping; `merge_overlapping` likewise | `a_page_of_text_is_grouped_into_words`, `merging_thousands_of_rects_is_fast`, `bench_full_screen_detection` (1080p ≈ 0.1 s) |
| 3 | Blocker | `elements_in_rect` with huge or overflowing arguments hung the server or panicked | Saturating `Rect`; rect queries bounded by occupied buckets; MCP checks overflow and clips to the screen | `maximal_queries_are_cheap_and_never_overflow`, `hostile_rectangles_are_answered_without_overflow_or_stalls` |
| 4 | Major | Frame pixels sent to the OS unchanged (wrong on HiDPI and secondary monitors); native input accepted with non-screen sources | `DisplayMapping` measured by `ScreenSource`, applied by `NativeInput`; `--input native` requires `--source screen` | `retina_frame_maps_to_points`, `secondary_monitor_left_of_primary_has_negative_origin`, `native_input_is_refused_unless_the_source_is_the_screen` |
| 5 | Major | Region expansion was single-pass: a neighbour touched by the grown region was re-detected as a fragment | Expansion repeats until nothing grows or merges | `expansion_repeats_until_no_element_is_cut`, `expansion_invariant_holds_on_random_scenes`, `neighbours_of_a_grown_region_are_re_detected_whole` |
| 6 | Major | A resize keeping the grid's cell count left ghost elements | Geometry change = frame size change; scene cleared, old ids reported as removed | `same_grid_but_different_size_is_a_geometry_change`, `a_resize_removes_every_previous_element_even_with_the_same_grid` |
| 7 | Major | Greedy identity matching in detection order: a focus ring took the button's id | Global best-IoU-first matching | `best_overlap_wins_identity_regardless_of_order` |
| 8 | Major | An action whose follow-up perception failed was reported as an error (invites a double click); nothing waited for the screen to settle | `Observer` with `SettlePolicy` and injected `Clock`; `AgentSession` returns `performed` + `observe_error` | `waits_for_an_animation_to_finish_and_reports_the_net_change`, `a_failed_observation_never_hides_a_performed_action`, `acting_waits_for_the_screen_to_settle` |
| 9 | Major | JSON-RPC: batches and non-object messages mishandled; invalid UTF-8 stopped `serve` | `jsonrpc` module validates envelopes, answers batches, reads bytes and answers parse errors | `batches_get_one_response_per_request`, `serve_survives_invalid_utf8_and_blank_lines`, `protocol_errors_use_json_rpc_codes` |
| 10 | Major | A change inside a container re-detected the whole container | Strict containers are excluded from expansion and invalidation | `containers_of_a_change_are_neither_expanded_nor_invalidated`, `a_change_inside_a_window_costs_one_cell`, `a_solid_panel_survives_a_change_on_its_surface` |
| 11 | Minor | Cells hashed byte by byte | 8-byte words, length-tagged tail | `every_single_pixel_change_marks_only_its_cell`, `odd_cell_widths_hash_their_tail` |
| 12 | Minor | Per-pixel closure-heavy detector loop | Dense RGB buffer + union-find in one pass | `bench_full_screen_detection` |
| 13 | Major | `ElementDetector` could not fail and could return boxes outside its region | `detect -> Result<_, PortError>`; the engine clips detections; ticks are atomic | `detections_never_leave_their_region`, `a_failed_detection_changes_nothing_and_is_retried`, `uncommitted_changes_are_reported_again` |
| 14 | Minor | Domain types were the wire format (`serde` in the domain) | DTOs in the MCP adapter only; no `serde` in domain or application | `element_ids_round_trip_and_reject_junk`, `every_key_and_button_has_one_wire_name` |
| 15 | Minor | Timings made `structuredContent` differ between identical runs | Timings in `_meta`; virtual clock in tests | `conversations_are_reproducible_byte_for_byte` |
| 16 | Minor | Error messages repeated their cause or lost it | Each level displays itself; `error_chain` prints the chain once | `chain_lists_every_cause_once` |
| 17 | Minor | Configuration validated in some places only | Engine and observer validate everything at construction | `invalid_configuration_is_rejected`, `rejects_policies_that_could_spin_forever`, `invalid_settings_are_reported` |
| 18 | Minor | Domain API footguns (zero bucket, any IoU, overflowing arithmetic) | `NonZeroU32` buckets, validated IoU, saturating geometry | `rejects_invalid_identity_threshold`, `extreme_coordinates_saturate_instead_of_overflowing` |
| 19 | Minor | `McpServer::handle` mixed protocol, routing and tool logic | Split into `jsonrpc`, `server` and a tool registry | `names_are_unique_and_definitions_complete`, `rejects_malformed_envelopes` |
| 20 | Minor | Control characters typed blindly; `ScreenModel` listed as a port | `ControlCharacter` error; `ScreenModel` in its own module | `invalid_requests_never_reach_the_controller`, `invalid_actions_are_tool_errors_and_never_reach_the_desktop` |
| 21 | Minor | Gaps: no tests for themes, hostile input, resize, determinism | 33 Gherkin scenarios (was 21) and the tests in this table | `cargo test --workspace` |
| 22 | Minor | The demo script duplicated the simulator's layout | `sim::scripted_session(theme)` renders it from the simulator | `the_scripted_session_shows_each_step` |
| 23 | Minor | Nothing prevented a stray `println!` from corrupting MCP's stdout | Workspace lint `clippy::print_stdout = "deny"`; only `demo` opts out | `cargo clippy --workspace --all-targets` |

## Resumen (ES)

Una revisión independiente del MVP encontró 23 problemas, 3 de ellos bloqueantes: el
detector convertía el fondo del escritorio en un elemento fantasma del tamaño de la
pantalla (y con ello se re-analizaba todo en cada frame), la agrupación de cajas era
cúbica (185 s en una página de texto 1080p) y `elements_in_rect` podía colgar o hacer
fallar el servidor con argumentos enormes. Todos se corrigieron con una prueba de
regresión cada uno: nuevo detector por regiones planas, invariantes del pipeline
incremental, espera de estabilidad tras cada acción con reloj inyectado, mapeo de
coordenadas HiDPI y un adaptador MCP robusto con DTOs propios. Las decisiones quedan en
los ADR 0010, 0011 y 0012.
