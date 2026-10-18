"""Regression tests for the live phase-1 bug: every editor goal (G07, G08, G09, G11) aborted because the
runner passed ``--folder`` a directory of its own, but ``ommatix-testeditor`` accepts only
``<temporary directory>/ommatix-e2e/<run id>`` (``SaveFolder::new``: wrong shape, exit code 2, no
window). The folder is now made by the S4 clients' own ``RunTree`` / ``new_run_id`` /
``windows_temp_dir`` (tools/s4/e2e_dossier.py), and the runner refuses, before starting a process, a
folder the editor would refuse. No GUI is started by any test here."""

import glob
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(HERE))

import run_goal as rg  # noqa: E402
from test_run_goal import BY_ID, GOALS, Harness, final  # noqa: E402


class FolderShapeTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-folder-")
        self.addCleanup(scratch.cleanup)
        self.temp = Path(scratch.name)

    def make(self, *parts):
        path = self.temp.joinpath(*parts)
        path.mkdir(parents=True)
        return path

    def test_the_editors_shape_is_accepted(self):
        good = self.make("ommatix-e2e", "ag02-G07-20261018-101500")
        self.assertIsNone(rg.editor_folder_problem(good, self.temp))

    def test_every_other_shape_is_refused_including_the_one_that_aborted_the_live_run(self):
        cases = {
            "the previous scratch root": self.make("ommatix-ag02", "G07-4242"),
            "the kit's runs folder": self.make("runs", "20261018T101500", "scratch"),
            "directly under the temporary directory": self.make("ag02-G07"),
            "the run root itself": self.make("ommatix-e2e"),
            "one level too deep": self.make("ommatix-e2e", "ag02-G07", "deeper"),
            "an invalid run id": self.make("ommatix-e2e", "a b"),
        }
        for name, folder in cases.items():
            self.assertIsNotNone(rg.editor_folder_problem(folder, self.temp), name)
        elsewhere = Path(tempfile.mkdtemp(prefix="ag02-elsewhere-"))
        self.addCleanup(elsewhere.rmdir)
        self.assertEqual(rg.editor_folder_problem(elsewhere, self.temp), "outside the temporary directory")

    def test_a_missing_relative_or_absent_folder_is_refused(self):
        self.assertIsNotNone(rg.editor_folder_problem(self.temp / "ommatix-e2e" / "nope", self.temp))
        self.assertEqual(rg.editor_folder_problem(Path("ommatix-e2e/x"), self.temp), "not absolute")
        self.assertIsNotNone(rg.editor_folder_problem(None, self.temp))
        self.assertIsNotNone(rg.editor_folder_problem(self.temp, None))

    def test_the_constants_agree_with_the_editors_source(self):
        source = (REPO / "tools" / "s4" / "testeditor" / "src" / "main.rs").read_text(encoding="utf-8")
        self.assertEqual(re.search(r'const RUN_ROOT: &str = "([^"]+)";', source).group(1), rg.EDITOR_RUN_ROOT)
        self.assertEqual(int(re.search(r"const RUN_ID_LIMIT: usize = (\d+);", source).group(1)), 64)
        self.assertEqual(rg.RUN_ID_RULE.pattern, r"^[A-Za-z0-9._-]{1,64}$")


class FakePopen:
    """Records the command line; ``returncode`` None means a process that stays alive."""
    instances: list = []

    def __init__(self, argv, **kw):
        self.argv, self.pid, self.returncode = list(argv), 4242, None
        FakePopen.instances.append(self)

    def poll(self):
        return self.returncode

    def kill(self):
        self.returncode = -9

    def wait(self, timeout=None):
        return self.returncode


class FakeDriver:
    def foreground(self):
        return {"exe": "elsewhere.exe", "pid": 1, "maximized_primary": True, "class": "x"}

    @staticmethod
    def is_helper_class(name):
        return False


class StartingTheEditorTests(unittest.TestCase):
    def setUp(self):
        FakePopen.instances = []
        scratch = tempfile.TemporaryDirectory(prefix="ag02-start-")
        self.addCleanup(scratch.cleanup)
        self.temp = Path(scratch.name)

    def start(self, goal_id, folder, temp_root):
        with mock.patch.object(rg.subprocess, "Popen", FakePopen):
            return rg.AppUnderTest(BY_ID[goal_id], Path("C:/x/ommatix-testeditor.exe"), folder, FakeDriver(),
                                   temp_root)

    def test_every_editor_goal_gets_a_folder_in_the_editors_shape_on_its_command_line(self):
        for goal_id, flags in (("G07", []), ("G11", []), ("G08", ["--decoy-focus", "--field-digest"]),
                               ("G09", ["--decoy-focus", "--field-digest"])):
            folder = self.temp / "ommatix-e2e" / f"ag02-{goal_id}-20261018-101500"
            folder.mkdir(parents=True)
            FakePopen.instances.clear()
            self.start(goal_id, folder, self.temp)
            argv = FakePopen.instances[0].argv
            self.assertEqual(argv[1], "--folder", goal_id)
            self.assertIsNone(rg.editor_folder_problem(Path(argv[2]), self.temp), goal_id)
            self.assertEqual(argv[3:], flags, goal_id)

    def test_a_folder_the_editor_would_refuse_never_starts_a_process(self):
        wrong = self.temp / "ommatix-ag02" / "G07-4242"  # the shape that aborted the live run
        wrong.mkdir(parents=True)
        with self.assertRaises(ValueError) as caught:
            self.start("G07", wrong, self.temp)
        self.assertIn("the editor's folder", str(caught.exception))
        self.assertEqual(FakePopen.instances, [])

    def test_the_test_application_takes_no_folder(self):
        self.start("G01", None, None)
        self.assertEqual(FakePopen.instances[0].argv[1:], [])

    def test_a_process_that_exits_at_once_ends_the_wait_with_its_exit_code(self):
        folder = self.temp / "ommatix-e2e" / "ag02-G07-x"
        folder.mkdir(parents=True)
        app = self.start("G07", folder, self.temp)
        FakePopen.instances[0].returncode = 2
        started = rg.time.monotonic()
        self.assertFalse(app.wait_foreground(180, lambda text: None))
        self.assertLess(rg.time.monotonic() - started, 5)  # not the 180 s wait
        self.assertEqual(app.exit_code, 2)

    def run_goal_with_the_real_application_class(self, goal_id, exit_code=None):
        init = GOALS["initial_states"][BY_ID[goal_id]["app"]]
        h = Harness(self, goal_id, 1, final_state=dict(init), events=final("DONE"))

        def real_app(goal, exe, folder, driver, temp_root=None):
            with mock.patch.object(rg.subprocess, "Popen", FakePopen):
                app = rg.AppUnderTest(goal, exe, folder, FakeDriver(), temp_root)
            if exit_code is not None:
                FakePopen.instances[-1].returncode = exit_code
            return app
        h.deps.make_app = staticmethod(real_app)
        return h, h.run()

    def test_run_goal_starts_the_editor_in_the_runs_own_correctly_shaped_folder(self):
        h, result = self.run_goal_with_the_real_application_class("G07", exit_code=2)
        argv = FakePopen.instances[0].argv
        folder = Path(argv[argv.index("--folder") + 1])
        self.assertEqual(folder.parent.name, "ommatix-e2e")
        self.assertEqual(folder.parent.parent, h.tmp / "folders")
        self.assertTrue(re.fullmatch(r"ag02-G07-\d{8}-\d{6}", folder.name))
        # the editor exits at once (as it did live): the goal is aborted at once, with the code
        self.assertEqual(result["aborted"], "the application exited before reaching the front (code 2)")
        self.assertFalse((h.tmp / "folders" / "ommatix-e2e").exists())  # the folder was removed

    def test_a_run_folder_in_the_wrong_shape_aborts_the_goal_before_any_process_exists(self):
        init = GOALS["initial_states"]["editor"]
        h = Harness(self, "G07", 1, final_state=dict(init), events=final("DONE"))

        class WrongFolder:
            def __init__(self, goal_id):
                self.temp_root = h.tmp / "folders"
                self.path = self.temp_root / "ommatix-ag02" / f"{goal_id}-4242"  # the old shape

            def create(self):
                self.path.mkdir(parents=True)

            def cleanup(self):
                self.path.rmdir()
                self.path.parent.rmdir()
                return {"left": []}
        h.deps.make_folder = staticmethod(WrongFolder)
        h.deps.make_app = staticmethod(lambda goal, exe, folder, driver, temp_root=None: (
            rg.AppUnderTest(goal, exe, folder, FakeDriver(), temp_root)))
        with mock.patch.object(rg.subprocess, "Popen", FakePopen):
            result = h.run()
        self.assertIn("the editor's folder", result["aborted"])
        self.assertEqual(FakePopen.instances, [])

    def test_an_editor_note_is_read_from_the_same_folder_the_editor_was_given(self):
        init = GOALS["initial_states"]["editor"]
        h = Harness(self, "G07", 2, final_state={"caracteres": 4, "guardado": "si"}, events=final("DONE"),
                    write_note=b"nota", proxy_records=[{"dir": "proxy", "kind": "end", "engine_exit": 0}])
        result = h.run()
        self.assertEqual(Path(h.folder).parent.name, "ommatix-e2e")
        self.assertEqual(result["note"], "ok")
        self.assertTrue(result["oracle"]["ok"])
        self.assertEqual(init["caracteres"], 0)


@unittest.skipUnless(sys.platform == "win32", "the S4 helpers import Windows APIs")
class RealS4HelpersTests(unittest.TestCase):
    """The folder is made by tools/s4/e2e_dossier.py's own RunTree, new_run_id and windows_temp_dir."""

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-tree-")
        self.addCleanup(scratch.cleanup)
        self.temp = Path(scratch.name)
        self.dossier = rg.windows_dossier()
        patcher = mock.patch.object(self.dossier, "windows_temp_dir", return_value=self.temp)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_the_editor_run_is_a_real_runtree_in_the_editors_shape(self):
        run = rg.EditorRun("G07", self.dossier)
        self.assertIsInstance(run.tree, self.dossier.RunTree)
        path = run.create()
        self.assertEqual(os.listdir(path), [])  # empty
        self.assertIsNone(rg.editor_folder_problem(path, run.temp_root))
        self.assertEqual(run.temp_root, self.temp)
        self.assertEqual(self.dossier.RUN_ROOT, rg.EDITOR_RUN_ROOT)
        self.assertTrue(self.dossier.valid_run_id(path.name))

    def test_the_folder_is_exclusive_and_removed_with_its_note_and_nothing_else(self):
        with mock.patch.object(self.dossier, "new_run_id", return_value="ag02-G07-fixed"):
            run = rg.EditorRun("G07", self.dossier)
            path = run.create()
            second = rg.EditorRun("G07", self.dossier)
            with self.assertRaises(rg.EditorRunRefused):  # a folder it did not create is never adopted
                second.create()
            self.assertEqual(second.cleanup(), {"left": [], "not_ours": True})
        (path / "nota.txt").write_bytes(b"nota")
        (path / "ajeno.txt").write_bytes(b"x")
        outcome = run.cleanup()
        self.assertTrue((path / "ajeno.txt").exists())  # an unexpected entry is left alone and reported
        self.assertTrue(outcome["left"])
        self.assertFalse((path / "nota.txt").exists())
        (path / "ajeno.txt").unlink()
        path.rmdir()
        (self.temp / "ommatix-e2e").rmdir()

    def test_a_clean_run_leaves_nothing_in_the_temporary_directory(self):
        run = rg.EditorRun("G08", self.dossier)
        path = run.create()
        (path / "nota.txt").write_bytes(b"nota")
        self.assertEqual(run.cleanup()["left"], [])
        self.assertEqual(os.listdir(self.temp), [])


def find_testeditor():
    candidates = [os.environ.get("OMMATIX_TESTEDITOR_EXE", "")]
    for base in (REPO, *REPO.parent.glob("Ommatix*")):
        for profile in ("release", "debug"):
            candidates += glob.glob(str(base / "tools" / "s4" / "testeditor" / "target" / profile /
                                        "ommatix-testeditor.exe"))
    return next((c for c in candidates if c and Path(c).is_file()), None)


@unittest.skipUnless(sys.platform == "win32" and find_testeditor(), "no built ommatix-testeditor.exe")
class RealEditorArgumentValidationTests(unittest.TestCase):
    """The real binary's own validation, WRONG shapes only: it exits with code 2 before any window.
    The correct shape is never run here (it would open a window); it is covered by the editor's unit
    tests and by the constants test above."""

    def run_editor(self, *args):
        done = subprocess.run([find_testeditor(), *args], capture_output=True, timeout=30, check=False,
                              creationflags=0x08000000)
        return done.returncode

    def test_the_shapes_that_aborted_the_live_run_exit_with_code_2_and_the_runner_refuses_them_too(self):
        with tempfile.TemporaryDirectory(prefix="ag02-real-") as tmp:
            tmp = Path(tmp)
            wrong = [tmp / "runs" / "G07", tmp / "ommatix-ag02" / "G07-4242", tmp / "ommatix-e2e",
                     tmp / "ommatix-e2e" / "a b"]
            for folder in wrong:
                folder.mkdir(parents=True)
                self.assertEqual(self.run_editor("--folder", str(folder)), 2, str(folder))
                self.assertIsNotNone(rg.editor_folder_problem(folder, tempfile.gettempdir()), str(folder))
            self.assertEqual(self.run_editor(), 2)
            self.assertEqual(self.run_editor("--folder"), 2)


if __name__ == "__main__":
    unittest.main()
