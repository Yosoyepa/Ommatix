"""Tests of the pure parts of perf_common.py. Run: python -m unittest discover -s tools/perf"""
import ctypes
import json
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_common as c  # noqa: E402


class Statistics(unittest.TestCase):
    def test_percentiles_use_the_nearest_rank_like_the_s0_report(self):
        self.assertEqual(c.percentile(list(range(1, 101)), 0.95), 95)
        self.assertEqual(c.percentile([7], 0.99), 7)

    def test_summary_of_nothing_is_just_n_zero(self):
        self.assertEqual(c.summary([]), {"n": 0})
        s = c.summary([1.0, 2.0, 3.0, 100.0])
        self.assertEqual((s["n"], s["median"], s["max"]), (4, 2.5, 100.0))

    def test_stage_timings_are_converted_from_microseconds(self):
        meta = {c.TIMINGS_KEY: {"detect_us": 80_000, "total_us": 90_500}}
        self.assertEqual(c.stage_ms(meta), {"detect": 80.0, "total": 90.5})
        self.assertEqual(c.stage_ms({}), {})


class InputFreeConfiguration(unittest.TestCase):
    def test_the_shipped_operator_configurations_are_input_free(self):
        paths = sorted((c.REPO / "tools" / "s0").glob("*.toml"))
        self.assertNotEqual(paths, [])
        for path in paths:
            self.assertIsNone(c.config_problem(path.read_text(encoding="utf-8")), path.name)

    def test_a_configuration_that_could_deliver_input_is_refused(self):
        live = '[safety]\nactuation_enabled = true\nallowed_applications = ["notepad.exe"]\n'
        self.assertIn("actuation_enabled", c.config_problem(live))
        allowed = '[safety]\nallowed_applications = ["notepad.exe"]\n'
        self.assertIn("allowed_applications", c.config_problem(allowed))
        self.assertIn("mode", c.config_problem('[engine]\nmode = "native"\n'))
        self.assertIn("TOML", c.config_problem("not [toml"))
        self.assertIsNone(c.config_problem(""))


class Files(unittest.TestCase):
    def test_a_report_is_written_whole_and_replaces_the_previous_one(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d, "r.json")
            c.write_json(path, {"a": 1})
            c.write_json(path, {"a": 2})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")), {"a": 2})
            self.assertEqual([p.name for p in Path(d).iterdir()], ["r.json"])


class ReadOnlyEngine(unittest.TestCase):
    def test_the_engine_wrapper_can_only_call_the_two_read_only_tools(self):
        engine = c.Engine.__new__(c.Engine)  # no process: the check comes first
        for tool in ("execute_action", "anything_else"):
            with self.assertRaises(ValueError):
                engine.call(tool, {})
        self.assertEqual(c.Engine.ALLOWED, ("observe_screen", "get_engine_metrics"))


class FakeThreads:
    """A thread snapshot: the owning process of each entry, in order. ``fail_at`` is the call
    that raises (0 is Thread32First, n the n-th Thread32Next); ``closed`` records the handles
    that were closed."""

    def __init__(self, owners, snapshot=True, fail_at=None):
        self.owners = list(owners)
        self.handle = "snapshot" if snapshot else None
        self.fail_at = fail_at
        self.cursor = 0
        self.closed = []

    def snapshot(self):
        return self.handle

    def first(self, handle):
        self.cursor = 0
        return self.entry()

    def next(self, handle):
        self.cursor += 1
        return self.entry()

    def entry(self):
        if self.cursor == self.fail_at:
            raise OSError(5, "access denied")
        return self.owners[self.cursor] if self.cursor < len(self.owners) else None

    def close(self, handle):
        self.closed.append(handle)


class ThreadCount(unittest.TestCase):
    def test_only_the_threads_of_the_process_are_counted(self):
        api = FakeThreads([100, 200, 100, 300, 100])
        self.assertEqual(c.count_threads(100, api), 3)
        self.assertEqual(api.closed, ["snapshot"])

    def test_a_process_with_no_thread_in_the_snapshot_counts_zero(self):
        api = FakeThreads([200, 300])
        self.assertEqual(c.count_threads(100, api), 0)
        self.assertEqual(api.closed, ["snapshot"])

    def test_an_empty_snapshot_counts_zero(self):
        api = FakeThreads([])
        self.assertEqual(c.count_threads(100, api), 0)
        self.assertEqual(api.closed, ["snapshot"])

    def test_no_snapshot_is_none_and_there_is_no_handle_to_close(self):
        api = FakeThreads([100], snapshot=False)
        self.assertIsNone(c.count_threads(100, api))
        self.assertEqual(api.closed, [])

    def test_a_failing_first_call_is_none_and_the_snapshot_is_still_closed(self):
        api = FakeThreads([100, 100], fail_at=0)
        self.assertIsNone(c.count_threads(100, api))
        self.assertEqual(api.closed, ["snapshot"])

    def test_a_failing_later_call_is_none_and_the_snapshot_is_still_closed(self):
        api = FakeThreads([100, 100, 100], fail_at=2)
        self.assertIsNone(c.count_threads(100, api))
        self.assertEqual(api.closed, ["snapshot"])


@unittest.skipUnless(sys.platform == "win32", "the thread snapshot is a Windows call")
class ThreadCountOnWindows(unittest.TestCase):
    def test_a_thread_started_here_is_counted_on_this_process(self):
        before = c.thread_count(os.getpid())
        release = threading.Event()
        worker = threading.Thread(target=release.wait)
        worker.start()
        try:
            after = c.thread_count(os.getpid())
        finally:
            release.set()
            worker.join()
        self.assertGreaterEqual(after, before + 1)

    def test_the_process_counters_carry_the_thread_count(self):
        counters = c.process_counters(os.getpid())
        self.assertEqual(set(counters), {"rss", "private", "handles", "threads"})
        self.assertGreaterEqual(counters["threads"], 1)

    def test_the_snapshot_is_closed_when_the_walk_fails(self):
        kernel32 = mock.Mock()
        kernel32.CreateToolhelp32Snapshot.return_value = 4242
        kernel32.Thread32First.return_value = 0
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=5):
                self.assertIsNone(c.thread_count(os.getpid()))
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_the_snapshot_is_closed_after_an_empty_walk(self):
        kernel32 = mock.Mock()
        kernel32.CreateToolhelp32Snapshot.return_value = 4242
        kernel32.Thread32First.return_value = 0
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=18):
                self.assertEqual(c.thread_count(os.getpid()), 0)
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_an_invalid_snapshot_is_none_and_nothing_is_closed(self):
        kernel32 = mock.Mock()
        kernel32.CreateToolhelp32Snapshot.return_value = ctypes.c_void_p(-1).value
        with mock.patch.object(c, "kernel32", kernel32):
            self.assertIsNone(c.thread_count(os.getpid()))
        kernel32.CloseHandle.assert_not_called()

    def mocked_walk(self, records):
        """A kernel32 whose thread walk yields ``records`` in order, each ``(ok, size, owner)``.
        Like the kernel, it writes the first ``size`` bytes of the entry: the owner field
        (bytes 12 to 16) only when ``size`` reaches it, so an unwritten field keeps its value."""
        kernel32 = mock.Mock()
        kernel32.CreateToolhelp32Snapshot.return_value = 4242
        pending = list(records)
        owner_end = c.ThreadEntry32.th32OwnerProcessID.offset + ctypes.sizeof(ctypes.c_uint32)

        def step(handle, entry_pointer):
            ok, size, owner = pending.pop(0) if pending else (False, 0, 0)
            if not ok:
                return 0
            entry = ctypes.cast(entry_pointer, ctypes.POINTER(c.ThreadEntry32)).contents
            entry.dwSize = size
            if size >= owner_end:
                entry.th32OwnerProcessID = owner
            return 1

        kernel32.Thread32First.side_effect = step
        kernel32.Thread32Next.side_effect = step
        return kernel32

    def test_a_short_first_entry_is_none_and_the_snapshot_is_closed_once(self):
        kernel32 = self.mocked_walk([(True, 12, 0)])
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=18):
                self.assertIsNone(c.thread_count(100))
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_a_short_later_entry_is_none_not_the_count_of_the_valid_prefix(self):
        kernel32 = self.mocked_walk([(True, 28, 100), (True, 12, 0)])
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=18):
                self.assertIsNone(c.thread_count(100))
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_an_entry_whose_size_reaches_the_owner_field_is_counted(self):
        kernel32 = self.mocked_walk([(True, 16, 100), (True, 28, 200)])
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=18):
                self.assertEqual(c.thread_count(100), 1)
        kernel32.CloseHandle.assert_called_once_with(4242)


if __name__ == "__main__":
    unittest.main()
