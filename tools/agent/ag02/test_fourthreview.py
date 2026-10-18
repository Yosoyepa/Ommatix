"""Regression test for Q1 of the fourth review of PR 86: a proof issued by an older, weaker
verifier must not authorise a run. See docs/reviews/2026-10-17-pr86-ag02-harness.md.

``fixtures/stale_proof_v2.json`` is a genuine artifact: the previous verifier's own preflight
(``b7e3848``) run against the scripted CLI while the production directory's accented instruction
reached the request. That verifier searched raw bytes, so it issued ``passed: true``. Nothing in
the file was edited."""

import argparse
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import preflight  # noqa: E402
import run_goal as rg  # noqa: E402
from test_preflight import PreflightBase  # noqa: E402
from test_run_goal import make_proof  # noqa: E402

STALE = HERE / "fixtures" / "stale_proof_v2.json"


class StaleProofTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="ag02-q1-")
        self.addCleanup(scratch.cleanup)
        self.tmp = Path(scratch.name)

    def test_the_fixture_is_the_old_verifiers_genuine_false_pass(self):
        stale = json.loads(STALE.read_text(encoding="utf-8"))
        self.assertEqual((stale["schema"], stale["passed"], stale["problems"]),
                         ("ag02-isolation-proof-2", True, []))
        self.assertNotIn("verifier_revision", stale)
        self.assertEqual(stale["production_dir_context_lines_checked"], 1)
        self.assertEqual(stale["production_dir_context_lines_in_request"], 0)  # the leak it could not see

    def test_the_current_verifier_fails_the_same_scenario_so_the_old_proof_was_wrong(self):
        class Base(PreflightBase):
            def runTest(self):  # pragma: no cover
                pass
        helper = Base()
        helper.addCleanup = self.addCleanup
        proof = helper.run_preflight("leak_dir", files={
            "CLAUDE.md": 'Instrucción privada número uno: dice "hola" y usa C:\\ruta\\secreta.\n'})
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_context_in_request", proof["problems"])

    def test_the_loader_refuses_the_stale_proof_with_an_instruction_to_run_the_preflight_again(self):
        copy = self.tmp / "proof.json"
        shutil.copy(STALE, copy)
        with self.assertRaises(SystemExit) as caught:
            rg.load_isolation_proof(copy)
        message = str(caught.exception)
        self.assertIn("older or different verifier", message)
        self.assertIn("run preflight.py again", message)
        self.assertIn("even if Claude Code, the model and the configuration directory are unchanged", message)

    def test_any_proof_without_the_exact_current_revision_is_refused(self):
        good = make_proof()
        self.assertEqual(good["verifier_revision"], rg.VERIFIER_REVISION)
        path = self.tmp / "p.json"

        def load(proof):
            path.write_text(json.dumps(proof), encoding="utf-8")
            return rg.load_isolation_proof(path)
        self.assertEqual(load(good)["verifier_revision"], rg.VERIFIER_REVISION)
        missing = {k: v for k, v in good.items() if k != "verifier_revision"}
        for bad in (missing, good | {"verifier_revision": rg.VERIFIER_REVISION - 1},
                    good | {"verifier_revision": rg.VERIFIER_REVISION + 1},
                    good | {"verifier_revision": str(rg.VERIFIER_REVISION)},
                    good | {"verifier_revision": float(rg.VERIFIER_REVISION)},
                    good | {"verifier_revision": True}, good | {"verifier_revision": None},
                    good | {"schema": "ag02-isolation-proof-2"}):
            with self.assertRaises(SystemExit, msg=str(bad.get("verifier_revision"))):
                load(bad)

    def run_main(self, proof_path):
        (self.tmp / "cfg").mkdir(exist_ok=True)
        config = self.tmp / "cfg" / "engine.toml"
        config.write_text("[safety]\nactuation_enabled = false\n", encoding="utf-8")
        argv = ["--phase", "1", "--model", "claude-haiku-5-5", "--exe", str(self.tmp / "ommatix.exe"),
                "--config", str(config), "--isolation-proof", str(proof_path),
                "--claude-config-dir", str(self.tmp / "home"), "--out", str(self.tmp / "out"), "--goal", "G01"]
        with mock.patch.object(rg, "windows_driver") as driver, \
                mock.patch.object(rg, "run_series", return_value=0) as series, \
                mock.patch.dict(rg.os.environ, {}, clear=False):
            for name in rg.routing_problems(rg.os.environ):
                rg.os.environ.pop(name, None)
            try:
                code = rg.main(argv)
                error = None
            except SystemExit as stop:
                code, error = None, str(stop)
        return code, error, driver, series

    def test_main_refuses_the_stale_proof_before_any_application_or_agent_starts(self):
        proof = self.tmp / "stale.json"
        shutil.copy(STALE, proof)
        code, error, driver, series = self.run_main(proof)
        self.assertIsNone(code)
        self.assertIn("older or different verifier", error)
        driver.assert_not_called()
        series.assert_not_called()
        self.assertFalse((self.tmp / "out").exists())

    def test_a_fresh_proof_from_the_current_verifier_is_accepted_and_the_run_proceeds(self):
        class Base(PreflightBase):
            def runTest(self):  # pragma: no cover
                pass
        helper = Base()
        helper.addCleanup = self.addCleanup
        fresh = helper.run_preflight("normal")
        self.assertTrue(fresh["passed"], fresh["problems"])
        self.assertEqual((fresh["schema"], fresh["verifier_revision"]),
                         (rg.PROOF_SCHEMA, rg.VERIFIER_REVISION))
        proof = self.tmp / "fresh.json"
        proof.write_text(json.dumps(fresh), encoding="utf-8")
        code, error, driver, series = self.run_main(proof)
        self.assertEqual((code, error), (0, None))
        series.assert_called_once()

    def test_the_preflight_writes_the_revision_it_verifies_by(self):
        source = (HERE / "preflight.py").read_text(encoding="utf-8")
        self.assertIn('"verifier_revision": rg.VERIFIER_REVISION', source)


if __name__ == "__main__":
    unittest.main()
