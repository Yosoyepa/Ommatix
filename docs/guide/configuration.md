# Configuration

[Español](configuration.es.md) · [Getting started](getting-started.md)

## Build features, all off by default:

| Feature | Adds | Platform |
|---|---|---|
| `screen` | `--source screen`: the primary monitor through `xcap` (interim) | Windows, macOS, Linux |
| `native-input` | `--input native`: `enigo` input (interim; implies `screen`; never acts live, see below) | Windows, macOS, Linux |
| `dxgi` | `--source dxgi`: DXGI Desktop Duplication of one output | Windows |
| `sendinput` | `--input sendinput`: Win32 `SendInput` (implies `dxgi`) | Windows |
| `onnx` | the ONNX detector of `[models]`; ONNX Runtime is loaded at run time from the library the operator pins, nothing is downloaded at build time | any |
| `directml` | `[models] detector_execution_provider = "directml"`: the detector on DirectML (implies `onnx`); needs an ONNX Runtime build with DirectML and a pinned `DirectML.dll` | Windows |
| `winrt-ocr` | `[models] ocr = "winrt"`: the OCR built into Windows | Windows |
| `uia` | `[models] accessibility = "uia"`: the foreground window's UI Automation tree as evidence fused with the visual path ([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)); needs `--source dxgi` or `screen` | Windows |

`serve` and `demo` take the same options:

| Option | Values | Default |
|---|---|---|
| `--config PATH` | a configuration file (below) | the dossier template's values |
| `--source` | `sim` (simulated desktop, with its own detector and recogniser), `images` (PNG frames), `screen` (primary monitor; feature `screen`) or `dxgi` (one monitor through DXGI Desktop Duplication, physical pixels, frames kept in memory; feature `dxgi`, Windows). The real sources take the `[models]` detector and OCR | `sim` |
| `--images DIR` | the PNG directory for `--source images` | none (required by `images`) |
| `--output N` | the output `--source dxgi` duplicates, in DXGI order (0 is normally the primary monitor); a rotated output is refused | `0` |
| `--input` | `dry-run` (record, never touch the OS), `native` (`enigo`; feature `native-input`, source `screen`) or `sendinput` (Win32 `SendInput`; feature `sendinput`, source `dxgi`) | `[engine] mode` (`dry_run`) |
| `--theme` | `light`, `dark`, `contrasting-wallpaper` (simulated desktop only) | `light` |
| `--session-id ID` | 1–64 visible ASCII characters, for reproducible output; with live input a fixed ID is refused at start (chapter 08 §8.4) | derived from the start time and the process ID |
| `--legacy` | also serve the legacy MCP profile 2025-11-25 (like `[mcp] legacy_enabled = true`) | off |
| `--observations N` (`demo` only) | 1–20 | 3 |
| `--plan-trace` (`demo` only) | also write one line per observation to stderr, a failed observation included: `ommatix-plan-trace` and a JSON object (schema `ommatix.plan-trace/2`) with why the planner chose that work, as counts, areas and class names only; every field is defined [below](#the---plan-trace-line). No pixel, text or coordinate; never on stdout, and no MCP client can ask for it | off |
| `--plan-check` (`demo` only) | also compare each regional observation with a full read of the same frame, run after the observation is committed, and write one line per observation to stderr: `ommatix-plan-check` and a JSON object (schema `ommatix.plan-check/2`) with how many boxes each read has and the other lacks, and where and how the differing words differ ([ADR-0045](../adr/0045-support-aware-closure.md)); every field is defined [below](#the---plan-check-line). A diagnostic of the operator's: it runs perception a second time, so a regional observation costs about twice as much, and its time is never reported as the observation's. Counts only; never on stdout, and no MCP client can ask for it | off |

### The `--plan-trace` line

Each line is `ommatix-plan-trace ` and one JSON object on the same line (keys sorted). It comes from
the planner of the observation (Algorithm 1) and holds counts, areas and closed names only. **Areas
are in pixels of the frame and none exceeds the frame's.** A field marked *union* counts a pixel once
however many boxes cover it; *one box* is the area of a single rectangle. `null` means that the planner
did not get to measure the field (see "When it is `null` or empty").

| Field | Unit | Meaning |
|---|---|---|
| `schema` | text | `ommatix.plan-trace/2` (schema 1, before [ADR-0045](../adr/0045-support-aware-closure.md), had no `chosen`, `spanning`, `component_support_classes` or `region_support_classes`) |
| `observation_id` | number | the observation's position in this `demo` run, from 1 (as in `--timings`) |
| `outcome` | text | `ok`, or `error` when the observation failed (nothing was published) |
| `error_code` | text | the contract's error code of a failed observation (for example `CAPTURE_UNAVAILABLE`); `null` when `outcome` is `ok` |
| `work` | text | the plan's work: `full`, `regions` or `idle` (as in the observation record) |
| `full_refresh_reason` | text | the reason of a `full` plan (`no_committed_base`, `geometry_changed`, `closure_budget`, `too_many_regions`, `area_budget`, `experimental_baseline`, `adaptive_cost`); `null` otherwise |
| `frame_area_px` | pixels | the frame's area, width times height |
| `tile_size` | pixels | the side of a tile (`[engine] tile_size`) |
| `changed_tiles` | count | tiles that differ from the committed base; every tile when there was no base or its geometry differed |
| `pending_tiles` | count | tiles of this frame's grid still pending from a failed observation, changed or not |
| `dirty_tiles` | count | the tiles the plan keeps dirty: changed and pending, each once; the whole grid when a full refresh was decided before any region |
| `components` | list | one entry per 8-connected group of dirty tiles, in the planner's order, as it stood after the first closure over the last snapshot's evidence |
| `components[].tiles` | count | tiles in the group |
| `components[].tight_area_px` | pixels, one box | the box around the group's tiles, before the halo |
| `components[].padded_area_px` | pixels, one box | that box after the halo, clipped to the frame: the region **before** the closure |
| `components[].closed_area_px` | pixels, one box | the region **after** the closure |
| `components[].largest_support_px` | pixels, one box | the largest evidence box (a detected control, a word, an accessibility element, or a member of a relation) that the closed region overlaps; `0` when none. Under the classic closure the region contains it; under the support-aware closure it may be a spanned surface or an accessibility box that the region overlaps without containing |
| `regions` | list | the plan's final regions after merging and the last closure, in the plan's order |
| `regions[].area_px` | pixels, one box | the region |
| `regions[].components` | count | how many groups ended in this region |
| `regions[].pre_closure_area_px` | pixels, **union** | what the region was made of before any closure or merge: the union of those groups' padded boxes, an overlap counted once; at most `area_px` |
| `regions[].largest_support_px` | pixels, one box | as `components[].largest_support_px`, for the final region |
| `covered_area_px` | pixels, sum | the sum of the regions' areas, which the area budget compares; the regions do not overlap, so it is also their union |
| `budget_area_px` | pixels | `full_refresh_area_ratio` of the frame (`[engine]`): a threshold, at most the frame |
| `max_regions` | count | `[engine] max_regions`, the most regions a regional plan may have |
| `decided_by` | text | the check that made the plan a full refresh (the same names as `full_refresh_reason`); `null` for an idle or regional plan |
| `decisive_region` | index | for `area_budget`: the 0-based index in `regions` of the largest region (the first on a tie); `null` otherwise |
| `chosen` | text | which closure made the plan that ran: `spanning` (the support-aware closure of [ADR-0045](../adr/0045-support-aware-closure.md)) or `classic` (no detector band declared, a plan decided before any region, or rule R4 gave way to the classic plan) |
| `spanning` | object | what the support-aware closure did, when it ran; `null` when it did not (the detector declares no surfaces, or the plan was decided before any region). When it ran and R4 gave way to the classic plan, `components`, `regions`, `covered_area_px`, `decided_by` and `decisive_region` describe the support-aware attempt, and `spanning.classic` the plan that ran |
| `spanning.surfaces_declared` | count | distinct surfaces the detector declared in the last evidence |
| `spanning.surfaces_spanned` | count | of those, the ones spanned (not closed over) |
| `spanning.largest_spanned_px` | pixels, one box | the largest spanned surface; `0` when none |
| `spanning.whole_on_read_cut` | count | accessibility boxes that a final region overlaps without containing (rule R1: they are re-read whole) |
| `spanning.largest_whole_on_read_cut_px` | pixels, one box | the largest of those; `0` when none |
| `spanning.relations_ignored` | count | relations the closure ignored because their control is a spanned surface or an accessibility box (rule R3) |
| `spanning.attempt_reason` | text | the reason the support-aware attempt was a full refresh (the names of `full_refresh_reason`); `null` when it was regional |
| `spanning.classic` | object | the classic plan, when rule R4 computed and ran it; `null` otherwise |
| `spanning.classic.work` | text | `full` or `regions` |
| `spanning.classic.full_refresh_reason` | text | its reason; `null` when regional |
| `spanning.classic.region_count` | count | the regions its closure produced (what the region budget compared); `0` when the closure ran out of rounds |
| `spanning.classic.covered_area_px` | pixels, sum | the sum of their areas (what the area budget compared) |
| `component_support_classes` | list | for each entry of `components`, in order, the class of its `largest_support_px` box: `cut_prone` (a box a region must contain whole), `surface` (a detector surface) or `whole_on_read` (an accessibility box); `null` when it touches none or the closure was the classic one without a split of the evidence |
| `region_support_classes` | list | the same for each entry of `regions` |

**When it is `null` or empty.** A failed observation publishes nothing, so its line carries only
`schema`, `observation_id`, `outcome` and `error_code`: every other field is `null`, and `components`
and `regions` are `[]` (`component_support_classes` and `region_support_classes` are `null`). A full refresh decided before any region exists (`no_committed_base`,
`geometry_changed`, `experimental_baseline`) has `components` and `regions` empty and
`covered_area_px` `0`; so has `closure_budget`, whose component that ran out of rounds leaves no
entry. `covered_area_px` and `regions` are filled once the regions are merged, so they are also filled
for `too_many_regions`. An idle plan has both lists empty.

To tell why a full refresh happened, compare a component's `padded_area_px` with its `closed_area_px`
and the frame: a small `padded_area_px` with a large `closed_area_px` and `largest_support_px` means
that the closure over a large evidence box made the region large; a `padded_area_px` that is already
large means that the change itself was large. With schema 2 the class of that box says what it
was: a `surface` or a `whole_on_read` box on a full refresh decided by the classic closure is a box
the support-aware closure would have left out.

### The `--plan-check` line

Each line is `ommatix-plan-check ` and one JSON object (keys sorted), one per observation. After a
**regional** observation is committed, the detector, the recogniser and the accessibility provider run
again on the whole frame, and the two sets of evidence are compared box by box, as multisets. A box
"at a spanned or whole-on-read box" overlaps a surface the closure spanned (a region met it without
containing it) or an accessibility box a region met: where [ADR-0045](../adr/0045-support-aware-closure.md)
changed what a regional observation keeps. Its counts there must be `0`; counts elsewhere come from
perception that is not local (OCR reads lines in context: limit L3).

| Field | Unit | Meaning |
|---|---|---|
| `schema` | text | `ommatix.plan-check/2` (version 2 adds `differing_words`) |
| `observation_id`, `outcome`, `error_code`, `work` | | as in the `--plan-trace` line |
| `checked` | boolean | `true` when the comparison ran: the observation was regional and the full read succeeded |
| `full_truncated` | boolean | the full read's accessibility walk ended on its budget, so its absences prove nothing; `null` when not checked |
| `only_regional` | object | boxes in the regional observation's evidence and not in the full read's; `null` when not checked |
| `only_full` | object | boxes in the full read's evidence and not in the regional observation's; `null` when not checked |
| `….controls`, `….words`, `….accessibility` | object | per kind of evidence |
| `….at_spanned_or_whole_on_read` | count | of those boxes, the ones that overlap a spanned surface or an accessibility box a region met |
| `….elsewhere` | count | the others |
| `differing_words` | object | the words of `only_full` and `only_regional` described; `null` when not checked. Counts, closed class names and one distance in pixels; no text and no coordinate |
| `differing_words.only_full`, `differing_words.only_regional` | object | those words by where each box lies against the observation's regions (where the recogniser read a crop) |
| `….inside_a_region` | count | inside one region. A difference here is the stated limit L3 of ADR-0045: the same pixels recognised from a crop |
| `….crossing_a_region_edge` | count | meets a region without lying inside any. Never L3: a word a region cut |
| `….outside_every_region` | count | meets no region. Never L3: a word the regional observation holds from an earlier read and a full read sees otherwise (a change the plan missed, or an earlier crop's difference still held) |
| `differing_words.pairs` | object | each word of `only_full` paired with the not yet paired word of `only_regional` whose box overlaps it most |
| `….same_text_other_box` | count | pairs with the same text and another box |
| `….largest_edge_difference_px` | pixels | the largest difference between corresponding edges of a same-text pair; `0` when there is none. A distance, not a coordinate |
| `….other_text` | count | pairs with another text |
| `….unpaired_only_full`, `….unpaired_only_regional` | count | words of each side left without a pair |
| `differing_words.unpaired_or_other_text` | object | the words of the pairs with another text and the unpaired words, of both sides |
| `….at_dirty_tiles` | count | of those, the ones whose box overlaps a dirty tile of the plan (a tile whose pixels changed since the committed base, or one still pending): where the screen changed, for instance the caret |
| `….away_from_dirty_tiles` | count | the others |
| `differing_words.never_l3` | count | `crossing_a_region_edge` plus `outside_every_region`, over both sides: the differing words that are never the limit L3 |

## Configuration file

The configuration file has the shape of
[`docs/thesis/plantillas/configuracion_mvp.toml`](../thesis/plantillas/configuracion_mvp.toml)
(`[engine]`, `[mcp]`, `[safety]`, `[models]`, `[cache]`; at most 64 KiB); without one, the
template's values apply, which deny actuation and redact values. No CLI flag loosens
`[safety]`. The file is checked strictly:

- **Refused** (the server does not start): unknown sections or keys; out-of-range values
  (`tile_size` 8–1024, `halo_px` ≤ 1024, `max_regions` 1–1024, `full_refresh_area_ratio`
  in (0, 1], `adaptive_min_regions` 0–1024 (default `2`; a plan with that many regions or more is processed as a full refresh, reason `adaptive_cost`, [ADR-0030](../adr/0030-adaptive-full-or-regional-plan.md); `0` switches the rule off), `max_snapshot_age_ms` ≥ 1, `verify_timeout_ms` 0–5000, `verify_poll_ms` 1–1000, `concurrent_stages` a boolean (default `true`), `candidate_search` `scan` (default, the reference) or `rstar` (experimental R*-tree, [ADR-0027](../adr/0027-candidate-enumeration-with-an-rstar-index.md); the same results), `max_message_bytes` 4096–65536, `max_entries` ≥ 1,
  blank `allowed_applications` entries, a blank `stop_file`, `window_pin_file` or `purge_file`, an
  `allowed_window_classes` entry that could never apply or that names the desktop, the taskbar or
  `applicationframehost.exe`); and values this build cannot
  honour: a `platform` other than `windows`, a `snapshot_policy` other than `strict`, a `profile` other than
  `2026-07-28` (the legacy profile is enabled with `legacy_enabled`), a `transport` other than
  `stdio`, `allow_destructive = true`, `allow_screenshots_to_client = true`,
  `[models] detector_path`, `detector_sha256`, `ocr_path` or `ocr_sha256` (weights are
  declared in the model manifest), a half-configured or malformed detector pin (below),
  and `[cache] persist = true`.
- **Enforced** (ADR-0025): `max_snapshot_age_ms` (default `10000`) bounds how old, on the
  engine's clock, the capture of the published snapshot and the accessibility read that
  confirms a target may be when an action is authorised. Past that age the action is refused
  with `STALE_SNAPSHOT` (observe again, never re-target): at the gate, simulated actions
  included, and again before every real input, including the keys of a two-phase `type`.
  A new publication does not renew an old accessibility read.
- **Accepted with no effect** (a note on stderr at startup): `actuation_enabled = true` with
  an input adapter that is not live.

`[cache]` may also turn on the icon resolver (Algorithm 3A) with six keys that go together:
`prototypes` (a JSON-lines file, relative to the configuration file, one
`{"label", "context", "grid"}` per line, where `grid` is the 9×8 greyscale crop as 72 values),
`prototypes_sha256` (the file's SHA-256: it is checked before the file is parsed, so a prototype
added after the operator reviewed it stops the server, SEC-06), `context` (the application the prototypes apply to; until S3 identifies the foreground
application, the operator names it), `gradient_tau` (1–255), `max_distance` and `min_margin`
(0–1). None has a default, because the thesis calibrates them on its corpus (H4); any of them
without the others is refused, and so is the whole file on any invalid line or more than
`max_entries` prototypes. Without them the startup note says that icon labels stay unknown.

`[models]` is empty by default: no model is loaded (`models_loaded: false`), the detector is
the flat-region finder and there is no OCR. With the `images`, `screen` or `dxgi` source it
can select:

| Key | Meaning |
|---|---|
| `detector_manifest`, `detector_manifest_sha256` | a manifest shaped like [`model_manifest.json`](../thesis/plantillas/model_manifest.json), with status `SELECCIONADO`, whose detector entry names the weights, their SHA-256 and the roles and affordances of each class; and the manifest's own SHA-256 (64 hex characters) |
| `onnxruntime_path`, `onnxruntime_sha256` | the ONNX Runtime library (`onnxruntime.dll`) and its SHA-256; keep it in a directory only the operator can write |
| `detector_execution_provider` | `"cpu"` (default) or `"directml"` (build feature `directml`); the manifest's `execution_provider` must list it. DirectML never falls back to the CPU: if it cannot be created, or any node would run on the CPU, the detector is refused |
| `directml_path`, `directml_sha256` | with `directml` only: the `DirectML.dll` loaded by absolute path before ONNX Runtime (never the copy in System32), and its SHA-256 |
| `detector_threads` | intra-op threads, `1`–`64`; `0` (default) lets ONNX Runtime choose the number of physical cores |
| `ocr` | `"none"` (default) or `"winrt"`, the OCR built into Windows |
| `ocr_language` | the BCP-47 tag of the OCR language, default `es-ES`; never replaced by another |

The four detector keys go together (build feature `onnx`); relative paths are relative to
the configuration file. The manifest, the runtime library and the weights are each checked
against their SHA-256 before anything is loaded, and a mismatch, a missing build feature or
an uninstalled OCR language stops the engine before any capture opens: nothing falls back to
another model or language. With `--source sim` the keys are ignored, with a note on stderr.
No detector is accepted for the product; the spike selected a candidate for integration and measurement (S0, [ADR-0009](../adr/0009-models-and-licences-decided-in-the-spike.md)).



## Accessibility and experimental options

`[models] accessibility = "uia"` adds UI Automation evidence fused with the visual path; it never reads a field value or replaces vision ([ADR-0017](../adr/0017-ui-automation-evidence-fused-with-vision.md)). The [canonical template](../thesis/plantillas/configuracion_mvp.toml) documents its time and element bounds. `action_freshness = "target_window"` stays off by default and needs the validation in [ADR-0035](../adr/0035-freshness-group-limited-to-occluders.md) before live use. `experimental_full_frame` is exclusively a research baseline ([ADR-0024](../adr/0024-observation-spans-and-a-switched-full-frame-baseline.md)).

## Contract and diagnostics

The MCP server exposes three tools, with the dossier's JSON schemas:

| Tool | Does |
|---|---|
| `observe_screen` | Observes the screen and returns a page of the new snapshot: `session_id`, `snapshot_id`, `geometry_epoch`, elements with `target_id`, `revision`, role, label and actionability, never coordinates; `query`, `limit` and a `cursor` bound to the snapshot |
| `execute_action` | `click`, `type` or `scroll` on a `target_id` at an expected revision; dry run by default; returns `simulated`, `rejected`, `input_delivered`, `effect_observed` or `indeterminate` |
| `get_engine_metrics` | Mode, observations, rejected and delivered actions, pending regions, whether models are loaded |

Domain errors come back as tool results with `isError: true`, a `code`, `retryable` and a
`next_step`; run-dependent values travel in the result's `_meta`: stage timings under
`io.ommatix/timings` and whether the accessibility evidence is complete under
`io.ommatix/accessibility` (`observe_screen`), the stage timings of the action under
`io.ommatix/action_timings` (`execute_action`).

See [operator control](live-input.md) for stop, purge and window scope.
