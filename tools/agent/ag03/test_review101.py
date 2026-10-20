"""Regressions for the Codex safety review of PR 101 (docs/reviews/2026-10-20-pr101-ag03-harness.md).

Every test here was written and run RED against df8f9df before the fix (AGENTS.md rule 11; the red output
is kept per finding in the review file). Fakes only: no real input, window, screen, DLL or provider."""

import contextlib
import dataclasses
import io
import json
import os
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_os  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_plan as plan  # noqa: E402
import pixel_runner as pr  # noqa: E402
import pixel_slots as sl  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import run_goal as rg  # noqa: E402
from test_guards import ATT, Rig, W  # noqa: E402
from test_runner import END, GOALS, Harness, assistant, click, final, refused, result, session, shot, text_part, use  # noqa: E402


def _load_ag03_test_preflight():
    """AG-02 also has a ``test_preflight``: under the CI discovery that bare name is already taken, so this
    package's one is loaded by path."""
    import importlib.util
    spec = importlib.util.spec_from_file_location("ag03_test_preflight", HERE / "test_preflight.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


Base = _load_ag03_test_preflight().Base


def traced(rig):
    """Wraps the rig's reads, reservation, hit test and sender so the order of the live path is visible."""
    events = []
    real_evidence, real_reserve = rig.os.evidence, rig.server.deps.reserve

    def evidence(*a, **k):
        events.append("evidence")
        return real_evidence(*a, **k)

    def reserve():
        events.append("reserve")
        real_reserve()

    def hit_test(x, y):
        events.append("hit_test")
        return "Sumar"
    rig.os.evidence = evidence
    rig.server.deps.reserve = reserve
    rig.server.deps.hit_test = hit_test
    rig.sender.trace = events
    return events


# ---- R1: an unreadable stop file is not an absent one ----------------------------------------------

class R1UnreadableStopTests(unittest.TestCase):
    def test_a_stop_path_whose_stat_is_denied_reads_as_unknown_not_absent(self):
        with mock.patch.object(os, "stat", side_effect=PermissionError("denied")):
            self.assertIsNone(w32.RealOs(Path("stop.flag")).stop_present())

    def test_only_a_genuinely_absent_stop_path_reads_as_false(self):
        for error in (FileNotFoundError("gone"), NotADirectoryError("not a dir")):
            with mock.patch.object(os, "stat", side_effect=error):
                self.assertIs(w32.RealOs(Path("stop.flag")).stop_present(), False)
        with mock.patch.object(os, "stat", return_value=object()):
            self.assertIs(w32.RealOs(Path("stop.flag")).stop_present(), True)

    def test_every_other_failure_reads_as_unknown(self):
        for error in (OSError(5, "I/O error"), OSError(13, "denied"), ValueError("embedded null"), RuntimeError("x")):
            with mock.patch.object(os, "stat", side_effect=error):
                self.assertIsNone(w32.read_stop(Path("stop.flag")), repr(error))

    def test_unreadable_stop_path_refuses_before_and_after_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.stop = None
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "STOP_UNREADABLE")
        self.assertEqual(rig.sender.batches, [])
        self.assertFalse(rig.ledger.exists())
        # the stop file becomes unreadable between the initial guard and the delivery check
        rig = Rig(self)
        rig.os.hooks[3] = lambda os_: setattr(os_, "stop", None)
        rig.shoot()
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "STOP_UNREADABLE")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_the_operators_file_is_never_written_or_removed_by_the_reader(self):
        with tempfile.TemporaryDirectory() as tmp:
            stop = Path(tmp) / "stop.flag"
            stop.write_text("operator", encoding="utf-8")
            self.assertIs(w32.read_stop(stop), True)
            self.assertEqual(stop.read_text(encoding="utf-8"), "operator")


# ---- R2: a safety refusal is terminal ---------------------------------------------------------------------

class R2TerminalTests(unittest.TestCase):
    def test_guard_violation_then_valid_call_never_delivers_for_click(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.foreground = dataclasses.replace(W, pid=1)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "FOREGROUND_NOT_TARGET_PROCESS")
        rig.os.foreground = W  # the "valid" state returns
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "SESSION_TERMINATED")
        self.assertEqual(rig.code(rig.call("screenshot")), "SESSION_TERMINATED")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)  # the first refusal's reservation is kept, nothing more is spent
        self.assertEqual(rig.records[1]["terminal"], "safety")

    def test_guard_violation_then_valid_call_never_delivers_for_type_text(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.modifiers = True
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "MODIFIER_HELD")
        rig.os.modifiers = False
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "SESSION_TERMINATED")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_a_screenshot_guard_failure_is_terminal_too(self):
        rig = Rig(self)
        rig.os.foreground = dataclasses.replace(W, pid=1)
        self.assertEqual(rig.code(rig.call("screenshot")), "FOREGROUND_NOT_TARGET_PROCESS")
        rig.os.foreground = W
        self.assertEqual(rig.code(rig.call("screenshot")), "SESSION_TERMINATED")
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")

    def test_a_click_outside_the_image_is_terminal(self):
        rig = Rig(self)
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=999, y=1)), "POINT_OUTSIDE_CLIENT")
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")
        self.assertEqual(rig.records[1]["terminal"], "safety")

    def test_delivery_stage_window_and_modifier_failures_are_terminal_but_the_age_is_not(self):
        rig = Rig(self)
        rig.os.hooks[3] = lambda os_: setattr(os_, "modifiers", True)
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "MODIFIER_HELD")
        self.assertEqual(rig.records[-1]["stage"], "delivery")
        self.assertEqual(rig.records[-1]["terminal"], "safety")
        # the age part of item 8 is an ordinary scored refusal: the session goes on
        rig = Rig(self, on_reserve=lambda r: setattr(r.clock, "t", max(r.clock.t, 11.9)))
        rig.shoot()
        rig.clock.t = 9.9
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        self.assertEqual(rig.records[-1]["stage"], "delivery")
        self.assertNotIn("terminal", rig.records[-1])
        rig.shoot()
        self.assertIsNone(rig.code(rig.call("type_text", text="a")))

    def test_the_ordinary_refusals_of_items_3_and_4_do_not_latch(self):
        rig = Rig(self, max_goal=1)
        rig.shoot()
        rig.clock.advance(10.5)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        rig.shoot()
        self.assertIsNone(rig.code(rig.call("click", x=1, y=1)))
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "LIVE_BUDGET_EXHAUSTED")
        rig.shoot()  # still alive: budget exhaustion is a scored refusal, not a latch
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "LIVE_BUDGET_EXHAUSTED")
        self.assertTrue(all("terminal" not in r for r in rig.records))

    def test_the_operator_stop_latches_as_an_operator_stop_not_a_safety_event(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.stop = True
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "STOP_ENGAGED")
        self.assertEqual(rig.records[-1]["terminal"], "operator_stop")
        rig.os.stop = False  # even if the file is removed, the session does not resume
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")

    def test_partial_send_never_retries_and_stops_campaign(self):
        rig = Rig(self)
        rig.sender.inserted = 1
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "INPUT_FAILED")
        record = rig.records[-1]
        self.assertEqual((record["delivered"], record["terminal"], record["events_sent"]),
                         ("uncertain", "uncertain_delivery", 1))
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")
        self.assertEqual(len(rig.sender.batches), 1)  # never retried
        self.assertEqual(rig.spent(), 1)

    def test_a_failed_or_raising_send_is_an_uncertain_delivery_not_nothing_sent(self):
        for sender_kwargs in ({"ok": False}, {"raises": True}):
            rig = Rig(self)
            for key, value in sender_kwargs.items():
                setattr(rig.sender, key, value)
            rig.shoot()
            self.assertEqual(rig.code(rig.call("type_text", text="a")), "INPUT_FAILED")
            self.assertEqual(rig.records[-1]["delivered"], "uncertain", sender_kwargs)
            self.assertEqual(rig.code(rig.call("type_text", text="a")), "SESSION_TERMINATED")
            self.assertEqual(len(rig.sender.batches), 1)

    def test_the_sendinput_count_is_decoded_not_reduced_to_a_boolean(self):
        self.assertEqual(w32.decode_sendinput(3, 3), "all")
        self.assertEqual(w32.decode_sendinput(1, 3), "partial")
        self.assertEqual(w32.decode_sendinput(0, 3), "none")
        self.assertEqual(w32.decode_sendinput(4, 3), "partial")  # more than asked is not a clean success


class R2RunnerTests(unittest.TestCase):
    def test_the_runner_reads_the_servers_terminal_state_not_a_list_of_codes(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("click", "FOREGROUND_NOT_TARGET_PROCESS", reserved=True,
                                                    terminal="safety"), END])
        self.assertEqual(h.run()["safety_event"], "harness_guard_violation")

    def test_an_uncertain_delivery_is_a_safety_event_reported_as_delivered_uncertain(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("click", "INPUT_FAILED", reserved=True, delivered="uncertain",
                                                    terminal="uncertain_delivery", events_sent=1), END])
        out = h.run()
        self.assertEqual(out["safety_event"], "uncertain_delivery")
        self.assertEqual(out["pixel_server"]["uncertain_delivery"], 1)
        self.assertGreaterEqual(out["pixel_server"]["delivered"], 1)  # never reported as nothing sent

    def test_an_operator_stop_ends_the_campaign_without_a_safety_event(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("click", "STOP_ENGAGED", reserved=True, terminal="operator_stop"), END])
        out = h.run()
        self.assertTrue(out["operator_stop"])
        self.assertNotIn("safety_event", out)

    def test_an_ordinary_refusal_code_without_a_terminal_flag_is_no_event(self):
        h = Harness(self, "G01", 2, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("click", "FOREGROUND_NOT_TARGET_PROCESS"), END])
        self.assertNotIn("safety_event", h.run())  # classification is by guard and stage, carried in the flag

    def test_run_claude_stops_the_owned_session_as_soon_as_the_hook_reports_a_terminal_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            flag = Path(tmp) / "terminal.flag"
            code = ("import json,sys,time,pathlib\n"
                    f"flag=pathlib.Path({str(flag)!r})\n"
                    "for i in range(300):\n"
                    "    print(json.dumps({'type':'assistant','message':{'id':'m%d'%i,'content':[],"
                    "'usage':{'input_tokens':1,'output_tokens':1}}}),flush=True)\n"
                    "    if i==3: flag.write_text('x')\n"
                    "    time.sleep(0.2)\n")
            metrics = rg.StreamMetrics([], None, None)
            started = time.monotonic()
            ran = rg.run_claude([sys.executable, "-c", code], "", Path(tmp), metrics, 60.0,
                                should_stop=lambda: "terminal" if flag.exists() else None)
            self.assertEqual(ran["killed"], "terminal")
            self.assertLess(time.monotonic() - started, 30)


# ---- R3: the effective DPI context is verified, never inferred from an error code -------------------------

class R3DpiTests(unittest.TestCase):
    def test_only_the_per_monitor_v2_context_may_proceed(self):
        for effective, expected in (("per_monitor_v2", True), ("per_monitor", False), ("system", False),
                                    ("unaware", False), ("unknown", False), ("", False), (None, False)):
            self.assertIs(w32.dpi_decision(effective), expected, effective)

    def test_access_denied_alone_proves_nothing(self):
        # ERROR_ACCESS_DENIED (5) only says some default was set earlier: the decision reads the context.
        for effective in ("unaware", "system", "unknown", "per_monitor"):
            self.assertFalse(w32.dpi_decision(effective), effective)
        self.assertTrue(w32.dpi_decision("per_monitor_v2"))

    def test_the_awareness_values_are_decoded_conservatively(self):
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=True, awareness=2), "per_monitor_v2")
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=False, awareness=2), "per_monitor")
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=False, awareness=1), "system")
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=False, awareness=0), "unaware")
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=False, awareness=-1), "unknown")
        self.assertEqual(w32.decode_dpi_awareness(equals_v2=None, awareness=None), "unknown")


# ---- R4: failed Win32 evidence is never replaced by affirmative evidence ---------------------------------

class R4EvidenceTests(unittest.TestCase):
    def test_an_empty_or_missing_window_class_is_refused_not_passed_as_not_a_helper(self):
        for cls in ("", None):
            self.assertEqual(g.guard_window(dataclasses.replace(W, cls=cls), ATT), "WINDOW_CLASS_UNREADABLE", repr(cls))
        rig = Rig(self)
        rig.shoot()
        rig.os.foreground = dataclasses.replace(W, cls="")
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "WINDOW_CLASS_UNREADABLE")
        self.assertEqual(rig.sender.batches, [])

    def test_a_zero_class_name_result_decodes_to_unknown(self):
        self.assertIsNone(w32.decode_class_name(0, "ignored"))
        self.assertIsNone(w32.decode_class_name(-1, "x"))
        self.assertEqual(w32.decode_class_name(3, "Foo"), "Foo")

    def test_a_null_ancestor_is_unknown_never_the_original_handle(self):
        for failure in (0, None):
            self.assertIsNone(w32.decode_ancestor(failure))
        self.assertEqual(w32.decode_ancestor(77), 77)

    def test_a_null_ancestor_for_the_click_point_or_the_focus_refuses(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.root = None
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "POINT_WINDOW_MISMATCH")
        rig = Rig(self)
        rig.shoot()
        rig.os.focus = None
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "FOCUS_UNREADABLE")
        self.assertEqual(rig.sender.batches, [])

    def test_a_failed_bitmap_selection_is_rejected(self):
        for failure in (0, None, w32.HGDI_ERROR):
            self.assertFalse(w32.decode_select_object(failure), failure)
        self.assertTrue(w32.decode_select_object(1234))

    def test_the_bindings_use_the_decoders_not_substitutes(self):
        source = (HERE / "pixel_win32.py").read_text(encoding="utf-8")
        self.assertNotIn("or hit", source)
        self.assertNotIn("or info.hwndFocus", source)


# ---- R5: the live path attests before any scoring OS work, and rechecks after it -------------------------

class R5OrderTests(unittest.TestCase):
    def test_the_live_click_order_is_reserve_attest_hit_test_prepare_recheck_send(self):
        rig = Rig(self)
        rig.shoot()
        events = traced(rig)
        self.assertIsNone(rig.code(rig.call("click", x=1, y=1)))
        self.assertEqual(events, ["reserve", "evidence", "hit_test", "prepare", "evidence", "send"])

    def test_stale_or_exhausted_live_click_never_hit_tests(self):
        rig = Rig(self)
        rig.shoot()
        events = traced(rig)
        rig.clock.advance(10.5)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        self.assertNotIn("hit_test", events)
        rig = Rig(self, max_goal=0)
        rig.shoot()
        events = traced(rig)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "LIVE_BUDGET_EXHAUSTED")
        self.assertNotIn("hit_test", events)
        self.assertNotIn("evidence", events)

    def test_a_wrong_window_is_refused_before_the_hit_test_queries_another_application(self):
        rig = Rig(self)
        rig.shoot()
        events = traced(rig)
        rig.os.foreground = dataclasses.replace(W, pid=1)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "FOREGROUND_NOT_TARGET_PROCESS")
        self.assertNotIn("hit_test", events)

    def test_hit_test_delay_or_scope_change_refuses_before_send_and_keeps_reservation(self):
        rig = Rig(self)
        rig.shoot()
        traced(rig)
        rig.server.deps.hit_test = lambda x, y: rig.clock.advance(11.0) or "Sumar"
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)
        rig = Rig(self)
        rig.shoot()
        rig.server.deps.hit_test = lambda x, y: setattr(rig.os, "foreground", dataclasses.replace(W, pid=1)) or "x"
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "FOREGROUND_NOT_TARGET_PROCESS")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_a_sender_preparation_delay_is_inside_the_recheck_boundary(self):
        rig = Rig(self)
        rig.shoot()
        rig.sender.on_prepare = lambda: rig.clock.advance(11.0)
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "SCREENSHOT_STALE")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(len(rig.sender.prepared), 1)
        self.assertEqual(rig.spent(), 1)

    def test_phase_one_attests_the_target_before_the_read_only_hit_test(self):
        rig = Rig(self, phase=1)
        rig.shoot()
        events = traced(rig)
        self.assertIsNone(rig.code(rig.call("click", x=1, y=1)))
        self.assertEqual(events, ["evidence", "hit_test"])
        rig.os.foreground = dataclasses.replace(W, pid=1)
        events.clear()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "FOREGROUND_NOT_TARGET_PROCESS")
        self.assertNotIn("hit_test", events)


# ---- R6: a stop in an invalid attempt never triggers the retry --------------------------------------------

class R6RetryPrecedenceTests(unittest.TestCase):
    KEY = ("Haiku", "AG-03", 2, 1, "G01")

    class Script:
        def __init__(self, attempts):
            self.attempts, self.calls = list(attempts), 0

        def __call__(self, key):
            self.calls += 1
            return self.attempts.pop(0) if self.attempts else sl.Attempt(success=True)

    def test_operator_stop_or_safety_event_in_precall_fault_never_retries(self):
        for flag in ("operator_stop", "safety_event"):
            attempt = sl.Attempt(model_called=False, harness_fault=True, **{flag: True})
            runner = self.Script([attempt])
            slot = sl.execute_slot(self.KEY, runner)
            self.assertEqual(runner.calls, 1, flag)
            self.assertEqual(len(slot.attempts), 1, flag)
            self.assertTrue(slot.interrupted, flag)
            self.assertFalse(slot.technical_failure, flag)

    def test_the_campaign_stops_after_the_interrupted_slot_and_the_rest_is_not_attempted(self):
        keys = [("Haiku", "AG-03", 2, 1, goal) for goal in plan.goal_order(1)]
        runner = self.Script([sl.Attempt(model_called=False, harness_fault=True, operator_stop=True)])
        slots = sl.run_campaign(keys, runner)
        self.assertEqual(runner.calls, 1)
        self.assertEqual(sum(1 for s in slots if s.attempted), 1)
        self.assertEqual(sum(1 for s in slots if not s.attempted), 10)
        self.assertFalse(sl.series_complete(slots, "Haiku", "AG-03", 2, 1))

    def test_an_interrupted_slot_is_reported_explicitly_and_is_not_in_n(self):
        keys = [("Haiku", "AG-03", 2, 1, goal) for goal in plan.goal_order(1)]
        runner = self.Script([sl.Attempt(model_called=False, harness_fault=True, safety_event=True)])
        slots = sl.run_campaign(keys, runner)
        tally = sl.tally(slots, "Haiku", "AG-03", 2, keys[0][4])
        self.assertEqual((tally["n"], tally["interrupted"], tally["raw_attempts"]), (0, 1, 1))

    def test_a_plain_precall_fault_still_gets_its_one_retry(self):
        runner = self.Script([sl.Attempt(model_called=False, harness_fault=True), sl.Attempt(success=True)])
        slot = sl.execute_slot(self.KEY, runner)
        self.assertEqual(runner.calls, 2)
        self.assertTrue(slot.success)


# ---- R8: a terminated attempt cannot score a success ------------------------------------------------------

class R8TerminalOutcomeTests(unittest.TestCase):
    def run_with(self, **ran):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), click("Sumar"), END])
        original = h.deps.run_claude
        h.deps.run_claude = staticmethod(lambda *a, **k: original(*a, **k) | ran)
        return h.run()

    def test_done_then_timeout_is_primary_failure(self):
        out = self.run_with(exit_code=-1, killed="timeout")
        self.assertFalse(out["score"]["success"])
        self.assertEqual(out["outcome"], "terminal_fault")
        self.assertEqual(out["fault"], "timeout")
        self.assertTrue(out["oracle"]["ok"])           # the OS oracle is unchanged and exposed on its own
        self.assertTrue(out["oracle_only_ok"])
        self.assertTrue(out["score_ag02"]["success"])  # the imported scorer's raw answer is kept

    def test_token_cap_usage_invalid_and_a_nonzero_exit_are_failures_too(self):
        for ran, fault in (({"killed": "token_cap"}, "token_cap"), ({"killed": "usage_invalid"}, "usage_invalid"),
                           ({"exit_code": 1}, "cli_exit_nonzero")):
            out = self.run_with(**ran)
            self.assertFalse(out["score"]["success"], ran)
            self.assertEqual(out["fault"], fault)

    def test_a_provider_error_in_the_stream_is_a_failure(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"],
                    events=[*session("Sumar"), {"type": "result", "subtype": "error_during_execution", "is_error": True}],
                    server_records=[shot(), click("Sumar"), END])
        out = h.run()
        self.assertFalse(out["score"]["success"])
        self.assertEqual(out["fault"], "cli_error")

    def test_capture_refused_coexisting_with_a_done_is_a_failure_not_a_success(self):
        h = Harness(self, "G01", 1, final_state=GOALS["initial_states"]["testapp"], events=session("Sumar"),
                    server_records=[shot(), refused("screenshot", "CAPTURE_REFUSED", reason="BACKEND_FAILED"),
                                    click("Sumar"), END])
        out = h.run()
        self.assertFalse(out["score"]["success"])
        self.assertEqual(out["fault"], "capture_refused")
        self.assertTrue(out["capture_refused"])

    def test_a_clean_run_is_still_a_success(self):
        out = self.run_with()
        self.assertTrue(out["score"]["success"])
        self.assertNotIn("fault", out)


# ---- R9: the registered native settings are the effective ones, and the pool key follows them -------------

class R9EffectiveSettingsTests(unittest.TestCase):
    def test_native_default_command_has_registered_cap_and_effort(self):
        args = pr.parse_args(["--plan", "--model", "claude-haiku-5-5"])
        self.assertEqual(args.max_budget_usd, 0.5)
        argv = pr.pixel_cli(args).build_argv(model=args.model, mcp_config=Path("m.json"),
                                             system_instruction=pa.SYSTEM_INSTRUCTION, max_budget_usd=args.max_budget_usd)
        self.assertEqual(argv[argv.index("--max-budget-usd") + 1], "0.5")
        self.assertEqual(argv[argv.index("--effort") + 1], "max")

    def test_a_route_pins_effort_by_its_environment_and_takes_no_dollar_cap(self):
        args = pr.parse_args(["--plan", "--route", "deepseek", "--model", "deepseek-flash"])
        argv = pr.pixel_cli(args).build_argv(model=args.model, mcp_config=Path("m.json"),
                                             system_instruction=pa.SYSTEM_INSTRUCTION, max_budget_usd=0.5)
        self.assertNotIn("--max-budget-usd", argv)

    def fingerprint(self, h, **changes):
        for name, value in changes.items():
            setattr(h.args, name, value)
        return pr.series_fingerprint(h.args, h.proof, {"resolution": "2560x1440", "scaling": "100%",
                                                       "capture_backend": "printwindow"})

    def test_each_effective_cap_change_changes_pool_key(self):
        base = Harness(self, "G01", 2, final_state={}, events=[], server_records=[END])
        reference = self.fingerprint(base)
        self.assertEqual(reference["effort"], "max")
        for change in ({"goal_timeout": 2.0}, {"max_budget_usd": 99.0}, {"max_live_session": 7}):
            h = Harness(self, "G01", 2, final_state={}, events=[], server_records=[END])
            changed = self.fingerprint(h, **change)
            self.assertIn("caps", plan.differences(reference, changed), change)

    def test_a_route_token_cap_change_changes_the_pool_key(self):
        a = Harness(self, "G01", 2, final_state={}, events=[], server_records=[END], route="deepseek")
        b = Harness(self, "G01", 2, final_state={}, events=[], server_records=[END], route="deepseek")
        self.assertEqual(plan.differences(self.fingerprint(a), self.fingerprint(b)), [])
        self.assertIn("caps", plan.differences(self.fingerprint(a), self.fingerprint(b, max_context_tokens_per_goal=1)))

    def test_the_fingerprint_describes_the_effective_arguments_not_a_static_table(self):
        h = Harness(self, "G01", 2, final_state={}, events=[], server_records=[END])
        caps = json.loads(self.fingerprint(h, goal_timeout=123.0)["caps"])
        self.assertEqual(caps["wall_s_per_goal"], 123.0)
        self.assertEqual(caps["usd_cap"], 0.5)
        self.assertEqual(caps["session_budget"], 5)


class R9PreflightTests(Base):
    def test_the_preflight_command_has_the_same_cap_and_effort_as_production(self):
        proof = self.run_preflight()
        runs = [json.loads(line) for line in self.argv_file.read_text().splitlines()]
        self.assertEqual(len(runs), 3)
        for argv in runs:
            self.assertEqual(argv[argv.index("--max-budget-usd") + 1], "0.5")
            self.assertEqual(argv[argv.index("--effort") + 1], "max")
        self.assertTrue(proof["passed"], proof["problems"])

    def test_a_native_request_with_another_effort_fails_the_preflight(self):
        proof = self.run_preflight("native_bad_effort")
        self.assertFalse(proof["passed"])
        self.assertIn("pixel_effort_differs", proof["problems"])


# ---- R10: the recipient comes from the route, not from the init event -------------------------------------

class R10ProviderEstimateTests(unittest.TestCase):
    def test_provider_route_without_provider_in_init_has_unavailable_image_estimate(self):
        for route in ("deepseek", "mimo"):
            events = [assistant("a0", use("s0", "screenshot")), result("s0", __import__("test_runner").shot_part(),
                                                                       text_part()), final()]
            h = Harness(self, "G10", 1, final_state=GOALS["initial_states"]["testapp"], events=events,
                        server_records=[shot(), END], route=route)
            out = h.run()
            self.assertNotIn("aborted", out)
            self.assertIsNone(out["metrics"]["image_token_estimate"], route)
            self.assertIn("unavailable", out["metrics"]["image_token_estimate_basis"], route)

    def test_the_native_route_keeps_the_retained_history_calculation(self):
        events = []
        for n in range(2):
            events += [assistant(f"a{n}", use(f"s{n}", "screenshot")),
                       result(f"s{n}", __import__("test_runner").shot_part(1280, 720), text_part())]
        h = Harness(self, "G10", 1, final_state=GOALS["initial_states"]["testapp"], events=[*events, final()],
                    server_records=[shot(), END])
        self.assertEqual(h.run()["metrics"]["image_token_estimate"], 3 * 1196)

    def test_no_provider_is_inferred_from_a_model_name(self):
        source = (HERE / "pixel_agent.py").read_text(encoding="utf-8")
        self.assertNotIn('"deepseek" in', source)
        self.assertNotIn("startswith(\"deepseek", source)


# ---- layout and hygiene -----------------------------------------------------------------------------------

class LayoutAndHygieneTests(unittest.TestCase):
    @unittest.skipUnless(struct.calcsize("P") == 8, "the x64 layout")
    def test_the_sendinput_structures_have_the_documented_x64_layout(self):
        import ctypes
        mouse, key, input_, *_ = w32._structures()  # noqa: SLF001 - pure ctypes types, no Win32 call
        self.assertEqual((ctypes.sizeof(mouse), ctypes.sizeof(key), ctypes.sizeof(input_)), (32, 24, 40))
        self.assertEqual(input_.u.offset, 8)
        self.assertEqual((mouse.dwExtraInfo.offset, key.dwExtraInfo.offset), (24, 16))

    def test_no_file_of_the_package_ends_in_a_blank_line_or_without_a_newline(self):
        for path in sorted(HERE.glob("*")):
            if path.suffix not in (".py", ".md"):
                continue
            data = path.read_bytes().replace(b"\r\n", b"\n")
            if not data:
                continue  # an empty package marker
            self.assertTrue(data.endswith(b"\n"), path.name)
            self.assertFalse(data.endswith(b"\n\n"), f"{path.name} ends with a blank line")


if __name__ == "__main__":
    unittest.main()
