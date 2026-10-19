"""Tests of the AG-02 multi-model extension (ADR-0037): the AgentCLI interface, the minimal agent, its
preflight and proof, the fail-closed probes for Codex and OpenCode, and the per-model summary.

No network beyond 127.0.0.1, no real provider, no Windows, no real CLI: the agent runs against the
stand-in of ``fake_api.py`` and a fake engine behind the real budget proxy."""

import argparse
import contextlib
import io
import hashlib
import http.server
import json
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import agentcli  # noqa: E402
import minimal_agent as ma  # noqa: E402
import preflight_cli as pc  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402
import summarise  # noqa: E402
from fake_api import FakeApi  # noqa: E402

GOALS = rg.load_goals()
INSTRUCTION = GOALS["system_instruction"]
PROMPT = rg.build_prompt({"prompt": "Say hi."}, 1)
FAKE_ENGINE = HERE / "fake_engine.py"
THREE = sorted(rg.ALLOWED_TOOL_NAMES)


class AgentRun:
    """One run of ``minimal_agent.py`` through the real proxy and a fake engine, against FakeApi."""

    def __init__(self, test, script, *, extra=None, provider="mimo", model="mimo-test", engine_extra=(),
                 key="sk-test-KEY-123456", max_turns=None, max_tokens=None, api=None, base_path=None,
                 phase=1, price=None, max_usd=None, ceiling=None, real_schemas=False):
        self.test = test
        tmp = tempfile.TemporaryDirectory(prefix="ag02-minimal-")
        test.addCleanup(tmp.cleanup)
        self.tmp = Path(tmp.name)
        self.key = key
        key_file = self.tmp / "k.key"
        key_file.write_text(key + "\n", encoding="utf-8")
        self.received = self.tmp / "engine.in"
        self.log = self.tmp / "proxy.jsonl"
        engine = [sys.executable, str(FAKE_ENGINE), "--received", str(self.received), *engine_extra,
                  *(["--real-schemas"] if real_schemas else [])]
        mcp = self.tmp / "mcp.json"
        mcp.write_text(json.dumps(rg.build_mcp_config(
            sys.executable, ["--dry-run-only", "--log", str(self.log)] if phase == 1 else
            ["--max-live", "5", "--ledger", str(self.tmp / "ledger.json"), "--goal-id", "G01",
             "--max-live-goal", "5", "--log", str(self.log)], engine)), encoding="utf-8")
        with contextlib.ExitStack() as stack:
            api = api or stack.enter_context(FakeApi(script))
            self.api = api
            base = api.url if base_path is None else api.url + base_path
            ns = argparse.Namespace(provider=provider, base_url=base, key_file=key_file, effort=None,
                                    extra_param=extra, max_total_tokens=max_tokens or 400_000, price=price,
                                    phase=phase, completion_ceiling=ceiling, max_usd_explicit=max_usd is not None)
            cli = agentcli.MinimalCLI(ns)
            argv = cli.build_argv(model=model, mcp_config=mcp, system_instruction=INSTRUCTION,
                                  max_budget_usd=max_usd if max_usd is not None else 1)
            if max_turns is not None:
                argv += ["--max-turns", str(max_turns)]
            self.argv = argv
            self.metrics = rg.StreamMetrics(GOALS["vocabulary"]["testapp"], None, None)
            self.stderr = self.tmp / "stderr.txt"
            self.ran = rg.run_claude(argv, PROMPT, self.tmp, self.metrics, 60, self.stderr, env=cli.env())
            self.requests = list(api.requests)

    @property
    def forwarded(self):
        """The tools/call lines the fake engine really received (what got past the proxy)."""
        if not self.received.exists():
            return []
        return [json.loads(line) for line in self.received.read_text(encoding="utf-8").splitlines()
                if '"tools/call"' in line]

    @property
    def result(self):
        return self.metrics.result or {}


def bodies(run):
    return [json.loads(r["body"]) for r in run.requests]


class EndpointAndParameterTests(unittest.TestCase):
    def test_only_https_or_a_loopback_http_endpoint_is_accepted(self):
        self.assertIsNone(ma.endpoint_problem("https://api.deepseek.com"))
        self.assertIsNone(ma.endpoint_problem("https://api.xiaomimimo.com/v1"))
        self.assertIsNone(ma.endpoint_problem("http://127.0.0.1:5000/v1"))
        for bad in ("http://api.deepseek.com", "ftp://x", "https://user:pw@x.com", "https://x.com/v1?a=1",
                    "https://x.com/#f", "file:///etc/passwd", "http://127.0.0.1.evil.com/"):
            self.assertIsNotNone(ma.endpoint_problem(bad), bad)

    def test_the_extra_parameters_are_a_closed_list_and_cannot_touch_messages_tools_or_model(self):
        self.assertIsNone(ma.extra_param_problem({"reasoning_effort": "max", "thinking": {"type": "enabled"}}))
        for bad in ({"messages": []}, {"tools": []}, {"model": "x"}, {"stream": True}, {"metadata": {}}, []):
            self.assertIsNotNone(ma.extra_param_problem(bad), bad)

    def test_a_malformed_key_file_is_refused_without_naming_its_content(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "k"
            for content in ("", "two\nlines\n", "ctl\x01char"):
                path.write_text(content, encoding="utf-8")
                with self.assertRaises(ma.AgentError) as caught:
                    ma.load_key(path)
                self.assertTrue(caught.exception.token.startswith("key_file_"))
                self.assertNotIn("ctl", caught.exception.token)

    def test_usage_is_split_into_fresh_and_cached_input_for_both_providers_spellings(self):
        openai_style = {"usage": {"prompt_tokens": 100, "completion_tokens": 7,
                                  "prompt_tokens_details": {"cached_tokens": 60}}}
        deepseek_style = {"usage": {"prompt_tokens": 100, "completion_tokens": 7, "prompt_cache_hit_tokens": 60,
                                    "prompt_cache_miss_tokens": 40}}
        for reply in (openai_style, deepseek_style):
            self.assertEqual(ma.usage_of(reply), {"input_tokens": 40, "output_tokens": 7,
                                                  "cache_read_input_tokens": 60, "cache_creation_input_tokens": 0})
        self.assertEqual(ma.usage_of({"usage": {"prompt_tokens": 5, "completion_tokens": 1,
                                                "prompt_cache_hit_tokens": 99}})["input_tokens"], 0)
        with self.assertRaises(ma.AgentError):
            ma.usage_of({})

    def test_only_text_reaches_the_model_an_image_or_other_block_is_replaced_by_a_fixed_note(self):
        reply = {"result": {"content": [{"type": "text", "text": "ok"},
                                        {"type": "image", "data": "AAAA", "mimeType": "image/png"}]}}
        text, error = ma.result_text(reply)
        self.assertNotIn("AAAA", text)
        self.assertIn("non_text_content_refused", text)
        self.assertFalse(error)
        text, error = ma.result_text({"result": {"structuredContent": {"a": 1}, "isError": True}})
        self.assertEqual((json.loads(text), error), ({"a": 1}, True))
        self.assertEqual(ma.result_text({"error": {"code": -1, "message": "SECRET"}}),
                         ('{"error": "rpc_error"}', True))


class EndToEndTests(unittest.TestCase):
    SCRIPT = [{"tool": "observe_screen", "args": {}}, {"text": "all good\nVERDICT: DONE"}]

    def test_the_agent_offers_exactly_the_three_tools_and_its_stream_is_the_runners_stream(self):
        run = AgentRun(self, self.SCRIPT)
        self.assertEqual((run.ran["exit_code"], run.ran["killed"]), (0, None))
        summary = run.metrics.summary()
        self.assertEqual(summary["init_problems"], [])
        self.assertEqual(summary["tools_listed"], sorted(rg.TOOLS))
        self.assertEqual(summary["tool_calls"], {"observe_screen": 1})
        self.assertEqual(summary["verdict"], "DONE")
        self.assertEqual((summary["unexpected_tool_count"], summary["turns"]), (0, 2))
        self.assertEqual(summary["mcp_status"], {"ommatix": "connected"})
        att = run.metrics.attestation
        self.assertEqual((att["cli"], att["provider"], att["claude_code_version"]),
                         ("minimal-agent", "mimo", ma.AGENT_VERSION))

    def test_every_request_carries_the_three_definitions_the_instruction_the_prompt_and_nothing_else(self):
        run = AgentRun(self, self.SCRIPT, extra={"reasoning_effort": "max"})
        first, second = bodies(run)
        for body in (first, second):
            self.assertEqual(set(body), {"model", "messages", "tools", "tool_choice", "stream", "max_tokens",
                                         "reasoning_effort"})
            self.assertEqual(body["max_tokens"], ma.DEFAULT_COMPLETION_CEILING)
            self.assertEqual([t["function"]["name"] for t in body["tools"]], list(rg.TOOLS))
            self.assertIs(body["stream"], False)
        self.assertEqual(first["messages"], [{"role": "system", "content": INSTRUCTION},
                                              {"role": "user", "content": PROMPT}])
        roles = [m["role"] for m in second["messages"]]
        self.assertEqual(roles, ["system", "user", "assistant", "tool"])
        self.assertEqual(second["messages"][:2], first["messages"])
        self.assertTrue(all(isinstance(m["content"], (str, type(None))) for m in second["messages"]))

    def test_the_provider_decides_the_auth_header_and_the_key_is_in_that_header_only(self):
        mimo = AgentRun(self, self.SCRIPT, provider="mimo", key="KEY-MIMO-1")
        self.assertEqual(mimo.requests[0]["headers"].get("api-key"), "KEY-MIMO-1")
        self.assertNotIn("authorization", mimo.requests[0]["headers"])
        deepseek = AgentRun(self, self.SCRIPT, provider="deepseek", key="KEY-DS-2")
        self.assertEqual(deepseek.requests[0]["headers"].get("authorization"), "Bearer KEY-DS-2")
        self.assertNotIn("api-key", deepseek.requests[0]["headers"])
        for run, key in ((mimo, "KEY-MIMO-1"), (deepseek, "KEY-DS-2")):
            everything = json.dumps(run.metrics.summary()) + run.stderr.read_text(encoding="utf-8") + " ".join(run.argv)
            self.assertNotIn(key, everything)
            self.assertTrue(all(key.encode() not in r["body"] for r in run.requests))

    def test_the_path_below_the_host_is_the_providers(self):
        run = AgentRun(self, self.SCRIPT, base_path="/v1")
        self.assertEqual({r["path"] for r in run.requests}, {"/v1/chat/completions"})

    def test_a_tool_the_model_invents_ends_the_session_and_nothing_is_forwarded(self):
        script = [{"tool": "bash", "args": {"command": "dir"}}, {"text": "VERDICT: IMPOSSIBLE"}]
        run = AgentRun(self, script)
        self.assertEqual(run.result["result"], "tool_call_unknown_name")
        self.assertEqual(len(run.requests), 1)
        self.assertEqual(run.forwarded, [])
        self.assertEqual(run.metrics.summary()["tool_calls"], {})

    def test_reasoning_content_is_carried_back_in_the_same_turn(self):
        script = [{"tool": "observe_screen", "args": {}, "reasoning": "thinking about it"}, {"text": "VERDICT: DONE"}]
        run = AgentRun(self, script)
        assistant = bodies(run)[1]["messages"][2]
        self.assertEqual(assistant["reasoning_content"], "thinking about it")

    def test_a_server_that_lists_a_fourth_tool_ends_the_session_before_any_request(self):
        run = AgentRun(self, self.SCRIPT, engine_extra=("--extra-tool", "run_shell"))
        self.assertEqual(run.requests, [])
        self.assertIn("tools_differ", run.metrics.init_problems)
        self.assertEqual(run.ran["killed"], "init")

    def test_a_turn_limit_and_a_token_limit_end_the_goal_with_the_streams_names(self):
        looping = [{"tool": "observe_screen", "args": {}}] * 10
        by_turns = AgentRun(self, looping, max_turns=2)
        self.assertEqual(by_turns.metrics.result["subtype"], "error_max_turns")
        self.assertEqual(len(by_turns.requests), 2)
        by_tokens = AgentRun(self, looping, max_tokens=150)
        self.assertEqual(by_tokens.metrics.result["subtype"], "error_max_budget_usd")
        self.assertLessEqual(len(by_tokens.requests), 2)

    def test_usage_comes_out_in_the_summary_the_way_the_runner_reads_it(self):
        run = AgentRun(self, self.SCRIPT)
        usage = run.metrics.result["usage"]
        self.assertEqual(usage["output_tokens"], 10)
        self.assertEqual(usage["input_tokens"], 101 + 102)
        figures = summarise.token_figures(run.metrics.summary())
        self.assertEqual((figures["context_tokens"], figures["output_tokens"], figures["source"]),
                         (203, 10, "cli_usage"))
        self.assertIsNone(figures["cost_usd"])

    def test_the_prompt_is_never_on_the_command_line(self):
        run = AgentRun(self, self.SCRIPT)
        self.assertFalse(any(PROMPT in a for a in run.argv))
        self.assertIn("-I", run.argv)

    def test_nothing_of_the_operators_environment_reaches_the_child(self):
        hostile = {"OPENAI_API_KEY": "x1", "DEEPSEEK_API_KEY": "x2", "MIMO_API_KEY": "x3", "ANTHROPIC_API_KEY": "x4",
                   "HTTPS_PROXY": "http://evil", "HTTP_PROXY": "http://evil", "ALL_PROXY": "http://evil",
                   "PYTHONPATH": "/evil", "PYTHONSTARTUP": "/evil.py", "ANTHROPIC_BASE_URL": "http://evil",
                   "OPENCODE_CONFIG_DIR": "/evil", "CODEX_HOME": "/evil"}
        with mock.patch.dict(os.environ, hostile):
            env = agentcli.MinimalCLI(argparse.Namespace(provider="mimo")).env()
        self.assertFalse(set(hostile) & set(env))
        basics = {b.upper() for b in rg.BASIC_ENV} | {"PYTHONUTF8", "PYTHONNOUSERSITE"}
        self.assertTrue(all(k.upper() in basics for k in env), sorted(k for k in env if k.upper() not in basics))


class HostileEndpointTests(unittest.TestCase):
    """An endpoint that answers with an error body or a redirect: nothing it says reaches the stream."""

    def serve(self, handler_body):
        class Handler(http.server.BaseHTTPRequestHandler):
            hits = []

            def log_message(self, *a):
                pass

            def do_POST(self):
                self.rfile.read(int(self.headers.get("Content-Length") or 0))
                Handler.hits.append(self.path)
                handler_body(self)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(lambda: (server.shutdown(), server.server_close()))
        api = mock.Mock()
        api.url = f"http://127.0.0.1:{server.server_address[1]}"
        api.requests = []
        return api, Handler

    def test_an_http_error_body_never_reaches_the_stream_or_the_logs(self):
        def reply(h):
            body = b'{"error":"echoed your prompt: SECRET-PROMPT-TEXT"}'
            h.send_response(500)
            h.send_header("Content-Length", str(len(body)))
            h.end_headers()
            h.wfile.write(body)
        api, handler = self.serve(reply)
        run = AgentRun(self, None, api=api)
        result = run.metrics.result
        self.assertEqual((result["subtype"], result["result"]), ("error_during_execution", "http_500"))
        self.assertNotIn("SECRET-PROMPT-TEXT", json.dumps(run.metrics.summary()) + run.stderr.read_text(encoding="utf-8"))

    def test_a_redirect_is_not_followed(self):
        def reply(h):
            h.send_response(302)
            h.send_header("Location", "http://127.0.0.1:1/elsewhere")
            h.send_header("Content-Length", "0")
            h.end_headers()
        api, handler = self.serve(reply)
        run = AgentRun(self, None, api=api)
        self.assertEqual(run.metrics.result["result"], "http_302")
        self.assertEqual(len(handler.hits), 1)

    def test_a_reply_without_a_message_is_an_error_not_a_crash(self):
        def reply(h):
            body = b'{"choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 1}}'
            h.send_response(200)
            h.send_header("Content-Length", str(len(body)))
            h.end_headers()
            h.wfile.write(body)
        api, handler = self.serve(reply)
        run = AgentRun(self, None, api=api)
        self.assertEqual(run.metrics.result["result"], "reply_has_no_message")


class VerifierTests(unittest.TestCase):
    def captured(self):
        run = AgentRun(self, EndToEndTests.SCRIPT, extra={"reasoning_effort": "max"}, base_path="/v1")
        self.assertEqual(len(run.requests), 2)
        return run

    def check(self, requests, **overrides):
        kw = dict(model="mimo-test", expected_extra={"reasoning_effort": "max"}, ceiling=ma.DEFAULT_COMPLETION_CEILING,
                  system_instruction=INSTRUCTION, prompt=PROMPT, sentinels={"k": "CANARY-K-1"},
                  dummy_key="sk-test-KEY-123456", auth_style="api-key", base_path="/v1")
        kw.update(overrides)
        return pc.check_chat_requests(requests, **kw)

    def test_a_real_request_sequence_passes_and_yields_the_facts_the_proof_binds(self):
        problems, facts = self.check(self.captured().requests)
        self.assertEqual(problems, [])
        self.assertEqual((facts["requests"], facts["request_keys"]),
                         (2, ["max_tokens", "messages", "model", "reasoning_effort", "stream", "tool_choice", "tools"]))
        self.assertEqual(len(facts["tool_definitions_sha256"]), 64)

    def test_each_kind_of_deviation_is_seen(self):
        run = self.captured()
        mutations = pc.control_mutations(run.requests[0], "CANARY-K-1", {"reasoning_effort": "max"})
        for name, mutated in mutations.items():
            problems, _ = self.check([mutated])
            self.assertTrue(problems, f"{name} went unseen")
        expected = {"extra_tool": "tools_not_the_three", "sentinel": "sentinel_in_request",
                    "escaped_sentinel": "sentinel_in_request",
                    "image_part": "non_text_content", "extra_key": "body_keys_differ",
                    "other_model": "model_differs", "dropped_tool": "tools_not_the_three",
                    "changed_instruction": "opening_messages_differ", "changed_ceiling": "param_values_differ",
                    "changed_value_reasoning_effort": "param_values_differ"}
        self.assertEqual(sorted(mutations), sorted(expected))
        for name, mutated in mutations.items():
            self.assertIn(expected[name], self.check([mutated])[0], name)

    def test_headers_paths_keys_and_history_are_checked_too(self):
        run = self.captured()
        base = run.requests
        extra_header = [base[0] | {"headers": base[0]["headers"] | {"x-extra": "1"}}]
        self.assertIn("header_not_expected", self.check(extra_header)[0])
        wrong_key = [base[0] | {"headers": base[0]["headers"] | {"api-key": "other"}}]
        self.assertIn("key_header_differs", self.check(wrong_key)[0])
        self.assertIn("request_path_differs", self.check([base[0] | {"path": "/v1/other"}])[0])
        self.assertIn("request_path_differs", self.check(base, base_path="")[0])
        rewritten = json.loads(base[1]["body"])
        rewritten["messages"][1]["content"] = "something else"
        self.assertIn("history_rewritten", self.check([base[0], base[1] | {"body": json.dumps(rewritten).encode()}])[0])
        self.assertEqual(self.check([])[0], ["no_request_captured"])
        self.assertIn("request_unreadable", self.check([base[0] | {"body": b"not json"}])[0])

    def test_a_key_in_a_body_and_a_foreign_tool_call_are_seen(self):
        run = self.captured()
        body = json.loads(run.requests[1]["body"])
        body["messages"][3]["content"] = "leak sk-test-KEY-123456"
        self.assertIn("key_in_body", self.check([run.requests[1] | {"body": json.dumps(body).encode()}])[0])
        body = json.loads(run.requests[1]["body"])
        body["messages"][2]["tool_calls"][0]["function"]["name"] = "bash"
        self.assertIn("tool_call_unknown_name", self.check([run.requests[1] | {"body": json.dumps(body).encode()}])[0])

    def test_the_agents_source_passes_its_static_check_and_hostile_sources_do_not(self):
        self.assertEqual(pc.static_problems(agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")), [])
        for source, token in (("import socket", "import_socket"), ("import shutil", "import_shutil"),
                              ("eval('1')", "forbidden_call_eval"), ("open('x')", "forbidden_call_open"),
                              ("import os\nos.system('x')", "os_system"), ("import os\nos.environ", "os_environ"),
                              ("import subprocess\nsubprocess.run([])", "subprocess_run"),
                              ("from http import client", "import_http")):
            self.assertIn(token, pc.static_problems(source), source)


class PreflightAndProofTests(unittest.TestCase):
    def args(self, **kw):
        base = dict(provider="mimo", model="mimo-test", effort=None, extra_param=None, real_base_url=None,
                    exe=None, timeout=60)
        return argparse.Namespace(**(base | kw))

    def engine(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        return [sys.executable, str(FAKE_ENGINE), "--received", str(Path(tmp.name) / "in")]

    def test_the_preflight_passes_on_the_real_agent_and_its_control_sees_every_deviation(self):
        proof = pc.run_minimal(self.args(), self.engine())
        self.assertEqual(proof["problems"], [])
        self.assertTrue(proof["passed"])
        self.assertEqual(proof["control_blind"], [])
        self.assertEqual(len(proof["control_detected"]), 9)  # no configured parameter on mimo: no value mutation
        self.assertEqual(proof["requests_checked"], 2)
        self.assertEqual((proof["plugins"], proof["agents"], sorted(proof["tools"])), ([], [], THREE))
        self.assertEqual(proof["base_url"], "https://api.xiaomimimo.com/v1")
        self.assertNotIn("sk-canary", json.dumps({k: v for k, v in proof.items() if k != "egress_controls"}))

    def test_a_leaky_agent_fails_the_preflight(self):
        leaky = agentcli.MINIMAL_AGENT.read_text(encoding="utf-8").replace(
            'messages = [{"role": "system", "content": args.system_prompt},\n                    {"role": "user", "content": prompt}]',
            'extra_ctx = Path("CLAUDE.md").read_text(encoding="utf-8") if Path("CLAUDE.md").exists() else ""\n'
            '        messages = [{"role": "system", "content": args.system_prompt},\n'
            '                    {"role": "user", "content": prompt + extra_ctx}]')
        self.assertNotEqual(leaky, agentcli.MINIMAL_AGENT.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "minimal_agent.py"
            copy.write_text(leaky, encoding="utf-8")
            with mock.patch.object(agentcli, "MINIMAL_AGENT", copy):
                proof = pc.run_minimal(self.args(), self.engine())
        self.assertFalse(proof["passed"])
        self.assertIn("sentinel_in_request", proof["problems"])
        self.assertIn("opening_messages_differ", proof["problems"])

    def test_an_agent_that_adds_a_tool_or_a_body_key_fails_the_preflight(self):
        source = agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")
        for old, new, token in (
                ('"max_tokens": args.completion_ceiling, **args.extra_param}',
                 '"max_tokens": args.completion_ceiling, "metadata": {"a": 1}, **args.extra_param}',
                 "body_keys_differ"),
                ('"tool_choice": "auto",', '"tool_choice": "required",', "stream_or_tool_choice_differs")):
            self.assertIn(old, source)
            with tempfile.TemporaryDirectory() as tmp:
                copy = Path(tmp) / "minimal_agent.py"
                copy.write_text(source.replace(old, new), encoding="utf-8")
                with mock.patch.object(agentcli, "MINIMAL_AGENT", copy):
                    proof = pc.run_minimal(self.args(), self.engine())
            self.assertIn(token, proof["problems"])

    def good_proof(self, **changes):
        proof = pc.run_minimal(self.args(), self.engine())
        self.assertTrue(proof["passed"], proof["problems"])
        return proof | changes

    def write(self, proof):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = Path(tmp.name) / "proof.json"
        path.write_text(json.dumps(proof), encoding="utf-8")
        return path

    def cli(self, **kw):
        return agentcli.MinimalCLI(argparse.Namespace(**({"provider": "mimo", "base_url": None, "effort": None,
                                                           "extra_param": None, "model": "mimo-test"} | kw)))

    def test_the_loader_accepts_a_passing_proof_and_refuses_the_rest(self):
        cli = self.cli()
        proof = self.good_proof()
        self.assertEqual(cli.load_proof(self.write(proof))["agent_sha256"], proof["agent_sha256"])
        for broken in (proof | {"passed": False}, proof | {"verifier_revision": 0}, proof | {"schema": "x"},
                       proof | {"tools": ["mcp__ommatix__observe_screen"]}, proof | {"control_detected": []},
                       proof | {"plugins": ["p"]}):
            with self.assertRaises(SystemExit):
                cli.load_proof(self.write(broken))
        with self.assertRaises(SystemExit):
            cli.load_proof(Path("does-not-exist.json"))

    def test_the_proof_binds_the_agent_the_provider_the_model_the_parameters_and_the_instruction(self):
        proof = self.good_proof()
        args = argparse.Namespace(model="mimo-test")
        self.assertEqual(self.cli().proof_problems(proof, args, None, INSTRUCTION), [])
        self.assertIn("agent_sha256_differs_from_proof",
                      self.cli().proof_problems(proof | {"agent_sha256": "0" * 64}, args, None, INSTRUCTION))
        self.assertIn("requested_model_differs_from_proof",
                      self.cli().proof_problems(proof, argparse.Namespace(model="other"), None, INSTRUCTION))
        self.assertIn("system_instruction_sha256_differs_from_proof",
                      self.cli().proof_problems(proof, args, None, INSTRUCTION + " "))
        self.assertIn("extra_param_sha256_differs_from_proof",
                      self.cli(extra_param={"reasoning_effort": "high"}).proof_problems(proof, args, None, INSTRUCTION))
        self.assertIn("provider_differs_from_proof",
                      self.cli(provider="deepseek").proof_problems(proof, args, None, INSTRUCTION))
        self.assertIn("base_url_differs_from_proof",
                      self.cli(base_url="http://127.0.0.1:9").proof_problems(proof, args, None, INSTRUCTION))

    def test_the_session_is_compared_with_the_proof_at_init(self):
        proof = self.good_proof()
        run = AgentRun(self, EndToEndTests.SCRIPT, model="mimo-test")
        att = run.metrics.attestation
        self.assertEqual(rg.init_problems(att, proof), [])
        self.assertIn("provider_differs_from_proof", rg.init_problems(att, proof | {"provider": "deepseek"}))
        self.assertIn("version_differs_from_proof", rg.init_problems(att, proof | {"claude_code_version": "9.9.9"}))
        self.assertIn("init_fields_differ", rg.init_problems(att, proof | {"init_keys_sha256": "0" * 64}))


class ProviderTableTests(unittest.TestCase):
    def test_the_providers_are_https_endpoints_with_a_stated_caveat_and_no_secret(self):
        for name, p in agentcli.PROVIDERS.items():
            self.assertIsNone(ma.endpoint_problem(p["base_url"]), name)
            self.assertTrue(p["base_url"].startswith("https://"))
            self.assertIn(p["auth_style"], ma.AUTH_STYLES)
            self.assertTrue(p["caveat"] and p["docs"].startswith("https://"))
            self.assertNotRegex(json.dumps(p), r"sk-[A-Za-z0-9]{8}")
        self.assertEqual(set(agentcli.PROVIDERS), {"deepseek", "mimo"})
        self.assertEqual(set(agentcli.PROVIDER_NAMES) - {"anthropic"}, set(agentcli.PROVIDERS))

    def test_openai_is_authorised_but_not_runnable_and_there_is_no_openai_endpoint(self):
        self.assertIn("openai-gpt-6-luna", agentcli.AUTHORISED_NOT_RUNNABLE)
        self.assertNotIn("openai", agentcli.PROVIDERS)

    def test_effort_max_has_a_spelling_only_where_the_documentation_was_read(self):
        self.assertEqual(agentcli.minimal_extra_param("deepseek", "max", None)["reasoning_effort"], "max")
        self.assertEqual(agentcli.minimal_extra_param("deepseek", "max", {"temperature": 1})["temperature"], 1)
        with self.assertRaises(SystemExit):
            agentcli.minimal_extra_param("mimo", "max", None)
        self.assertEqual(agentcli.minimal_extra_param("mimo", None, {"reasoning_effort": "max"}),
                         {"reasoning_effort": "max"})


class RunnerIntegrationTests(unittest.TestCase):
    def test_plan_for_minimal_prints_the_flags_without_a_key_and_starts_nothing(self):
        args = rg.parse_args(["--plan", "--agent-cli", "minimal", "--provider", "deepseek", "--effort", "max"])
        goals = rg.load_goals()
        line = rg.plan_lines(args, goals, rg.select_goals(goals, ["G01"]), Path("out"))[0]
        flags = line["claude_flags"]
        self.assertIn("https://api.deepseek.com", flags)
        self.assertIn("deepseek-flash", flags)
        self.assertEqual(line["agent_cli"], "minimal")
        self.assertIn('"reasoning_effort": "max"', " ".join(flags))
        self.assertNotIn("sk-", " ".join(flags))

    def test_minimal_needs_a_provider_a_proof_and_a_key_file_and_only_a_loopback_override(self):
        stderr = contextlib.redirect_stderr(io.StringIO())
        stderr.__enter__()
        self.addCleanup(stderr.__exit__, None, None, None)
        with self.assertRaises(SystemExit):
            rg.parse_args(["--plan", "--agent-cli", "minimal"])
        with self.assertRaises(SystemExit):
            rg.parse_args(["--agent-cli", "minimal", "--provider", "mimo", "--model", "m", "--exe", "x"])
        with self.assertRaises(SystemExit):
            rg.parse_args(["--plan", "--agent-cli", "minimal", "--provider", "mimo",
                           "--base-url", "https://elsewhere.example/v1"])
        with self.assertRaises(SystemExit):  # a smoke would reach the provider: it needs the key file too
            rg.parse_args(["--smoke", "--agent-cli", "minimal", "--provider", "mimo", "--exe", "x"])
        args = rg.parse_args(["--plan", "--agent-cli", "minimal", "--provider", "mimo"])
        self.assertEqual(args.model, "mimo-v2.6-flash")

    def test_codex_and_opencode_cannot_run_a_goal(self):
        stderr = contextlib.redirect_stderr(io.StringIO())
        stderr.__enter__()
        self.addCleanup(stderr.__exit__, None, None, None)
        for kind in ("codex", "opencode"):
            with self.assertRaises(SystemExit):
                rg.parse_args(["--agent-cli", kind, "--model", "m", "--exe", "x"])
            plan = rg.parse_args(["--plan", "--agent-cli", kind])
            self.assertTrue(rg.agent_flags(plan, Path("out"))[0].startswith("refused:"))
            cli = agentcli.make(plan)
            self.assertIn("cannot be confined", cli.refusal())
            with self.assertRaises(SystemExit):
                cli.build_argv(model="m", mcp_config=Path("x"), system_instruction="i", max_budget_usd=1)

    def test_a_passing_probe_still_cannot_run_a_goal_and_a_failing_one_names_why(self):
        cli = agentcli.CodexCLI(argparse.Namespace())
        with tempfile.TemporaryDirectory() as tmp:
            failing = Path(tmp) / "p.json"
            failing.write_text(json.dumps({"schema": agentcli.CLI_PROOF_SCHEMA, "passed": False,
                                           "problems": ["tool_surface_wider_than_three"]}), encoding="utf-8")
            with self.assertRaises(SystemExit) as caught:
                cli.load_proof(failing)
            self.assertIn("tool_surface_wider_than_three", str(caught.exception))
            passing = Path(tmp) / "q.json"
            passing.write_text(json.dumps({"schema": agentcli.CLI_PROOF_SCHEMA, "passed": True, "problems": []}),
                               encoding="utf-8")
            with self.assertRaises(SystemExit) as caught:
                cli.load_proof(passing)
            self.assertIn("reads no codex stream", str(caught.exception))

    def test_claude_stays_the_default_and_its_command_is_unchanged(self):
        args = rg.parse_args(["--plan"])
        cli = agentcli.make(args)
        self.assertIsInstance(cli, agentcli.ClaudeCLI)
        argv = cli.build_argv(model="m", mcp_config=Path("x.json"), system_instruction="i", max_budget_usd=1)
        self.assertEqual(argv, rg.build_claude_argv(claude="claude", model="m", mcp_config=Path("x.json"),
                                                    system_instruction="i", max_budget_usd=1))

    def test_a_goal_runs_end_to_end_through_the_runner_with_the_minimal_agent_and_its_provenance(self):
        from test_run_goal import FakeEditorRun, FakeApp, FakeSampler, stream  # noqa: F401
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        key = root / "k.key"
        key.write_text("sk-runner-key-1\n", encoding="utf-8")
        out = root / "out"
        out.mkdir()
        engine = [sys.executable, str(FAKE_ENGINE), "--received", str(root / "in")]
        act = {"target_id": "e1", "expected_revision": 1, "action": "click", "dry_run": True}
        script = [{"tool": "observe_screen", "args": {}}, {"tool": "execute_action", "args": act},
                  {"text": "VERDICT: DONE"}]
        with FakeApi(script) as api:
            proof_args = argparse.Namespace(provider="mimo", model="mimo-test", effort=None, extra_param=None,
                                            real_base_url=api.url + "/v1", exe=None, timeout=60)
            proof = pc.run_minimal(proof_args, engine)
            self.assertTrue(proof["passed"], proof["problems"])
            (root / "proof.json").write_text(json.dumps(proof), encoding="utf-8")
            args = rg.parse_args(["--phase", "1", "--agent-cli", "minimal", "--provider", "mimo", "--model", "mimo-test",
                                  "--exe", str(root / "ommatix.exe"), "--isolation-proof", str(root / "proof.json"),
                                  "--key-file", str(key), "--base-url", api.url + "/v1",
                                  "--testapp-exe", str(root / "t.exe")])
            args.base_url = api.url + "/v1"
            initial = GOALS["initial_states"]["testapp"]
            app = FakeApp(GOALS["goals"][0], dict(initial))
            deps = type("Deps", (), {
                "announce": staticmethod(lambda t: None), "sleep": staticmethod(lambda s: None),
                "make_app": staticmethod(lambda goal, exe, folder, driver, temp_root=None: app),
                "make_sampler": staticmethod(lambda read: FakeSampler([])),
                "run_claude": staticmethod(self.relay(engine, root))})
            with mock.patch.object(agentcli.MinimalCLI, "base_url", lambda s: api.url + "/v1"):
                loaded = agentcli.make(args).load_proof(root / "proof.json")
                with mock.patch.object(rg, "ancestor_context", return_value=[]), \
                        mock.patch.object(rg, "engine_argv", return_value=engine):
                    result = rg.run_goal(args, GOALS, GOALS["goals"][0], out, None, None, loaded, deps)
        self.assertNotIn("aborted", result, result.get("aborted"))
        self.assertEqual(result["metrics"]["init_problems"], [])
        prov = result["provenance"]
        self.assertEqual((prov["cli"], prov["provider"], prov["model"], prov["claude_code_version"]),
                         ("minimal-agent", "mimo", "mimo-test", ma.AGENT_VERSION))
        self.assertEqual(prov["agent_sha256"], hashlib.sha256(agentcli.MINIMAL_AGENT.read_bytes()).hexdigest())
        self.assertEqual(prov["base_url_host"], "127.0.0.1")
        action = result["metrics"]["actions"][0]
        self.assertEqual((action["dry_run"], action["label"], action["revision_current"], action["observed_before"]),
                         (True, "other", True, True))
        self.assertEqual(result["metrics"]["tool_calls"], {"execute_action": 1, "observe_screen": 1})
        self.assertFalse(result["score"]["success"])  # the fake element's label is not the goal's "Sumar"
        self.assertTrue(result["score"]["nothing_changed"])
        self.assertNotIn("sk-runner-key-1", json.dumps(result))
        self.assertNotIn("ETIQUETA-SECRETA", json.dumps(result))

    def relay(self, engine, root):
        """The runner's own ``run_claude`` with the proxy log pointed where the harness reads it."""
        def run(argv, prompt, cwd, metrics, timeout, *a, **k):
            return rg.run_claude(argv, prompt, cwd, metrics, timeout, *a, **k)
        return run


class ProbeTests(unittest.TestCase):
    """The Codex and OpenCode verdicts, read off request bodies shaped like the ones the installed
    versions sent to the stand-in (names only; Codex 0.163.0, OpenCode 2.0.12)."""
    ALLOWED_CODEX = {f"mcp__ommatix__{t}" for t in rg.TOOLS}

    def codex_body(self, tools, additional=None, skills=True):
        items = []
        if skills:
            items.append({"type": "message", "role": "developer", "content": [
                {"type": "input_text", "text": "<skills_instructions>\n### Available skills\n- imagegen: makes images (file: r1/x)\n</skills_instructions>"}]})
        if additional is not None:
            items.append({"type": "additional_tools", "role": "developer", "tools": additional})
        return {"model": "m", "tools": tools, "input": items}

    def test_codex_with_the_strictest_configuration_still_offers_view_image_and_a_tool_search(self):
        body = self.codex_body([{"type": "function", "name": "view_image"}],
                               [{"type": "function", "name": "list_mcp_resources"},
                                {"type": "function", "name": "read_mcp_resource"}, {"type": "tool_search"}])
        surface = pc.codex_surface(body)
        self.assertIn("view_image", surface)
        problems = pc.probe_verdict(surface, pc.codex_has_skills(body), self.ALLOWED_CODEX)
        self.assertEqual(sorted(problems), ["ommatix_tools_not_offered_directly", "skills_from_real_home",
                                            "tool_surface_wider_than_three"])

    def test_codex_code_mode_and_namespaces_are_expanded_into_the_surface(self):
        body = self.codex_body([], [
            {"type": "custom", "name": "exec", "description": "x\n### `apply_patch`\ny\n### `view_image`\nz"},
            {"type": "namespace", "name": "collaboration", "tools": [{"type": "function", "name": "spawn_agent"}]}])
        surface = pc.codex_surface(body)
        for name in ("exec", "exec>apply_patch", "exec>view_image", "collaboration.spawn_agent"):
            self.assertIn(name, surface)

    def test_a_codex_request_with_exactly_the_three_tools_and_no_skills_would_pass(self):
        body = self.codex_body([{"type": "function", "name": n} for n in sorted(self.ALLOWED_CODEX)], skills=False)
        self.assertEqual(pc.probe_verdict(pc.codex_surface(body), pc.codex_has_skills(body), self.ALLOWED_CODEX), [])

    def opencode_body(self, tools, system):
        return {"model": "m", "tools": [{"type": "function", "function": {"name": n, "description": d}}
                                        for n, d in tools], "messages": [{"role": "system", "content": system}]}

    def test_opencode_reaches_the_mcp_tools_only_through_a_code_runtime_with_fetch(self):
        body = self.opencode_body([("execute", "Run JavaScript ... `fetch` is available for HTTP requests.")],
                                  "intro\n<available_skills>\n  <skill>\n    <id>x</id>\n  </skill>\n</available_skills>\n"
                                  "## Available tools\n\n- browser (45 tools, 9 shown) // desktop\n")
        surface = pc.opencode_surface(body)
        self.assertEqual(surface, ["catalog:browser", "execute", "execute+fetch"])
        problems = pc.probe_verdict(surface, pc.opencode_has_skills(body), {f"ommatix_{t}" for t in rg.TOOLS})
        self.assertEqual(sorted(problems), ["ommatix_tools_not_offered_directly", "skills_from_real_home",
                                            "tool_surface_wider_than_three"])

    def test_no_skills_block_means_no_skills_finding(self):
        body = self.opencode_body([("execute", "x")], "nothing here")
        self.assertFalse(pc.opencode_has_skills(body))

    def test_the_builders_make_the_strictest_configuration_found(self):
        text = pc.codex_config(1234, "gpt-6-luna", Path("C:/x/catalog.json"), "C:/py/python.exe", ["C:/p/proxy.py", "--", "e"])
        for needle in ('web_search = "disabled"', "shell_tool = false", "multi_agent = false", "goals = false",
                       "[tools.update_plan]", "[tools.experimental_request_user_input]", 'wire_api = "responses"',
                       "http://127.0.0.1:1234/v1", "[mcp_servers.ommatix]"):
            self.assertIn(needle, text)
        config = pc.opencode_config(1234, "m1", ["py", "proxy.py"])
        self.assertEqual(config["permission"]["*"], "deny")
        self.assertEqual((config["share"], config["update"], config["websearch"]), ("disabled", "disable", False))
        self.assertIn("127.0.0.1:1234", config["provider"]["local"]["options"]["baseURL"])


class SummaryByModelTests(unittest.TestCase):
    def result(self, goal, provenance, success=True):
        return {"goal": goal, "phase": 1, "expected": "done", "provenance": provenance,
                "score": {"success": success, "false_success": False},
                "metrics": {"cli_usage": {"input_tokens": 10, "output_tokens": 5}, "tokens_per_message": [],
                            "step_ms": [], "wall_s": 1.0}, "proxy": {}}

    def test_results_are_grouped_by_agent_provider_and_model_and_never_blended_in_the_render(self):
        results = [self.result("G01", {"cli": "claude-code", "provider": "anthropic", "model": "claude-haiku-5-5"}),
                   self.result("G01", {"cli": "minimal-agent", "provider": "deepseek", "model": "deepseek-flash"}, False),
                   self.result("G02", {"cli": "minimal-agent", "provider": "mimo", "model": "mimo-v2.6-flash"}),
                   self.result("G03", {"claude_code_version": "2.1.302", "model": "claude-haiku-5-5"})]
        summary = summarise.summarise(results)
        self.assertEqual(sorted(summary["models"]), ["claude-code:anthropic/claude-haiku-5-5",
                                                     "minimal-agent:deepseek/deepseek-flash",
                                                     "minimal-agent:mimo/mimo-v2.6-flash"])
        self.assertEqual(summary["models"]["claude-code:anthropic/claude-haiku-5-5"]["phases"]["1"]["attempted"], 2)
        self.assertEqual(summary["models"]["minimal-agent:deepseek/deepseek-flash"]["phases"]["1"]["success"], 0)
        self.assertEqual(summary["phases"]["1"]["attempted"], 4)
        text = summarise.render(summary)
        self.assertEqual(text.count("== "), 3)

    def test_one_model_renders_as_before(self):
        summary = summarise.summarise([self.result("G01", {"cli": "claude-code", "provider": "anthropic", "model": "m"})])
        self.assertNotIn("== ", summarise.render(summary))


if __name__ == "__main__":
    unittest.main()
