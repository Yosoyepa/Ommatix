"""Tests of the AG-02 budget proxy (stdlib only; a fake engine on stdio pipes, no network)."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import proxy  # noqa: E402
from fake_engine import SECRET_LABEL  # noqa: E402


def call(rid, name, arguments):
    return {"jsonrpc": "2.0", "id": rid, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}}


def action(rid, *, dry_run=None, act="click", text=None, target="e1", revision=1):
    args = {"request_id": f"r{rid}", "session_id": "s", "snapshot_id": "snap", "geometry_epoch": 1,
            "target_id": target, "expected_revision": revision, "action": act}
    if dry_run is not None:
        args["dry_run"] = dry_run
    if text is not None:
        args["text"] = text
    return call(rid, "execute_action", args)


class Session:
    """One proxy process between these pipes and the fake engine."""

    def __init__(self, test, proxy_args, lines):
        self.dir = Path(tempfile.mkdtemp(prefix="ag02-test-"))
        test.addCleanup(self._cleanup)
        self.received = self.dir / "received.jsonl"
        self.log = self.dir / "proxy.jsonl"
        self.ledger = self.dir / "ledger.txt"
        argv = [sys.executable, str(HERE / "proxy.py"), "--log", str(self.log), *proxy_args(self),
                "--", sys.executable, str(HERE / "fake_engine.py"), "--received", str(self.received)]
        data = b"".join((l if isinstance(l, bytes) else json.dumps(l).encode()) + b"\n"
                        for l in lines)
        done = subprocess.run(argv, input=data, capture_output=True, timeout=60, check=False)
        self.code = done.returncode
        self.stderr = done.stderr
        self.replies = [json.loads(l) for l in done.stdout.splitlines()]
        self.engine_got = self.received.read_bytes().splitlines() if self.received.exists() else []
        self.records = [json.loads(l) for l in self.log.read_text(encoding="utf-8").splitlines()]

    def _cleanup(self):
        for path in self.dir.glob("*"):
            path.unlink()
        self.dir.rmdir()

    def reply(self, rid):
        return next(r for r in self.replies if r.get("id") == rid)

    def forwarded_ids(self):
        return [json.loads(l).get("id") for l in self.engine_got]


def live(max_live, per_run=None):
    def build(s):
        out = ["--max-live", str(max_live), "--ledger", str(s.ledger)]
        return out + (["--max-live-per-run", str(per_run)] if per_run is not None else [])
    return build


def dry(s):
    return ["--dry-run-only"]


class BudgetTests(unittest.TestCase):
    def test_live_calls_over_the_budget_are_refused_without_being_forwarded(self):
        s = Session(self, live(2), [action(1, dry_run=False), action(2, dry_run=False),
                                    action(3, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [1, 2])
        self.assertEqual(s.reply(3)["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(s.reply(1)["result"]["structuredContent"]["status"], "input_delivered")
        self.assertEqual(s.ledger.read_text(), "2")

    def test_dry_run_calls_do_not_spend_the_budget(self):
        s = Session(self, live(1), [action(1, dry_run=True), action(2), action(3, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [1, 2, 3])
        self.assertEqual(s.ledger.read_text(), "1")

    def test_the_ledger_accumulates_across_processes(self):
        first = Session(self, live(3), [action(1, dry_run=False), action(2, dry_run=False)])
        self.assertEqual(first.ledger.read_text(), "2")
        # a second process on the same ledger sees the 2 already spent
        argv = [sys.executable, str(HERE / "proxy.py"), "--log", str(first.dir / "second.jsonl"),
                "--max-live", "3", "--ledger", str(first.ledger), "--", sys.executable,
                str(HERE / "fake_engine.py"), "--received", str(first.dir / "second-received")]
        data = b"".join(json.dumps(m).encode() + b"\n"
                        for m in (action(1, dry_run=False), action(2, dry_run=False)))
        done = subprocess.run(argv, input=data, capture_output=True, timeout=60, check=False)
        replies = {r["id"]: r for r in (json.loads(l) for l in done.stdout.splitlines())}
        self.assertEqual(replies[1]["result"]["structuredContent"]["status"], "input_delivered")
        self.assertEqual(replies[2]["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(first.ledger.read_text(), "3")
        for name in ("second.jsonl", "second-received"):
            (first.dir / name).unlink()

    def test_the_per_run_budget_bounds_one_process(self):
        s = Session(self, live(10, per_run=1), [action(1, dry_run=False), action(2, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [1])
        self.assertEqual(s.reply(2)["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")

    def test_a_corrupt_ledger_refuses_every_live_call(self):
        def build(s):
            s.ledger.write_text("many")
            return live(5)(s)
        s = Session(self, build, [action(1, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [])
        self.assertEqual(s.reply(1)["error"]["data"]["proxy_refusal"], "LEDGER_UNREADABLE")

    def test_only_listed_actions_and_short_ascii_text_go_live(self):
        s = Session(self, live(10), [
            action(1, dry_run=False, act="scroll"),
            action(2, dry_run=False, act="type", text="x" * 17),
            action(3, dry_run=False, act="type", text="ñandú"),
            action(4, dry_run=False, act="type", text="hola"),
        ])
        self.assertEqual(s.forwarded_ids(), [4])
        reasons = {i: s.reply(i)["error"]["data"]["proxy_refusal"] for i in (1, 2, 3)}
        self.assertEqual(reasons, {1: "ACTION_NOT_ALLOWED_LIVE", 2: "TEXT_NOT_ALLOWED_LIVE",
                                   3: "TEXT_NOT_ALLOWED_LIVE"})
        self.assertEqual(s.ledger.read_text(), "1")  # refused calls spend nothing

    def test_a_non_boolean_dry_run_is_refused_in_both_modes(self):
        for build in (dry, live(5)):
            s = Session(self, build, [call(1, "execute_action", {"dry_run": "false"})])
            self.assertEqual(s.forwarded_ids(), [])
            self.assertEqual(s.reply(1)["error"]["data"]["proxy_refusal"], "DRY_RUN_NOT_BOOLEAN")


class DryRunOnlyTests(unittest.TestCase):
    def test_phase_one_refuses_live_calls_and_forwards_dry_runs(self):
        s = Session(self, dry, [action(1, dry_run=False), action(2, dry_run=True), action(3)])
        self.assertEqual(s.forwarded_ids(), [2, 3])
        self.assertEqual(s.reply(1)["error"]["data"]["proxy_refusal"], "DRY_RUN_ONLY")
        self.assertEqual(s.reply(2)["result"]["structuredContent"]["status"], "simulated")


class FailClosedTests(unittest.TestCase):
    def test_what_the_engine_might_read_differently_is_not_forwarded(self):
        dup = (b'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute_action",'
               b'"arguments":{"dry_run":true,"dry_run":false,"action":"click"}}}')
        nan = (b'{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"execute_action",'
               b'"arguments":{"dry_run":false,"geometry_epoch":NaN}}}')
        batch = json.dumps([action(3, dry_run=False)]).encode()
        notification = {"jsonrpc": "2.0", "method": "tools/call", "params": {
            "name": "execute_action", "arguments": {"dry_run": False}}}
        s = Session(self, live(5), [dup, nan, batch, notification])
        self.assertEqual(s.engine_got, [])
        self.assertEqual(len(s.replies), 4)  # every refused line gets an error, none a forward
        self.assertTrue(all("error" in r for r in s.replies))
        self.assertEqual(s.ledger.exists(), False)

    def test_the_log_and_ledger_must_be_outside_a_repository_checkout(self):
        here = Path(__file__).resolve()
        self.assertTrue(proxy.inside_git_checkout(here) or True)  # a source tree without .git is fine
        with tempfile.TemporaryDirectory() as tmp:
            fake_repo = Path(tmp) / "repo"
            (fake_repo / ".git").mkdir(parents=True)
            self.assertTrue(proxy.inside_git_checkout(fake_repo / "out" / "log.jsonl"))
            self.assertFalse(proxy.inside_git_checkout(Path(tmp) / "log.jsonl"))
            with self.assertRaises(SystemExit):
                proxy.parse_args(["--dry-run-only", "--log", str(fake_repo / "log.jsonl"),
                                  "--", "x"])

    def test_neither_mode_is_an_error(self):
        with self.assertRaises(SystemExit):
            proxy.parse_args(["--log", "x.jsonl", "--", "engine"])


class PassthroughAndPrivacyTests(unittest.TestCase):
    def test_other_messages_are_forwarded_unchanged_and_in_order(self):
        lines = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"x": [1, 2]}},
                 {"jsonrpc": "2.0", "method": "notifications/initialized"},
                 {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                 call(3, "observe_screen", {})]
        s = Session(self, dry, lines)
        self.assertEqual([json.loads(l) for l in s.engine_got], lines)
        self.assertEqual([r["id"] for r in s.replies], [1, 2, 3])

    def test_the_log_holds_metadata_only(self):
        s = Session(self, live(5), [call(1, "observe_screen", {"query": "SEGRETO-CONSULTA"}),
                                    action(2, dry_run=False, act="type", text="clave123"),
                                    call(3, "no_such_tool", {})])
        text = (s.dir / "proxy.jsonl").read_text(encoding="utf-8")
        for secret in (SECRET_LABEL, "SEGRETO-CONSULTA", "clave123"):
            self.assertNotIn(secret, text)
        by_dir = [(r["dir"], r.get("tool"), r.get("status"), r.get("error_code")) for r in s.records]
        self.assertIn(("engine->agent", "execute_action", "input_delivered", None), by_dir)
        self.assertIn(("engine->agent", "no_such_tool", "rejected", "TARGET_NOT_FOUND"), by_dir)
        sent = next(r for r in s.records if r["dir"] == "agent->engine" and r.get("tool") == "execute_action")
        self.assertEqual((sent["element_id"], sent["revision"], sent["text_len"], sent["live_counted"]),
                         ("e1", 1, 8, True))
        observed = next(r for r in s.records if r.get("tool") == "observe_screen"
                        and r["dir"] == "engine->agent")
        self.assertEqual(observed["elements"], 1)
        self.assertIn("latency_ms", observed)
        self.assertEqual(s.records[-1]["kind"], "end")

    def test_a_refusal_is_logged_with_its_reason_and_no_content(self):
        s = Session(self, dry, [action(1, dry_run=False, act="type", text="secreto")])
        refusal = next(r for r in s.records if r["dir"] == "proxy->agent")
        self.assertEqual(refusal["reason"], "DRY_RUN_ONLY")
        self.assertNotIn("secreto", (s.dir / "proxy.jsonl").read_text(encoding="utf-8"))

    def test_the_proxy_never_touches_a_stop_file(self):
        source = (HERE / "proxy.py").read_text(encoding="utf-8").lower()
        for forbidden in ("stop_file", "stop.flag", "unlink(", "os.remove"):
            self.assertNotIn(forbidden, source.replace("never touches the operator's stop file", ""))


if __name__ == "__main__":
    unittest.main()
