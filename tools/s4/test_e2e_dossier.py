"""Tests of the dossier's E2E-02 (editor) and E2E-03 (Explorer) client, ``e2e_dossier.py``.

No window is opened or activated, no UI Automation is read, no process is started or killed and
no input is sent: the desktop, the engine and the applications are fakes, and the temporary
tree lives in a private directory of the test. Run with:
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

    def test_the_explorer_flow_is_three_single_clicks_and_nothing_else(self):
        flow = dossier.explorer_flow("run1")
        self.assertTrue(dossier.is_click_only(flow))
        self.assertEqual([str(s[0]) for s in flow["steps"]], ["carpeta-a", "carpeta-b", "run1"])
        self.assertEqual([s[2] for s in flow["steps"]],
                         [("carpeta-a",), ("carpeta-a", "carpeta-b"), ()])
        self.assertTrue(all(len(s) == 3 for s in flow["steps"]), "no typing step")
        self.assertEqual(flow["exe"], "explorer.exe")

    def test_a_flow_with_a_typing_step_is_not_click_only(self):
        flow = dossier.explorer_flow("run1")
        flow["steps"].append(("carpeta-a", driver.NOT_BY_OCR, (), "x"))
        self.assertFalse(dossier.is_click_only(flow))

    def test_the_navigation_targets_name_their_control_types_and_zones(self):
        a, b, back = (s[0] for s in dossier.explorer_flow("run1")["steps"])
        self.assertEqual((a.control_types, a.zone), (("TreeItem",), "left_sidebar"))
        self.assertEqual((b.control_types, b.zone), (("TreeItem",), "left_sidebar"))
        self.assertEqual((back.control_types, back.zone), (("Button", "SplitButton"), "top_bar"))
        self.assertEqual(a, "carpeta-a")  # still a plain string for the shared code


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
    """A Protocol on a fake Explorer: a tree view with ``carpeta-a`` and ``carpeta-b``, a file
    list that shows the same names (a decoy: a single click there selects, it does not
    navigate), a breadcrumb, and the owner's own Explorer window next to the one the client
    opened. It records the clicks the gate would have delivered."""

    def __init__(self, tmp, temp_root, pin_file, max_live=10, mode="run"):
        self.args = SimpleNamespace(
            app="explorer", max_live=max_live, settle=0.0, results=tmp / "results.jsonl",
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
        self.flow = dossier.explorer_flow("(set per run)")
        self.temp_root = temp_root
        self.pin_file = pin_file
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        self.root_name = None
        # the fake desktop
        self.root = None
        self.location = None  # the folder of our window, as a path
        self.fg_hwnd = OURS
        self.under = OURS  # the top-level window under the click point
        self.windows = {OURS, OWNERS}
        self.closed_windows = []
        self.stubborn = False
        self.reused_handle = False
        self.killed = []
        self.pin_seen = []
        self.clicks = []  # (target_id, label, zone)
        self.cursor = None
        self.twin_in_sidebar = False
        self.navigate = True
        self.damage = None  # a function that damages the tree after the nth click
        self.on_screen = True
        self.front_ok = True

    # the layout of the window, by target id
    def elements(self):
        names = {"a": "carpeta-a", "b": "carpeta-b"}
        found = [
            {"label": "carpeta-a", "role": "unknown", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "tree-a", "revision": 1,
             "zone": "left_sidebar"},
            {"label": "carpeta-b", "role": "unknown", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "tree-b", "revision": 1,
             "zone": "left_sidebar"},
            # the file list shows the same names: a click there only selects
            {"label": "carpeta-a", "role": "unknown", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "list-a", "revision": 1,
             "zone": "center_main"},
            {"label": "carpeta-b", "role": "unknown", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "list-b", "revision": 1,
             "zone": "center_main"},
            {"label": self.root_name or "", "role": "button", "actionability": "confirmed",
             "label_source": "accessibility", "target_id": "crumb-root", "revision": 1,
             "zone": "top_bar"},
        ]
        if self.twin_in_sidebar:
            found.append(dict(found[0], target_id="tree-a-twin"))
        del names
        return found

    def observe(self):
        return ({"session_id": "s", "snapshot_id": "s1", "geometry_epoch": 1},
                self.elements(), {})

    def call(self, tool, args):
        assert tool == "execute_action" and args["dry_run"] is False
        assert args["action"] == "click", "the Explorer flow only clicks"
        target = next(e for e in self.elements() if e["target_id"] == args["target_id"])
        self.clicks.append((target["target_id"], target["label"], target["zone"]))
        self.pin_seen.append(self.pin_file.read_text() if self.pin_file.exists() else None)
        self.cursor = target["target_id"]
        if self.navigate:
            root = self.root
            where = {"tree-a": root / "carpeta-a", "tree-b": root / "carpeta-a" / "carpeta-b",
                     "crumb-root": root}.get(target["target_id"])
            if where is not None:
                self.location = where
        if self.damage is not None:
            self.damage(len(self.clicks))
        out = {"status": "effect_observed", "input_delivered": True, "verified": True,
               "observed_effect": "visual_change"}
        return out, False, {}, 5.0

    # UI Automation
    IDENTITY = {("carpeta-a", "TreeItem"): row("9.1", "TreeA", "TreeItem"),
                ("carpeta-b", "TreeItem"): row("9.2", "TreeB", "TreeItem"),
                ("carpeta-a", "ListItem"): row("9.3", "ListA", "ListItem"),
                ("carpeta-b", "ListItem"): row("9.4", "ListB", "ListItem")}

    def identify(self, hwnd, label, control_types):
        assert hwnd == OURS, "UI Automation reads only the recorded window"
        label = str(label)
        found = [dict(r) for (name, ctype), r in self.IDENTITY.items()
                 if name == label and ctype in control_types]
        if label == self.root_name and ({"Button", "SplitButton"} & set(control_types)):
            found.append(row("9.9", "Crumb", "Button"))
        if self.twin_in_sidebar and label == "carpeta-a" and "TreeItem" in control_types:
            found.append(row("9.8", "TreeATwin", "TreeItem"))
        return found

    def element_centre(self, hwnd, label, control_types):
        return (100, 200) if self.on_screen else None

    def cursor_chain(self):
        by_target = {"tree-a": row("9.1", "TreeA", "TreeItem"),
                     "tree-b": row("9.2", "TreeB", "TreeItem"),
                     "list-a": row("9.3", "ListA", "ListItem"),
                     "list-b": row("9.4", "ListB", "ListItem"),
                     "crumb-root": row("9.9", "Crumb", "Button")}
        return [by_target[self.cursor]]

    # the OS
    def open_explorer(self, root):
        self.root = root
        self.location = root
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

    def test_the_run_navigates_with_three_clicks_and_checks_the_folder_after_each(self):
        desk = self.desk()
        res = desk.run_once(1)
        self.assertTrue(res["success"], res)
        self.assertEqual([(c[1], c[2]) for c in desk.clicks],
                         [("carpeta-a", "left_sidebar"), ("carpeta-b", "left_sidebar"),
                          (desk.root_name, "top_bar")])
        self.assertEqual(desk.location, desk.root, "back at the root")
        self.assertEqual((res["independent_checks"], res["independent_failed"]), (3, 0))
        self.assertEqual((res["grounding"], res["grounding_wrong"]), ([True, True, True], 0))
        self.assertEqual((res["actions_requested"], res["delivered"]), (3, 3))
        self.assertTrue(res["tree_unchanged"])
        self.assertEqual(res["e2e"], "E2E-03-plan")

    def test_the_decoy_in_the_file_list_is_never_clicked(self):
        desk = self.desk()
        desk.run_once(1)
        self.assertEqual([c[0] for c in desk.clicks], ["tree-a", "tree-b", "crumb-root"])

    def test_two_elements_with_the_name_in_the_same_zone_stop_the_protocol(self):
        desk = self.desk()
        desk.twin_in_sidebar = True
        with self.assertRaises(driver.Abort):
            desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertEqual(desk.closed_windows, [OURS], "the window it opened is still closed")

    def test_the_identity_of_a_tree_item_is_looked_up_among_tree_items_only(self):
        desk = self.desk()
        seen = []
        original = desk.identify

        def spy(hwnd, label, control_types):
            seen.append((str(label), tuple(control_types)))
            return original(hwnd, label, control_types)

        desk.identify = spy
        desk.run_once(1)
        self.assertIn(("carpeta-a", ("TreeItem",)), seen)
        self.assertNotIn("ListItem", {t for _, types in seen for t in types})

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

    def test_a_click_that_does_not_navigate_ends_the_run_with_no_further_input(self):
        desk = self.desk()
        desk.navigate = False
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(len(desk.clicks), 1, "the second click was not sent")
        self.assertEqual(res["independent_failed"], 1)
        self.assertEqual(res["engine_false_success"], 1, "effect_observed, folder unchanged")

    def test_a_click_that_reached_another_element_ends_the_run(self):
        desk = self.desk()
        desk.cursor_chain = lambda: [row("9.3", "ListA", "ListItem")]
        res = desk.run_once(1)
        self.assertFalse(res["success"])
        self.assertEqual(res["grounding_wrong"], 1)
        self.assertEqual(len(desk.clicks), 1)

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

    def test_the_window_that_cannot_be_told_apart_gets_no_input(self):
        desk = self.desk()
        desk.open_explorer = lambda root: None
        res = desk.run_once(1)
        self.assertIn("could not be told apart", res["failure"])
        self.assertEqual((desk.clicks, desk.ledger_total()), ([], 0))
        self.assertFalse(self.pin.exists(), "no pin without a recorded window")
        self.assertEqual(desk.closed_windows, [])

    def test_the_probe_opens_and_observes_but_clicks_nothing(self):
        desk = self.desk(mode="probe")
        desk.probe_print = lambda elements: None
        probes = []
        with mock.patch.object(driver, "log", side_effect=lambda event, **f: probes.append(event)):
            res = desk.run_once(1)
        self.assertEqual((desk.clicks, desk.ledger_total(), res["steps"]), ([], 0, 0))
        self.assertEqual(probes.count("explorer_probe"), 3)
        self.assertEqual(desk.closed_windows, [OURS])
        self.assertFalse(desk.root.exists())

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


if __name__ == "__main__":
    unittest.main()
