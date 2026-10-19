"""Regression tests of the review of PR 96 (R1-R3): the route's token cap must fail closed on unknown or invalid
consumption (R1), read decoded and validated data and never the bytes (R2), and return only once the session's
whole process tree (CLI, budget proxy, engine) has exited (R3).

No network, no real CLI: fake children print scripted streams or start the real budget proxy."""

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import run_goal as rg  # noqa: E402
from test_routecost import init_event  # noqa: E402

# A fake CLI that prints scripted stream lines exactly as given (one per line of a file), then waits as a long
# session would ("wait") or ends on its own ("exit").
STREAM_CLI = """import sys, time
init = open(sys.argv[1], encoding="utf-8").read()
sys.stdin.read()
print(init, flush=True)
for line in open(sys.argv[2], encoding="utf-8").read().splitlines():
    print(line, flush=True)
if sys.argv[3] == "wait":
    time.sleep(60)
"""


def assistant(message_id, usage=None):
    message = {"id": message_id, "role": "assistant", "content": []}
    if usage is not None:
        message["usage"] = usage
    return json.dumps({"type": "assistant", "message": message})


class UsageGuardTests(unittest.TestCase):
    """R1 and R2: unknown or invalid consumption ends the session; the cap reads decoded, validated data."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ag02-guard-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "init.json").write_text(json.dumps(init_event()), encoding="utf-8")
        (self.root / "cli.py").write_text(STREAM_CLI, encoding="utf-8")

    def run_stream(self, lines, cap=100, wait=True, timeout=30):
        (self.root / "lines.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
        argv = [sys.executable, str(self.root / "cli.py"), str(self.root / "init.json"),
                str(self.root / "lines.txt"), "wait" if wait else "exit"]
        metrics = rg.StreamMetrics([], None, None)
        started = time.monotonic()
        ran = rg.run_claude(argv, "prompt", self.root, metrics, timeout, max_context_tokens=cap)
        return ran, time.monotonic() - started

    def assert_killed(self, lines, reason):
        ran, took = self.run_stream(lines)
        self.assertEqual(ran["killed"], reason, lines)
        self.assertLess(took, 20, "the runner must end the session promptly, not wait for the time cap")

    def test_unknown_consumption_ends_the_session(self):
        self.assert_killed([assistant("m1")], "usage_invalid")  # no usage at all
        self.assert_killed([assistant("m1", {})], "usage_invalid")  # an empty usage is not zero
        self.assert_killed([assistant("m1", {"output_tokens": 5})], "usage_invalid")  # no input counter
        self.assert_killed(['{"type": "assistant", "message": 7}'], "usage_invalid")

    def test_wrong_types_negatives_booleans_and_nonfinite_values_end_the_session(self):
        for usage in ({"input_tokens": "1000"}, {"input_tokens": 1000.0}, {"input_tokens": True},
                      {"input_tokens": None}, {"input_tokens": 10, "cache_read_input_tokens": -1000},
                      {"input_tokens": 10, "cache_creation_input_tokens": "5"}, {"input_tokens": [3]},
                      {"input_tokens": -1}):
            self.assert_killed([assistant("m1", usage)], "usage_invalid")
        for constant in ("Infinity", "NaN"):
            self.assert_killed(['{"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": '
                                + constant + "}}}"], "usage_invalid")

    def test_missing_or_invalid_message_ids_end_the_session(self):
        for message_id in (None, "", 5, ["m"], "x" * 500):
            line = json.dumps({"type": "assistant", "message": {"id": message_id, "usage": {"input_tokens": 10}}})
            self.assert_killed([line], "usage_invalid")
        self.assert_killed([json.dumps({"type": "assistant", "message": {"usage": {"input_tokens": 1000}}})],
                           "usage_invalid")

    def test_oversized_numbers_and_unreadable_events_end_the_session(self):
        self.assert_killed(['{"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": '
                            + "9" * 5000 + "}}}"], "usage_invalid")
        self.assert_killed([assistant("m1", {"input_tokens": 10 ** 100})], "usage_invalid")  # beyond any real count
        # an event that cannot be read is not evidence of low consumption
        self.assert_killed(['{"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": 10'],
                           "usage_invalid")

    def test_a_decreasing_update_never_lowers_the_spend(self):
        self.assert_killed([assistant("m1", {"input_tokens": 60}), assistant("m1", {"input_tokens": 0}),
                            assistant("m2", {"input_tokens": 60})], "token_cap")

    def test_the_usage_key_may_be_a_json_escape_it_is_the_decoded_data_that_counts(self):
        line = '{"type": "assistant", "message": {"id": "m1", "us\\u0061ge": {"input_tokens": 1000}}}'
        self.assertNotIn("usage", line)  # the bytes do not contain the word a raw-text gate would search for
        self.assert_killed([line], "token_cap")

    def test_valid_streams_count_per_distinct_message_and_are_not_killed_under_the_cap(self):
        self.assert_killed([assistant("m1", {"input_tokens": 60}), assistant("m2", {"input_tokens": 60})], "token_cap")
        self.assert_killed([assistant("m1", {"input_tokens": 10, "cache_creation_input_tokens": 60,
                                             "cache_read_input_tokens": 60})], "token_cap")
        ran, _ = self.run_stream([assistant("m1", {"input_tokens": 60}), assistant("m1", {"input_tokens": 60})],
                                 wait=False)  # the same message reported twice counts once
        self.assertIsNone(ran["killed"])
        ran, _ = self.run_stream([assistant("m1", {"input_tokens": 40, "output_tokens": 999999})], wait=False)
        self.assertIsNone(ran["killed"])  # output tokens are not context

    def test_a_result_that_reports_more_than_the_cap_is_flagged_even_without_message_usage(self):
        result = json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": "VERDICT: DONE",
                             "usage": {"input_tokens": 1000, "cache_read_input_tokens": 0}})
        ran, _ = self.run_stream([result], wait=False)
        self.assertEqual(ran["killed"], "token_cap")


# An engine that is slow to leave after its stdin closes, behind the real budget proxy, started by a fake CLI
# the way Claude Code starts its MCP server.
TREE_ENGINE = """import json, os, sys, time
open(sys.argv[1], "w").write(str(os.getpid()))
for raw in sys.stdin:
    request = json.loads(raw)
    if "id" in request:
        body = {"status": "simulated", "input_delivered": False, "verified": False}
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": {
            "content": [{"type": "text", "text": json.dumps(body)}], "structuredContent": body,
            "isError": False}}), flush=True)
time.sleep(4)
"""

TREE_CLI = """import json, subprocess, sys, time
init, proxy, log, ledger, engine, pids = sys.argv[1:7]
sys.stdin.read()
print(open(init, encoding="utf-8").read(), flush=True)
p = subprocess.Popen([sys.executable, proxy, "--log", log, "--max-live", "5", "--ledger", ledger, "--goal-id", "G01",
                      "--", sys.executable, engine, pids + ".engine"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
open(pids + ".proxy", "w").write(str(p.pid))
call = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "execute_action", "arguments": {
    "request_id": "r1", "session_id": "s", "snapshot_id": "snap", "geometry_epoch": 1, "target_id": "e1",
    "expected_revision": 1, "action": "click", "dry_run": False}}}
p.stdin.write(json.dumps(call).encode() + b"\\n")
p.stdin.flush()
p.stdout.readline()
print(json.dumps({"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": 1000}}}), flush=True)
time.sleep(60)
"""


def alive(pid):
    if os.name == "nt":
        import ctypes
        handle = ctypes.windll.kernel32.OpenProcess(0x1000, 0, pid)
        if not handle:
            return False
        code = ctypes.c_ulong()
        ctypes.windll.kernel32.GetExitCodeProcess(handle, ctypes.byref(code))
        ctypes.windll.kernel32.CloseHandle(handle)
        return code.value == 259
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    try:  # a zombie is not running
        return Path(f"/proc/{pid}/stat").read_text().split(")")[-1].split()[0] != "Z"
    except OSError:
        return True


class SessionTreeTests(unittest.TestCase):
    """R3: a cap (or any end) returns only once the CLI and what it started have exited."""

    def test_the_cap_returns_only_after_the_proxy_and_the_engine_have_exited(self):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-tree-")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        (root / "init.json").write_text(json.dumps(init_event()), encoding="utf-8")
        (root / "cli.py").write_text(TREE_CLI, encoding="utf-8")
        (root / "engine.py").write_text(TREE_ENGINE, encoding="utf-8")
        log, ledger, pids = root / "p.jsonl", root / "ledger.json", root / "pid"
        argv = [sys.executable, str(root / "cli.py"), str(root / "init.json"), str(HERE / "proxy.py"), str(log),
                str(ledger), str(root / "engine.py"), str(pids)]
        metrics = rg.StreamMetrics([], None, None)
        started = time.monotonic()
        ran = rg.run_claude(argv, "prompt", root, metrics, 60, max_context_tokens=100)
        took = time.monotonic() - started
        survivors = [pid for pid in (int((root / "pid.proxy").read_text()), int((root / "pid.engine").read_text()))
                     if alive(pid)]
        for pid in survivors:  # never leave a stray behind, whatever the verdict
            subprocess.run(["taskkill", "/PID", str(pid), "/F"] if os.name == "nt" else ["kill", "-9", str(pid)],
                           capture_output=True)
        self.assertEqual(ran["killed"], "token_cap")
        self.assertEqual(survivors, [], "the proxy and the engine must be gone when run_claude returns")
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        self.assertTrue([r for r in records if r.get("kind") == "end"], "the proxy must have written its receipts")
        # the reservation made before the cap is kept: nothing refunds a delivered or reserved call
        self.assertEqual(json.loads(ledger.read_text(encoding="utf-8")), {"goals": {"G01": 1}, "total": 1, "v": 1})
        self.assertLess(took, 50)

    def test_a_timeout_also_waits_for_the_tree(self):
        tmp = tempfile.TemporaryDirectory(prefix="ag02-tree-")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        (root / "init.json").write_text(json.dumps(init_event()), encoding="utf-8")
        (root / "cli.py").write_text(TREE_CLI.replace(
            'print(json.dumps({"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": 1000}}}), '
            "flush=True)\n", ""), encoding="utf-8")
        (root / "engine.py").write_text(TREE_ENGINE, encoding="utf-8")
        pids = root / "pid"
        argv = [sys.executable, str(root / "cli.py"), str(root / "init.json"), str(HERE / "proxy.py"),
                str(root / "p.jsonl"), str(root / "ledger.json"), str(root / "engine.py"), str(pids)]
        ran = rg.run_claude(argv, "prompt", root, rg.StreamMetrics([], None, None), 6)
        survivors = [pid for pid in (int((root / "pid.proxy").read_text()), int((root / "pid.engine").read_text()))
                     if alive(pid)]
        for pid in survivors:
            subprocess.run(["taskkill", "/PID", str(pid), "/F"] if os.name == "nt" else ["kill", "-9", str(pid)],
                           capture_output=True)
        self.assertEqual(ran["killed"], "timeout")
        self.assertEqual(survivors, [])


if __name__ == "__main__":
    unittest.main()
