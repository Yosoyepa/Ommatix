"""What a session leaves on disk (the on-disk investigation of 2026-10-20; docs/reviews/2026-10-20-ag03-disk-evidence.md).

Measured with the real CLI 2.1.304 against the loopback stand-in and the synthetic pixel server: with the production
command (``--no-session-persistence``) nothing under the isolated configuration directory holds an image; WITHOUT that
option Claude Code writes every MCP image result in clear under ``<config>/projects/<cwd>/<session>/tool-results/`` and
every transcript with the image in base64. The CLI also keeps a debug log of the MCP server outside the configuration
directory (``<cache>/claude-cli-nodejs/Cache/<cwd>/mcp-logs-pixel/``), with no image on synthetic sessions.

So the isolation preflight now refuses a proof when, after its synthetic sessions, the configuration directory holds a
project transcript, a ``tool-results`` file, anything written under ``projects/`` or any PNG / base64 PNG data, or the CLI's
cache holds image data; ``--no-session-persistence`` and the agent's feedback-survey switch are checked invariants of the
production command and environment. Stand-ins and fakes only: no provider, no screen, no real capture."""

import argparse
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import run_goal as rg  # noqa: E402
from pin_support import pinned  # noqa: E402

FAKE_CLI = [sys.executable, str(HERE / "fake_claude_pixel.py")]
SURVEY = "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY"


class PreflightHarness(unittest.TestCase):
    def preflight(self, mode="normal", *, route=None, argv_filter=None):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-disk-")
        self.addCleanup(scratch.cleanup)
        tmp = Path(scratch.name)
        self.env_file, self.production, self.cache = tmp / "env.json", tmp / "production", tmp / "cache"
        self.production.mkdir()
        self.cache.mkdir()
        extra = {"FAKE_CLAUDE_MODE": mode, "FAKE_CLAUDE_ENV_FILE": str(self.env_file),
                 "FAKE_CLAUDE_CACHE_ROOT": str(self.cache), "FAKE_CLAUDE_LEAK_DIR": str(self.production)}
        model, url = (agentcli.CLAUDE_ROUTES[route]["model"], agentcli.CLAUDE_ROUTES[route]["base_urls"][0]) \
            if route else ("claude-haiku-5-5", None)
        args = argparse.Namespace(exe=Path(sys.executable), model=model, claude=FAKE_CLI, timeout=60,
                                  claude_config_dir=self.production, extra_env=extra, route=route, route_base_url=url)
        patches = [mock.patch.object(pf.ag02, "refusals", return_value=[]), pinned()]
        try:
            import pixel_disk  # noqa: PLC0415
        except ImportError:  # before the fix there is no module (and no cache check): the test then fails on its merits
            pixel_disk = None
        if pixel_disk is not None:
            patches.append(mock.patch.object(pixel_disk, "cli_cache_root", return_value=self.cache))
        if argv_filter is not None:
            real = pa.build_claude_argv
            patches.append(mock.patch.object(pa, "build_claude_argv",
                                             lambda **kw: argv_filter(real(**kw))))
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        return pf.run(args)


class OnDiskEvidenceTests(PreflightHarness):
    def test_a_session_that_writes_nothing_passes_and_the_proof_records_the_disk_facts(self):
        proof = self.preflight()
        self.assertTrue(proof["passed"], proof["problems"])
        disk = proof["disk"]
        self.assertTrue(disk["no_session_persistence_flag"])
        self.assertFalse(disk["image_data_found"])
        self.assertEqual(disk["config_files_touched"], {"canary": 0, "control": 0, "production_dir": 0})
        self.assertEqual(proof["agent_env"], pa.PIXEL_ENV)

    def test_tool_results_images_in_the_configuration_directory_refuse_the_proof(self):
        proof = self.preflight("persist_tool_results")
        self.assertFalse(proof["passed"])
        for token in ("production_dir_disk_config_tool_results", "production_dir_disk_config_image_data",
                      "production_dir_disk_config_projects_written"):
            self.assertIn(token, proof["problems"])

    def test_a_transcript_with_an_image_in_the_configuration_directory_refuses_the_proof(self):
        proof = self.preflight("persist_transcript")
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_disk_config_transcript", proof["problems"])
        self.assertIn("production_dir_disk_config_image_data", proof["problems"])

    def test_a_transcript_without_an_image_still_refuses_the_proof(self):
        proof = self.preflight("persist_clean_transcript")
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_disk_config_transcript", proof["problems"])
        self.assertNotIn("production_dir_disk_config_image_data", proof["problems"])

    def test_image_data_in_the_cli_cache_log_refuses_the_proof(self):
        proof = self.preflight("cache_log_image")
        self.assertFalse(proof["passed"])
        self.assertIn("production_dir_disk_cache_image_data", proof["problems"])

    def test_a_cache_log_without_an_image_passes_and_is_counted(self):
        proof = self.preflight("cache_log_clean")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertGreaterEqual(proof["disk"]["cache_files_touched"], 1)

    def test_the_other_sessions_are_checked_too(self):
        proof = self.preflight("persist_tool_results")
        for name in ("canary", "control"):
            self.assertIn(f"{name}_disk_config_tool_results", proof["problems"])

    def test_the_loader_refuses_a_proof_without_the_disk_facts_or_with_image_data(self):
        proof = self.preflight()
        with tempfile.TemporaryDirectory() as tmp:
            for name, broken in {"no_disk": {k: v for k, v in proof.items() if k != "disk"},
                                 "image_data": proof | {"disk": proof["disk"] | {"image_data_found": True}},
                                 "persistence": proof | {"disk": proof["disk"] | {"no_session_persistence_flag": False}},
                                 "no_env": {k: v for k, v in proof.items() if k != "agent_env"}}.items():
                with self.subTest(broken=name):
                    path = Path(tmp) / f"{name}.json"
                    path.write_text(json.dumps(broken), encoding="utf-8")
                    with self.assertRaises(SystemExit):
                        pf.load_isolation_proof(path)

    def test_a_proof_for_another_agent_environment_does_not_cover_the_run(self):
        proof = self.preflight()
        args = argparse.Namespace(model="claude-haiku-5-5")
        self.assertNotIn("agent_env_differs_from_proof",
                         pf.proof_problems(proof, args, self.production, None))
        other = proof | {"agent_env": {}}
        self.assertIn("agent_env_differs_from_proof", pf.proof_problems(other, args, self.production, None))


class ProductionCommandInvariantTests(PreflightHarness):
    def test_the_production_command_keeps_no_session_persistence(self):
        for builder in (pa.build_claude_argv,):
            argv = builder(claude="claude", model="m", mcp_config=Path("mcp.json"), system_instruction="s",
                           max_budget_usd=0.5)
            self.assertIn("--no-session-persistence", argv)
        ag02_argv = rg.build_claude_argv(claude="claude", model="m", mcp_config=Path("mcp.json"),
                                         system_instruction="s", max_budget_usd=0.5)
        self.assertIn("--no-session-persistence", ag02_argv)
        self.assertEqual(pa.command_problems(argv), [])
        self.assertEqual(pa.command_problems([a for a in argv if a != "--no-session-persistence"]),
                         ["production_command_persists_session"])

    def test_a_preflight_whose_command_lost_the_flag_refuses_the_proof(self):
        proof = self.preflight(argv_filter=lambda argv: [a for a in argv if a != "--no-session-persistence"])
        self.assertFalse(proof["passed"])
        self.assertIn("production_command_persists_session", proof["problems"])

    def test_the_runner_launches_nothing_when_the_command_lost_the_flag(self):
        from test_campaign import Series
        series = Series(self)
        real = pa.build_claude_argv
        with mock.patch.object(pa, "build_claude_argv",
                               lambda **kw: [a for a in real(**kw) if a != "--no-session-persistence"]):
            series.run()
        self.assertEqual(series.calls, [], "a session was launched with a command that persists the session")


class AgentEnvironmentTests(PreflightHarness):
    def test_the_native_preflight_child_runs_with_the_feedback_survey_off(self):
        self.preflight()
        self.assertEqual(json.loads(self.env_file.read_text())["feedback_survey"], "1")

    def test_a_route_preflight_child_runs_with_it_off_and_the_route_check_accepts_it(self):
        proof = self.preflight(route="deepseek")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(json.loads(self.env_file.read_text())["feedback_survey"], "1")

    def test_the_pixel_agents_environment_carries_it_and_ag02s_does_not(self):
        import pixel_runner as pr
        args = argparse.Namespace(route=None, model="claude-haiku-5-5", claude="claude",
                                  claude_config_dir=Path("cfg"))
        self.assertEqual(pr.PixelCLI(args).env({"CLAUDE_CONFIG_DIR": "cfg"})[SURVEY], "1")
        self.assertNotIn(SURVEY, rg.agent_env())
        self.assertNotIn(SURVEY, rg.QUIET_ENV)  # AG-02's switches, and so its proofs, are unchanged

    def test_the_route_environment_of_the_pixel_agent_carries_it_without_a_route_extra_variable_problem(self):
        import pixel_runner as pr
        spec = agentcli.CLAUDE_ROUTES["deepseek"]
        key = Path(tempfile.mkdtemp()) / "key.txt"
        key.write_text("sk-test-0000000000000000", encoding="utf-8")
        self.addCleanup(key.unlink)
        args = argparse.Namespace(route="deepseek", route_base_url=spec["base_urls"][0], model=spec["model"],
                                  claude="claude", claude_config_dir=Path("cfg"), key_file=key)
        env = pr.PixelCLI(args).env({"CLAUDE_CONFIG_DIR": "cfg"})
        self.assertEqual(env[SURVEY], "1")

    def test_the_runner_launches_nothing_when_the_agent_environment_lacks_it(self):
        from test_campaign import Series
        series = Series(self)
        real = pa.PIXEL_ENV
        import pixel_runner as pr
        with mock.patch.object(pr.PixelCLI, "env", lambda self_, extra=None: rg.agent_env(extra)):
            series.run()
        self.assertEqual(series.calls, [], "a session was launched without the agent environment of the proof")
        self.assertEqual(real, pa.PIXEL_ENV)


class PixelDiskModuleTests(unittest.TestCase):
    """The module itself (new with the fix): scanning, snapshots and the classification of what a session touched."""

    def setUp(self):
        import pixel_disk
        self.disk = pixel_disk
        scratch = tempfile.TemporaryDirectory(prefix="ag03-diskmod-")
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)

    def png(self):
        return cap.synthetic_png(0, 64, 48)[0]

    def test_it_sees_raw_json_escaped_and_base64_png_in_any_alignment_and_nothing_else(self):
        import base64
        png = self.png()
        for name, data in {"raw": b"xx" + png, "escaped": b'{"d": "\\u0089PNG"}',
                           "b64": base64.b64encode(png), "b64_shift1": base64.b64encode(b"a" + png),
                           "b64_shift2": base64.b64encode(b"ab" + png)}.items():
            with self.subTest(form=name):
                self.assertTrue(self.disk.image_hits(data), name)
        for clean in (b"", b"hello", b"IHDR only", base64.b64encode(b"not an image" * 20)):
            self.assertEqual(self.disk.image_hits(clean), [])

    def test_untouched_files_are_ignored_and_touched_ones_classified(self):
        (self.root / "projects" / "p").mkdir(parents=True)
        (self.root / "projects" / "p" / "old.jsonl").write_text("{}", encoding="utf-8")
        (self.root / ".claude.json").write_text("{}", encoding="utf-8")
        before = self.disk.snapshot(self.root)
        self.assertEqual(self.disk.config_problems(self.root, before, self.disk.snapshot(self.root))[0], [])
        (self.root / ".claude.json").write_text('{"a": 1}', encoding="utf-8")  # touched, clean: fine
        (self.root / "backups").mkdir()
        (self.root / "backups" / "b.json").write_text("{}", encoding="utf-8")
        self.assertEqual(self.disk.config_problems(self.root, before, self.disk.snapshot(self.root))[0], [])
        (self.root / "projects" / "p" / "s" / "tool-results").mkdir(parents=True)
        (self.root / "projects" / "p" / "s" / "tool-results" / "x.png").write_bytes(self.png())
        (self.root / "projects" / "p" / "s.jsonl").write_text("{}", encoding="utf-8")
        problems, facts = self.disk.config_problems(self.root, before, self.disk.snapshot(self.root))
        self.assertEqual(problems, ["disk_config_image_data", "disk_config_projects_written",
                                    "disk_config_tool_results", "disk_config_transcript"])
        self.assertEqual(facts["touched"], 4)

    def test_an_unreadable_touched_file_refuses(self):
        before = self.disk.snapshot(self.root)
        (self.root / "f.bin").write_bytes(b"data")
        after = self.disk.snapshot(self.root)
        with mock.patch.object(Path, "read_bytes", side_effect=PermissionError("denied")):
            self.assertEqual(self.disk.config_problems(self.root, before, after)[0], ["disk_config_unreadable"])
            self.assertEqual(self.disk.cache_problems(self.root, before, after)[0], ["disk_cache_unreadable"])

    def test_the_cache_check_looks_at_content_only(self):
        before = self.disk.snapshot(self.root)
        log = self.root / "C--scratch" / "mcp-logs-pixel"
        log.mkdir(parents=True)
        (log / "a.jsonl").write_text('{"debug": "connected"}', encoding="utf-8")
        self.assertEqual(self.disk.cache_problems(self.root, before, self.disk.snapshot(self.root))[0], [])
        (log / "b.jsonl").write_bytes(self.png())
        self.assertEqual(self.disk.cache_problems(self.root, before, self.disk.snapshot(self.root))[0],
                         ["disk_cache_image_data"])

    def test_a_missing_root_is_an_empty_snapshot_and_the_cache_root_follows_the_platform(self):
        self.assertEqual(self.disk.snapshot(self.root / "absent"), {})
        env = {"LOCALAPPDATA": r"C:\Users\u\AppData\Local", "HOME": "/home/u"}
        self.assertEqual(self.disk.cli_cache_root("win32", env), Path(r"C:\Users\u\AppData\Local") / "claude-cli-nodejs" / "Cache")
        self.assertEqual(self.disk.cli_cache_root("linux", env), Path("/home/u") / ".cache" / "claude-cli-nodejs")
        self.assertEqual(self.disk.cli_cache_root("linux", env | {"XDG_CACHE_HOME": "/x"}), Path("/x") / "claude-cli-nodejs")
        self.assertEqual(self.disk.cli_cache_root("darwin", env), Path("/home/u") / "Library" / "Caches" / "claude-cli-nodejs")


if __name__ == "__main__":
    unittest.main()
