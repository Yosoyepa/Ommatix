"""Tests of the S3 driver's engine command (no engine, no screen, no input)."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import mcp_driver  # noqa: E402


class ServeArgvTest(unittest.TestCase):
    def test_a_live_run_passes_no_fixed_session_id(self):
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", True, None)
        self.assertNotIn("--session-id", argv)
        self.assertEqual(argv[-2:], ["--input", "sendinput"])

    def test_a_live_run_with_a_session_id_is_refused_before_the_engine_starts(self):
        with self.assertRaises(ValueError):
            mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", True, "s3-supervised")

    def test_observe_and_dry_runs_keep_an_identifier(self):
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", False, None)
        self.assertEqual(argv[-2:], ["--session-id", mcp_driver.PROBE_SESSION_ID])
        self.assertNotIn("--input", argv)
        argv = mcp_driver.serve_argv("ommatix.exe", 0, "c.toml", False, "mine")
        self.assertEqual(argv[-2:], ["--session-id", "mine"])


if __name__ == "__main__":
    unittest.main()
