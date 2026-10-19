# AG-02 multi-model: probe findings and design hazards (ADR-0037)

Record of what was found while extending AG-02 to other providers' models (branch
`feat/ag02-multi-model`, built on `c835ba5`/`f98e810`): each finding, its resolution and the test
that covers it. ADR-0037 is the decision; this file is the evidence. No box, requirement status or
engine behaviour changes. Nothing here involved a real provider: every request was sent to a stand-in
on 127.0.0.1 (`fake_api.py`) with a dummy key. All tests are in `tools/agent/ag02/test_multimodel.py`
unless a path says otherwise.

**Probed versions.** Codex CLI 0.163.0 (`gpt-6-luna` from its bundled model catalog) and OpenCode
2.0.12 on Windows 11, 2026-10-18, each with an isolated home, the Ommatix budget proxy in front of
`ommatix serve --source sim` as the only MCP server, and the strictest configuration found.

## F1 (P1). Codex cannot be confined to the three tools

Finding: with every switch that exists (`shell_tool`, `unified_exec`, `multi_agent`, `goals` off;
`web_search` disabled; `tools.update_plan` and `tools.experimental_request_user_input` disabled; a
model-catalog entry with `apply_patch_tool_type`, `multi_agent_version`, `tool_mode` and image input
removed), the request still offers `view_image`, `list_mcp_resources`, `list_mcp_resource_templates`,
`read_mcp_resource` and a `tool_search` behind which the Ommatix tools are deferred. `view_image` is
not a feature flag (`--disable view_image` and `features.view_image` are unknown to the CLI), not a
`[tools]` key, and a text-only `input_modalities` does not remove it.
Resolution: the CLI is a probe only. `preflight_cli.py --agent-cli codex` reads the whole request
(`tools` and `additional_tools` input items, namespaces and code-mode `exec` declarations expanded)
and reports `tool_surface_wider_than_three` and `ommatix_tools_not_offered_directly`; the runner
refuses `--agent-cli codex` before anything starts.
Regression: `ProbeTests.test_codex_with_the_strictest_configuration_still_offers_view_image_and_a_tool_search`,
`ProbeTests.test_codex_code_mode_and_namespaces_are_expanded_into_the_surface`,
`ProbeTests.test_a_codex_request_with_exactly_the_three_tools_and_no_skills_would_pass` (the verdict is
not a blanket refusal), `RunnerIntegrationTests.test_codex_and_opencode_cannot_run_a_goal`.

## F2 (P1). Codex reads skills from the real home

Finding: the request lists the operator's `~/.agents/skills` (and the bundled system skills) although
`CODEX_HOME` is an empty directory; `HOME` and `USERPROFILE` are ignored on Windows, so redirecting
them is not a boundary. `skills.enabled`, `features.skills` and `--enable skip_host_skill_discovery`
change nothing.
Resolution: `codex_has_skills` finds a `<skills_instructions>` block with entries in the request and
the verdict is `skills_from_real_home`.
Regression: the two Codex verdict tests above.

## F3 (P1). OpenCode reaches the MCP tools only through a code runtime with `fetch`

Finding: in 2.0.12 the MCP server connects but its tools never appear as direct tools; they are
callable only inside `execute`, a JavaScript runtime whose own description says "`fetch` is available
for HTTP requests". Denying every built-in action leaves `execute` as the only tool, and `fetch` stays.
Resolution: `opencode_surface` reports `execute` and `execute+fetch`; neither is accepted as inert.
Regression: `ProbeTests.test_opencode_reaches_the_mcp_tools_only_through_a_code_runtime_with_fetch`.

## F4 (P1). OpenCode lists the operator's skills and extra tool namespaces

Finding: the system prompt lists the operator's own skills from the real home until the `skill` action
is denied; the runtime's catalog also lists a desktop `browser` namespace (45 tools) and an `opencode`
namespace unless each is denied by name (`permission: {"browser": "deny"}` removes the first; top-level
`policies` and `experimental.policies` had no effect); the title-generation request sends the goal
prompt to `small_model`.
Resolution: `opencode_has_skills` and the catalog namespaces (`catalog:<name>`) enter the verdict; the
probe's configuration denies by default and allows only `execute` and `ommatix`.
Regression: the OpenCode verdict test above, `ProbeTests.test_no_skills_block_means_no_skills_finding`,
`ProbeTests.test_the_builders_make_the_strictest_configuration_found`.

## F5 (P2). The OpenCode CLI starts a background service

Finding: `opencode debug paths`, `debug config` and similar start `opencode serve --service`, which keeps
running after the command, writes a `service.json` with a password into the (isolated) home and holds
a listening socket.
Resolution: the probe runs `opencode run --standalone` only and the process tree is killed on a
timeout by pid (never by image name, which would also kill the operator's own OpenCode). The ADR
tells the owner a stray service may remain after the author's probe (it was stopped).
Regression: none in code (process hygiene, documented in ADR-0037).

## F6 (P1). Redirecting the home or the environment is not an isolation boundary

Finding (F2, F4): the CLIs read the real home through the operating system, not through the
environment.
Resolution: the minimal agent reads nothing from any home, instruction file, skill or environment
variable; its child environment is the operating system's basics only; the preflight proves it with
sentinels in the working directory, its ancestors, a stand-in home and the parent environment
(provider keys, a proxy).
Regression: `EndToEndTests.test_nothing_of_the_operators_environment_reaches_the_child`,
`PreflightAndProofTests.test_the_preflight_passes_on_the_real_agent_and_its_control_sees_every_deviation`,
`PreflightAndProofTests.test_a_leaky_agent_fails_the_preflight`.

## F7 (P1). A loop of our own is code: the proof must bind it

Finding: unlike a vendor binary, `minimal_agent.py` can change in the same PR as the goals.
Resolution: the proof binds the agent's source SHA-256, version, provider, production endpoint, auth
style, model, request parameters and instruction; the runner recomputes them and refuses a run that
differs, and every session's `init` is compared with the proof. Editing the agent invalidates every
proof.
Regression: `PreflightAndProofTests.test_the_proof_binds_the_agent_the_provider_the_model_the_parameters_and_the_instruction`,
`...test_the_loader_accepts_a_passing_proof_and_refuses_the_rest`,
`...test_the_session_is_compared_with_the_proof_at_init`,
`PreflightAndProofTests.test_an_agent_that_adds_a_tool_or_a_body_key_fails_the_preflight`.

## F8 (P1). The verifier must be shown to see

Finding: a request verifier that never fails proves nothing (the same lesson as ADR-0036's
positive control).
Resolution: the preflight feeds the verifier seven broken copies of a real request (extra tool,
dropped tool, sentinel, image part, extra key, other model, other instruction) and fails with
`control_blind` if any goes unflagged; the verifier also checks headers, the path, the key's single
place, the history and each message's shape.
Regression: `VerifierTests.test_each_kind_of_deviation_is_seen`,
`VerifierTests.test_headers_paths_keys_and_history_are_checked_too`,
`VerifierTests.test_a_key_in_a_body_and_a_foreign_tool_call_are_seen`,
`VerifierTests.test_a_real_request_sequence_passes_and_yields_the_facts_the_proof_binds`.

## F9 (P1). A provider error body or a redirect could carry input out or elsewhere

Finding: providers echo input in error bodies, and a redirect would send the observations to another
host; urllib follows redirects and honours proxy variables by default.
Resolution: an HTTP error becomes a code (`http_500`) and nothing else reaches the stream or stderr; no
redirect handler follows a redirect (`http_302` is an error and one request was made); the opener
ignores proxy variables; a non-loopback endpoint must be https and may carry no credentials, query or
fragment.
Regression: `HostileEndpointTests` (three tests), `EndpointAndParameterTests.test_only_https_or_a_loopback_http_endpoint_is_accepted`.

## F10 (P1). Nothing but text may reach the model, and no tool beyond the three

Finding: an MCP result could carry an image block; a model could call a tool that is not offered; a
server could list a fourth tool.
Resolution: non-text blocks are replaced by a fixed note; a tool name outside the three is answered
with an error and never forwarded to the engine; a server listing anything but the three tools makes
`init` list them, the runner ends the session (`tools_differ`) and no request is sent.
Regression: `EndpointAndParameterTests.test_only_text_reaches_the_model_an_image_or_other_block_is_replaced_by_a_fixed_note`,
`EndToEndTests.test_a_tool_the_model_invents_is_never_forwarded_to_the_engine`,
`EndToEndTests.test_a_server_that_lists_a_fourth_tool_ends_the_session_before_any_request`.

## F11 (P2). Keys

Finding: a key in an argument, environment variable, log or error would leak.
Resolution: the key comes from a file outside the checkout (`--key-file`, validated like the other
paths), is sent in one header (`Authorization: Bearer` for DeepSeek, `api-key` for Xiaomi) and is
never printed; a malformed key file is refused without naming its content.
Regression: `EndToEndTests.test_the_provider_decides_the_auth_header_and_the_key_is_in_that_header_only`,
`EndpointAndParameterTests.test_a_malformed_key_file_is_refused_without_naming_its_content`,
`RunnerIntegrationTests.test_a_goal_runs_end_to_end_through_the_runner_with_the_minimal_agent_and_its_provenance`.

## F12 (P2). The runner must stay one runner

Finding: the agent-specific parts of `run_goal.py` (command, environment, proof, provenance) were
inlined for Claude Code.
Resolution: an `AgentCLI` interface (`agentcli.py`); Claude's behaviour is unchanged (the command is
byte-for-byte `build_claude_argv`, the proof and init comparisons are the same functions); the stream
reader reads `cli_version` as well as `claude_code_version`, and results gain `cli`, `provider`, and
for the minimal agent the agent's source hash, the endpoint's host and the parameter names;
`summarise.py` groups by `<agent>:<provider>/<model>` and renders one block per group.
Regression: the whole existing suite (`test_run_goal.py` with the provenance expectation updated),
`RunnerIntegrationTests.test_claude_stays_the_default_and_its_command_is_unchanged`,
`SummaryByModelTests` (two tests).

## Not covered

- The minimal agent has only met our stand-in. Provider differences (MiMo's effort parameter and model
  id, `reasoning_content` handling, rate limits, tool-call argument spelling) will show on the first
  real dry run and would be fixed in the agent and its proof.
- The providers' data-use terms were not read (their API documentation has none); the ADR says so.
- Codex and OpenCode were probed for one version each; a later version or configuration may pass, and
  the runner would still need a stream reader for it (a new ADR revision).
- GPT-6 Luna through OpenAI's own Chat Completions API was not checked: the owner has no API key, and
  Zen routes GPT models through the Responses API only.
