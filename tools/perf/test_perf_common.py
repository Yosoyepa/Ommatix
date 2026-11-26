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


class TimingStages(unittest.TestCase):
    """The closed stage names of a response's `_meta`, kept for the slow observations of the
    soak (box 7). The repository test `timing_stage_names_in_the_perf_tools_are_the_wire_names`
    binds the tuple to the real serializer."""

    FULL = {"capture_us": 12_000, "plan_us": 300, "detect_us": 80_000, "recognize_us": 41_250,
            "accessibility_us": 9_000_000, "fuse_us": 700, "track_us": 90, "commit_us": 40,
            "total_us": 9_500_000, "response_build_us": 1_500}

    def test_the_closed_names_are_the_wire_names_without_the_suffix_in_wire_order(self):
        self.assertEqual(c.TIMING_STAGES, (
            "capture", "plan", "detect", "recognize", "accessibility", "fuse", "track", "commit",
            "total", "response_build"))
        self.assertEqual(c.ACCESSIBILITY_KEY, "io.ommatix/accessibility")

    def test_the_stage_timings_of_a_response_are_milliseconds_with_three_decimals(self):
        got = c.timing_stages_ms({c.TIMINGS_KEY: self.FULL})
        self.assertEqual(list(got), list(c.TIMING_STAGES))
        self.assertEqual(got["recognize"], 41.25)
        self.assertEqual(got["plan"], 0.3)
        self.assertEqual(got["accessibility"], 9000.0)
        self.assertEqual(c.timing_stages_ms({c.TIMINGS_KEY: {"track_us": 1_234_567}}),
                         {"track": 1234.567})
        self.assertEqual(c.timing_stages_ms({c.TIMINGS_KEY: {"plan_us": 1}}), {"plan": 0.001})

    def test_a_key_outside_the_closed_names_is_dropped(self):
        meta = {c.TIMINGS_KEY: {"detect_us": 5_000, "foo_us": 7_000, "detect": 1, "total": 2,
                                "": 3, "plan_ms": 4}}
        self.assertEqual(c.timing_stages_ms(meta), {"detect": 5.0})

    def test_a_null_or_a_value_that_is_not_a_number_is_omitted_and_never_invented(self):
        meta = {c.TIMINGS_KEY: {"detect_us": 5_000, "response_build_us": None,
                                "fuse_us": "12", "track_us": [1], "commit_us": True,
                                "capture_us": {"x": 1}}}
        self.assertEqual(c.timing_stages_ms(meta), {"detect": 5.0})

    def test_a_response_without_timings_gives_nothing(self):
        for meta in ({}, {c.TIMINGS_KEY: None}, {c.TIMINGS_KEY: []}, {c.TIMINGS_KEY: "x"},
                     None, [], "x"):
            self.assertEqual(c.timing_stages_ms(meta), {}, meta)

    def test_the_completeness_of_the_accessibility_evidence_is_true_false_or_unknown(self):
        key = c.ACCESSIBILITY_KEY
        self.assertIs(c.accessibility_complete({key: {"complete": True}}), True)
        self.assertIs(c.accessibility_complete({key: {"complete": False}}), False)
        for meta in ({}, {key: None}, {key: {}}, {key: {"complete": None}},
                     {key: {"complete": "yes"}}, {key: {"complete": 1}}, {key: []}, None, "x"):
            self.assertIsNone(c.accessibility_complete(meta), meta)

    def test_stage_ms_is_unchanged(self):
        meta = {c.TIMINGS_KEY: {"foo_us": 1_000, "total_us": 2_000}}
        self.assertEqual(c.stage_ms(meta), {"foo": 1.0, "total": 2.0})


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


# ---- which thread appeared (box 7, C2): thread ids and their start modules ------------------

class FakeThreadIds(FakeThreads):
    """A thread snapshot whose entries are ``(owner, thread_id)`` pairs, as ``Win32ThreadIds``
    returns them; the walk and its failures are those of ``FakeThreads``."""


class ThreadIds(unittest.TestCase):
    def test_only_the_ids_of_the_process_come_back_sorted(self):
        api = FakeThreadIds([(100, 30), (200, 5), (100, 10), (300, 1), (100, 20)])
        self.assertEqual(c.thread_ids(100, api), [10, 20, 30])
        self.assertEqual(api.closed, ["snapshot"])

    def test_a_process_with_no_thread_in_the_snapshot_has_an_empty_list_not_none(self):
        for owners in ([(200, 1), (300, 2)], []):
            api = FakeThreadIds(owners)
            self.assertEqual(c.thread_ids(100, api), [])
            self.assertEqual(api.closed, ["snapshot"])

    def test_no_snapshot_is_none_and_there_is_no_handle_to_close(self):
        api = FakeThreadIds([(100, 1)], snapshot=False)
        self.assertIsNone(c.thread_ids(100, api))
        self.assertEqual(api.closed, [])

    def test_a_failing_first_or_later_call_is_none_and_the_snapshot_is_still_closed(self):
        for fail_at in (0, 2):
            api = FakeThreadIds([(100, 1), (100, 2), (100, 3)], fail_at=fail_at)
            self.assertIsNone(c.thread_ids(100, api), fail_at)
            self.assertEqual(api.closed, ["snapshot"])

    def test_the_ids_agree_with_the_count_of_the_same_snapshot(self):
        owners = [100, 200, 100, 300, 100]
        ids = c.thread_ids(100, FakeThreadIds([(o, 1000 + i) for i, o in enumerate(owners)]))
        self.assertEqual(len(ids), c.count_threads(100, FakeThreads(owners)))


class ModuleOf(unittest.TestCase):
    MODULES = [(0x7FF600000000, 0x20000, "ommatix.exe"),
               (0x7FFA10000000, 0x1000, "tiny.dll"),
               (0x7FFA20000000, 0x300000, "UIAutomationCore.dll")]

    def test_the_range_is_closed_at_the_base_and_open_at_the_end(self):
        base, size, name = self.MODULES[2]
        self.assertEqual(c.module_of(base, self.MODULES), name)
        self.assertEqual(c.module_of(base + 1, self.MODULES), name)
        self.assertEqual(c.module_of(base + size - 1, self.MODULES), name)
        self.assertEqual(c.module_of(base + size, self.MODULES), "unknown")
        self.assertEqual(c.module_of(base - 1, self.MODULES), "unknown")

    def test_the_list_is_searched_whatever_its_order(self):
        self.assertEqual(c.module_of(0x7FFA10000000, self.MODULES), "tiny.dll")
        self.assertEqual(c.module_of(0x7FF600000010, list(reversed(self.MODULES))),
                         "ommatix.exe")

    def test_no_address_and_a_zero_address_are_unknown(self):
        self.assertEqual(c.module_of(None, self.MODULES), "unknown")
        self.assertEqual(c.module_of(0, self.MODULES), "unknown")
        self.assertEqual(c.module_of(0, [(0, 0x1000, "zero.dll")]), "unknown")

    def test_no_modules_is_unknown_and_a_name_is_only_ever_a_base_name(self):
        self.assertEqual(c.module_of(0x1234, []), "unknown")
        self.assertEqual(c.module_of(0x1234, [(0x1000, 0x1000, "C:\\Windows\\ntdll.dll")]),
                         "ntdll.dll")


class ThreadChanges(unittest.TestCase):
    def test_what_appeared_and_what_vanished_are_sorted(self):
        self.assertEqual(c.thread_changes([1, 2, 3, 9], [2, 3, 7, 5]),
                         {"added": [5, 7], "removed": [1, 9]})

    def test_the_same_threads_change_nothing(self):
        self.assertEqual(c.thread_changes([1, 2], [2, 1]), {"added": [], "removed": []})
        self.assertEqual(c.thread_changes([], []), {"added": [], "removed": []})

    def test_from_and_to_nothing(self):
        self.assertEqual(c.thread_changes([], [4, 3]), {"added": [3, 4], "removed": []})
        self.assertEqual(c.thread_changes([4, 3], []), {"added": [], "removed": [3, 4]})

    def test_the_inputs_are_not_modified(self):
        before, after = [3, 1], [2, 1]
        c.thread_changes(before, after)
        self.assertEqual((before, after), ([3, 1], [2, 1]))


@unittest.skipUnless(sys.platform == "win32", "thread start addresses are a Windows call")
class ThreadStartModulesOnWindows(unittest.TestCase):
    def test_every_thread_of_this_process_resolves_to_a_module_name(self):
        release = threading.Event()
        worker = threading.Thread(target=release.wait)
        worker.start()
        try:
            ids = c.thread_ids(os.getpid(), c.Win32ThreadIds())
            modules = c.process_modules(os.getpid())
            starts = {tid: c.thread_start_address(tid) for tid in ids or []}
        finally:
            release.set()
            worker.join()
        self.assertIsNotNone(ids)
        self.assertGreaterEqual(len(ids), 2)
        self.assertEqual(ids, sorted(ids))
        self.assertTrue(modules)
        names = {tid: c.module_of(address, modules) for tid, address in starts.items()}
        print(f"\nstart modules of this process: {sorted(names.values())}", file=sys.stderr)
        for tid, name in names.items():
            self.assertTrue(name.lower().endswith((".dll", ".exe")), (tid, starts[tid], name))
        for base, size, name in modules:
            self.assertGreater(base, 0)
            self.assertGreater(size, 0)
            self.assertEqual(name, os.path.basename(name))

    def test_the_main_module_of_this_process_is_among_the_modules(self):
        names = {name.lower() for _, _, name in c.process_modules(os.getpid())}
        self.assertIn(Path(sys.executable).name.lower(), names)

    def test_a_thread_that_does_not_exist_has_no_start_address(self):
        self.assertIsNone(c.thread_start_address(0xFFFFFFF0))

    def test_a_process_that_does_not_exist_has_no_modules(self):
        self.assertIsNone(c.process_modules(0xFFFFFFF0))

    def test_the_thread_handle_is_closed_when_the_query_fails(self):
        kernel32, ntdll = mock.Mock(), mock.Mock()
        kernel32.OpenThread.return_value = 4242
        ntdll.NtQueryInformationThread.return_value = -1073741790  # STATUS_ACCESS_DENIED
        with mock.patch.object(c, "kernel32", kernel32), mock.patch.object(c, "ntdll", ntdll):
            self.assertIsNone(c.thread_start_address(1))
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_the_limited_access_is_tried_when_the_full_one_is_refused(self):
        kernel32, ntdll = mock.Mock(), mock.Mock()
        kernel32.OpenThread.side_effect = [None, 4242]
        ntdll.NtQueryInformationThread.return_value = 0
        with mock.patch.object(c, "kernel32", kernel32), mock.patch.object(c, "ntdll", ntdll):
            self.assertIsNone(c.thread_start_address(1))  # the buffer stays 0: no address
        self.assertEqual([call.args[0] for call in kernel32.OpenThread.call_args_list],
                         [0x0040, 0x0800])
        kernel32.CloseHandle.assert_called_once_with(4242)

    def test_the_snapshot_of_ids_is_closed_when_the_walk_fails(self):
        kernel32 = mock.Mock()
        kernel32.CreateToolhelp32Snapshot.return_value = 4242
        kernel32.Thread32First.return_value = 0
        with mock.patch.object(c, "kernel32", kernel32):
            with mock.patch("ctypes.get_last_error", return_value=5):
                self.assertIsNone(c.thread_ids(os.getpid(), c.Win32ThreadIds()))
        kernel32.CloseHandle.assert_called_once_with(4242)


if __name__ == "__main__":
    unittest.main()
