"""Tests of the AG-02 summary (synthetic per-goal results, no files from a real session)."""

import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import summarise  # noqa: E402
from statistics import median as statistics_median  # noqa: E402


def result(goal, phase, *, success, false_success=False, expected="done", delivered=0, spent=0,
           tokens=(100, 10), refusals=None, codes=None, step_ms=None, abstention=None, safety=None,
           aborted=None):
    if aborted:
        return {"goal": goal, "phase": phase, "aborted": aborted}
    r = {"goal": goal, "phase": phase, "expected": expected, "live_spent": spent,
         "score": {"success": success, "false_success": false_success, "abstention_correct": abstention},
         "metrics": {"tokens": {"input_tokens": tokens[0], "output_tokens": tokens[1]},
                     "tokens_per_message": [{"input": tokens[0], "output": tokens[1]}],
                     "cost_usd": 0.01, "wall_s": 12.0, "step_ms": step_ms or []},
         "proxy": {"delivered": delivered, "proxy_refusals": refusals or {},
                   "engine_error_codes": codes or {}, "statuses": {}}}
    if safety:
        r["safety_event"] = safety
    return r


class SummariseTests(unittest.TestCase):
    def test_an_abstention_not_scored_in_the_phase_is_not_in_the_denominator(self):
        # G09 in phase 1: expected "impossible", but its phase-1 verdict is null, so the score has
        # no abstention verdict (None); only G10 is scored. It must read 1/1, not 1/2.
        results = [
            result("G09", 1, success=True, expected="impossible", abstention=None),
            result("G10", 1, success=True, expected="impossible", abstention=True),
        ]
        phase1 = summarise.summarise(results)["phases"]["1"]
        self.assertEqual((phase1["abstention_cases"], phase1["abstention_correct"]), (1, 1))
        self.assertIn("abstention correct 1/1", summarise.render(summarise.summarise(results)))

    def test_success_false_claims_abstention_and_refusals_are_counted_by_phase(self):
        results = [
            result("G01", 2, success=True, spent=2, delivered=2,
                   step_ms=[{"tool": "observe_screen", "ms": 100.0}, {"tool": "observe_screen", "ms": 300.0}]),
            result("G02", 2, success=False, false_success=True, spent=3, delivered=3,
                   codes={"STALE_SNAPSHOT": 1}),
            result("G10", 2, success=True, expected="impossible", abstention=True),
            result("G11", 2, success=False, expected="impossible", abstention=False,
                   refusals={"LIVE_BUDGET_EXHAUSTED": 2}, false_success=True),
            result("G01", 1, success=True),
            result("G03", 2, success=False, aborted="the application never reached the front"),
        ]
        summary = summarise.summarise(results)
        phase2 = summary["phases"]["2"]
        self.assertEqual((phase2["completed"], phase2["success"], phase2["false_success_claims"]), (4, 2, 2))
        self.assertEqual((phase2["abstention_cases"], phase2["abstention_correct"]), (2, 1))
        self.assertEqual((phase2["live_actions_spent"], phase2["delivered"]), (5, 5))
        self.assertEqual(phase2["proxy_refusals"], {"LIVE_BUDGET_EXHAUSTED": 2})
        self.assertEqual(phase2["engine_error_codes"], {"STALE_SNAPSHOT": 1})
        self.assertEqual(phase2["tool_latency_ms"]["observe_screen"]["n"], 2)
        # no cli_usage in these fixtures: the stream's sums stand in, and say so
        self.assertEqual(phase2["token_sources"], {"stream": 4})
        self.assertEqual(phase2["context_tokens_per_goal_median"], 100)
        self.assertEqual(phase2["uncached_input_plus_output_tokens_per_goal_median_extra"], 110)
        self.assertEqual(summary["goals"]["G03"]["2"]["aborted"], 1)
        self.assertEqual(summary["phases"]["1"]["attempted"], 1)

    # A real result's shape (G01 of the owner's live phase 1): the stream's per-message counts are
    # provisional and the old figure left out nearly everything the model read.
    REAL = {"goal": "G01", "phase": 1, "expected": "done", "score": {"success": True},
            "metrics": {"tokens": {"input_tokens": 8, "output_tokens": 45, "cache_read_input_tokens": 20775,
                                   "cache_creation_input_tokens": 10218},
                        "cli_usage": {"input_tokens": 8, "cache_creation_input_tokens": 10218,
                                      "cache_read_input_tokens": 20775, "output_tokens": 1490,
                                      "output_tokens_details": {"thinking_tokens": 541}},
                        "cost_usd": 0.00299715, "wall_s": 15.4,
                        "tokens_per_message": [
                            {"input": 2, "output": 28, "cache_creation": 475, "cache_read": 9743},
                            {"input": 2, "output": 7, "cache_creation": 100, "cache_read": 10000}]}}

    def test_the_headline_is_the_context_the_model_read_not_the_uncached_remainder(self):
        figures = summarise.token_figures(self.REAL["metrics"])
        self.assertEqual(figures["context_tokens"], 8 + 10218 + 20775)  # input + creation + read
        self.assertEqual(figures["output_tokens"], 1490)  # the CLI's total, thinking included
        self.assertEqual(figures["cost_usd"], 0.00299715)
        self.assertEqual(figures["source"], "cli_usage")
        self.assertEqual(figures["uncached_input_plus_output_tokens"], 8 + 1490)  # the old figure
        summary = summarise.summarise([self.REAL])
        goal, phase = summary["goals"]["G01"]["1"], summary["phases"]["1"]
        self.assertEqual((goal["context_tokens_median"], goal["output_tokens_median"], goal["cost_usd_total"]),
                         (31001, 1490, 0.00299715))
        self.assertEqual(goal["uncached_input_plus_output_tokens_median_extra"], 1498)
        self.assertEqual((phase["context_tokens_total"], phase["output_tokens_total"], phase["cost_usd_total"]),
                         (31001, 1490, 0.00299715))
        self.assertEqual(phase["context_tokens_per_message_median"], statistics_median([477 + 9743 - 0, 10102]))
        self.assertEqual(phase["token_sources"], {"cli_usage": 1})

    def test_the_rendering_labels_the_old_figure_as_an_extra_and_leads_with_the_new_ones(self):
        text = summarise.render(summarise.summarise([self.REAL]))
        self.assertIn("context tokens (fresh input + cache creation + cache read): total 31001", text)
        self.assertIn("output tokens (thinking included): total 1490", text)
        self.assertIn("[extra, old figure: fresh input + output without cached tokens", text)
        self.assertIn("31001", text.splitlines()[3])  # the per-goal row
        self.assertNotIn("tokens(med)", text)

    def test_without_cli_usage_the_stream_sums_are_used_and_labelled(self):
        legacy = {"goal": "G02", "phase": 1, "expected": "done", "score": {"success": True},
                  "metrics": {"tokens": {"input_tokens": 8, "output_tokens": 45, "cache_read_input_tokens": 100,
                                         "cache_creation_input_tokens": 50}, "cli_usage": None}}
        figures = summarise.token_figures(legacy["metrics"])
        self.assertEqual((figures["context_tokens"], figures["source"]), (158, "stream"))
        self.assertEqual(summarise.summarise([legacy])["phases"]["1"]["token_sources"], {"stream": 1})

    def test_malformed_usage_never_breaks_the_figures(self):
        for bad in ({"cli_usage": {"input_tokens": "8", "output_tokens": 5}}, {"cli_usage": {"input_tokens": True}},
                    {"cli_usage": 5}, {}):
            figures = summarise.token_figures(bad | {"tokens": {}})
            self.assertEqual((figures["context_tokens"], figures["output_tokens"]), (0, 0))

    def test_load_and_render_from_a_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            (run / "G01.json").write_text(json.dumps(result("G01", 1, success=True)), encoding="utf-8")
            (run / "G01.mcp.json").write_text("{}", encoding="utf-8")
            (run / "G01.proxy.jsonl").write_text("{}\n", encoding="utf-8")
            loaded = summarise.load_results([run])
            self.assertEqual(len(loaded), 1)
            text = summarise.render(summarise.summarise(loaded))
            self.assertIn("phase 1: 1/1 attempts succeeded", text)
            out = run / "summary.json"
            with redirect_stdout(io.StringIO()):
                self.assertEqual(summarise.main([str(run), "--out", str(out)]), 0)
            self.assertIn("phases", json.loads(out.read_text(encoding="utf-8")))

    def test_an_aborted_attempt_stays_in_the_phase_denominator(self):
        results = [result("G01", 2, success=True), result("G02", 2, success=False, aborted="never in front")]
        phase = summarise.summarise(results)["phases"]["2"]
        self.assertEqual((phase["attempted"], phase["completed"], phase["aborted"], phase["success"]),
                         (2, 1, 1, 1))
        text = summarise.render(summarise.summarise(results))
        self.assertIn("phase 2: 1/2 attempts succeeded (1 completed, 1 aborted)", text)
        self.assertNotIn("1/1", text)

    def test_the_summary_is_never_written_inside_the_repository(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp) / "run"
            run.mkdir()
            (run / "G01.json").write_text(json.dumps(result("G01", 1, success=True)), encoding="utf-8")
            repo = Path(tmp) / "repo"
            (repo / ".git").mkdir(parents=True)
            before = os.getcwd()
            os.chdir(repo)  # a relative --out would land in the checkout
            try:
                with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
                    with self.assertRaises(SystemExit):
                        summarise.main([str(run), "--out", "resumen.json"])
            finally:
                os.chdir(before)
            self.assertFalse((repo / "resumen.json").exists())

    def test_an_empty_directory_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(summarise.main([tmp]), 1)


if __name__ == "__main__":
    unittest.main()
