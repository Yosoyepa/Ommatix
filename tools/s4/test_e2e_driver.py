"""Regression tests for the S4 driver: its activation target, the Calculator clear control, the
Notepad menu checks, the per-round live cap, the action timings, grounding by identity and the
Calculator Standard-mode step. No window is activated here, no UI Automation is read and no
input is sent: the desktop is a fake. Run with:
python -m unittest discover -s tools/s4 -p "test_*.py"
"""

import importlib.util
import json
import shutil
import subprocess
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
        self.pre_chain = [dict(EDITAR_ROW)]  # what is under the target point before the click
        self.under = 77  # the top-level window under the target point
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

    def element_centre(self, hwnd, label, control_types):
        return (10, 20)

    def point_chain(self, x, y):  # the pre-click read
        return self.pre_chain

    def island_walk(self, x, y, host_runtime_id):
        return None

    def window_under(self, point):
        return self.under

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


def fake_signals(mode):
    """What the signals record holds for a Calculator in `mode` (no header name in it)."""
    return {"mode": mode, "readable": mode != "unknown", "header_present": True,
            "header_matched": mode == "standard", "scientific_controls": 0}


class FakeCalculator(driver.Protocol):
    """A Protocol on a fake Calculator: no server, no windows, no UI Automation, no input. It
    records the clicks the engine's gate would have delivered."""

    def __init__(self, tmp, mode="other", std_count=1, pane_opens=True, max_live=4,
                 std_name="Estándar", std_aid="Standard", std_type="ListItem", uia_std=None,
                 pane_delay=0):
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
        self.std_count = std_count  # the engine's elements named like the item
        self.uia_std = std_count if uia_std is None else uia_std  # the items UI Automation sees
        self.std_name, self.std_aid, self.std_type = std_name, std_aid, std_type
        self.pane_delay = pane_delay  # item reads that find nothing after the pane opened
        self.pane_reads = 0
        self.now = 0.0  # a virtual clock: pause() advances it, nothing really sleeps
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
        self.centre_of = None  # the label whose centre the pre-click read asked for
        self.pre_chain = None  # what the point query returns before a click, instead of the target
        self.under = 77  # the top-level window under the target point
        self.point_reads = []

    def controls(self):
        nav = "Cerrar navegación" if self.pane_open else "Abrir navegación"
        found = [control(nav, "button", "nav", "42.1", "TogglePaneButton", "Button")]
        if self.pane_open:
            found += [control(self.std_name, "unknown", f"std{i}", f"42.7.{i}", self.std_aid,
                              self.std_type) for i in range(self.std_count)]
        return found

    def mono(self):
        return self.now

    def pause(self, seconds):
        self.now += seconds

    def standard_item_rows(self, hwnd):
        """What the item read of the pane returns (UI Automation, read-only)."""
        self.pane_reads += 1
        if self.unreadable:
            return None
        if not self.pane_open or self.pane_reads <= self.pane_delay:
            return []
        return [{"runtime_id": f"42.7.{i}", "automation_id": self.std_aid,
                 "control_type": self.std_type, "offscreen": False, "name": self.std_name}
                for i in range(self.uia_std)]

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
            self.pane_reads = 0
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

    def element_centre(self, hwnd, label, control_types):
        self.centre_of = label
        return (10, 20)

    def point_chain(self, x, y):  # the pre-click read: the control about to be clicked
        self.point_reads.append((x, y))
        if self.pre_chain is not None:
            return self.pre_chain
        top = next(c for c in self.controls() if c["label"] == self.centre_of)
        return [{"runtime_id": top["rid"], "automation_id": top["aid"],
                 "control_type": top["ctype"], "class_name": "", "offscreen": False},
                {"runtime_id": "42.0", "automation_id": "", "control_type": "Pane",
                 "class_name": "", "offscreen": False}]

    def island_walk(self, x, y, host_runtime_id):
        return None

    def window_under(self, point):
        return self.under

    def cursor_chain(self):
        if self.chain_override is not None:
            return self.chain_override
        top = self.last
        return [{"runtime_id": top["rid"], "automation_id": top["aid"],
                 "control_type": top["ctype"], "offscreen": False},
                {"runtime_id": "42.0", "automation_id": "", "control_type": "Pane",
                 "offscreen": False}]

    def calc_signals(self, hwnd):
        return fake_signals(self.mode)

    def launch(self):
        self.started.add(4242)
        return 4242

    def wait_foreground(self, pid):
        return True, 0

    def current_hwnd(self):
        return 77

    def window_of(self, pid):
        return None

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


class RunLoopDesk(driver.Protocol):
    """Protocol.run over scripted runs: no desktop, no engine. A script item is the fields of the
    run's result, or an exception run_once raises."""

    def __init__(self, tmp, script, runs=3):
        self.args = SimpleNamespace(first_run=1, runs=runs, pause=0.0,
                                    results=tmp / "results.jsonl")
        self.stop_file = tmp / "stop.flag"
        self.results = []
        self.script = script
        self.started = []  # the runs run_once was called for

    def metrics(self):
        return {"delivered_actions": 0, "rejected_actions": 0}

    def ledger_total(self):
        return 0

    def run_once(self, n):
        self.started.append(n)
        item = self.script[n - 1]
        if isinstance(item, Exception):
            raise item
        res = driver.new_result("E2E-03", n)
        res.update(item)
        return res


class SafetyStopTest(QuietTest):
    """Finding 13 of the tenth round: a safety event ends the whole invocation at once and engages
    the operator's stop, as three consecutive failed runs do; a plain failure does not."""

    def run_loop(self, script, runs=3):
        desk = RunLoopDesk(self.tmp, script, runs)
        try:
            desk.run()
            error = None
        except driver.Abort as raised:
            error = raised
        return desk, error

    def recorded(self):
        text = (self.tmp / "results.jsonl").read_text(encoding="utf-8")
        return [json.loads(line) for line in text.splitlines()]

    def test_a_wrong_grounding_in_run_1_of_3_sends_nothing_more_and_engages_the_stop(self):
        desk, error = self.run_loop([{"grounding_wrong": 1}, {"success": True}, {"success": True}])
        self.assertIsNotNone(error)
        self.assertEqual(desk.started, [1], "runs 2 and 3 never started")
        self.assertTrue(desk.stop_file.exists(), "the operator's stop is engaged")
        self.assertEqual(len(desk.results), 1)
        self.assertEqual(self.recorded()[0]["safety_stop"], "grounding_wrong")

    def test_a_false_success_ends_the_invocation_the_same_way(self):
        desk, error = self.run_loop([{"engine_false_success": 1}, {"success": True}])
        self.assertIsNotNone(error)
        self.assertEqual(desk.started, [1])
        self.assertTrue(desk.stop_file.exists())
        self.assertEqual(self.recorded()[0]["safety_stop"], "engine_false_success")
        self.assertIn("engine_false_success", str(error))

    def test_the_safety_event_wins_even_when_the_run_reports_success(self):
        desk, error = self.run_loop([{"success": True, "grounding_wrong": 1}, {"success": True}])
        self.assertEqual(desk.started, [1])
        self.assertTrue(desk.stop_file.exists())

    def test_an_abort_ends_the_invocation_and_engages_the_stop(self):
        desk, error = self.run_loop([driver.Abort("observe_screen failed"), {"success": True}])
        self.assertEqual(str(error), "observe_screen failed")
        self.assertEqual(desk.started, [1])
        self.assertTrue(desk.stop_file.exists(), "an abort is a safety event too")

    def test_an_abort_that_already_engaged_the_stop_does_it_once(self):
        desk = RunLoopDesk(self.tmp, [{}])
        writes = []
        original = driver.Protocol.write_stop
        desk.write_stop = lambda reason: (writes.append(reason), original(desk, reason))[1]
        desk.run_once = lambda n: desk.engage_stop("POLICY_DENIED during the protocol")
        with self.assertRaises(driver.Abort):
            desk.run()
        self.assertEqual(writes, ["POLICY_DENIED during the protocol"])

    def test_a_plain_failure_still_lets_the_next_run_start(self):
        desk, error = self.run_loop([{"failure": "initial state not visible by OCR"},
                                     {"success": True}, {"success": True}])
        self.assertIsNone(error)
        self.assertEqual(desk.started, [1, 2, 3])
        self.assertFalse(desk.stop_file.exists())
        self.assertNotIn("safety_stop", self.recorded()[0])

    def test_three_consecutive_plain_failures_still_engage_the_stop(self):
        desk, error = self.run_loop([{"failure": "x"}] * 3 + [{"success": True}], runs=4)
        self.assertEqual(str(error), "three consecutive failed runs")
        self.assertEqual(desk.started, [1, 2, 3])
        self.assertTrue(desk.stop_file.exists())

    def test_a_run_without_a_stop_file_configured_still_ends(self):
        desk = RunLoopDesk(self.tmp, [{"grounding_wrong": 1}, {"success": True}])
        desk.stop_file = None
        with self.assertRaises(driver.Abort):
            desk.run()
        self.assertEqual(desk.started, [1])

    def test_the_read_only_check_never_writes_the_stop(self):
        desk = RunLoopDesk(self.tmp, [{"grounding_wrong": 1}])
        desk.read_only = True
        with self.assertRaises(driver.Abort):
            desk.run()
        self.assertFalse(desk.stop_file.exists())


def pane_row(rid="42.0"):
    return {"runtime_id": rid, "automation_id": "", "control_type": "Pane", "class_name": "",
            "offscreen": False}


class PreClickGroundingTest(QuietTest):
    """ADR-0022, rule 1: immediately before the request, the element at the target point is the
    identified one (or has it as parent) in the expected window, in every flow."""

    def run_steps(self, desk, flow=None):
        desk.flow = flow or driver.single_click_flow("Abrir navegación")
        res = driver.new_result("E2E-02", 1)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, 77, 4242)
        return res

    def test_the_point_is_read_and_recorded_before_the_request(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        pre = res["pre_grounded"][0]
        self.assertEqual((pre["step"], pre["attempt"], pre["grounded"], pre["status"]),
                         (1, 1, True, "ok"))
        self.assertEqual((pre["window_ok"], pre["identity_hit"]), (True, True))
        self.assertEqual([r["is_identified"] for r in pre["chain"]], [True, False])
        self.assertEqual(desk.point_reads, [(10, 20)])

    def test_the_element_at_the_point_may_be_a_child_of_the_identified_one_no_deeper(self):
        # identity_hit's rule is unchanged: the element or its PARENT, not an ancestor above.
        desk = FakeCalculator(self.tmp)
        target = {"runtime_id": "42.1", "automation_id": "TogglePaneButton",
                  "control_type": "Button", "class_name": "", "offscreen": False}
        child = {"runtime_id": "42.1.1", "automation_id": "", "control_type": "Text",
                 "class_name": "", "offscreen": False}
        desk.pre_chain = [child, target, pane_row()]
        res = self.run_steps(desk)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertEqual([r["is_identified"] for r in res["pre_grounded"][0]["chain"]],
                         [False, True, False])
        other = FakeCalculator(self.tmp)
        other.pre_chain = [child, pane_row(), target]
        res = self.run_steps(other)
        self.assertEqual((other.clicks, res["actions_requested"]), ([], 0))
        self.assertEqual(res["pre_grounded"][0]["status"], "identity_mismatch")

    def test_another_element_at_the_point_sends_no_request(self):
        for name, chain in (("another control", [pane_row("99.1")]), ("nothing", []),
                            ("same AutomationId, other RuntimeId", [
                                {"runtime_id": "99.1", "automation_id": "TogglePaneButton",
                                 "control_type": "Button", "class_name": "", "offscreen": False}])):
            desk = FakeCalculator(self.tmp)
            desk.pre_chain = chain
            res = self.run_steps(desk)
            self.assertEqual((desk.clicks, desk.ledger_total(), res["actions_requested"]),
                             ([], 0, 0), name)
            self.assertEqual(res["pre_grounded"][0]["status"], "identity_mismatch", name)
            self.assertIn("not grounded", res["failure"], name)
            self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (0, 0), name)

    def test_another_window_at_the_point_sends_no_request(self):
        desk = FakeCalculator(self.tmp)
        desk.under = 5  # the desktop, the taskbar or another window
        res = self.run_steps(desk)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual((res["pre_grounded"][0]["status"], res["pre_grounded"][0]["window_ok"]),
                         ("window_mismatch", False))
        no_window = FakeCalculator(self.tmp)
        no_window.under = None
        self.run_steps(no_window)
        self.assertEqual(no_window.clicks, [])

    def test_an_unreadable_point_or_a_failed_read_sends_no_request(self):
        desk = FakeCalculator(self.tmp)
        desk.element_centre = lambda hwnd, label, types: None
        res = self.run_steps(desk)
        self.assertEqual((desk.clicks, res["pre_grounded"][0]["status"]), ([], "no_point"))
        for error in (OSError("gone"), timeout_error()):
            broken = FakeCalculator(self.tmp)

            def read(x, y, error=error):
                raise error

            broken.point_chain = read
            res = self.run_steps(broken)
            self.assertEqual((broken.clicks, res["pre_grounded"][0]["status"]),
                             ([], "unreadable"))

    def test_the_grounding_runs_again_before_a_retry_after_a_stale_snapshot(self):
        desk = FakeCalculator(self.tmp)
        original = desk.call
        calls = []

        def call(tool, args):
            if not calls:
                calls.append(1)
                return ({"status": "rejected", "input_delivered": False,
                         "error": {"code": "STALE_SNAPSHOT"}}, False, {}, 5.0)
            return original(tool, args)

        desk.call = call
        res = self.run_steps(desk)
        self.assertEqual([p["attempt"] for p in res["pre_grounded"]], [1, 2])
        self.assertEqual(len(desk.point_reads), 2)

    def test_a_calc_standard_navigation_click_at_a_wrong_point_is_not_sent(self):
        desk = FakeCalculator(self.tmp)
        desk.pre_chain = [pane_row("99.1")]
        summary = desk.calc_standard()
        self.assertEqual((desk.clicks, summary["switched"]), ([], False))
        self.assertIn("not grounded", summary["failure"])

    def test_the_restore_click_is_grounded_at_the_point_first_too(self):
        for setup, status in (({"pre_chain": [pane_row("99.1")]}, "identity_mismatch"),
                              ({"under": 5}, "window_mismatch")):
            proto = FakeNotepad(["Expanded"])
            for key, value in setup.items():
                setattr(proto, key, value)
            res = {"run": 1, "observations": []}
            closed = proto.restore_menu(res, 1)
            self.assertFalse(closed)
            self.assertEqual(proto.clicks, [], status)
            self.assertEqual(res["menu_restore"]["pre_grounded"]["status"], status)
            self.assertFalse(res["menu_restore"]["clicked"])
        proto = FakeNotepad(["Expanded", "Expanded"])
        res = {"run": 1, "observations": []}
        self.assertTrue(proto.restore_menu(res, 1))
        self.assertEqual(res["menu_restore"]["pre_grounded"]["status"], "ok")

    def test_the_records_hold_no_name_and_no_runtime_id(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk)
        text = json.dumps(res["pre_grounded"], ensure_ascii=False)
        for secret in ("Abrir navegación", "42.1", "42.0"):
            self.assertNotIn(secret, text)
        self.assertNotIn("runtime_id", text)


class NavigatingStepTest(QuietTest):
    """ADR-0022, rule 2, at the level of the generic steps(): a step on the closed list records
    its identity after the click but is decided by its OS-side check."""

    def run_steps(self, desk, os_ok, navigating=(1,), two_steps=False):
        flow = driver.single_click_flow("Abrir navegación")
        flow["steps"] = [("Abrir navegación", driver.NOT_BY_OCR, "expected")]
        if two_steps:
            flow["steps"].append(("Cerrar navegación", driver.NOT_BY_OCR, "expected"))
        flow["navigating"] = navigating
        desk.flow = flow
        desk.os_check = lambda hwnd, expected: os_ok
        res = driver.new_result("E2E-02", 1)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, 77, 4242)
        return res

    def test_a_recreated_control_with_a_passing_os_check_is_grounded(self):
        desk = FakeCalculator(self.tmp)
        desk.chain_override = [pane_row("99.1")]  # not the identified control any more
        res = self.run_steps(desk, os_ok=True)
        self.assertTrue(res["success"], res)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (0, 0))
        post = res["post_grounded"][0]
        self.assertEqual((post["identity_hit"], post["os_check"]), (False, True))
        self.assertEqual([r["is_identified"] for r in post["chain"]], [False])
        self.assertNotIn("grounding", res)

    def test_a_failing_os_check_is_grounding_wrong_and_a_false_success_and_stops(self):
        desk = FakeCalculator(self.tmp)
        res = self.run_steps(desk, os_ok=False, navigating=(1, 2), two_steps=True)
        self.assertFalse(res["success"])
        self.assertEqual(desk.clicks, ["Abrir navegación"], "no further request")
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertIs(res["post_grounded"][0]["identity_hit"], True)
        self.assertIs(res["post_grounded"][0]["os_check"], False)
        self.assertIn("navigating click", res["failure"])

    def test_the_false_success_of_a_navigating_step_needs_effect_observed(self):
        desk = FakeCalculator(self.tmp)
        desk.statuses = ["indeterminate"]
        res = self.run_steps(desk, os_ok=False)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 0))

    def test_a_step_off_the_list_keeps_the_identity_check_after_the_click(self):
        desk = FakeCalculator(self.tmp)
        desk.chain_override = [pane_row("99.1")]
        res = self.run_steps(desk, os_ok=True, navigating=())
        self.assertFalse(res["success"])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertEqual(res["grounding"], [False])
        self.assertNotIn("post_grounded", res)
        self.assertIn("not the element", res["failure"])

    def test_no_name_is_compared_after_a_navigating_click(self):
        # A same-named control with another identity does not pass a failing OS check either.
        desk = FakeCalculator(self.tmp)
        desk.chain_override = [pane_row("99.1")]
        res = self.run_steps(desk, os_ok=False)
        self.assertEqual((res["success"], res["grounding_wrong"]), (False, 1))

    def test_a_click_that_was_not_delivered_has_no_post_grounding_and_no_os_verdict(self):
        desk = FakeCalculator(self.tmp)
        desk.statuses = ["rejected"]
        res = self.run_steps(desk, os_ok=True)
        self.assertNotIn("post_grounded", res)
        self.assertEqual(res["grounding_wrong"], 0)
        self.assertIn("rejected", res["failure"])


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
        self.assertEqual(summary["standard_item"]["stopped_by"], "several_candidates")
        self.assertIn("several_candidates", summary["failure"])

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
        desk.calc_signals = lambda hwnd: fake_signals("other")
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


class StandardItemPollTest(QuietTest):
    """Seventh round, finding C1: after the navigation click the client found no element named
    exactly «Estándar» about 1.5 s later and the record could not say why. The client now polls
    UI Automation (read-only) for the pane's item by its AutomationId `Standard`, in a closed
    list of list-item-like control types, for a bounded time, and accepts its name only from a
    closed set. Zero, several, an unknown name or a timeout means no click."""

    def run_step(self, **kwargs):
        desk = FakeCalculator(self.tmp, **kwargs)
        return desk, desk.calc_standard()

    def test_a_pane_that_appears_late_within_the_window_is_found(self):
        desk, summary = self.run_step(pane_delay=9)
        item = summary["standard_item"]
        self.assertEqual(desk.clicks, ["Abrir navegación", "Estándar"])
        self.assertTrue(summary["switched"])
        self.assertEqual(item["status"], "ok")
        self.assertEqual(item["polls"], 10)
        self.assertGreater(item["waited_ms"], 0)
        self.assertLess(item["waited_ms"], driver.STANDARD_POLL_WINDOW_S * 1000)

    def test_the_item_is_found_by_its_automation_id_whatever_its_name_is(self):
        for name in ("Calculadora Estándar", "Calculadora estándar", "Texto del propietario 4711"):
            desk, summary = self.run_step(std_name=name)
            self.assertEqual(desk.clicks, ["Abrir navegación", name], name)
            self.assertEqual([c["grounded"] for c in summary["clicks"]], [True, True])
            self.assertTrue(summary["switched"])
            item = summary["standard_item"]
            self.assertEqual((item["control_type"], item["automation_id"]), ("ListItem", "Standard"))
            self.assertEqual([i["status"] for i in summary["identity"]], ["ok", "ok"])

    def test_the_item_name_is_used_in_memory_only_and_never_written_or_logged(self):
        secret = "Calculadora Estándar 4711"
        for kwargs in ({}, {"std_aid": "Otro"}, {"uia_std": 2}):
            self.setUp()  # fresh log mock and results file
            desk, summary = self.run_step(std_name=secret, **kwargs)
            written = (self.tmp / "results.jsonl").read_text(encoding="utf-8")
            logged = json.dumps(driver.log.call_args_list, ensure_ascii=False, default=str)
            for text in (json.dumps(summary, ensure_ascii=False), written, logged):
                self.assertNotIn(secret, text, kwargs)
            self.assertNotIn("name", summary["standard_item"], kwargs)
            self.assertNotIn("name_accepted", summary["standard_item"], kwargs)

    def test_the_name_list_is_gone(self):
        for gone in ("CALC_STANDARD_NAMES", "accepted_standard_name"):
            self.assertFalse(hasattr(driver, gone), gone)

    def test_an_item_with_no_runtime_id_means_no_click(self):
        desk = FakeCalculator(self.tmp)
        original = desk.standard_item_rows
        desk.standard_item_rows = lambda hwnd: [dict(r, runtime_id="") for r in original(hwnd)]
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertEqual(summary["standard_item"]["stopped_by"], "unidentifiable")

    def test_an_engine_element_that_is_not_unique_means_no_click(self):
        desk, summary = self.run_step(std_count=2, uia_std=1)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertIn("ambiguous label", summary["failure"])

    def test_two_items_with_the_automation_id_mean_no_click(self):
        desk, summary = self.run_step(uia_std=2)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertEqual(summary["standard_item"]["stopped_by"], "several_candidates")
        self.assertEqual(summary["standard_item"]["candidates"], 2)
        self.assertEqual(desk.closed, [4242])

    def test_a_timeout_means_no_click_and_is_recorded(self):
        desk, summary = self.run_step(pane_delay=10_000)
        item = summary["standard_item"]
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertFalse(summary["switched"])
        self.assertEqual((item["status"], item["stopped_by"]), ("not_found", "timeout"))
        self.assertGreaterEqual(item["waited_ms"], driver.STANDARD_POLL_WINDOW_S * 1000)
        self.assertLess(item["waited_ms"], (driver.STANDARD_POLL_WINDOW_S + 1) * 1000)
        self.assertEqual(driver.STANDARD_POLL_WINDOW_S, 3.0)
        self.assertEqual(driver.STANDARD_POLL_STEP_S, 0.15)
        self.assertEqual(desk.ledger_total(), 1, "only the navigation click was spent")

    def test_an_item_under_another_automation_id_times_out_and_the_record_says_so(self):
        desk, summary = self.run_step(std_aid="Otro")
        item = summary["standard_item"]
        self.assertEqual(desk.clicks, ["Abrir navegación"], "never found by name alone")
        self.assertEqual((item["status"], item["stopped_by"]), ("not_found", "timeout"))
        self.assertNotIn("Estándar", json.dumps(item, ensure_ascii=False))

    def test_an_unreadable_pane_is_polled_until_the_timeout_and_never_clicked(self):
        desk = FakeCalculator(self.tmp)
        original = desk.standard_item_rows
        desk.standard_item_rows = lambda hwnd: None if desk.pane_open else original(hwnd)
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertEqual(summary["standard_item"]["status"], "unreadable")
        self.assertEqual(summary["standard_item"]["stopped_by"], "timeout")

    def test_an_item_that_the_engine_does_not_see_stops_at_the_engine_side_record(self):
        desk, summary = self.run_step(std_count=0, uia_std=1)
        self.assertEqual(desk.clicks, ["Abrir navegación"])
        self.assertEqual(summary["standard_item"]["status"], "ok")
        self.assertEqual(summary["targets"][-1]["stopped_by"], "zero_candidates")

    def test_the_click_must_reach_the_item_that_was_polled(self):
        desk = FakeCalculator(self.tmp)
        original = desk.identify

        def identify(hwnd, label, control_types):
            rows = original(hwnd, label, control_types)
            return [dict(r, runtime_id="9.9") if r["automation_id"] == "Standard" else r
                    for r in rows]

        desk.identify = identify
        summary = desk.calc_standard()
        self.assertEqual(desk.clicks, ["Abrir navegación"], "another element has the same name")
        self.assertFalse(summary["switched"])
        self.assertEqual(summary["identity"][-1]["status"], "not_the_polled_item")

    def test_the_polled_identity_does_not_leak_into_later_clicks(self):
        desk, _ = self.run_step()
        self.assertIsNone(desk.pinned_runtime_id)

    def test_the_mode_after_is_recorded_when_the_step_stops(self):
        for kwargs in ({"pane_delay": 10_000}, {"uia_std": 2},
                       {"pane_opens": False}, {"std_count": 0, "uia_std": 1}):
            _, summary = self.run_step(**kwargs)
            self.assertEqual(summary["mode_after"], "other", kwargs)

    def test_the_mode_after_is_read_even_when_the_mode_before_was_unreadable(self):
        _, summary = self.run_step(mode="unknown")
        self.assertEqual((summary["mode_before"], summary["mode_after"]), ("unknown", "unknown"))

    def test_the_mode_is_read_before_the_calculator_is_closed(self):
        desk = FakeCalculator(self.tmp, pane_delay=10_000)
        order = []
        read = desk.calc_signals
        desk.calc_signals = lambda hwnd: (order.append("mode"), read(hwnd))[1]
        close = desk.close
        desk.close = lambda pid: (order.append("close"), close(pid))[1]
        desk.calc_standard()
        self.assertEqual(order[-2:], ["mode", "close"])

    def test_the_foreground_wait_is_in_the_summary(self):
        desk, summary = self.run_step()
        self.assertIsInstance(summary["foreground_wait_s"], float)
        self.assertEqual((summary["activation_requests"], summary["window_placements"]), (0, 0))
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertIn("foreground_wait_s", record)

    def test_the_foreground_wait_is_recorded_when_the_window_never_arrives(self):
        desk = FakeCalculator(self.tmp)
        desk.wait_foreground = lambda pid: (False, 3)
        summary = desk.calc_standard()
        self.assertEqual(summary["activation_requests"], 3)
        self.assertIsInstance(summary["foreground_wait_s"], float)
        self.assertEqual(summary["mode_after"], "unknown", "no window: nothing to read")
        self.assertEqual(desk.clicks, [])


class CalcStandardRecordTest(QuietTest):
    """Ninth round, findings 2, 3 and 8: the summary says what the run path counted, the mode
    after the last click is polled instead of read once, and the signals are recorded."""

    def run_step(self, **kwargs):
        desk = FakeCalculator(self.tmp, **kwargs)
        return desk, desk.calc_standard()

    def test_the_summary_prints_the_grounding_and_false_success_counters(self):
        _, ok = self.run_step()
        self.assertEqual((ok["grounding_wrong"], ok["engine_false_success"]), (0, 0))
        desk = FakeCalculator(self.tmp)
        original = desk.cursor_chain

        def wrong_second_click():
            if desk.last["key"].startswith("std"):
                return [{"runtime_id": "7.7", "automation_id": "Other", "control_type": "Button",
                         "offscreen": False}]
            return original()

        desk.cursor_chain = wrong_second_click
        summary = desk.calc_standard()
        self.assertEqual([c["grounded"] for c in summary["clicks"]], [True, False])
        self.assertEqual((summary["grounding_wrong"], summary["engine_false_success"]), (1, 1))
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8").splitlines()[-1])
        self.assertEqual((record["grounding_wrong"], record["engine_false_success"]), (1, 1))

    def test_a_stop_after_a_delivered_click_polls_the_mode_until_it_settles(self):
        desk = FakeCalculator(self.tmp)
        reads = []
        modes = iter(["other", "other", "other", "standard"])  # before, then three after

        def signals(hwnd):
            mode = next(modes)
            reads.append((mode, desk.now))
            return fake_signals(mode)

        desk.calc_signals = signals
        original = desk.cursor_chain
        desk.cursor_chain = lambda: ([{"runtime_id": "7.7", "automation_id": "Other",
                                       "control_type": "Button", "offscreen": False}]
                                     if desk.last["key"].startswith("std") else original())
        summary = desk.calc_standard()  # the second click is delivered, not grounded: it stops
        self.assertIn("calc-standard stops", summary["failure"])
        self.assertEqual(summary["mode_after"], "standard", "the late mode is seen")
        self.assertEqual(summary["mode_after_reads"], 3)
        self.assertEqual(len(reads), 1 + summary["mode_after_reads"])
        self.assertEqual(summary["mode_signals_after"]["mode"], "standard")

    def test_the_poll_after_the_clicks_is_bounded(self):
        desk = FakeCalculator(self.tmp)
        desk.calc_signals = lambda hwnd: fake_signals("other")
        summary = desk.calc_standard()
        window = driver.CALC_MODE_POLL_S
        self.assertEqual(summary["mode_after"], "other")
        self.assertLessEqual(window, 3.0)
        self.assertLessEqual(desk.now, window + driver.CALC_MODE_POLL_STEP_S)
        self.assertEqual(summary["mode_after_reads"],
                         int(window / driver.CALC_MODE_POLL_STEP_S) + 1)

    def test_a_stop_before_any_click_reads_the_mode_once(self):
        desk, summary = self.run_step(mode="unknown")
        self.assertEqual((summary["mode_after"], summary["mode_after_reads"]), ("unknown", 1))
        self.assertEqual(desk.now, 0.0, "nothing to wait for")

    def test_the_mode_signals_are_recorded_before_and_after(self):
        _, summary = self.run_step()
        self.assertEqual(summary["mode_signals_before"]["mode"], "other")
        self.assertEqual(summary["mode_signals_after"]["mode"], "standard")
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertIn("mode_signals_before", record)


class CalcCheckModeTest(QuietTest):
    """Ninth round, finding 9: a read-only mode check that cannot reach the run path, and a
    --calc-standard that refuses a zero budget instead of engaging the operator's stop."""

    def desk(self, **kwargs):
        desk = FakeCalculator(self.tmp, **kwargs)
        desk.stop_file = self.tmp / "stop.flag"
        desk.args.settle = 0.0
        desk.spend_live = mock.Mock(side_effect=AssertionError("spend_live reached"))
        desk.call = mock.Mock(side_effect=AssertionError("the engine was called"))
        desk.observe = mock.Mock(side_effect=AssertionError("the engine was observed"))
        return desk

    def test_a_standard_calculator_answers_standard_and_is_closed(self):
        desk = self.desk(mode="standard")
        summary = desk.check_mode()
        self.assertTrue(summary["standard"])
        self.assertEqual((summary["mode"], summary["closed"], desk.closed), ("standard", True, [4242]))
        self.assertEqual(desk.clicks, [])

    def test_another_mode_is_not_standard_and_nothing_is_clicked(self):
        desk = self.desk(mode="other")
        summary = desk.check_mode()
        self.assertFalse(summary["standard"])
        self.assertEqual(summary["mode"], "other")
        self.assertEqual(summary["mode_reads"], int(driver.CALC_MODE_POLL_S
                                                    / driver.CALC_MODE_POLL_STEP_S) + 1)
        self.assertEqual((desk.clicks, desk.closed), ([], [4242]))

    def test_it_never_creates_the_stop_file_nor_touches_the_ledger(self):
        for mode in ("standard", "other", "unknown"):
            desk = self.desk(mode=mode)
            desk.check_mode()
            self.assertFalse(desk.stop_file.exists(), mode)
            self.assertFalse(desk.ledger.exists(), mode)
        desk = self.desk()
        desk.wait_foreground = lambda pid: (False, 3)
        summary = desk.check_mode()
        self.assertIn("foreground", summary["failure"])
        self.assertFalse(desk.stop_file.exists(), "even when the window never arrives")
        self.assertEqual(desk.closed, [4242])

    def test_the_stop_cannot_be_engaged_while_the_check_reads(self):
        desk = self.desk()
        desk.read_only = True
        with self.assertRaises(driver.Abort):
            desk.engage_stop("a Calculator the protocol did not start is running")
        self.assertFalse(desk.stop_file.exists())
        with self.assertRaises(driver.Abort):
            desk.spend_live = driver.Protocol.spend_live.__get__(desk)
            desk.spend_live()
        self.assertFalse(desk.ledger.exists())

    def test_a_calculator_the_check_did_not_start_stops_it_without_the_stop_file(self):
        desk = self.desk()
        desk.launch = lambda: driver.Protocol.launch(desk)
        desk.flow = driver.calculator_flow()
        with mock.patch.object(driver, "pids_of", return_value={99}):
            summary = desk.check_mode()
        self.assertIn("did not start", summary["failure"])
        self.assertFalse(desk.stop_file.exists())
        self.assertFalse(summary["standard"])

    def test_the_summary_is_written_without_screen_text(self):
        self.desk(mode="standard").check_mode()
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertEqual(record["step"], "calc_check_mode")
        self.assertNotIn("header_name", record["mode_signals"], "the fake's header is not matched")

    def test_the_check_is_a_calculator_only_invocation_of_its_own(self):
        def args(**kw):
            base = dict(app="calculator", calc_standard=False, check_mode=True, max_live=0)
            base.update(kw)
            return SimpleNamespace(**base)

        self.assertIsNone(driver.argument_problem(args(), live=False))
        self.assertIsNone(driver.argument_problem(args(), live=True))
        self.assertIn("calculator", driver.argument_problem(args(app="notepad"), live=True))
        self.assertIn("separate", driver.argument_problem(args(calc_standard=True), live=True))

    def test_calc_standard_with_no_live_budget_is_refused_when_the_arguments_are_parsed(self):
        def args(**kw):
            base = dict(app="calculator", calc_standard=True, check_mode=False, max_live=2)
            base.update(kw)
            return SimpleNamespace(**base)

        self.assertIsNone(driver.argument_problem(args(), live=True))
        self.assertIsNone(driver.argument_problem(args(max_live=1), live=True))
        self.assertIn("--max-live", driver.argument_problem(args(max_live=0), live=True))
        self.assertIn("run mode", driver.argument_problem(args(), live=False))
        self.assertIn("calculator", driver.argument_problem(args(app="testapp"), live=True))
        self.assertIsNone(driver.argument_problem(args(calc_standard=False, max_live=0), live=True))


class FlowIdTest(QuietTest):
    """Owner decision 2026-10-14: the reduced flows are S4R-02 (Calculator) and S4R-03 (Notepad);
    the dossier's E2E-02 and E2E-03 are other flows. The records keep `e2e` and add `flow_id`."""

    def test_the_reduced_flows_have_their_own_identifiers(self):
        self.assertEqual(driver.FLOW_ID, {"testapp": "E2E-01", "calculator": "S4R-02",
                                          "notepad": "S4R-03"})
        self.assertEqual(driver.E2E_ID["calculator"], "E2E-02", "recorded evidence refers to it")

    def test_a_run_record_carries_both(self):
        res = driver.new_result("E2E-03", 4, "S4R-03")
        self.assertEqual((res["e2e"], res["flow_id"]), ("E2E-03", "S4R-03"))
        self.assertIsNone(driver.new_result("MODE", 0)["flow_id"])

    def test_the_standard_mode_step_belongs_to_s4r_02(self):
        summary = FakeCalculator(self.tmp).calc_standard()
        self.assertEqual(summary["flow_id"], "S4R-02")
        record = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertEqual(record["flow_id"], "S4R-02")


class StandardItemRuleTest(unittest.TestCase):
    """The pure rule: which UI Automation rows are the Standard item."""

    @staticmethod
    def row(**kw):
        return {"runtime_id": "1.2", "automation_id": "Standard", "control_type": "ListItem",
                "offscreen": False, "name": "Estándar", **kw}

    def test_exactly_one_on_screen_item_is_ok_whatever_its_name_is(self):
        for name in ("Estándar", "Calculadora estándar", "Calculadora Estándar", "estándar", ""):
            status, found = driver.standard_item_decision([self.row(name=name)])
            self.assertEqual((status, found["runtime_id"]), ("ok", "1.2"), name)

    def test_the_name_is_returned_in_nfc_for_the_exact_match_with_the_engine_label(self):
        decomposed = "Estándar"
        status, found = driver.standard_item_decision([self.row(name=decomposed)])
        self.assertEqual((status, found["name"]), ("ok", "Estándar"))

    def test_only_the_closed_list_of_control_types_counts(self):
        for ctype in ("ListItem", "TreeItem", "TabItem", "MenuItem", "RadioButton"):
            self.assertEqual(driver.standard_item_decision([self.row(control_type=ctype)])[0], "ok")
        for ctype in ("Button", "Text", "Pane", "Custom", "Edit"):
            self.assertEqual(driver.standard_item_decision([self.row(control_type=ctype)])[0],
                             "not_found", ctype)

    def test_another_automation_id_is_not_the_item(self):
        rows = [self.row(automation_id="Scientific"), self.row(automation_id="")]
        self.assertEqual(driver.standard_item_decision(rows)[0], "not_found")

    def test_off_screen_items_are_ignored(self):
        status, found = driver.standard_item_decision([self.row(), self.row(offscreen=True)])
        self.assertEqual((status, found["runtime_id"]), ("ok", "1.2"))
        self.assertEqual(driver.standard_item_decision([self.row(offscreen=True)])[0], "not_found")

    def test_several_on_screen_items_and_unreadable_reads_decide_nothing(self):
        self.assertEqual(driver.standard_item_decision([self.row(), self.row(runtime_id="1.3")])[0],
                         "ambiguous")
        self.assertEqual(driver.standard_item_decision(None)[0], "unreadable")
        self.assertEqual(driver.standard_item_decision([])[0], "not_found")

    def test_an_item_with_no_identifier_is_not_clickable(self):
        status, _ = driver.standard_item_decision([self.row(runtime_id="")])
        self.assertEqual(status, "unidentifiable")

    def test_the_record_never_holds_a_name(self):
        for name in ("Privado", "Estándar"):
            record = driver.standard_item_record("ok", [self.row(name=name)], 3, 0.45)
            self.assertNotIn(name, json.dumps(record, ensure_ascii=False))
            self.assertNotIn("name", record)
            self.assertNotIn("name_accepted", record)
            self.assertEqual((record["control_type"], record["automation_id"]),
                             ("ListItem", "Standard"))

    def test_the_rows_are_parsed_strictly(self):
        import base64
        name = base64.b64encode("Calculadora estándar".encode("utf-8")).decode("ascii")
        rows = driver.parse_standard_item_rows(
            f"42.7\tStandard\tListItem\t0\t{name}\nbad row\n42.8\tStandard\tListItem\t1\t!!!")
        self.assertEqual(rows[0], {"runtime_id": "42.7", "automation_id": "Standard",
                                   "control_type": "ListItem", "offscreen": False,
                                   "name": "Calculadora estándar"})
        self.assertEqual(len(rows), 2, "the unreadable name stays a row, with no name")
        self.assertEqual(rows[1]["name"], "")
        self.assertEqual(driver.parse_standard_item_rows(""), [])


class Utf8LogTest(unittest.TestCase):
    """Seventh round, finding C3: redirected to a file, stdout was written in the console code
    page and the `calc_standard` line held a non-UTF-8 byte (0xF3 for `ó`)."""

    def test_the_log_is_utf8_whatever_the_console_code_page_is(self):
        import io
        raw = io.BytesIO()
        stream = io.TextIOWrapper(raw, encoding="cp1252", write_through=True)
        with mock.patch.object(driver.sys, "stdout", stream):
            driver.use_utf8_stdout()
            driver.log("calc_standard", failure="Estándar no encontrado")
            stream.flush()
        data = raw.getvalue()
        self.assertIn("Estándar".encode("utf-8"), data)
        self.assertNotIn("Estándar".encode("cp1252"), data)
        self.assertEqual(json.loads(data.decode("utf-8"))["failure"], "Estándar no encontrado")

    def test_a_stream_that_cannot_be_reconfigured_is_left_alone(self):
        import io
        with mock.patch.object(driver.sys, "stdout", io.StringIO()):
            driver.use_utf8_stdout()  # a stream with no reconfigure(): nothing to do


class CalcModeTest(unittest.TestCase):
    def test_standard_needs_the_header_and_no_scientific_control(self):
        self.assertEqual(driver.calc_mode("Estándar", 0), "standard")
        self.assertEqual(driver.calc_mode("  Estándar ", 0), "standard")
        self.assertEqual(driver.calc_mode("Estándar", 2), "unknown", "the signals disagree")

    def test_a_header_that_contains_estandar_is_standard_whatever_surrounds_it(self):
        # Ninth round, finding 8: the owner switched the Calculator to Standard by hand and the
        # exact-name list still read `other`.
        for name in ("Calculadora Estándar", "ESTÁNDAR", "Modo estándar de la calculadora",
                     "Standard", "Standard Calculator", "Estándar"):  # NFD accent
            self.assertEqual(driver.calc_mode(name, 0), "standard", name)
            self.assertEqual(driver.calc_mode(name, 1), "unknown", f"{name}: signals disagree")

    def test_another_header_is_another_mode_and_no_header_is_unknown(self):
        self.assertEqual(driver.calc_mode("Científica", 2), "other")
        self.assertEqual(driver.calc_mode("Científica", 0), "other")
        self.assertEqual(driver.calc_mode("Programador", 0), "other")
        self.assertEqual(driver.calc_mode(None, 0), "unknown")
        self.assertEqual(driver.calc_mode("", 0), "unknown")

    def test_the_mode_read_is_parsed_strictly(self):
        self.assertEqual(driver.parse_calc_mode("1\tEstándar\t0\tText\t1"),
                         {"header": "Estándar", "scientific_controls": 0,
                          "control_type": "Text", "headers": 1})
        self.assertEqual(driver.parse_calc_mode("0\t\t3\t\t0"),
                         {"header": None, "scientific_controls": 3, "control_type": None,
                          "headers": 0})
        for bad in ("", "1\tEstándar", "1\tEstándar\t0", "2\tx\t0\tText\t1",
                    "1\tx\tmany\tText\t1", "1\tx\t0\tText\tmany", "1\tx\t0\tText\t1\textra"):
            self.assertIsNone(driver.parse_calc_mode(bad), bad)

    def test_the_header_name_is_recorded_only_when_it_matches_the_closed_rule(self):
        matched = driver.calc_signals(driver.parse_calc_mode("1\tCalculadora Estándar\t0\tText\t1"))
        self.assertEqual((matched["mode"], matched["header_matched"]), ("standard", True))
        self.assertEqual(matched["header_name"], "Calculadora Estándar")
        self.assertEqual((matched["header_automation_id"], matched["header_control_type"],
                          matched["headers"], matched["scientific_controls"]),
                         ("Header", "Text", 1, 0))
        other = driver.calc_signals(driver.parse_calc_mode("1\tMi nombre privado\t0\tText\t2"))
        self.assertEqual((other["mode"], other["header_matched"]), ("other", False))
        self.assertNotIn("header_name", other)
        self.assertNotIn("privado", json.dumps(other, ensure_ascii=False))
        self.assertEqual((other["header_control_type"], other["header_automation_id"],
                          other["headers"], other["header_name_chars"]),
                         ("Text", "Header", 2, len("Mi nombre privado")))

    def test_signals_that_disagree_or_are_missing_are_unknown_and_say_why(self):
        disagree = driver.calc_signals(driver.parse_calc_mode("1\tEstándar\t2\tText\t1"))
        self.assertEqual((disagree["mode"], disagree["header_matched"],
                          disagree["scientific_controls"]), ("unknown", True, 2))
        missing = driver.calc_signals(driver.parse_calc_mode("0\t\t0\t\t0"))
        self.assertEqual((missing["mode"], missing["header_present"],
                          missing["header_matched"]), ("unknown", False, False))
        self.assertEqual(driver.calc_signals(None), {"mode": "unknown", "readable": False})

    def test_the_mode_script_reads_the_header_type_and_count_and_no_value(self):
        script = driver.CALC_MODE_PS
        self.assertIn("ControlType.ProgrammaticName", script)
        self.assertIn("$headers.Count", script)
        self.assertNotIn("Invoke", script)
        self.assertNotIn("ValuePattern", script)
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
        # UI Automation sees the item, the engine's observation holds none or two of them
        _, _, zero = self.stopped(std_count=0, uia_std=1)
        _, _, two = self.stopped(std_count=2, uia_std=1)
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
        for kwargs in ({"std_count": 0, "uia_std": 1}, {"std_count": 2, "uia_std": 1}, {}):
            _, summary, _ = self.stopped(**kwargs)
            text = json.dumps(summary["targets"], ensure_ascii=False)
            for name in ("Estándar", "Abrir navegación", "Cerrar navegación"):
                self.assertNotIn(name, text)

    def test_a_uia_read_that_fails_does_not_hide_the_stop(self):
        desk = FakeCalculator(self.tmp, std_count=0, uia_std=1)
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


# ---- S4 sixth-round finding 1: grounding a point check inside a XAML island ---------------------

HOST_RID = "7.1"
ISLAND_CLASS = "Microsoft.UI.Content.DesktopChildSiteBridge"
POINT = (100, 25)


def walk_row(index, parent, depth, rid, aid, ctype, rect, offscreen=False, cls="", name="Editar"):
    """One row of the island walk as the reader returns it. `name` is never part of a real row:
    it is here so that a record that copied a row whole would leak it into the tests."""
    return {"index": index, "parent": parent, "depth": depth, "runtime_id": rid,
            "automation_id": aid, "control_type": ctype, "class_name": cls,
            "offscreen": offscreen, "rect": rect, "name": name}


def island_tree(edit_rid="5.1", extra=()):
    """The Notepad island: host > group > menu bar > {Edit, File}. Only `Edit` contains POINT."""
    rows = [walk_row(0, -1, 0, HOST_RID, "", "Pane", (0, 0, 400, 300), cls=ISLAND_CLASS),
            walk_row(1, 0, 1, "7.2", "RootGrid", "Group", (0, 0, 400, 300)),
            walk_row(2, 1, 2, "7.3", "MenuBar", "MenuBar", (0, 0, 300, 40)),
            walk_row(3, 2, 3, edit_rid, "Edit", "MenuItem", (80, 10, 40, 30)),
            walk_row(4, 2, 3, "7.5", "File", "MenuItem", (10, 10, 60, 30))]
    return rows + list(extra)


def host_point_chain():
    """What the point query returns inside Notepad: the island host, the window, the desktop."""
    def row(rid, ctype, cls):
        return {"runtime_id": rid, "automation_id": "", "control_type": ctype,
                "class_name": cls, "offscreen": False}
    return [row(HOST_RID, "Pane", ISLAND_CLASS), row("2.9", "Window", "Notepad"),
            row("2.1", "Pane", "#32769")]


PLAIN_CHAIN = [{"runtime_id": "5.1", "automation_id": "Edit", "control_type": "MenuItem",
                "class_name": "MenuBarItem", "offscreen": False},
               {"runtime_id": "5.0", "automation_id": "MenuBar", "control_type": "MenuBar",
                "class_name": "MenuBar", "offscreen": False}]


class IslandReads:
    """UI Automation reads of a fake island: the point query and the walk. The point is the cursor."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.point_rows = host_point_chain()  # what the reads return after the click
        self.walk_rows = island_tree()
        self.walk_truncated = False
        self.walk_readable = True
        self.walk_calls = []
        # The reads that precede the click (ADR-0022): the world before the click is the good
        # island unless the test says otherwise; a click changes it to the scenario above.
        self.pre_point_rows = host_point_chain()
        self.phase = "pre"

    def cursor_point(self):
        return POINT

    def element_centre(self, hwnd, label, control_types):
        self.phase = "pre"
        return POINT

    def point_chain(self, x, y):
        return self.pre_point_rows if self.phase == "pre" else self.point_rows

    def island_walk(self, x, y, host_rid):
        self.walk_calls.append((x, y, host_rid))
        if self.phase == "pre":
            return island_tree(), False
        if not self.walk_readable:
            return None
        return self.walk_rows, self.walk_truncated


def no_real_uia(case):
    """The island tests never read the real desktop: a stray call to a reader fails the test."""
    for name in ("uia_at_cursor", "uia_chain_at", "uia_island_walk"):
        patcher = mock.patch.object(driver, name, create=True,
                                    side_effect=AssertionError(f"real UI Automation: {name}"))
        patcher.start()
        case.addCleanup(patcher.stop)


class IslandRestore(IslandReads, FakeNotepad):
    cursor_chain = driver.Protocol.cursor_chain  # the real grounding, over the fake reads

    def restore_click(self, res, target, snap):
        self.phase = "post"
        return super().restore_click(res, target, snap)


class IslandRun(IslandReads, FakeNotepadRun):
    cursor_chain = driver.Protocol.cursor_chain
    diag_at_element = driver.Protocol.diag_at_element

    def call(self, tool, args):
        self.phase = "post"
        return super().call(tool, args)


def timeout_error():
    return subprocess.TimeoutExpired("powershell", 60)


class DescentRuleTest(unittest.TestCase):
    """The rule, as pure functions: descend only into a closed list of island hosts, take the
    deepest on-screen element that contains the point, and confirm nothing when it is ambiguous,
    the walk was cut or the host changed."""

    def descend(self, rows, truncated=False, point=POINT):
        host = host_point_chain()[0]
        return driver.descend(point, host, lambda x, y, rid: (rows, truncated))

    def test_the_deepest_element_under_the_point_is_the_result_with_its_ancestors_to_the_host(self):
        chain, record = self.descend(island_tree())
        self.assertEqual([r["automation_id"] for r in chain], ["Edit", "MenuBar", "RootGrid", ""])
        self.assertEqual(chain[-1]["runtime_id"], HOST_RID, "the chain ends at the host")
        self.assertEqual((record["descended"], record["status"], record["depth"]),
                         (True, "ok", 3))
        self.assertEqual((record["visited"], record["truncated"], record["ambiguous"]),
                         (4, False, False))

    def test_a_deeper_element_wins_over_a_shallower_one(self):
        glyph = walk_row(5, 3, 4, "7.9", "Glyph", "Text", (95, 15, 10, 20))  # inside Edit
        chain, record = self.descend(island_tree(extra=[glyph]))
        self.assertEqual(chain[0]["automation_id"], "Glyph")
        self.assertEqual(chain[1]["automation_id"], "Edit", "its parent, which identity_hit reads")
        self.assertEqual(record["depth"], 4)

    def test_two_elements_at_the_deepest_level_are_ambiguous_and_confirm_nothing(self):
        twin = walk_row(5, 2, 3, "7.8", "Twin", "MenuItem", (90, 5, 40, 30))
        chain, record = self.descend(island_tree(extra=[twin]))
        self.assertEqual(chain, [])
        self.assertEqual((record["status"], record["ambiguous"]), ("ambiguous", True))

    def test_a_cut_walk_confirms_nothing_even_when_the_item_was_seen(self):
        chain, record = self.descend(island_tree(), truncated=True)
        self.assertEqual(chain, [])
        self.assertEqual((record["status"], record["truncated"]), ("truncated", True))

    def test_the_bounds_are_explicit_and_enforced_on_the_rows_too(self):
        self.assertEqual((driver.DESCENT_MAX_ELEMENTS, driver.DESCENT_MAX_DEPTH), (500, 16))
        crowd = [walk_row(5 + i, 2, 3, f"8.{i}", "", "Text", (0, 0, 1, 1))
                 for i in range(driver.DESCENT_MAX_ELEMENTS)]  # 4 + 500 rows below the host
        chain, record = self.descend(island_tree(extra=crowd), truncated=False)
        self.assertEqual((chain, record["status"]), ([], "truncated"))
        deep = walk_row(5, 3, driver.DESCENT_MAX_DEPTH + 1, "8.1", "", "Text", (90, 15, 10, 10))
        chain, record = self.descend(island_tree(extra=[deep]))
        self.assertEqual((chain, record["status"]), ([], "truncated"))

    def test_nothing_under_the_point_is_not_found(self):
        chain, record = self.descend(island_tree(), point=(900, 900))
        self.assertEqual((chain, record["status"]), ([], "not_found"))
        self.assertEqual(record["visited"], 4)

    def test_an_off_screen_element_or_an_empty_rectangle_is_never_the_result(self):
        for edit in (dict(offscreen=True), dict(rect=(80, 10, 0, 0))):
            rows = island_tree()
            rows[3] = dict(rows[3], **edit)
            chain, record = self.descend(rows)
            self.assertEqual(chain[0]["automation_id"], "MenuBar", "the next deepest, not Edit")
            self.assertEqual(record["depth"], 2)

    def test_an_unreadable_walk_or_another_host_confirms_nothing(self):
        host = host_point_chain()[0]
        chain, record = driver.descend(POINT, host, lambda x, y, rid: None)
        self.assertEqual((chain, record["status"]), ([], "unreadable"))
        other = island_tree()
        other[0] = dict(other[0], runtime_id="9.9")
        self.assertEqual(self.descend(other)[1]["status"], "host_changed")

        def boom(x, y, rid):
            raise timeout_error()

        chain, record = driver.descend(POINT, host, boom)
        self.assertEqual((chain, record["status"]), ([], "unreadable"))

    def test_only_the_closed_list_of_hosts_is_descended(self):
        self.assertEqual(driver.ISLAND_HOST_CLASSES, (ISLAND_CLASS,))
        self.assertTrue(driver.is_island_host({"class_name": ISLAND_CLASS}))
        for cls in ("Notepad", "", ISLAND_CLASS + "2", ISLAND_CLASS.lower()):
            self.assertFalse(driver.is_island_host({"class_name": cls}), cls)
        self.assertFalse(driver.is_island_host({}), "a row with no class")

    def test_a_chain_outside_an_island_is_returned_as_read_without_a_walk(self):
        calls = []
        chain, record = driver.ground_chain(POINT, lambda x, y: PLAIN_CHAIN,
                                            lambda *a: calls.append(a))
        self.assertIs(chain, PLAIN_CHAIN)
        self.assertEqual((record, calls), ({"descended": False}, []))
        chain, record = driver.ground_chain(POINT, lambda x, y: [], lambda *a: calls.append(a))
        self.assertEqual((chain, record, calls), ([], {"descended": False}, []))

    def test_the_result_is_identified_by_the_same_rule_as_before(self):
        chain, _ = self.descend(island_tree())
        edit = {"runtime_id": "5.1", "automation_id": "Edit", "control_type": "MenuItem"}
        self.assertTrue(driver.identity_hit(edit, chain))
        self.assertFalse(driver.identity_hit(dict(edit, runtime_id="5.2"), chain),
                         "same AutomationId, another RuntimeId")
        # AutomationId decides only when a RuntimeId is missing
        self.assertTrue(driver.identity_hit(dict(edit, runtime_id=""), chain))
        self.assertFalse(driver.identity_hit(dict(edit, runtime_id=""), []))


class WalkParsingTest(unittest.TestCase):
    def line(self, *fields):
        return "\t".join(str(f) for f in fields)

    def test_a_complete_walk_is_parsed_with_its_truncation_flag(self):
        host = self.line(0, -1, 0, "7.1", "", "Pane", ISLAND_CLASS, 0, 0, 0, 400, 300)
        out = "\n".join([host,
                         self.line(1, 0, 1, "7.2", "Edit", "MenuItem", "", 1, 80, 10, 40, 30),
                         "#end\t0"])
        rows, truncated = driver.parse_island_walk(out)
        self.assertEqual((len(rows), truncated), (2, False))
        self.assertEqual(rows[1], {"index": 1, "parent": 0, "depth": 1, "runtime_id": "7.2",
                                   "automation_id": "Edit", "control_type": "MenuItem",
                                   "class_name": "", "offscreen": True, "rect": (80, 10, 40, 30)})
        self.assertTrue(driver.parse_island_walk(host + "\n#end\t1")[1])

    def test_an_incomplete_or_malformed_walk_is_unreadable(self):
        host = self.line(0, -1, 0, "7.1", "", "Pane", ISLAND_CLASS, 0, 0, 0, 4, 3)
        orphan = self.line(1, 5, 1, "7.2", "", "Text", "", 0, 0, 0, 1, 1)
        not_a_number = self.line(1, 0, 1, "7.2", "", "Text", "", 0, "x", 0, 1, 1)
        for bad in ("", "#host_changed", host, host + "\ngarbage\n#end\t0",
                    host + "\n" + orphan + "\n#end\t0",
                    host + "\n" + not_a_number + "\n#end\t0", host + "\n#end\t2"):
            self.assertIsNone(driver.parse_island_walk(bad), bad)

    def test_point_rows_carry_the_class_name(self):
        rows = driver.parse_point_rows(f"7.1\t\tControlType.Pane\t{ISLAND_CLASS}\nbroken\n")
        self.assertEqual(rows, [{"runtime_id": "7.1", "automation_id": "", "control_type": "Pane",
                                 "class_name": ISLAND_CLASS, "offscreen": False}])

    def test_the_scripts_only_read_identifiers_types_classes_and_rectangles(self):
        walk = driver.island_walk_script(1, 2, "7.1")
        for script in (driver.UIA_AT_PS, walk, driver.UIA_ELEMENT_CENTRE_PS):
            for forbidden in ("Current.Name", "ValuePattern", "TextPattern", "InvokePattern",
                              "SetFocus", "SendKeys", "Click"):
                self.assertNotIn(forbidden, script)
        self.assertIn(str(driver.DESCENT_MAX_ELEMENTS), walk)
        self.assertIn(str(driver.DESCENT_MAX_DEPTH), walk)
        self.assertNotIn("__", walk, "every placeholder is filled")


class IslandGroundingTest(QuietTest):
    """Sixth round, finding 1: inside Notepad the point query returns the island host, never the
    menu item. The client descends into the host and grounds the click on what it finds."""

    def setUp(self):
        super().setUp()
        no_real_uia(self)

    def run_flow(self, diag=None, **setup):
        """One Notepad run over the fake island; `diag` is what the point query at the target's
        centre returns (nothing by default: the diagnostic then records no chain)."""
        desk = IslandRun(self.tmp, ids_change=False)
        for key, value in setup.items():
            setattr(desk, key, value)
        res = driver.new_result("E2E-03", 1)
        snap, elements, _ = desk.observe()
        with mock.patch.object(driver, "uia_diag_at_element", return_value=diag):
            desk.steps(res, snap, elements, 77, 4242)
        return desk, res

    # (a) the descent finds `Editar` with the expected RuntimeId
    def test_a_point_query_that_returns_the_island_is_grounded_on_the_item_found_inside(self):
        desk, res = self.run_flow()
        self.assertEqual(res["grounding"], [True, True])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (0, 0))
        self.assertTrue(res["success"])
        self.assertIsNone(res["failure"])
        self.assertEqual(desk.clicks, ["Editar", "Editar"])
        self.assertEqual(len(desk.walk_calls), 4, "one walk before each click and one after")
        self.assertEqual(desk.walk_calls[0], (100, 25, HOST_RID))
        self.assertEqual([(d["step"], d["status"], d["depth"]) for d in res["descent"]],
                         [(1, "ok", 3), (2, "ok", 3)])

    # (b) the descent finds another control: not grounded, and nothing is retried
    def test_a_descent_that_finds_another_control_is_not_grounded_and_not_retried(self):
        other = island_tree(edit_rid="9.9")
        other[3] = dict(other[3], automation_id="Other", control_type="Button")
        desk, res = self.run_flow(walk_rows=other)
        self.assertEqual(desk.clicks, ["Editar"], "one click, no second step, no retry")
        self.assertEqual(res["grounding"], [False])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertIn("not the element", res["failure"])
        self.assertEqual(len(desk.walk_calls), 2, "the one before the click and the one after")

    # (c) two elements that contain the point and are not nested: ambiguous
    def test_an_ambiguous_descent_is_not_grounded_even_if_one_candidate_is_the_item(self):
        twin = walk_row(5, 2, 3, "7.8", "Twin", "MenuItem", (90, 5, 40, 30))
        desk, res = self.run_flow(walk_rows=island_tree(extra=[twin]))
        self.assertEqual(desk.clicks, ["Editar"])
        self.assertEqual(res["grounding"], [False])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertEqual((res["descent"][0]["status"], res["descent"][0]["ambiguous"]),
                         ("ambiguous", True))
        self.assertNotIn("grounding_chain_empty", res, "that counter is for an empty point query")

    # (d) the bound is reached
    def test_a_truncated_descent_is_not_grounded_even_if_the_item_was_seen(self):
        desk, res = self.run_flow(walk_truncated=True)
        self.assertEqual(desk.clicks, ["Editar"])
        self.assertEqual(res["grounding"], [False])
        self.assertEqual((res["descent"][0]["status"], res["descent"][0]["truncated"]),
                         ("truncated", True))

    def test_an_unreadable_walk_is_a_grounding_failure(self):
        desk, res = self.run_flow(walk_readable=False)
        self.assertEqual((desk.clicks, res["grounding"]), (["Editar"], [False]))
        self.assertEqual(res["descent"][0]["status"], "unreadable")

    # (e) a chain outside an island behaves exactly as before
    def test_a_chain_that_is_not_an_island_is_grounded_as_before_and_never_walked(self):
        desk, res = self.run_flow(point_rows=PLAIN_CHAIN, pre_point_rows=PLAIN_CHAIN)
        self.assertEqual(res["grounding"], [True, True])
        self.assertEqual(desk.walk_calls, [])
        self.assertNotIn("descent", res)

    def test_a_chain_that_is_not_an_island_and_is_another_element_fails_as_before(self):
        other = [{"runtime_id": "9.9", "automation_id": "Other", "control_type": "Button",
                  "class_name": "Button", "offscreen": False}]
        desk, res = self.run_flow(point_rows=other, pre_point_rows=PLAIN_CHAIN)
        self.assertEqual((res["grounding"], desk.walk_calls), ([False], []))
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))

    def test_the_island_host_alone_never_confirms_an_item(self):
        # The old failure, kept as a regression: the host with nothing under it grounds nothing.
        _, res = self.run_flow(walk_rows=[island_tree()[0]])
        self.assertEqual(res["grounding"], [False])

    # (f) no names anywhere in the records
    def test_the_records_hold_no_names(self):
        twin = walk_row(5, 2, 3, "7.8", "Twin", "MenuItem", (90, 5, 40, 30))
        for setup in ({}, {"walk_truncated": True}, {"walk_rows": island_tree(extra=[twin])}):
            _, res = self.run_flow(**setup)
            text = json.dumps({k: res[k] for k in ("descent", "menu_diagnostic", "identity",
                                                   "grounding")}, ensure_ascii=False)
            self.assertNotIn("Editar", text)
            for key in ("name", "rect"):
                self.assertNotIn(f'"{key}"', text, "rows are not copied into the record")

    def test_the_diagnostic_records_whether_the_descent_happened_and_how_it_ended(self):
        island_rows = [dict(r, expand_state="") for r in host_point_chain()]
        desk, res = self.run_flow(diag=island_rows)
        first = res["menu_diagnostic"][0]
        expected = {"descended": True, "status": "ok", "depth": 3, "visited": 4,
                    "truncated": False, "ambiguous": False, "control_type": "MenuItem",
                    "automation_id": "Edit", "max_elements": 500, "max_depth": 16}
        self.assertEqual(first["before_descent"], expected)
        self.assertEqual(first["after_descent"], expected)
        self.assertEqual(len(desk.walk_calls), 6, "diagnostic before, grounding before and "
                                                  "diagnostic after, for each of the two clicks")

    def test_the_diagnostic_of_a_chain_without_an_island_says_there_was_no_descent(self):
        _, res = self.run_flow(diag=[dict(r, expand_state="") for r in PLAIN_CHAIN],
                               point_rows=PLAIN_CHAIN)
        for record in res["menu_diagnostic"]:
            self.assertEqual((record["before_descent"], record["after_descent"]),
                             ({"descended": False}, {"descended": False}))

    def test_a_diagnostic_that_cannot_be_read_does_not_change_the_run(self):
        _, res = self.run_flow(diag=None)
        self.assertTrue(res["success"])


class IslandRestoreTest(QuietTest):
    """The closing click is grounded by the same rule."""

    def setUp(self):
        super().setUp()
        no_real_uia(self)

    def restore(self, **setup):
        proto = IslandRestore(["Expanded"])
        for key, value in setup.items():
            setattr(proto, key, value)
        res = {"run": 1, "observations": []}
        closed = proto.restore_menu(res, 1)
        return proto, closed, res, res["menu_restore"]

    def test_a_restore_click_is_grounded_on_the_item_found_inside_the_island(self):
        proto, closed, res, record = self.restore()
        self.assertTrue(closed)
        self.assertIs(record["grounded"], True)
        self.assertEqual(record["descent"]["status"], "ok")
        self.assertEqual((res.get("grounding_wrong", 0), res.get("engine_false_success", 0)),
                         (0, 0))
        self.assertEqual(proto.walk_calls, [(100, 25, HOST_RID)] * 2, "before and after")

    def test_a_restore_click_whose_descent_finds_another_control_is_reported_not_retried(self):
        other = island_tree()
        other[3] = dict(other[3], runtime_id="9.9", automation_id="Other")
        proto, _, res, record = self.restore(walk_rows=other)
        self.assertEqual(proto.clicks, ["Editar"], "one click, nothing more")
        self.assertIs(record["grounded"], False)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))

    def test_an_ambiguous_or_truncated_descent_does_not_ground_the_restore_click(self):
        twin = walk_row(5, 2, 3, "7.8", "Twin", "MenuItem", (90, 5, 40, 30))
        for setup, status in (({"walk_rows": island_tree(extra=[twin])}, "ambiguous"),
                              ({"walk_truncated": True}, "truncated")):
            proto, _, _, record = self.restore(**setup)
            self.assertEqual(proto.clicks, ["Editar"], status)
            self.assertIs(record["grounded"], False, status)
            self.assertEqual(record["descent"]["status"], status)

    def test_a_restore_without_an_island_has_no_descent_record(self):
        proto, _, _, record = self.restore(point_rows=PLAIN_CHAIN, pre_point_rows=PLAIN_CHAIN)
        self.assertIs(record["grounded"], True)
        self.assertNotIn("descent", record)
        self.assertEqual(proto.walk_calls, [])

    def test_the_record_holds_no_names(self):
        _, _, _, record = self.restore()
        self.assertNotIn("Editar", json.dumps(record, ensure_ascii=False))


class HelperWindowTest(QuietTest):
    """Eighth round, finding 2: winit's event target is a VISIBLE 16 x 16 window of the same
    process. The client maximized it (which also activated it), so it became the foreground
    window and passed for the maximized application."""

    def test_the_event_target_and_the_input_method_windows_are_helpers(self):
        for name in ("Winit Thread Event Target", "MSCTFIME UI", "IME"):
            self.assertTrue(driver.is_helper_class(name), name)
        for name in ("Window Class", "ApplicationFrameWindow", "CabinetWClass", "Notepad"):
            self.assertFalse(driver.is_helper_class(name), name)

    def test_placing_a_process_never_touches_its_helper_windows(self):
        fake = mock.MagicMock()
        classes = {1: "Winit Thread Event Target", 2: "Window Class", 3: "MSCTFIME UI"}
        with mock.patch.object(driver, "user32", fake), \
                mock.patch.object(driver, "app_windows", return_value=[1, 2, 3]), \
                mock.patch.object(driver, "_class", side_effect=lambda h: classes[h]), \
                mock.patch.object(driver, "on_primary_maximized", return_value=False):
            moved = driver.place_on_primary(5151)
        self.assertEqual(moved, 1)
        touched = {call.args[0] for call in fake.ShowWindow.call_args_list}
        self.assertEqual(touched, {2}, "only the main window is restored and maximized")
        self.assertEqual({c.args[0] for c in fake.SetWindowPos.call_args_list}, {2})

    def test_a_helper_window_in_front_is_not_the_application_in_front(self):
        front = FakeFront()
        helper = {"exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 1, "maximized_primary": True,
                  "class": "Winit Thread Event Target"}
        main = dict(helper, hwnd=2, **{"class": "Window Class"})
        self.assertFalse(front.is_front(helper, 5151))
        self.assertTrue(front.is_front(main, 5151))
        self.assertFalse(front.is_front(dict(main, maximized_primary=False), 5151))
        self.assertFalse(front.is_front(dict(main, pid=9), 5151))

    def test_the_gate_refuses_a_foreground_that_is_a_helper_window(self):
        front = FakeFront()
        front.engaged = []
        front.engage_stop = lambda reason: front.engaged.append(reason) or (_ for _ in ()).throw(
            driver.Abort(reason))
        with mock.patch.object(driver, "foreground", return_value={
                "exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 1,
                "maximized_primary": True, "class": "Winit Thread Event Target"}):
            with self.assertRaises(driver.Abort):
                front.require_foreground()

    def test_the_steps_read_the_main_window_not_the_foreground_helper(self):
        front = FakeFront()
        front.windows = {5151: 2}
        front.fg = {"exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 1,
                    "maximized_primary": True, "class": "Winit Thread Event Target"}
        self.assertEqual(front.app_window(5151), 2)
        front.windows = {}
        self.assertEqual(front.app_window(5151), 1, "else the foreground window")


class FakeFront(driver.Protocol):
    """A Protocol whose window management is a script: no window, no input, a virtual clock."""

    def __init__(self, exe="ommatix-testeditor.exe", operator=False, wait=10.0, operator_wait=20.0,
                 arrives_after=None, arrives_by=None, notepad=None, arrives_hwnd=2):
        self.args = SimpleNamespace(foreground_wait=wait, operator_activation=operator,
                                    operator_activation_wait=operator_wait)
        self.flow = {"exe": exe}
        self.started = {5151}
        self.notepad = notepad
        self.placements = 0
        self.activation = driver.new_activation()
        self.now = 0.0
        self.windows = {5151: 2}
        self.fg = {"exe": "other.exe", "pid": 1, "hwnd": 50, "maximized_primary": True,
                   "class": "Chrome_WidgetWin_1"}
        self.arrives_by = arrives_by  # the method name after which the window is in front
        self.arrives_after = arrives_after  # the virtual time at which the operator clicks
        self.arrives_hwnd = arrives_hwnd
        self.activations = []  # the handles asked for
        self.announced = []
        self.sent = []  # anything that would be input: must stay empty
        self.placed = 0
        self.diag = {"windows": [{"class": "Window Class", "visible": True, "iconic": False,
                                  "zoomed": True, "primary_monitor": True}],
                     "foreground": {"class": "Chrome_WidgetWin_1", "exe": "msedge.exe"}}

    def mono(self):
        return self.now

    def pause(self, seconds):
        self.now += seconds
        if self.arrives_after is not None and self.now >= self.arrives_after:
            self.bring(self.arrives_hwnd)

    def bring(self, hwnd):
        self.fg = {"exe": self.flow["exe"], "pid": 5151, "hwnd": hwnd, "maximized_primary": True,
                   "class": "Window Class"}

    def front(self):
        return self.fg

    def front_hwnd(self):
        return self.fg["hwnd"]

    def place(self, pid):
        self.placed += 1
        return 0

    def window_of(self, pid):
        return self.windows.get(pid)

    def current_hwnd(self):
        return self.fg["hwnd"]

    def activate(self, hwnd):
        self.activations.append(hwnd)
        if self.arrives_by is not None and len(self.activations) >= self.arrives_by[1]:
            self.bring(hwnd)
            return self.arrives_by[0]
        return None

    def diagnose(self, pid, hwnd=None):
        return self.diag

    def announce(self, text):
        self.announced.append(text)


class WaitForegroundTest(QuietTest):
    """Eighth round, finding 2 (owner's observation): the window opens, its taskbar button
    flashes and it stays behind: the Windows foreground lock. The window is asked for by its
    handle with a ladder of methods, the one that works is recorded, and nothing is sent."""

    def test_the_window_is_asked_for_by_handle_and_the_method_that_worked_is_recorded(self):
        front = FakeFront(arrives_by=("attach_show_top", 3))
        ok, requests = front.wait_foreground(5151)
        self.assertTrue(ok)
        self.assertEqual(front.activations, [2, 2, 2], "the main window's handle, never a pid")
        self.assertEqual(requests, 3)
        fields = driver.activation_fields(front.activation)
        self.assertEqual(fields["activation_method"], "attach_show_top")
        self.assertEqual(fields["activation_tried"], {"attach_show_top": 1, "none": 2})
        self.assertIs(fields["operator_activation"], False)
        self.assertNotIn("activation_diagnostic", fields)

    def test_a_window_already_in_front_needs_no_request(self):
        front = FakeFront()
        front.bring(2)
        self.assertEqual(front.wait_foreground(5151), (True, 0))
        self.assertEqual(front.activations, [])

    def test_the_placement_runs_every_round_and_counts(self):
        front = FakeFront(arrives_by=("attach_foreground", 1))
        front.place = lambda pid: 1
        front.wait_foreground(5151)
        self.assertGreaterEqual(front.placements, 1)

    def test_a_window_that_never_arrives_leaves_a_read_only_diagnostic(self):
        front = FakeFront()
        ok, requests = front.wait_foreground(5151)
        self.assertFalse(ok)
        self.assertGreater(requests, 3)
        fields = driver.activation_fields(front.activation)
        diag = fields["activation_diagnostic"]
        self.assertEqual(diag["windows"][0]["class"], "Window Class")
        self.assertEqual(diag["foreground"], {"class": "Chrome_WidgetWin_1", "exe": "msedge.exe"})
        self.assertIsNone(fields["activation_method"])
        self.assertIs(fields["operator_activation"], False)
        self.assertEqual(front.announced, [], "no operator prompt unless it was asked for")

    def test_the_real_diagnostic_holds_classes_and_flags_but_never_a_title(self):
        facts = {"class": "Window Class", "visible": True, "iconic": False, "zoomed": True,
                 "primary_monitor": True}
        with mock.patch.object(driver, "process_windows", return_value=[2]), \
                mock.patch.object(driver, "window_facts", return_value=facts), \
                mock.patch.object(driver, "user32") as user32, \
                mock.patch.object(driver, "_class", return_value="Chrome_WidgetWin_1"), \
                mock.patch.object(driver, "_pid", return_value=7), \
                mock.patch.object(driver, "_exe", return_value="msedge.exe"):
            user32.GetForegroundWindow.return_value = 50
            diag = driver.activation_diagnostic(5151)
        self.assertEqual(diag, {"windows": [facts],
                                "foreground": {"class": "Chrome_WidgetWin_1", "exe": "msedge.exe"}})
        self.assertFalse(user32.GetWindowTextW.called)
        self.assertEqual(set(facts), {"class", "visible", "iconic", "zoomed", "primary_monitor"})

    def test_the_run_records_carry_the_activation_fields(self):
        front = FakeFront()
        front.wait_foreground(5151)
        record = json.loads(json.dumps(driver.activation_fields(front.activation)))
        self.assertEqual(set(record), {"activation_method", "activation_tried",
                                       "operator_activation", "activation_diagnostic"})

    def test_the_notepad_window_is_asked_for_by_its_recorded_handle(self):
        front = FakeFront(exe="notepad.exe", notepad={"pid": 8960, "hwnd": 777})
        front.started = set()
        front.wait_foreground(8960)
        self.assertEqual(set(front.activations), {777})


class OperatorActivationTest(QuietTest):
    """Eighth round, finding 2 (b): off by default. When the lock still wins, the operator is
    told to click the flashing taskbar button; the client only waits, bounded, for the recorded
    window to be the foreground window, and sends nothing before that."""

    def test_it_is_a_flag_that_is_off_by_default(self):
        import inspect
        self.assertIn('"--operator-activation", action="store_true"', inspect.getsource(driver.main))
        self.assertFalse(getattr(SimpleNamespace(foreground_wait=1.0), "operator_activation", False))

    def test_the_operator_is_asked_on_stderr_and_the_client_waits_for_the_recorded_handle(self):
        front = FakeFront(operator=True, arrives_after=14.0)
        ok, _ = front.wait_foreground(5151)
        self.assertTrue(ok)
        self.assertEqual(len(front.announced), 1)
        self.assertIn("taskbar button", front.announced[0])
        self.assertIn("ommatix-testeditor.exe", front.announced[0])
        self.assertIs(front.activation["operator_activation"], True)
        self.assertIs(driver.activation_fields(front.activation)["operator_activation"], True)
        self.assertEqual(front.sent, [])

    def test_only_the_recorded_handle_in_front_is_the_operators_answer(self):
        front = FakeFront(operator=True, wait=4.0, operator_wait=6.0, arrives_after=7.0,
                          arrives_hwnd=1)
        ok, _ = front.wait_foreground(5151)
        self.assertFalse(ok, "another window of the same application is not the recorded one")
        self.assertIs(front.activation["operator_activation"], False)

    def test_the_wait_is_bounded_and_the_record_says_it_failed(self):
        front = FakeFront(operator=True, wait=4.0, operator_wait=6.0)
        ok, _ = front.wait_foreground(5151)
        self.assertFalse(ok)
        self.assertLessEqual(front.now, 4.0 + 6.0 + 0.5)
        self.assertIs(front.activation["operator_activation"], False)
        self.assertEqual(len(front.announced), 1)

    def test_nothing_is_announced_when_the_method_ladder_works(self):
        front = FakeFront(operator=True, arrives_by=("attach_foreground", 1))
        ok, _ = front.wait_foreground(5151)
        self.assertTrue(ok)
        self.assertEqual(front.announced, [])
        self.assertIs(front.activation["operator_activation"], False)

    def test_there_is_nothing_to_wait_for_without_a_recorded_window(self):
        front = FakeFront(operator=True, wait=2.0)
        front.windows = {}
        ok, _ = front.wait_foreground(5151)
        self.assertFalse(ok)
        self.assertEqual(front.announced, [])


class ActivationLadderTest(unittest.TestCase):
    """The methods are tried in turn and verified by GetForegroundWindow, never by their own
    return value."""

    @staticmethod
    def ladder(works_at=None, raises=()):
        state = {"fg": 50, "calls": []}

        def method(name):
            def apply(hwnd):
                state["calls"].append(name)
                if name in raises:
                    raise OSError(name)
                if name == works_at:
                    state["fg"] = hwnd
            return name, apply

        names = ("attach_foreground", "attach_show_top", "attach_all_topmost")
        return state, tuple(method(n) for n in names)

    def run_ladder(self, **kwargs):
        state, methods = self.ladder(**kwargs)
        found = driver.activate_window(2, methods, lambda: state["fg"], pause=lambda s: None)
        return found, state["calls"]

    def test_the_first_method_that_puts_the_window_in_front_is_returned(self):
        self.assertEqual(self.run_ladder(works_at="attach_show_top"),
                         ("attach_show_top", ["attach_foreground", "attach_show_top"]))
        self.assertEqual(self.run_ladder(works_at="attach_foreground"),
                         ("attach_foreground", ["attach_foreground"]))

    def test_a_window_already_in_front_calls_no_method(self):
        state, methods = self.ladder()
        state["fg"] = 2
        self.assertEqual(driver.activate_window(2, methods, lambda: state["fg"]),
                         "already_foreground")
        self.assertEqual(state["calls"], [])

    def test_when_no_method_works_the_answer_is_none(self):
        self.assertEqual(self.run_ladder(), (None, ["attach_foreground", "attach_show_top",
                                                    "attach_all_topmost"]))

    def test_a_method_that_raises_is_skipped(self):
        self.assertEqual(self.run_ladder(works_at="attach_all_topmost", raises=("attach_foreground",))[0],
                         "attach_all_topmost")

    def test_the_methods_are_named_and_ordered(self):
        self.assertEqual([n for n, _ in driver.ACTIVATION_METHODS],
                         ["attach_foreground", "attach_show_top", "attach_all_topmost"])

    def test_no_method_sends_input_or_changes_a_setting(self):
        import inspect
        source = "".join(inspect.getsource(f) for _, f in driver.ACTIVATION_METHODS)
        source += inspect.getsource(driver.activate_window)
        for forbidden in ("SystemParametersInfo", "keybd_event", "SendInput", "mouse_event",
                          "Shell_TrayWnd", "SendMessage", "PostMessage", "LockSetForegroundWindow",
                          "ForegroundLockTimeout", "VK_MENU"):
            self.assertNotIn(forbidden, source)


class ServeArgvTest(unittest.TestCase):
    """SEC-02 across a restart (chapter 08 §8.4): a live run starts the engine without a fixed
    session id, because the engine refuses one with live input. A probe keeps its identifier."""

    def test_a_live_run_takes_no_fixed_session_id_and_sends_with_sendinput(self):
        argv = driver.serve_argv("ommatix.exe", "out", "live.toml", True, None)
        self.assertNotIn("--session-id", argv)
        self.assertEqual(argv[-2:], ["--input", "sendinput"])
        self.assertEqual(argv[:2], ["ommatix.exe", "serve"])

    def test_a_live_run_refuses_an_explicit_session_id_before_any_server_starts(self):
        with self.assertRaises(driver.Abort):
            driver.serve_argv("ommatix.exe", "out", "live.toml", True, "s4-e2e01")

    def test_a_probe_keeps_the_probe_identifier_and_sends_no_input(self):
        argv = driver.serve_argv("ommatix.exe", "out", "probe.toml", False, None)
        self.assertEqual(argv[-2:], ["--session-id", driver.PROBE_SESSION_ID])
        self.assertNotIn("--input", argv)

    def test_a_probe_passes_the_identifier_it_was_given(self):
        argv = driver.serve_argv("ommatix.exe", "out", "probe.toml", False, "check-1")
        self.assertEqual(argv[-2:], ["--session-id", "check-1"])


if __name__ == "__main__":
    unittest.main()
