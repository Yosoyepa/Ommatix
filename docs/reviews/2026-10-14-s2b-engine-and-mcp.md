# S2b review: engine use case and MCP adapter (2026-10-14)

Code review of the S2b stage (`ommatix-application` engine and `ommatix-adapter-mcp`) at
`96330a4`, against thesis chapter 07 (MCP contract and errors), chapter 02 §2.6–2.9
(SDD: identity, transactional publication, coordinates, execution and verification) and
chapter 08 (security and privacy). Each finding was reproduced with a test that failed
before its fix; each fix ships with that test (AGENTS.md rule 10).

## Findings and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Blocker | The live action gate ignored `LocalPolicy::allowed_applications`: live input was delivered with an empty allowlist and to any foreground application (chapter 08, fail-closed actuation) | Live input needs a non-empty allowlist **and** an actuator that attests the foreground application on it. New default port method `OsActuator::foreground_application() -> Option<String>` (`None` until S3 adds a foreground-window source, so live input stays denied until then). Otherwise `POLICY_DENIED` / `operator_review` | `live_input_is_denied_when_no_application_is_allowlisted`, `live_input_is_denied_when_the_foreground_application_is_unknown` |
| 2 | Major | `effect_observed` compared the post-delivery frame with the committed base, so a change that happened before delivery (unrelated to the action) counted as its effect | Freshness check before delivery (SDD §2.9): a frame captured right before delivery that differs from the committed base rejects the action with `STALE_SNAPSHOT` / `observe_again` and delivers nothing; the visual effect is measured against that pre-delivery frame | `a_change_before_delivery_is_not_an_observed_effect`, `visual_change_is_measured_against_the_pre_delivery_frame` |
| 3 | Major | After a delivery the snapshot stayed actionable, so a second action (new `request_id`) was gated on evidence the first action may have invalidated | Any `deliver` call, whatever its outcome, spends the snapshot until the next commit: further actions get `STALE_SNAPSHOT` / `observe_again`; journal replays still return the stored result | `a_second_action_after_delivery_needs_a_new_observation` |
| 4 | Major | The `observe_screen` cursor bound the requested `include_values`, but the page uses the effective value (request **and** policy): a policy change between pages silently changed the filtered view the cursor offsets into | The cursor binds the effective value; a mismatch is `STALE_SNAPSHOT` | `a_policy_change_between_pages_invalidates_the_cursor` |
| 5 | Major | A capture failure or a plan failure (`UNSUPPORTED_GEOMETRY`) left the previous snapshot actionable, although §2.7 blocks actions on stale evidence after any failed observation | Both paths call `record_failure` (empty dirty set: no plan is known) before returning the error | `a_capture_failure_is_retryable_and_publishes_nothing` (now also asserts actions are blocked), `an_unplannable_frame_blocks_actions` |
| 6 | Minor | A frame with `geometry_epoch` 0 was published, producing output that violates the schema (`geometry_epoch` minimum 1) | Rejected before planning with `UNSUPPORTED_GEOMETRY` / `stop`; the failure is recorded and nothing reaches the detector | `a_frame_with_epoch_zero_is_not_published` |
| 7 | Major | `sanitize_serde_message` stripped quoted values by toggling on `"`, so a value with escaped quotes (`x\" secret \"y`) was echoed in the argument error (chapter 08, data minimisation) | Fixed messages per serde error category; only a field name matching `[a-z_]{1,32}` may appear (unknown field names are agent text too) | `messages_do_not_echo_values_with_escaped_quotes` |
| 8 | Major | `MIN_MAX_MESSAGE_BYTES` = 1024 let a delivered `execute_action` result exceed the limit and be replaced by `-32603`, hiding input that reached the OS | Floor raised to 4096; a non-observation result that does not fit drops its text duplicate and `_meta` (server info, timings) first and keeps `structuredContent`, which always fits the floor | `ct_10_a_delivered_action_result_always_fits_the_minimum_limit` |
| 9 | Minor | The legacy 2025-11-25 profile answered `tools/list` and `tools/call` before `notifications/initialized` | Until `initialized`, only `ping` is served; any other request is `-32600` Invalid Request | `legacy_requests_before_initialized_are_rejected` |

## Notes

- Finding 1 makes live delivery impossible with the current actuators, none of which can
  attest the foreground application. That is the intended fail-closed state before S3; the
  S3 Windows actuator implements `foreground_application` from the foreground window.
- Finding 2's freshness check captures one extra frame per live action. Dry runs, the
  default, capture nothing.
- Existing tests that performed several live actions on one snapshot now observe before
  each one (`unverifiable_or_partial_delivery_is_indeterminate_and_never_retried`).

## Verification

`cargo fmt --all`, `cargo clippy -p ommatix-application -p ommatix-adapter-mcp
--all-targets -- -D warnings` and `cargo test -p ommatix-application -p
ommatix-adapter-mcp` pass on `fix/s2b-review`.
