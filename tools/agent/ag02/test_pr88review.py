"""Regression tests for the review of PR 88 (F1-F4). See docs/reviews/2026-10-17-pr86-ag02-harness.md.

F1 cleanup after a refused creation must delete nothing; F2 a redirected parent or temporary directory
is refused before anything is created; F3 the guard compares as the editor's Rust does (case-sensitive
``ommatix-e2e``), checked against the editor's own ``SaveFolder::new`` compiled as a probe; F4 historical
per-message figures are never presented as context. No GUI is started."""

import os
import re
import shutil
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
import summarise  # noqa: E402
from test_run_goal import GOALS, Harness, final  # noqa: E402


class FakeAbort(Exception):
    pass


class FakeRunTree:
    """The semantics of tools/s4/e2e_dossier.py's RunTree that matter here: exclusive creation (a
    folder that exists is refused with Abort) and a cleanup that ADOPTS the named files found in the
    run folder - which is exactly what deleted another run's note before the fix."""

    def __init__(self, temp_root, run_id):
        self.base = Path(temp_root) / "ommatix-e2e"
        self.root = self.base / run_id
        self.dirs, self.files, self.created_base = [], [], False

    def create(self):
        if not self.base.is_dir():
            try:
                self.base.mkdir()
                self.created_base = True
            except FileExistsError:
                pass
        try:
            self.root.mkdir()
        except FileExistsError as error:
            raise FakeAbort("the run folder already exists: not adopted") from error
        self.dirs.append(self.root)
        return self.root

    def cleanup(self, adopt=()):
        outcome = {"deleted_files": 0, "deleted_dirs": 0, "left": []}
        files = list(self.files)
        for name in adopt:
            path = self.root / name
            if path.parent == self.root and path.is_file() and not path.is_symlink() and path not in files:
                files.append(path)
        for path in reversed(files):
            path.unlink()
            outcome["deleted_files"] += 1
        for path in reversed(self.dirs):
            try:
                path.rmdir()
                outcome["deleted_dirs"] += 1
            except OSError:
                outcome["left"].append(path.name)
        if self.created_base:
            try:
                self.base.rmdir()
            except OSError:
                outcome["left"].append("ommatix-e2e")
        return outcome


class FakeDossier:
    Abort = FakeAbort
    RunTree = FakeRunTree

    def __init__(self, temp, run_id="ag02-G07-20261018-101500"):
        self.temp, self.run_id = Path(temp), run_id

    def windows_temp_dir(self):
        return self.temp

    def new_run_id(self, prefix):
        return self.run_id


def make_link(link: Path, target: Path) -> bool:
    """A junction on Windows (no privilege needed), a symbolic link elsewhere; False if impossible."""
    try:
        if os.name == "nt":
            done = subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(target)],
                                  capture_output=True, check=False)
            return done.returncode == 0
        os.symlink(target, link, target_is_directory=True)
        return True
    except (OSError, NotImplementedError):
        return False


def remove_link(link: Path):
    try:
        os.rmdir(link) if os.name == "nt" else os.unlink(link)
    except OSError:
        pass


class Base(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-r88-")
        self.addCleanup(scratch.cleanup)
        self.tmp = Path(scratch.name)
        self.temp = self.tmp / "temp"
        self.temp.mkdir()


class RefusedCreationDeletesNothingTests(Base):
    """F1."""

    def existing_run(self):
        folder = self.temp / "ommatix-e2e" / "ag02-G07-20261018-101500"
        folder.mkdir(parents=True)
        (folder / "nota.txt").write_bytes(b"another run's note")
        (folder / "ajeno.txt").write_bytes(b"unrelated")
        return folder

    def test_run_goal_leaves_a_colliding_runs_note_and_files_untouched_and_records_the_abort(self):
        folder = self.existing_run()
        init = GOALS["initial_states"]["editor"]
        h = Harness(self, "G07", 1, final_state=dict(init), events=final("DONE"))
        started = []
        h.deps.make_folder = staticmethod(lambda goal_id: rg.EditorRun(goal_id, FakeDossier(self.temp)))
        h.deps.make_app = staticmethod(lambda *a, **k: started.append(a))
        result = h.run()
        self.assertEqual(result["aborted"], "the editor's run folder was refused: run_folder_already_exists")
        self.assertEqual((folder / "nota.txt").read_bytes(), b"another run's note")
        self.assertEqual((folder / "ajeno.txt").read_bytes(), b"unrelated")
        self.assertTrue(folder.is_dir())
        self.assertEqual(started, [])  # no application started
        self.assertNotIn("score", result)

    def test_cleanup_before_or_after_a_refused_creation_removes_nothing(self):
        folder = self.existing_run()
        run = rg.EditorRun("G07", FakeDossier(self.temp))
        self.assertEqual(run.cleanup(), {"left": [], "not_ours": True})  # never created
        with self.assertRaises(rg.EditorRunRefused) as caught:
            run.create()
        self.assertEqual(caught.exception.reason, "run_folder_already_exists")
        self.assertFalse(run.owned)
        self.assertEqual(run.cleanup(), {"left": [], "not_ours": True})
        self.assertEqual(sorted(os.listdir(folder)), ["ajeno.txt", "nota.txt"])

    def test_an_owned_folder_is_still_cleaned_up_with_its_note_and_nothing_else(self):
        run = rg.EditorRun("G07", FakeDossier(self.temp))
        path = run.create()
        self.assertTrue(run.owned)
        (path / "nota.txt").write_bytes(b"nota")
        (path / "ajeno.txt").write_bytes(b"x")
        outcome = run.cleanup()
        self.assertEqual(outcome["deleted_files"], 1)
        self.assertEqual(os.listdir(path), ["ajeno.txt"])  # an entry it did not create is left, reported

    def test_a_folder_that_no_longer_resolves_to_what_was_created_is_not_adopted_or_removed(self):
        run = rg.EditorRun("G07", FakeDossier(self.temp))
        path = run.create()
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "nota.txt").write_bytes(b"not ours")
        path.rmdir()  # swapped after creation
        if not make_link(path, elsewhere):
            self.skipTest("links are not available here")
        self.addCleanup(remove_link, path)
        outcome = run.cleanup()
        self.assertTrue(outcome["left"])
        self.assertEqual((elsewhere / "nota.txt").read_bytes(), b"not ours")

    def test_other_closed_refusals_become_an_aborted_attempt_not_an_uncaught_exception(self):
        init = GOALS["initial_states"]["editor"]
        h = Harness(self, "G07", 1, final_state=dict(init), events=final("DONE"))

        class Failing:
            def __init__(self, goal_id):
                raise rg.EditorRunRefused("temporary_directory_or_run_id_refused")
        h.deps.make_folder = staticmethod(Failing)
        self.assertEqual(h.run()["aborted"],
                         "the editor's run folder was refused: temporary_directory_or_run_id_refused")


class RedirectedParentTests(Base):
    """F2."""

    def setUp(self):
        super().setUp()
        self.outside = self.tmp / "outside"
        self.outside.mkdir()
        (self.outside / "sentinel.txt").write_bytes(b"outside")

    def test_a_junction_or_link_as_the_shared_parent_is_refused_before_anything_is_created(self):
        link = self.temp / "ommatix-e2e"
        if not make_link(link, self.outside):
            self.skipTest("links are not available here")
        self.addCleanup(remove_link, link)
        run = rg.EditorRun("G07", FakeDossier(self.temp))
        with self.assertRaises(rg.EditorRunRefused) as caught:
            run.create()
        self.assertEqual(caught.exception.reason, "shared_parent_redirected")
        self.assertEqual(os.listdir(self.outside), ["sentinel.txt"])  # nothing created beyond the link
        self.assertEqual((self.outside / "sentinel.txt").read_bytes(), b"outside")
        self.assertEqual(run.cleanup(), {"left": [], "not_ours": True})

    def test_run_goal_refuses_a_redirected_parent_and_starts_no_process(self):
        link = self.temp / "ommatix-e2e"
        if not make_link(link, self.outside):
            self.skipTest("links are not available here")
        self.addCleanup(remove_link, link)
        init = GOALS["initial_states"]["editor"]
        h = Harness(self, "G07", 1, final_state=dict(init), events=final("DONE"))
        started = []
        h.deps.make_folder = staticmethod(lambda goal_id: rg.EditorRun(goal_id, FakeDossier(self.temp)))
        h.deps.make_app = staticmethod(lambda *a, **k: started.append(a))
        result = h.run()
        self.assertEqual(result["aborted"], "the editor's run folder was refused: shared_parent_redirected")
        self.assertEqual((started, os.listdir(self.outside)), ([], ["sentinel.txt"]))

    def test_a_redirected_temporary_directory_is_refused_too(self):
        link = self.tmp / "temp-link"
        if not make_link(link, self.temp):
            self.skipTest("links are not available here")
        self.addCleanup(remove_link, link)
        with self.assertRaises(rg.EditorRunRefused) as caught:
            rg.EditorRun("G07", FakeDossier(link)).create()
        self.assertEqual(caught.exception.reason, "temporary_directory_redirected")
        self.assertEqual(os.listdir(self.temp), [])

    def test_the_guard_refuses_any_reparse_point_in_the_chain_even_if_the_shape_resolves(self):
        real = self.temp / "ommatix-e2e" / "ag02-G07-x"
        real.mkdir(parents=True)
        link = self.tmp / "alias"
        if not make_link(link, self.temp):
            self.skipTest("links are not available here")
        self.addCleanup(remove_link, link)
        self.assertIsNone(rg.editor_folder_problem(real, self.temp))
        self.assertEqual(rg.editor_folder_problem(link / "ommatix-e2e" / "ag02-G07-x", self.temp),
                         "reparse point in the path")

    @unittest.skipUnless(sys.platform == "win32", "the S4 helpers import Windows APIs")
    def test_with_the_real_s4_helpers_an_outside_target_stays_unchanged(self):
        dossier = rg.windows_dossier()
        link = self.temp / "ommatix-e2e"
        if not make_link(link, self.outside):
            self.skipTest("junctions are not available here")
        self.addCleanup(remove_link, link)
        with mock.patch.object(dossier, "windows_temp_dir", return_value=self.temp):
            with self.assertRaises(rg.EditorRunRefused):
                rg.EditorRun("G07", dossier).create()
        self.assertEqual(os.listdir(self.outside), ["sentinel.txt"])


def _block(source: str, start: str) -> str:
    """The text from `start` to the brace that closes its first block."""
    begin = source.index(start)
    depth, i = 0, source.index("{", begin)
    for j in range(i, len(source)):
        depth += source[j] == "{"
        depth -= source[j] == "}"
        if depth == 0:
            return source[begin:j + 1]
    raise AssertionError("unbalanced")


def rust_probe_exe(tmp: Path):
    """The editor's own ``valid_run_id`` and ``SaveFolder::new`` (std only, no GUI code), extracted
    from its source and compiled with rustc; None if rustc is missing."""
    if not shutil.which("rustc"):
        return None
    source = (REPO / "tools" / "s4" / "testeditor" / "src" / "main.rs").read_text(encoding="utf-8")
    enum_start = source.index("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\nenum FolderError")
    program = "\n".join([
        "use std::ffi::OsStr;", "use std::path::{Component, Path, PathBuf};",
        re.search(r'const RUN_ROOT: &str = "[^"]+";', source).group(0),
        re.search(r"const RUN_ID_LIMIT: usize = \d+;", source).group(0),
        _block(source[enum_start:], "enum FolderError") if False else
        source[enum_start:enum_start + len("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n") +
               len(_block(source[enum_start:], "enum FolderError"))],
        _block(source, "fn valid_run_id"),
        "struct SaveFolder { dir: PathBuf }",
        "impl SaveFolder {", _block(source, "fn new(dir: &Path, temp_root: &Path)"), "}",
        'fn main() { let a: Vec<String> = std::env::args().collect();'
        ' match SaveFolder::new(Path::new(&a[1]), Path::new(&a[2])) {'
        ' Ok(_) => println!("ok"), Err(e) => println!("{:?}", e) } }'])
    src = tmp / "probe.rs"
    src.write_text(program, encoding="utf-8")
    exe = tmp / ("probe.exe" if os.name == "nt" else "probe")
    done = subprocess.run(["rustc", "--edition", "2021", "-A", "warnings", "-o", str(exe), str(src)],
                          capture_output=True, text=True, check=False)
    return exe if done.returncode == 0 else None


class CaseParityTests(Base):
    """F3."""

    def test_a_parent_stored_in_another_case_is_refused_by_the_guard_and_before_creation(self):
        upper = self.temp / "OMMATIX-E2E"
        upper.mkdir()
        run_dir = upper / "ag02-G07-x"
        run_dir.mkdir()
        self.assertIsNotNone(rg.editor_folder_problem(run_dir, self.temp))  # as Rust: WrongShape
        with self.assertRaises(rg.EditorRunRefused) as caught:
            rg.EditorRun("G07", FakeDossier(self.temp)).create()
        self.assertEqual(caught.exception.reason, "shared_parent_stored_under_another_case")
        self.assertEqual(os.listdir(upper), ["ag02-G07-x"])  # nothing created, nothing renamed

    def test_the_ordinary_lowercase_parent_is_accepted_and_the_run_id_may_be_any_case(self):
        for run_id in ("ag02-G07-x", "AG02-g07-X"):
            folder = self.temp / "ommatix-e2e" / run_id
            folder.mkdir(parents=True, exist_ok=True)
            self.assertIsNone(rg.editor_folder_problem(folder, self.temp), run_id)

    def test_the_guard_agrees_with_the_editors_own_function_on_every_case(self):
        exe = rust_probe_exe(self.tmp)
        if exe is None:
            self.skipTest("no rustc")
        root = Path(os.path.realpath(self.temp))
        cases = [("ommatix-e2e", "ag02-G07-1"), ("ommatix-e2e", "AG02-g07-1"), ("OMMATIX-E2E", "ag02-G07-1"),
                 ("Ommatix-E2E", "ag02-G07-1"), ("ommatix-e2e", "a b"), ("ommatix-e2e", "x" * 65),
                 ("ommatix-e2e", "x" * 64), ("ommatix-ag02", "G07-4242"), ("runs", "G07")]
        for index, (parent, run_id) in enumerate(cases):
            folder = root / f"case{index}" / parent / run_id
            folder.mkdir(parents=True)
            temp_root = folder.parents[1]
            # the editor judges <temp>/<parent>/<id> against its temporary directory = case dir
            theirs = subprocess.run([str(exe), str(folder), str(temp_root)], capture_output=True, text=True,
                                    check=True).stdout.strip()
            ours = rg.editor_folder_problem(folder, temp_root)
            self.assertEqual(theirs == "ok", ours is None, (parent, run_id, theirs, ours))

    def test_the_names_the_guard_compares_are_the_editors_constants(self):
        source = (REPO / "tools" / "s4" / "testeditor" / "src" / "main.rs").read_text(encoding="utf-8")
        self.assertIn("*root_dir != OsStr::new(RUN_ROOT)", source)  # a case-sensitive comparison
        self.assertEqual(re.search(r'const RUN_ROOT: &str = "([^"]+)";', source).group(1), rg.EDITOR_RUN_ROOT)


class HistoricalPerMessageFigureTests(unittest.TestCase):
    """F4."""

    def report(self, per_message, cli_usage=True):
        usage = {"input_tokens": 8, "cache_creation_input_tokens": 10218, "cache_read_input_tokens": 20775,
                 "output_tokens": 1490} if cli_usage else None
        return {"goal": "G01", "phase": 1, "expected": "done", "score": {"success": True},
                "metrics": {"tokens": {"input_tokens": 8, "output_tokens": 45}, "cli_usage": usage,
                            "cost_usd": 0.00299715, "tokens_per_message": per_message}}

    LEGACY = [{"input": 2, "output": 28}, {"input": 2, "output": 7}]
    NEW = [{"input": 2, "output": 28, "cache_creation": 475, "cache_read": 9743},
           {"input": 2, "output": 7, "cache_creation": 100, "cache_read": 10000}]

    def phase(self, *reports):
        return summarise.summarise(list(reports))["phases"]["1"]

    def test_a_historical_report_has_no_per_message_context_figure_and_the_totals_survive(self):
        phase = self.phase(self.report(self.LEGACY))
        self.assertEqual((phase["context_tokens_total"], phase["output_tokens_total"]), (31001, 1490))
        self.assertEqual(phase["cost_usd_total"], 0.00299715)
        self.assertIsNone(phase["context_tokens_per_message_median"])  # not 2.0
        self.assertEqual(phase["context_per_message_coverage"], {"measured": 0, "unavailable": 2})
        self.assertEqual(phase["fresh_input_per_message_median_extra_uncached_only"], 2)
        self.assertEqual(phase["token_sources"], {"cli_usage": 1})

    def test_the_rendering_says_unavailable_and_labels_the_fresh_only_figure_as_an_extra(self):
        text = summarise.render(summarise.summarise([self.report(self.LEGACY)]))
        self.assertIn("per message (median) unavailable (historical report", text)
        self.assertIn("[extra, uncached input only] median 2", text)
        self.assertNotRegex(text, r"per message \(median\) 2(\.0)?\b")

    def test_mixed_old_and_new_reports_never_mix_unknown_cache_into_the_median(self):
        phase = self.phase(self.report(self.LEGACY), self.report(self.NEW))
        self.assertEqual(phase["context_per_message_coverage"], {"measured": 2, "unavailable": 2})
        self.assertEqual(phase["context_tokens_per_message_median"], 10161.0)  # the two measured only
        text = summarise.render(summarise.summarise([self.report(self.LEGACY), self.report(self.NEW)]))
        self.assertIn("10161.0 over 2 measured messages; 2 historical messages without cache fields left out",
                      text)

    def test_a_measured_zero_is_not_unavailable_and_a_null_is(self):
        zero = [{"input": 5, "output": 1, "cache_creation": 0, "cache_read": 0}]
        none = [{"input": 5, "output": 1, "cache_creation": None, "cache_read": 0}]
        self.assertEqual(self.phase(self.report(zero))["context_per_message_coverage"],
                         {"measured": 1, "unavailable": 0})
        self.assertEqual(self.phase(self.report(zero))["context_tokens_per_message_median"], 5)
        self.assertEqual(self.phase(self.report(none))["context_per_message_coverage"],
                         {"measured": 0, "unavailable": 1})

    def test_current_reports_and_the_stream_fallback_are_unchanged(self):
        phase = self.phase(self.report(self.NEW))
        self.assertEqual((phase["context_tokens_per_message_median"], phase["context_tokens_total"]), (10161.0, 31001))
        legacy = self.phase(self.report(self.LEGACY, cli_usage=False))
        self.assertEqual(legacy["token_sources"], {"stream": 1})


if __name__ == "__main__":
    unittest.main()
