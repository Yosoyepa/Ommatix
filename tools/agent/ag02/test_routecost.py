"""Regression tests of the route cost cap (MiMo G08 of 2026-10-19, ``error_max_budget_usd``).

On the provider routes (``--route mimo|deepseek``) the Claude Code CLI prices a response with Anthropic's
prices for a model it does not know, so its dollar figure is invalid, and ``--max-budget-usd`` stopped a
MiMo session on that figure. A route run therefore passes the CLI no dollar cap and refuses one; the
per-goal limit is a token cap read from the stream (``--max-context-tokens-per-goal``) next to the time
cap. The native Claude path keeps its dollar cap unchanged.

No network, no real CLI: a fake CLI prints a scripted stream."""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import agentcli  # noqa: E402
import run_goal as rg  # noqa: E402
from test_prreview93 import DEEPSEEK_URL, MIMO_URL, THREE  # noqa: E402

TOKEN = "tp-dummy-secret-route-token-0002"

# A fake CLI that behaves like Claude Code does with a model it cannot price: it ends the session with
# ``error_max_budget_usd`` when, and only when, it was given a dollar cap. With ``--spend`` it also reports
# that many context tokens (one assistant message) and then waits, as a long session would.
FAKE_CLI = """import json, sys, time
init = open(sys.argv[1], encoding="utf-8").read()
sys.stdin.read()
print(init, flush=True)
spend = int(sys.argv[2])
if spend:
    print(json.dumps({"type": "assistant", "message": {"id": "m1", "role": "assistant", "content": [],
          "usage": {"input_tokens": 10, "cache_read_input_tokens": spend, "cache_creation_input_tokens": 0,
                    "output_tokens": 5}}}), flush=True)
    time.sleep(60)
capped = "--max-budget-usd" in sys.argv
print(json.dumps({"type": "result", "subtype": "error_max_budget_usd" if capped else "success",
                  "is_error": capped, "result": "" if capped else "VERDICT: DONE", "num_turns": 1}), flush=True)
"""


def init_event():
    return {"type": "system", "subtype": "init", "claude_code_version": "2.1.303", "model": "mimo-v2.6-flash",
            "tools": list(rg.ALLOWED_TOOL_NAMES), "mcp_servers": [{"name": "ommatix", "status": "connected"}],
            "skills": [], "slash_commands": [], "plugins": [], "agents": [], "memory_paths": {},
            "additional_directories": []}


class RouteArgvTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ag02-cost-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.key = self.root / "k.key"
        self.key.write_text(TOKEN + "\n", encoding="utf-8")
        self.cfg = self.root / "cfg"
        self.cfg.mkdir()
        err = contextlib.redirect_stderr(io.StringIO())
        err.__enter__()
        self.addCleanup(err.__exit__, None, None, None)

    def route_args(self, *extra, route="mimo", url=MIMO_URL):
        return rg.parse_args(["--plan", "--agent-cli", "claude", "--route", route, "--route-base-url", url,
                              "--key-file", str(self.key), "--claude-config-dir", str(self.cfg),
                              "--isolation-proof", str(self.root / "p.json"), *extra])

    def argv(self, args):
        return agentcli.make(args).build_argv(model="m", mcp_config=self.root / "m.json", system_instruction="i",
                                              max_budget_usd=args.max_budget_usd)

    def test_a_route_command_carries_no_dollar_cap_whatever_the_default(self):
        for route, url in (("mimo", MIMO_URL), ("deepseek", DEEPSEEK_URL)):
            argv = self.argv(self.route_args(route=route, url=url))
            self.assertNotIn("--max-budget-usd", argv, route)
            self.assertIn("--setting-sources", argv)  # the rest of the command is the confined one

    def test_the_native_command_keeps_its_dollar_cap(self):
        args = rg.parse_args(["--plan", "--max-budget-usd", "0.5"])
        argv = agentcli.make(args).build_argv(model="m", mcp_config=self.root / "m.json", system_instruction="i",
                                              max_budget_usd=args.max_budget_usd)
        self.assertEqual(argv[argv.index("--max-budget-usd") + 1], "0.5")
        default = rg.parse_args(["--plan"])
        self.assertEqual(rg.build_claude_argv(claude="claude", model="m", mcp_config=self.root / "m.json",
                                              system_instruction="i", max_budget_usd=default.max_budget_usd)[-2:],
                         ["--max-budget-usd", "1"])

    def test_a_route_run_given_a_dollar_cap_is_refused(self):
        refusal = agentcli.make(self.route_args("--max-budget-usd", "0.5")).refusal()
        self.assertIn("dollar cap", refusal)
        self.assertIn("--max-context-tokens-per-goal", refusal)
        self.assertIsNone(agentcli.make(self.route_args()).refusal())
        self.assertEqual(agentcli.make(self.route_args("--max-budget-usd", "0.5")).refusal() is None, False)

    def test_a_plan_of_a_route_run_given_a_dollar_cap_prints_the_refusal_not_a_command(self):
        flags = rg.agent_flags(self.route_args("--max-budget-usd", "0.5"), self.root / "out")
        self.assertEqual(len(flags), 1)
        self.assertTrue(flags[0].startswith("refused:"))

    def test_the_token_cap_is_for_routes_only_positive_and_defaults_from_the_observed_maxima(self):
        default = self.route_args()
        self.assertEqual(default.max_context_tokens_per_goal, agentcli.ROUTE_CONTEXT_TOKENS_DEFAULT)
        # 2 x the largest per-goal context of the four route series of 2026-10-19 (274 232, MiMo phase 1 G02)
        self.assertGreaterEqual(agentcli.ROUTE_CONTEXT_TOKENS_DEFAULT, 2 * 274_232)
        self.assertEqual(self.route_args("--max-context-tokens-per-goal", "123456").max_context_tokens_per_goal, 123456)
        for bad in ("0", "-5"):
            with self.assertRaises(SystemExit):
                self.route_args("--max-context-tokens-per-goal", bad)
        with self.assertRaises(SystemExit):
            rg.parse_args(["--plan", "--max-context-tokens-per-goal", "1000"])  # native: the dollar cap is its cap

    def test_the_cap_is_recorded_in_the_provenance_and_the_native_provenance_is_unchanged(self):
        cli = agentcli.make(self.route_args("--max-context-tokens-per-goal", "300000"))
        self.assertEqual(cli.provenance(), {"cli": "claude-code", "provider": "mimo",
                                            "route_host": "token-plan-sgp.xiaomimimo.com",
                                            "max_context_tokens_per_goal": 300000, "dollar_cap": "none"})
        self.assertEqual(agentcli.make(rg.parse_args(["--plan"])).provenance(),
                         {"cli": "claude-code", "provider": "anthropic"})

    def test_a_plan_shows_the_token_cap(self):
        lines = rg.plan_lines(self.route_args("--max-context-tokens-per-goal", "300000"), rg.load_goals(),
                              rg.select_goals(rg.load_goals(), ["G01"]), self.root / "out")
        self.assertIn("300000", json.dumps(lines))


class FakeCliTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ag02-cost-run-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "init.json").write_text(json.dumps(init_event()), encoding="utf-8")
        (self.root / "fake_cli.py").write_text(FAKE_CLI, encoding="utf-8")

    def fake(self, spend, *flags):
        return [sys.executable, str(self.root / "fake_cli.py"), str(self.root / "init.json"), str(spend), *flags]

    def run_fake(self, spend, *flags, cap=None, timeout=20):
        metrics = rg.StreamMetrics([], None, None)
        ran = rg.run_claude(self.fake(spend, *flags), "prompt", self.root, metrics, timeout,
                            **({} if cap is None else {"max_context_tokens": cap}))
        return ran, metrics

    def test_the_fake_cli_ends_on_the_dollar_cap_only_when_given_one(self):
        ran, metrics = self.run_fake(0, "--max-budget-usd", "0.5")
        self.assertEqual(metrics.summary()["cli_subtype"], "error_max_budget_usd")
        ran, metrics = self.run_fake(0)
        self.assertEqual((metrics.summary()["cli_subtype"], metrics.verdict()), ("success", "DONE"))

    def test_a_session_over_the_token_cap_is_killed_and_says_so(self):
        ran, metrics = self.run_fake(500_000, cap=100_000)
        self.assertEqual(ran["killed"], "token_cap")
        self.assertNotEqual(ran["exit_code"], 0)

    def test_a_session_under_the_token_cap_is_not_killed(self):
        ran, metrics = self.run_fake(0, cap=100_000)
        self.assertIsNone(ran["killed"])
        ran, metrics = self.run_fake(0)  # no cap: native runs, minimal agent, tests
        self.assertIsNone(ran["killed"])

    def route_smoke(self, *extra, spend=0):
        key = self.root / "mimo.key"
        key.write_text(TOKEN + "\n", encoding="utf-8")
        cfg, out = self.root / "cfg", self.root / "out"
        cfg.mkdir(exist_ok=True)
        att = rg.init_attestation(init_event())
        proof = {"schema": rg.PROOF_SCHEMA, "verifier_revision": rg.VERIFIER_REVISION, "passed": True,
                 "claude_code_version": "2.1.303", "model": "mimo-v2.6-flash", "requested_model": "mimo-v2.6-flash",
                 "tools": THREE, "plugins": [], "agents": [], "plugin_hashes": [], "agent_hashes": [],
                 "init_keys_sha256": att["init_keys_sha256"], "config_identity": rg.config_identity(cfg),
                 "context_manifest": rg.config_manifest(cfg), "canary_kinds": ["user_claude_md"],
                 "control_leaked_kinds": ["user_claude_md"], "date_utc": "2026-10-19T00:00:00Z",
                 "route": {"provider": "mimo", "base_url": MIMO_URL, "model": "mimo-v2.6-flash", "effort": "max",
                           "auth": "bearer"},
                 "route_verifier_revision": agentcli.ROUTE_VERIFIER_REVISION, "request_tools": THREE}
        (self.root / "proof.json").write_text(json.dumps(proof), encoding="utf-8")

        build = rg.build_claude_argv

        def real_argv(*, claude, model, mcp_config, system_instruction, max_budget_usd):
            # the fake stands where Claude Code stands: the dollar flag is appended exactly when the runner's
            # own argv builder appends it
            real = build(claude=claude, model=model, mcp_config=mcp_config,
                         system_instruction=system_instruction, max_budget_usd=max_budget_usd)
            return self.fake(spend, *(["--max-budget-usd", "x"] if "--max-budget-usd" in real else []))
        stdout = io.StringIO()
        with mock.patch.object(rg, "build_claude_argv", side_effect=real_argv), contextlib.redirect_stdout(stdout), \
                contextlib.redirect_stderr(io.StringIO()):
            code = rg.main(["--smoke", "--agent-cli", "claude", "--route", "mimo", "--route-base-url", MIMO_URL,
                            "--key-file", str(key), "--claude-config-dir", str(cfg),
                            "--isolation-proof", str(self.root / "proof.json"), "--model", "mimo-v2.6-flash",
                            "--exe", str(self.root / "ommatix.exe"), "--out", str(out), *extra])
        smoke = json.loads(next(out.rglob("smoke.json")).read_text(encoding="utf-8"))
        return code, smoke

    def test_a_route_session_is_not_stopped_by_a_dollar_cap_the_cli_never_got(self):
        code, smoke = self.route_smoke()
        self.assertEqual(code, 0)
        self.assertEqual((smoke["metrics"]["cli_subtype"], smoke["metrics"]["verdict"], smoke["cli"]["killed"]),
                         ("success", "DONE", None))
        self.assertEqual(smoke["provenance"]["max_context_tokens_per_goal"], agentcli.ROUTE_CONTEXT_TOKENS_DEFAULT)
        self.assertEqual(smoke["provenance"]["dollar_cap"], "none")

    def test_a_route_session_over_its_token_cap_is_killed_by_the_runner(self):
        code, smoke = self.route_smoke("--max-context-tokens-per-goal", "100000", spend=500_000)
        self.assertEqual(smoke["cli"]["killed"], "token_cap")
        self.assertEqual(smoke["provenance"]["max_context_tokens_per_goal"], 100000)


if __name__ == "__main__":
    unittest.main()
