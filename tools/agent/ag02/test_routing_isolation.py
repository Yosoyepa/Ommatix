"""The AG-02 and AG-03 tests are not refused by the developer's own provider-routing variables.

The tools refuse a real session while a variable that selects another provider or endpoint is set
(``run_goal.routing_problems``, ADR-0036). A Claude Code session sets ``ANTHROPIC_BASE_URL`` in the
environment of the processes it starts, so, run from one, every test that drives a tool's ``main``
was refused by that variable instead of exercising what it tests (64 tests on 2026-11-01; CI, which
has no such variable, never saw it).
"""

import os
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import routing_isolation  # noqa: E402
import run_goal as rg  # noqa: E402


class RoutingIsolationTests(unittest.TestCase):
    def test_start_removes_every_routing_variable_and_stop_puts_them_back(self):
        ambient = {"ANTHROPIC_BASE_URL": "http://127.0.0.1:9", "CLAUDE_CODE_USE_BEDROCK": "1",
                   "ANTHROPIC_VERTEX_PROJECT_ID": "p"}
        with mock.patch.dict(os.environ, ambient):
            path = os.environ.get("PATH")
            routing_isolation.start()
            try:
                self.assertEqual(rg.routing_problems(os.environ), [])
                self.assertEqual(os.environ.get("PATH"), path, "only routing variables are removed")
            finally:
                routing_isolation.stop()
            self.assertEqual(rg.routing_problems(os.environ), sorted(ambient))

    def test_a_module_that_drives_a_main_passes_with_a_routing_variable_in_the_environment(self):
        env = dict(os.environ, ANTHROPIC_BASE_URL="http://127.0.0.1:9")
        done = subprocess.run([sys.executable, "-m", "unittest", "test_routecost.FakeCliTests"], cwd=HERE,
                              env=env, capture_output=True, text=True, timeout=300, check=False)
        self.assertEqual(done.returncode, 0, done.stderr[-2000:])


if __name__ == "__main__":
    unittest.main()
