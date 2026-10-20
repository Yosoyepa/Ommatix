"""AG-02 dry rehearsal: the proxy and a fake agent script against an engine, end to end.

Always runs against the fake engine (no network, no screen). When a built ``ommatix`` binary is
available (``OMMATIX_EXE``, or ``target/debug/ommatix`` of the checkout) it also runs against
``ommatix serve --source sim`` - the simulated desktop, never the screen - and proves the same
three things there: the proxy's budget refusal, its metadata-only log and its dry-run-only
refusal. No model, no API call, no input.
"""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import proxy  # noqa: E402

REPO = HERE.parents[2]


def real_engine():
    candidates = [os.environ.get("OMMATIX_EXE", "")]
    suffix = ".exe" if os.name == "nt" else ""
    candidates.append(str(REPO / "target" / "debug" / f"ommatix{suffix}"))
    return next((c for c in candidates if c and Path(c).is_file()), None)


def rehearse(test, engine_argv, proxy_flags, profile, live_calls=3):
    scratch = tempfile.TemporaryDirectory(prefix="ag02-rehearsal-")
    test.addCleanup(scratch.cleanup)
    tmp = Path(scratch.name)
    log, ledger, report = tmp / "proxy.jsonl", tmp / "ledger.txt", tmp / "report.json"
    argv = [sys.executable, str(HERE / "proxy.py"), "--log", str(log),
            *[f.format(ledger=ledger) for f in proxy_flags], "--", *engine_argv]
    # The fake agent is the proxy's client: the proxy's stdin/stdout are wired to its stdout/stdin.
    proxy = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL)
    agent = subprocess.Popen(
        [sys.executable, str(HERE / "fake_agent.py"), "--report", str(report), "--profile", profile,
         "--live-calls", str(live_calls)],
        stdin=proxy.stdout, stdout=proxy.stdin, stderr=subprocess.DEVNULL)
    agent_code = agent.wait(timeout=120)
    proxy.stdin.close()
    proxy.stdout.close()
    proxy.wait(timeout=60)
    records = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines()]
    return {"agent_code": agent_code, "records": records, "ledger": ledger,
            "report": json.loads(report.read_text(encoding="utf-8")) if report.exists() else None,
            "log_text": log.read_text(encoding="utf-8"), "dir": tmp}


def fake_engine():
    received = Path(tempfile.gettempdir()) / f"ag02-received-{os.getpid()}.jsonl"
    return [sys.executable, str(HERE / "fake_engine.py"), "--received", str(received)], received


class RehearsalBase:
    profile = "legacy"

    def engine(self):
        raise NotImplementedError

    def check_budget_refusal(self):
        engine_argv, cleanup = self.engine()
        result = rehearse(self, engine_argv, ["--max-live", "2", "--ledger", "{ledger}", "--goal-id", "G01"], self.profile)
        self.assertEqual(result["agent_code"], 0)
        live = result["report"]["live"]
        self.assertEqual([("rpc_error" in o) for o in live], [False, False, True])
        self.assertEqual(live[2]["rpc_error"], "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(proxy.read_ledger(result["ledger"])["total"], 2)
        forwarded_live = [r for r in result["records"] if r["dir"] == "agent->engine" and r.get("live_counted")]
        self.assertEqual(len(forwarded_live), 2)
        self.assertEqual(result["report"]["dry"].get("rpc_error"), None)
        cleanup()
        return result

    def check_metadata_only(self, result):
        # Everything the log may hold, by key; no label, text or message field ever exists.
        allowed = {"t_ms", "dir", "bytes", "kind", "method", "tool", "action", "dry_run",
                   "element_id", "revision", "text_len", "live_counted", "is_error", "status",
                   "error_code", "input_delivered", "verified", "observed_effect", "elements",
                   "rpc_error_code", "latency_ms", "reason", "engine_exit", "live_spent_here",
                   # observation evidence (PE-13, PE-14): booleans, bounded numbers, closed names
                   "request_index", "observe_index", "query_present", "limit", "cursor_present",
                   "total_elements", "truncated", "timings_us", "plan"}
        for record in result["records"]:
            self.assertLessEqual(set(record), allowed, record)
            for nested in (record.get("timings_us") or {}, record.get("plan") or {}):
                for value in nested.values():
                    self.assertTrue(value is None or isinstance(value, (bool, int))
                                    or value in (proxy.PLAN_WORK | proxy.PLAN_REASONS | {"other"}), record)
        self.assertNotIn("ETIQUETA-SECRETA", result["log_text"])

    def check_dry_run_only(self):
        engine_argv, cleanup = self.engine()
        result = rehearse(self, engine_argv, ["--dry-run-only"], self.profile)
        self.assertEqual(result["agent_code"], 0)
        self.assertTrue(all(o.get("rpc_error") == "DRY_RUN_ONLY" for o in result["report"]["live"]))
        self.assertEqual([r for r in result["records"]
                          if r["dir"] == "agent->engine" and r.get("live_counted")], [])
        # the live calls never reached the engine: only the dry run was forwarded
        forwarded = [r for r in result["records"] if r["dir"] == "agent->engine"
                     and r.get("tool") == "execute_action"]
        self.assertEqual([r["dry_run"] for r in forwarded], [True])
        cleanup()
        return result


class FakeEngineRehearsal(RehearsalBase, unittest.TestCase):
    def engine(self):
        argv, received = fake_engine()
        return argv, lambda: received.exists() and received.unlink()

    def test_the_budget_refuses_the_third_live_call_end_to_end(self):
        self.check_budget_refusal()

    def test_the_log_is_metadata_only_end_to_end(self):
        self.check_metadata_only(self.check_budget_refusal())

    def test_phase_one_refuses_every_live_call_end_to_end(self):
        self.check_dry_run_only()


@unittest.skipUnless(real_engine(), "no built ommatix binary (set OMMATIX_EXE)")
class SimulatedDesktopRehearsal(RehearsalBase, unittest.TestCase):
    def engine(self):
        return [real_engine(), "serve", "--source", "sim", "--legacy",
                "--session-id", "ag02-rehearsal"], lambda: None

    def test_the_budget_refuses_the_third_live_call_on_the_simulated_desktop(self):
        result = self.check_budget_refusal()
        self.check_metadata_only(result)
        observed = [r for r in result["records"] if r.get("tool") == "observe_screen"
                    and r["dir"] == "engine->agent"]
        self.assertTrue(observed and observed[0]["elements"] > 0)

    def test_phase_one_refuses_every_live_call_on_the_simulated_desktop(self):
        self.check_dry_run_only()


if __name__ == "__main__":
    unittest.main()
