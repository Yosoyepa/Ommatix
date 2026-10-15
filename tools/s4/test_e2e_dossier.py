"""Tests of the dossier's E2E-02 (editor) and E2E-03 (Explorer) client, ``e2e_dossier.py``.

No window is opened or activated, no UI Automation is read, no process is started or killed and
no input is sent: the desktop, the engine and the applications are fakes, and the temporary
tree lives in a private directory of the test. Run with:
python -m unittest discover -s tools/s4 -p "test_*.py"
"""

import contextlib
import importlib.util
import io
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import MappingProxyType, SimpleNamespace
from unittest import mock

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s4_e2e_dossier", HERE / "e2e_dossier.py")
dossier = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(dossier)
driver = dossier.driver


class QuietTest(unittest.TestCase):
    """No sleeping, no log lines, no idle read: the fakes answer at once, in a private dir."""

    def setUp(self):
        for target, name, extra in ((driver.time, "sleep", {}), (driver, "log", {}),
                                    (driver, "idle_seconds", {"return_value": 60.0})):
            patcher = mock.patch.object(target, name, **extra)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.temp_root = self.tmp / "temp"
        self.temp_root.mkdir()


# ---- the run folder ---------------------------------------------------------------------------


class RunTreeTest(QuietTest):
    def tree(self, run="s4-e2e03-r1-20261014-183000"):
        return dossier.RunTree(self.temp_root, run)

    def test_the_run_folder_is_created_under_ommatix_e2e_and_never_adopted(self):
        tree = self.tree()
        root = tree.create()
        self.assertEqual(root, self.temp_root / "ommatix-e2e" / "s4-e2e03-r1-20261014-183000")
        self.assertTrue(root.is_dir() and tree.created_base)
        with self.assertRaises(driver.Abort):
            self.tree().create()  # the same folder exists: not adopted
        self.assertTrue(root.is_dir(), "the existing folder was not touched")

    def test_run_ids_are_plain_names(self):
        for bad in ("", ".", "..", "a b", "a/b", "a\\b", "é", "x" * 65, "..."):
            with self.assertRaises(driver.Abort, msg=repr(bad)):
                dossier.RunTree(self.temp_root, bad)
        self.assertTrue(dossier.valid_run_id("s4-e2e02-r1-20261014-183000"))

    def test_only_what_the_client_created_is_deleted(self):
        tree = self.tree()
        root = tree.create()
        tree.add_dir("carpeta-a")
        tree.add_dir("carpeta-a/carpeta-b")
        tree.add_file("leeme.txt", b"x")
        tree.add_file("carpeta-a/carpeta-b/dummy.txt", b"y")
        bystander = self.temp_root / "ommatix-e2e" / "other-run"
        bystander.mkdir()
        (bystander / "keep.txt").write_bytes(b"keep")
        outside = self.temp_root / "keep-too.txt"
        outside.write_bytes(b"keep")

        outcome = tree.cleanup()

        self.assertEqual((outcome["deleted_files"], outcome["deleted_dirs"]), (2, 3))
        self.assertEqual(outcome["left"], [ "ommatix-e2e" ], "the root was shared: not removed")
        self.assertFalse(root.exists())
        self.assertEqual((bystander / "keep.txt").read_bytes(), b"keep")
        self.assertEqual(outside.read_bytes(), b"keep")

    def test_the_shared_root_is_removed_only_when_the_client_made_it_and_it_is_empty(self):
        mine = self.tree()
        mine.create()
        mine.add_file("leeme.txt", b"x")
        outcome = mine.cleanup()
        self.assertEqual(outcome["left"], [])
        self.assertFalse((self.temp_root / "ommatix-e2e").exists())

        (self.temp_root / "ommatix-e2e").mkdir()  # exists before: not the client's
        theirs = self.tree("run-2")
        theirs.create()
        self.assertFalse(theirs.created_base)
        theirs.cleanup()
        self.assertTrue((self.temp_root / "ommatix-e2e").is_dir())

    def test_an_unexpected_entry_is_left_alone_and_reported(self):
        tree = self.tree()
        root = tree.create()
        tree.add_dir("carpeta-a")
        stranger = root / "carpeta-a" / "stranger.txt"
        stranger.write_bytes(b"not ours")
        with mock.patch.object(shutil, "rmtree", side_effect=AssertionError("rmtree")):
            outcome = tree.cleanup()
        self.assertTrue(stranger.exists(), "an unrecorded file is never deleted")
        self.assertIn(f"{root.name}/carpeta-a", outcome["left"])
        self.assertIn(root.name, outcome["left"])

    def test_a_recorded_path_outside_the_run_folder_is_not_deleted(self):
        tree = self.tree()
        tree.create()
        victim = self.temp_root / "victim.txt"
        victim.write_bytes(b"keep")
        tree.files.append(victim)  # as if a bug recorded it
        outcome = tree.cleanup()
        self.assertEqual(victim.read_bytes(), b"keep")
        self.assertIn("victim.txt", outcome["left"])

    def test_paths_that_climb_out_are_refused_when_added(self):
        tree = self.tree()
        tree.create()
        for relative in ("../x", "a/../../x", "..", "/abs", "C:/abs"):
            with self.assertRaises(driver.Abort, msg=relative):
                tree.add_file(relative, b"x")
        self.assertEqual(sorted(p.name for p in (self.temp_root / "ommatix-e2e").iterdir()),
                         [tree.root.name])

    def test_a_file_is_never_overwritten(self):
        tree = self.tree()
        tree.create()
        tree.add_file("leeme.txt", b"one")
        with self.assertRaises(FileExistsError):
            tree.add_file("leeme.txt", b"two")
        self.assertEqual((tree.root / "leeme.txt").read_bytes(), b"one")

    def test_the_application_written_note_is_removed_only_by_its_exact_name(self):
        tree = self.tree()
        root = tree.create()
        (root / dossier.EDITOR_NOTE).write_bytes(b"prueba 123")
        (root / "other.txt").write_bytes(b"x")
        outcome = tree.cleanup(adopt=(dossier.EDITOR_NOTE,))
        self.assertFalse((root / dossier.EDITOR_NOTE).exists())
        self.assertTrue((root / "other.txt").exists())
        self.assertIn(root.name, outcome["left"], "the folder is not empty")

    def test_the_snapshot_sees_added_changed_and_removed_files(self):
        tree = self.tree()
        tree.create()
        tree.add_dir("carpeta-a")
        tree.add_file("carpeta-a/dummy.txt", b"12345")
        before = tree.snapshot()
        self.assertEqual(before, {"carpeta-a/": 0, "carpeta-a/dummy.txt": 5})
        (tree.root / "carpeta-a" / "dummy.txt").write_bytes(b"123456")
        self.assertNotEqual(tree.snapshot(), before)
        (tree.root / "carpeta-a" / "dummy.txt").write_bytes(b"12345")
        self.assertEqual(tree.snapshot(), before)
        (tree.root / "carpeta-a" / "dummy.txt").unlink()
        self.assertNotEqual(tree.snapshot(), before)
        (tree.root / "carpeta-a" / "dummy.txt").write_bytes(b"12345")
        (tree.root / "extra").mkdir()
        self.assertNotEqual(tree.snapshot(), before)


class LocationTest(unittest.TestCase):
    ROOT = r"C:\Users\x\AppData\Local\Temp\ommatix-e2e\run1"

    def test_the_folders_below_the_root_are_compared_as_windows_does(self):
        self.assertEqual(dossier.relative_parts(self.ROOT, self.ROOT), ())
        self.assertEqual(dossier.relative_parts(self.ROOT, self.ROOT.upper() + "\\"), ())
        self.assertEqual(dossier.relative_parts(self.ROOT, self.ROOT + r"\Carpeta-A\carpeta-b"),
                         ("carpeta-a", "carpeta-b"))

    def test_a_folder_outside_the_root_or_unknown_is_none(self):
        for here in (None, "", r"C:\Users\x", self.ROOT + "2", self.ROOT[:-1],
                     r"C:\Users\x\AppData\Local\Temp\ommatix-e2e"):
            self.assertIsNone(dossier.relative_parts(self.ROOT, here), here)

    def test_the_new_window_is_the_one_new_window_on_the_root(self):
        root = Path(self.ROOT)
        pick = dossier.pick_new_window
        self.assertEqual(pick({1, 2}, {1, 2, 9}, {9: self.ROOT}, root), 9)
        self.assertEqual(pick({1}, {1, 9}, {9: self.ROOT.lower()}, root), 9)
        # An existing window already on the root (the owner's) is not ours.
        self.assertIsNone(pick({1}, {1}, {}, root))
        # A new window elsewhere, an unreadable one, or two on the root: nothing is picked.
        self.assertIsNone(pick({1}, {1, 9}, {9: r"C:\Users\x"}, root))
        self.assertIsNone(pick({1}, {1, 9}, {9: None}, root))
        self.assertIsNone(pick({1}, {1, 8, 9}, {8: self.ROOT, 9: self.ROOT}, root))
        self.assertEqual(pick({1}, {1, 8, 9}, {8: r"C:\Users\x", 9: self.ROOT}, root), 9)


# ---- configuration ----------------------------------------------------------------------------


def good_explorer_config():
    return {"safety": {"actuation_enabled": True, "stop_file": "stop.flag",
                       "allowed_applications": ["explorer.exe"],
                       "allowed_window_classes": {"explorer.exe": ["CabinetWClass"]},
                       "window_pin_file": "window.pin"},
            "models": {"accessibility": "uia"}}


def good_editor_config():
    return {"safety": {"actuation_enabled": True, "stop_file": "stop.flag",
                       "allowed_applications": ["ommatix-testeditor.exe"]},
            "models": {"accessibility": "uia"}}


class ConfigProblemsTest(unittest.TestCase):
    def test_the_configurations_that_fit_have_no_problem(self):
        self.assertEqual(dossier.config_problems("explorer", good_explorer_config()), [])
        self.assertEqual(dossier.config_problems("editor", good_editor_config()), [])

    def test_explorer_needs_the_class_the_pin_and_nothing_else_allowed(self):
        def with_safety(**changes):
            config = good_explorer_config()
            config["safety"].update(changes)
            return dossier.config_problems("explorer", config)

        self.assertTrue(with_safety(allowed_window_classes={}), "no class restriction")
        self.assertTrue(with_safety(allowed_window_classes={"explorer.exe": ["Progman"]}))
        self.assertTrue(with_safety(
            allowed_window_classes={"explorer.exe": ["CabinetWClass", "WorkerW"]}))
        self.assertTrue(with_safety(window_pin_file=""), "no pin")
        self.assertTrue(with_safety(allowed_applications=["explorer.exe", "notepad.exe"]))
        self.assertTrue(with_safety(allowed_applications=["notepad.exe"]))
        self.assertTrue(with_safety(actuation_enabled=False))
        self.assertTrue(with_safety(stop_file=""))
        frame = with_safety(allowed_applications=["explorer.exe", "applicationframehost.exe"])
        self.assertTrue([p for p in frame if "applicationframehost.exe" in p])
        config = good_explorer_config()
        config["models"] = {}
        self.assertTrue(dossier.config_problems("explorer", config), "no UI Automation")

    def test_the_editor_run_allows_only_the_editor_and_pins_nothing(self):
        def with_safety(**changes):
            config = good_editor_config()
            config["safety"].update(changes)
            return dossier.config_problems("editor", config)

        self.assertTrue(with_safety(allowed_applications=["notepad.exe"]))
        self.assertTrue(with_safety(
            allowed_applications=["ommatix-testeditor.exe", "explorer.exe"]))
        self.assertTrue(with_safety(window_pin_file="window.pin"))
        self.assertTrue(with_safety(
            allowed_applications=["ommatix-testeditor.exe", "applicationframehost.exe"]))

    def test_the_pin_file_is_resolved_like_the_engine_does(self):
        config = good_explorer_config()
        self.assertEqual(dossier.pin_file_path(Path(r"C:\run\live.toml"), config),
                         Path(r"C:\run") / "window.pin")
        config["safety"]["window_pin_file"] = r"D:\pins\w.pin"
        self.assertEqual(dossier.pin_file_path(Path(r"C:\run\live.toml"), config),
                         Path(r"D:\pins\w.pin"))
        config["safety"]["window_pin_file"] = ""
        self.assertIsNone(dossier.pin_file_path(Path(r"C:\run\live.toml"), config))


# ---- the flows --------------------------------------------------------------------------------


class FlowTest(unittest.TestCase):
    def test_the_editor_flow_types_the_synthetic_text_into_the_field_and_clicks_save(self):
        flow = dossier.editor_flow()
        self.assertEqual(flow["exe"], "ommatix-testeditor.exe")
        self.assertEqual([(s[0], len(s)) for s in flow["steps"]],
                         [("Editor de texto", 4), ("Guardar", 3)])
        self.assertEqual(flow["steps"][0][3], "prueba 123")
        self.assertEqual(flow["steps"][0][3], driver.TYPED_TEXT)
        # The OS-side states: the title and the folder, never the screen's text.
        self.assertEqual(flow["steps"][0][2],
                         "Ommatix S4 editor [caracteres=10 guardado=no]|file:absent")
        self.assertEqual(flow["steps"][1][2],
                         "Ommatix S4 editor [caracteres=10 guardado=si]|file:ok")

    def test_the_explorer_flow_is_up_up_back_as_three_single_clicks(self):
        flow = dossier.explorer_flow("run1")
        self.assertTrue(dossier.is_click_only(flow))
        self.assertEqual([str(s[0]) for s in flow["steps"]], ["carpeta-a", "run1", "Atrás"])
        self.assertEqual([s[2] for s in flow["steps"]],
                         [("carpeta-a",), (), ("carpeta-a",)])
        self.assertTrue(all(len(s) == 3 for s in flow["steps"]), "no typing step")
        self.assertEqual(flow["exe"], "explorer.exe")

    def test_a_flow_with_a_typing_step_is_not_click_only(self):
        flow = dossier.explorer_flow("run1")
        flow["steps"].append(("carpeta-a", driver.NOT_BY_OCR, (), "x"))
        self.assertFalse(dossier.is_click_only(flow))

    def test_the_navigation_targets_name_their_control_types_and_the_roles_they_stand_for(self):
        a, root, back = (s[0] for s in dossier.explorer_flow("run1")["steps"])
        self.assertEqual(a.control_types, ("Button", "SplitButton"))
        self.assertEqual(root.control_types, ("Button", "SplitButton"))
        self.assertEqual(back.control_types, ("Button",))
        for target in (a, root, back):
            self.assertEqual(target.roles, ("button",))
            self.assertNotIn("TreeItem", target.control_types)
            self.assertNotIn("ListItem", target.control_types)
        self.assertEqual(a, "carpeta-a")  # still a plain string for the shared code

    def test_a_list_item_or_a_tree_item_stands_for_the_unknown_role_only(self):
        self.assertEqual(dossier.NavLabel("x", ("ListItem",), "z").roles, ("unknown",))
        self.assertEqual(dossier.NavLabel("x", ("TreeItem",), "z").roles, ("unknown",))

    def test_the_back_button_name_and_identifier_are_parameters(self):
        flow = dossier.explorer_flow("run1", "Back", "BackButton")
        back = flow["steps"][2][0]
        self.assertEqual((str(back), back.automation_id), ("Back", "BackButton"))
        self.assertIsNone(dossier.explorer_flow("run1")["steps"][2][0].automation_id)


class NoteStateTest(QuietTest):
    def test_the_folder_as_the_os_shows_it(self):
        folder = self.tmp / "run"
        folder.mkdir()
        want = b"prueba 123"
        self.assertEqual(dossier.note_state(folder, want), "absent")
        (folder / "nota.txt").write_bytes(want)
        self.assertEqual(dossier.note_state(folder, want), "ok")
        (folder / "nota.txt").write_bytes(want + b"\n")
        self.assertEqual(dossier.note_state(folder, want), "mismatch:11")
        (folder / "nota.txt").write_bytes(b"\xef\xbb\xbf" + want)
        self.assertEqual(dossier.note_state(folder, want), "mismatch:13", "a BOM is a mismatch")
        (folder / "otro.txt").write_bytes(b"x")
        self.assertEqual(dossier.note_state(folder, want), "extra")
        (folder / "otro.txt").unlink()
        (folder / "nota.txt").unlink()
        (folder / "nota.txt").mkdir()
        self.assertEqual(dossier.note_state(folder, want), "not_a_file")
        self.assertEqual(dossier.note_state(self.tmp / "missing", want), "unreadable")


# ---- the editor run ---------------------------------------------------------------------------


def row(rid, aid, ctype):
    return {"runtime_id": rid, "automation_id": aid, "control_type": ctype, "offscreen": False}


class FakeEditorDesk(dossier.DossierProtocol):
    """A Protocol on a fake editor: it records what the engine's gate would have delivered and
    behaves like the real editor, which writes ``nota.txt`` into the folder it was started on."""

    def __init__(self, tmp, temp_root, max_live=10, mode="run"):
        self.args = SimpleNamespace(
            app="editor", max_live=max_live, settle=0.0, results=tmp / "results.jsonl",
            debug_grounding=False, restart="each", mode=mode, editor_exe="editor.exe",
            explorer_launch="root", foreground_wait=1.0, foreground_wait_explorer=1.0,
            first_run=1, runs=1, pause=0.0)
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
        self.flow = dossier.editor_flow()
        self.temp_root = temp_root
        self.pin_file = None
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        self.root_name = None
        # the fake application
        self.spawned = []
        self.folder = None
        self.typed = ""
        self.saved = False
        self.drop_keys = False
        self.save_writes = None  # bytes the "application" writes instead of the typed text
        self.extra_file = False
        self.start_dirty = False
        self.closed = []
        self.actions = []
        self.ambiguous = False
        self.centre_of = None
        self.pre_chain = None  # what the point query returns before a click, instead of the target
        self.under = 77  # the top-level window under the target point

    # the engine
    def observe(self):
        elements = [
            {"label": "Editor de texto", "role": "input", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "field", "revision": 1,
             "zone": "center_main"},
            {"label": "Guardar", "role": "button", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "save", "revision": 1,
             "zone": "center_main"},
        ]
        status = "guardado" if self.saved else "pendiente"
        elements.append({"label": f"Estado: {status}", "role": "text_node",
                         "actionability": "unknown", "label_source": "ocr",
                         "target_id": "status", "revision": 1, "zone": "center_main"})
        if self.ambiguous:
            elements.append(dict(elements[1], target_id="save2"))
        return {"session_id": "s", "snapshot_id": "s1", "geometry_epoch": 1}, elements, {}

    def call(self, tool, args):
        assert tool == "execute_action" and args["dry_run"] is False
        self.actions.append({"target": args["target_id"], "action": args["action"],
                             "text": args.get("text")})
        if args["action"] == "type":
            assert args["target_id"] == "field"
            if not self.drop_keys:
                self.typed = args["text"]
        else:
            assert args["target_id"] == "save"
            data = self.save_writes if self.save_writes is not None else self.typed.encode()
            (self.folder / "nota.txt").write_bytes(data)
            if self.extra_file:
                (self.folder / "other.txt").write_bytes(b"x")
            self.saved = True
        out = {"status": "effect_observed", "input_delivered": True, "verified": True,
               "observed_effect": "visual_change"}
        return out, False, {}, 5.0

    def identify(self, hwnd, label, control_types):
        table = {"Editor de texto": row("7.1", "Field", "Edit"),
                 "Guardar": row("7.2", "Save", "Button")}
        found = table.get(label)
        return [found] if found and found["control_type"] in control_types else []

    def element_centre(self, hwnd, label, control_types):
        self.centre_of = label
        return (100, 200)

    def point_chain(self, x, y):  # the pre-click read: the control about to be clicked
        if self.pre_chain is not None:
            return self.pre_chain
        return [self.identify(77, self.centre_of, ("Edit", "Button"))[0]]

    def island_walk(self, x, y, host_runtime_id):
        return None

    def window_under(self, point):
        return self.under

    def cursor_chain(self):
        target = self.actions[-1]["target"]
        return [row("7.1", "Field", "Edit") if target == "field" else row("7.2", "Save", "Button")]

    # the OS
    def spawn(self, argv):
        self.spawned.append(argv)
        self.folder = Path(argv[argv.index("--folder") + 1])
        if self.start_dirty:
            (self.folder / "leftover.txt").write_bytes(b"x")
        return 5151

    def wait_foreground(self, pid):
        return True, 0

    def current_hwnd(self):
        return 77

    def require_foreground(self):
        return {"exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 77, "maximized_primary": True}

    def title_of(self, hwnd):
        return dossier.editor_title(len(self.typed), self.saved and self.typed != "")

    def close(self, pid):
        if pid is not None and pid in self.started:
            self.closed.append(pid)
            self.started.discard(pid)


class EditorRunTest(QuietTest):
    def desk(self, **kwargs):
        return FakeEditorDesk(self.tmp, self.temp_root, **kwargs)

    def test_the_editor_run_types_the_text_saves_and_leaves_nothing_behind(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual(desk.actions, [
            {"target": "field", "action": "type", "text": "prueba 123"},
            {"target": "save", "action": "click", "text": None}])
        self.assertEqual((res["independent_checks"], res["independent_failed"]), (2, 0))
        self.assertEqual((res["engine_false_success"], res["grounding_wrong"]), (0, 0))
        self.assertEqual(res["note_state"], "ok")
        self.assertEqual(res["e2e"], "E2E-02-plan")
        # The editor was started on the run folder under the temporary directory, and only that.
        self.assertEqual(desk.spawned, [["editor.exe", "--folder", str(desk.folder)]])
        self.assertEqual(desk.folder.parent, self.temp_root / "ommatix-e2e")
        self.assertEqual(desk.closed, [5151])
        # Cleanup: the note and the folder are gone, and so is the root this client made.
        self.assertEqual(res["cleanup"]["left"], [])
        self.assertFalse((self.temp_root / "ommatix-e2e").exists())

    def test_the_written_bytes_are_checked_against_the_typed_text(self):
        desk = self.desk()
        desk.save_writes = b"prueba 12"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["independent_failed"], 1)
        self.assertEqual(res["engine_false_success"], 1, "effect_observed with a wrong file")
        self.assertEqual(res["note_state"], "mismatch:9")
        self.assertFalse(desk.folder.exists(), "the client's folder is cleaned up all the same")

    def test_keys_that_never_arrive_stop_the_run_before_save(self):
        desk = self.desk()
        desk.drop_keys = True
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual([a["action"] for a in desk.actions], ["type"], "Guardar was not clicked")
        self.assertEqual(desk.ledger_total(), 1)

    def test_a_point_that_is_not_the_field_sends_no_request_and_the_editor_never_navigates(self):
        # ADR-0022 rule 1 applies to every flow; the editor has no navigating step.
        self.assertNotIn("navigating", dossier.editor_flow())
        for setup in ({"pre_chain": [row("7.2", "Save", "Button")]}, {"under": 5}):
            desk = FakeEditorDesk(self.tmp, self.temp_root)
            for key, value in setup.items():
                setattr(desk, key, value)
            res = desk.run_once(1)
            self.assertFalse(res["success"], setup)
            self.assertEqual((desk.actions, desk.ledger_total()), ([], 0), setup)
            self.assertEqual(res["pre_grounded"][0]["grounded"], False)

    def test_an_editor_click_that_reached_another_control_is_still_grounding_wrong(self):
        desk = FakeEditorDesk(self.tmp, self.temp_root)
        desk.cursor_chain = lambda: [row("7.9", "Other", "Button")]
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["grounding"], [False])
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertNotIn("post_grounded", res)
        self.assertEqual(len(desk.actions), 1)

    def test_a_second_file_in_the_folder_fails_the_run_and_is_never_deleted(self):
        desk = self.desk()
        desk.extra_file = True
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["note_state"], "extra")
        self.assertTrue((desk.folder / "other.txt").exists(), "an unrecorded file stays")
        self.assertIn(desk.folder.name, res["cleanup"]["left"])

    def test_an_editor_that_does_not_start_clean_gets_no_input(self):
        desk = self.desk()
        desk.start_dirty = True
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("does not start empty", res["failure"])
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))

    def test_an_ambiguous_target_aborts_but_the_cleanup_still_happens(self):
        desk = self.desk()
        desk.ambiguous = True
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual(desk.actions[:1], [{"target": "field", "action": "type",
                                             "text": "prueba 123"}])
        self.assertEqual(desk.closed, [5151])
        self.assertFalse(desk.folder.exists())
        recorded = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertIn("ambiguous", recorded["aborted"])

    def test_the_probe_sends_nothing(self):
        desk = self.desk(mode="probe")
        desk.probe_print = lambda elements: None
        res = desk.run_once(1)
        self.assertEqual((desk.actions, desk.ledger_total(), res["steps"]), ([], 0, 0))
        self.assertEqual(desk.closed, [5151])
        self.assertFalse(desk.folder.exists())

    def test_the_live_budget_still_applies(self):
        desk = self.desk(max_live=1)
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual(len(desk.actions), 1)


# ---- the Explorer run -------------------------------------------------------------------------

OURS, OWNERS, DESKTOP = 900, 901, 4


class FakeExplorerDesk(dossier.DossierProtocol):
    """A Protocol on a fake Explorer window that starts on ``carpeta-b``: the breadcrumb (a
    Button per parent folder), the Back button, and the decoys that share the targets' names
    with another control type and role (a tab title, a navigation-pane tree item, a file-list
    item). The engine gives every navigation element zone ``unknown``, as in the ninth round.
    The owner's own Explorer window sits next to the one the client opened. It records the clicks
    the gate would have delivered."""

    def __init__(self, tmp, temp_root, pin_file, max_live=10, mode="run"):
        self.args = SimpleNamespace(
            app="explorer", max_live=max_live, settle=0.0, results=tmp / "results.jsonl",
            debug_grounding=False, restart="each", mode=mode, editor_exe="editor.exe",
            explorer_launch="e", foreground_wait=1.0, foreground_wait_explorer=1.0,
            first_run=1, runs=1, pause=0.0)
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
        self.flow = dossier.explorer_flow("(set per run)")
        self.temp_root = temp_root
        self.pin_file = pin_file
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        self.root_name = None
        # the fake desktop
        self.root = None
        self.start = None
        self.location = None  # the folder of our window, as a path
        self.history = []  # the folders Back returns to
        self.fg_hwnd = OURS
        self.under = OURS  # the top-level window under the click point
        self.windows = {OURS, OWNERS}
        self.closed_windows = []
        self.stubborn = False
        self.reused_handle = False
        self.killed = []
        self.pin_seen = []
        self.opened_with = []
        self.clicks = []  # (target_id, label, zone)
        self.cursor = None
        self.twin_crumb = False
        self.navigate = True
        self.damage = None  # a function that damages the tree after the nth click
        self.on_screen = True
        self.front_ok = True
        self.centre_of = None
        self.pre_chain = None  # what the point query returns before a click, instead of the target

    def folder_name(self):
        return self.location.name

    # the layout of the window: engine elements by target id
    def elements(self):
        def el(label, role, target_id, zone="unknown"):
            return {"label": label, "role": role, "actionability": "confirmed",
                    "label_source": "accessibility", "target_id": target_id, "revision": 1,
                    "zone": zone}

        root = self.root_name or ""
        found = [
            el(self.folder_name(), "unknown", "tab"),  # the tab title: the current folder
            el("carpeta-a", "unknown", "tree-a"),  # the navigation pane, rooted at the tree
            el(root, "unknown", "tree-root"),
            el("Atrás", "button", "back"),
        ]
        if self.location == self.root:
            found.append(el("carpeta-a", "unknown", "list-a"))  # the file list
        if self.location == self.root / "carpeta-a":
            found.append(el("carpeta-b", "unknown", "list-b"))
        if self.location is not None and self.location != self.root:
            found.append(el(root, "button", "crumb-root"))
        if self.location == self.root / "carpeta-a" / "carpeta-b":
            found.append(el("carpeta-a", "button", "crumb-a"))
            if self.twin_crumb:
                found.append(el("carpeta-a", "button", "crumb-a-twin"))
        return found

    def observe(self):
        return ({"session_id": "s", "snapshot_id": "s1", "geometry_epoch": 1},
                self.elements(), {})

    def go(self, where):
        if where != self.location:
            self.history.append(self.location)
            self.location = where

    def call(self, tool, args):
        assert tool == "execute_action" and args["dry_run"] is False
        assert args["action"] == "click", "the Explorer flow only clicks"
        target = next(e for e in self.elements() if e["target_id"] == args["target_id"])
        self.clicks.append((target["target_id"], target["label"], target["zone"]))
        self.pin_seen.append(self.pin_file.read_text() if self.pin_file.exists() else None)
        self.cursor = target["target_id"]
        if self.navigate:
            if target["target_id"] == "crumb-a":
                self.go(self.root / "carpeta-a")
            elif target["target_id"] == "crumb-root":
                self.go(self.root)
            elif target["target_id"] == "back" and self.history:
                self.location = self.history.pop()
        if self.damage is not None:
            self.damage(len(self.clicks))
        out = {"status": "effect_observed", "input_delivered": True, "verified": True,
               "observed_effect": "visual_change"}
        return out, False, {}, 5.0

    # UI Automation: (name, control type) -> row
    def uia_rows(self):
        root = self.root_name or ""
        rows = {("carpeta-a", "TreeItem"): row("9.1", "TreeA", "TreeItem"),
                (root, "TreeItem"): row("9.6", "TreeRoot", "TreeItem"),
                ("Atrás", "Button"): row("9.7", "BackButton", "Button"),
                (self.folder_name(), "TabItem"): row("9.2", "Tab", "TabItem")}
        if self.location == self.root:
            rows[("carpeta-a", "ListItem")] = row("9.3", "ListA", "ListItem")
        if self.location == self.root / "carpeta-a":
            rows[("carpeta-b", "ListItem")] = row("9.4", "ListB", "ListItem")
        if self.location != self.root:
            rows[(root, "SplitButton")] = row("9.9", "CrumbRoot", "SplitButton")
        if self.location == self.root / "carpeta-a" / "carpeta-b":
            rows[("carpeta-a", "Button")] = row("9.5", "CrumbA", "Button")
        return rows

    def identify(self, hwnd, label, control_types):
        assert hwnd == OURS, "UI Automation reads only the recorded window"
        found = [dict(r) for (name, ctype), r in self.uia_rows().items()
                 if name == str(label) and ctype in control_types]
        if self.twin_crumb and str(label) == "carpeta-a" and "Button" in control_types:
            found.append(row("9.8", "CrumbATwin", "Button"))
        return found

    def element_centre(self, hwnd, label, control_types):
        self.centre_of = (str(label), tuple(control_types))
        return (100, 200) if self.on_screen else None

    def point_chain(self, x, y):  # the pre-click read: the control about to be clicked
        if self.pre_chain is not None:
            return self.pre_chain
        label, types = self.centre_of
        return [self.identify(OURS, label, types)[0]]

    def island_walk(self, x, y, host_runtime_id):
        return None

    def cursor_chain(self):
        by_target = {"tree-a": row("9.1", "TreeA", "TreeItem"),
                     "tree-root": row("9.6", "TreeRoot", "TreeItem"),
                     "tab": row("9.2", "Tab", "TabItem"),
                     "list-a": row("9.3", "ListA", "ListItem"),
                     "list-b": row("9.4", "ListB", "ListItem"),
                     "crumb-a": row("9.5", "CrumbA", "Button"),
                     "crumb-root": row("9.9", "CrumbRoot", "SplitButton"),
                     "back": row("9.7", "BackButton", "Button")}
        return [by_target[self.cursor]]

    # the OS
    def open_explorer(self, start):
        self.opened_with.append(start)
        self.start = start
        self.root = start.parent.parent
        self.location = start
        self.history = []
        return OURS

    def location_of(self, hwnd):
        return str(self.location) if hwnd == OURS else None

    def front_window(self):
        return {"exe": "explorer.exe", "pid": 11, "hwnd": self.fg_hwnd, "maximized_primary": True}

    def bring_front(self, hwnd):
        pass

    def wait_explorer_front(self, hwnd):
        return self.front_ok

    def window_under(self, point):
        return self.under

    def window_exists(self, hwnd):
        return hwnd in self.windows

    def is_our_explorer_window(self, hwnd):
        return not self.reused_handle

    def post_close(self, hwnd):
        self.closed_windows.append(hwnd)
        if not self.stubborn:
            self.windows.discard(hwnd)

    def close(self, pid):  # the process kill of the base protocol must never be used
        self.killed.append(pid)


class ExplorerRunTest(QuietTest):
    def setUp(self):
        super().setUp()
        self.pin = self.tmp / "window.pin"

    def desk(self, **kwargs):
        return FakeExplorerDesk(self.tmp, self.temp_root, self.pin, **kwargs)

    def test_the_window_opens_directly_on_carpeta_b(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertEqual(len(desk.opened_with), 1)
        self.assertEqual(desk.opened_with[0].parts[-2:], ("carpeta-a", "carpeta-b"))
        self.assertTrue(res["success"], res)

    def test_the_run_goes_up_twice_then_back_and_checks_the_folder_after_each_click(self):
        desk = self.desk()
        folders = []
        original = desk.call

        def spy(tool, args):
            out = original(tool, args)
            folders.append(dossier.relative_parts(desk.root, str(desk.location)))
            return out

        desk.call = spy
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual([(c[1], c[0]) for c in desk.clicks],
                         [("carpeta-a", "crumb-a"), (desk.root_name, "crumb-root"),
                          ("Atrás", "back")])
        self.assertEqual(folders, [("carpeta-a",), (), ("carpeta-a",)])
        self.assertEqual((res["independent_checks"], res["independent_failed"]), (3, 0))
        self.assertEqual(res["grounding_wrong"], 0)
        self.assertEqual([(p["step"], p["grounded"], p["status"]) for p in res["pre_grounded"]],
                         [(1, True, "ok"), (2, True, "ok"), (3, True, "ok")])
        self.assertEqual([(p["step"], p["identity_hit"], p["os_check"])
                          for p in res["post_grounded"]],
                         [(1, True, True), (2, True, True), (3, True, True)])
        self.assertEqual((res["actions_requested"], res["delivered"]), (3, 3))
        self.assertTrue(res["tree_unchanged"])
        self.assertEqual(res["e2e"], "E2E-03-plan")

    def test_the_targets_are_chosen_by_control_type_and_role_never_by_zone(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual({c[2] for c in desk.clicks}, {"unknown"},
                         "the engine's zone was unknown for every target and nothing depended on it")
        for target in res["targets"]:
            self.assertEqual(target["roles_expected"], ["button"])
            self.assertEqual(target["zones"], {"unknown": 1}, "recorded as an observation")
            self.assertGreaterEqual(target["candidates"], 1, "decoys of other roles count here")

    def test_the_decoys_with_the_same_name_are_never_clicked(self):
        desk = self.desk()
        desk.run_once(1)
        clicked = {c[0] for c in desk.clicks}
        self.assertEqual(clicked, {"crumb-a", "crumb-root", "back"})
        self.assertFalse(clicked & {"tab", "tree-a", "tree-root", "list-a", "list-b"})

    def test_the_identity_is_looked_up_among_buttons_never_among_items_or_tabs(self):
        desk = self.desk()
        seen = []
        original = desk.identify

        def spy(hwnd, label, control_types):
            seen.append((str(label), tuple(control_types)))
            return original(hwnd, label, control_types)

        desk.identify = spy
        res = desk.run_once(1)
        self.assertIn(("carpeta-a", ("Button", "SplitButton")), seen)
        self.assertIn(("Atrás", ("Button",)), seen)
        self.assertEqual({t for _, types in seen for t in types}, {"Button", "SplitButton"})
        self.assertEqual([i["control_type"] for i in res["identity"]],
                         ["Button", "SplitButton", "Button"])
        self.assertEqual([i["runtime_id"] for i in res["identity"]], ["9.5", "9.9", "9.7"])

    def test_two_buttons_with_the_name_stop_the_protocol_before_any_click(self):
        desk = self.desk()
        desk.twin_crumb = True
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(desk.closed_windows, [OURS], "the window it opened is still closed")

    def test_two_ui_automation_buttons_with_the_name_send_nothing(self):
        desk = self.desk()
        original = desk.identify
        desk.identify = lambda hwnd, label, types: (
            original(hwnd, label, types) + [row("9.8", "Twin", "Button")])
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("ambiguous", res["failure"])
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))

    def test_the_back_button_may_be_required_to_carry_its_automation_id(self):
        desk = self.desk()
        desk.args.explorer_back_id = "BackButton"
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual(res["identity"][2]["automation_id"], "BackButton")
        other = self.desk()
        other.args.explorer_back_id = "NotTheBackButton"
        res = other.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(other.clicks), 2, "the third click was not sent")
        self.assertIn("not_found", res["failure"])

    def test_the_back_button_name_is_an_option(self):
        desk = self.desk()
        desk.args.explorer_back_name = "Back"
        res = desk.run_once(1)
        self.assertFalse(res["success"], "a Back button under another name is not found")
        self.assertEqual(len(desk.clicks), 2)

    def test_the_pin_holds_the_recorded_handle_while_clicking_and_is_gone_after(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertEqual(desk.pin_seen, ["900", "900", "900"])
        self.assertFalse(self.pin.exists())
        self.assertTrue(res["pin_written"])

    def test_only_the_window_the_client_opened_is_closed_and_no_process_is_killed(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertEqual(desk.closed_windows, [OURS])
        self.assertIn(OWNERS, desk.windows, "the owner's Explorer window stays open")
        self.assertEqual(desk.killed, [])
        self.assertTrue(res["window_closed"])

    def test_the_tree_is_deleted_and_only_the_tree(self):
        desk = self.desk()
        bystander = self.temp_root / "ommatix-e2e" / "owners-run"
        bystander.mkdir(parents=True)
        (bystander / "keep.txt").write_bytes(b"keep")
        res = desk.run_once(1)
        self.assertFalse(desk.root.exists())
        self.assertEqual(res["cleanup"]["deleted_files"], 2)
        self.assertEqual(res["cleanup"]["deleted_dirs"], 3)
        self.assertEqual((bystander / "keep.txt").read_bytes(), b"keep")

    def test_the_owners_own_explorer_window_in_front_stops_the_protocol_before_any_click(self):
        desk = self.desk()
        desk.fg_hwnd = OWNERS
        with self.assertRaises(driver.Abort) as raised:
            desk.run_once(1)
        self.assertIn("not the Explorer window the client opened", str(raised.exception))
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(desk.closed_windows, [OURS])

    def test_a_click_point_that_lies_on_the_desktop_or_the_taskbar_sends_nothing(self):
        for other in (DESKTOP, OWNERS, 5):
            desk = self.desk()
            desk.under = other
            res = desk.run_once(1)
            self.assertFalse(res["success"], other)
            self.assertIn("outside_recorded_window", res["failure"], other)
            self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0), other)

    def test_a_target_whose_position_cannot_be_read_is_not_clicked(self):
        desk = self.desk()
        desk.on_screen = False
        res = desk.run_once(1)
        self.assertEqual((desk.clicks, res["success"]), ([], False))

    def test_a_safety_event_in_run_1_of_3_sends_nothing_more_and_engages_the_stop(self):
        desk = self.desk()
        desk.args.runs = 3
        desk.stop_file = self.tmp / "stop.flag"
        desk.metrics = lambda: {"delivered_actions": 0, "rejected_actions": 0}
        desk.navigate = False  # the first click does nothing: grounding_wrong, false success
        with self.assertRaises(driver.Abort):
            desk.run()
        self.assertEqual(len(desk.clicks), 1, "no click of run 1 beyond the first, none of 2 or 3")
        self.assertEqual(len(desk.results), 1)
        self.assertTrue(desk.stop_file.exists())
        self.assertEqual(desk.results[0]["safety_stop"], "grounding_wrong")
        self.assertEqual(desk.closed_windows, [OURS], "the window it opened is still closed")

    def test_a_plain_failure_lets_the_next_explorer_run_start(self):
        desk = self.desk()
        desk.args.runs = 2
        desk.stop_file = self.tmp / "stop.flag"
        desk.metrics = lambda: {"delivered_actions": 0, "rejected_actions": 0}
        desk.front_ok = False  # the window never arrives: a failure, no input, no safety event
        desk.run()
        self.assertEqual(len(desk.results), 2)
        self.assertFalse(desk.stop_file.exists())

    def test_a_click_that_does_not_navigate_ends_the_run_with_no_further_input(self):
        desk = self.desk()
        desk.navigate = False
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(desk.clicks), 1, "the second click was not sent")
        self.assertEqual(res["independent_failed"], 1)
        self.assertEqual(res["engine_false_success"], 1, "effect_observed, folder unchanged")
        self.assertEqual(res["grounding_wrong"], 1, "the OS-side check is the grounding")

    def test_a_back_that_does_not_return_to_carpeta_a_fails_the_run(self):
        desk = self.desk()
        desk.history = []
        original = desk.call

        def call(tool, args):
            if args["target_id"] == "back":
                desk.navigate = False
            return original(tool, args)

        desk.call = call
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(desk.clicks), 3)
        self.assertEqual(res["independent_failed"], 1)

    # ---- ADR-0022: the grounding rule for clicks that rebuild their own control -------------
    def test_a_navigating_click_whose_control_was_recreated_is_grounded_by_the_os_check(self):
        # The tenth round's pilot: the folder changed, and the control under the cursor
        # afterwards is another one (the breadcrumb was drawn again). The OS side decides.
        desk = self.desk()
        desk.cursor_chain = lambda: [row("9.2", "Tab", "TabItem")]
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual(len(desk.clicks), 3)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (0, 0))
        self.assertEqual([p["identity_hit"] for p in res["post_grounded"]], [False] * 3)
        self.assertEqual([p["os_check"] for p in res["post_grounded"]], [True] * 3)
        self.assertNotIn("grounding", res, "no click needed its identity after the click")

    def test_a_navigating_click_whose_os_check_fails_is_grounding_wrong_and_stops(self):
        desk = self.desk()
        desk.navigate = False
        desk.cursor_chain = lambda: [row("9.2", "Tab", "TabItem")]
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(desk.clicks), 1, "no further request after the failed check")
        self.assertEqual(desk.ledger_total(), 1)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertEqual(res["post_grounded"][0]["os_check"], False)
        self.assertIn("navigating click", res["failure"])

    def test_a_navigating_click_that_reached_the_same_control_still_needs_the_os_check(self):
        # Identity alone is not the criterion for these steps either way round.
        desk = self.desk()
        desk.navigate = False  # the cursor chain is the right control, the folder did not change
        res = desk.run_once(1)
        self.assertEqual(res["post_grounded"][0]["identity_hit"], True)
        self.assertEqual((res["success"], res["grounding_wrong"]), (False, 1))

    def test_a_step_that_is_not_on_the_list_still_needs_its_identity_after_the_click(self):
        # The list is closed: with Explorer's steps off it, the same changed identity is the
        # old grounding_wrong, even though the folder did change.
        with mock.patch.object(driver, "NAVIGATING_STEPS", MappingProxyType({"explorer": ()})):
            desk = self.desk()
            desk.cursor_chain = lambda: [row("9.2", "Tab", "TabItem")]
            res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(desk.clicks), 1)
        self.assertEqual((res["grounding_wrong"], res["engine_false_success"]), (1, 1))
        self.assertEqual(res["grounding"], [False])
        self.assertNotIn("post_grounded", res)
        self.assertIn("not the element", res["failure"])

    def test_a_pre_click_mismatch_sends_no_request(self):
        for name, chain in (("another control", [row("9.2", "Tab", "TabItem")]),
                            ("an empty chain", [])):
            desk = self.desk()
            desk.pre_chain = chain
            res = desk.run_once(1)
            self.assertFalse(res["success"], name)
            self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0), name)
            self.assertEqual(res["actions_requested"], 0, name)
            self.assertEqual(res["pre_grounded"][0]["status"], "identity_mismatch", name)
            self.assertIn("not grounded", res["failure"], name)

    def test_a_pre_click_window_that_is_not_the_foreground_one_sends_no_request(self):
        desk = self.desk()
        asked = []

        def under(point):  # the identity lookup asks first, the pre-click read second
            asked.append(point)
            return OURS if len(asked) == 1 else OWNERS

        desk.window_under = under
        res = desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(res["pre_grounded"][0]["status"], "window_mismatch")
        self.assertIs(res["pre_grounded"][0]["window_ok"], False)

    def test_a_pre_click_read_that_fails_sends_no_request(self):
        desk = self.desk()

        def broken(x, y):
            raise subprocess.TimeoutExpired("powershell", 30)

        desk.point_chain = broken
        res = desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(res["pre_grounded"][0]["status"], "unreadable")

    def test_the_pre_click_grounding_immediately_precedes_every_request(self):
        desk = self.desk()
        order = []
        point_chain, call = desk.point_chain, desk.call
        desk.point_chain = lambda x, y: (order.append("point"), point_chain(x, y))[1]
        desk.call = lambda tool, args: (order.append("request"), call(tool, args))[1]
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual(order.count("request"), 3)
        for index, event in enumerate(order):
            if event == "request":
                self.assertEqual(order[index - 1], "point")

    def test_the_records_of_the_rule_hold_no_name_and_no_runtime_id(self):
        desk = self.desk()
        desk.cursor_chain = lambda: [row("9.2", "Tab", "TabItem")]
        res = desk.run_once(1)
        text = json.dumps({k: res[k] for k in ("pre_grounded", "post_grounded")},
                          ensure_ascii=False)
        for secret in ("carpeta-a", "carpeta-b", "Atrás", desk.root_name, "9.5", "9.9", "9.7",
                       "9.2"):
            self.assertNotIn(secret, text)
        for key in ('"name"', '"runtime_id"', '"rect"'):
            self.assertNotIn(key, text)

    def test_the_list_of_navigating_steps_is_closed_and_not_a_command_line_option(self):
        self.assertEqual(dict(driver.NAVIGATING_STEPS), {"explorer": (1, 2, 3)})
        with self.assertRaises(TypeError):
            driver.NAVIGATING_STEPS["calculator"] = (1,)
        self.assertEqual(dossier.explorer_flow("r")["navigating"], (1, 2, 3))
        for flow in (dossier.editor_flow(), driver.calculator_flow(), driver.notepad_flow(),
                     driver.testapp_flow(), driver.single_click_flow("x")):
            self.assertNotIn("navigating", flow)
        for main, app in ((dossier.main, "explorer"), (driver.main, "calculator")):
            for option in ("--navigating-steps", "--navigating"):
                argv = ["prog", "--exe", "x", "--config", "c", "--app", app, "--ledger", "l",
                        "--results", "r", option, "1,2"]
                with mock.patch.object(sys, "argv", argv), \
                        contextlib.redirect_stderr(io.StringIO()), \
                        self.assertRaises(SystemExit) as raised:
                    main()
                self.assertEqual(raised.exception.code, 2, (app, option))

    def test_a_navigating_step_without_an_os_check_sends_nothing(self):
        desk = self.desk()
        label = dossier.NavLabel("carpeta-a", dossier.BREADCRUMB_TYPES, "top_bar")
        desk.flow = {"exe": "explorer.exe", "initial": driver.NOT_BY_OCR, "vocabulary": set(),
                     "navigating": (1,), "steps": [(label, driver.NOT_BY_OCR, None)]}
        desk.root_name = "r"
        desk.location = Path("x") / "carpeta-a" / "carpeta-b"
        desk.root = Path("x")
        res = driver.new_result("E2E-03-plan", 1)
        snap, elements, _ = desk.observe()
        desk.steps(res, snap, elements, OURS, None)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertIn("OS-side check", res["failure"])

    def test_the_s4_document_names_the_closed_list(self):
        doc = (HERE.parents[1] / "docs" / "thesis" / "S4_ACEPTACION_REDUCIDA.md").read_text(
            encoding="utf-8")
        self.assertIn('NAVIGATING_STEPS = {"explorer": (1, 2, 3)}', doc)

    def test_a_tree_that_changed_fails_the_run(self):
        desk = self.desk()

        def damage(clicks):
            if clicks == 3:
                (desk.root / "leeme.txt").write_bytes(b"changed on disk")

        desk.damage = damage
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertFalse(res["tree_unchanged"])
        self.assertIn("tree changed", res["failure"])

    def test_a_window_that_does_not_close_keeps_the_tree_for_the_operator(self):
        desk = self.desk()
        desk.stubborn = True
        res = desk.run_once(1)
        self.assertFalse(res["window_closed"])
        self.assertTrue(desk.root.is_dir(), "the tree an open window may show is not deleted")
        self.assertEqual(res["cleanup"]["left"], [desk.root.name])
        self.assertEqual(desk.killed, [])

    def test_a_handle_that_is_no_longer_an_explorer_window_is_not_closed(self):
        desk = self.desk()
        desk.reused_handle = True
        res = desk.run_once(1)
        self.assertEqual(desk.closed_windows, [], "no message to a window that is not ours")
        self.assertFalse(res["window_closed"])

    def test_a_window_that_never_reaches_the_foreground_gets_no_input(self):
        desk = self.desk()
        desk.front_ok = False
        res = desk.run_once(1)
        self.assertIn("did not reach the foreground", res["failure"])
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(desk.closed_windows, [OURS])

    def test_a_window_that_does_not_start_on_carpeta_b_gets_no_input(self):
        desk = self.desk()
        original = desk.open_explorer

        def open_on_root(start):
            handle = original(start)
            desk.location = desk.root
            return handle

        desk.open_explorer = open_on_root
        res = desk.run_once(1)
        self.assertIn("does not start on carpeta-b", res["failure"])
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))

    def test_the_window_that_cannot_be_told_apart_gets_no_input(self):
        desk = self.desk()
        desk.open_explorer = lambda start: None
        res = desk.run_once(1)
        self.assertIn("could not be told apart", res["failure"])
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertFalse(self.pin.exists(), "no pin without a recorded window")
        self.assertEqual(desk.closed_windows, [])

    def test_the_probe_opens_and_observes_but_clicks_nothing(self):
        desk = self.desk(mode="probe")
        desk.probe_print = lambda elements: None
        probes = []
        with mock.patch.object(driver, "log",
                               side_effect=lambda event, **f: probes.append((event, f))):
            res = desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total(), res["steps"]), ([], 0, 0))
        events = [e for e, _ in probes]
        self.assertEqual(events.count("explorer_probe"), 3)
        for event, fields in probes:
            if event == "explorer_probe":
                self.assertIn("uia_identifiers", fields)
                self.assertIn("engine_usable", fields)
        self.assertEqual(desk.closed_windows, [OURS])
        self.assertFalse(desk.root.exists())

    def test_the_probe_records_the_back_buttons_identifiers_and_never_a_name(self):
        desk = self.desk(mode="probe")
        desk.probe_print = lambda elements: None
        seen = {}
        with mock.patch.object(driver, "log", side_effect=lambda event, **f: seen.setdefault(
                (event, f.get("target")), f)):
            desk.run_once(1)
        back = next(f for (event, target), f in seen.items()
                    if event == "explorer_probe" and target == "Atrás")
        self.assertEqual(back["uia_identifiers"], [["Button", "BackButton"]])

    def test_a_run_that_aborts_closes_the_window_and_removes_the_pin_and_the_tree(self):
        desk = self.desk(max_live=1)
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual(desk.closed_windows, [OURS])
        self.assertFalse(self.pin.exists())
        self.assertFalse(desk.root.exists())
        recorded = json.loads((self.tmp / "results.jsonl").read_text(encoding="utf-8"))
        self.assertIn("budget", recorded["aborted"])

    def test_no_result_carries_a_folder_name_a_handle_or_a_path(self):
        desk = self.desk()
        res = desk.run_once(1)
        text = json.dumps(res)
        for secret in (str(desk.root), str(self.temp_root), "900"):
            self.assertNotIn(secret.replace("\\", "\\\\"), text)
            self.assertNotIn(secret, text)

    def test_the_default_launch_shows_the_whole_path(self):
        import inspect
        source = inspect.getsource(dossier.main)
        self.assertIn('"--explorer-launch", choices=["root", "e"], default="e"', source)


class ExplorerActivationTest(QuietTest):
    """Eighth round, finding 2: the Explorer window is the one the client opened, asked for by
    its handle (the shell's explorer.exe owns the desktop and the taskbar too, so a process
    lookup could name another window). The result is verified by GetForegroundWindow."""

    class Desk(dossier.DossierProtocol):
        def __init__(self, hwnd=900, arrives_by=None, operator=False):
            self.args = SimpleNamespace(foreground_wait=6.0, operator_activation=operator,
                                        operator_activation_wait=4.0)
            self.flow = {"exe": "explorer.exe"}
            self.notepad = None
            self.started = set()
            self.activation = driver.new_activation()
            self.now = 0.0
            self.hwnd = hwnd
            self.fg = {"exe": "msedge.exe", "pid": 7, "hwnd": 50, "maximized_primary": True,
                       "class": "Chrome_WidgetWin_1"}
            self.arrives_by = arrives_by
            self.asked = []
            self.maximized = []
            self.announced = []
            self.zoomed = False

        def mono(self):
            return self.now

        def pause(self, seconds):
            self.now += seconds

        def front_window(self):
            return self.fg

        def front_hwnd(self):
            return self.fg["hwnd"]

        def bring_front(self, hwnd):
            self.maximized.append(hwnd)

        def activate(self, hwnd):
            self.asked.append(hwnd)
            if self.arrives_by is not None and len(self.asked) >= self.arrives_by[1]:
                self.fg = {"exe": "explorer.exe", "pid": 11, "hwnd": hwnd,
                           "maximized_primary": True, "class": "CabinetWClass"}
                return self.arrives_by[0]
            return None

        def diagnose(self, pid, hwnd=None):
            return {"windows": [{"class": "CabinetWClass", "visible": True, "iconic": False,
                                 "zoomed": True, "primary_monitor": True}],
                    "foreground": {"class": "Chrome_WidgetWin_1", "exe": "msedge.exe"},
                    "asked_for": hwnd}

        def announce(self, text):
            self.announced.append(text)

    def test_the_operator_flag_exists_and_is_off_by_default(self):
        import inspect
        source = inspect.getsource(dossier.main)
        self.assertIn('"--operator-activation", action="store_true"', source)
        self.assertIn('"--operator-activation-wait", type=float, default=120.0', source)

    def test_the_recorded_handle_is_asked_for_and_the_method_is_recorded(self):
        desk = self.Desk(arrives_by=("attach_all_topmost", 2))
        self.assertTrue(desk.wait_explorer_front(900))
        self.assertEqual(desk.asked, [900, 900])
        self.assertEqual(desk.activation["method"], "attach_all_topmost")
        self.assertTrue(desk.maximized and set(desk.maximized) == {900},
                        "placed by handle on every round")

    def test_a_window_that_never_arrives_leaves_a_diagnostic_of_that_handle_only(self):
        desk = self.Desk()
        self.assertFalse(desk.wait_explorer_front(900))
        diag = desk.activation["diagnostic"]
        self.assertEqual(diag["asked_for"], 900)
        self.assertEqual(diag["windows"][0]["class"], "CabinetWClass")
        self.assertEqual(desk.announced, [])

    def test_the_operator_may_bring_the_recorded_window_when_asked_to(self):
        desk = self.Desk(operator=True)
        original = desk.pause

        def pause(seconds):
            original(seconds)
            if desk.now >= 7.0:  # after the 6 s wait, inside the operator's
                desk.fg = {"exe": "explorer.exe", "pid": 11, "hwnd": 900,
                           "maximized_primary": True, "class": "CabinetWClass"}

        desk.pause = pause
        self.assertTrue(desk.wait_explorer_front(900))
        self.assertIs(desk.activation["operator_activation"], True)
        self.assertEqual(len(desk.announced), 1)
        self.assertIn("explorer.exe", desk.announced[0])

    def test_the_run_record_carries_the_activation_and_the_failure(self):
        desk = FakeExplorerDesk(self.tmp, self.temp_root, self.tmp / "window.pin")
        desk.front_ok = False
        desk.activation = {"method": None, "tried": {"none": 3}, "operator_activation": False,
                           "diagnostic": {"windows": [], "foreground": {}}}
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["activation_tried"], {"none": 3})
        self.assertIn("activation_diagnostic", res)
        self.assertIn("did not reach the foreground", res["failure"])


class EditorAccessibilityTest(QuietTest):
    """Eighth round, finding 3: the field «Editor de texto» is named by UI Automation only. In
    the probe that reached the foreground the engine reported no accessibility-sourced element,
    and the run would have gone on to a step with no candidate."""

    class Desk(FakeEditorDesk):
        def __init__(self, *args, accessible_after=0, never=False, **kwargs):
            super().__init__(*args, **kwargs)
            self.accessible_after = accessible_after  # observations that come back empty
            self.never = never
            self.observations = 0
            self.windows = {5151: 99}

        def window_of(self, pid):
            return self.windows.get(pid)

        def observe(self):
            self.observations += 1
            snap, elements, obs = super().observe()
            if self.never or self.observations <= self.accessible_after:
                elements = [dict(e, label_source="ocr", actionability="unknown",
                                 role="text_node") for e in elements]
            return snap, elements, obs

        def focus_owner(self, pid):
            return {"pid": 4, "class": "Chrome_WidgetWin_1", "control_type": "Document",
                    "top_level_class": "Chrome_WidgetWin_1", "is_app": False}

    def desk(self, **kwargs):
        return self.Desk(self.tmp, self.temp_root, **kwargs)

    def test_a_first_read_without_accessibility_is_read_again(self):
        desk = self.desk(accessible_after=2)
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertGreater(res["accessibility_elements"], 0)
        self.assertEqual(desk.observations >= 3, True)

    def test_no_accessibility_evidence_stops_the_run_before_any_input(self):
        desk = self.desk(never=True)
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["failure"], "no accessibility evidence from the editor")
        self.assertEqual(res["accessibility_elements"], 0)
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))
        self.assertEqual(desk.closed, [5151], "the editor is closed all the same")
        self.assertFalse(desk.folder.exists())

    def test_the_focus_owner_is_recorded_without_names(self):
        desk = self.desk(never=True)
        res = desk.run_once(1)
        self.assertEqual(set(res["uia_focus"]),
                         {"pid", "class", "control_type", "top_level_class", "is_app"})
        self.assertIs(res["uia_focus"]["is_app"], False)

    def test_the_steps_read_the_editors_main_window(self):
        desk = self.desk()
        desk.run_once(1)
        self.assertEqual(desk.app_hwnd, 99, "not the foreground window, which may be a helper")

    def test_the_focus_owner_says_whether_it_belongs_to_the_application(self):
        desk = FakeEditorDesk(self.tmp, self.temp_root)
        with mock.patch.object(driver, "uia_focus_owner", return_value={
                "pid": 5151, "class": "Window Class", "control_type": "Window",
                "top_level_class": "Window Class"}):
            self.assertIs(dossier.DossierProtocol.focus_owner(desk, 5151)["is_app"], True)
            self.assertIs(dossier.DossierProtocol.focus_owner(desk, 9)["is_app"], False)
        with mock.patch.object(driver, "uia_focus_owner", return_value=None):
            self.assertEqual(dossier.DossierProtocol.focus_owner(desk, 5151), {"unreadable": True})

    def test_the_focus_owner_output_is_parsed_strictly(self):
        self.assertEqual(driver.parse_focus_owner("12\tWindow Class\tWindow\tWindow Class"),
                         {"pid": 12, "class": "Window Class", "control_type": "Window",
                          "top_level_class": "Window Class"})
        for bad in ("", "x\ty\tz\tw", "12\ta\tb", "12\ta\tb\tc\td"):
            self.assertIsNone(driver.parse_focus_owner(bad), bad)


class DossierServeArgvTest(unittest.TestCase):
    """The dossier's engine command follows the same rule as the reduced protocol's: a live run
    takes no fixed session id (chapter 08 §8.4), and a probe keeps the dossier's identifier."""

    def args(self, session_id):
        return SimpleNamespace(exe="ommatix.exe", output=0, config="live.toml",
                               session_id=session_id)

    def test_a_dossier_live_run_takes_no_fixed_session_id(self):
        argv = dossier.dossier_serve_argv(self.args(None), live=True)
        self.assertNotIn("--session-id", argv)
        self.assertEqual(argv[-2:], ["--input", "sendinput"])

    def test_a_dossier_live_run_refuses_an_explicit_session_id(self):
        with self.assertRaises(dossier.Abort):
            dossier.dossier_serve_argv(self.args("s4-e2e01"), live=True)

    def test_a_dossier_probe_keeps_the_dossier_identifier(self):
        argv = dossier.dossier_serve_argv(self.args(None), live=False)
        self.assertEqual(argv[-2:], ["--session-id", dossier.DOSSIER_PROBE_SESSION_ID])
        self.assertNotIn("--input", argv)


# ---- the decoy case: BDD-35 on the real actuator ---------------------------------------------

DECOY_RIDS = {"main": "7.1", "decoy": "7.3", "target": "7.4", "other": "7.9"}


class FakeDecoyDesk(dossier.DossierProtocol):
    """The decoy case on a fake editor started with --decoy-focus. The engine's `type` is answered
    as ADR-0018 answers a focus that never came: indeterminate, the focusing click delivered, no key
    sent. The switches below script each way the case could be wrong."""

    def __init__(self, tmp, temp_root, max_live=10, mode="run"):
        self.args = SimpleNamespace(
            app="decoy", max_live=max_live, settle=0.0, results=tmp / "results.jsonl",
            debug_grounding=False, restart="each", mode=mode, editor_exe="editor.exe",
            explorer_launch="root", foreground_wait=1.0, foreground_wait_explorer=1.0,
            first_run=1, runs=1, pause=0.0, focus_wait=0.0, operator_focus=False,
            operator_focus_wait=0.0)
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
        self.flow = dossier.decoy_flow()
        self.temp_root = temp_root
        self.pin_file = None
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        self.root_name = None
        self.spawned = []
        self.closed = []
        self.actions = []
        self.stored = {}  # request_id -> the answer the engine stored for it
        self.delivered = 0
        self.rejected = 0
        self.answer = "indeterminate"  # "effect_observed" scripts a false success
        self.stale_answers = 0  # the first requests refused as STALE_SNAPSHOT, before any delivery
        self.refusal = None  # a code every new request is refused with, before any delivery
        self.request_ids = []  # every request the engine receives, in order
        self.replay_delivers = False  # the repetition delivers again
        self.leak = None  # "decoy" or "target": a key reaches that field on the first request
        self.leaked = None  # the field the keys reached, once the first request is handled
        self.focus = "target"  # where UI Automation puts the keyboard focus
        self.focus_reads = []  # scripted reads, in order; the last state holds afterwards
        self.snapshot_no = 0  # every observation is a new snapshot: s1, s2, ...
        self.snapshots_used = []  # the snapshot each request names
        self.click_landed = "decoy"  # the element under the focusing click
        self.accessible = True
        self.ambiguous = False
        self.pre_chain = None
        self.centre_of = None
        self.probe_print = lambda elements: None
        self.fg_pid = 5151  # the process of the window in front: the editor's by default
        self.fg_reads = []  # scripted foreground reads, in order; the last one holds afterwards
        self.fg_after_send = None  # the foreground process from the first request on, if set
        self.minimized = False
        self.minimized_reads = []  # scripted minimized states, in order; the last one holds

    # the engine
    def observe(self):
        source = "accessibility" if self.accessible else "ocr"
        elements = [
            {"label": dossier.DECOY_FIELD, "role": "input", "actionability": "confirmed",
             "label_source": source, "target_id": "decoy", "revision": 1, "zone": "center_main"},
            {"label": dossier.DECOY_TARGET, "role": "input", "actionability": "confirmed",
             "label_source": source, "target_id": "target", "revision": 1, "zone": "center_main"},
            {"label": "Editor de texto", "role": "input", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "main", "revision": 1,
             "zone": "center_main"},
            {"label": "Guardar", "role": "button", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "save", "revision": 1,
             "zone": "center_main"},
            {"label": "Estado: pendiente", "role": "text_node", "actionability": "unknown",
             "label_source": "ocr", "target_id": "status", "revision": 1, "zone": "center_main"},
        ]
        if self.ambiguous:
            elements.append(dict(elements[0], target_id="decoy2"))
        self.snapshot_no += 1
        return {"session_id": "s", "snapshot_id": f"s{self.snapshot_no}", "geometry_epoch": 1}, elements, {}

    def call(self, tool, args):
        assert tool == "execute_action" and args["dry_run"] is False
        self.snapshots_used.append(args["snapshot_id"])
        self.request_ids.append(args["request_id"])
        self.actions.append({"target": args["target_id"], "action": args["action"],
                             "text": args.get("text")})
        if args["request_id"] in self.stored:  # the repetition: the stored result, no new delivery
            if self.replay_delivers:
                self.delivered += 1
            return dict(self.stored[args["request_id"]]), False, {}, 0.2
        if self.stale_answers:  # the screen moved since the observation: refused before any delivery
            self.stale_answers -= 1
            out = self.refused("STALE_SNAPSHOT", "the screen changed since the observation; nothing "
                               "was delivered; observe again")
        elif self.refusal is not None:
            out = self.refused(self.refusal, "the element is not actionable; nothing was delivered")
        else:
            if self.delivered == 0:
                self.leaked = self.leak  # the keys reach a field only with the first delivery
            if self.answer == "indeterminate":
                out = {"status": "indeterminate", "input_delivered": True, "verified": False,
                       "observed_effect": "unknown",
                       "error": {"code": "INDETERMINATE_EFFECT", "message": "no key was sent",
                                 "retryable": False, "next_step": "reconcile_effect"}}
            else:
                out = {"status": "effect_observed", "input_delivered": True, "verified": True,
                       "observed_effect": "visual_change"}
            self.delivered += 1
        self.stored[args["request_id"]] = out
        if self.fg_after_send is not None:  # the window the operator had in front goes away
            self.fg_pid = self.fg_after_send
        return dict(out), False, {}, 5.0

    def refused(self, code: str, message: str) -> dict:
        self.rejected += 1
        return {"status": "rejected", "input_delivered": False, "verified": False,
                "error": {"code": code, "message": message, "retryable": False,
                          "next_step": "observe_again"}}

    def metrics(self):
        return {"delivered_actions": self.delivered, "rejected_actions": self.rejected}

    def identify(self, hwnd, label, control_types):
        table = {dossier.DECOY_FIELD: row(DECOY_RIDS["decoy"], "Decoy", "Edit"),
                 dossier.DECOY_TARGET: row(DECOY_RIDS["target"], "Target", "Edit"),
                 "Editor de texto": row(DECOY_RIDS["main"], "Field", "Edit"),
                 "Guardar": row("7.2", "Save", "Button")}
        found = table.get(label)
        return [found] if found and found["control_type"] in control_types else []

    def element_centre(self, hwnd, label, control_types):
        self.centre_of = label
        return (100, 200)

    def point_chain(self, x, y):  # the pre-click read: the control about to be clicked
        if self.pre_chain is not None:
            return self.pre_chain
        return [self.identify(77, self.centre_of, ("Edit", "Button"))[0]]

    def island_walk(self, x, y, host_runtime_id):
        return None

    def window_under(self, point):
        return 77

    def cursor_chain(self):  # after the click: the control under the cursor
        landed = dossier.DECOY_FIELD if self.click_landed == "decoy" else dossier.DECOY_TARGET
        return [self.identify(77, landed, ("Edit", "Button"))[0]]

    def focus_runtime(self):
        if self.focus_reads:
            self.focus = self.focus_reads.pop(0)
        if self.focus == "outside":  # the desktop: another process, no element of the editor
            return {"pid": 9999, "runtime_id": "7.8", "control_type": "Pane"}
        return {"pid": 5151, "runtime_id": DECOY_RIDS[self.focus], "control_type": "Edit"}

    GUARD_POLL_S = 0.0  # the guard's wait polls at once: the tests script the reads, not the time

    def foreground_pid(self):
        if self.fg_reads:
            self.fg_pid = self.fg_reads.pop(0)
        return self.fg_pid

    def is_minimized(self, hwnd):
        if self.minimized_reads:
            self.minimized = self.minimized_reads.pop(0)
        return self.minimized

    def foreground_fact(self):
        return {"exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 77,
                "maximized_primary": True, "class": ""}

    def spawn(self, argv):
        self.spawned.append(argv)
        self.folder = Path(argv[argv.index("--folder") + 1])
        return 5151

    def wait_foreground(self, pid):
        return True, 0

    def grant_editor_foreground(self, pid):
        assert pid == self.editor_pid and pid in self.started
        return True

    def request_editor_foreground(self, hwnd):
        assert hwnd == 77 and self.editor_pid in self.started
        return True

    def app_window(self, pid):
        return 77

    def current_hwnd(self):
        return 77

    def require_foreground(self):
        return {"exe": "ommatix-testeditor.exe", "pid": 5151, "hwnd": 77,
                "maximized_primary": True}

    def title_of(self, hwnd):
        typed = len(dossier.EDITOR_TEXT) if self.leaked else 0
        return dossier.editor_title(0, False, (typed if self.leaked == "decoy" else 0,
                                               typed if self.leaked == "target" else 0))

    def close(self, pid):
        if pid is not None and pid in self.started:
            self.closed.append(pid)
            self.started.discard(pid)



class DecoyRunTest(QuietTest):
    def desk(self, **kwargs):
        return FakeDecoyDesk(self.tmp, self.temp_root, **kwargs)

    def test_startup_grants_only_the_started_editor_pid(self):
        desk = self.desk()
        desk.started.add(5151)
        desk.editor_pid = 5151
        with mock.patch.object(dossier.user32, "AllowSetForegroundWindow", create=True,
                               return_value=1) as grant:
            self.assertTrue(dossier.DossierProtocol.grant_editor_foreground(desk, 5151))
            self.assertFalse(dossier.DossierProtocol.grant_editor_foreground(desk, 9999))
            self.assertFalse(dossier.DossierProtocol.grant_editor_foreground(desk, -1))
        grant.assert_called_once_with(5151)

    def test_startup_foreground_request_uses_no_input_queue_attachment(self):
        desk = self.desk()
        desk.started.add(5151)
        desk.editor_pid = 5151
        with mock.patch.object(dossier.driver, "_pid", return_value=5151), \
             mock.patch.object(dossier.driver, "_class", return_value="winit window"), \
             mock.patch.object(dossier.user32, "SetForegroundWindow", return_value=1) as request, \
             mock.patch.object(dossier.user32, "AttachThreadInput",
                               side_effect=AssertionError("input queues must stay separate")):
            self.assertTrue(dossier.DossierProtocol.request_editor_foreground(desk, 77))
        request.assert_called_once_with(77)

    def test_startup_never_requests_another_process_or_a_helper_window(self):
        desk = self.desk()
        desk.started.add(5151)
        desk.editor_pid = 5151
        with mock.patch.object(dossier.user32, "SetForegroundWindow") as request:
            for owner, cls in ((9999, "winit window"), (5151, "Winit Thread Event Target")):
                with mock.patch.object(dossier.driver, "_pid", return_value=owner), \
                     mock.patch.object(dossier.driver, "_class", return_value=cls):
                    self.assertFalse(dossier.DossierProtocol.request_editor_foreground(desk, 77))
            desk.started.clear()
            self.assertFalse(dossier.DossierProtocol.request_editor_foreground(desk, 77))
        request.assert_not_called()

    def test_startup_activation_never_uses_the_drivers_attachment_methods(self):
        desk = self.desk()
        with mock.patch.object(desk, "front_hwnd", side_effect=[12, 12, 77]), \
             mock.patch.object(desk, "request_editor_foreground", return_value=True) as request, \
             mock.patch.object(dossier.user32, "AttachThreadInput",
                               side_effect=AssertionError("input queues must stay separate")):
            self.assertEqual(desk.activate(77), "set_foreground")
        request.assert_called_once_with(77)

    def test_startup_requests_are_opt_in_for_the_decoy(self):
        desk = self.desk()
        desk.args.app = "editor"
        desk.started.add(5151)
        desk.editor_pid = 5151
        with mock.patch.object(dossier.user32, "AllowSetForegroundWindow") as grant, \
             mock.patch.object(dossier.user32, "SetForegroundWindow") as request:
            self.assertFalse(dossier.DossierProtocol.grant_editor_foreground(desk, 5151))
            self.assertFalse(dossier.DossierProtocol.request_editor_foreground(desk, 77))
        grant.assert_not_called()
        request.assert_not_called()

    def test_startup_denied_rights_do_not_replace_the_real_focus_check(self):
        desk = self.desk()
        desk.focus = "outside"
        with mock.patch.object(desk, "grant_editor_foreground", return_value=False), \
             mock.patch.object(desk, "request_editor_foreground", return_value=True):
            res = desk.run_once(1)
        self.assertFalse(res["foreground_granted"])
        self.assertTrue(res["startup_focus_requested"])
        self.assertFalse(res["success"])
        self.assertEqual(desk.actions, [], "API acceptance is not keyboard focus")

    def test_startup_foreground_denial_keeps_the_operator_focus_fallback(self):
        for arrives in (False, True):
            with self.subTest(arrives=arrives):
                desk = self.desk()
                desk.args.operator_focus = True
                with mock.patch.object(desk, "wait_foreground", return_value=(False, 1)), \
                     mock.patch.object(desk, "wait_editor_focus", return_value=desk.focus_runtime()) as wait, \
                     mock.patch.object(desk, "front", return_value={}), \
                     mock.patch.object(desk, "is_front", return_value=arrives):
                    res = desk.run_once(1)
                self.assertGreaterEqual(wait.call_count, 1)
                self.assertEqual(res["success"], arrives)
                if not arrives:
                    self.assertEqual(desk.actions, [])

    def test_startup_os_denials_and_errors_are_best_effort(self):
        desk = self.desk()
        desk.started.add(5151)
        desk.editor_pid = 5151
        for result in (0, OSError("denied")):
            kwargs = {"side_effect": result} if isinstance(result, OSError) else {"return_value": result}
            with mock.patch.object(dossier.user32, "AllowSetForegroundWindow", **kwargs), \
                 mock.patch.object(dossier.user32, "SetForegroundWindow", **kwargs), \
                 mock.patch.object(dossier.driver, "_pid", return_value=5151), \
                 mock.patch.object(dossier.driver, "_class", return_value="winit window"):
                self.assertFalse(dossier.DossierProtocol.grant_editor_foreground(desk, 5151))
                self.assertFalse(dossier.DossierProtocol.request_editor_foreground(desk, 77))

    def test_the_decoy_run_passes_when_no_key_is_sent_and_the_focus_is_on_the_target(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(res["e2e"], "BDD-35-live-plan")
        self.assertEqual(desk.actions, [
            {"target": "decoy", "action": "type", "text": "prueba 123"},
            {"target": "decoy", "action": "type", "text": "prueba 123"}], "the repetition")
        self.assertEqual(res["decoy"]["placement"], "target")
        self.assertEqual((res["decoy"]["delivered_delta"], res["decoy"]["rejected_delta"]), (1, 0))
        self.assertTrue(res["decoy"]["replay_is_stored"])
        self.assertTrue(res["decoy"]["grounding_ok"])
        self.assertEqual(desk.ledger_total(), 2, "the request and its repetition")
        self.assertEqual(res["actions_requested"], 2, "the repetition is a live request too")
        self.assertEqual(res["actions"][0]["message"], "no key was sent",
                         "the engine's sentence is recorded with the action")
        self.assertEqual(res["decoy"]["replay"]["status"], "indeterminate")
        self.assertEqual(res["decoy"]["replay"]["message"], "no key was sent",
                         "the stored answer repeats the sentence, and nothing new was sent")
        self.assertNotIn("keys_without_focus", res)
        self.assertEqual(res["grounding_wrong"], 0)
        self.assertEqual(desk.spawned, [["editor.exe", "--folder", str(desk.folder),
                                         dossier.DECOY_FLAG]])
        self.assertEqual(desk.closed, [5151])
        self.assertFalse(desk.folder.exists(), "the run folder is cleaned up")

    def test_a_focus_that_never_moved_does_not_pass_the_case(self):
        desk = self.desk()
        desk.focus = "decoy"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["decoy"]["placement"], "decoy")
        self.assertIn("not exercised", res["failure"])

    def test_a_focus_inside_the_editor_on_another_element_is_not_the_redirect_target(self):
        desk = self.desk()
        desk.focus = "other"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["decoy"]["placement"], "editor")
        self.assertIn("inside the editor", res["failure"])

    def test_no_request_is_sent_when_the_editor_holds_no_keyboard_focus(self):
        desk = self.desk()
        desk.focus = "outside"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("no request", res["failure"])
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))
        self.assertEqual(res["focus_before"], {"in_editor": False, "control_type": "Pane"})

    def test_the_focus_is_waited_for_read_only_and_the_request_follows_it(self):
        desk = self.desk()
        desk.args.focus_wait = 5.0
        desk.focus_reads = ["outside", "outside", "target"]
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(len(desk.actions), 2, "the request and its repetition, after the wait")
        self.assertEqual(desk.snapshots_used, ["s2", "s2"], "the observation after the wait")
        self.assertEqual(res["focus_before"], {"in_editor": True, "control_type": "Edit"})

    def test_the_operator_is_told_how_to_give_the_focus_and_the_wait_is_its_own(self):
        desk = self.desk()
        desk.args.operator_focus = True
        desk.args.operator_focus_wait = 0.0
        desk.focus = "outside"
        messages = []
        desk.announce = messages.append
        res = desk.run_once(1)
        self.assertEqual(len(messages), 1)
        self.assertIn("OPERATOR", messages[0])
        self.assertIn("do not type", messages[0])
        self.assertFalse(res["success"])
        self.assertEqual(desk.actions, [])

    def test_the_probe_reads_the_focus_and_sends_nothing(self):
        desk = self.desk(mode="probe")
        desk.focus = "outside"
        res = desk.run_once(1)
        self.assertEqual(res["focus_probe"], {"in_editor": False, "control_type": "Pane"})
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))

    def test_a_focusing_click_that_landed_on_the_target_is_grounding_wrong(self):
        desk = self.desk()
        desk.click_landed = "target"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["grounding_wrong"], 1)
        self.assertIn("did not reach Campo auxiliar", res["failure"])

    def test_a_key_that_reached_a_field_is_the_safety_event(self):
        for field in ("decoy", "target"):
            desk = self.desk()
            desk.leak = field
            res = desk.run_once(1)
            self.assertFalse(res["success"], field)
            self.assertEqual(res["keys_without_focus"], 1, field)
            self.assertIn("safety event", res["failure"], field)

    def test_an_effect_observed_answer_fails_the_case(self):
        desk = self.desk()
        desk.answer = "effect_observed"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("INDETERMINATE_EFFECT", res["failure"])

    def test_a_repetition_that_delivers_again_fails_the_case(self):
        desk = self.desk()
        desk.replay_delivers = True
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["decoy"]["delivered_delta"], 2)
        self.assertIn("counted 2 deliveries", res["failure"])

    def test_the_request_waits_for_the_operator_when_the_editor_lost_the_foreground(self):
        desk = self.desk(max_live=4)
        desk.args.operator_focus = True
        desk.args.operator_focus_wait = 5.0
        desk.fg_reads = [9999, 5151]  # the first guard finds another window in front; then the editor
        messages = []
        desk.announce = messages.append
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(res["operator_waits"], 1)
        self.assertEqual([g["outcome"] for g in res["guard"]], ["waited"])
        guard_messages = [m for m in messages if "does not have the foreground" in m]
        self.assertEqual(len(guard_messages), 1, "the guard said what it waited for, once")
        self.assertIn("OPERATOR", guard_messages[0])
        self.assertEqual(desk.request_ids, ["s4-bdd35-r1-1", "s4-bdd35-r1-1"],
                         "nothing went out while the window was away; the request followed the wait")
        self.assertEqual(desk.ledger_total(), 2)

    def test_a_lost_window_with_no_operator_focus_sends_nothing_and_spends_nothing(self):
        desk = self.desk(max_live=4)
        desk.args.operator_focus = True
        desk.args.operator_focus_wait = 0.0  # the operator does not give the focus back in time
        desk.fg_pid = 9999  # another window stays in front
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))
        self.assertEqual(res["operator_waits"], 1)
        self.assertEqual([g["outcome"] for g in res["guard"]], ["lost"])
        self.assertIn("no request", res["failure"])
        self.assertIn("nothing was sent", res["failure"])

    def test_the_operator_waits_are_counted_and_capped(self):
        desk = self.desk(max_live=4)
        desk.args.operator_focus = True
        desk.args.operator_focus_wait = 5.0
        desk.fg_reads = [9999, 5151] * 4  # the window keeps leaving and coming back
        messages = []
        desk.announce = messages.append
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["operator_waits"], dossier.DECOY_GUARD_WAITS)
        guard_messages = [m for m in messages if "does not have the foreground" in m]
        self.assertEqual(len(guard_messages), dossier.DECOY_GUARD_WAITS)
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0), "nothing sent or spent")
        self.assertIn("nothing was sent", res["failure"])

    def test_the_repetition_is_guarded_and_not_sent_when_the_window_is_away(self):
        desk = self.desk(max_live=4)
        desk.fg_after_send = 9999  # another window comes in front right after the focusing click
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(desk.request_ids, ["s4-bdd35-r1-1"], "the repetition was not sent")
        self.assertEqual(desk.ledger_total(), 1, "only the request that went out is spent")
        self.assertEqual(res["decoy"]["status"], "indeterminate")
        self.assertIs(res["decoy"]["repetition_sent"], False)
        self.assertIn("repetition", res["failure"])

    def test_a_minimized_editor_is_said_on_stderr_and_nothing_goes_out_until_it_is_back(self):
        desk = self.desk(max_live=4)
        desk.args.operator_focus = True
        desk.args.operator_focus_wait = 5.0
        desk.minimized_reads = [True, False]  # minimized at the first guard, back at the next read
        messages = []
        desk.announce = messages.append
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(res["operator_waits"], 1)
        minimized = [m for m in messages if "minimized" in m]
        self.assertEqual(len(minimized), 1)
        self.assertIn("ONCE", minimized[0])
        self.assertEqual(desk.ledger_total(), 2)

    def test_each_request_records_how_long_after_the_focus_check_it_went_out(self):
        desk = self.desk(max_live=4)
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(res["operator_waits"], 0, "the guard passed at once: nothing waited")
        self.assertIsInstance(res["actions"][0]["since_focus_ms"], float)
        self.assertGreaterEqual(res["actions"][0]["since_focus_ms"], 0.0)

    def test_a_stale_snapshot_is_retried_with_a_new_observation_and_request_id(self):
        desk = self.desk(max_live=4)
        desk.stale_answers = 1  # refused before any delivery: the screen moved since the observation
        res = desk.run_once(1)
        self.assertTrue(res["success"], res["failure"])
        self.assertEqual(desk.request_ids, ["s4-bdd35-r1-1", "s4-bdd35-r1-2", "s4-bdd35-r1-2"],
                         "a new request_id per attempt; the repetition names the final one")
        self.assertEqual(desk.snapshots_used, ["s2", "s3", "s3"], "a new observation per attempt")
        self.assertEqual([p["attempt"] for p in res["pre_grounded"]], [1, 2], "grounded again")
        self.assertEqual([a["attempt"] for a in res["actions"]], [1, 2])
        self.assertEqual(res["actions"][0]["error"], "STALE_SNAPSHOT")
        self.assertIsInstance(res["actions"][0]["observe_to_action_ms"], float)
        self.assertEqual((res["stale_retries"], res["decoy"]["attempts"]), (1, 2))
        self.assertEqual((res["actions_requested"], desk.ledger_total()), (3, 3),
                         "both attempts and the repetition are live requests in the ledger")
        self.assertEqual((res["rejected"], res["indeterminate"]), (1, 1))
        self.assertEqual((res["decoy"]["delivered_delta"], res["decoy"]["rejected_delta"]), (1, 0),
                         "the refused attempt is not counted against the final request")
        self.assertEqual(res["decoy"]["placement"], "target")
        self.assertTrue(res["decoy"]["replay_is_stored"])
        self.assertEqual(desk.delivered, 1)

    def test_three_stale_snapshots_fail_the_case_with_no_repetition_and_no_fourth_request(self):
        desk = self.desk(max_live=4)
        desk.stale_answers = 3
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("STALE_SNAPSHOT", res["failure"])
        self.assertIn("nothing was delivered", res["failure"])
        self.assertEqual(desk.request_ids, ["s4-bdd35-r1-1", "s4-bdd35-r1-2", "s4-bdd35-r1-3"])
        self.assertEqual(desk.snapshots_used, ["s2", "s3", "s4"])
        self.assertEqual(len(res["pre_grounded"]), 3)
        self.assertEqual((res["stale_retries"], res["actions_requested"]), (2, 3))
        self.assertEqual((desk.ledger_total(), desk.delivered, res["delivered"]), (3, 0, 0))
        self.assertEqual(res["decoy"], {"attempts": 3, "status": "rejected",
                                        "error": "STALE_SNAPSHOT", "input_delivered": False})

    def test_a_refusal_other_than_stale_snapshot_is_not_retried(self):
        desk = self.desk(max_live=4)
        desk.refusal = "NOT_ACTIONABLE"
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(desk.request_ids, ["s4-bdd35-r1-1", "s4-bdd35-r1-1"],
                         "the request and its repetition only")
        self.assertEqual((res["stale_retries"], res["decoy"]["attempts"]), (0, 1))
        self.assertIn("did not report indeterminate", res["failure"])

    def test_no_accessibility_evidence_sends_nothing(self):
        desk = self.desk()
        desk.accessible = False
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertIn("no accessibility evidence", res["failure"])
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))

    def test_a_point_that_is_not_the_decoy_sends_nothing(self):
        desk = self.desk()
        desk.pre_chain = [row(DECOY_RIDS["target"], "Target", "Edit")]
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["pre_grounded"][0]["grounded"], False)
        self.assertEqual((desk.actions, desk.ledger_total()), ([], 0))

    def test_an_ambiguous_decoy_field_aborts_before_any_request(self):
        desk = self.desk()
        desk.ambiguous = True
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual(desk.actions, [])
        self.assertFalse(desk.folder.exists(), "the cleanup still happens")

    def test_the_probe_sends_nothing(self):
        desk = self.desk(mode="probe")
        res = desk.run_once(1)
        self.assertEqual((desk.actions, desk.ledger_total(), res["steps"]), ([], 0, 0))
        self.assertEqual(desk.closed, [5151])

    def test_the_live_budget_still_applies_to_the_repetition(self):
        desk = self.desk(max_live=1)
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual(len(desk.actions), 1, "the repetition is not sent")


class DecoyPureTest(unittest.TestCase):
    def test_the_title_keeps_the_editors_own_format_with_and_without_the_decoy(self):
        self.assertEqual(dossier.editor_title(10, True), "Ommatix S4 editor [caracteres=10 guardado=si]")
        self.assertEqual(dossier.editor_title(0, False, (0, 0)),
                         "Ommatix S4 editor [caracteres=0 guardado=no auxiliar=0 destino=0]")

    def test_the_decoy_title_gives_three_lengths_and_no_text(self):
        counts = dossier.decoy_counts(
            "Ommatix S4 editor [caracteres=0 guardado=no auxiliar=10 destino=0]")
        self.assertEqual(counts, {"caracteres": 0, "auxiliar": 10, "destino": 0})
        self.assertIsNone(dossier.decoy_counts("Ommatix S4 editor [caracteres=0 guardado=no]"))
        self.assertTrue(dossier.keys_reached_a_field(counts))
        self.assertFalse(dossier.keys_reached_a_field({"caracteres": 0, "auxiliar": 0,
                                                       "destino": 0}))

    def test_the_focus_is_named_by_its_runtime_id_and_process(self):
        focus = {"pid": 5151, "runtime_id": "7.4"}
        self.assertEqual(dossier.focus_placement(focus, 5151, "7.3", "7.4"), "target")
        self.assertEqual(dossier.focus_placement({"pid": 5151, "runtime_id": "7.3"}, 5151,
                                                 "7.3", "7.4"), "decoy")
        self.assertEqual(dossier.focus_placement({"pid": 5151, "runtime_id": "7.9"}, 5151,
                                                 "7.3", "7.4"), "editor")
        self.assertEqual(dossier.focus_placement({"pid": 9999, "runtime_id": "7.4"}, 5151,
                                                 "7.3", "7.4"), "outside")
        self.assertEqual(dossier.focus_placement(None, 5151, "7.3", "7.4"), "unknown")
        self.assertEqual(dossier.focus_placement({"pid": 5151, "runtime_id": ""}, 5151,
                                                 "7.3", "7.4"), "unknown")

    def test_the_focus_read_yields_a_process_an_identifier_and_a_kind_of_element_only(self):
        self.assertEqual(dossier.parse_focus_runtime("5151\t7.4\tEdit"),
                         {"pid": 5151, "runtime_id": "7.4", "control_type": "Edit"})
        self.assertIsNone(dossier.parse_focus_runtime("garbage"))
        self.assertIsNone(dossier.parse_focus_runtime("5151\t7.4"))
        self.assertIsNone(dossier.parse_focus_runtime("5151\t7.4\tEdit\textra"))
        summary = dossier.focus_summary({"pid": 5151, "runtime_id": "7.4", "control_type": "Edit"},
                                        5151)
        self.assertEqual(summary, {"in_editor": True, "control_type": "Edit"})
        self.assertNotIn("7.4", str(summary), "no RuntimeId in a record")
        self.assertEqual(dossier.focus_summary(None, 5151),
                         {"in_editor": False, "control_type": None})
        for name_or_value in (".Current.Name", ".Current.Value", "ValuePattern", "TextPattern"):
            self.assertNotIn(name_or_value, dossier.UIA_FOCUS_RUNTIME_PS)

    def test_a_passing_case_has_no_problem_and_each_fault_is_named(self):
        clean = {"status": "indeterminate", "error": "INDETERMINATE_EFFECT",
                 "input_delivered": True, "verified": False, "retryable": False,
                 "next_step": "reconcile_effect", "grounding_ok": True, "placement": "target",
                 "replay_is_stored": True, "delivered_delta": 1, "rejected_delta": 0,
                 "counts": {"caracteres": 0, "auxiliar": 0, "destino": 0},
                 "folder_empty": True}
        self.assertEqual(dossier.decoy_problems(clean), [])
        faults = {"status": "effect_observed", "error": None, "input_delivered": False,
                  "verified": True, "retryable": True, "next_step": "observe_again",
                  "grounding_ok": False, "placement": "editor", "replay_is_stored": False,
                  "delivered_delta": 2, "rejected_delta": 1, "counts": None,
                  "folder_empty": False}
        problems = dossier.decoy_problems({**clean, **faults})
        for fragment in ("INDETERMINATE_EFFECT", "focusing click was not reported", "retry",
                         "identity after the click", "inside the editor", "stored result",
                         "counted 2 deliveries", "cannot be read", "not empty"):
            self.assertTrue(any(fragment in p for p in problems), fragment)
        keys = dossier.decoy_problems({**clean, "counts": {"caracteres": 0, "auxiliar": 4,
                                                           "destino": 0}})
        self.assertTrue(any("safety event" in p for p in keys))

    def test_the_decoy_flow_checks_three_zero_lengths_and_an_empty_folder(self):
        flow = dossier.decoy_flow()
        label, ocr, state, text = flow["steps"][0]
        self.assertEqual(label, dossier.DECOY_FIELD)
        self.assertEqual(state, dossier.editor_state(
            dossier.editor_title(0, False, (0, 0)), "absent"))
        self.assertEqual(text, dossier.EDITOR_TEXT)
        self.assertNotIn("navigating", flow)
        self.assertEqual(dossier.E2E_ID["decoy"], "BDD-35-live-plan")

    def test_a_live_decoy_run_needs_room_for_its_attempts_and_the_repetition(self):
        self.assertIsNone(dossier.decoy_budget_problem("decoy", True, dossier.DECOY_LIVE_REQUESTS))
        self.assertIn("at least 4", dossier.decoy_budget_problem("decoy", True, 3))
        self.assertIsNone(dossier.decoy_budget_problem("decoy", False, 1), "a probe sends nothing")
        self.assertIsNone(dossier.decoy_budget_problem("editor", True, 1))
        self.assertEqual(dossier.DECOY_LIVE_REQUESTS, dossier.DECOY_ATTEMPTS + 1)

    def test_the_editor_and_the_decoy_share_one_configuration_rule(self):
        config = {"safety": {"actuation_enabled": True, "stop_file": "stop.flag",
                             "allowed_applications": [dossier.EDITOR_EXE]},
                  "models": {"accessibility": "uia"}}
        self.assertEqual(dossier.config_problems("decoy", config),
                         dossier.config_problems("editor", config))
        self.assertEqual(dossier.config_problems("decoy", config), [])

    def test_the_client_injects_no_input_of_its_own(self):
        source = (HERE / "e2e_dossier.py").read_text(encoding="utf-8")
        for forbidden in ("SendInput(", "keybd_event", "mouse_event", "SetCursorPos",
                          ".kill(", "PostMessageW(hwnd, 0x0100"):
            self.assertNotIn(forbidden, source, forbidden)


if __name__ == "__main__":
    unittest.main()
