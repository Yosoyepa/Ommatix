"""Regression tests of the review of PR 93 (R1-R6) and of the Claude Code route (ADR-0037), written to fail
against the reviewed head 65a83c9 (see docs/reviews/2026-10-18-ag02-multi-model.md for each finding).

No network beyond 127.0.0.1, no real provider, no real CLI: the minimal agent runs against the stand-in
of ``fake_api.py`` and a fake engine behind the real budget proxy; the route's preflight runs the scripted
``fake_claude.py``."""

import argparse
import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import agentcli  # noqa: E402
import fake_api  # noqa: E402
import minimal_agent as ma  # noqa: E402
import preflight  # noqa: E402
import preflight_cli as pc  # noqa: E402
import run_goal as rg  # noqa: E402
from test_multimodel import FAKE_ENGINE, INSTRUCTION, PROMPT, AgentRun, EndToEndTests  # noqa: E402

GOALS = rg.load_goals()
THREE = sorted(rg.ALLOWED_TOOL_NAMES)
SENTINEL = "CANARY-K-1"
OBSERVE = {"tool": "observe_screen", "args": {}}
ACTION = {"request_id": "r1", "session_id": "fake", "snapshot_id": "s1", "geometry_epoch": 1,
          "target_id": "e1", "expected_revision": 1, "action": "click", "dry_run": True}


def call(name, arguments, call_id="call_1", **extra):
    """A raw assistant message with one function call, written exactly as given."""
    return {"message": {"role": "assistant", "content": None, "tool_calls": [
        {"id": call_id, "type": "function", "function": {"name": name, "arguments": arguments}} | extra]}}


def silence():
    stderr = contextlib.redirect_stderr(io.StringIO())
    stderr.__enter__()
    return stderr


def stored_text(run):
    """Everything the runner would keep of a session: the summary plus stderr."""
    return json.dumps(run.metrics.summary()) + run.stderr.read_text(encoding="utf-8")


# ---- R1: what the recipient reads, not the bytes -----------------------------------------------------

class R1DecodedRequestTests(unittest.TestCase):
    def captured(self):
        return AgentRun(self, EndToEndTests.SCRIPT, extra={"reasoning_effort": "max"}, base_path="/v1").requests

    def check(self, requests, **kw):
        base = dict(model="mimo-test", expected_extra={"reasoning_effort": "max"}, ceiling=ma.DEFAULT_COMPLETION_CEILING,
                    system_instruction=INSTRUCTION, prompt=PROMPT, sentinels={"k": SENTINEL},
                    dummy_key="sk-test-KEY-123456", auth_style="api-key", base_path="/v1")
        return pc.check_chat_requests(requests, **(base | kw))

    def test_decoded_strings_reach_keys_and_values_at_any_depth(self):
        body = json.loads('{"\\u006bey": ["\\u0043ANARY", {"inner": "x\\u0041"}]}')
        self.assertEqual(sorted(pc.decoded_strings(body)), ["CANARY", "inner", "key", "xA"])

    def test_a_sentinel_behind_a_json_escape_is_seen_although_the_raw_bytes_do_not_contain_it(self):
        mutated = pc.control_mutations(self.captured()[0], SENTINEL, {"reasoning_effort": "max"})["escaped_sentinel"]
        self.assertNotIn(SENTINEL.encode(), mutated["body"])
        self.assertEqual(json.loads(mutated["body"])["messages"][1]["content"][-len(SENTINEL):], SENTINEL)
        self.assertIn("sentinel_in_request", self.check([mutated])[0])

    def test_the_key_behind_an_escape_is_seen_too(self):
        request = self.captured()[0]
        body = request["body"].decode().replace('"role": "user"', '"role": "user", "x": "sk-test-\\u004bEY-123456"', 1)
        self.assertNotIn(b"sk-test-KEY-123456", body.encode())
        self.assertIn("key_in_body", self.check([request | {"body": body.encode()}])[0])

    def test_the_key_may_be_in_the_authentication_header_exactly_and_in_no_other_header(self):
        request = self.captured()[0]
        self.assertEqual(self.check([request])[0], [])
        copy = request | {"headers": request["headers"] | {"x-trace": "id sk-test-KEY-123456"}}
        problems = self.check([copy])[0]
        self.assertIn("key_in_another_header", problems)
        self.assertIn("header_not_expected", problems)
        suffix = request | {"headers": request["headers"] | {"api-key": "sk-test-KEY-123456-and-more"}}
        self.assertIn("key_header_differs", self.check([suffix])[0])

    def source_with(self, old, new):
        source = agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")
        self.assertIn(old, source)
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        copy = Path(tmp.name) / "minimal_agent.py"
        copy.write_text(source.replace(old, new), encoding="utf-8")
        return copy

    def test_the_full_preflight_rejects_an_agent_that_reads_the_context_and_hides_it_behind_an_escape(self):
        source = agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")
        leaky = source.replace(
            '''messages = [{"role": "system", "content": args.system_prompt},
                    {"role": "user", "content": prompt}]''',
            '''messages = [{"role": "system", "content": args.system_prompt},
                    {"role": "user", "content": prompt}]
        messages.append({"role": "assistant", "content": Path("CLAUDE.md").read_text(encoding="utf-8")})''')
        leaky = leaky.replace('data = json.dumps(body, ensure_ascii=False).encode("utf-8")',
                              'data = json.dumps(body, ensure_ascii=False).encode("utf-8").replace('
                              'b"CANARY", b"\\\\u0043ANARY")')
        self.assertNotEqual(leaky, source)
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "minimal_agent.py"
            copy.write_text(leaky, encoding="utf-8")
            with mock.patch.object(agentcli, "MINIMAL_AGENT", copy):
                proof = run_preflight()
        self.assertFalse(proof["passed"])
        self.assertIn("sentinel_in_request", proof["problems"])

    def test_a_stricter_verifier_invalidates_the_proofs_of_the_weaker_one(self):
        self.assertGreaterEqual(agentcli.MINIMAL_VERIFIER_REVISION, 2)
        proof = run_preflight()
        self.assertTrue(proof["passed"], proof["problems"])
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "p.json"
            path.write_text(json.dumps(proof | {"verifier_revision": 1}), encoding="utf-8")
            with self.assertRaises(SystemExit):
                agentcli.MinimalCLI(argparse.Namespace(provider="mimo")).load_proof(path)
            path.write_text(json.dumps(proof), encoding="utf-8")
            agentcli.MinimalCLI(argparse.Namespace(provider="mimo")).load_proof(path)


def run_preflight(provider="mimo", model="mimo-test", effort=None, extra_param=None):
    tmp = tempfile.TemporaryDirectory()
    engine = [sys.executable, str(FAKE_ENGINE), "--received", str(Path(tmp.name) / "in")]
    args = argparse.Namespace(provider=provider, model=model, effort=effort, extra_param=extra_param,
                              real_base_url=None, exe=None, timeout=60)
    try:
        return pc.run_minimal(args, engine)
    finally:
        tmp.cleanup()


# ---- R2: the budgets are real ---------------------------------------------------------------------------

class R2BudgetTests(unittest.TestCase):
    CAP = 20_000
    CEILING = 100

    def run_cap(self, script, **kw):
        return AgentRun(self, script, max_tokens=kw.pop("cap", self.CAP), ceiling=self.CEILING, **kw)

    def test_the_boundary_is_exact_a_request_that_fits_is_made_and_one_byte_more_is_not(self):
        budget = ma.Budget(1000, 100)
        budget.totals["input_tokens"] = 900
        self.assertTrue(budget.can_afford(0))
        self.assertFalse(budget.can_afford(1))
        self.assertFalse(budget.exhausted)
        budget.totals["output_tokens"] = 101
        self.assertTrue(budget.exhausted)

    def test_a_cap_smaller_than_the_first_reservation_makes_no_request_at_all(self):
        run = self.run_cap(EndToEndTests.SCRIPT, cap=500)
        self.assertEqual((run.requests, run.forwarded), ([], []))
        self.assertEqual(run.result["subtype"], "error_max_budget_usd")
        self.assertTrue(run.result["is_error"])

    def test_a_final_reply_that_overruns_the_cap_ends_the_goal_as_budget_exhausted_not_as_success(self):
        run = self.run_cap([{"text": "VERDICT: DONE", "usage": {"prompt_tokens": 30_000, "completion_tokens": 10}}])
        self.assertEqual(run.result["subtype"], "error_max_budget_usd")
        self.assertEqual(run.result["usage"]["input_tokens"] + run.result["usage"]["output_tokens"], 30_010)

    def test_a_final_reply_that_lands_exactly_on_the_cap_is_a_success(self):
        run = self.run_cap([{"text": "VERDICT: DONE", "usage": {"prompt_tokens": 19_990, "completion_tokens": 10}}])
        self.assertEqual((run.result["subtype"], run.result["is_error"]), ("success", False))

    def test_a_reply_that_overruns_the_cap_does_not_get_its_call_executed(self):
        run = self.run_cap([OBSERVE | {"usage": {"prompt_tokens": 30_000, "completion_tokens": 10}}])
        self.assertEqual(run.result["subtype"], "error_max_budget_usd")
        self.assertEqual(run.forwarded, [])

    def test_the_next_request_needs_its_reservation_a_big_context_so_far_stops_the_loop(self):
        run = self.run_cap([OBSERVE | {"usage": {"prompt_tokens": 19_000, "completion_tokens": 0}}, {"text": "x"}])
        self.assertEqual(len(run.requests), 1)
        self.assertEqual(len(run.forwarded), 1)
        self.assertEqual(run.result["subtype"], "error_max_budget_usd")

    def test_a_missing_or_malformed_usage_is_refused_never_counted_as_free(self):
        for usage in (None, {}, {"prompt_tokens": 10}, {"prompt_tokens": -1, "completion_tokens": 1},
                      {"prompt_tokens": True, "completion_tokens": 1}, {"prompt_tokens": "10", "completion_tokens": 1},
                      {"prompt_tokens": 1.5, "completion_tokens": 1}, [1, 2]):
            with self.subTest(usage=usage):
                run = self.run_cap([OBSERVE | {"usage": usage}, {"text": "VERDICT: DONE"}])
                self.assertEqual(run.result["result"], "usage_missing_or_invalid")
                self.assertEqual((len(run.requests), run.forwarded), (1, []))

    def test_the_dollar_cap_needs_prices_and_a_cap_below_the_next_reservation_stops_everything(self):
        run = AgentRun(self, EndToEndTests.SCRIPT, price="1,1,1", max_usd=0.0001, ceiling=100)
        self.assertEqual((run.requests, run.result["subtype"]), ([], "error_max_budget_usd"))
        run = AgentRun(self, EndToEndTests.SCRIPT, price="1,1,1", max_usd=50, ceiling=100)
        self.assertEqual(run.result["subtype"], "success")
        self.assertIsInstance(run.result["total_cost_usd"], float)

    def test_a_reply_that_overruns_the_dollar_cap_ends_the_goal(self):
        run = AgentRun(self, [{"text": "VERDICT: DONE", "usage": {"prompt_tokens": 10, "completion_tokens": 10_000}}],
                       price="0.001,0.001,1000", max_usd=1, ceiling=100)
        self.assertEqual(run.result["subtype"], "error_max_budget_usd")
        self.assertGreater(run.result["total_cost_usd"], 1)

    def test_the_runners_dollar_budget_reaches_the_command_or_is_refused(self):
        def cli(**kw):
            base = dict(provider="mimo", key_file=Path("k"), phase=1, price=None, max_usd_explicit=False)
            return agentcli.MinimalCLI(argparse.Namespace(**(base | kw)))
        args = dict(model="m", mcp_config=Path("m.json"), system_instruction="i")
        self.assertNotIn("--max-usd", cli().build_argv(max_budget_usd=0.01, **args))
        self.assertIn("dollar cap only with --price", cli(max_usd_explicit=True).refusal())
        small = cli(price="1,1,1", max_usd_explicit=True).build_argv(max_budget_usd=0.01, **args)
        large = cli(price="1,1,1", max_usd_explicit=True).build_argv(max_budget_usd=100, **args)
        self.assertNotEqual(small, large)
        self.assertEqual(small[small.index("--max-usd") + 1], "0.01")
        self.assertIsNone(cli(price="1,1,1", max_usd_explicit=True).refusal())

    def test_prices_and_caps_must_be_finite_and_nonnegative(self):
        stderr = silence()
        self.addCleanup(stderr.__exit__, None, None, None)
        base = ["--base-url", "http://127.0.0.1:9", "--model", "m", "--provider", "p", "--mcp-config", "m.json",
                "--system-prompt", "s"]
        with tempfile.TemporaryDirectory() as tmp:
            key = ["--key-file", str(Path(tmp) / "k")]
            for bad in (["--price", "nan,1,1"], ["--price", "1,2"], ["--price", "-1,1,1"], ["--price", "inf,1,1"],
                        ["--max-usd", "1"], ["--price", "1,1,1", "--max-usd", "nan"],
                        ["--price", "1,1,1", "--max-usd", "-1"], ["--completion-ceiling", "0"]):
                with self.assertRaises(SystemExit, msg=bad):
                    ma.parse_args(base + key + bad)


# ---- R3: the provider's calls are validated whole, before anything is emitted or forwarded -----------------

class R3CallValidationTests(unittest.TestCase):
    def refused(self, script, token, **kw):
        run = AgentRun(self, script, real_schemas=True, **kw)
        self.assertEqual((run.result["subtype"], run.result["result"]), ("error_during_execution", token))
        self.assertEqual(run.forwarded, [], "the bridge must not be reached")
        self.assertEqual(run.metrics.summary()["tool_calls"], {}, "nothing is emitted for a refused reply")
        return run

    def test_non_string_arguments_are_refused_not_repaired_into_an_empty_object(self):
        self.refused([call("get_engine_metrics", {"a": 1})], "arguments_not_a_string")
        self.refused([call("get_engine_metrics", None)], "arguments_not_a_string")

    def test_duplicate_keys_nan_and_non_objects_are_refused_before_the_proxy_could_see_them(self):
        self.refused([call("observe_screen", '{"limit": 1, "limit": 2}')], "arguments_unreadable")
        self.refused([call("observe_screen", '{"limit": NaN}')], "arguments_unreadable")
        self.refused([call("observe_screen", '{"limit": Infinity}')], "arguments_unreadable")
        self.refused([call("observe_screen", "not json")], "arguments_unreadable")
        self.refused([call("observe_screen", "[1]")], "arguments_not_an_object")

    def test_parallel_calls_are_refused_atomically_before_the_first_one_executes(self):
        message = call("get_engine_metrics", "{}")
        message["message"]["tool_calls"] *= 2
        self.refused([message], "multiple_tool_calls")

    def test_call_ids_must_be_present_plain_strings(self):
        for call_id in (None, 5, "", "a b", "x" * 65, "é"):
            with self.subTest(call_id=call_id):
                message = call("get_engine_metrics", "{}", call_id=call_id)
                self.refused([message], "tool_call_id_invalid")
        no_id = call("get_engine_metrics", "{}")
        del no_id["message"]["tool_calls"][0]["id"]
        self.refused([no_id], "tool_call_id_invalid")

    def test_a_repeated_id_across_turns_is_refused_so_receipts_stay_one_to_one(self):
        run = AgentRun(self, [call("get_engine_metrics", "{}", call_id="same"),
                              call("get_engine_metrics", "{}", call_id="same")], real_schemas=True)
        self.assertEqual(run.result["result"], "tool_call_id_repeated")
        self.assertEqual(len(run.forwarded), 1)
        summary = run.metrics.summary()
        self.assertEqual(summary["tool_calls"], {"get_engine_metrics": 1})
        self.assertEqual(len(summary["step_ms"]), 1)

    def test_only_function_calls_of_exactly_the_expected_shape_pass(self):
        message = call("get_engine_metrics", "{}")
        message["message"]["tool_calls"][0]["type"] = "custom"
        self.refused([message], "tool_call_shape")
        self.refused([call("get_engine_metrics", "{}", index=0)], "tool_call_shape")
        extra_function = call("get_engine_metrics", "{}")
        extra_function["message"]["tool_calls"][0]["function"]["strict"] = True
        self.refused([extra_function], "tool_call_shape")
        self.refused([call("bash", "{}")], "tool_call_unknown_name")
        self.refused([call(7, "{}")], "tool_call_unknown_name")
        listed = {"message": {"role": "assistant", "content": None, "tool_calls": {"id": "x"}}}
        self.refused([listed], "tool_calls_not_a_list")

    def test_arguments_must_match_the_tools_schema(self):
        for name, arguments in (("observe_screen", {"limit": 999}), ("observe_screen", {"limit": "5"}),
                                ("observe_screen", {"zzz": 1}), ("observe_screen", {"limit": True}),
                                ("get_engine_metrics", {"x": 1}),
                                ("execute_action", {k: v for k, v in ACTION.items() if k != "target_id"}),
                                ("execute_action", ACTION | {"action": "type"}),
                                ("execute_action", ACTION | {"action": "drag"}),
                                ("execute_action", ACTION | {"expected_revision": 0}),
                                ("execute_action", ACTION | {"action": "scroll"})):
            with self.subTest(name=name, arguments=arguments):
                self.refused([call(name, json.dumps(arguments))], "arguments_violate_the_schema")

    def test_valid_calls_pass_and_correlate_one_to_one(self):
        run = AgentRun(self, [call("observe_screen", '{"limit": 5}', call_id="c1"),
                              call("execute_action", json.dumps(ACTION), call_id="c2"), {"text": "VERDICT: DONE"}],
                       real_schemas=True)
        self.assertEqual(run.result["subtype"], "success")
        summary = run.metrics.summary()
        self.assertEqual(summary["tool_calls"], {"execute_action": 1, "observe_screen": 1})
        self.assertEqual([s["tool"] for s in summary["step_ms"]], ["observe_screen", "execute_action"])
        self.assertEqual(len(run.forwarded), 2)

    def test_in_the_dry_run_phase_a_live_or_unstated_dry_run_never_reaches_the_bridge(self):
        self.refused([call("execute_action", json.dumps(ACTION | {"dry_run": False}))],
                     "live_argument_in_dry_run_phase")
        without = {k: v for k, v in ACTION.items() if k != "dry_run"}
        self.refused([call("execute_action", json.dumps(without))], "live_argument_in_dry_run_phase")

    def test_in_phase_two_the_agent_does_not_pretend_to_be_the_gate_the_proxy_and_engine_are(self):
        run = AgentRun(self, [call("execute_action", json.dumps(ACTION | {"dry_run": False})),
                              {"text": "VERDICT: DONE"}], real_schemas=True, phase=2)
        self.assertNotIn("--dry-run-only", run.argv)
        self.assertEqual(len(run.forwarded), 1)

    def test_the_runner_asks_for_the_dry_run_flag_exactly_in_phase_one(self):
        def argv(phase):
            cli = agentcli.MinimalCLI(argparse.Namespace(provider="mimo", key_file=Path("k"), phase=phase))
            return cli.build_argv(model="m", mcp_config=Path("m.json"), system_instruction="i", max_budget_usd=1)
        self.assertIn("--dry-run-only", argv(1))
        self.assertNotIn("--dry-run-only", argv(2))

    def test_the_validator_refuses_a_keyword_it_does_not_implement(self):
        with self.assertRaises(ma.AgentError):
            ma.schema_ok({"type": "object", "patternProperties": {}}, {})
        self.assertTrue(ma.schema_ok({"type": "integer", "minimum": 1}, 3))
        self.assertFalse(ma.schema_ok({"type": "integer"}, 3.0))


# ---- R4: bounds before allocation and dispatch ---------------------------------------------------------------

class R4BoundsTests(unittest.TestCase):
    MARKER = "LEAKED-REPLY-TEXT"

    def refused(self, step, token):
        run = AgentRun(self, [step], real_schemas=True)
        self.assertEqual((run.result["subtype"], run.result["result"]), ("error_during_execution", token))
        self.assertEqual(run.forwarded, [])
        self.assertNotIn(self.MARKER, stored_text(run))
        return run

    def test_a_body_declared_larger_than_the_limit_is_refused_before_it_is_read(self):
        self.refused({"raw": self.MARKER.encode() + b" {}", "declared_length": ma.MAX_RESPONSE_BYTES + 1},
                     "reply_too_large")

    def test_a_body_streamed_past_the_limit_without_a_length_is_refused(self):
        raw = b'{"x": "' + self.MARKER.encode() + b"a" * (ma.MAX_RESPONSE_BYTES + 10) + b'"}'
        self.refused({"raw": raw, "undeclared": True}, "reply_too_large")

    def test_the_read_is_bounded_by_one_byte_more_than_the_limit(self):
        asked = []

        class Reply:
            headers = {}

            def read(self, n=-1):
                asked.append(n)
                return b"{}"

            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return False

        class Opener:
            def open(self, request, timeout=None):
                return Reply()
        ma.chat_request(Opener(), "http://127.0.0.1:9/chat/completions", {}, b"{}", 5)
        self.assertEqual(asked, [ma.MAX_RESPONSE_BYTES + 1])

    def test_oversized_arguments_text_and_reasoning_are_refused(self):
        self.refused(call("observe_screen", '{"query": "' + "a" * (ma.MAX_ARGUMENT_CHARS + 1) + '"}'),
                     "arguments_too_large")
        self.refused(call("observe_screen", '{"query": "' + "a" * 1_000_000 + '"}'), "arguments_too_large")
        self.refused({"message": {"role": "assistant", "content": self.MARKER + "a" * ma.MAX_TEXT_CHARS}},
                     "content_invalid")
        self.refused({"message": {"role": "assistant", "content": "ok",
                                  "reasoning_content": "a" * (ma.MAX_TEXT_CHARS + 1)}}, "reasoning_invalid")
        self.refused({"message": {"role": "assistant", "content": ["not", "text"]}}, "content_invalid")

    def test_a_deeply_nested_reply_or_argument_is_a_fixed_refusal_not_a_crash(self):
        deep = b'{"a": ' + b"[" * 200_000 + b"]" * 200_000 + b"}"
        self.refused({"raw": deep}, "reply_unreadable")
        self.refused(call("observe_screen", "[" * 7_000 + "]" * 7_000), "arguments_unreadable")
        # within the argument size limit and well inside any decoder's own recursion limit: the agent's bound decides
        self.refused(call("observe_screen", '{"query": ' + "[" * 100 + "]" * 100 + "}"), "arguments_unreadable")
        self.refused({"raw": b'{"a": ' + b"[" * 100 + b"]" * 100 + b"}"}, "reply_unreadable")

    def test_the_depth_is_measured_without_recursion_and_the_boundary_is_exact(self):
        ok = json.loads("[" * ma.MAX_JSON_DEPTH + "]" * ma.MAX_JSON_DEPTH)
        self.assertEqual(ma.json_depth(ok), ma.MAX_JSON_DEPTH)
        ma.strict_loads("[" * ma.MAX_JSON_DEPTH + "]" * ma.MAX_JSON_DEPTH, "t")
        with self.assertRaises(ma.AgentError):
            ma.strict_loads("[" * (ma.MAX_JSON_DEPTH + 1) + "]" * (ma.MAX_JSON_DEPTH + 1), "t")

    def test_a_large_call_list_is_refused_before_any_of_it_runs(self):
        message = call("get_engine_metrics", "{}")
        message["message"]["tool_calls"] *= 1000
        self.refused(message, "multiple_tool_calls")

    def test_a_reply_that_is_not_text_json_is_refused_without_echoing_it(self):
        self.refused({"raw": b"\xff\xfe" + self.MARKER.encode()}, "reply_unreadable")
        self.refused({"raw": b"[" + self.MARKER.encode().join([b'"', b'"']) + b"]"}, "reply_unreadable")


# ---- R6: the key path is checked where the file is opened ------------------------------------------------------

class R6KeyPathTests(unittest.TestCase):
    def checkout(self):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-checkout-")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        (root / ".git").mkdir()
        (root / "inner").mkdir()
        (root / "inner" / "k.key").write_text("sk-dummy-key-1\n", encoding="utf-8")
        return root

    def outside(self):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-outside-")
        self.addCleanup(tmp.cleanup)
        path = Path(tmp.name) / "k.key"
        path.write_text("sk-dummy-key-2\n", encoding="utf-8")
        return path

    def test_a_key_path_is_judged_by_the_runners_rules(self):
        inside, outside = self.checkout() / "inner" / "k.key", self.outside()
        self.assertEqual(ma.key_path_problem(inside), "key_path_inside_checkout")
        self.assertIsNone(ma.key_path_problem(outside))
        self.assertEqual(ma.key_path_problem(inside), "key_path_inside_checkout")
        for bad in ("k.key", "relative/k.key", str(outside.parent / ".." / "k.key"), "\\\\?\\C:\\k.key",
                    "//?/C:/k.key", "C:k.key"):
            self.assertIsNotNone(ma.key_path_problem(bad), bad)
        self.assertEqual(ma.key_path_problem(str(outside)) is None, proxy_agrees(outside))

    def test_load_key_refuses_before_reading(self):
        inside = self.checkout() / "inner" / "k.key"
        with self.assertRaises(ma.AgentError) as caught:
            ma.load_key(inside)
        self.assertEqual(caught.exception.token, "key_path_inside_checkout")
        self.assertEqual(ma.load_key(self.outside()), "sk-dummy-key-2")

    def test_the_agents_own_cli_refuses_a_key_in_a_checkout_and_starts_nothing(self):
        inside = self.checkout() / "inner" / "k.key"
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        received = Path(tmp.name) / "engine.in"
        mcp = Path(tmp.name) / "mcp.json"
        mcp.write_text(json.dumps(rg.build_mcp_config(sys.executable, ["--dry-run-only", "--log", str(Path(tmp.name) / "l")],
                                                      [sys.executable, str(FAKE_ENGINE), "--received", str(received)])),
                       encoding="utf-8")
        with fake_api.FakeApi() as api:
            done = subprocess.run(
                [sys.executable, "-I", str(agentcli.MINIMAL_AGENT), "--base-url", api.url, "--model", "m",
                 "--provider", "p", "--key-file", str(inside), "--mcp-config", str(mcp), "--system-prompt", "s"],
                input=b"goal", capture_output=True, timeout=60)
            self.assertEqual(api.requests, [])
        self.assertNotEqual(done.returncode, 0)
        self.assertIn(b"key_path_inside_checkout", done.stderr)
        self.assertFalse(received.exists(), "the bridge/engine must not have been started")
        self.assertNotIn(b"sk-dummy-key-1", done.stdout + done.stderr)

    def test_a_link_or_junction_into_a_checkout_is_followed_and_refused(self):
        root = self.checkout()
        link_parent = tempfile.TemporaryDirectory(prefix="ag02-link-")
        self.addCleanup(link_parent.cleanup)
        link = Path(link_parent.name) / "alias"
        try:
            os.symlink(root / "inner", link, target_is_directory=True)
        except (OSError, NotImplementedError):
            made = subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(root / "inner")],
                                  capture_output=True) if os.name == "nt" else None
            if made is None or made.returncode != 0:
                self.skipTest("neither a symbolic link nor a junction can be made here")
        self.assertEqual(ma.key_path_problem(link / "k.key"), "key_path_inside_checkout")

    def test_the_runner_still_refuses_the_same_path_and_the_helper_stays_in_the_standard_library(self):
        inside = self.checkout() / "inner" / "k.key"
        self.assertIsNotNone(rg.proxy_module.path_problem(inside))
        self.assertEqual(pc.static_problems(agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")), [])


def proxy_agrees(path) -> bool:
    return rg.proxy_module.path_problem(path) is None


# ---- R5: the proof binds what the requests carried ---------------------------------------------------------------

class R5ParameterValueTests(unittest.TestCase):
    EXTRA = {"reasoning_effort": "max", "thinking": {"type": "enabled"}}

    def check(self, requests, expected=None):
        return pc.check_chat_requests(
            requests, model="deepseek-flash", expected_extra=expected if expected is not None else self.EXTRA,
            ceiling=ma.DEFAULT_COMPLETION_CEILING, system_instruction=INSTRUCTION, prompt=PROMPT,
            sentinels={"k": SENTINEL}, dummy_key="sk-test-KEY-123456", auth_style="bearer", base_path="")

    def captured(self):
        return AgentRun(self, EndToEndTests.SCRIPT, provider="deepseek", model="deepseek-flash",
                        extra=self.EXTRA).requests

    def test_a_changed_value_is_seen_for_every_configured_parameter_and_for_the_ceiling(self):
        requests = self.captured()
        self.assertEqual(self.check(requests)[0], [])
        mutations = pc.control_mutations(requests[0], SENTINEL, self.EXTRA)
        for name in ("changed_value_reasoning_effort", "changed_value_thinking", "changed_ceiling"):
            self.assertIn("param_values_differ", self.check([mutations[name]])[0], name)

    def test_a_value_of_another_type_is_not_equal(self):
        body = json.loads(self.captured()[0]["body"])
        body["top_p"] = 1.0
        request = self.captured()[0] | {"body": json.dumps(body).encode()}
        body["top_p"] = 1
        as_int = self.captured()[0] | {"body": json.dumps(body).encode()}
        problems, _ = pc.check_chat_requests(
            [as_int], model="deepseek-flash", expected_extra={**self.EXTRA, "top_p": 1.0},
            ceiling=ma.DEFAULT_COMPLETION_CEILING, system_instruction=INSTRUCTION, prompt=PROMPT,
            sentinels={}, dummy_key="sk-test-KEY-123456", auth_style="bearer", base_path="")
        self.assertIn("param_values_differ", problems)

    def test_the_full_preflight_rejects_an_agent_that_sends_a_lower_effort_than_the_one_requested(self):
        source = agentcli.MINIMAL_AGENT.read_text(encoding="utf-8")
        old = '"max_tokens": args.completion_ceiling, **args.extra_param}'
        self.assertIn(old, source)
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "minimal_agent.py"
            copy.write_text(source.replace(
                old, '"max_tokens": args.completion_ceiling, **{**args.extra_param, "reasoning_effort": "low"}}'),
                encoding="utf-8")
            with mock.patch.object(agentcli, "MINIMAL_AGENT", copy):
                proof = run_preflight("deepseek", "deepseek-flash", effort="max")
        self.assertFalse(proof["passed"])
        self.assertIn("param_values_differ", proof["problems"])
        self.assertIn("param_hash_differs", proof["problems"])

    def test_a_good_proof_hashes_the_values_the_requests_carried(self):
        proof = run_preflight("deepseek", "deepseek-flash", effort="max")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["extra_param_sha256"], agentcli.canonical_sha(self.EXTRA))
        self.assertEqual(proof["completion_ceiling"], ma.DEFAULT_COMPLETION_CEILING)
        self.assertIn("changed_value_reasoning_effort", proof["control_detected"])
        self.assertIn("changed_value_thinking", proof["control_detected"])


# ---- the minimal agent and a Token Plan key --------------------------------------------------------------------------

class TokenPlanKeyTests(unittest.TestCase):
    def test_the_agent_itself_refuses_a_mimo_token_plan_key_and_makes_no_request(self):
        for key in ("tp-abc123", "ttp-abc123"):
            run = AgentRun(self, EndToEndTests.SCRIPT, provider="mimo", key=key)
            self.assertEqual(run.result["result"], "token_plan_key_not_for_scripts")
            self.assertEqual((run.requests, run.forwarded), ([], []))
            self.assertNotIn(key, stored_text(run))

    def test_a_token_plan_key_is_refused_whatever_provider_label_the_agent_is_started_with(self):
        for provider in ("deepseek", "mimo"):
            for key in ("tp-whatever", "ttp-whatever"):
                with self.subTest(provider=provider, key=key[:3]):
                    run = AgentRun(self, EndToEndTests.SCRIPT, provider=provider, key=key)
                    self.assertEqual(run.result["result"], "token_plan_key_not_for_scripts")
                    self.assertEqual((run.requests, run.forwarded), ([], []))

    def test_ordinary_keys_are_not_caught_by_that_rule(self):
        for provider in ("deepseek", "mimo"):
            self.assertEqual(AgentRun(self, EndToEndTests.SCRIPT, provider=provider, key="sk-ordinary").result["subtype"],
                             "success")

    def test_the_runner_refuses_before_starting_and_names_the_route_instead(self):
        with tempfile.TemporaryDirectory() as tmp:
            key = Path(tmp) / "mimo.key"
            key.write_text("tp-secret-value\n", encoding="utf-8")
            cli = agentcli.MinimalCLI(argparse.Namespace(provider="mimo", key_file=key))
            problem = cli.key_problem()
            self.assertIn("--route mimo", problem)
            self.assertNotIn("tp-secret-value", problem)
            key.write_text("sk-ordinary\n", encoding="utf-8")
            self.assertIsNone(cli.key_problem())
            self.assertIsNone(agentcli.MinimalCLI(argparse.Namespace(provider="deepseek", key_file=key)).key_problem())
            for provider in ("deepseek", "mimo", "whatever"):
                for prefix in ("tp-", "ttp-"):
                    key.write_text(prefix + "secret-value\n", encoding="utf-8")
                    problem = agentcli.MinimalCLI(argparse.Namespace(provider=provider, key_file=key)).key_problem()
                    self.assertIn("--route mimo", problem, (provider, prefix))
                    self.assertNotIn("secret-value", problem)


# ---- the Claude Code route ------------------------------------------------------------------------------------------------

MIMO_URL = "https://token-plan-sgp.xiaomimimo.com/anthropic"
DEEPSEEK_URL = "https://api.deepseek.com/anthropic"


class RouteEnvironmentTests(unittest.TestCase):
    def test_the_child_gets_the_basics_and_the_route_variables_and_nothing_of_the_operators(self):
        hostile = {"ANTHROPIC_API_KEY": "operators-key", "ANTHROPIC_BASE_URL": "http://evil", "AWS_PROFILE": "prod",
                   "OPENAI_API_KEY": "x", "HTTPS_PROXY": "http://evil", "CLAUDE_CODE_USE_BEDROCK": "1",
                   "OTEL_EXPORTER_OTLP_ENDPOINT": "http://evil", "CLAUDE_CODE_OAUTH_TOKEN": "oauth"}
        with mock.patch.dict(os.environ, hostile):
            env = agentcli.route_env("mimo", MIMO_URL, "tp-route-token", "mimo-v2.6-flash", "/cfg")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], MIMO_URL)
        self.assertEqual((env["ANTHROPIC_AUTH_TOKEN"], env["ANTHROPIC_API_KEY"]), ("tp-route-token",) * 2)
        self.assertEqual(env["CLAUDE_CODE_EFFORT_LEVEL"], "max")
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_SMALL_FAST_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL",
                     "ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL"):
            self.assertEqual(env[name], "mimo-v2.6-flash", name)
        self.assertTrue(set(hostile) - {"ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL"} <= set(hostile))
        for leaked in ("AWS_PROFILE", "OPENAI_API_KEY", "HTTPS_PROXY", "CLAUDE_CODE_USE_BEDROCK",
                       "OTEL_EXPORTER_OTLP_ENDPOINT", "CLAUDE_CODE_OAUTH_TOKEN"):
            self.assertNotIn(leaked, env)
        self.assertNotIn("operators-key", json.dumps(env))
        self.assertEqual(agentcli.route_env_problems(env, "mimo", agentcli.CLAUDE_ROUTES["mimo"]["base_urls"]), [])

    def test_only_the_harness_controlled_route_is_allowed_by_the_routing_refusal(self):
        urls = agentcli.CLAUDE_ROUTES["mimo"]["base_urls"]
        env = agentcli.route_env("mimo", MIMO_URL, "t", "m", "/cfg")
        self.assertEqual(agentcli.route_env_problems(env, "mimo", urls), [])
        self.assertIn("route_base_url_not_allowed", agentcli.route_env_problems(
            env | {"ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/anthropic"}, "mimo", urls))
        self.assertIn("routing_variables_differ", agentcli.route_env_problems(
            env | {"CLAUDE_CODE_USE_BEDROCK": "1"}, "mimo", urls))
        self.assertIn("routing_variables_differ", agentcli.route_env_problems(
            env | {"ANTHROPIC_CUSTOM_HEADERS": "x: y"}, "mimo", urls))
        self.assertIn("route_extra_variable", agentcli.route_env_problems(env | {"ANTHROPIC_FOO": "1"}, "mimo", urls))
        without = {k: v for k, v in env.items() if k != "ANTHROPIC_BASE_URL"}
        self.assertTrue(agentcli.route_env_problems(without, "mimo", urls))
        self.assertEqual(agentcli.route_env_problems(
            env | {"ANTHROPIC_BASE_URL": "http://127.0.0.1:5000"}, "mimo", urls), [])

    def test_a_token_plan_key_goes_only_with_a_token_plan_host_and_the_other_way_round(self):
        self.assertIsNone(agentcli.route_key_problem("mimo", MIMO_URL, "tp-x"))
        self.assertIsNone(agentcli.route_key_problem("mimo", "https://token-plan-cn.xiaomimimo.com/anthropic", "ttp-x"))
        self.assertIn("pay-as-you-go", agentcli.route_key_problem("mimo", "https://api.xiaomimimo.com/anthropic", "tp-x"))
        self.assertIn("Token Plan key", agentcli.route_key_problem("mimo", MIMO_URL, "sk-x"))
        self.assertIsNone(agentcli.route_key_problem("deepseek", DEEPSEEK_URL, "sk-x"))
        # a Token Plan key belongs to the MiMo route only: a wrong key file must not reach another provider
        for key in ("tp-looks-like-a-plan-key", "ttp-team-key"):
            problem = agentcli.route_key_problem("deepseek", DEEPSEEK_URL, key)
            self.assertIn("MiMo Token Plan route only", problem, key)
            self.assertNotIn(key, problem)
        self.assertIn("MiMo Token Plan route only", agentcli.route_key_problem(
            "deepseek", "https://token-plan-sgp.xiaomimimo.com/anthropic", "tp-x"))

    def test_the_routes_are_pinned_to_the_owners_models_and_endpoints(self):
        self.assertEqual(agentcli.CLAUDE_ROUTES["mimo"]["model"], "mimo-v2.6-flash")
        self.assertEqual(agentcli.CLAUDE_ROUTES["deepseek"]["model"], "deepseek-flash")
        self.assertEqual(agentcli.CLAUDE_ROUTES["deepseek"]["base_urls"], (DEEPSEEK_URL,))
        self.assertIn(MIMO_URL, agentcli.CLAUDE_ROUTES["mimo"]["base_urls"])
        for spec in agentcli.CLAUDE_ROUTES.values():
            self.assertEqual(spec["effort"], "max")
            self.assertTrue(all(u.startswith("https://") and u.endswith("/anthropic") for u in spec["base_urls"]))
        self.assertNotIn("https://api.xiaomimimo.com/anthropic", agentcli.CLAUDE_ROUTES["mimo"]["base_urls"])


class RouteRunnerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ag02-route-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.key = self.root / "mimo.key"
        self.key.write_text("tp-secret-route-key\n", encoding="utf-8")
        self.cfg = self.root / "cfg"
        self.cfg.mkdir()
        stderr = silence()
        self.addCleanup(stderr.__exit__, None, None, None)

    def parse(self, *extra, route="mimo", url=MIMO_URL, plan=True):
        argv = ["--agent-cli", "claude", "--route", route, "--route-base-url", url, "--key-file", str(self.key),
                "--claude-config-dir", str(self.cfg), "--isolation-proof", str(self.root / "p.json")]
        return rg.parse_args((["--plan"] if plan else ["--model", "m", "--exe", "x"]) + argv + list(extra))

    def test_the_base_url_is_required_explicit_and_from_the_closed_list(self):
        args = self.parse()
        self.assertIsNone(agentcli.make(args).refusal())
        for url in ("https://api.xiaomimimo.com/anthropic", "https://evil.example/anthropic", MIMO_URL + "/"):
            cli = agentcli.make(self.parse(url=url))
            self.assertIn("--route-base-url is required and must be one of", cli.refusal(), url)
        silent = rg.parse_args(["--plan", "--agent-cli", "claude", "--route", "mimo", "--key-file", str(self.key)])
        self.assertIn("is required", agentcli.make(silent).refusal())
        with self.assertRaises(SystemExit):
            rg.parse_args(["--agent-cli", "claude", "--route", "mimo", "--model", "m", "--exe", "x"])
        with self.assertRaises(SystemExit):
            rg.parse_args(["--plan", "--agent-cli", "minimal", "--provider", "mimo", "--route", "mimo"])
        with self.assertRaises(SystemExit):
            rg.parse_args(["--plan", "--route-base-url", MIMO_URL])

    def test_a_plan_shows_the_command_and_never_the_key(self):
        args = self.parse()
        plan = rg.plan_lines(args, rg.load_goals(), rg.select_goals(rg.load_goals(), ["G01"]), self.root / "out")
        text = json.dumps(plan)
        self.assertNotIn("tp-secret-route-key", text)
        self.assertIn("--setting-sources", text)
        cli = agentcli.make(args)
        self.assertNotIn("tp-secret-route-key", json.dumps(cli.provenance()))
        self.assertEqual(cli.provenance(), {"cli": "claude-code", "provider": "mimo",
                                            "route_host": "token-plan-sgp.xiaomimimo.com",
                                            "max_context_tokens_per_goal": agentcli.ROUTE_CONTEXT_TOKENS_DEFAULT,
                                            "dollar_cap": "none"})

    def test_the_key_is_read_from_the_file_only_into_the_childs_environment(self):
        args = self.parse(plan=False)
        args.model = "mimo-v2.6-flash"
        cli = agentcli.make(args)
        self.assertIsNone(cli.key_problem())
        with mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": "operators-key"}):
            env = cli.env({"CLAUDE_CONFIG_DIR": str(self.cfg)})
        self.assertEqual((env["ANTHROPIC_AUTH_TOKEN"], env["CLAUDE_CONFIG_DIR"]), ("tp-secret-route-key", str(self.cfg)))
        self.assertNotIn("operators-key", json.dumps(env))
        argv = cli.build_argv(model=args.model, mcp_config=self.root / "m.json", system_instruction="i", max_budget_usd=1)
        self.assertNotIn("tp-secret-route-key", " ".join(argv))
        # the route's command is the native one without the dollar cap (see test_routecost.py)
        self.assertEqual(argv, rg.build_claude_argv(claude="claude", model=args.model, mcp_config=self.root / "m.json",
                                                    system_instruction="i", max_budget_usd=None))

    def test_a_token_plan_key_with_a_non_token_plan_url_or_a_missing_key_file_is_refused(self):
        deepseek = self.parse(route="deepseek", url=DEEPSEEK_URL, plan=False)
        deepseek.model = "deepseek-flash"
        self.key.write_text("sk-deepseek\n", encoding="utf-8")
        self.assertIsNone(agentcli.make(deepseek).key_problem())
        for key in ("tp-secret-route-key", "ttp-team-secret"):  # the MiMo key file given to the DeepSeek route
            self.key.write_text(key + "\n", encoding="utf-8")
            cli = agentcli.make(deepseek)
            self.assertIn("MiMo Token Plan route only", cli.key_problem())
        self.key.write_text("sk-deepseek\n", encoding="utf-8")
        mimo = self.parse(plan=False)
        self.assertIn("Token Plan key", agentcli.make(mimo).key_problem())  # an ordinary key on a Token Plan host
        self.key.unlink()
        with self.assertRaises(SystemExit):
            agentcli.make(mimo).key_problem()

    def test_the_operators_own_routing_variables_are_still_refused_for_a_route_run(self):
        with mock.patch.dict(os.environ, {"ANTHROPIC_BASE_URL": "https://elsewhere.example"}):
            self.assertEqual(rg.routing_problems(os.environ), ["ANTHROPIC_BASE_URL"])
            self.assertTrue(agentcli.make(self.parse()).checks_anthropic_routing)

    def route_proof(self, **changes):
        att = rg.init_attestation({"type": "system", "subtype": "init", "claude_code_version": "2.1.303",
                                   "model": "mimo-v2.6-flash", "tools": list(rg.ALLOWED_TOOL_NAMES),
                                   "mcp_servers": [{"name": "ommatix", "status": "connected"}], "skills": [],
                                   "slash_commands": [], "plugins": [], "agents": [], "memory_paths": {},
                                   "additional_directories": []})
        proof = {"schema": rg.PROOF_SCHEMA, "verifier_revision": rg.VERIFIER_REVISION, "passed": True,
                 "claude_code_version": "2.1.303", "model": "mimo-v2.6-flash", "requested_model": "mimo-v2.6-flash",
                 "tools": THREE, "plugins": [], "agents": [], "plugin_hashes": [], "agent_hashes": [],
                 "init_keys_sha256": att["init_keys_sha256"], "config_identity": rg.config_identity(self.cfg),
                 "context_manifest": rg.config_manifest(self.cfg), "canary_kinds": ["user_claude_md"],
                 "control_leaked_kinds": ["user_claude_md"], "date_utc": "2026-10-18T00:00:00Z",
                 "route": {"provider": "mimo", "base_url": MIMO_URL, "model": "mimo-v2.6-flash", "effort": "max",
                           "auth": "bearer"},
                 "route_verifier_revision": agentcli.ROUTE_VERIFIER_REVISION, "request_tools": THREE}
        return proof | changes

    def test_a_route_proof_binds_the_route_and_is_not_a_native_proof_nor_the_other_way_round(self):
        args = self.parse(plan=False)
        args.model = "mimo-v2.6-flash"
        cli = agentcli.make(args)
        path = self.root / "p.json"
        path.write_text(json.dumps(self.route_proof()), encoding="utf-8")
        proof = cli.load_proof(path)
        self.assertEqual(cli.proof_problems(proof, args, self.cfg, "i"), [])
        other = self.parse(url="https://token-plan-cn.xiaomimimo.com/anthropic", plan=False)
        other.model = "mimo-v2.6-flash"
        self.assertIn("route_differs_from_proof", agentcli.make(other).proof_problems(proof, other, self.cfg, "i"))
        args.model = "mimo-v2.6-pro"
        self.assertIn("route_differs_from_proof", cli.proof_problems(proof, args, self.cfg, "i"))
        native = agentcli.ClaudeCLI(rg.parse_args(["--plan"]))
        self.assertIn("route_proof_for_a_native_run", native.proof_problems(proof, args, self.cfg, "i"))
        for broken in ({"route_verifier_revision": 0}, {"route_verifier_revision": 1}, {"request_tools": THREE[:2]},
                       {"route": None}):
            path.write_text(json.dumps(self.route_proof(**broken)), encoding="utf-8")
            with self.assertRaises(SystemExit):
                cli.load_proof(path)

    def test_a_route_smoke_runs_only_under_a_proof_of_this_route(self):
        stderr = silence()
        self.addCleanup(stderr.__exit__, None, None, None)
        with self.assertRaises(SystemExit):
            rg.parse_args(["--smoke", "--agent-cli", "claude", "--route", "mimo", "--route-base-url", MIMO_URL,
                           "--key-file", str(self.key), "--model", "m", "--exe", "x"])
        args = rg.parse_args(["--smoke", "--agent-cli", "claude", "--route", "mimo", "--route-base-url", MIMO_URL,
                              "--key-file", str(self.key), "--model", "mimo-v2.6-flash", "--exe", "x",
                              "--claude-config-dir", str(self.cfg), "--isolation-proof", str(self.root / "p.json")])
        self.assertEqual((args.phase, args.engine_source), (1, "sim"))


def route_requests(model="mimo-v2.6-flash", token="tp-canary"):
    body = {"model": model, "stream": True, "thinking": {"type": "adaptive"}, "output_config": {"effort": "max"},
            "tools": [{"name": n} for n in THREE],
            "messages": [{"role": "user", "content": "hello"}], "system": [{"type": "text", "text": "s"}]}
    headers = {"authorization": f"Bearer {token}", "x-api-key": token, "content-type": "application/json"}
    return [{"path": "/v1/messages", "headers": headers, "body": json.dumps(body).encode()}]


class RouteRequestVerdictTests(unittest.TestCase):
    def check(self, requests, **kw):
        base = dict(model="mimo-v2.6-flash", token="tp-canary", effort="max")
        return preflight.check_route_requests(requests, **(base | kw))

    def test_a_clean_request_passes_and_reports_what_it_carried(self):
        problems, facts = self.check(route_requests())
        self.assertEqual(problems, [])
        self.assertEqual((facts["request_tools"], facts["request_models"], facts["request_effort"],
                          facts["request_thinking"]), (THREE, ["mimo-v2.6-flash"], ["max"], ["adaptive"]))

    def test_every_deviation_is_seen(self):
        mutations = preflight.route_control_mutations(route_requests()[0], "tp-canary")
        expected = {"extra_tool": "tools_not_the_three", "dropped_tool": "tools_not_the_three",
                    "other_model": "request_model_differs", "image_block": "image_in_request",
                    "x_api_key_other_value": "x_api_key_header_differs", "other_effort": "effort_differs",
                    "token_in_body": "key_in_body", "token_in_header": "key_in_another_header",
                    "cookie": "header_not_expected", "proxy_authorization": "header_not_expected",
                    "other_api_key_header": "header_not_expected", "extra_auth_token_header": "header_not_expected"}
        self.assertEqual(sorted(mutations), sorted(expected))
        for name, request in mutations.items():
            self.assertIn(expected[name], self.check([request])[0], name)

    def test_only_the_reviewed_header_names_pass_whatever_their_values(self):
        request = route_requests()[0]
        for name, value in (("cookie", "a=b"), ("proxy-authorization", "Basic eHg6eXk="), ("x-other-api-key", "k"),
                            ("x-auth-token", "t"), ("x-goog-api-key", "k"), ("api-key", "k"), ("x-extra", "v")):
            with self.subTest(name=name):
                copy = request | {"headers": request["headers"] | {name: value}}
                self.assertIn("header_not_expected", self.check([copy])[0])
        problems, facts = self.check([request])
        self.assertEqual(problems, [])
        self.assertTrue(set(facts["request_header_names"]) <= preflight.ROUTE_ALLOWED_HEADERS)
        for name in ("authorization", "x-api-key", "x-stainless-os", "x-claude-code-session-id"):
            self.assertIn(name, preflight.ROUTE_ALLOWED_HEADERS)
        self.assertTrue(not any("cookie" in n or "proxy" in n for n in preflight.ROUTE_ALLOWED_HEADERS))

    def test_no_request_no_tools_and_a_wrong_credential_are_failures(self):
        self.assertEqual(self.check([])[0], ["no_request_captured"])
        body = json.loads(route_requests()[0]["body"])
        del body["tools"]
        bare = route_requests()[0] | {"body": json.dumps(body).encode()}
        self.assertIn("no_request_offers_tools", self.check([bare])[0])
        wrong = route_requests()[0]
        wrong = wrong | {"headers": wrong["headers"] | {"authorization": "Bearer other"}}
        self.assertIn("auth_header_differs", self.check([wrong])[0])
        self.assertIn("request_path_differs", self.check([route_requests()[0] | {"path": "/v1/other"}])[0])

    def test_a_token_behind_a_json_escape_is_seen_too(self):
        request = route_requests()[0]
        text = request["body"].decode().replace('"hello"', '"tp-\\u0063anary"')
        self.assertNotIn(b"tp-canary", text.encode())
        self.assertIn("key_in_body", self.check([request | {"body": text.encode()}])[0])


class RouteVerifierRevisionTests(unittest.TestCase):
    def test_a_stricter_route_verifier_invalidates_the_route_proofs_of_the_weaker_one(self):
        self.assertGreaterEqual(agentcli.ROUTE_VERIFIER_REVISION, 2)
        self.assertEqual(rg.VERIFIER_REVISION, 3, "the native ADR-0036 verifier revision is not touched")
        self.assertEqual(rg.PROOF_SCHEMA, "ag02-isolation-proof-3")


class RouteSmokeStderrTests(unittest.TestCase):
    """N1: a route smoke never persists the child's stderr, which can echo the credential or a response."""
    TOKEN = "tp-dummy-secret-route-token-0001"
    MARKER = "SYNTHETIC-RESPONSE-MARKER-7"

    FAKE_CLI = """import json, os, sys
sys.stdin.read()
print(open(sys.argv[1], encoding="utf-8").read(), flush=True)
sys.stderr.write("provider error: bad credential " + os.environ["ANTHROPIC_AUTH_TOKEN"] + " " + sys.argv[2] + "\\n")
sys.stderr.flush()
print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": "VERDICT: DONE",
                  "num_turns": 1}), flush=True)
"""

    def test_a_failing_provider_that_echoes_the_credential_leaves_it_in_no_file(self):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-smoke-")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        cfg, out = root / "cfg", root / "out"
        cfg.mkdir()
        key = root / "mimo.key"
        key.write_text(self.TOKEN + "\n", encoding="utf-8")
        init = {"type": "system", "subtype": "init", "claude_code_version": "2.1.303", "model": "mimo-v2.6-flash",
                "tools": list(rg.ALLOWED_TOOL_NAMES), "mcp_servers": [{"name": "ommatix", "status": "connected"}],
                "skills": [], "slash_commands": [], "plugins": [], "agents": [], "memory_paths": {},
                "additional_directories": []}
        (root / "init.json").write_text(json.dumps(init), encoding="utf-8")
        (root / "fake_cli.py").write_text(self.FAKE_CLI, encoding="utf-8")
        att = rg.init_attestation(init)
        proof = {"schema": rg.PROOF_SCHEMA, "verifier_revision": rg.VERIFIER_REVISION, "passed": True,
                 "claude_code_version": "2.1.303", "model": "mimo-v2.6-flash", "requested_model": "mimo-v2.6-flash",
                 "tools": THREE, "plugins": [], "agents": [], "plugin_hashes": [], "agent_hashes": [],
                 "init_keys_sha256": att["init_keys_sha256"], "config_identity": rg.config_identity(cfg),
                 "context_manifest": rg.config_manifest(cfg), "canary_kinds": ["user_claude_md"],
                 "control_leaked_kinds": ["user_claude_md"], "date_utc": "2026-10-18T00:00:00Z",
                 "route": {"provider": "mimo", "base_url": MIMO_URL, "model": "mimo-v2.6-flash", "effort": "max",
                           "auth": "bearer"},
                 "route_verifier_revision": agentcli.ROUTE_VERIFIER_REVISION, "request_tools": THREE}
        (root / "proof.json").write_text(json.dumps(proof), encoding="utf-8")
        fake = [sys.executable, str(root / "fake_cli.py"), str(root / "init.json"), self.MARKER]
        stdout = io.StringIO()
        with mock.patch.object(rg, "build_claude_argv", return_value=fake), contextlib.redirect_stdout(stdout):
            code = rg.main(["--smoke", "--agent-cli", "claude", "--route", "mimo", "--route-base-url", MIMO_URL,
                            "--key-file", str(key), "--claude-config-dir", str(cfg),
                            "--isolation-proof", str(root / "proof.json"), "--model", "mimo-v2.6-flash",
                            "--exe", str(root / "ommatix.exe"), "--out", str(out)])
        self.assertEqual(code, 0)
        files = [f for f in root.rglob("*") if f.is_file() and f != key]
        self.assertTrue(files)
        for f in files:
            data = f.read_bytes()
            self.assertNotIn(self.TOKEN.encode(), data, f.name)
            self.assertNotIn(self.MARKER.encode(), data, f.name)
        self.assertEqual([f.name for f in files if "stderr" in f.name], [])
        smoke = json.loads(next(out.rglob("smoke.json")).read_text(encoding="utf-8"))
        self.assertEqual((smoke["metrics"]["verdict"], smoke["metrics"]["init_problems"], smoke["cli"]["exit_code"]),
                         ("DONE", [], 0))
        self.assertEqual(smoke["provenance"]["provider"], "mimo")
        self.assertNotIn(self.TOKEN, stdout.getvalue())

    def test_the_production_runner_never_asks_for_a_stderr_file(self):
        source = (HERE / "run_goal.py").read_text(encoding="utf-8")
        self.assertNotIn("smoke.stderr", source)


class RoutePreflightTests(unittest.TestCase):
    """The route preflight end to end with the scripted ``fake_claude.py``; the real Claude Code is run
    against the same stand-in by the owner-facing preflight (recorded in the review)."""

    def run_route(self, mode="normal", route="mimo", url=MIMO_URL):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-routepf-")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        cfg = root / "cfg"
        cfg.mkdir()
        args = argparse.Namespace(
            exe=Path(sys.executable), model=agentcli.CLAUDE_ROUTES[route]["model"],
            claude=[sys.executable, str(HERE / "fake_claude.py")], timeout=60, claude_config_dir=cfg,
            route=route, route_base_url=url,
            extra_env={"FAKE_CLAUDE_MODE": mode, "FAKE_CLAUDE_LEAK_DIR": str(cfg)})
        with mock.patch.object(preflight, "refusals", return_value=[]):
            return preflight.run(args)

    def test_a_confined_route_session_passes_and_the_proof_carries_the_route(self):
        for route, url in (("mimo", MIMO_URL), ("deepseek", DEEPSEEK_URL)):
            proof = self.run_route(route=route, url=url)
            self.assertTrue(proof["passed"], proof["problems"])
            self.assertEqual(proof["route"], {"provider": route, "base_url": url,
                                              "model": agentcli.CLAUDE_ROUTES[route]["model"], "effort": "max",
                                              "auth": "bearer"})
            self.assertEqual(proof["request_tools"], THREE)
            self.assertEqual(proof["route_control_blind"], [])
            self.assertEqual(len(proof["route_control_detected"]), 12)
            self.assertTrue(set(proof["request_header_names"]) <= preflight.ROUTE_ALLOWED_HEADERS)
            self.assertEqual(proof["route_verifier_revision"], agentcli.ROUTE_VERIFIER_REVISION)
            self.assertNotIn("tp-canary", json.dumps({k: v for k, v in proof.items() if k != "differences_from_production"}))

    def test_each_deviation_of_the_route_session_fails_the_preflight(self):
        expected = {"route_extra_tool": "route_tools_not_the_three", "route_other_model": "route_request_model_differs",
                    "route_image": "route_image_in_request", "route_key_in_body": "route_key_in_body",
                    "route_bad_effort": "route_effort_differs", "route_x_api_key": "route_x_api_key_header_differs",
                    "route_proxy_authorization": "route_header_not_expected", "route_cookie": "route_header_not_expected",
                    "extra_tool": "init_tools_differ"}
        for mode, token in expected.items():
            with self.subTest(mode=mode):
                proof = self.run_route(mode)
                self.assertFalse(proof["passed"])
                self.assertIn(token, proof["problems"] + [f"init_{p}" for p in proof["problems"]])

    def test_the_proof_is_accepted_by_the_routes_loader_and_a_native_proof_is_not(self):
        proof = self.run_route()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "p.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            args = rg.parse_args(["--plan", "--agent-cli", "claude", "--route", "mimo", "--route-base-url", MIMO_URL,
                                  "--key-file", str(path)])
            self.assertEqual(agentcli.make(args).load_proof(path)["route"]["provider"], "mimo")
            native = {k: v for k, v in proof.items() if k not in ("route", "route_verifier_revision", "request_tools")}
            path.write_text(json.dumps(native), encoding="utf-8")
            with self.assertRaises(SystemExit):
                agentcli.make(args).load_proof(path)

    def test_the_cli_requires_an_explicit_endpoint_from_the_list(self):
        stderr = silence()
        self.addCleanup(stderr.__exit__, None, None, None)
        for argv in (["--route", "mimo"], ["--route", "mimo", "--route-base-url", "https://api.xiaomimimo.com/anthropic"],
                     ["--route-base-url", MIMO_URL]):
            with self.assertRaises(SystemExit):
                preflight.main(["--exe", "x", "--claude-config-dir", tempfile.gettempdir(), "--out",
                                str(Path(tempfile.gettempdir()) / "x.json"), *argv])


if __name__ == "__main__":
    unittest.main()
