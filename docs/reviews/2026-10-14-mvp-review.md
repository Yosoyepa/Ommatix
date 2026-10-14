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

## Verification pass

A second, independent reviewer re-checked the fixes above at `1e189f8`, reproducing each
claim with scratch tests and release-build timings. Of the 23 findings, 18 were confirmed
fixed, five (#1, #2, #4, #15, #18) were fixed only in part and #20 had a nit; 12 new
defects turned up (N1–N12). All are resolved in this PR, each with a regression test.

### Findings fixed only in part

| # | What was left | Resolution | Regression test |
|---|---|---|---|
| 1 | A window flush with three screen edges (snapped to the left half, 960–1620 px wide) took over the screen border: the wallpaper became the element and the window vanished | Screen-edge surfaces are flat stretches covering at least half of an edge, measured on the whole frame; a snapped window and the wallpaper are both surfaces | `a_snapped_window_and_the_wallpaper_are_both_surfaces` (unit and pipeline) |
| 2 | Screens packed with tiny segments (2×2 dots at a 3 px pitch, 3×3 random tiles) still took ≈ 15 s per 1080p frame: every grouping search scanned a 64 px bucket of some 450 boxes | Each grouping pass builds its own grid (4 px cells, coarser when sparse); output unchanged. 1080p dots 16 s → 0.15 s, text page 109 → 74 ms | `a_dotted_screen_is_one_element`, `random_tiles_are_grouped_deterministically`, `agrees_with_the_definitions_on_random_boxes`, `bench_dense_screens` |
| 4 | The mapping was wrong on Linux X11 with a display scale | See N3 | See N3 |
| 15 | With the real clock, frame counters in `structuredContent` (`observation.frames`, `first_seen`) differed between identical live runs | Not a code defect: those runs saw different numbers of frames. ADR-0012 now states the contract exactly: the same frame sequence gives byte-identical payloads | `conversations_are_reproducible_byte_for_byte` |
| 18 | `Frame::from_rgba` and `Frame::solid` overflowed on huge sizes, `Rect::iou` on huge rectangles, and `DisplayMapping::to_os` instead of saturating | Checked buffer length (`FrameTooLarge`); IoU and the mapping computed in 128 bits, the mapping saturating | `rejects_sizes_that_overflow`, `iou_of_huge_rects`, `extreme_coordinates_saturate` |
| 20 | U+2028, U+2029 and U+202E got through `type_text` | Line and paragraph separators and bidirectional marks, embeddings, overrides and isolates are refused | `invalid_requests_never_reach_the_controller`, `actuation.feature` |

### New findings

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| N1 | High | Whether a segment on the screen edge was background was judged from the analysed area's slice of the border: a full-width bar that appeared was missed, and a hover inside a taskbar removed the taskbar | Screen-edge surfaces are measured once per call on the whole frame (`FrameBorder`), so incremental and full analysis agree | `the_screen_edge_is_judged_the_same_in_every_region`, `surfaces_cover_half_an_edge_and_buttons_do_not` |
| N2 | High | A container that shrank kept its old, larger bounds, and a click by id missed it: every container strictly around a change was skipped, even when its own edge was the change | Only containers that enclose the change in their interior, sharing no edge, are skipped | `a_shrinking_container_gets_its_new_bounds`, `containers_sharing_an_edge_with_the_change_are_re_detected` |
| N3 | High | Linux X11 with `Xft.dpi` 192: clicks landed at half the distance from the origin, because xcap reports scaled geometry while capture and XTest use physical pixels | Mapping per platform (macOS points, X11 physical pixels, Windows only at 100%); native input refused where unverified (Wayland, scaled Windows). Checked live under Xvfb: frame pixel (300,200) went to (150,100) before and lands on (300,200) now | `x11_hidpi_clicks_land_on_the_physical_pixel`, `retina_frames_map_to_points`, `unverified_systems_refuse_native_input` |
| N4 | Medium | `ScreenSource` served the frame captured at start-up forever | A fresh capture on every tick; a frame of another size is an error, since the mapping no longer holds. The monitor is looked up by id, which also makes the source `Send` on Windows | `a_resolution_change_is_an_error`; CI now cross-checks capture and input for Windows and macOS |
| N5 | Medium | Grouping glyphs into words depended on history: the region of a new glyph did not reach the glyph 3 px before it | Expansion absorbs elements within the detector's context margin, and regions that close merge | `glyphs_typed_one_by_one_group_like_a_full_analysis`, `elements_within_the_detector_margin_are_absorbed` |
| N6 | Medium | A failed observation lost the changes of the frames it had already observed | `ObserveError` carries the partial observation; MCP reports it next to `observe_error` | `a_failing_tick_keeps_what_was_already_seen`, `a_failure_while_watching_keeps_what_was_already_seen`, `actuation.feature` |
| N7 | Low–Med | A 1 px diagonal line left phantom fragments: continuity outside the area checked only the straight-across neighbour | All 8 outside neighbours are checked | `a_diagonal_line_continues_outside_the_area`, `a_diagonal_line_through_a_region_leaves_no_fragment` |
| N8 | Low | A `tools/call` without an id was performed and never answered | Requests without an id are never executed | `a_tool_call_without_an_id_does_nothing`, `mcp_protocol.feature` |
| N9 | Low | `timeout: Duration::MAX` passed validation; observing a blinking screen never returned | The timeout is capped at 60 s | `rejects_policies_that_could_spin_forever` |
| N10 | Low | The cell hash missed about one in 2¹⁶ colour changes spread over two words, and never saw the top bit flip in two words | Multiply-xorshift mixing, a bijection per word; alpha is not hashed. 1080p hashing 1.3 → 1.9 ms | `changes_spread_over_two_words_do_not_collide`, `alpha_is_not_part_of_the_picture` |
| N11 | Low | Merging regions needed one pass per merge on a staircase (1.5 s for 2,000 rectangles) | `merge_nearby`: one sweep through the spatial index | `a_staircase_merges_in_one_sweep` (20,000 rectangles), `matches_the_naive_fixpoint_on_random_rects` |
| N12 | Low | Array arguments were read positionally, and `"e00001"` resolved to `e1` | Arguments must be objects; an element id has one form | `arguments_must_be_an_object`, `element_ids_round_trip_and_reject_junk`, `mcp_protocol.feature` |

No problems were found in the dependency rule, in ordering (no hash map or randomness
reaches the output), on tiny frames (1×1, 1×N, N×1) or with other malformed MCP input. The
Gherkin specification grew to 40 scenarios (191 steps).

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

Una segunda revisión independiente verificó esas correcciones: 18 estaban completas, 5
eran parciales y aparecieron 12 problemas nuevos. Los más graves: el borde de la pantalla
se juzgaba con el trozo analizado y no con el frame completo (barras que aparecían o
desaparecían sin motivo), un contenedor que se encogía conservaba su tamaño anterior, y en
Linux X11 con escalado los clics caían a la mitad de distancia. También se agruparon
pantallas llenas de segmentos diminutos en 0,15 s en vez de 15 s, se conservó lo observado
cuando la observación falla a mitad, se ignoran las peticiones sin id y se reforzó el hash
de celdas. Todo quedó corregido en este PR, cada punto con su prueba de regresión, y la
especificación Gherkin pasó a 40 escenarios.
