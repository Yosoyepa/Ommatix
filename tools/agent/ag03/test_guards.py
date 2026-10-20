"""AG-03 guards (ADR-0038 decision 4): every guard as a pure function, the delivery-time recheck as a
trace with a fake clock and a fake operating system. No input is ever sent (AGENTS.md section 6): the
sender is a fake that records batches, and the Win32 code paths are never imported as running code."""

import dataclasses
import json
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
import pixel_win32 as w32  # noqa: E402
import proxy  # noqa: E402

W = fake_os.window_4x3()
ATT = g.Attested(pid=4242, exe=g.TESTAPP_EXE)
SHOT = g.Shot(t=0.0, hwnd=W.hwnd, client=W.client, width=4, height=3)


def ev(**kw):
    base = dict(stop_present=False, foreground=W, root_at_point=W.hwnd, modifiers_held=False,
                focus=g.Focus(hwnd=W.hwnd + 1, top_hwnd=W.hwnd, pid=W.pid))
    return g.Evidence(**(base | kw))


class ConstantTests(unittest.TestCase):
    def test_the_allowed_executables_are_exactly_two_and_exclude_the_shell(self):
        self.assertEqual(sorted(g.ALLOWED_EXES), ["ommatix-testapp.exe", "ommatix-testeditor.exe"])
        for name in ("explorer.exe", "applicationframehost.exe"):
            self.assertNotIn(name, g.ALLOWED_EXES)

    def test_an_attestation_for_another_executable_cannot_be_built(self):
        for exe in ("explorer.exe", "applicationframehost.exe", "notepad.exe", ""):
            with self.assertRaises(ValueError):
                g.Attested(pid=1, exe=exe)


class WindowGuardTests(unittest.TestCase):
    def check(self, window, expected, shot=None):
        self.assertEqual(g.guard_window(window, ATT, shot), expected)

    def test_the_target_window_passes(self):
        self.check(W, None)

    def test_no_foreground_is_a_refusal(self):
        self.check(None, "FOREGROUND_UNREADABLE")

    def test_another_process_is_refused(self):
        self.check(dataclasses.replace(W, pid=1), "FOREGROUND_NOT_TARGET_PROCESS")

    def test_the_shell_and_any_other_executable_are_refused(self):
        for exe in ("explorer.exe", "applicationframehost.exe", "notepad.exe", None):
            self.check(dataclasses.replace(W, exe=exe), "EXE_NOT_ALLOWED")

    def test_the_other_test_application_is_refused_for_this_goal(self):
        self.check(dataclasses.replace(W, exe=g.EDITOR_EXE), "EXE_NOT_ALLOWED")

    def test_helper_window_classes_are_refused(self):
        for cls in ("Winit Event Target", "MSCTFIME UI", "IME"):
            self.check(dataclasses.replace(W, cls=cls), "HELPER_WINDOW")

    def test_an_elevated_or_unreadable_window_is_refused(self):
        self.check(dataclasses.replace(W, elevated=True), "ELEVATED_OR_UNKNOWN")
        self.check(dataclasses.replace(W, elevated=None), "ELEVATED_OR_UNKNOWN")

    def test_a_window_off_the_primary_monitor_is_refused(self):
        self.check(dataclasses.replace(W, primary_monitor=False), "NOT_PRIMARY_MONITOR")

    def test_an_iconified_window_is_refused(self):
        self.check(dataclasses.replace(W, iconic=True), "WINDOW_ICONIC")

    def test_a_different_window_than_the_screenshot_came_from_is_refused(self):
        self.check(dataclasses.replace(W, hwnd=999), "WINDOW_CHANGED", SHOT)

    def test_a_moved_or_resized_window_is_refused_because_the_mapping_would_be_wrong(self):
        self.check(dataclasses.replace(W, client=(5, 5, 9, 8)), "GEOMETRY_CHANGED", SHOT)


class PointStopAgeModifierFocusTests(unittest.TestCase):
    def test_a_point_outside_the_client_rectangle_is_refused(self):
        for x, y in ((-1, 0), (0, -1), (4, 1), (1, 3), (100, 100)):
            self.assertEqual(g.guard_point(W, x, y, W.hwnd), "POINT_OUTSIDE_CLIENT")
        self.assertIsNone(g.guard_point(W, 3, 2, W.hwnd))

    def test_another_window_in_front_at_the_point_is_refused(self):
        self.assertEqual(g.guard_point(W, 1, 1, 999), "POINT_WINDOW_MISMATCH")
        self.assertEqual(g.guard_point(W, 1, 1, None), "POINT_WINDOW_MISMATCH")

    def test_a_held_or_unreadable_modifier_is_refused(self):
        self.assertEqual(g.guard_modifiers(True), "MODIFIER_HELD")
        self.assertEqual(g.guard_modifiers(None), "MODIFIERS_UNREADABLE")
        self.assertIsNone(g.guard_modifiers(False))

    def test_the_stop_file_present_or_unreadable_is_a_refusal(self):
        self.assertEqual(g.guard_stop(True), "STOP_ENGAGED")
        self.assertEqual(g.guard_stop(None), "STOP_UNREADABLE")
        self.assertIsNone(g.guard_stop(False))

    def test_a_stale_missing_or_future_screenshot_is_refused(self):
        self.assertIsNone(g.guard_shot_age(10.0, SHOT))
        self.assertEqual(g.guard_shot_age(10.01, SHOT), "SCREENSHOT_STALE")
        self.assertEqual(g.guard_shot_age(1.0, None), "SCREENSHOT_MISSING")
        self.assertEqual(g.guard_shot_age(-1.0, SHOT), "SCREENSHOT_STALE")

    def test_the_focus_window_must_be_the_attested_window_or_a_descendant_in_its_process(self):
        self.assertIsNone(g.guard_focus(g.Focus(5, W.hwnd, W.pid), W))
        self.assertEqual(g.guard_focus(g.Focus(5, 999, W.pid), W), "FOCUS_OUTSIDE_WINDOW")
        self.assertEqual(g.guard_focus(g.Focus(5, W.hwnd, 7), W), "FOCUS_OUTSIDE_WINDOW")
        self.assertEqual(g.guard_focus(None, W), "FOCUS_UNREADABLE")

    def test_the_screen_point_scales_the_client_rectangle_to_the_image(self):
        shot = g.Shot(t=0, hwnd=1, client=(100, 50, 1380, 770), width=640, height=360)
        self.assertEqual(g.screen_point(shot, 0, 0), (100, 50))
        self.assertEqual(g.screen_point(shot, 320, 180), (740, 410))


class BatchCheckTests(unittest.TestCase):
    def test_a_click_batch_passes_when_every_fact_holds(self):
        self.assertIsNone(g.check_batch(ev(), 5.0, SHOT, ATT, (1, 1)))

    def test_a_type_text_batch_checks_the_focus_window_and_has_no_point(self):
        self.assertIsNone(g.check_batch(ev(), 5.0, SHOT, ATT, None))
        self.assertEqual(g.check_batch(ev(focus=g.Focus(5, 999, W.pid)), 5.0, SHOT, ATT, None), "FOCUS_OUTSIDE_WINDOW")
        self.assertEqual(g.check_batch(ev(focus=None), 5.0, SHOT, ATT, None), "FOCUS_UNREADABLE")

    def test_each_fact_alone_refuses_a_click_batch(self):
        cases = [(ev(stop_present=True), 5.0, "STOP_ENGAGED"), (ev(), 10.5, "SCREENSHOT_STALE"),
                 (ev(foreground=dataclasses.replace(W, pid=1)), 5.0, "FOREGROUND_NOT_TARGET_PROCESS"),
                 (ev(root_at_point=999), 5.0, "POINT_WINDOW_MISMATCH"), (ev(modifiers_held=True), 5.0, "MODIFIER_HELD")]
        for evidence, now, token in cases:
            self.assertEqual(g.check_batch(evidence, now, SHOT, ATT, (1, 1)), token)


# ---- the server driving the guards, with a fake clock and a fake operating system --------------------

class Rig:
    """A real-mode pixel server over fakes and a real ledger file; the screenshot is taken at t = 0."""

    def __init__(self, test, *, phase=2, max_live=5, max_goal=3, on_reserve=None):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-guards-")
        test.addCleanup(scratch.cleanup)
        self.ledger = Path(scratch.name) / "ledger.json"
        self.clock = fake_os.FakeClock(0.0)
        self.os = fake_os.FakeOs(self.clock, W)
        self.sender = fake_os.FakeSender()
        self.records: list[dict] = []
        backend = fake_os.SceneBackend("opaque", app_rendered=True)

        def reserve():
            if on_reserve is not None:
                on_reserve(self)
            proxy.reserve(self.ledger, "G01", max_live, max_goal)
        cfg = ps.ServerConfig(phase=phase, synthetic=False, goal_id="G01", attested=ATT,
                              vocabulary=("Sumar", "Restar"))
        self.server = ps.PixelServer(cfg, ps.Deps(clock=self.clock, os=self.os, backend=backend,
                                                  admission=fake_os.full_admission(), sender=self.sender,
                                                  reserve=reserve, log=self.records.append,
                                                  hit_test=lambda x, y: "Sumar"))

    def call(self, tool, **args):
        return self.server.call_tool(tool, args)

    def shoot(self):
        result = self.call("screenshot")
        assert not result.get("isError"), result
        return result

    def code(self, result):
        if not result.get("isError"):
            return None
        return json.loads(result["content"][0]["text"])["error"]["code"]

    def spent(self):
        return proxy.read_ledger(self.ledger)["total"]


class DeliveryTests(unittest.TestCase):
    def test_a_click_is_one_batch_of_move_press_release_and_spends_one_action(self):
        rig = Rig(self)
        rig.shoot()
        rig.clock.advance(1.0)
        result = rig.call("click", x=2, y=1)
        self.assertIsNone(rig.code(result))
        self.assertEqual(len(rig.sender.batches), 1)
        self.assertEqual([e["kind"] for e in rig.sender.batches[0]], ["mouse"] * 3)
        self.assertEqual(rig.spent(), 1)

    def test_type_text_is_one_batch_of_unicode_key_events(self):
        rig = Rig(self)
        rig.shoot()
        result = rig.call("type_text", text="hola")
        self.assertIsNone(rig.code(result))
        self.assertEqual(len(rig.sender.batches), 1)
        self.assertEqual(len(rig.sender.batches[0]), 8)
        self.assertEqual(rig.spent(), 1)

    def test_phase_one_sends_no_input_spends_nothing_and_only_reads_to_score(self):
        rig = Rig(self, phase=1)
        rig.shoot()
        calls = rig.os.evidence_calls
        result = rig.call("click", x=2, y=1)
        self.assertEqual(json.loads(result["content"][0]["text"]), {"status": "simulated"})
        self.assertEqual(rig.call("type_text", text="hola")["content"][0]["text"], '{"status": "simulated"}')
        # R5: the only operating-system reads are the read-only ones - the click's attestation of the target
        # before the scoring hit test (type_text reads nothing) - and nothing is reserved or sent
        self.assertEqual(rig.os.evidence_calls, calls + 1)
        self.assertEqual(rig.sender.batches, [])
        self.assertFalse(rig.ledger.exists())
        self.assertEqual(rig.records[1]["label"], "Sumar")  # the read-only hit test, for scoring and the log

    def test_a_stop_file_present_at_the_start_refuses_before_any_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.stop = True
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "STOP_ENGAGED")
        self.assertEqual(rig.sender.batches, [])
        self.assertFalse(rig.ledger.exists())

    def test_a_click_without_a_screenshot_is_refused(self):
        rig = Rig(self)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_MISSING")
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "SCREENSHOT_MISSING")

    def test_a_stale_screenshot_refuses_before_the_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.clock.advance(10.5)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        self.assertFalse(rig.ledger.exists())
        self.assertEqual(rig.sender.batches, [])

    def test_the_wrong_foreground_refuses_after_the_reservation_which_is_kept(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.foreground = dataclasses.replace(W, pid=1)
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "FOREGROUND_NOT_TARGET_PROCESS")
        self.assertEqual(rig.spent(), 1)
        self.assertEqual(rig.sender.batches, [])

    def test_another_window_at_the_point_refuses_a_click_and_keeps_the_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.root = 999
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "POINT_WINDOW_MISMATCH")
        self.assertEqual(rig.spent(), 1)

    def test_a_held_modifier_refuses_and_keeps_the_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.modifiers = True
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "MODIFIER_HELD")
        self.assertEqual(rig.spent(), 1)
        self.assertEqual(rig.sender.batches, [])

    def test_click_coordinates_outside_the_image_are_refused_without_spending_and_end_the_session(self):
        for x, y in ((4, 0), (0, 3), (-1, 0), (99, 99)):
            rig = Rig(self)  # the refusal is terminal (R2), so each case needs its own session
            rig.shoot()
            self.assertEqual(rig.code(rig.call("click", x=x, y=y)), "POINT_OUTSIDE_CLIENT")
            self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SESSION_TERMINATED")
            self.assertFalse(rig.ledger.exists())

    def test_malformed_arguments_are_refused(self):
        rig = Rig(self)
        rig.shoot()
        for args in ({"x": 1}, {"x": "1", "y": 1}, {"x": True, "y": 1}, {"x": 1.5, "y": 1}, {"x": 1, "y": 1, "z": 1}):
            self.assertEqual(rig.code(rig.call("click", **args)), "INVALID_ARGUMENT")
        for text in ("", "x" * 17, "a\nb", "\t", "ñ", 5):
            self.assertEqual(rig.code(rig.call("type_text", text=text)), "INVALID_ARGUMENT")
        self.assertEqual(rig.code(rig.call("screenshot", x=1)), "INVALID_ARGUMENT")
        self.assertEqual(rig.code(rig.call("scroll")), "UNKNOWN_TOOL")
        self.assertFalse(rig.ledger.exists())


class DeliveryRecheckTests(unittest.TestCase):
    """Decision 4 item 8: after the reservation and any blocking work, with a fresh clock reading and a
    fresh read of the system, the stop file, the screenshot age, the modifiers and the window evidence are
    checked again immediately before the batch; the reservation stays spent."""

    def test_the_review_trace_ends_in_a_refusal_with_no_input(self):
        # screenshot at t = 0; the age check passes at 9.9 s; the ledger wait lasts to 11.9 s;
        # the operator engages the stop at 10 s.
        def wait_for_the_ledger(rig):
            rig.clock.t = 11.9
        rig = Rig(self, on_reserve=wait_for_the_ledger)
        rig.os.timeline.append((10.0, lambda os_: setattr(os_, "stop", True)))
        rig.shoot()
        rig.clock.t = 9.9
        result = rig.call("click", x=1, y=1)
        self.assertEqual(rig.code(result), "STOP_ENGAGED")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)
        self.assertEqual(rig.records[-1]["stage"], "delivery")

    def test_the_screenshot_ageing_past_ten_seconds_during_the_ledger_wait_refuses_a_click(self):
        rig = Rig(self, on_reserve=lambda rig_: setattr(rig_.clock, "t", 11.9))
        rig.shoot()
        rig.clock.t = 9.9
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "SCREENSHOT_STALE")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_the_screenshot_ageing_past_ten_seconds_during_the_wait_refuses_type_text(self):
        rig = Rig(self, on_reserve=lambda rig_: setattr(rig_.clock, "t", 11.9))
        rig.shoot()
        rig.clock.t = 9.9
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "SCREENSHOT_STALE")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_the_stop_file_alone_appearing_during_the_wait_refuses(self):
        rig = Rig(self, on_reserve=lambda rig_: setattr(rig_.clock, "t", 5.0))
        rig.os.timeline.append((4.0, lambda os_: setattr(os_, "stop", True)))
        rig.shoot()
        rig.clock.t = 1.0
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "STOP_ENGAGED")
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def _between_readings(self, tool, mutate, expected, **args):
        """The first reading (initial guard) passes; ``mutate`` runs before the second (the delivery check)."""
        rig = Rig(self)
        rig.os.hooks[3] = mutate  # reading 1 is the screenshot's; 2 the initial guard; 3 the delivery check
        rig.shoot()
        self.assertEqual(rig.code(rig.call(tool, **args)), expected)
        self.assertEqual(rig.os.evidence_calls, 3)
        self.assertEqual(rig.sender.batches, [])
        self.assertEqual(rig.spent(), 1)

    def test_a_modifier_pressed_between_the_initial_guard_and_delivery_refuses_a_click(self):
        self._between_readings("click", lambda os_: setattr(os_, "modifiers", True), "MODIFIER_HELD", x=1, y=1)

    def test_a_modifier_pressed_between_the_initial_guard_and_delivery_refuses_type_text(self):
        self._between_readings("type_text", lambda os_: setattr(os_, "modifiers", True), "MODIFIER_HELD", text="a")

    def test_a_foreground_change_between_the_initial_guard_and_delivery_refuses_a_click(self):
        self._between_readings("click", lambda os_: setattr(os_, "foreground", dataclasses.replace(W, pid=1)),
                               "FOREGROUND_NOT_TARGET_PROCESS", x=1, y=1)

    def test_a_foreground_change_between_the_initial_guard_and_delivery_refuses_type_text(self):
        self._between_readings("type_text", lambda os_: setattr(os_, "foreground", dataclasses.replace(W, pid=1)),
                               "FOREGROUND_NOT_TARGET_PROCESS", text="a")

    def test_a_window_in_front_of_the_point_between_the_two_readings_refuses_a_click(self):
        self._between_readings("click", lambda os_: setattr(os_, "root", 999), "POINT_WINDOW_MISMATCH", x=1, y=1)

    def test_a_focus_window_outside_the_attested_one_between_the_two_readings_refuses_type_text(self):
        self._between_readings("type_text", lambda os_: setattr(os_, "focus", g.Focus(1, 999, W.pid)),
                               "FOCUS_OUTSIDE_WINDOW", text="a")

    def test_a_focus_window_of_another_process_refuses_type_text(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.focus = g.Focus(1, W.hwnd, 7)
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "FOCUS_OUTSIDE_WINDOW")
        self.assertEqual(rig.spent(), 1)

    def test_an_unreadable_operating_system_refuses_and_keeps_the_reservation(self):
        rig = Rig(self)
        rig.shoot()
        rig.os.fail = True
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "OS_READ_FAILED")
        self.assertEqual(rig.spent(), 1)
        self.assertEqual(rig.sender.batches, [])

    def test_a_sender_that_fails_is_reported_not_claimed_as_delivered(self):
        rig = Rig(self)
        rig.sender.ok = False
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "INPUT_FAILED")
        self.assertEqual(rig.records[-1]["delivered"], "uncertain")  # never claimed as delivered nor as nothing sent


class LedgerTests(unittest.TestCase):
    def test_an_exhausted_goal_budget_refuses_without_reading_the_system_or_sending(self):
        rig = Rig(self, max_goal=1)
        rig.shoot()
        self.assertIsNone(rig.code(rig.call("click", x=1, y=1)))
        reads = rig.os.evidence_calls
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(rig.os.evidence_calls, reads)
        self.assertEqual(len(rig.sender.batches), 1)
        self.assertEqual(rig.spent(), 1)

    def test_an_exhausted_session_budget_refuses(self):
        rig = Rig(self, max_live=0)
        rig.shoot()
        self.assertEqual(rig.code(rig.call("type_text", text="a")), "LIVE_BUDGET_EXHAUSTED")
        self.assertEqual(rig.sender.batches, [])

    def test_an_unreadable_ledger_refuses_everything(self):
        rig = Rig(self)
        rig.ledger.write_text("not a ledger", encoding="ascii")
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "LEDGER_UNREADABLE")
        self.assertEqual(rig.sender.batches, [])

    def test_the_ledger_is_the_ag02_ledger_not_a_copy(self):
        self.assertIs(ps.proxy.reserve, proxy.reserve)
        rig = Rig(self)
        rig.shoot()
        rig.call("click", x=1, y=1)
        data = json.loads(rig.ledger.read_text(encoding="ascii"))
        self.assertEqual(set(data), {"v", "total", "goals"})
        self.assertEqual(data["goals"], {"G01": 1})

    def test_without_a_reserve_function_a_live_action_is_refused(self):
        rig = Rig(self)
        rig.server.deps.reserve = None
        rig.shoot()
        self.assertEqual(rig.code(rig.call("click", x=1, y=1)), "NO_LIVE_BUDGET")


class ScreenshotGuardTests(unittest.TestCase):
    """The same checks gate ``screenshot`` where they apply (decision 4): the stop file, the foreground
    window of item 5 and the capture conditions of decision 5. No image is returned on a refusal."""

    def test_a_screenshot_of_the_wrong_process_is_refused_and_no_image_is_returned(self):
        rig = Rig(self)
        rig.os.foreground = dataclasses.replace(W, pid=1)
        result = rig.call("screenshot")
        self.assertEqual(rig.code(result), "FOREGROUND_NOT_TARGET_PROCESS")
        self.assertEqual({c["type"] for c in result["content"]}, {"text"})

    def test_a_screenshot_with_the_stop_file_present_is_refused(self):
        rig = Rig(self)
        rig.os.stop = True
        self.assertEqual(rig.code(rig.call("screenshot")), "STOP_ENGAGED")

    def test_a_backend_that_is_not_admitted_gives_capture_refused_and_no_image(self):
        rig = Rig(self)
        rig.server.deps.admission = None
        result = rig.call("screenshot")
        self.assertEqual(rig.code(result), "CAPTURE_REFUSED")
        self.assertEqual({c["type"] for c in result["content"]}, {"text"})
        self.assertEqual(rig.records[-1]["reason"], "BACKEND_NOT_ADMITTED")
        self.assertEqual(rig.call("click", x=1, y=1)["isError"], True)  # and there is no screenshot to click on

    def test_a_desktop_source_backend_is_refused_even_with_a_full_record(self):
        rig = Rig(self)
        rig.server.deps.backend = fake_os.SceneBackend("foreign_during_acquisition", app_rendered=False)
        self.assertEqual(rig.code(rig.call("screenshot")), "CAPTURE_REFUSED")
        self.assertEqual(rig.server.deps.backend.calls, 0)

    def test_a_failing_printwindow_gives_capture_refused_with_its_reason(self):
        rig = Rig(self)
        rig.server.deps.backend = fake_os.FailingBackend("raise")
        self.assertEqual(rig.code(rig.call("screenshot")), "CAPTURE_REFUSED")
        self.assertEqual(rig.records[-1]["reason"], "BACKEND_FAILED")

    def test_a_result_of_the_wrong_size_gives_capture_refused(self):
        rig = Rig(self)
        rig.server.deps.backend = fake_os.FailingBackend("wrong_size")
        self.assertEqual(rig.code(rig.call("screenshot")), "CAPTURE_REFUSED")
        self.assertEqual(rig.records[-1]["reason"], "WRONG_SIZE")

    def test_the_screenshot_records_its_age_origin_so_a_later_click_can_be_judged(self):
        rig = Rig(self)
        rig.clock.t = 3.0
        rig.shoot()
        self.assertEqual(rig.server.shot.t, 3.0)
        self.assertEqual(rig.server.shot.client, W.client)


class CapTests(unittest.TestCase):
    def test_the_thirty_first_screenshot_is_refused(self):
        rig = Rig(self)
        for _ in range(30):
            rig.shoot()
        self.assertEqual(rig.code(rig.call("screenshot")), "LIMIT_EXCEEDED")

    def test_the_sixty_first_tool_call_is_refused(self):
        rig = Rig(self)
        for _ in range(60):
            rig.call("scroll")  # unknown tools still count as calls
        self.assertEqual(rig.code(rig.call("screenshot")), "LIMIT_EXCEEDED")


class InputHelperTests(unittest.TestCase):
    VS = (0, 0, 1920, 1080)

    def test_a_click_is_move_press_release_in_absolute_virtual_desktop_units(self):
        events = w32.build_click_inputs(960, 540, self.VS)
        self.assertEqual(len(events), 3)
        self.assertEqual(events[0]["flags"], w32.MOUSEEVENTF_MOVE | w32.MOUSEEVENTF_ABSOLUTE | w32.MOUSEEVENTF_VIRTUALDESK)
        self.assertEqual((events[0]["dx"], events[0]["dy"]), (round(960 * 65535 / 1919), round(540 * 65535 / 1079)))
        self.assertEqual([e["flags"] for e in events[1:]], [w32.MOUSEEVENTF_LEFTDOWN, w32.MOUSEEVENTF_LEFTUP])

    def test_only_a_left_click_can_be_built(self):
        flags = {e["flags"] for e in w32.build_click_inputs(1, 1, self.VS)}
        self.assertTrue(all(not (f & 0x0008 or f & 0x0010 or f & 0x0800) for f in flags))  # no right button, no wheel

    def test_a_point_outside_the_virtual_screen_is_refused(self):
        for point in ((-1, 0), (0, -1), (1920, 0), (0, 1080)):
            with self.assertRaises(w32.InputRefused):
                w32.build_click_inputs(*point, self.VS)

    def test_text_is_one_press_and_release_per_character_in_one_batch(self):
        events = w32.build_text_inputs("a b")
        self.assertEqual(len(events), 6)
        self.assertEqual([e["scan"] for e in events[::2]], [97, 32, 98])
        self.assertTrue(all(e["flags"] & w32.KEYEVENTF_UNICODE for e in events))
        self.assertEqual([bool(e["flags"] & w32.KEYEVENTF_KEYUP) for e in events], [False, True] * 3)

    def test_a_sixteen_character_text_is_at_most_thirty_two_events(self):
        self.assertEqual(len(w32.build_text_inputs("x" * 16)), 32)

    def test_special_keys_and_long_or_empty_text_cannot_be_built(self):
        for text in ("", "x" * 17, "\n", "\r", "\t", "a\x1b", "\x7f", "é", None, 7):
            with self.assertRaises(w32.InputRefused):
                w32.build_text_inputs(text)

    def test_importing_the_win32_module_called_no_win32_function(self):
        # The real classes load their DLLs inside their methods; the module itself holds no handle.
        for name in ("user32", "kernel32", "gdi32", "advapi32", "_user32"):
            self.assertFalse(hasattr(w32, name))


if __name__ == "__main__":
    unittest.main()
