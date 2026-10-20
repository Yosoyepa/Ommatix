"""Tests of the AG-02 proxy's observation evidence (PE-13, PE-14 of the editorial review of paper v0.4).

The log must let a reader tell a filtered or paginated observation from a poor one, and tell the
engine's own time from the rest of a round trip, without ever holding what the agent asked for or
what is on the screen: booleans, bounded numbers and closed names only (ADR-0012, ADR-0036).
"""

import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import proxy  # noqa: E402
from test_proxy import Session, call, action, dry  # noqa: E402

CANARY = "PRIVATE-SCREEN-CANARY"


def observe(args):
    return call(1, "observe_screen", args)


def timings(**over):
    base = {"capture_us": 1, "plan_us": 2, "detect_us": 3, "recognize_us": 4, "accessibility_us": 5,
            "fuse_us": 6, "track_us": 7, "commit_us": 8, "total_us": 36, "response_build_us": 9}
    return {**base, **over}


def plan(**over):
    base = {"work": "regions", "full_refresh_reason": None, "region_count": 2, "region_area_px": 1200,
            "inference_count": 2, "input_shapes": None, "accessibility_truncated": False,
            "controls_n": 10, "words_n": 20, "supports_n": 3, "relations_n": 4}
    return {**base, **over}


def observe_reply(*, elements=3, total=3, truncated=False, meta=None, extra=None):
    result = {"isError": False, "structuredContent": {
        "elements": [{"target_id": f"e{i}", "label": CANARY} for i in range(elements)],
        "total_elements": total, "truncated": truncated, **(extra or {})}}
    if meta is not None:
        result["_meta"] = meta
    return {"jsonrpc": "2.0", "id": 1, "result": result}


class RequestEvidence(unittest.TestCase):
    def test_an_observation_request_logs_only_whether_it_filtered_and_how_much_it_asked_for(self):
        meta = proxy.request_meta(observe({"query": CANARY, "limit": 25, "cursor": CANARY + "-CURSOR"}))
        self.assertEqual((meta["query_present"], meta["limit"], meta["cursor_present"]), (True, 25, True))
        self.assertNotIn(CANARY, json.dumps(meta))

    def test_a_bare_observation_request_says_it_had_no_arguments(self):
        for args in ({}, None, [], "x"):
            meta = proxy.request_meta(observe(args))
            self.assertEqual((meta["query_present"], meta["cursor_present"]), (False, False), args)
            self.assertNotIn("limit", meta)

    def test_an_empty_query_is_not_a_filter(self):
        self.assertFalse(proxy.request_meta(observe({"query": ""}))["query_present"])
        self.assertFalse(proxy.request_meta(observe({"query": 7}))["query_present"])

    def test_a_limit_outside_the_contract_is_not_logged_as_a_number(self):
        for bad in (0, -1, 201, True, 2.5, "5", None, [5]):
            self.assertNotIn("limit", proxy.request_meta(observe({"limit": bad})), bad)
        for good in (1, 100, 200):
            self.assertEqual(proxy.request_meta(observe({"limit": good}))["limit"], good)

    def test_only_observation_requests_carry_the_observation_fields(self):
        for message in (action(1), call(1, "get_engine_metrics", {}), call(1, "no_such_tool", {"query": "x"})):
            meta = proxy.request_meta(message)
            for key in ("query_present", "limit", "cursor_present"):
                self.assertNotIn(key, meta)


class ResponseEvidence(unittest.TestCase):
    def test_an_observation_response_logs_truncation_and_the_total(self):
        meta = proxy.response_meta(observe_reply(elements=2, total=75, truncated=True), "observe_screen")
        self.assertEqual((meta["elements"], meta["total_elements"], meta["truncated"]), (2, 75, True))

    def test_values_the_contract_does_not_allow_are_dropped(self):
        for bad_total in (-1, True, 1.5, "7", None):
            meta = proxy.response_meta(observe_reply(total=bad_total), "observe_screen")
            self.assertNotIn("total_elements", meta, bad_total)
        meta = proxy.response_meta(observe_reply(truncated="yes"), "observe_screen")
        self.assertNotIn("truncated", meta)

    def test_the_stage_timings_and_the_plan_are_kept_as_numbers_and_closed_names(self):
        meta = proxy.response_meta(observe_reply(meta={
            "io.ommatix/timings": timings(), "io.ommatix/plan": plan(
                work="full", full_refresh_reason="adaptive_cost", input_shapes=[{"w": 1}, {"w": 2}])}),
            "observe_screen")
        self.assertEqual(meta["timings_us"], timings())
        self.assertEqual(meta["plan"]["work"], "full")
        self.assertEqual(meta["plan"]["full_refresh_reason"], "adaptive_cost")
        self.assertEqual(meta["plan"]["input_shapes_n"], 2)
        self.assertEqual(meta["plan"]["region_count"], 2)
        self.assertIs(meta["plan"]["accessibility_truncated"], False)
        self.assertNotIn("input_shapes", meta["plan"])

    def test_a_null_timing_stays_null_and_a_plan_without_shapes_has_no_count(self):
        meta = proxy.response_meta(observe_reply(meta={
            "io.ommatix/timings": timings(response_build_us=None),
            "io.ommatix/plan": plan()}), "observe_screen")
        self.assertIsNone(meta["timings_us"]["response_build_us"])
        self.assertIsNone(meta["plan"]["full_refresh_reason"])
        self.assertIsNone(meta["plan"]["input_shapes_n"])

    def test_names_outside_the_closed_lists_never_reach_the_log(self):
        meta = proxy.response_meta(observe_reply(meta={
            "io.ommatix/timings": {**timings(), CANARY + "_us": 5, "capture_us": CANARY},
            "io.ommatix/plan": plan(work=CANARY, full_refresh_reason=CANARY, **{CANARY: 1})}),
            "observe_screen")
        self.assertNotIn(CANARY, json.dumps(meta))
        self.assertEqual(meta["plan"]["work"], "other")
        self.assertEqual(meta["plan"]["full_refresh_reason"], "other")
        self.assertIsNone(meta["timings_us"]["capture_us"])
        self.assertNotIn(CANARY + "_us", meta["timings_us"])

    def test_numbers_the_engine_could_not_have_sent_are_dropped(self):
        meta = proxy.response_meta(observe_reply(meta={
            "io.ommatix/timings": timings(capture_us=-1, plan_us=True, detect_us=1.5, fuse_us="3"),
            "io.ommatix/plan": plan(region_count=-4, controls_n=True, words_n=2**70)}), "observe_screen")
        for key in ("capture_us", "plan_us", "detect_us", "fuse_us"):
            self.assertIsNone(meta["timings_us"][key], key)
        for key in ("region_count", "controls_n", "words_n"):
            self.assertIsNone(meta["plan"][key], key)

    def test_a_response_without_meta_or_with_a_malformed_one_logs_no_timings(self):
        for meta in (None, [], "x", {"io.ommatix/timings": [1], "io.ommatix/plan": "x"}):
            out = proxy.response_meta(observe_reply(meta=meta), "observe_screen")
            self.assertNotIn("timings_us", out)
            self.assertNotIn("plan", out)

    def test_other_tools_do_not_carry_observation_fields(self):
        reply = observe_reply(meta={"io.ommatix/timings": timings()})
        meta = proxy.response_meta(reply, "execute_action")
        for key in ("elements", "total_elements", "truncated", "timings_us", "plan"):
            self.assertNotIn(key, meta)

    def test_an_error_response_has_none_of_them(self):
        meta = proxy.response_meta({"error": {"code": -32602}}, "observe_screen")
        self.assertEqual(meta, {"kind": "response", "rpc_error_code": -32602})


class RequestIndex(unittest.TestCase):
    def test_every_request_and_its_response_share_an_index_and_observations_are_counted(self):
        s = Session(self, dry, [
            {"jsonrpc": "2.0", "id": 10, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            call(11, "observe_screen", {"query": "x"}),
            action(12, dry_run=True),
            call(13, "observe_screen", {}),
        ])
        sent = [r for r in s.records if r["dir"] == "agent->engine" and r["kind"] == "request"]
        got = [r for r in s.records if r["dir"] == "engine->agent" and "request_index" in r]
        self.assertEqual([r["request_index"] for r in sent], [1, 2, 3, 4])
        self.assertEqual([r["request_index"] for r in got], [1, 2, 3, 4])
        self.assertEqual([r.get("observe_index") for r in sent], [None, 1, None, 2])
        self.assertEqual([r.get("observe_index") for r in got if r.get("tool") == "observe_screen"], [1, 2])

    def test_a_notification_has_no_index_and_a_refusal_does_not_use_one_up(self):
        s = Session(self, dry, [
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            action(1, dry_run=False),  # refused in phase 1: never forwarded
            call(2, "observe_screen", {}),
        ])
        note = next(r for r in s.records if r.get("method") == "notifications/initialized")
        self.assertNotIn("request_index", note)
        sent = [r for r in s.records if r["dir"] == "agent->engine" and r.get("tool") == "observe_screen"]
        self.assertEqual(sent[0]["request_index"], 1)

    def test_the_session_log_holds_the_new_fields_and_still_no_agent_text(self):
        s = Session(self, dry, [observe({"query": CANARY, "limit": 7, "cursor": CANARY})])
        text = s.log.read_text(encoding="utf-8")
        self.assertNotIn(CANARY, text)
        sent = next(r for r in s.records if r["dir"] == "agent->engine" and r.get("tool") == "observe_screen")
        self.assertEqual((sent["query_present"], sent["limit"], sent["cursor_present"]), (True, 7, True))
        got = next(r for r in s.records if r["dir"] == "engine->agent" and r.get("tool") == "observe_screen")
        self.assertEqual((got["elements"], got["total_elements"], got["truncated"]), (1, 1, False))
        self.assertEqual(got["timings_us"]["total_us"], 10110)
        self.assertEqual(got["plan"]["work"], "full")


class ForwardingIsUnchanged(unittest.TestCase):
    def test_the_engine_receives_every_line_byte_for_byte_and_the_agent_every_reply(self):
        lines = [call(1, "observe_screen", {"query": CANARY, "limit": 7, "cursor": CANARY}),
                 call(2, "observe_screen", {}), action(3, dry_run=True),
                 {"jsonrpc": "2.0", "method": "notifications/initialized"}]
        s = Session(self, dry, lines)
        self.assertEqual(s.engine_got, [json.dumps(line).encode() for line in lines])
        self.assertEqual([r["id"] for r in s.replies], [1, 2, 3])
        observation = s.reply(1)["result"]
        self.assertIn("_meta", observation)  # the engine's own `_meta` reaches the agent untouched
        self.assertEqual(observation["_meta"]["io.ommatix/timings"]["total_us"], 10110)


class IsolationProofIsUnaffected(unittest.TestCase):
    """The proof of ADR-0036/0037 binds the agent's side (version, model, tools, context, requests
    to the provider). It names neither the proxy's file nor the fields its log carries."""

    def test_the_proof_verifier_does_not_hash_or_read_the_proxy_log_format(self):
        for name in ("preflight.py", "preflight_cli.py"):
            source = (HERE / name).read_text(encoding="utf-8")
            self.assertNotIn("proxy.py\").read", source)
            self.assertNotIn("query_present", source)
            self.assertNotIn("request_index", source)
        run_goal = (HERE / "run_goal.py").read_text(encoding="utf-8")
        self.assertNotIn("proxy_sha", run_goal)


if __name__ == "__main__":
    unittest.main()
