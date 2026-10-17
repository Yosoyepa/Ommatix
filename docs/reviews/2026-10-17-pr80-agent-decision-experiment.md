# PR #80 review: the offline agent-decision experiment (AG-01)

An independent read-only review (Codex, extra-high effort) of `a11f8f2` found two P1 and four
P2 defects in `tools/agent/subq_decisions.py` and `src/acceptance/examples/agent_scenes.rs`.
All six were reproduced by the reviewer with synthetic canaries and mocks, without a network.
Every finding below has a regression test that failed on the code that was reviewed and passes
now. The architect decided finding 1 (the tool generates its own scenes). Nothing was sent to the
API at any point of this work.

The first recorded results of the experiment were withdrawn (the dialog oracle of finding 6 biased
them); the architect re-runs the experiment after these fixes.

## 1. P1: the synthetic-only boundary accepted arbitrary screen content

**Finding.** The tool trusted two strings supplied by its input (`"synthetic": true`,
`"generator": "agent_scenes"`). A wrapper file with a live `observe_screen` element list and those
two markers was accepted, and its labels reached the request.

**Resolution (architect's decision).** The tool no longer accepts a scenes file; `--scenes` is
removed. It runs `cargo run --locked --quiet -p ommatix-acceptance --example agent_scenes -- --out
<fresh temporary directory outside the checkout>` from the repository root it lives in (resolved
from `__file__`) and reads only that output. `--scenes-out PATH` (outside the checkout) only saves a
copy and is never read. The markers stay as a shape check, not as proof. Tests inject the generator
through a function parameter (`generate=`), never through a command-line option. ADR-0033
(decision 2), chapter 09 §9.10, the AGENTS.md lines and the tool's help say so.

**Regression tests** (`GeneratedScenes` in `tools/agent/test_subq_decisions.py`):
`test_there_is_no_option_that_hands_the_tool_a_scenes_file` (it failed once while writing the fix:
`--scenes` was accepted as an unambiguous abbreviation of `--scenes-out` and exited 0, so the parser
now has `allow_abbrev=False`), `test_the_request_only_carries_what_the_generator_wrote`,
`test_the_generator_command_is_fixed_and_locked`,
`test_the_generator_runs_from_the_checkout_the_tool_lives_in`,
`test_a_failing_generator_is_a_fixed_refusal`,
`test_the_scenes_are_generated_outside_the_checkout_and_removed`, `test_scenes_out_saves_a_copy_of_what_was_generated`,
`test_scenes_out_inside_the_checkout_is_refused_before_anything_runs`,
`test_scenes_out_is_never_read_back`. The old interface does not exist any more, so the canary
reproduction of the review (a marked file with a private label) is expressed as the absence of any
option that takes a file.

## 2. P1: billed malformed replies did not consume the budget

**Finding.** `used` advanced only when every answer parsed. An HTTP 200 reply with a valid
`usage.input_tokens` and unreadable answers authorised the next request against an unchanged
balance; a negative usage raised the balance; the error counter did not bound the spend.

**Resolution.** Usage is read by `billed_tokens` independently of the answers and added to
`RunState.used` for every HTTP 200 reply before the next request is decided. If the count is
missing, not an integer (a boolean or a float is not one) or negative, the run stops, records
`usage_unreadable` and says "the charge is unknown". The next-request check uses the real
balance, so a billed failure cannot authorise a request beyond the budget.

**Regression tests** (`BilledBudget`): `test_a_billed_reply_without_answers_still_consumes_the_budget`
(a 489-token style case: budget equal to the first estimate, billed reply with `answers = {}`,
second request must not be sent), `test_the_total_billed_is_the_sum_of_every_200_reply`,
`test_unreadable_usage_stops_the_run_conservatively` (missing, empty, string, float, boolean, null and
negative), `test_a_negative_usage_never_raises_the_remaining_balance`,
`test_billed_failures_between_good_answers_cannot_pass_the_budget`.

## 3. P2: a malformed key could be printed in a header exception

**Finding.** `read_key` stripped only the ends. A key file with an internal newline reached the
`Authorization` header; Python's header validation raised a `ValueError` whose text holds the whole
header value, uncaught, so the traceback printed the key.

**Resolution.** `validate_key` accepts only printable ASCII without spaces, 8 to 512 characters, and
refuses with a fixed message (the key is not shown) before any header is built. `post` converts a
`ValueError`/`UnicodeError` into the fixed refusal "the request could not be built" and raises with
`from None`, so no context carries the header. `main` catches any other exception and prints only its
type name; the report is still written.

**Regression tests** (`KeyHandling`):
`test_a_key_with_whitespace_or_control_characters_is_refused_with_a_fixed_message`,
`test_a_key_file_with_a_nul_is_refused_without_a_traceback`,
`test_a_header_failure_becomes_a_fixed_refusal_without_the_key`,
`test_an_unexpected_exception_prints_only_its_type`,
`test_the_key_is_valid_if_it_is_printable_ascii_of_sane_length`.

## 4. P2: the default redirect handler forwarded the bearer key

**Finding.** `urllib` follows a redirect and keeps `Authorization`, so a cross-origin 3xx (or an
https-to-http one) would send the key elsewhere.

**Resolution.** `post` uses an opener whose redirect handler never builds a follow-up request; a 3xx
reply is returned and the run stops ("a redirect is never followed with the key"). The base URL must
be `https`, except `http` to `127.0.0.1` or `localhost` (the tests' fake server); user information,
queries and fragments are refused. The command line enforces it with `--base-url`.

**Regression tests** (`Redirects`): `test_a_redirect_is_a_fatal_stop_and_the_target_gets_nothing`
(301, 302, 303, 307, 308; the second server receives no request),
`test_the_base_url_must_be_https_unless_it_is_loopback`,
`test_the_command_line_refuses_a_plain_http_remote`.

## 5. P2: a later transport failure erased completed results and paid usage

**Finding.** `post` could raise before `run` returned, so `main` wrote the initial empty results and
zero usage instead of what had been paid for.

**Resolution.** `run` records into a `RunState` owned by `main` and catches a transport refusal
(including the retry's) itself; `main` writes the report from that state however the run ends,
including an unexpected exception.

**Regression tests** (`PartialResults`): `test_a_failure_on_request_two_keeps_request_one`,
`test_a_failure_on_the_retry_keeps_the_earlier_answers`.

## 6. P2: the dialog scene marked an achievable save goal as impossible

**Finding.** In `form_with_confirmation_dialog`, "Save the form." was expected `none`, but the
simulator saves when OK is clicked (`SimState::confirm`), so choosing OK was counted as a wrong
action accepted by the gate and abstaining was rewarded. No test checked the behavioural truth of
the `none` goals.

**Resolution.** "Save the form." now expects `(button, OK)`. Every goal of the simulator's scenes
carries the effect that defines it (`Effect`: focus the field, clear the text, open the dialog,
confirm a save, dismiss without saving, or beyond what the simulator models), and a test replays the
scene's steps in `SimState`, clicks each element on screen and requires that exactly the expected
element achieves the effect and that no element achieves it when the goal is marked impossible. All
`none` goals were reviewed against `state.rs`. Three goals changed their wording so that one click
defines them: "Start saving the form." (opens the confirmation; a save needs a second click on OK),
"Clear the text in the form." and "Put the cursor in the Name field." replace goals that depended on
state the scene did not have. The dialog scene now has typed text in it, so "clear the text" is
observable and Cancel is disabled there. The scenes now have 25 goals. Fixture scenes (`injection`,
`only_disabled_match`) have no behaviour and keep their declared truth.

**Regression test** (`agent_scenes` example): `ag_01_every_goal_is_behaviourally_true_in_the_simulator`.
It failed before the fix with `form_with_confirmation_dialog: "Save the form." is marked impossible
but [("button", "OK")] achieves it`.

## Checks

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and the Windows-feature clippy,
`cargo test --workspace`, `python -m unittest discover` for `tools/s4`, `tools/perf`, `tools/repro`
and `tools/agent` (58 tests, of which 14 failed on the reviewed code before the fixes), and the
thesis checks. The real generator was run through the tool's `--plan` (25 requests, about 12.9
thousand estimated input tokens); the API was not called.

# Round 2: the re-review of `73a073e`

A second read-only review (Codex, extra-high effort) confirmed that findings 2, 3 (ordinary paths), 4
(redirects), 5 (ordinary paths) and 6 were fixed, and reported one P1, two P2 and one P3, plus a
conditional billing note and a wording inconsistency. It also recomputed both result summaries of
chapter 09 §9.10 from the raw files and found them exact. Every item below has a regression test that
failed on `73a073e` and passes now. The recorded results stay valid: the SHA-256 of the scenes the
fixed tool generates equals that of the scenes of the first valid run
(`c2cdafd6e4aab2cfc6bdf3d8570d3c2961eb5959ec1700ed2b2a09e34a4b43e4`).

## R2-1. P1: inherited Cargo configuration still substituted the generator

**Finding.** The subprocess inherited the caller's environment, resolved a bare `cargo` from `PATH`
and used Cargo's runners and target directory. A `CARGO_TARGET_<triple>_RUNNER`, a shadowing
`cargo` or a planted `agent_scenes` artefact could write a well-shaped scene with real labels, and
those labels became the next request. Fixing the working directory and `--locked` authenticates
nothing.

**Resolution (architect's decision).** The scenes are deterministic, so their output is pinned. The
SHA-256 of the canonical bytes is a constant in the tool (`PINNED_SCENES_SHA256`) and in a test of the
generator (`ag_01_the_scenes_match_the_pinned_hash`); the tool hashes what it generated before
anything reads it, and refuses (no request built) unless it matches, the same idea as ADR-0021. A
change to the generator that does not update both constants fails CI. Additionally `cargo` runs with
`--frozen` (locked and offline), resolved once with `shutil.which` to an absolute path, and with a
minimal environment (an allow-list: `PATH`, system and user directories, `CARGO_HOME`, `RUSTUP_HOME`,
temporary directories); every runner, wrapper, flag and target-directory override is dropped. A private
target directory is not needed once the output is pinned. ADR-0033 states the threat model: the pin
prevents the tool from sending anything but the canonical synthetic scenes, and a compromised
operator machine is out of scope.

**Regression tests** (`ProductionProvenance`, production generation path, no test seam; 6 of them
failed on `73a073e`): `test_a_shadowed_cargo_that_writes_a_canary_scene_is_refused_and_no_request_is_built`
(a fake `cargo` first on `PATH` writes a well-shaped scene with a canary label; `build_request` and
`post` are guarded to fail if reached),
`test_a_canary_runner_and_other_cargo_overrides_never_reach_the_subprocess` (a fake `cargo` records the
names of the variables it received while the runner, wrapper, flags and target-directory variables are
set; none arrives),
`test_a_planted_artifact_with_a_well_shaped_scene_is_refused_by_the_pin`,
`test_cargo_is_resolved_to_an_absolute_path_with_a_minimal_environment`,
`test_without_cargo_the_tool_refuses_with_a_fixed_message`,
`test_the_pin_in_the_tool_is_the_pin_the_generator_test_asserts`. The Rust test
`ag_01_the_scenes_match_the_pinned_hash` pins the same value on the generator side.

## R2-2. P2: a proxy could receive the bearer key in clear text

**Finding.** The default opener honours proxy variables, so with `http_proxy` set, a base URL of
`http://127.0.0.1:8000` was sent to the proxy, key included.

**Resolution.** The opener uses `ProxyHandler({})` for every request: the connection goes to the base
URL's host and nowhere else.

**Regression test** (`Proxies`): `test_the_destination_is_the_base_url_host_even_with_proxy_variables`
(proxy variables set; the connection class is replaced by one that records its destination and raises
before any socket; the destination must be `127.0.0.1:8123`).

## R2-3. P2: an interruption discarded completed, paid results

**Finding.** `KeyboardInterrupt` is not an `Exception`, so Ctrl+C during the second request left no
report.

**Resolution.** `main` catches it, finalises the report from the accumulated `RunState` with
`"status": "interrupted"` (the report now carries `status`: `complete`, `stopped` or `interrupted`)
and exits 130. An interrupt before any request was sent writes nothing and exits 130.

**Regression tests** (`Interruption`): `test_an_interrupt_on_the_second_request_writes_the_first_result`
(the interrupt escaped on `73a073e`), `test_a_finished_run_says_it_is_complete`,
`test_a_run_that_stops_says_so`.

## R2-4. P3: `--plan` could permit network access through Cargo

**Finding.** `--plan` generated the scenes with `--locked`, which does not stop a download on an
incomplete dependency cache, although it advertises no network.

**Resolution.** The generator always runs with `--frozen` (locked and offline), so `"network": false`
holds by construction.

**Regression tests** (`Offline`): `test_the_generator_runs_frozen_so_planning_cannot_touch_the_network`,
`test_plan_reports_no_network_by_construction`.

## R2-5. Conditional: a charge reported on a 429 or 503 was discarded

**Finding.** The accounting assumed that non-200 replies are unbilled. A 429 carrying a valid
`usage.input_tokens` was dropped before the retry, so a retry could pass the budget. The review did not
establish that SubQ bills such replies.

**Resolution.** A valid charge reported on any reply counts toward `used`, and a retry is sent only if
its estimate still fits the remainder.

**Regression tests** (`ChargeOnAnyReply`): `test_a_billed_429_counts_and_can_forbid_the_retry`,
`test_a_billed_503_counts_toward_the_total` (both failed on `73a073e`),
`test_a_429_without_usage_is_still_retried`.

## R2-6. Wording

The changelog said that the six defects were corrected before anything was executed against the API,
while two earlier executions had been discarded. It now says "before the valid runs".

## Checks (round 2)

`cargo fmt`, both `clippy` runs, `cargo test --workspace` (the example has 10 tests), the Python
suites of `tools/s4`, `tools/perf`, `tools/repro` and `tools/agent` (73 tests), `run_tests.py` and the
validator. `--plan` through the production path (`--frozen`, minimal environment, pin) gives 25
requests and about 12.9 thousand estimated tokens. The API was not called.
