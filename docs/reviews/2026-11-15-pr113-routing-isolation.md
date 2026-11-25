# Review: PR 113, the AG-02 and AG-03 tool tests without the developer's routing variables

Independent review of PR 113 (`fix/agent-tests-routing-isolation`, head `4bf89f5`, against `main` at `1951633`),
2026-11-15, on a second Windows 11 machine (Intel Core Ultra 7 155H, CPython 3.13.17), not the reference PC.
Tests only: no screen, input, provider or network beyond the tests' own loopback fakes.

**Verdict: the approach is right and the PR does what it says.** `routing_isolation.start()` snapshots
`os.environ` with `mock.patch.dict`, removes exactly the names `run_goal.routing_problems` reports (the closed
selectors and prefixes of ADR-0036), and `stop()` restores the snapshot; the children the tests start inherit the
cleaned environment. The tests of the refusal itself still set their variables inside the test, so the refusal
stays covered. Two findings concern the PR's own regression test.

## Findings

| # | P | Finding | Resolution | Evidence / regression test |
|---|---|---|---|---|
| R113-1 | P2 | `test_start_removes_every_routing_variable_and_stop_puts_them_back` asserted that, after `stop()`, the routing variables are exactly the three it set itself (`sorted(ambient)`). In a developer environment that already carries another routing variable — a Claude Code session routed through Foundry (`CLAUDE_CODE_USE_FOUNDRY`, `ANTHROPIC_FOUNDRY_RESOURCE`) or with `ANTHROPIC_CUSTOM_HEADERS` — the PR's own regression test failed, which is the situation the PR exists to handle. | The expectation is read from the environment inside the test (`routing_problems` before `start()`), and the test checks that the variables it set are among them. | With `CLAUDE_CODE_USE_FOUNDRY=1` in the environment: the original test **fails** (`Lists differ: [..., 'CLAUDE_CODE_USE_FOUNDRY'] != [...]`), the new one **passes**. |
| R113-2 | P3 | "Only routing variables are removed" was checked on `PATH` alone, and the restoration only on the routing names. A `start()` that also dropped another variable (`TEMP`, `SYSTEMROOT`, `CLAUDE_CODE_GIT_BASH_PATH`, …) passed the test while changing what every child of the module inherits. | The test compares the whole environment: during isolation it equals the snapshot minus the routing names; after `stop()` it equals the snapshot exactly, values included. | A mutant of `start()` that also removes `TEMP`: the original test **passes**, the new one **fails** (scratch script, not committed). |

## Checked and not a finding

* Coverage of the modules. Six test modules drive a tool's `main` without the new fixture: `test_run_goal.py`
  calls it only with `--plan` (which starts nothing and checks no routing); `test_rereview.py`,
  `test_fourthreview.py` and `test_gate_smoke.py` already read or clear the routing variables themselves; and
  `test_preflight.py` and `test_thirdreview.py` pass with the variable set. The full suite with
  `ANTHROPIC_BASE_URL` set (below) is the check, and it is green; a guard that fails when a new module drives a
  `main` without the fixture would make the opt-in safer, but it is a suggestion, not a defect.
* Module fixtures and imported test classes. `test_prreview93` imports `EndToEndTests`, a `TestCase` of
  `test_multimodel`, so unittest also runs that class inside the `test_prreview93` suite and switches module
  fixtures around it (tear down `test_prreview93`, run the imported class without fixtures, set
  `test_prreview93` up again). `routing_isolation` pairs `start`/`stop` through a stack, so the environment is
  restored in order across those switches, and `EndToEndTests` passes with the variable set in either context.
* `agentcli.py` requires a route's child environment to carry exactly `ANTHROPIC_BASE_URL`; with the developer's
  routing variables removed by the module fixture, a route test sees only the variable it sets itself.

## Checks run on the branch

* The problem, reproduced on `main`: `python -m unittest test_routecost` gives 2 errors with
  `ANTHROPIC_BASE_URL=http://127.0.0.1:9` in the environment and passes without it.
* The PR head (`4bf89f5`) with `ANTHROPIC_BASE_URL=http://127.0.0.1:9` in the environment:
  `python -m unittest discover -s tools/agent -p "test_*.py"` ran **1 160 tests, OK (9 skipped)** in 975 s. The
  worktree had no `target\debug\ommatix.exe`, so `test_rehearsal.SimulatedDesktopRehearsal` skipped; in the main
  checkout of this machine those two tests fail on `main` too, with or without the variable, because Windows
  Smart App Control blocks the locally built binary they launch (`WinError 4551`, Code Integrity events 3033
  and 3077): a limit of this machine, not of the PR.
* After the resolution: `test_routing_isolation` (2 tests) OK; `test_routing_isolation` and `test_routecost`
  (14 tests) OK with `CLAUDE_CODE_USE_FOUNDRY=1` and `ANTHROPIC_BASE_URL` both set.
