"""Tests of the AG-02 log statistics, the aggregate export (``tables.py``) and the summary's
observation evidence. Synthetic fixtures only: no owner log, no screen, no network."""

import csv
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import logstats  # noqa: E402
import summarise  # noqa: E402
import tables  # noqa: E402

CANARY = "PRIVATE-SCREEN-CANARY"


# ---- fixtures ----------------------------------------------------------------------------------

def request(index, tool, size, observe_index=None, **extra):
    record = {"t_ms": float(index), "dir": "agent->engine", "bytes": size, "live_counted": False,
              "kind": "request", "method": "tools/call", "tool": tool, **extra}
    if index is not None:
        record["request_index"] = index
    if observe_index is not None:
        record["observe_index"] = observe_index
    return record


def response(index, tool, **extra):
    record = {"t_ms": float(index) + 0.5, "dir": "engine->agent", "bytes": 900, "kind": "response",
              "is_error": False, "tool": tool, "latency_ms": 2500.0, **extra}
    if index is not None:
        record["request_index"] = index
    return record


def old(record):
    """The same record as an old log would hold it: no evidence fields, no indices."""
    return {k: v for k, v in record.items() if k not in (
        "request_index", "observe_index", "query_present", "limit", "cursor_present", "truncated",
        "total_elements", "timings_us", "plan")}


END = {"t_ms": 99.0, "dir": "proxy", "bytes": 0, "kind": "end", "engine_exit": 0, "live_spent_here": 0}


def write_goal(folder: Path, goal="G01", phase=2, *, records, expected="done", success=True,
               oracle_ok=True, verdict="DONE", live_spent=0, aborted=None, label="model-a",
               abstention=None, steps=None, provenance=True, tokens=(1000, 50), extra=None):
    folder.mkdir(parents=True, exist_ok=True)
    result = {
        "goal": goal, "phase": phase, "app": "testapp", "expected": expected, "started_utc": "2026-10-19T00:00:00Z",
        "cli": {"exit_code": 0, "killed": None},
        "metrics": {"turns": 4, "tool_calls": {"observe_screen": 2, "execute_action": 1, "get_engine_metrics": 0},
                    "verdict": verdict, "wall_s": 12.5, "cost_usd": 0.01,
                    "cli_usage": {"input_tokens": tokens[0], "output_tokens": tokens[1],
                                  "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
                    "step_ms": steps if steps is not None else [
                        {"tool": "observe_screen", "ms": 2500.0}, {"tool": "execute_action", "ms": 16.0}],
                    "chosen_labels": [CANARY], "actions": [], "actions_observed_after": 0},
        "proxy": {"live_forwarded": live_spent, "delivered": 0, "proxy_refusals": {}, "engine_error_codes": {},
                  "statuses": {}, "rpc_errors": 0, "complete": True},
        "final_state_counts": {"estado": CANARY}, "note": CANARY, "live_spent": live_spent,
        "oracle": {"ok": oracle_ok, "reason": "differs" if not oracle_ok else "note_ok"},
        "score": {"success": success, "false_success": False, "missed_success": False,
                  "abstention_correct": abstention},
    }
    if provenance:
        result["provenance"] = {"claude_code_version": "2.1.303", "cli": "claude-code", "provider": "test",
                                "model": label, "requested_model": label, "route_host": "example.invalid"}
    if aborted:
        result["aborted"] = aborted
    if extra:
        result.update(extra)
    (folder / f"{goal}.json").write_text(json.dumps(result), encoding="utf-8")
    (folder / f"{goal}.mcp.json").write_text("{}", encoding="utf-8")
    (folder / f"{goal}.proxy.jsonl").write_text(
        "\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8")
    return result


def observed(index, size, elements, observe_index, **kw):
    """A request/response pair of one observation."""
    return [request(index, "observe_screen", size, observe_index, **kw.get("req", {})),
            response(index, "observe_screen", elements=elements, **kw.get("resp", {}))]


class TempCase(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-tables-")
        self.addCleanup(scratch.cleanup)
        self.tmp = Path(scratch.name)


# ---- logstats ----------------------------------------------------------------------------------

class SizeClasses(unittest.TestCase):
    def test_the_classes_follow_the_measured_sizes(self):
        self.assertEqual([logstats.size_class(n) for n in (542, 548, 549, 552, 553, 621)],
                         ["bare", "bare", "ambiguous", "ambiguous", "with_args", "with_args"])
        for bad in (None, "553", True, 1.5):
            self.assertEqual(logstats.size_class(bad), "unknown")

    def test_the_percentile_is_the_one_the_summary_uses(self):
        values = [5.0, 1.0, 9.0, 3.0, 7.0, 2.0, 8.0]
        for q in (0.5, 0.95, 1.0, 0.0):
            self.assertEqual(logstats.percentile(values, q), summarise._percentile(values, q))
        self.assertIsNone(logstats.percentile([], 0.5))


class Pairing(unittest.TestCase):
    def test_new_logs_pair_by_request_index_and_say_what_the_request_carried(self):
        records = [
            *observed(1, 543, 1, 1, req={"query_present": True, "limit": 20, "cursor_present": False},
                      resp={"total_elements": 70, "truncated": True}),
            request(2, "execute_action", 600),
            response(2, "execute_action", status="simulated"),
            *observed(3, 543, 80, 2, req={"query_present": False, "cursor_present": False},
                      resp={"total_elements": 80, "truncated": False,
                            "timings_us": {"total_us": 10}, "plan": {"work": "full"}}),
        ]
        entries = logstats.observations(records)
        self.assertEqual([e["observe_index"] for e in entries], [1, 2])
        self.assertEqual((entries[0]["query_present"], entries[0]["limit"], entries[0]["truncated"],
                          entries[0]["total_elements"]), (True, 20, True, 70))
        self.assertEqual(entries[1]["plan"], {"work": "full"})

    def test_old_logs_pair_by_order_and_leave_the_unrecorded_unrecorded(self):
        records = [old(r) for r in [*observed(1, 543, 1, 1), *observed(2, 560, 0, 2), *observed(3, 560, 5, 3)]]
        entries = logstats.observations(records)
        self.assertEqual([e["elements"] for e in entries], [1, 0, 5])
        self.assertEqual([e["observe_index"] for e in entries], [1, 2, 3])
        for e in entries:
            self.assertIsNone(e["query_present"])
            self.assertIsNone(e["truncated"])
        figures = logstats.observation_figures(entries)
        self.assertEqual(figures["request_args_recorded"], 0)
        self.assertEqual(figures["truncated_unrecorded"], 3)

    def test_a_request_without_a_response_is_unanswered_and_an_error_response_has_no_elements(self):
        records = [*observed(1, 543, 3, 1),
                   request(2, "observe_screen", 543, 2),
                   {**response(2, "observe_screen"), "rpc_error_code": -32602},
                   request(3, "observe_screen", 543, 3)]
        entries = logstats.observations(records)
        self.assertEqual([(e["answered"], e["is_error"], e["elements"]) for e in entries],
                         [(True, False, 3), (True, True, None), (False, None, None)])
        figures = logstats.observation_figures(entries)
        self.assertEqual((figures["requests"], figures["responses"], figures["unanswered"],
                          figures["error_responses"]), (3, 2, 1, 1))

    def test_figures_cross_small_answers_with_the_request_size(self):
        entries = logstats.observations([old(r) for r in [
            *observed(1, 543, 3, 1), *observed(2, 543, 80, 2), *observed(3, 560, 0, 3),
            *observed(4, 580, 1, 4), *observed(5, 550, 2, 5)]])
        figures = logstats.observation_figures(entries)
        self.assertEqual(figures["elements_distribution"], {"0": 1, "1": 1, "2": 1, "3+": 2})
        self.assertEqual((figures["elements_3plus_min"], figures["elements_3plus_max"]), (3, 80))
        self.assertEqual(figures["small_responses_by_request_size_class"], {"ambiguous": 1, "with_args": 2})
        self.assertEqual(figures["request_size_classes"], {"ambiguous": 1, "bare": 2, "with_args": 2})

    def test_unreadable_lines_are_counted_not_fatal(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-jsonl-")
        self.addCleanup(scratch.cleanup)
        path = Path(scratch.name) / "x.jsonl"
        path.write_text('{"a": 1}\n[1]\nnot json\n\n{"b": 2', encoding="utf-8")
        records, bad = logstats.read_jsonl(path)
        self.assertEqual((records, bad), ([{"a": 1}], 3))
        self.assertEqual(logstats.read_jsonl(Path(scratch.name) / "missing.jsonl"), ([], 0))


class Loop(unittest.TestCase):
    def test_actions_are_counted_after_and_before_observations_that_arrived_without_error(self):
        records = [
            request(1, "observe_screen", 543, 1), response(1, "observe_screen", elements=3),
            request(2, "execute_action", 600), response(2, "execute_action", status="rejected"),
            request(3, "execute_action", 600), response(3, "execute_action", status="rejected"),
            request(4, "observe_screen", 543, 2), {**response(4, "observe_screen"), "rpc_error_code": -1},
            request(5, "execute_action", 600), response(5, "execute_action", status="simulated"),
            request(6, "observe_screen", 543, 3), response(6, "observe_screen", elements=2),
        ]
        self.assertEqual(logstats.loop_counts(records),
                         {"actions": 3, "after_observation": 1, "followed_by_observation": 2})

    def test_an_action_sent_before_the_observation_answered_does_not_follow_a_received_observation(self):
        # R1 of the review of PR 102: the agent may send a call while another is pending.
        records = [request(1, "observe_screen", 543, 1), request(2, "execute_action", 600),
                   response(1, "observe_screen", elements=3), response(2, "execute_action", status="simulated")]
        self.assertEqual(logstats.loop_counts(records),
                         {"actions": 1, "after_observation": 0, "followed_by_observation": 0})
        # the same log from before the indices: sequential pairing, same arrival rule
        self.assertEqual(logstats.loop_counts([old(r) for r in records]),
                         {"actions": 1, "after_observation": 0, "followed_by_observation": 0})

    def test_responses_pair_with_their_request_by_index_not_by_arrival_order(self):
        records = [request(1, "observe_screen", 543, 1), request(2, "observe_screen", 543, 2),
                   response(2, "observe_screen", elements=3),
                   {**response(1, "observe_screen"), "rpc_error_code": -32602},
                   request(3, "execute_action", 600), response(3, "execute_action", status="simulated")]
        self.assertEqual(logstats.loop_counts(records),
                         {"actions": 1, "after_observation": 1, "followed_by_observation": 0})

    def test_an_action_right_after_an_observation_answered_with_an_error_is_not_after_one(self):
        records = [request(1, "observe_screen", 543, 1), request(2, "observe_screen", 543, 2),
                   {**response(2, "observe_screen"), "rpc_error_code": -32602},
                   response(1, "observe_screen", elements=3),
                   request(3, "execute_action", 600), response(3, "execute_action", status="simulated")]
        self.assertEqual(logstats.loop_counts(records)["after_observation"], 0)

    def test_a_call_the_proxy_refused_is_an_intermediate_call_and_is_not_an_action_counted(self):
        refused = {"dir": "agent->proxy", "kind": "request", "method": "tools/call", "tool": "execute_action",
                   "bytes": 600, "live_counted": False}
        records = [request(1, "observe_screen", 543, 1), response(1, "observe_screen", elements=3),
                   refused,
                   request(2, "execute_action", 600), response(2, "execute_action", status="simulated")]
        self.assertEqual(logstats.loop_counts(records),
                         {"actions": 1, "after_observation": 0, "followed_by_observation": 0})


# ---- tables ------------------------------------------------------------------------------------

def two_goal_series(root: Path, *, label="model-a", phase=2):
    folder = root / "run1"
    write_goal(folder, "G01", phase, records=[
        *observed(1, 543, 70, 1), request(2, "execute_action", 600, action="click", dry_run=False, live_counted=True),
        response(2, "execute_action", status="input_delivered", input_delivered=True, verified=False),
        *observed(3, 560, 1, 2), END], live_spent=1, label=label, abstention=None)
    write_goal(folder, "G10", phase, expected="impossible", success=True, verdict="IMPOSSIBLE", label=label,
               abstention=True, records=[*observed(1, 543, 66, 1), END])
    write_goal(folder, "G07", phase, success=False, oracle_ok=False, verdict="other", label=label,
               aborted="the application never reached the front", records=[END])
    return folder


class Export(TempCase):
    def run_tables(self, *args, out=None, expect=0):
        out = out or self.tmp / "out"
        argv = [sys.executable, str(HERE / "tables.py"), *args, "--out", str(out)]
        done = subprocess.run(argv, capture_output=True, text=True, timeout=120, check=False)
        self.assertEqual(done.returncode, expect, done.stdout + done.stderr)
        return out, done

    def make(self):
        runs = self.tmp / "runs"
        two_goal_series(runs)
        return runs

    def test_it_writes_rows_aggregates_observations_and_a_manifest(self):
        runs = self.make()
        out, _ = self.run_tables("--series", f"Model A={runs}")
        for name in ("goals.json", "goals.csv", "observations.csv", "series.json", "series.csv", "manifest.json"):
            self.assertTrue((out / name).exists(), name)
        goals = json.loads((out / "goals.json").read_text(encoding="utf-8"))
        by_goal = {g["goal"]: g for g in goals}
        self.assertEqual(sorted(by_goal), ["G01", "G07", "G10"])
        self.assertEqual(by_goal["G01"]["kind"], "series")
        self.assertEqual(by_goal["G07"]["kind"], "aborted")
        self.assertEqual((by_goal["G01"]["observe_requests"], by_goal["G01"]["observe_responses"]), (2, 2))
        self.assertEqual((by_goal["G01"]["elements_1"], by_goal["G01"]["elements_3plus"]), (1, 1))
        self.assertEqual((by_goal["G01"]["request_bare"], by_goal["G01"]["request_with_args"]), (1, 1))
        self.assertEqual(by_goal["G01"]["context_tokens"], 1000)
        self.assertEqual(by_goal["G01"]["delivered_input_delivered"], 1)
        series = json.loads((out / "series.json").read_text(encoding="utf-8"))
        self.assertEqual(len(series), 1)
        row = series[0]
        self.assertEqual((row["series"], row["phase"], row["completed"], row["aborted"]), ("Model A", 2, 2, 1))
        self.assertEqual(row["score_success"], 2)
        self.assertEqual((row["reachable_n"], row["reachable_score_success"], row["reachable_oracle_ok"]), (1, 1, 1))
        self.assertEqual((row["abstention_cases"], row["abstention_correct"]), (1, 1))

    def test_calls_are_counted_from_the_proxy_log_apart_from_the_agents_own_count(self):
        runs = self.tmp / "runs"
        write_goal(runs / "r", "G01", 2, records=[
            request(1, "get_engine_metrics", 300), response(1, "get_engine_metrics"),
            request(2, "get_engine_metrics", 300),  # in flight when the session was cut: no response
            *observed(3, 543, 5, 1), END],
            extra={"metrics": {"turns": 1, "verdict": "DONE", "tool_calls": {"get_engine_metrics": 3}}})
        out, _ = self.run_tables("--series", f"M={runs}")
        row = json.loads((out / "goals.json").read_text(encoding="utf-8"))[0]
        self.assertEqual((row["tool_calls_metrics"], row["log_calls_metrics"], row["log_calls_metrics_answered"]), (3, 2, 1))
        series = json.loads((out / "series.json").read_text(encoding="utf-8"))[0]
        self.assertEqual((series["log_calls_metrics"], series["log_calls_metrics_answered"]), (2, 1))

    def test_the_reasoning_tokens_are_exported_when_the_cli_reported_them(self):
        runs = self.tmp / "runs"
        write_goal(runs / "r", "G01", 2, records=[END], extra={"metrics": {
            "turns": 1, "verdict": "DONE", "cli_usage": {"input_tokens": 1, "output_tokens": 30,
                                                          "output_tokens_details": {"thinking_tokens": 12}}}})
        write_goal(runs / "r", "G02", 2, records=[END], extra={"metrics": {
            "turns": 1, "verdict": "DONE", "cli_usage": {"input_tokens": 1, "output_tokens": 30}}})
        out, _ = self.run_tables("--series", f"M={runs}")
        goals = {g["goal"]: g for g in json.loads((out / "goals.json").read_text(encoding="utf-8"))}
        self.assertEqual(goals["G01"]["thinking_tokens"], 12)
        self.assertIsNone(goals["G02"]["thinking_tokens"])

    def test_a_rehearsal_is_exported_but_leaves_the_series_denominators(self):
        runs = self.make()
        rehearsal = self.tmp / "rehearsal"
        write_goal(rehearsal, "G01", 2, records=[*observed(1, 543, 70, 1), END], label="model-a")
        out, _ = self.run_tables("--series", f"Model A={runs},{rehearsal}", "--rehearsal", str(rehearsal))
        goals = json.loads((out / "goals.json").read_text(encoding="utf-8"))
        kinds = sorted((g["folder"], g["goal"], g["kind"]) for g in goals)
        self.assertIn(("rehearsal", "G01", "rehearsal"), kinds)
        row = json.loads((out / "series.json").read_text(encoding="utf-8"))[0]
        self.assertEqual((row["completed"], row["rehearsal_rows"]), (2, 1))

    def test_the_regrouping_counts_reachable_goals_and_abstentions_apart(self):
        runs = self.tmp / "runs"
        folder = runs / "r"
        write_goal(folder, "G01", 2, records=[END], success=True, oracle_ok=True)
        write_goal(folder, "G02", 2, records=[END], success=False, oracle_ok=True, verdict="IMPOSSIBLE")
        write_goal(folder, "G03", 2, records=[END], success=False, oracle_ok=False, verdict="IMPOSSIBLE")
        write_goal(folder, "G09", 2, records=[END], expected="impossible", success=True, abstention=True,
                   verdict="IMPOSSIBLE")
        write_goal(folder, "G10", 2, records=[END], expected="impossible", success=False, abstention=False,
                   verdict="IMPOSSIBLE")
        out, _ = self.run_tables("--series", f"M={runs}")
        row = json.loads((out / "series.json").read_text(encoding="utf-8"))[0]
        self.assertEqual((row["reachable_n"], row["reachable_score_success"], row["reachable_oracle_ok"]), (3, 1, 2))
        self.assertEqual((row["abstention_cases"], row["abstention_correct"]), (2, 1))
        self.assertEqual(row["score_success"], 2)

    def test_phase_one_has_no_oracle_state_regrouping(self):
        runs = self.tmp / "runs"
        write_goal(runs / "r", "G01", 1, records=[END])
        out, _ = self.run_tables("--series", f"M={runs}")
        row = json.loads((out / "series.json").read_text(encoding="utf-8"))[0]
        self.assertIsNone(row["reachable_oracle_ok"])

    def test_nothing_the_runner_wrote_as_free_text_reaches_the_export(self):
        runs = self.make()
        out, _ = self.run_tables("--series", f"Model A={runs}")
        for path in out.iterdir():
            text = path.read_text(encoding="utf-8")
            self.assertNotIn(CANARY, text, path.name)
            self.assertNotIn("never reached the front", text, path.name)

    def test_a_verdict_outside_the_closed_set_is_exported_as_other(self):
        runs = self.tmp / "runs"
        write_goal(runs / "r", "G01", 2, records=[END], verdict=CANARY)
        out, _ = self.run_tables("--series", f"M={runs}")
        goals = json.loads((out / "goals.json").read_text(encoding="utf-8"))
        self.assertEqual(goals[0]["verdict"], "other")

    def test_the_manifest_hashes_every_input_and_output(self):
        runs = self.make()
        summary = self.tmp / "resumen.json"
        summary.write_text('{"phases": {}}', encoding="utf-8")
        out, _ = self.run_tables("--series", f"Model A={runs}", "--summary", str(summary))
        manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
        inputs = {i["path"]: i for i in manifest["inputs"]}
        for name in ("run1/G01.json", "run1/G01.proxy.jsonl", "run1/G10.json", "run1/G10.proxy.jsonl"):
            digest = hashlib.sha256((runs / name).read_bytes()).hexdigest()
            self.assertEqual(inputs[f"Model A/{name}"]["sha256"], digest, name)
        self.assertEqual(inputs["summary/resumen.json"]["sha256"], hashlib.sha256(summary.read_bytes()).hexdigest())
        self.assertEqual(inputs["summary/resumen.json"]["role"], "summary")
        for name, entry in manifest["outputs"].items():
            self.assertEqual(entry["sha256"], hashlib.sha256((out / name).read_bytes()).hexdigest(), name)
        self.assertEqual(manifest["tool"]["script_sha256"], hashlib.sha256((HERE / "tables.py").read_bytes()).hexdigest())
        self.assertNotIn(str(self.tmp), json.dumps(manifest))  # relative paths only, no operator path

    def test_the_export_is_deterministic(self):
        runs = self.make()
        first, _ = self.run_tables("--series", f"Model A={runs}", out=self.tmp / "o1")
        second, _ = self.run_tables("--series", f"Model A={runs}", out=self.tmp / "o2")
        for name in ("goals.json", "goals.csv", "observations.csv", "series.json", "series.csv", "manifest.json"):
            self.assertEqual((first / name).read_bytes(), (second / name).read_bytes(), name)

    def test_observation_rows_carry_numbers_and_the_size_class(self):
        runs = self.make()
        out, _ = self.run_tables("--series", f"Model A={runs}")
        rows = list(csv.DictReader(io.StringIO((out / "observations.csv").read_text(encoding="utf-8"))))
        first = next(r for r in rows if r["goal"] == "G01" and r["observe_index"] == "1")
        self.assertEqual((first["request_bytes"], first["request_size_class"], first["elements"]), ("543", "bare", "70"))
        self.assertEqual(rows[0].keys() & {"query", "label", "text"}, set())

    def test_the_output_must_be_absolute_and_outside_a_checkout(self):
        runs = self.make()
        done = subprocess.run([sys.executable, str(HERE / "tables.py"), "--series", f"M={runs}", "--out", "relative"],
                              capture_output=True, text=True, timeout=60, check=False)
        self.assertNotEqual(done.returncode, 0)
        inside = HERE.parent.parent.parent / "tables-out"  # inside this checkout
        done = subprocess.run([sys.executable, str(HERE / "tables.py"), "--series", f"M={runs}", "--out", str(inside)],
                              capture_output=True, text=True, timeout=60, check=False)
        self.assertNotEqual(done.returncode, 0)
        self.assertFalse(inside.exists())

    def test_no_results_is_an_error_and_so_is_a_bad_series_label(self):
        empty = self.tmp / "empty"
        empty.mkdir()
        self.run_tables("--series", f"M={empty}", expect=1)
        self.run_tables("--series", f"bad;label={empty}", expect=2)
        self.run_tables("--series", f"M={self.tmp / 'missing'}", expect=2)

    def test_scratch_folders_and_unreadable_results_are_skipped(self):
        runs = self.make()
        (runs / "run1" / "scratch").mkdir()
        write_goal(runs / "run1" / "scratch", "G05", 2, records=[END])
        (runs / "run1" / "G09.json").write_text("{not json", encoding="utf-8")
        out, _ = self.run_tables("--series", f"M={runs}")
        goals = json.loads((out / "goals.json").read_text(encoding="utf-8"))
        self.assertEqual(sorted(g["goal"] for g in goals), ["G01", "G07", "G10"])

    def test_a_missing_proxy_log_leaves_the_log_figures_empty_not_zero(self):
        runs = self.make()
        (runs / "run1" / "G10.proxy.jsonl").unlink()
        out, _ = self.run_tables("--series", f"M={runs}")
        goals = {g["goal"]: g for g in json.loads((out / "goals.json").read_text(encoding="utf-8"))}
        self.assertIsNone(goals["G10"]["observe_requests"])
        self.assertIsNone(goals["G10"]["proxy_log_sha256"])

    def test_the_export_has_no_path_of_the_operator_in_any_file(self):
        runs = self.make()
        out, _ = self.run_tables("--series", f"Model A={runs}")
        for path in out.iterdir():
            self.assertNotIn(str(self.tmp), path.read_text(encoding="utf-8"), path.name)
            self.assertNotIn(os.path.expanduser("~"), path.read_text(encoding="utf-8"), path.name)


# ---- summarise ---------------------------------------------------------------------------------

class SummaryEvidence(TempCase):
    def test_the_summary_reports_the_observation_evidence_from_the_logs_beside_the_results(self):
        folder = self.tmp / "run"
        write_goal(folder, "G01", 1, records=[
            *observed(1, 543, 70, 1, req={"query_present": False, "cursor_present": False},
                      resp={"total_elements": 70, "truncated": False}),
            *observed(2, 570, 2, 2, req={"query_present": True, "limit": 10, "cursor_present": False},
                      resp={"total_elements": 70, "truncated": True}), END])
        summary = summarise.summarise(summarise.load_results([folder]))
        observation = summary["phases"]["1"]["observations"]
        self.assertEqual((observation["requests"], observation["responses"]), (2, 2))
        self.assertEqual((observation["with_query"], observation["with_limit"], observation["truncated_true"]), (1, 1, 1))
        self.assertEqual(observation["elements_distribution"], {"2": 1, "3+": 1})
        text = summarise.render(summary)
        self.assertIn("observations: 2 requests, 2 responses", text)
        self.assertIn("with a query 1", text)

    def test_old_logs_are_reported_as_unrecorded_never_as_zero(self):
        folder = self.tmp / "run"
        write_goal(folder, "G01", 1, records=[old(r) for r in [*observed(1, 543, 70, 1), *observed(2, 570, 2, 2)]] + [END])
        summary = summarise.summarise(summarise.load_results([folder]))
        text = summarise.render(summary)
        self.assertIn("request arguments not recorded in 2 of 2 requests", text)
        self.assertIn("small answers (0 to 2 elements): 1", text)

    def test_a_result_without_a_log_still_summarises(self):
        folder = self.tmp / "run"
        write_goal(folder, "G01", 1, records=[END])
        (folder / "G01.proxy.jsonl").unlink()
        summary = summarise.summarise(summarise.load_results([folder]))
        self.assertEqual(summary["phases"]["1"]["observations"]["requests"], 0)

    def test_the_json_summary_holds_no_path(self):
        folder = self.tmp / "run"
        write_goal(folder, "G01", 1, records=[END])
        summary = summarise.summarise(summarise.load_results([folder]))
        self.assertNotIn(str(self.tmp), json.dumps(summary))


if __name__ == "__main__":
    unittest.main()
