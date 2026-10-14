"""Regression tests for the S4 driver: its activation target, the Calculator clear control, the
Notepad menu checks, the per-round live cap, the action timings, grounding by identity and the
Calculator Standard-mode step. No window is activated here, no UI Automation is read and no
input is sent: the desktop is a fake. Run with:
python -m unittest discover -s tools/s4 -p "test_*.py"
"""

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

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


# The `Editar` menu bar item as UI Automation reports it: one on-screen MenuItem.
EDITAR_ROW = {"runtime_id": "5.1", "automation_id": "Edit", "control_type": "MenuItem",
              "offscreen": False}


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
        self.ident_rows = [dict(EDITAR_ROW)]  # what UI Automation finds under the name
        self.chain = [dict(EDITAR_ROW)]  # what is under the cursor after the click
        self.identify_calls = []

    def menu_state(self, hwnd):
        return self.states[0] if len(self.states) == 1 else self.states.pop(0)

    def current_hwnd(self):
        return 77

    def identify(self, hwnd, label, control_types):
        self.identify_calls.append((hwnd, label, control_types))
        return self.ident_rows

    def cursor_chain(self):
        return self.chain

    def soft_foreground_ok(self):
        return self.foreground_ok

    def observe(self):
        elements = [element("Editar", role="unknown")] if self.has_target else []
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
        self.assertEqual({k: record[k] for k in ("state_before", "clicked", "state_after")},
                         {"state_before": "Expanded", "clicked": True,
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


# ---- S4 fifth-round preparation (fourth-round findings 1, 2, 4 and 5) -------------------------


class QuietTest(unittest.TestCase):
    """No sleeping and no log lines: the fakes answer at once."""

    def setUp(self):
        for target, name in ((driver.time, "sleep"), (driver, "log")):
            patcher = mock.patch.object(target, name)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)


class RoundCapTest(QuietTest):
    """Fourth round, finding 1: LIVE_CAP counted the whole ledger, not the round."""

    def protocol(self, ledger_at_start, round_start):
        proto = driver.Protocol.__new__(driver.Protocol)
        proto.args = SimpleNamespace(max_live=10_000)
        proto.stop_file = None
        proto.ledger = self.tmp / "ledger.txt"
        proto.ledger.write_text(str(ledger_at_start), encoding="utf-8")
        proto.round_start = round_start
        proto.live_sent = 0
        return proto

    def test_a_round_never_gets_more_than_400_requests(self):
        self.assertLessEqual(driver.LIVE_CAP, 400)

    def test_a_round_that_starts_at_291_may_send_400_and_not_401(self):
        proto = self.protocol(291, 291)
        for _ in range(400):
            proto.spend_live()
        self.assertEqual(proto.round_live(), 400)
        with self.assertRaises(driver.Abort):
            proto.spend_live()
        self.assertEqual(proto.ledger_total(), 691, "the ledger only accumulates")

    def test_a_ledger_above_the_cap_does_not_stop_a_round_that_started_there(self):
        proto = self.protocol(291, 291)
        for _ in range(110):  # more than the 109 that were left under a whole-ledger cap
            proto.spend_live()
        self.assertEqual(proto.ledger_total(), 401)

    def test_the_round_range_is_ledger_start_to_ledger_end(self):
        proto = self.protocol(396, 291)
        self.assertEqual(proto.round_live(), 105)
        proto.spend_live()
        self.assertEqual((proto.round_live(), proto.ledger_total()), (106, 397))

    def test_the_max_live_of_an_invocation_still_applies(self):
        proto = self.protocol(0, 0)
        proto.args.max_live = 2
        proto.spend_live()
        proto.spend_live()
        with self.assertRaises(driver.Abort):
            proto.spend_live()

    def test_a_live_round_needs_a_start_inside_the_ledger_with_room_left(self):
        self.assertIsNone(driver.round_problem(True, 291, 396))
        self.assertIsNone(driver.round_problem(False, None, 396), "a probe sends nothing")
        self.assertIsNotNone(driver.round_problem(True, None, 396))
        self.assertIsNotNone(driver.round_problem(True, 400, 396), "start beyond the ledger")
        self.assertIsNotNone(driver.round_problem(True, -1, 396))
        self.assertIsNotNone(driver.round_problem(True, 0, 400), "the round's cap is spent")


class ActionTimingsTest(unittest.TestCase):
    """Fourth round, finding 2: io.ommatix/action_timings was dropped."""

    def test_action_timings_are_recorded_as_action_stages(self):
        meta = {driver.ACTION_TIMINGS_KEY: {"total_us": 82_800, "gate_us": 200,
                                            "freshness_us": 19_400, "delivery_us": 2_000,
                                            "focus_us": 0, "verification_us": 58_000}}
        self.assertEqual(driver.stage_ms(meta),
                         {"action_total": 82.8, "action_gate": 0.2, "action_freshness": 19.4,
                          "action_delivery": 2.0, "action_focus": 0.0,
                          "action_verification": 58.0})

    def test_observation_timings_keep_their_names(self):
        meta = {driver.s3.TIMINGS_KEY: {"total_us": 312_800, "accessibility_us": 65_100}}
        self.assertEqual(driver.stage_ms(meta), {"total": 312.8, "accessibility": 65.1})

    def test_both_sets_in_one_response_do_not_collide(self):
        meta = {driver.s3.TIMINGS_KEY: {"total_us": 1_000},
                driver.ACTION_TIMINGS_KEY: {"total_us": 2_000}}
        self.assertEqual(driver.stage_ms(meta), {"total": 1.0, "action_total": 2.0})

    def test_a_response_without_timings_has_no_stages(self):
        self.assertEqual(driver.stage_ms({}), {})


class IdentityFunctionsTest(unittest.TestCase):
    """Fourth round, finding 4: grounding is by RuntimeId or AutomationId, never by name."""

    def row(self, rid="42.1", aid="TogglePaneButton", ctype="Button", offscreen=False):
        return {"runtime_id": rid, "automation_id": aid, "control_type": ctype,
                "offscreen": offscreen}

    def test_rows_are_read_from_the_script_output(self):
        out = "42.1\tTogglePaneButton\tControlType.Button\t0\n42.9\t\tListItem\t1\nbroken\n"
        self.assertEqual(driver.parse_identity_rows(out),
                         [self.row(), self.row("42.9", "", "ListItem", True)])
        self.assertEqual(driver.parse_identity_rows(""), [])

    def test_exactly_one_identifiable_on_screen_match_is_clickable(self):
        status, found = driver.identity_decision([self.row()])
        self.assertEqual((status, found), ("ok", self.row()))
        self.assertEqual(driver.identity_decision([self.row(), self.row("42.2")])[0], "ambiguous")
        self.assertEqual(driver.identity_decision([])[0], "not_found")
        self.assertEqual(driver.identity_decision(None)[0], "unreadable")
        self.assertEqual(driver.identity_decision([self.row("", "")])[0], "unidentifiable")

    def test_an_off_screen_twin_is_not_a_candidate(self):
        status, found = driver.identity_decision([self.row(), self.row("42.2", offscreen=True)])
        self.assertEqual((status, found["runtime_id"]), ("ok", "42.1"))
        self.assertEqual(driver.identity_decision([self.row(offscreen=True)])[0], "not_found")

    def test_a_renamed_toggle_is_still_the_same_element(self):
        # "Abrir navegación" became "Cerrar navegación": the chain has no names at all, only
        # the identity, so a rename cannot change the verdict.
        self.assertTrue(driver.identity_hit(self.row(), [self.row()]))

    def test_the_runtime_id_decides_when_both_sides_have_one(self):
        self.assertFalse(driver.identity_hit(self.row(), [self.row("42.2")]),
                         "another element, even with the same AutomationId")
        self.assertTrue(driver.identity_hit(self.row(), [self.row("42.2", "x"), self.row()]),
                        "the parent is the element")

    def test_the_automation_id_decides_only_when_a_runtime_id_is_unavailable(self):
        self.assertTrue(driver.identity_hit(self.row("", "Send"), [self.row("", "Send")]))
        self.assertTrue(driver.identity_hit(self.row("42.1", "Send"), [self.row("", "Send")]))
        self.assertFalse(driver.identity_hit(self.row("", "Send"), [self.row("", "Other")]))

    def test_nothing_confirms_an_unreadable_chain_or_a_missing_identifier(self):
        self.assertFalse(driver.identity_hit(self.row(), []))
        self.assertFalse(driver.identity_hit(self.row("42.1", ""), [self.row("", "")]))

    def test_every_contract_role_with_a_control_has_its_uia_types(self):
        self.assertEqual(driver.ROLE_CONTROL_TYPES["button"], ("Button", "SplitButton"))
        self.assertIn("MenuItem", driver.ROLE_CONTROL_TYPES["unknown"])
        self.assertNotIn("text_node", driver.ROLE_CONTROL_TYPES)
        self.assertNotIn("icon_button", driver.ROLE_CONTROL_TYPES)

    def test_the_flows_have_no_name_based_grounding(self):
        for flow in (driver.testapp_flow(), driver.calculator_flow(), driver.notepad_flow()):
            self.assertNotIn("grounding", flow)
        for gone in ("named_control", "uia_named_contains_cursor", "UIA_NAMED_PS"):
            self.assertFalse(hasattr(driver, gone), gone)


def control(label, role, key, rid, aid, ctype):
    return {"label": label, "role": role, "key": key, "rid": rid, "aid": aid, "ctype": ctype}


class FakeCalculator(driver.Protocol):
    """A Protocol on a fake Calculator: no server, no windows, no UI Automation, no input. It
    records the clicks the engine's gate would have delivered."""

    def __init__(self, tmp, mode="other", std_count=1, pane_opens=True, max_live=4):
        self.args = SimpleNamespace(app="calculator", max_live=max_live, settle=0.0,
                                    results=tmp / "results.jsonl", debug_grounding=False,
                                    restart="each", mode="run")
        self.srv = self
        self.stop_file = None
        self.ledger = tmp / "ledger.txt"
        self.round_start = 0
        self.live_sent = 0
        self.results = []
        self.started = set()
        self.current = None
        self.notepad = None
        self.app_hwnd = None
        self.placements = 0
        self.flow = driver.calculator_flow()
        self.mode = mode
        self.std_count = std_count
        self.pane_opens = pane_opens
        self.pane_open = False
        self.clicks = []  # the labels clicked, in order
        self.closed = []
        self.last = None
        self.rows_override = None
        self.unreadable = False
        self.chain_override = None
        self.identify_calls = []
        self.statuses = []  # scripted engine statuses, then effect_observed

    def controls(self):
        nav = "Cerrar navegación" if self.pane_open else "Abrir navegación"
        found = [control(nav, "button", "nav", "42.1", "TogglePaneButton", "Button")]
        if self.pane_open:
            found += [control("Estándar", "unknown", f"std{i}", f"42.7.{i}", "Std", "ListItem")
                      for i in range(self.std_count)]
        return found

    def observe(self):
        elements = [{"label": c["label"], "role": c["role"], "actionability": "confirmed",
                     "label_source": "accessibility", "target_id": c["key"], "revision": 1}
                    for c in self.controls()]
        snap = {"session_id": "s", "snapshot_id": "s1", "geometry_epoch": 1}
        return snap, elements, {}

    def call(self, tool, args):  # the server
        assert tool == "execute_action" and args["dry_run"] is False
        clicked = next(c for c in self.controls() if c["key"] == args["target_id"])
        self.clicks.append(clicked["label"])
        self.last = clicked
        status = self.statuses.pop(0) if self.statuses else "effect_observed"
        if clicked["key"] == "nav" and self.pane_opens:
            self.pane_open = not self.pane_open
        if clicked["key"].startswith("std"):
            self.mode, self.pane_open = "standard", False
        out = {"status": status, "input_delivered": status != "rejected", "verified": True,
               "observed_effect": "visual_change"}
        if status == "rejected":
            out["error"] = {"code": "INVALID_ARGUMENT"}
        meta = {driver.ACTION_TIMINGS_KEY: {"total_us": 82_800, "gate_us": 200}}
        return out, False, meta, 5.0

    def identify(self, hwnd, label, control_types):
        self.identify_calls.append((hwnd, label, control_types))
        if self.unreadable:
            return None
        if self.rows_override is not None:
            return self.rows_override
        return [{"runtime_id": c["rid"], "automation_id": c["aid"], "control_type": c["ctype"],
                 "offscreen": False}
                for c in self.controls() if c["label"] == label and c["ctype"] in control_types]

    def cursor_chain(self):
        if self.chain_override is not None:
            return self.chain_override
        top = self.last
        return [{"runtime_id": top["rid"], "automation_id": top["aid"],
                 "control_type": top["ctype"], "offscreen": False},
                {"runtime_id": "42.0", "automation_id": "", "control_type": "Pane",
                 "offscreen": False}]

    def calc_mode(self, hwnd):
        return self.mode

    def launch(self):
        self.started.add(4242)
        return 4242

    def wait_foreground(self, pid):
        return True, 0

    def current_hwnd(self):
        return 77

    def require_foreground(self):
        return {"exe": "calculatorapp.exe", "pid": 4242, "hwnd": 77, "maximized_primary": True}

    def close(self, pid):
        if pid is not None and pid in self.started:
            self.closed.append(pid)
            self.started.discard(pid)


class GroundingByIdentityTest(QuietTest):
    """Fourth round, finding 4: the click goes only to the one element found by identity."""

    def run_steps(self, desk, flow):
        desk.flow = flow
        res = driver.new_result("E2E-02", 1)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, 77, 4242)
        return res

    def test_a_toggle_that_renames_itself_is_grounded_by_identity(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertTrue(desk.pane_open)
        self.assertEqual(desk.controls()[0]["label"], "Cerrar navegación", "renamed by the click")
        self.assertEqual(res["grounding"], [True])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (0, 0))
        self.assertIsNone(res["failure"])

    def test_the_identity_is_found_in_the_foreground_window_by_name_and_role_types(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(desk.identify_calls,
                         [(77, "Abrir navegación", driver.ROLE_CONTROL_TYPES["button"])])
        self.assertEqual(res["identity"][0]["runtime_id"], "42.1")
        self.assertEqual(res["identity"][0]["automation_id"], "TogglePaneButton")
        self.assertEqual(res["identity"][0]["control_type"], "Button")
        self.assertEqual(res["identity"][0]["status"], "ok")

    def test_more_than_one_match_sends_nothing(self):
        desk = FakeCalculator(self.tmp)
        twin = {"runtime_id": "42.9", "automation_id": "TogglePaneButton",
                "control_type": "Button", "offscreen": False}
        desk.rows_override = [dict(twin, runtime_id="42.1"), twin]
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(desk.clicks, [])
        self.assertEqual(desk.ledger_total(), 0, "not even a live request was spent")
        self.assertIn("ambiguous", res["failure"])
        self.assertEqual(res["identity"][0]["candidates"], 2)

    def test_no_match_or_an_unreadable_tree_sends_nothing(self):
        for rows, status in (([], "not_found"), (None, "unreadable")):
            desk = FakeCalculator(self.tmp)
            desk.rows_override = rows
            desk.unreadable = rows is None
            res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
            self.assertEqual(desk.clicks, [], status)
            self.assertIn(status, res["failure"])

    def test_an_element_with_no_identifier_is_not_clicked_by_name(self):
        desk = FakeCalculator(self.tmp)
        desk.rows_override = [{"runtime_id": "", "automation_id": "", "control_type": "Button",
                               "offscreen": False}]
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(desk.clicks, [])
        self.assertIn("unidentifiable", res["failure"])

    def test_a_role_without_a_uia_control_is_not_clicked(self):
        desk = FakeCalculator(self.tmp)
        original = desk.observe

        def observe():
            snap, elements, obs = original()
            return snap, [dict(e, role="text_node") for e in elements], obs

        desk.observe = observe
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(desk.clicks, [])
        self.assertIn("no_control_type", res["failure"])

    def test_a_click_that_reached_another_element_ends_the_run_with_no_further_input(self):
        desk = FakeCalculator(self.tmp)
        desk.chain_override = [{"runtime_id": "99.1", "automation_id": "TogglePaneButton",
                                "control_type": "Button", "offscreen": False}]
        flow = driver.single_click_flow("Abrir navegación")
        flow["steps"].append(("Cerrar navegación", driver.NOT_BY_OCR, None))
        res = self.run_steps(desk, flow)
        self.assertEqual(desk.clicks, ["Abrir navegación"], "the second step never ran")
        self.assertEqual(res["grounding"], [False])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertIn("not the element", res["failure"])

    def test_the_engine_timings_of_each_action_are_recorded_per_action(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk, driver.single_click_flow("Abrir navegación"))
        self.assertEqual(res["actions"][0]["stages_ms"], {"action_total": 82.8,
                                                          "action_gate": 0.2})


class ResultCountersTest(QuietTest):
    """Fourth round, finding 5: the mode-switch helper died with KeyError on a counter."""

    def test_every_counter_that_steps_increments_is_initialised(self):
        res = driver.new_result("E2E-02", 1)
        for key in ("steps", "actions_requested", "delivered", "typed_delivered",
                    "effect_observed", "indeterminate", "rejected", "stale_retries",
                    "ocr_checks_failed", "independent_checks", "independent_failed",
                    "engine_false_success", "grounding_wrong"):
            self.assertEqual(res[key], 0, key)
        self.assertEqual((res["observations"], res["actions"], res["failure"]), ([], [], None))

    def test_steps_counts_every_engine_status_without_a_missing_key(self):
        for status in ("effect_observed", "indeterminate", "rejected"):
            desk = FakeCalculator(self.tmp)
            desk.statuses = [status]
            desk.flow = driver.single_click_flow("Abrir navegación")
            res = driver.new_result("MODE", 0)
            snap, elements, _ = desk.observe()
            desk.steps(res, snap, elements, 77, 4242)
            self.assertEqual(res[status], 1, status)


class CalcStandardTest(QuietTest):
    """Fourth round, finding 4 and the owner's decision: the E2E itself switches the Calculator
    to Standard mode, through the engine's gate only."""

    def run_step(self, **kwargs):
        desk = FakeCalculator(self.tmp, **kwargs)
        return desk, desk.calc_standard()

    def test_it_opens_the_pane_then_picks_standard_and_verifies_the_mode(self):
        desk, summary = self.run_step()
        self.assertEqual(desk.clicks, ["Abrir navegación", "Estándar"])
        self.assertEqual(summary["nav_label"], "Abrir navegación", "the real label is recorded")
        self.assertEqual([c["grounded"] for c in summary["clicks"]], [True, True])
        self.assertEqual((summary["mode_before"], summary["mode_after"]), ("other", "standard"))
        self.assertTrue(summary["switched"])
        self.assertIsNone(summary["failure"])
        self.assertEqual([i["status"] for i in summary["identity"]], ["ok", "ok"])
        self.assertEqual(summary["identity"][0]["automation_id"], "TogglePaneButton")
        self.assertEqual((summary["ledger_start"], summary["ledger_end"]), (0, 2))

    def test_it_closes_the_calculator_it_started_and_only_that(self):
        desk, _ = self.run_step()
        self.assertEqual(desk.closed, [4242])
        self.assertEqual(desk.started, set())

    def test_it_writes_a_result_record_without_screen_text(self):
        self.run_step()
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertEqual(record["step"], "calc_standard")
        self.assertTrue(record["switched"])

    def test_a_calculator_already_in_standard_is_not_clicked(self):
        desk, summary = self.run_step(mode="standard")
        self.assertEqual(desk.clicks, [])
        self.assertTrue(summary["switched"])
        self.assertEqual(desk.closed, [4242])

    def test_an_unreadable_mode_means_no_click(self):
        desk, summary = self.run_step(mode="unknown")
        self.assertEqual(desk.clicks, [])
        self.assertFalse(summary["switched"])
        self.assertIn("cannot be read", summary["failure"])
        self.assertEqual(desk.closed, [4242])

    def test_a_pane_that_does_not_open_stops_without_another_click(self):
        desk, summary = self.run_step(pane_opens=False)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertIsNotNone(summary["failure"])
        self.assertEqual(desk.closed, [4242])

    def test_an_estandar_that_is_not_unique_stops_without_another_click(self):
        desk, summary = self.run_step(std_count=2)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertIn("ambiguous", summary["failure"])

    def test_an_estandar_that_is_not_confirmed_stops_without_another_click(self):
        desk = FakeCalculator(self.tmp)
        original = desk.observe

        def observe():
            snap, elements, obs = original()
            return snap, [dict(e, actionability="unknown") if e["label"] == "Estándar" else e
                          for e in elements], obs

        desk.observe = observe
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])

    def test_a_navigation_click_that_is_not_grounded_stops_the_step(self):
        desk = FakeCalculator(self.tmp)
        desk.chain_override = [{"runtime_id": "7.7", "automation_id": "Other",
                                "control_type": "Button", "offscreen": False}]
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertEqual(summary["clicks"][0]["grounded"], False)

    def test_an_ambiguous_navigation_identity_sends_nothing(self):
        desk = FakeCalculator(self.tmp)
        row = {"runtime_id": "1", "automation_id": "a", "control_type": "Button",
               "offscreen": False}
        desk.rows_override = [row, dict(row, runtime_id="2")]
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, [])
        self.assertFalse(summary["switched"])
        self.assertEqual(desk.ledger_total(), 0)

    def test_a_mode_that_is_not_standard_afterwards_is_reported_not_retried(self):
        desk = FakeCalculator(self.tmp)
        desk.calc_mode = lambda hwnd: "other"
        summary = desk.calc_standard()
        self.assertEqual(len(desk.clicks), 2, "no third click")
        self.assertFalse(summary["switched"])
        self.assertEqual(summary["mode_after"], "other")

    def test_the_step_cannot_spend_more_than_its_own_live_budget(self):
        self.assertLessEqual(driver.CALC_STANDARD_MAX_LIVE, 4)
        desk = FakeCalculator(self.tmp, max_live=1)
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])


class CalcModeTest(unittest.TestCase):
    def test_standard_needs_the_header_and_no_scientific_control(self):
        self.assertEqual(driver.calc_mode("Estándar", 0), "standard")
        self.assertEqual(driver.calc_mode("  Estándar ", 0), "standard")
        self.assertEqual(driver.calc_mode("Estándar", 2), "unknown", "the signals disagree")

    def test_another_header_is_another_mode_and_no_header_is_unknown(self):
        self.assertEqual(driver.calc_mode("Científica", 2), "other")
        self.assertEqual(driver.calc_mode("Científica", 0), "other")
        self.assertEqual(driver.calc_mode(None, 0), "unknown")
        self.assertEqual(driver.calc_mode("", 0), "unknown")

    def test_the_mode_read_is_parsed_strictly(self):
        self.assertEqual(driver.parse_calc_mode("1\tEstándar\t0"), ("Estándar", 0))
        self.assertEqual(driver.parse_calc_mode("0\t\t3"), (None, 3))
        for bad in ("", "1\tEstándar", "2\tx\t0", "1\tx\tmany"):
            self.assertIsNone(driver.parse_calc_mode(bad), bad)


# ---- S4 fifth-round findings 1 to 3 --------------------------------------------------------------


class RestoreGroundingTest(QuietTest):
    """Fifth round, finding 2: `restore_click` had no UI Automation identity grounding. Like every
    other click of the client it is grounded before and after, and a mismatch is reported and
    never followed by another input."""

    def restore(self, proto):
        res = {"run": 1, "observations": []}
        closed = proto.restore_menu(res, 1)
        return closed, res, res["menu_restore"]

    def test_a_restore_click_that_reaches_another_element_is_reported_and_not_retried(self):
        proto = FakeNotepad(["Expanded"])
        # After the click the cursor is over something else: not Editar.
        proto.chain = [{"runtime_id": "9.9", "automation_id": "Other", "control_type": "Button",
                        "offscreen": False}]
        _, res, record = self.restore(proto)
        self.assertEqual(proto.clicks, ["Editar"], "one click, nothing more")
        self.assertIs(record["grounded"], False)
        self.assertEqual(res["grounding_wrong"], 1)
        self.assertEqual(res["engine_false_success"], 1, "the engine said effect_observed")
        self.assertEqual(proto.stops, [])

    def test_a_grounded_restore_click_is_recorded_as_grounded(self):
        proto = FakeNotepad(["Expanded"])
        closed, res, record = self.restore(proto)
        self.assertTrue(closed)
        self.assertIs(record["grounded"], True)
        self.assertEqual(record["identity"]["status"], "ok")
        self.assertEqual(record["identity"]["automation_id"], "Edit")
        self.assertEqual((res.get("grounding_wrong", 0), res.get("engine_false_success", 0)),
                         (0, 0))

    def test_the_identity_is_looked_up_in_the_foreground_window_by_name_and_role_types(self):
        proto = FakeNotepad(["Expanded"])
        self.restore(proto)
        self.assertEqual(proto.identify_calls,
                         [(77, "Editar", driver.ROLE_CONTROL_TYPES["unknown"])])

    def test_no_single_identifiable_editar_means_no_restore_click(self):
        twin = dict(EDITAR_ROW, runtime_id="5.9")
        for rows, status in (([EDITAR_ROW, twin], "ambiguous"), ([], "not_found"),
                             (None, "unreadable"),
                             ([dict(EDITAR_ROW, runtime_id="", automation_id="")],
                              "unidentifiable")):
            proto = FakeNotepad(["Expanded"])
            proto.ident_rows = rows
            closed, _, record = self.restore(proto)
            self.assertFalse(closed, status)
            self.assertEqual(proto.clicks, [], status)
            self.assertFalse(record["clicked"], status)
            self.assertEqual(record["identity"]["status"], status)

    def test_the_polls_until_the_menu_reads_closed_are_recorded(self):
        proto = FakeNotepad(["Expanded"], closes=False)
        _, _, record = self.restore(proto)
        self.assertEqual(record["state_polls"], 8)
        proto = FakeNotepad(["Expanded", "Expanded"])
        _, _, record = self.restore(proto)
        self.assertEqual(record["state_polls"], 1)


class TargetRecordTest(QuietTest):
    """Fifth round, finding 3: `--calc-standard` stopped at `Estándar` with 'not found or not
    confirmed' and did not say which. The step now records, for each target label, how many
    candidates there were, their control types (never names) and the condition that stopped it."""

    def stopped(self, **kwargs):
        desk = FakeCalculator(self.tmp, **kwargs)
        summary = desk.calc_standard()
        return desk, summary, summary["targets"][-1]

    def test_zero_candidates_and_two_candidates_produce_distinct_records(self):
        _, _, zero = self.stopped(std_count=0)
        _, _, two = self.stopped(std_count=2)
        self.assertEqual((zero["candidates"], zero["stopped_by"]), (0, "zero_candidates"))
        self.assertEqual((two["candidates"], two["stopped_by"]), (2, "several_candidates"))
        self.assertEqual(zero["roles"], {})
        self.assertEqual(two["roles"], {"unknown": 2})
        self.assertEqual(two["uia"], {"candidates": 2, "offscreen": 0,
                                      "control_types": ["ListItem"]})
        self.assertEqual(zero["uia"], {"candidates": 0, "offscreen": 0, "control_types": []})
        self.assertNotEqual(zero, two)

    def test_one_candidate_that_is_not_confirmed_is_a_third_condition(self):
        desk = FakeCalculator(self.tmp)
        original = desk.observe

        def observe():
            snap, elements, obs = original()
            return snap, [dict(e, actionability="unknown") if e["label"] == "Estándar" else e
                          for e in elements], obs

        desk.observe = observe
        summary = desk.calc_standard()
        record = summary["targets"][-1]
        self.assertEqual(record["stopped_by"], "not_confirmed")
        self.assertEqual((record["candidates"], record["confirmed"]), (1, 0))
        self.assertEqual(record["actionability"], {"unknown": 1})
        self.assertEqual(record["uia"]["control_types"], ["ListItem"])
        self.assertIn("not found or not confirmed", summary["failure"])
        self.assertEqual(desk.clicks, ["Abrir navegación"], "no second click")

    def test_each_target_label_has_its_record_and_a_successful_one_has_no_stop(self):
        desk, summary, last = self.stopped()
        self.assertTrue(summary["switched"])
        self.assertEqual([t["step"] for t in summary["targets"]], [1, 2])
        self.assertEqual([t["stopped_by"] for t in summary["targets"]], [None, None])
        self.assertEqual([t["candidates"] for t in summary["targets"]], [1, 1])
        self.assertEqual(summary["targets"][0]["roles"], {"button": 1})
        self.assertNotIn("uia", last, "no UI Automation read on the path that proceeds")
        self.assertEqual(len(desk.identify_calls), 2, "only the two grounding lookups")

    def test_the_records_hold_no_label_or_name(self):
        for kwargs in ({"std_count": 0}, {"std_count": 2}, {}):
            _, summary, _ = self.stopped(**kwargs)
            text = json.dumps(summary["targets"], ensure_ascii=False)
            for name in ("Estándar", "Abrir navegación", "Cerrar navegación"):
                self.assertNotIn(name, text)

    def test_a_uia_read_that_fails_does_not_hide_the_stop(self):
        desk = FakeCalculator(self.tmp, std_count=0)
        original = desk.identify

        def identify(hwnd, label, control_types):
            if len(control_types) > 5:  # the diagnosis; the grounding lookups use role types
                raise OSError("powershell is gone")
            return original(hwnd, label, control_types)

        desk.identify = identify
        record = desk.calc_standard()["targets"][-1]
        self.assertEqual((record["stopped_by"], record["uia"]), ("zero_candidates", None))


class FakeNotepadRun(FakeCalculator):
    """The Edit menu on a fake Notepad: each click toggles it, and when `ids_change` the menu bar
    item gets a new RuntimeId once it expands (the cause finding 1 suspects)."""

    def __init__(self, tmp, ids_change=True, diag_error=None):
        super().__init__(tmp)
        self.args.app = "notepad"
        self.flow = driver.notepad_flow()
        self.menu = "Collapsed"
        self.ids_change = ids_change
        self.diag_error = diag_error  # None, "unreadable" or "raises"
        self.order = []

    def controls(self):
        rid = "5.2" if self.menu == "Expanded" and self.ids_change else "5.1"
        return [control("Editar", "unknown", "edit", rid, "Edit", "MenuItem")]

    def call(self, tool, args):
        self.order.append("click")
        out = super().call(tool, args)
        self.menu = "Expanded" if self.menu == "Collapsed" else "Collapsed"
        return out

    def menu_state(self, hwnd):
        return self.menu

    def cursor_chain(self):
        return self.diag_rows()

    def diag_rows(self):
        now = self.controls()[0]
        return [{"runtime_id": now["rid"], "automation_id": "Edit", "control_type": "MenuItem",
                 "class_name": "MenuBarItem", "expand_state": self.menu, "offscreen": False},
                {"runtime_id": "5.0", "automation_id": "MenuBar", "control_type": "MenuBar",
                 "class_name": "MenuBar", "expand_state": "", "offscreen": False}]

    def diag_at_element(self, hwnd, label, control_types):
        self.order.append("diag_before")
        if self.diag_error == "raises":
            raise OSError("powershell is gone")
        return None if self.diag_error == "unreadable" else self.diag_rows()

    def diag_at_cursor(self):
        self.order.append("diag_after")
        if self.diag_error == "raises":
            raise OSError("powershell is gone")
        return None if self.diag_error == "unreadable" else self.diag_rows()


SCORING_KEYS = ("success", "failure", "steps", "actions_requested", "delivered",
                "effect_observed", "indeterminate", "rejected", "independent_checks",
                "independent_failed", "engine_false_success", "grounding_wrong", "grounding",
                "identity")


class MenuDiagnosticTest(QuietTest):
    """Fifth round, finding 1 (preparation): a read-only record of what is under the target
    point before and after the click, to tell a menu whose identifiers change on expansion from a
    click that reached another control. It changes neither the scoring nor the identity check."""

    def run_flow(self, **kwargs):
        desk = FakeNotepadRun(self.tmp, **kwargs)
        res = driver.new_result("E2E-03", 1)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, 77, 4242)
        return desk, res

    def test_identifiers_that_change_on_expansion_are_recorded_before_and_after(self):
        _, res = self.run_flow()
        record = res["menu_diagnostic"][0]
        before, after = record["before"], record["after"]
        self.assertEqual((record["step"], len(before), len(after)), (1, 2, 2))
        self.assertEqual(
            before[0],
            {"depth": 0, "control_type": "MenuItem", "automation_id": "Edit",
             "class_name": "MenuBarItem", "expand_state": "Collapsed", "same_runtime_id": True})
        self.assertEqual(
            after[0],
            {"depth": 0, "control_type": "MenuItem", "automation_id": "Edit",
             "class_name": "MenuBarItem", "expand_state": "Expanded", "same_runtime_id": False})
        self.assertEqual([row["same_runtime_id"] for row in after], [False, False])
        self.assertEqual(after[1]["control_type"], "MenuBar")
        self.assertGreaterEqual(record["after_ms"], 0, "the delay of the read is recorded")

    def test_it_is_read_before_the_click_and_after_it_and_sends_nothing(self):
        desk, _ = self.run_flow()
        self.assertEqual(desk.order, ["diag_before", "click", "diag_after"])
        self.assertEqual(desk.clicks, ["Editar"])

    def test_a_menu_whose_identifiers_do_not_change_records_the_same_runtime_id(self):
        _, res = self.run_flow(ids_change=False)
        for step in res["menu_diagnostic"]:
            self.assertTrue(step["before"][0]["same_runtime_id"])
            self.assertTrue(step["after"][0]["same_runtime_id"])
        self.assertEqual([r["after"][0]["expand_state"] for r in res["menu_diagnostic"]],
                         ["Expanded", "Collapsed"])
        self.assertTrue(res["success"])

    def test_the_diagnostic_does_not_change_how_a_run_is_scored(self):
        for kwargs in ({"ids_change": True}, {"ids_change": False}):
            scored = []
            for error in (None, "unreadable", "raises"):
                _, res = self.run_flow(diag_error=error, **kwargs)
                scored.append({k: res[k] for k in SCORING_KEYS})
            self.assertEqual(scored[0], scored[1], kwargs)
            self.assertEqual(scored[0], scored[2], kwargs)

    def test_the_identity_check_is_not_relaxed_when_the_identifiers_change(self):
        desk, res = self.run_flow()
        self.assertEqual(desk.clicks, ["Editar"], "the run ended after the first click")
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertIn("not the element", res["failure"])

    def test_an_unreadable_diagnostic_is_recorded_as_unreadable_not_as_a_failure(self):
        for error in ("unreadable", "raises"):
            _, res = self.run_flow(ids_change=False, diag_error=error)
            self.assertTrue(res["success"], error)
            self.assertEqual([(r["before"], r["after"]) for r in res["menu_diagnostic"]],
                             [(None, None)] * 2, error)

    def test_only_the_notepad_menu_step_has_it(self):
        desk = FakeCalculator(self.tmp)
        desk.flow = driver.single_click_flow("Abrir navegación")
        res = driver.new_result("MODE", 0)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, 77, 4242)
        self.assertNotIn("menu_diagnostic", res)

    def test_the_record_holds_no_name_and_the_scripts_read_none(self):
        _, res = self.run_flow(ids_change=False)
        self.assertNotIn("Editar", json.dumps(res["menu_diagnostic"], ensure_ascii=False))
        for script in (driver.UIA_DIAG_AT_ELEMENT_PS, driver.UIA_DIAG_AT_POINT_PS):
            self.assertNotIn("Current.Name", script)
            self.assertNotIn("ValuePattern", script)
            self.assertNotIn("TextPattern", script)

    def test_diagnostic_rows_are_parsed_and_recorded_without_the_runtime_ids(self):
        rows = driver.parse_diag_rows(
            "5.1\tEdit\tControlType.MenuItem\tMenuBarItem\tCollapsed\n5.0\t\tMenuBar\t\t\nbroken\n")
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0]["control_type"], "MenuItem")
        recorded = driver.diag_record(rows, "5.1")
        self.assertEqual([r["same_runtime_id"] for r in recorded], [True, False])
        self.assertTrue(all("runtime_id" not in r for r in recorded))
        self.assertEqual(len(driver.diag_record(rows * 4, "5.1")), 5, "an element + 4 ancestors")
        self.assertFalse(driver.diag_record(rows, "")[0]["same_runtime_id"])


if __name__ == "__main__":
    unittest.main()
