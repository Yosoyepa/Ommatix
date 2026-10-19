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
