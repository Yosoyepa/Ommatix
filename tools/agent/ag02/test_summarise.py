"""Tests of the AG-02 summary (synthetic per-goal results, no files from a real session)."""

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import summarise  # noqa: E402


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
        self.assertEqual((phase2["runs"], phase2["success"], phase2["false_success_claims"]), (4, 2, 2))
        self.assertEqual((phase2["abstention_cases"], phase2["abstention_correct"]), (2, 1))
        self.assertEqual((phase2["live_actions_spent"], phase2["delivered"]), (5, 5))
        self.assertEqual(phase2["proxy_refusals"], {"LIVE_BUDGET_EXHAUSTED": 2})
        self.assertEqual(phase2["engine_error_codes"], {"STALE_SNAPSHOT": 1})
        self.assertEqual(phase2["tool_latency_ms"]["observe_screen"]["n"], 2)
        self.assertEqual(phase2["tokens_per_goal_median"], 110)
        self.assertEqual(summary["goals"]["G03"]["2"]["aborted"], 1)
        self.assertEqual(summary["phases"]["1"]["runs"], 1)

    def test_load_and_render_from_a_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            (run / "G01.json").write_text(json.dumps(result("G01", 1, success=True)), encoding="utf-8")
            (run / "G01.mcp.json").write_text("{}", encoding="utf-8")
            (run / "G01.proxy.jsonl").write_text("{}\n", encoding="utf-8")
            loaded = summarise.load_results([run])
            self.assertEqual(len(loaded), 1)
            text = summarise.render(summarise.summarise(loaded))
            self.assertIn("phase 1: 1/1 succeeded", text)
            out = run / "summary.json"
            with redirect_stdout(io.StringIO()):
                self.assertEqual(summarise.main([str(run), "--out", str(out)]), 0)
            self.assertIn("phases", json.loads(out.read_text(encoding="utf-8")))

    def test_an_empty_directory_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(summarise.main([tmp]), 1)


if __name__ == "__main__":
    unittest.main()
