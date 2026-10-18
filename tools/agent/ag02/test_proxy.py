"""Tests of the AG-02 budget proxy (stdlib only; a fake engine on stdio pipes, no network)."""

import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

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


def live_args(ledger, *, max_live, goal="G01", per_goal=None):
    out = ["--max-live", str(max_live), "--ledger", str(ledger), "--goal-id", goal]
    return out + (["--max-live-goal", str(per_goal)] if per_goal is not None else [])


def engine_cmd(received, delay_ms=0):
    return [sys.executable, str(HERE / "fake_engine.py"), "--received", str(received),
            "--delay-action-ms", str(delay_ms)]


def converse(test, proxy_args, messages):
    """Sends each message and waits for its reply before the next: (replies, log records)."""
    scratch = tempfile.TemporaryDirectory(prefix="ag02-conv-")
    test.addCleanup(scratch.cleanup)
    tmp = Path(scratch.name)
    argv = [sys.executable, str(HERE / "proxy.py"), "--log", str(tmp / "p.jsonl"), *proxy_args, "--",
            *engine_cmd(tmp / "received.jsonl")]
    p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    replies = []
    for message in messages:
        p.stdin.write(json.dumps(message).encode() + b"\n")
        p.stdin.flush()
        if "id" in message:
            replies.append(json.loads(p.stdout.readline()))
    p.stdin.close()
    p.wait(timeout=30)
    records = [json.loads(l) for l in (tmp / "p.jsonl").read_text(encoding="utf-8").splitlines()]
    return replies, records


class Session:
    """One proxy process between these pipes and the fake engine."""

    def __init__(self, test, proxy_args, lines, *, ledger=None, delay_ms=0):
        self.dir = Path(tempfile.mkdtemp(prefix="ag02-test-"))
        test.addCleanup(self._cleanup)
        self.received = self.dir / f"received-{time.monotonic_ns()}.jsonl"
        self.log = self.dir / f"proxy-{time.monotonic_ns()}.jsonl"
        self.ledger = ledger or self.dir / "ledger.json"
        argv = [sys.executable, str(HERE / "proxy.py"), "--log", str(self.log), *proxy_args(self),
                "--", *engine_cmd(self.received, delay_ms)]
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

    def total(self):
        return proxy.read_ledger(self.ledger)["total"]


def live(max_live, per_goal=None, goal="G01"):
    return lambda s: live_args(s.ledger, max_live=max_live, goal=goal, per_goal=per_goal)


def dry(s):
    return ["--dry-run-only"]


class BudgetTests(unittest.TestCase):
    def test_live_calls_over_the_budget_are_refused_without_being_forwarded(self):
        s = Session(self, live(2), [action(1, dry_run=False), action(2, dry_run=False),
                                    action(3, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [1, 2])
        self.assertEqual(s.reply(3)["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(s.reply(1)["result"]["structuredContent"]["status"], "input_delivered")
        self.assertEqual(s.total(), 2)

    def test_dry_run_calls_do_not_spend_the_budget(self):
        s = Session(self, live(1), [action(1, dry_run=True), action(2), action(3, dry_run=False)])
        self.assertEqual(s.forwarded_ids(), [1, 2, 3])
        self.assertEqual(s.total(), 1)

    def test_the_ledger_accumulates_across_processes(self):
        first = Session(self, live(3), [action(1, dry_run=False), action(2, dry_run=False)])
        second = Session(self, live(3), [action(1, dry_run=False), action(2, dry_run=False)],
                         ledger=first.ledger)
        by_id = {r["id"]: r for r in second.replies}
        self.assertEqual(by_id[1]["result"]["structuredContent"]["status"], "input_delivered")
        self.assertEqual(by_id[2]["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(first.total(), 3)

    def test_restarting_the_proxy_for_the_same_goal_does_not_reset_its_cap(self):
        first = Session(self, live(10, per_goal=1), [action(1, dry_run=False)])
        again = Session(self, live(10, per_goal=1), [action(2, dry_run=False)], ledger=first.ledger)
        self.assertEqual(again.forwarded_ids(), [])
        self.assertEqual(again.reply(2)["error"]["data"]["proxy_refusal"], "LIVE_BUDGET_EXHAUSTED")
        other = Session(self, live(10, per_goal=1, goal="G02"), [action(3, dry_run=False)],
                        ledger=first.ledger)
        self.assertEqual(other.forwarded_ids(), [3])  # another goal has its own cap
        self.assertEqual(proxy.read_ledger(first.ledger), {"total": 2, "goals": {"G01": 1, "G02": 1}})

    def test_a_corrupt_or_old_format_ledger_refuses_every_live_call(self):
        for content in ("many", "3", '{"v":1,"total":-1,"goals":{}}', '{"v":2,"total":0,"goals":{}}',
                        '{"v":1,"total":0,"goals":{"bad id":1}}', '{"v":1,"total":0}'):
            def build(s, content=content):
                s.ledger.write_text(content)
                return live(5)(s)
            s = Session(self, build, [action(1, dry_run=False)])
            self.assertEqual(s.forwarded_ids(), [], content)
            self.assertEqual(s.reply(1)["error"]["data"]["proxy_refusal"], "LEDGER_UNREADABLE", content)

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
        self.assertEqual(s.total(), 1)  # refused calls spend nothing

    def test_a_non_boolean_dry_run_is_refused_in_both_modes(self):
        for build in (dry, live(5)):
            s = Session(self, build, [call(1, "execute_action", {"dry_run": "false"})])
            self.assertEqual(s.forwarded_ids(), [])
            self.assertEqual(s.reply(1)["error"]["data"]["proxy_refusal"], "DRY_RUN_NOT_BOOLEAN")


WORKER = """
import sys
sys.path.insert(0, {here!r})
import proxy
from pathlib import Path
policy = proxy.Policy(dry_run_only=False, max_live=3, goal_id="G01", max_goal=None,
                      ledger=Path({ledger!r}))
try:
    policy.check({{"dry_run": False, "action": "click"}})
    print("RESERVED")
except proxy.Refusal as refusal:
    print(refusal.reason)
"""


class LedgerTransactionTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-ledger-")
        self.addCleanup(scratch.cleanup)
        self.dir = Path(scratch.name)
        self.ledger = self.dir / "ledger.json"

    def test_concurrent_processes_never_oversell_the_budget(self):
        code = WORKER.format(here=str(HERE), ledger=str(self.ledger))
        procs = [subprocess.Popen([sys.executable, "-c", code], stdout=subprocess.PIPE, text=True)
                 for _ in range(8)]
        outputs = [p.communicate(timeout=60)[0].strip() for p in procs]
        self.assertEqual(outputs.count("RESERVED"), 3, outputs)
        self.assertEqual(proxy.read_ledger(self.ledger), {"total": 3, "goals": {"G01": 3}})
        self.assertEqual(set(outputs) - {"RESERVED"}, {"LIVE_BUDGET_EXHAUSTED"})

    def test_a_held_lock_refuses_instead_of_racing(self):
        lock_path = self.dir / "ledger.json.lock"
        with proxy.InterprocessLock(lock_path):
            with self.assertRaises(proxy.Refusal) as caught:
                with proxy.InterprocessLock(lock_path, wait_s=0.2):
                    pass
        self.assertEqual(caught.exception.reason, "LEDGER_LOCKED")
        with proxy.InterprocessLock(lock_path, wait_s=0.2):  # released afterwards
            pass

    def test_a_crash_while_writing_leaves_the_old_ledger_and_no_temporary_file(self):
        proxy.reserve(self.ledger, "G01", 5, None)
        before = self.ledger.read_bytes()
        with mock.patch("proxy.os.replace", side_effect=OSError("disk gone")):
            with self.assertRaises(proxy.Refusal) as caught:
                proxy.reserve(self.ledger, "G01", 5, None)
        self.assertEqual(caught.exception.reason, "LEDGER_UNWRITABLE")
        self.assertEqual(self.ledger.read_bytes(), before)
        self.assertEqual([p.name for p in self.dir.iterdir() if p.suffix == ".tmp"], [])

    def test_the_reservation_is_committed_before_anything_is_forwarded(self):
        # The crash point between the reservation and the forward: check() has returned and
        # the call was never sent. The spend is already on disk (conservative).
        policy = proxy.Policy(dry_run_only=False, max_live=2, goal_id="G01", max_goal=None,
                              ledger=self.ledger)
        self.assertTrue(policy.check({"dry_run": False, "action": "click"}))
        self.assertEqual(proxy.read_ledger(self.ledger)["total"], 1)

    def test_a_refusal_before_the_reservation_changes_nothing(self):
        policy = proxy.Policy(dry_run_only=False, max_live=2, goal_id="G01", max_goal=None,
                              ledger=self.ledger)
        with self.assertRaises(proxy.Refusal):
            policy.check({"dry_run": False, "action": "scroll"})
        self.assertFalse(self.ledger.exists())

    def test_the_temporary_name_is_unique_per_writer(self):
        names = set()
        real = os.replace

        def spy(src, dst):
            names.add(Path(src).name)
            return real(src, dst)
        with mock.patch("proxy.os.replace", side_effect=spy):
            for _ in range(3):
                proxy.reserve(self.ledger, "G01", 9, None)
        self.assertEqual(len(names), 3)


class PathTests(unittest.TestCase):
    def test_relative_and_ambiguous_paths_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            ok = str(Path(tmp) / "log.jsonl")
            self.assertIsNone(proxy.path_problem(ok))
            for bad in ("log.jsonl", "./log.jsonl", str(Path(tmp) / ".." / "log.jsonl"),
                        "\\\\?\\C:\\x\\log.jsonl", str(Path(tmp) / "log.jsonl:stream")):
                self.assertIsNotNone(proxy.path_problem(bad), bad)

    def test_a_relative_ledger_or_log_makes_the_proxy_refuse_to_start(self):
        for flags in (["--max-live", "1", "--goal-id", "G01", "--ledger", "LIBRO.txt", "--log",
                       str(Path(tempfile.gettempdir()) / "l.jsonl")],
                      ["--dry-run-only", "--log", "relative.jsonl"]):
            with self.assertRaises(SystemExit):
                proxy.parse_args([*flags, "--", "engine"])

    def test_the_log_and_ledger_must_be_outside_a_repository_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake_repo = Path(tmp) / "repo"
            (fake_repo / ".git").mkdir(parents=True)
            self.assertTrue(proxy.inside_git_checkout(fake_repo / "out" / "log.jsonl"))
            self.assertFalse(proxy.inside_git_checkout(Path(tmp) / "log.jsonl"))
            with self.assertRaises(SystemExit):
                proxy.parse_args(["--dry-run-only", "--log", str(fake_repo / "log.jsonl"), "--", "x"])

    def test_a_symlink_into_a_checkout_is_resolved_and_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            (repo / ".git").mkdir(parents=True)
            (repo / "out").mkdir()
            link = Path(tmp) / "alias"
            try:
                os.symlink(repo / "out", link, target_is_directory=True)
            except (OSError, NotImplementedError):
                self.skipTest("symbolic links are not available here")
            self.assertEqual(proxy.path_problem(link / "log.jsonl"),
                             "must be outside the repository checkout")

    def test_neither_mode_is_an_error(self):
        with self.assertRaises(SystemExit):
            proxy.parse_args(["--log", str(Path(tempfile.gettempdir()) / "x.jsonl"), "--", "engine"])

    def test_a_live_budget_needs_a_ledger_and_a_plain_goal_id(self):
        log = str(Path(tempfile.gettempdir()) / "x.jsonl")
        ledger = str(Path(tempfile.gettempdir()) / "ledger.json")
        for flags in (["--max-live", "1", "--log", log, "--ledger", ledger],
                      ["--max-live", "1", "--log", log, "--goal-id", "G01"],
                      ["--max-live", "1", "--log", log, "--ledger", ledger, "--goal-id", "a b"]):
            with self.assertRaises(SystemExit):
                proxy.parse_args([*flags, "--", "engine"])


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
        self.assertFalse(s.ledger.exists())


class InFlightIdTests(unittest.TestCase):
    def test_reusing_an_outstanding_id_is_refused_and_the_delivery_record_survives(self):
        # The engine answers the action late; an observe_screen with the same id arrives first.
        s = Session(self, live(5), [action(1, dry_run=False), call(1, "observe_screen", {})],
                    delay_ms=400)
        self.assertEqual(s.forwarded_ids(), [1])  # only the action reached the engine
        refusal = next(r for r in s.replies if "error" in r)
        self.assertEqual(refusal["error"]["data"]["proxy_refusal"], "DUPLICATE_ID")
        delivered = [r for r in s.records if r["dir"] == "engine->agent"
                     and r.get("tool") == "execute_action"]
        self.assertEqual([(r["status"], r["input_delivered"]) for r in delivered],
                         [("input_delivered", True)])
        self.assertEqual(s.total(), 1)  # the refused duplicate reserved nothing

    def test_a_duplicate_live_action_is_not_reserved(self):
        s = Session(self, live(5), [action(7, dry_run=False), action(7, dry_run=False)], delay_ms=300)
        self.assertEqual(s.forwarded_ids(), [7])
        self.assertEqual(s.total(), 1)

    def test_an_id_may_be_reused_once_its_reply_has_arrived_and_1_differs_from_the_string_1(self):
        replies, _ = converse(self, ["--dry-run-only"], [call(1, "observe_screen", {}),
                                                         call(1, "observe_screen", {}),  # after the reply
                                                         call("1", "observe_screen", {})])  # another type
        self.assertTrue(all("result" in r for r in replies), replies)



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
        text = s.log.read_text(encoding="utf-8")
        for secret in (SECRET_LABEL, "SEGRETO-CONSULTA", "clave123"):
            self.assertNotIn(secret, text)
        by_dir = [(r["dir"], r.get("tool"), r.get("status"), r.get("error_code")) for r in s.records]
        self.assertIn(("engine->agent", "execute_action", "input_delivered", None), by_dir)
        self.assertIn(("engine->agent", "unknown", "rejected", "TARGET_NOT_FOUND"), by_dir)
        sent = next(r for r in s.records if r["dir"] == "agent->engine" and r.get("tool") == "execute_action")
        # "e1" was issued by the engine in the observation at id 1 (the fake engine answers in order)
        self.assertEqual((sent["revision"], sent["text_len"], sent["live_counted"]), (1, 8, True))
        observed = next(r for r in s.records if r.get("tool") == "observe_screen"
                        and r["dir"] == "engine->agent")
        self.assertEqual(observed["elements"], 1)
        self.assertIn("latency_ms", observed)
        self.assertEqual(s.records[-1]["kind"], "end")

    def test_strings_the_agent_chooses_never_reach_the_log_as_metadata(self):
        canary = "PRIVATE-SCREEN-CANARY"
        s = Session(self, live(5), [
            {"jsonrpc": "2.0", "id": 1, "method": canary + "-METHOD", "params": {}},
            call(2, canary + "-TOOL", {}),
            action(3, dry_run=True, target=canary),
            action(4, dry_run=True, act=canary + "-ACTION"),
        ])
        self.assertNotIn(canary, s.log.read_text(encoding="utf-8"))
        records = [r for r in s.records if r["dir"] == "agent->engine"]
        self.assertEqual([r.get("method") for r in records], ["other", "tools/call", "tools/call",
                                                              "tools/call"])
        self.assertEqual([r.get("tool") for r in records[1:]], ["unknown", "execute_action", "execute_action"])
        self.assertEqual(records[2]["element_id"], "unknown")
        self.assertEqual(records[3]["action"], "invalid")

    def test_an_element_id_is_logged_only_after_the_engine_issued_it(self):
        _, records = converse(self, ["--dry-run-only"], [
            call(1, "observe_screen", {}), action(2, dry_run=True, target="e1"),
            action(3, dry_run=True, target="not-issued")])
        sent = [r for r in records if r["dir"] == "agent->engine" and r.get("tool") == "execute_action"]
        self.assertEqual([r["element_id"] for r in sent], ["e1", "unknown"])

    def test_engine_strings_outside_the_contract_enums_are_not_retained(self):
        reply = {"result": {"isError": True, "structuredContent": {
            "status": "PRIVATE-SCREEN-CANARY", "input_delivered": True,
            "observed_effect": "PRIVATE-SCREEN-CANARY",
            "error": {"code": "PRIVATE-SCREEN-CANARY", "message": "PRIVATE-SCREEN-CANARY"}}}}
        meta = proxy.response_meta(reply, "execute_action")
        self.assertNotIn("PRIVATE-SCREEN-CANARY", json.dumps(meta))
        self.assertEqual(meta, {"kind": "response", "is_error": True, "input_delivered": True})

    def test_a_refusal_is_logged_with_its_reason_and_no_content(self):
        s = Session(self, dry, [action(1, dry_run=False, act="type", text="secreto")])
        refusal = next(r for r in s.records if r["dir"] == "proxy->agent")
        self.assertEqual(refusal["reason"], "DRY_RUN_ONLY")
        self.assertNotIn("secreto", s.log.read_text(encoding="utf-8"))

    def test_the_proxy_never_touches_a_stop_file(self):
        source = (HERE / "proxy.py").read_text(encoding="utf-8").lower()
        for forbidden in ("stop_file", "stop.flag", "unlink(", "os.remove"):
            self.assertNotIn(forbidden, source.replace("never touches the operator's stop file", "")
                             .replace("os.unlink(tmp)", ""))


if __name__ == "__main__":
    unittest.main()
