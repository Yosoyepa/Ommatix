"""Regression tests for the S4 driver: its activation target, the Calculator clear control and
the Notepad menu checks. No window is activated here, no UI Automation is read and no input is
sent. Run with: python -m unittest discover -s tools/s4 -p "test_*.py"
"""

import importlib.util
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s4_e2e_driver", HERE / "e2e_driver.py")
driver = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(driver)


class PickMainWindowTest(unittest.TestCase):
    def test_the_event_target_helper_is_not_the_main_window(self):
        # winit's hidden event target is listed first and is larger than the window itself
        windows = [(1, "Winit Thread Event Target", 1920 * 1080),
                   (2, "Window Class", 1936 * 1048)]
        self.assertEqual(driver.pick_main_window(windows), 2)

    def test_the_largest_non_helper_window_is_the_main_one(self):
        # a UWP application is hosted by an ApplicationFrameWindow; the IME window is a helper
        windows = [(7, "IME", 4000), (8, "ApplicationFrameWindow", 1000), (9, "Other", 500)]
        self.assertEqual(driver.pick_main_window(windows), 8)

    def test_no_main_window_means_no_activation_target(self):
        self.assertIsNone(driver.pick_main_window([(3, "MSCTFIME UI", 10)]))
        self.assertIsNone(driver.pick_main_window([]))


def element(label, actionability="confirmed", role="button"):
    return {"label": label, "actionability": actionability, "role": role}


class ClearControlTest(unittest.TestCase):
    """S4 third round, finding 3: E2E-02 must not depend on a button name that is absent."""

    def test_the_borrar_family_is_chosen_by_exact_label_in_a_fixed_order(self):
        everything = [element(n) for n in ("Borrar entrada", "Borrar todo", "Borrar")]
        self.assertEqual(driver.pick_control(everything, driver.CLEAR_CONTROLS), "Borrar")
        self.assertEqual(driver.pick_control(everything[:2], driver.CLEAR_CONTROLS), "Borrar todo")
        self.assertEqual(driver.pick_control(everything[:1], driver.CLEAR_CONTROLS),
                         "Borrar entrada")

    def test_other_clear_looking_controls_are_never_chosen(self):
        # The Scientific-mode tree after "Es igual a" in the third round: none of these is on
        # the list, so no clear control is chosen and the step fails before any input.
        other = [element(n) for n in ("Quitar entrada", "Borrar toda la memoria",
                                       "Borrar toda el historial", "borrar")]
        self.assertIsNone(driver.pick_control(other, driver.CLEAR_CONTROLS))

    def test_an_unconfirmed_control_is_not_chosen(self):
        elements = [element("Borrar", "unknown"), element("Borrar todo")]
        self.assertEqual(driver.pick_control(elements, driver.CLEAR_CONTROLS), "Borrar todo")
        self.assertIsNone(driver.pick_control([element("Borrar", "unknown")],
                                              driver.CLEAR_CONTROLS))

    def test_a_plain_label_is_itself(self):
        self.assertEqual(driver.pick_control([element("Siete")], "Siete"), "Siete")
        self.assertIsNone(driver.pick_control([element("Ocho")], "Siete"))

    def test_the_calculator_flow_clears_with_whichever_control_exists_and_checks_zero(self):
        flow = driver.calculator_flow()
        label, ocr, display = flow["steps"][-1]
        self.assertEqual(label, driver.CLEAR_CONTROLS)
        self.assertEqual(display, "0", "the display returns to 0, read through UI Automation")
        self.assertIs(ocr, driver.NOT_BY_OCR)
        # No step names a clear control as a plain label, and the initial state needs at
        # least one of them (and every key of the flow) before any input.
        self.assertFalse([s for s in flow["steps"] if s[0] in driver.CLEAR_CONTROLS])
        self.assertEqual(flow["initial"]["any_of"], driver.CLEAR_CONTROLS)
        self.assertNotIn("Borrar", flow["initial"]["exact"])


class InitialStateTest(unittest.TestCase):
    def test_the_initial_calculator_state_needs_one_clear_control(self):
        check = driver.calculator_flow()["initial"]
        keys = [element(n) for n in driver.CALC_BUTTONS]
        proto = driver.Protocol.__new__(driver.Protocol)
        self.assertFalse(proto.ocr_ok(keys, check), "no clear control")
        self.assertTrue(proto.ocr_ok(keys + [element("Borrar entrada")], check))
        self.assertFalse(proto.ocr_ok(keys[1:] + [element("Borrar")], check), "a key is missing")


class NotepadMenuTest(unittest.TestCase):
    """S4 third round, finding 4: the menu state is UI Automation's, and it is checked."""

    def test_the_menu_state_is_exactly_one_known_expand_collapse_state(self):
        self.assertEqual(driver.parse_menu_state("Collapsed"), "Collapsed")
        self.assertEqual(driver.parse_menu_state(" Expanded\r\n"), "Expanded")
        for unknown in ("", "Collapsed,Expanded", "Collapsed,Collapsed", "LeafNode",
                        "PartiallyExpanded", "garbage"):
            self.assertIsNone(driver.parse_menu_state(unknown), unknown)

    def test_a_run_starts_only_with_the_menu_collapsed(self):
        self.assertIsNone(driver.menu_start_failure("Collapsed"))
        for state in ("Expanded", None):
            self.assertIsNotNone(driver.menu_start_failure(state), state)

    def test_only_an_expanded_menu_is_closed_again(self):
        self.assertTrue(driver.needs_menu_restore("Expanded"))
        for state in ("Collapsed", None):
            self.assertFalse(driver.needs_menu_restore(state), state)

    def test_the_flow_verifies_open_and_closed_by_the_expanded_state_not_by_text(self):
        flow = driver.notepad_flow()
        self.assertIs(flow["initial"], driver.NOT_BY_OCR)
        self.assertEqual([(s[0], s[2]) for s in flow["steps"]],
                         [("Editar", "Expanded"), ("Editar", "Collapsed")])
        self.assertTrue(all(s[1] is driver.NOT_BY_OCR for s in flow["steps"]))


class FakeNotepad(driver.Protocol):
    """A Protocol with no server, no windows and no input: it records what the restore does."""

    def __init__(self, states, foreground_ok=True, closes=True, target=True):
        self.args = type("Args", (), {"app": "notepad"})()
        self.flow = driver.notepad_flow()
        self.states = list(states)
        self.foreground_ok = foreground_ok
        self.closes = closes
        self.has_target = target
        self.clicks = []
        self.stops = []

    def menu_state(self, hwnd):
        return self.states[0] if len(self.states) == 1 else self.states.pop(0)

    def soft_foreground_ok(self):
        return self.foreground_ok

    def observe(self):
        elements = [element("Editar")] if self.has_target else []
        snap = {"session_id": "s", "snapshot_id": "s1", "geometry_epoch": 1}
        return snap, elements, {}

    def restore_click(self, res, target, snap):
        self.clicks.append(target["label"])
        if self.closes:
            self.states = ["Collapsed"]
        return {"status": "effect_observed", "input_delivered": True}

    def engage_stop(self, reason):  # the restore must never reach the stop of the operator
        self.stops.append(reason)
        raise AssertionError("the restore touched the operator stop")


class RestoreMenuTest(unittest.TestCase):
    def restore(self, proto):
        res = {"run": 1, "observations": []}
        return proto.restore_menu(res, 1), res["menu_restore"]

    def test_an_expanded_menu_is_closed_with_one_click_on_editar(self):
        proto = FakeNotepad(["Expanded", "Expanded"])
        closed, record = self.restore(proto)
        self.assertTrue(closed)
        self.assertEqual(proto.clicks, ["Editar"])
        self.assertEqual(record, {"state_before": "Expanded", "clicked": True,
                                  "state_after": "Collapsed"})

    def test_a_collapsed_or_unreadable_menu_is_left_alone(self):
        for state in ("Collapsed", None):
            proto = FakeNotepad([state])
            closed, record = self.restore(proto)
            self.assertFalse(closed)
            self.assertEqual(proto.clicks, [], state)
            self.assertFalse(record["clicked"])

    def test_it_does_not_click_when_notepad_is_not_in_front(self):
        proto = FakeNotepad(["Expanded"], foreground_ok=False)
        closed, _ = self.restore(proto)
        self.assertFalse(closed)
        self.assertEqual(proto.clicks, [])
        self.assertEqual(proto.stops, [])

    def test_it_does_not_click_without_a_confirmed_editar_target(self):
        proto = FakeNotepad(["Expanded"], target=False)
        closed, _ = self.restore(proto)
        self.assertFalse(closed)
        self.assertEqual(proto.clicks, [])

    def test_a_menu_that_stays_open_is_clicked_once_and_never_retried(self):
        proto = FakeNotepad(["Expanded"], closes=False)
        closed, record = self.restore(proto)
        self.assertFalse(closed)
        self.assertEqual(proto.clicks, ["Editar"], "one click, no retry loop")
        self.assertEqual(record["state_after"], "Expanded")


if __name__ == "__main__":
    unittest.main()
