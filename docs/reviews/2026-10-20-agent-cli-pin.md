# The pinned agent CLI and the tool-search switch (AG-02, AG-02R, AG-03)

Origin: the first real AG-03 isolation preflight (2026-10-20, PR 106 work) failed with `pixel_tools_not_the_three`. This file
records what was measured, the decisions of the coordinator, and for each point the resolution and the regression test.
Everything is synthetic: the real CLI against the loopback stand-in, the harness servers in their simulated/synthetic modes, no
provider, no capture, no key, no desktop read. The owner's own Claude Code, its updates, `PATH` and `~/.local` were not touched.

## What was measured

* Every request of a session carries a `DeferredToolPlaceholder` entry next to the three tools of the harness (the `init` still lists
  exactly three). Its embedded definition (read statically by the PR 106 reviewer) is a reserved, property-less placeholder
  that "keeps deferred tool loading active; never call this tool".
* It is **not specific to 2.1.305**: the byte-identical 2.1.304 binary (SHA-256 below) shows it too, although the AG-02 proofs of
  2026-10-18/19 were made with 2.1.304 without it. The behaviour follows the CLI's own configuration, not its version.
* `ENABLE_TOOL_SEARCH=false` in the agent's environment removes it: with the pinned 2.1.304 and with 2.1.305 the request tool
  lists were exactly the three pixel tools (`init_problems` empty).
* The native installer of the owner keeps `~/.local/share/claude/versions/2.1.304`; its SHA-256 is
  `019e5174d5d954585edb1a946fe280e44a973050b4897034adf0e16306f85fdd`, and so is that of the `claude.exe` of the npm package
  `@anthropic-ai/claude-code-win32-x64@2.1.304` (registry integrity `sha512-BXEQwzYyCKAWxa+CPVoq2+jl2ZcQIi8urs5iPQBcQyHOwONuwJ+InHC4nCDkWy8Nk7cMsT+2o3DHeYRxlkn/nw==`),
  downloaded with `npm pack` into a temporary directory.

## Decisions (coordinator, 2026-10-20) and resolutions

| # | Decision | Resolution | Regression tests |
|---|---|---|---|
| P1 | The exactly-three-tools verdict stays; **no exception by name** (an exception would need the owner and an ADR) | Unchanged in `check_pixel_requests` and in AG-02's route verdict | existing verdict tests; the new fake mode `deferred_placeholder` makes both preflights fail without the switch |
| P2 | `ENABLE_TOOL_SEARCH=false` in the agent environment of **both** harnesses | `rg.QUIET_ENV` gains it (the preflight children, the provider routes and AG-03's native environment already used `QUIET_ENV`); AG-02's native production environment now also gets `QUIET_ENV` (it had only the preflight child) via `ClaudeCLI.env`; `pa.AGENT_ENV = QUIET_ENV + PIXEL_ENV`; the proofs record `agent_env`; each runner refuses to launch with an environment that lacks it (`agent_environment_lacks_switches`) | `ag02/test_clipin.py::AgentEnvironmentTests`, `Ag02PreflightTests`, `ag03/test_clipin_pixel.py::PixelEnvironmentTests`, `PixelPreflightTests` |
| P3 | The CLI is **pinned**: 2.1.304, SHA-256 above, an absolute `--claude` path outside the repository, `claude` from PATH refused, SHA-256 and `--version` checked before every launch | `ag02/clipin.py` (`CLAUDE_CODE_PIN`, `check`, `launch_problems`); both preflights call `check` first and refuse with closed tokens (`claude_not_an_absolute_path`, `claude_missing`, `claude_inside_repository`, `claude_hash_differs`, `claude_version_differs`, ...); `AgentCLI.launch_problems` is called before every goal/attempt launch and before the smoke session; `probe_image` too | `ag02/test_clipin.py::CheckTests`, `Ag02LaunchTests`, `ag03/test_clipin_pixel.py::PixelLaunchTests` |
| P4 | The proof records `cli_sha256` (verifier bump) | AG-02 `VERIFIER_REVISION` 3 → 4 (routes 2 → 3), AG-03 2 → 3 (routes 1 → 2); both loaders require `cli_sha256` = the pin and the expected `agent_env`; older proofs are refused | `Ag02ProofTests`, `PixelPreflightTests.test_the_loader_needs_the_pin_facts_and_proof_problems_compare_them` |
| P5 | Proofs valid **at most 24 h** | `clipin.age_problems` (`proof_expired`, `proof_from_the_future`, `proof_date_unreadable`) inside `launch_problems`, so every launch checks it; the hash of the proof must equal the hash of the binary about to run (`cli_differs_from_proof`) | `AgeAndLaunchTests`, `PixelLaunchTests.test_nothing_is_launched_with_an_expired_proof` |
| P6 | Private install, read-only | `claude.exe` extracted from the npm package to `%LOCALAPPDATA%\Ommatix\agent-cli\2.1.304\` (hash and version verified; file and folder marked read-only by attribute only: an ACL that denies writes is the owner's call), with a `SOURCE.txt` | n/a (operator step) |

Red-first: the tests were written and run against `25079c0` (the PR 106 head the work started from): 27 tests, 4 failures and 22 errors
(the pure checks of a module that did not exist error); output beside the task prompt (`step5fix_red\pin\`). Several existing
test supports (stand-in proofs, harnesses) were extended with `cli_sha256`, `agent_env` and a `pin_support` helper that answers
for the pin where a fake CLI stands in for the 290 MB binary.

## Verification

* Real CLI (the pinned `claude.exe`, loopback stand-in, synthetic/simulated servers, no provider): the AG-03 preflight passes native and
  with `--route deepseek`; the AG-02 preflight (against the simulated engine of the current binary) passes native and with
  `--route deepseek`; with the default `claude` the AG-03 preflight refuses (`claude_not_an_absolute_path`) before launching anything.
  Each proof loads with its loader, `launch_problems` on the real binary is empty, and the same proof is `proof_expired` 25 h later.
  The owner's `~/.local/bin`, its versions folder and the pinned folder did not change.
* Not run: a real provider session, a goal run, the Windows job tests of the process harness (the Linux suite covers them).

## Limits and open points

* One binary, one platform (win32-x64). The pin does not freeze the CLI's remote-driven behaviour (the reason for the 24 h limit and
  for the request-level verdicts, which stay).
* The 24-hour limit is checked at launch, not while a series runs; a long series must be started inside the window.
* The read-only mark is a file attribute. Without the attribute (or an ACL) the folder could be written by the same user; the hash check
  before every launch is what detects a replacement.
* AG-02's recorded results (9.11.1–9.11.4) were obtained before the switch and the pin and are not changed.
