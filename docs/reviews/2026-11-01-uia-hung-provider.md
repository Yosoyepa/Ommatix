# Review: a hung UI Automation provider must not stall `observe_screen`

Found by the architect while reading `adapter-accessibility` against Windows-MCP's timeouts (2026-11-01), at `42b7104`.
Nothing here used the real screen, UI Automation, input or a provider: the tests use a fake worker that never answers.

## Finding

| # | P | Finding | Resolution | Regression tests (`src/adapters/accessibility/src/link.rs`) |
|---|---|---|---|---|
| H1 | P2 | The walk's 250 ms budget is checked only **between** UI Automation calls, and `UiaProvider::ask` waited for the worker with an unbounded `recv()`. One call into a provider that does not answer (UI Automation's own transaction timeout is 20 s) blocked the observation far beyond the budget. | The wait is `recv_timeout(budget + 500 ms)` (`Budget::reply_deadline`). On expiry a snapshot is an **empty, truncated read** (unknown, not absent): the engine keeps the visual path, leaves the regions it could not read unconfirmed and reads them again next observation. The abandoned request leaves the worker busy, so the link refuses fast (`pending`, one `try_recv`) instead of queueing behind it, starts no thread, and serves again when the worker's late reply arrives. `keyboard_focus` that is not answered is an error: `type` has no confirmation and sends no key (ADR-0018). | `a_provider_that_never_answers_does_not_stall_the_snapshot`, `a_read_after_a_timeout_refuses_fast_and_queues_nothing_behind_the_stuck_worker`, `the_link_serves_again_once_the_stuck_worker_answers`, `keyboard_focus_that_is_not_answered_is_an_error_not_a_focus`, `an_observation_with_a_provider_that_never_answers_still_yields_the_visual_elements` (the engine over the simulated desktop), `the_reply_deadline_is_the_time_budget_plus_a_margin` |

## Decisions that differ from the first reading

* **A timeout is a truncated read, not a `PortError`.** The engine ends an observation whose accessibility read fails
  (`an_accessibility_failure_publishes_nothing`, `INFERENCE_FAILED`, nothing published), so reporting the timeout as the
  port's failure would have made the stall an error instead of removing it. `AccessibleRead::truncated` is the port's
  existing way to say "not covered": no contract, schema or port change.
* **`ConnectionTimeout` / `TransactionTimeout` are not set.** `uiautomation` 0.25.1 (pinned in `Cargo.lock`) exposes
  `IUIAutomation` only (`AsRef<IUIAutomation>`), not `IUIAutomation2`; reaching it would take the `windows` crate and
  `unsafe`, which ADR-0016 does not allow in this adapter. The bounded wait is the whole fix.
* **No timeout counter in `_meta`.** The existing `accessibility_truncated` in the plan's `_meta` already reports that a
  read did not cover its regions (it does not tell a timeout from a budget cut); a dedicated counter needs a port method
  and a metrics field, so it is left out.

## Evidence

* Red first: seven tests were run against the unbounded `recv()` and four failed ("did not return within 5s"); the three
  that passed were controls (a worker that answers, a worker that ended, the deadline arithmetic). The engine-level test was added after the fix
  and not seen red on its own; with the old wait it cannot return (the same wait).
* Unit-level cost of the bounded path (release): a read that is refused while the worker is stuck costs a `try_recv`,
  about 0.2 to 0.5 µs (fifty reads in 9 to 26 µs). A first read that is not answered costs the deadline once:
  750 ms with the default budget (250 ms + 500 ms). No allocation was added to the answered path.
* Not measured: the real provider (no screen was used); a slow but answering provider keeps its previous cost.
* Open: `UiaProvider::new` still waits without a bound for the worker to start (COM initialisation); a hang there delays
  `serve` start-up, not an observation.
