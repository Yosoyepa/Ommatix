# Review: PR 112, the AG-02 proxy's latencies on a sub-millisecond clock

Independent review of PR 112 (`fix/ag02-proxy-clock`, head `1aa39f8`, against `main` at `1951633`), 2026-11-15,
on a second Windows 11 machine (Intel Core Ultra 7 155H, CPython 3.13.17), not the reference PC.
Nothing here used a screen, input, a provider or the real engine; the proxy's tests use in-process fakes.

**Verdict: the fix is correct.** `time.perf_counter` is monotonic and sub-millisecond on every platform and
CPython version, the two `time.monotonic()` calls that remain in `proxy.py` are a lock deadline (not a
measurement), and no other AG-02 or perf tool measures a latency with `time.monotonic()`: the soak and DPI
tools take their round trips from `tools/s3/mcp_driver.py`, which already uses `time.perf_counter()`, and use
`time.monotonic()` only to schedule 5-second ticks. The findings are about precision of the text and the
strength of one test.

## Findings

| # | P | Finding | Resolution | Evidence / regression test |
|---|---|---|---|---|
| R112-1 | P3 | The dossier (09 §9.11.4 (a)), CHANGELOG 1.0.116, the comment in `proxy.py` and the test docstring said that `time.monotonic()` *is* `GetTickCount64` (15.625 ms) on Windows CPython. That holds up to CPython 3.12 only: 3.13 moved `time.monotonic()` to `QueryPerformanceCounter` on Windows. On this machine, CPython 3.13.17 reports `monotonic: QueryPerformanceCounter(), resolution 1e-07`. The quantisation of the recorded AG-02 latencies is still supported, by the reference machine's own `get_clock_info` check and by the records themselves (`execute_action` medians of 0, 15, 16 and 31 ms). | The four texts now attribute the 15.6 ms resolution to CPython 3.12 and earlier on Windows and say that 3.13 changed it, so the defect depended on the Python that ran the proxy. | `time.get_clock_info('monotonic')` on CPython 3.13.17, Windows 11 (above). Documentation only. |
| R112-2 | P3 | `ClockTests.test_the_latency_clock_is_monotonic_with_sub_millisecond_resolution` checked only the properties of the clock named by `proxy.CLOCK`. With `CLOCK = "monotonic"` those properties hold on Linux (CI) and on Windows with CPython 3.13 or later, so a return to the old clock would have passed everywhere except Windows with CPython 3.12 or older — the one configuration where it matters. | The test also pins the name: `proxy.CLOCK == "perf_counter"`. | Red/green shown with the reverted clock on CPython 3.13.17: the original assertions **pass** with `time.monotonic`, the pinned test **fails** (scratch script, not committed). `test_proxy`: 33 tests OK (1 skipped) after the change. |
| R112-3 | P3 | The reading of the slow mode (09 §9.11.4 (b)) said each observation with work makes one inference "por `adaptive_min_regions = 2`" as a fact of the AG-02 runs. The AG-02 configurations (`phase2.toml` and the phase 1 one) are the owner's and are not in the repository; 2 is the default of `[engine] adaptive_min_regions` (`src/app/src/config.rs`, pinned by the config tests). | The sentence now names 2 as the default that the reading assumes in the AG-02 configurations, which are not in the repository. The reading stays a reading, as the PR already says. | Documentation only. |

## Checked and not a finding

* The second `ClockTests` case feeds `_clock` an iterator of five ticks; it consumes them in the order the
  proxy calls the clock (t0, request log, request sent, reply received, reply log) and expects 1.2 ms. A
  revert to `time.monotonic()` in `pump_engine` would not consume the fake ticks and the latency would not be
  1.2 ms, so the case is a real guard of *where* the latency is taken.
* The isolation proofs of ADR-0036 and ADR-0037 do not hash `proxy.py` (they check the CLI, its configuration
  and the requests at the stand-in), as the PR says; no proof needs renewing.
* The slow-mode reading matches S0 §9.3–§9.4 (2 605 ms median per CPU inference, 2 637–2 638 ms of `detect` in
  2 674–2 686 ms observations) and 9.11.1/9.11.2 record the CPU detector for both phases.

## Checks run on the branch after the resolution

`python -m unittest test_proxy` (from `tools/agent/ag02`): 33 tests OK, 1 skipped. The thesis oracle and
validator: see the PR.
