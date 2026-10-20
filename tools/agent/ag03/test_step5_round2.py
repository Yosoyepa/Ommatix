"""Regressions for the independent re-review of PR 104 at 23c43cc (R104-1, R104-2; docs/reviews/2026-10-20-ag03-step5.md).

Written and run RED against 23c43cc before the fixes (output kept beside the task prompt, ``step5fix_red/round2``). Fakes
only, as ``test_step5``: the native boundary is a set of Python objects, the child process a Python object.

R104-1 matching desktop names and READOBJECTS access do not show that a key-state read observed anything: a zero from
       ``GetAsyncKeyState`` also means "no cross-process hook/journal access" or "blocked by UIPI". Zero key states count
       as idle only with positive evidence of both (the input desktop opened with the access the read needs; a caller
       whose integrity level is at least medium); anything else is unknown and refuses.
R104-2 an ordinary exception out of ``Popen`` does not prove that no child exists: ``Popen`` returns the process only
       after ``CreateProcess`` AND its own pipe clean-up succeeded. Ownership is lowered only for a refusal that
       cannot follow a creation (the program or directory is not there).
"""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_win32 as w32  # noqa: E402
import run_goal as rg  # noqa: E402
from test_step5 import (JobWorld, NativeWorld, S3LockDispositionTests, S3OwnershipBeforeTheChildExistsTests,  # noqa: E402
                        error_code)

ACCESS_NEEDED = 0x0001 | 0x0008 | 0x0010  # DESKTOP_READOBJECTS | DESKTOP_HOOKCONTROL | DESKTOP_JOURNALRECORD
MEDIUM, HIGH, LOW = 0x2000, 0x3000, 0x1000


class R1041KeyStateAccessTests(unittest.TestCase):
    def run_type(self, **changes):
        world = NativeWorld()
        for name, value in changes.items():
            setattr(world, name, value)
        return (world, *world.run("type_text", {"text": "a"}))

    def test_idle_keys_with_the_full_evidence_deliver(self):
        for integrity in (MEDIUM, HIGH):
            with self.subTest(caller_integrity=hex(integrity)):
                world, server, answer, sender, records, reservations = self.run_type(caller_integrity=integrity)
                self.assertEqual(len(sender.batches), 1)

    def test_a_held_modifier_with_the_full_evidence_refuses(self):
        world, server, answer, sender, records, reservations = self.run_type(held={0x10})
        self.assertEqual(sender.batches, [])
        self.assertEqual(error_code(answer), "MODIFIER_HELD")

    def test_the_input_desktop_is_opened_with_the_access_a_cross_process_key_read_needs(self):
        world, *_ = self.run_type()
        self.assertTrue(world.open_desktop_access)
        for mask in world.open_desktop_access:
            self.assertEqual(mask & ACCESS_NEEDED, ACCESS_NEEDED, f"asked only for {mask:#x}")

    def test_a_readable_same_name_desktop_that_denies_hook_and_journal_access_refuses(self):
        world, server, answer, sender, records, reservations = self.run_type(denied_access_bits=~0x0001 & 0xFFFF)
        self.assertEqual(sender.batches, [], "all-zero key states were taken as idle without cross-process access")
        self.assertEqual(error_code(answer), "MODIFIERS_UNREADABLE")
        self.assertEqual(len(reservations), 1)
        self.assertEqual(server.terminal, "safety")

    def test_a_caller_below_medium_integrity_or_an_unreadable_one_refuses(self):
        for name, integrity in {"low": LOW, "unreadable": None, "zero": 0}.items():
            with self.subTest(caller_integrity=name):
                world, server, answer, sender, records, reservations = self.run_type(caller_integrity=integrity)
                self.assertEqual(sender.batches, [])
                self.assertEqual(error_code(answer), "MODIFIERS_UNREADABLE")
                self.assertEqual(len(reservations), 1)

    def test_the_integrity_decoder_needs_a_readable_level_of_at_least_medium(self):
        self.assertTrue(w32.decode_integrity_ok(MEDIUM))
        self.assertTrue(w32.decode_integrity_ok(HIGH))
        for bad in (LOW, 0, None, "8192", True, -1):
            self.assertFalse(w32.decode_integrity_ok(bad))


@unittest.skipUnless(os.name == "nt", "the job object is a Windows mechanism; the fakes stand in for it")
class R1042PopenErrorTests(unittest.TestCase):
    def test_a_program_that_is_not_there_owns_nothing(self):
        world, owned = JobWorld("popen_oserror"), {"unverified": 0}
        error = S3OwnershipBeforeTheChildExistsTests().start(world, owned)
        self.assertIsInstance(error, FileNotFoundError)
        self.assertEqual(owned["unverified"], 0)
        self.assertFalse(world.exists)

    def test_an_ordinary_error_after_the_child_was_created_never_lowers_the_counter_of_a_live_child(self):
        world, owned = JobWorld("popen_post_create_oserror"), {"unverified": 0}
        error = S3OwnershipBeforeTheChildExistsTests().start(world, owned)
        self.assertIsInstance(error, (OSError, rg.OwnedSessionNotQuiescent))
        self.assertTrue(world.alive, "the fake child exists: Popen never returned it")
        self.assertGreaterEqual(owned["unverified"], 1, "a live, unowned child was assumed absent")

    def test_the_production_main_keeps_the_lock_after_such_an_error_and_a_second_executor_is_refused(self):
        from test_campaign import Series
        series, calls = Series(self), []
        world = JobWorld("popen_post_create_oserror")
        error, stderr = S3LockDispositionTests().run_main(world, series, calls)
        self.assertTrue(world.alive)
        self.assertTrue((series.root / "campaign" / "campaign.lock").exists(), "the lock was released")
        self.assertIn("by hand", stderr)
        second = Series(self, root=series.root)
        with self.assertRaises(SystemExit):
            second.run(extra=["--out", str(series.root / "out2")])
        self.assertEqual((second.calls, second.app_calls), ([], {}))


@unittest.skipIf(os.name == "nt", "a real child with a process group: no job object, no Win32; Windows uses the fakes")
class R1042PosixTests(unittest.TestCase):
    def test_an_ordinary_error_after_a_real_child_was_created_never_lowers_the_counter(self):
        children, owned = [], {"unverified": 0}
        real = subprocess.Popen

        def popen_then_fail(*args, **kwargs):
            children.append(real(*args, **kwargs))
            raise OSError("synthetic failure after the child was created")
        try:
            with mock.patch.object(subprocess, "Popen", popen_then_fail):
                with self.assertRaises((OSError, rg.OwnedSessionNotQuiescent)):
                    rg.run_claude([sys.executable, "-c", "import time; time.sleep(60)"], "",
                                  Path(tempfile.gettempdir()), rg.StreamMetrics([], None, None), 5.0, env={},
                                  ownership=owned)
            self.assertEqual(len(children), 1)
            if children[0].poll() is None:
                self.assertGreaterEqual(owned["unverified"], 1, "the lock may be released with a live child")
        finally:
            for child in children:
                child.kill()
                child.wait()


if __name__ == "__main__":
    unittest.main()
