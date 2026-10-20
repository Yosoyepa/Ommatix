"""AG-03 privacy (ADR-0038 decision 5, O5): images live in memory only, to answer the call. No image
bytes, no hash of an image, no screen text and no typed text in any result, log or file; no stderr is
stored. These tests look at what the server, the runner and the preflight actually write."""

import base64
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_os  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_server as ps  # noqa: E402
import test_runner as tr  # noqa: E402

LOG_KEYS = {"t_ms", "tool", "ok", "code", "width", "height", "png_bytes", "screenshots", "latency_ms", "label",
            "text_len", "reserved", "delivered", "events", "stage", "limit", "reason", "end"}
HEX64 = re.compile(r"[0-9a-f]{64}")
TYPED = "hola-secreta"[:16]


def rpc(process, message):
    process.stdin.write((json.dumps(message) + "\n").encode())
    process.stdin.flush()
    return json.loads(process.stdout.readline())


class ServerOutputTests(unittest.TestCase):
    def run_server(self, *calls):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-priv-")
        self.addCleanup(scratch.cleanup)
        self.dir = Path(scratch.name)
        self.log = self.dir / "pixel.jsonl"
        process = subprocess.Popen(
            [sys.executable, "-I", str(HERE / "pixel_server.py"), "--synthetic", "--synthetic-size", "320x200",
             "--log", str(self.log), "--vocabulary", "Sumar,Restar"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=self.dir)
        replies = [rpc(process, {"jsonrpc": "2.0", "id": i, "method": "tools/call",
                                 "params": {"name": name, "arguments": args}})
                   for i, (name, args) in enumerate(calls, start=1)]
        process.stdin.close()
        stdout, stderr = process.stdout.read(), process.stderr.read()
        process.wait(timeout=20)
        process.stdout.close()
        process.stderr.close()
        return replies, stdout, stderr

    def test_the_server_writes_one_metadata_log_and_nothing_else_and_stores_no_stderr(self):
        replies, stdout, stderr = self.run_server(("screenshot", {}), ("click", {"x": 3, "y": 4}),
                                                  ("type_text", {"text": TYPED}), ("click", {"x": 999, "y": 1}))
        self.assertEqual(stdout, b"")  # every reply was read from the pipe; nothing stray on stdout
        self.assertEqual(stderr, b"")
        self.assertEqual(sorted(p.name for p in self.dir.rglob("*") if p.is_file()), ["pixel.jsonl"])
        self.assertTrue(replies[3]["result"]["isError"])

    def test_the_log_has_only_the_closed_keys_and_no_image_hash_or_text(self):
        self.run_server(("screenshot", {}), ("click", {"x": 3, "y": 4}), ("type_text", {"text": TYPED}),
                        ("screenshot", {}))
        text = self.log.read_text(encoding="utf-8")
        for line in text.splitlines():
            record = json.loads(line)
            self.assertLessEqual(set(record), LOG_KEYS, record)
            for value in record.values():
                if isinstance(value, str):
                    self.assertIn(value, {"screenshot", "click", "type_text", "server", "other", "unknown"} | set(ps.RESULT_CODES)
                                  | ps.CAPTURE_REASONS | {"initial", "delivery", "send", "tool_calls", "screenshots"})
        self.assertNotIn(TYPED, text)
        self.assertFalse(HEX64.search(text), "a 64-hex string (a hash) is in the log")
        self.assertNotIn("iVBOR", text)
        self.assertNotIn("base64", text)

    def test_only_the_screenshot_result_carries_an_image_and_the_others_are_text_only(self):
        replies, *_ = self.run_server(("screenshot", {}), ("click", {"x": 3, "y": 4}), ("type_text", {"text": "a"}),
                                      ("click", {"x": 9999, "y": 4}))
        self.assertEqual([c["type"] for c in replies[0]["result"]["content"]], ["image", "text"])
        for reply in replies[1:]:
            self.assertEqual({c["type"] for c in reply["result"]["content"]}, {"text"})
        image = replies[0]["result"]["content"][0]
        self.assertEqual(image["mimeType"], "image/png")
        self.assertEqual(cap.png_info(base64.b64decode(image["data"])), (320, 200))

    def test_the_click_label_is_a_closed_value_and_no_hit_test_reading_reaches_the_agent(self):
        replies, *_ = self.run_server(("screenshot", {}), ("click", {"x": 3, "y": 4}))
        self.assertEqual(json.loads(replies[1]["result"]["content"][0]["text"]), {"status": "simulated"})
        records = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertIn(records[1]["label"], {"unknown", "other", "Sumar", "Restar"})


class RealModeFakeTests(unittest.TestCase):
    def test_a_fake_real_session_logs_no_pixels_and_returns_the_image_only_to_the_caller(self):
        from test_guards import Rig
        rig = Rig(self)
        shot = rig.shoot()
        rig.call("click", x=1, y=1)
        rig.call("type_text", text=TYPED)
        text = json.dumps(rig.records)
        self.assertNotIn("iVBOR", text)
        self.assertNotIn(TYPED, text)
        self.assertFalse(HEX64.search(text))
        for record in rig.records:
            self.assertLessEqual(set(record), LOG_KEYS)
        self.assertEqual(shot["content"][0]["type"], "image")


class ResultAndSourceTests(unittest.TestCase):
    def test_a_goal_result_holds_no_image_text_typed_text_or_stderr(self):
        h = tr.Harness(self, "G06", 1, final_state=tr.GOALS["initial_states"]["testapp"],
                       events=tr.session("Campo de texto", text="hola"),
                       server_records=[tr.shot(), tr.click("Campo de texto"), tr.END])
        result = h.run()
        text = json.dumps(result, default=str)
        self.assertNotIn("iVBOR", text)
        self.assertNotIn("base64", text)
        self.assertNotIn('"hola"', text)
        self.assertFalse(HEX64.search(re.sub(r"\"(goals_sha256|server_sha256|instruction_sha256)\": \"[0-9a-f]{64}\"",
                                             "", text)))
        self.assertNotIn("stderr", text)
        kwargs = h.captured["kwargs"]
        self.assertLessEqual(set(kwargs), {"env", "max_context_tokens"})  # no stderr_path: production stores none
        self.assertEqual(len(h.captured["args"]), 0)

    def test_the_directory_of_a_run_holds_no_image_file(self):
        h = tr.Harness(self, "G01", 1, final_state=tr.GOALS["initial_states"]["testapp"], events=tr.session("Sumar"),
                       server_records=[tr.shot(), tr.click("Sumar"), tr.END])
        h.run()
        names = sorted(p.suffix for p in h.tmp.rglob("*") if p.is_file())
        for forbidden in (".png", ".jpg", ".jpeg", ".bmp", ".raw", ".rgba"):
            self.assertNotIn(forbidden, names)

    def test_no_module_writes_a_file_except_the_reviewed_sites(self):
        # The only writers of the ag03 package: the server's metadata log, the runner's per-goal result,
        # MCP config and stop file, the probe's and the preflight's result files. Nothing writes pixels.
        allowed = {"pixel_server.py": 1, "pixel_runner.py": 3, "pixel_preflight.py": 2, "probe_image.py": 2}
        pattern = re.compile(r"\.write_text\(|\.write_bytes\(|open\([^)]*[\"'](?:w|a|wb|ab|w\+|a\+)[\"']")
        for path in sorted(HERE.glob("*.py")):
            if path.name.startswith(("test_", "fake_")):
                continue
            count = sum(1 for line in path.read_text(encoding="utf-8").splitlines()
                        if pattern.search(line) and not line.lstrip().startswith("#"))
            self.assertEqual(count, allowed.get(path.name, 0), path.name)

    def test_the_capture_and_win32_modules_hold_images_only_in_memory(self):
        for name in ("pixel_capture.py", "pixel_win32.py", "pixel_guards.py", "pixel_agent.py"):
            source = (HERE / name).read_text(encoding="utf-8")
            self.assertNotRegex(source, r"\.write_(text|bytes)\(", name)
            self.assertNotIn("NamedTemporaryFile", source, name)
            self.assertNotIn("tempfile", source, name)

    def test_no_hash_function_is_applied_to_image_data(self):
        for name in ("pixel_server.py", "pixel_capture.py", "pixel_runner.py", "pixel_agent.py"):
            source = (HERE / name).read_text(encoding="utf-8")
            for line in source.splitlines():
                if "hashlib" in line and not line.lstrip().startswith(("#", "import")):
                    self.assertNotRegex(line, r"png|image|frame|pixel|captured", f"{name}: {line}")


if __name__ == "__main__":
    unittest.main()
