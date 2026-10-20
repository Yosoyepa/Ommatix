# What an AG-03 session leaves on disk (follow-up to the oi1 question; ADR-0038 O5)

Origin: oi1 found that Claude Code's directory documentation lists full-size copies of MCP image tool results under
`<config>/projects/<project>/<session>/tool-results/` (30-day clean-up) and that no page says whether
`--no-session-persistence` prevents them, while ADR-0038 O5 says screenshots live in memory only. This file records the
measurement, the finding → resolution → regression test for each point, and what is **not** measured. Everything is
synthetic: the real CLI against the loopback stand-in (`fake_messages.ScriptedMessages`), the pixel server in `--synthetic`
mode (drawn PNGs), no provider, no capture, no key, no desktop read.

## Measurement (Claude Code 2.1.304, Windows; the files are outside the repository)

Production command (`pixel_agent.build_claude_argv`), a fresh `CLAUDE_CONFIG_DIR` and fresh `TEMP`/`TMP` per series, 4 image
sizes × 3 screenshots; every file of the configuration directory and of the fresh `TEMP` listed and scanned (PNG signature, its
JSON-escaped form, base64 in the three alignments, the probe's own synthetic PNGs), plus a before/after comparison of the
CLI's other known locations.

| Series | `--no-session-persistence` | Env | Configuration directory | Image data |
|---|---|---|---|---|
| base | yes (production) | — | `.claude.json` (51 KB) and a backup (84 B); no `projects/` | none |
| survey | yes | `CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY=1` | the same; a 2 KB `cache-break-state-*.json` in `TEMP` | none |
| control | **no** | — | 12 PNG under `projects/<cwd>/<session>/tool-results/mcp-pixel-blob-*.png` (1.4–46 KB) and 4 transcripts | **12 PNG files and 4 transcripts with base64 PNG** |

Also: the CLI keeps `%LOCALAPPDATA%\claude-cli-nodejs\Cache\<cwd>\mcp-logs-pixel\<date>.jsonl` (one file per session, ~4.6 KB,
keys `cwd`/`debug`/`sessionId`/`timestamp`, no image data, no mention of "image") **outside** the configuration directory.
Files with a PNG signature elsewhere under `~\.claude\projects` belonged to other live sessions of this machine (none held the
probe's synthetic PNGs); three `.tmp` files in the real `TEMP` could not be read and are not attributed (the probe used its own
`TEMP`).

## Findings and resolutions

| # | Finding | Resolution | Regression tests (`tools/agent/ag03/test_disk_evidence.py`) |
|---|---|---|---|
| D1 | The preflight did not check what its synthetic sessions leave on disk, so a CLI (or a command) that persists images would still pass | `pixel_disk` snapshots the configuration directory and the CLI's cache around each synthetic session; the proof is refused when a session wrote anything under `projects/`, a `tool-results` file, a `.jsonl` transcript, PNG / base64-PNG data, or when a touched file cannot be read (`*_disk_config_*`, `*_disk_cache_*`); the facts go in the proof (`disk`, verifier revision 2) and the loader refuses a proof without them | `OnDiskEvidenceTests` (clean session; tool-results image; transcript with and without an image; image data in the cache log; a clean cache log counted; all three sessions checked; the loader refuses missing or bad facts), `PixelDiskModuleTests` |
| D2 | `--no-session-persistence` was only a flag in a function, with nothing that fails if it disappears | A checked invariant: `pixel_agent.command_problems` in a test, in every preflight session and before every launch in the runner (`production_command_persists_session`; the attempt is aborted and no session starts) | `ProductionCommandInvariantTests` |
| D3 | The CLI's feedback survey was not switched off in the agent's environment | `pixel_agent.PIXEL_ENV` (`CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY=1`) in the preflight children (native and route), in `PixelCLI.env` (native and route) and in the probe; the proof records `agent_env` and a proof for another environment does not cover the run; checked again before every launch. AG-02's `QUIET_ENV`, environment and proofs are unchanged | `AgentEnvironmentTests`, `OnDiskEvidenceTests.test_a_proof_for_another_agent_environment_does_not_cover_the_run` |
| D4 | The MCP debug log outside the configuration directory is not covered by `CLAUDE_CONFIG_DIR` | Recorded (ADR-0038 decision 5 amendment, README, §9.12.4, CHANGELOG 1.0.111): location, not deleted by the harness, no image on synthetic sessions, **real-screen content not measured**; the preflight scans what a session adds to it. Deleting it is a retention decision, not made here | `test_image_data_in_the_cli_cache_log_refuses_the_proof`, `test_a_cache_log_without_an_image_passes_and_is_counted` |

Red-first: the tests were written and run against `21e93bf` (main after PR 104): 22 tests, 10 failures and 12 errors on Linux
(the errors are tests of the new module and new names); output kept beside the task prompt (`step5fix_red\disk\`). The
preflight tests start a fake CLI as a real child process, so they were run on Linux (WSL), where the owned tree is a process
group; on Windows they would create a job object and were not run there.

## Not measured / limits

* One CLI version (2.1.304 for the measurement), Windows, drawn PNGs. The macOS and Linux cache locations follow the CLI's
  `env-paths` convention and are not measured.
* The content of the MCP debug log with a real screen, and anything another Claude Code process or the operating system keeps
  (crash dumps, search indexing, backups), are not measured.
* Images the CLI re-encodes (2560×1440 → 2000×1125) are covered by signature, not by exact match.

## Claude Code 2.1.305 (auto-updated on 2026-10-20)

Every proof must be regenerated with the version used before a live series (O11). With 2.1.305 the native AG-03 preflight
(real CLI, fresh configuration directory) **fails with `pixel_tools_not_the_three`**, with or without this change: every
request's tool list now carries a `DeferredToolPlaceholder` entry next to the three pixel tools (init still lists exactly the
three). The disk checks of this change pass on 2.1.305 (config files touched 2/1/2, cache files 3, no image data). The verdict
is the owner's rule and is **not** changed here: it needs a decision (accept the placeholder with a positive control, or pin the
CLI to the version the proofs were made with). AG-02's preflight was not run (it needs the engine binary).
