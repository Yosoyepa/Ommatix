"""The gate smoke runner (``gate_smoke.py``, ADR-0038 section B step 3b): fakes only.

A scripted ``claude`` (``fake_claude_smoke.py``) stands in for the CLI: no provider is reached, no key is real, nothing is
captured, no window or input is touched. What is tested is the runner's own rules: the owner's preconditions before
anything is read or sent, the proof for the same model, route and environment, one attempt per model (claimed before the
CLI can reach a provider, kept after), a record of booleans and closed tokens only (never the answer), the key only in the
child's environment, and a failing model excluded and never run again."""

import argparse
import contextlib
import io
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import gate_smoke as gs  # noqa: E402
import pixel_gate as gate  # noqa: E402
import run_goal as rg  # noqa: E402
from pin_support import launch_refused  # noqa: E402
from test_disk_evidence import PreflightHarness  # noqa: E402

FAKE_SMOKE_CLI = [sys.executable, str(HERE / "fake_claude_smoke.py")]
KEY = "sk-test-0000000000000000"
SPEC = agentcli.CLAUDE_ROUTES["deepseek"]


def utc(offset_s: float = 0.0) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + offset_s))


class Runner:
    """``run_claude`` for ``gate_smoke.main``: the real one (it owns and ends the process tree) with the fake CLI's switches
    added to the child's environment, remembering every call."""

    def __init__(self, mode="good", shots=4, record=None, size="1280x720", cost="0.01"):
        self.extra = {"FAKE_SMOKE_MODE": mode, "FAKE_SMOKE_SHOTS": str(shots), "FAKE_SMOKE_SECRET": KEY,
                      "FAKE_SMOKE_SIZE": size, "FAKE_SMOKE_COST": cost}
        if record:
            self.extra["FAKE_SMOKE_RECORD"] = str(record)
        self.calls = []

    def __call__(self, argv, prompt, cwd, metrics, timeout, env=None, **kwargs):
        self.calls.append({"argv": list(argv), "prompt": prompt, "kwargs": dict(kwargs), "timeout": timeout})
        return rg.run_claude(argv, prompt, cwd, metrics, timeout, env=(env or {}) | self.extra, **kwargs)


class NeverRuns:
    calls = ()

    def __call__(self, *args, **kwargs):
        raise AssertionError("the CLI was launched")


class GateSmokeCase(PreflightHarness):
    """Builds a real (fake-CLI) isolation proof for the native or a route model, then drives ``gate_smoke.main``."""

    def setUp(self):
        self._dir = tempfile.TemporaryDirectory(prefix="ag03-gate-")
        self.addCleanup(self._dir.cleanup)
        self.tmp = Path(self._dir.name)
        self.out_dir = self.tmp / "gate"
        self.key_file = self.tmp / "key.txt"
        self.key_file.write_text(KEY, encoding="utf-8")
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()
        environ = mock.patch.dict(os.environ)  # the operator's routing variables are not the test's business
        environ.start()
        self.addCleanup(environ.stop)
        for name in rg.routing_problems(os.environ):
            os.environ.pop(name, None)
        # the canonical root of the one-attempt rule lives under LOCALAPPDATA: the test's own, never the owner's
        os.environ["LOCALAPPDATA"] = str(self.tmp / "local")
        self.canonical = self.tmp / "local" / "Ommatix" / "ag03" / "gate"

    def prepare(self, route=None, *, proof_edit=None, preconditions="ok", proof_age_s=0.0):
        proof = self.preflight(route=route)
        self.assertTrue(proof["passed"], proof["problems"])
        if proof_edit:
            proof_edit(proof)
        self.proof_path = self.tmp / "proof.json"
        self.proof_path.write_text(json.dumps(proof), encoding="utf-8")
        self.route = route
        spec = agentcli.CLAUDE_ROUTES.get(route)
        self.model = spec["model"] if route else "claude-haiku-5-5"
        self.url = spec["base_urls"][0] if route else None
        self.provider = route or "anthropic"
        self.pre_path = self.tmp / "pre" / "pre.json"
        if preconditions:
            self.write_preconditions(utc())
        return proof

    def write_preconditions(self, stamp, **override):
        fields = dict(gs.preconditions_fields(self.provider, self.model, self.url)) | override
        fields = {k: v for k, v in fields.items() if v is not ...}
        self.pre_path.parent.mkdir(parents=True, exist_ok=True)
        self.pre_path.write_text(json.dumps(fields | {"utc": stamp}), encoding="utf-8-sig")

    def argv(self, *extra, claude=None):
        args = ["--claude", str(Path(sys.executable)) if claude is None else claude,
                "--claude-config-dir", str(self.production), "--isolation-proof", str(self.proof_path),
                "--preconditions", str(self.pre_path), "--out-dir", str(self.out_dir), "--timeout", "60"]
        if self.route:
            args += ["--route", self.route, "--route-base-url", self.url, "--key-file", str(self.key_file)]
        return args + list(extra)

    def run_main(self, runner, *extra):
        # the pinned-CLI guard of the proof preflight is still patched (PreflightHarness); the fake CLI is the "claude"
        # that ``clipin.verified`` hands back, so the head of the command is the scripted stand-in
        with mock.patch.object(gs.pr.PixelCLI, "build_argv", self._fake_head(gs.pr.PixelCLI.build_argv)), \
                contextlib.redirect_stdout(self.stdout), contextlib.redirect_stderr(self.stderr), \
                mock.patch.dict(os.environ, {k: v for k, v in os.environ.items()}, clear=True):
            return gs.main(self.argv(*extra), run_claude=runner)

    @staticmethod
    def _fake_head(real):
        def build(self_, **kw):
            kw["claude"] = FAKE_SMOKE_CLI
            return real(self_, **kw)
        return build

    def record(self, name):
        return json.loads((self.out_dir / f"{name}.gate.json").read_text(encoding="utf-8"))

    def summary(self):
        return json.loads(self.stdout.getvalue().strip().splitlines()[-1])


class PassAndExcludeTests(GateSmokeCase):
    def test_a_native_session_that_names_every_word_and_shape_passes(self):
        self.prepare()
        runner = Runner()
        self.assertEqual(self.run_main(runner), 0)
        record = self.record("Haiku")
        self.assertEqual((record["status"], record["per_image_ok"], record["provider_called"]),
                         ("passed", [True] * 4, True))
        self.assertEqual((record["model"], record["provider"], record["requested_model"], record["route_base_url"]),
                         ("Haiku", "anthropic", "claude-haiku-5-5", None))
        self.assertEqual(record["resolved_model"], "claude-haiku-5-5")
        self.assertTrue(record["preflight_passed"])
        self.assertEqual(len(runner.calls), 1)  # one real session, no retry inside

    def test_the_record_binds_the_gate_the_runner_consumes(self):
        import pixel_runner as pr
        self.prepare()
        self.run_main(Runner())
        record = pr.read_gate(self.out_dir / "Haiku.gate.json", "Haiku", None, "claude-haiku-5-5", "claude-haiku-5-5")
        self.assertEqual(record["status"], "passed")

    def test_a_route_session_passes_with_its_endpoint_bound(self):
        self.prepare("deepseek")
        self.assertEqual(self.run_main(Runner()), 0)
        record = self.record("DeepSeek")
        self.assertEqual((record["status"], record["provider"], record["route_base_url"]),
                         ("passed", "deepseek", SPEC["base_urls"][0]))
        import pixel_runner as pr
        self.assertEqual(pr.read_gate(self.out_dir / "DeepSeek.gate.json", "DeepSeek", "deepseek", SPEC["model"],
                                      record["resolved_model"], SPEC["base_urls"][0])["status"], "passed")

    def test_wrong_words_exclude_the_model_with_a_boolean_per_image(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner("wrong")), 1)
        record = self.record("Haiku")
        self.assertEqual((record["status"], record["reason"], record["per_image_ok"]),
                         ("excluded", "smoke_failed", [False] * 4))

    def test_an_answer_without_the_lines_is_a_failed_smoke(self):
        self.prepare()
        self.run_main(Runner("garbled"))
        self.assertEqual(self.record("Haiku")["reason"], "smoke_failed")

    def test_right_words_but_fewer_screenshots_than_asked_are_not_a_pass(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner(shots=2)), 1)
        record = self.record("Haiku")
        self.assertEqual((record["status"], record["reason"]), ("excluded", "cli_error"))
        self.assertIn("screenshots_called_fewer_than_asked", record["evidence"]["problems"])

    def test_images_that_are_not_the_size_the_server_drew_are_not_a_pass(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner(size="64x48")), 1)
        record = self.record("Haiku")
        self.assertEqual(record["reason"], "cli_error")
        self.assertIn("screenshot_size_differs_from_the_server_s", record["evidence"]["problems"])

    def test_a_widened_tool_surface_ends_the_session_and_excludes(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner("extra_tool")), 1)
        self.assertEqual(self.record("Haiku")["reason"], "cli_error")

    def test_a_provider_that_refuses_the_image_is_excluded_with_that_reason(self):
        self.prepare()
        self.run_main(Runner("error_image"))
        self.assertEqual((self.record("Haiku")["status"], self.record("Haiku")["reason"]),
                         ("excluded", "provider_rejected_image"))

    def test_a_transport_failure_is_excluded_as_a_transport_error(self):
        self.prepare()
        self.run_main(Runner("error_transport"))
        self.assertEqual(self.record("Haiku")["reason"], "transport_error")

    def test_any_other_error_is_a_cli_error(self):
        self.prepare()
        self.run_main(Runner("error_other"))
        self.assertEqual(self.record("Haiku")["reason"], "cli_error")

    def test_a_session_that_outlasts_its_time_is_excluded_as_a_timeout(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner("hang"), "--timeout", "4"), 1)
        self.assertEqual(self.record("Haiku")["reason"], "timeout")


class OneAttemptTests(GateSmokeCase):
    def test_a_failed_model_is_never_run_again_with_other_settings(self):
        self.prepare()
        self.run_main(Runner("wrong"))
        runner = NeverRuns()
        with self.assertRaises(SystemExit) as raised:
            self.run_main(runner, "--timeout", "120")
        self.assertIn("already", str(raised.exception))
        self.assertEqual(self.record("Haiku")["reason"], "smoke_failed")

    def test_a_passed_model_is_not_run_again_either(self):
        self.prepare()
        self.run_main(Runner())
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())

    def test_the_attempt_is_claimed_before_the_cli_runs_and_a_claim_without_a_record_is_consumed(self):
        self.prepare()
        seen = {}

        def dying(argv, prompt, cwd, metrics, timeout, env=None, **kwargs):
            seen["claim"] = (self.out_dir / "Haiku.claim.json").exists()
            raise ValueError("the process died")

        with self.assertRaises(ValueError):
            self.run_main(dying)
        self.assertTrue(seen["claim"])
        self.assertFalse((self.out_dir / "Haiku.gate.json").exists())
        with self.assertRaises(SystemExit) as raised:
            self.run_main(NeverRuns())
        self.assertIn("already", str(raised.exception))

    def test_a_launch_that_is_refused_consumes_nothing(self):
        self.prepare()
        with launch_refused("claude_hash_differs"), self.assertRaises(SystemExit) as raised:
            self.run_main(NeverRuns())
        self.assertIn("nothing was sent", str(raised.exception))
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())
        self.assertEqual(self.run_main(Runner()), 0)  # the attempt was still there to make

    def test_an_expired_or_foreign_proof_is_refused_before_a_claim(self):
        self.prepare()
        with launch_refused("proof_expired"), self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())


class PreconditionTests(GateSmokeCase):
    def never_read_the_key(self):
        return mock.patch.object(agentcli.ClaudeCLI, "read_key", side_effect=AssertionError("the key was read"))

    def test_without_the_owners_file_nothing_is_read_sent_or_written(self):
        self.prepare("deepseek", preconditions=None)
        with self.never_read_the_key():
            self.assertEqual(self.run_main(NeverRuns()), 3)
        out = self.summary()
        self.assertEqual((out["status"], out["provider_called"]), ("not_run", False))
        self.assertIn("owner_go_ahead_not_recorded", out["missing"])
        self.assertFalse((self.out_dir / "DeepSeek.claim.json").exists())
        self.assertFalse((self.out_dir / "DeepSeek.gate.json").exists())

    def test_the_file_of_another_model_or_endpoint_counts_for_nothing(self):
        self.prepare("deepseek")
        self.write_preconditions(utc(), model="deepseek-pro")
        with self.never_read_the_key():
            self.assertEqual(self.run_main(NeverRuns()), 3)
        self.assertIn("preconditions_model_differs", self.stdout.getvalue())

    def test_the_file_of_another_provider_counts_for_nothing(self):
        self.prepare("deepseek")
        self.write_preconditions(utc(), provider="mimo")
        self.assertEqual(self.run_main(NeverRuns()), 3)

    def test_a_stale_go_ahead_counts_for_nothing(self):
        self.prepare("deepseek")
        self.write_preconditions(utc(-25 * 3600))
        self.assertEqual(self.run_main(NeverRuns()), 3)
        self.assertIn("preconditions_expired", self.stdout.getvalue())

    def test_a_go_ahead_from_the_future_counts_for_nothing(self):
        self.prepare("deepseek")
        self.write_preconditions(utc(3 * 3600))
        self.assertEqual(self.run_main(NeverRuns()), 3)

    def test_a_field_that_is_not_true_is_never_defaulted_to_true(self):
        self.prepare("deepseek")
        self.write_preconditions(utc(), privacy_reading_recorded="yes")
        self.assertEqual(self.run_main(NeverRuns()), 3)
        self.assertIn("privacy_reading_not_recorded", self.stdout.getvalue())

    def test_anthropic_also_needs_oi1(self):
        self.prepare()
        self.write_preconditions(utc(), oi1_recorded=False)
        self.assertEqual(self.run_main(NeverRuns()), 3)
        self.assertIn("oi1_not_recorded", self.stdout.getvalue())

    def test_a_bom_in_the_file_the_owners_shell_wrote_is_read(self):
        self.prepare("deepseek")
        self.assertEqual(self.run_main(Runner()), 0)

    def test_the_printed_command_records_what_the_file_reader_accepts(self):
        self.prepare("mimo", preconditions=None)
        with contextlib.redirect_stdout(self.stdout):
            code = gs.main(["--route", "mimo", "--route-base-url", agentcli.CLAUDE_ROUTES["mimo"]["base_urls"][0],
                            "--preconditions", str(self.pre_path), "--print-preconditions-command"])
        text = self.stdout.getvalue()
        self.assertEqual(code, 0)
        for fragment in ("ag03-gate-preconditions-1", "'mimo'", "mimo-v2.6-flash", "token-plan-sgp", "$true",
                         "ConvertTo-Json", "utc =", str(self.pre_path)):
            self.assertIn(fragment, text)
        self.assertNotIn("oi1_recorded", text)
        self.assertFalse(self.pre_path.exists())  # this tool never writes the owner's file
        # the same fields, stamped, are what the reader accepts
        self.write_preconditions(utc())
        got, problems = gs.load_preconditions(self.pre_path, "mimo", "mimo-v2.6-flash",
                                              agentcli.CLAUDE_ROUTES["mimo"]["base_urls"][0])
        self.assertEqual(problems, [])
        self.assertEqual(gate.preconditions_missing("mimo", got), [])

    def test_the_native_command_carries_oi1_and_a_null_endpoint(self):
        with contextlib.redirect_stdout(self.stdout):
            gs.main(["--preconditions", str(self.tmp / "x" / "p.json"), "--print-preconditions-command"])
        self.assertIn("oi1_recorded = $true", self.stdout.getvalue())
        self.assertIn("route_base_url = $null", self.stdout.getvalue())

    def test_a_model_outside_the_owners_accepted_set_is_refused_by_the_command_line(self):
        with self.assertRaises(SystemExit), contextlib.redirect_stderr(self.stderr):
            gs.parse_args(["--model", "claude-opus-5-5", "--preconditions", str(self.tmp / "p.json"),
                           "--print-preconditions-command"])


class CommandLineTests(GateSmokeCase):
    def test_a_refusal_is_exit_2_with_its_message_and_sends_nothing(self):
        self.prepare()
        self.proof_path.write_text("not json", encoding="utf-8")
        with contextlib.redirect_stderr(self.stderr), contextlib.redirect_stdout(self.stdout):
            code = gs.entry(self.argv())
        self.assertEqual(code, 2)
        self.assertIn("isolation proof", self.stderr.getvalue())
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())


class CapTests(GateSmokeCase):
    def parse_refused(self, *extra):
        self.prepare()
        with contextlib.redirect_stderr(self.stderr), self.assertRaises(SystemExit) as raised:
            gs.parse_args(self.argv(*extra))
        return raised.exception.code

    def test_the_timeout_can_be_lowered_but_not_raised(self):
        self.prepare()
        self.assertEqual(gs.parse_args(self.argv("--timeout", "30")).timeout, 30.0)
        self.assertEqual(gs.parse_args(self.argv("--timeout", str(gs.TIMEOUT_S))).timeout, gs.TIMEOUT_S)
        self.assertEqual(self.parse_refused("--timeout", str(gs.TIMEOUT_S + 1)), 2)
        self.assertEqual(self.parse_refused("--timeout", "99999"), 2)

    def test_the_context_cap_can_be_lowered_but_not_raised(self):
        self.prepare("deepseek")
        self.assertEqual(gs.parse_args(self.argv("--max-context-tokens", "1000")).max_context_tokens_per_goal, 1000)
        with contextlib.redirect_stderr(self.stderr), self.assertRaises(SystemExit) as raised:
            gs.parse_args(self.argv("--max-context-tokens", str(gs.ROUTE_CONTEXT_TOKENS + 1)))
        self.assertEqual(raised.exception.code, 2)

    def test_a_raised_cap_runs_nothing_through_the_command_line(self):
        self.prepare()
        with contextlib.redirect_stderr(self.stderr), contextlib.redirect_stdout(self.stdout), \
                self.assertRaises(SystemExit) as raised:
            gs.entry(self.argv("--timeout", "99999"))
        self.assertEqual(raised.exception.code, 2)
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())


class CanonicalAttemptTests(GateSmokeCase):
    def test_the_attempt_is_also_claimed_in_the_canonical_root(self):
        self.prepare()
        self.run_main(Runner())
        self.assertTrue((self.canonical / "Haiku.claim.json").exists())

    def test_another_out_dir_does_not_give_another_attempt(self):
        self.prepare()
        self.run_main(Runner("wrong"))
        self.out_dir = self.tmp / "gate2"
        with self.assertRaises(SystemExit) as raised:
            self.run_main(NeverRuns())
        self.assertIn("already", str(raised.exception))
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())

    def test_a_claim_without_a_record_in_the_canonical_root_is_a_consumed_attempt_for_any_out_dir(self):
        self.prepare()
        self.canonical.mkdir(parents=True)
        (self.canonical / "Haiku.claim.json").write_text("{}", encoding="utf-8")
        with self.assertRaises(SystemExit) as raised:
            self.run_main(NeverRuns())
        self.assertIn("already", str(raised.exception))

    def test_a_record_in_the_canonical_root_is_also_refused(self):
        self.prepare()
        self.canonical.mkdir(parents=True)
        (self.canonical / "Haiku.gate.json").write_text("{}", encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())

    def test_the_canonical_root_as_out_dir_works(self):
        self.prepare()
        self.out_dir = self.canonical
        self.assertEqual(self.run_main(Runner()), 0)
        self.assertTrue((self.canonical / "Haiku.gate.json").exists())

    def test_a_refused_launch_consumes_nothing_in_either_place(self):
        self.prepare()
        with launch_refused("claude_hash_differs"), self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.canonical / "Haiku.claim.json").exists())
        self.assertEqual(self.run_main(Runner()), 0)


class CostTests(GateSmokeCase):
    def test_a_native_session_over_the_dollar_cap_is_noted_not_decisive(self):
        self.prepare()
        self.assertEqual(self.run_main(Runner(cost="0.50")), 0)
        record = self.record("Haiku")
        self.assertEqual(record["status"], "passed")
        self.assertIn("cost_over_cap", record["evidence"]["problems"])
        self.assertIn("cost_over_cap", self.stderr.getvalue())

    def test_a_native_session_within_the_cap_has_no_note(self):
        self.prepare()
        self.run_main(Runner(cost="0.25"))
        self.assertNotIn("cost_over_cap", self.record("Haiku")["evidence"]["problems"])
        self.assertNotIn("cost_over_cap", self.stderr.getvalue())

    def test_a_route_has_no_dollar_figure_to_compare(self):
        self.prepare("deepseek")
        self.run_main(Runner(cost="9.0"))
        self.assertNotIn("cost_over_cap", self.record("DeepSeek")["evidence"]["problems"])


class ProofTests(GateSmokeCase):
    def test_a_proof_that_did_not_pass_excludes_the_model_and_calls_no_provider(self):
        self.prepare(proof_edit=lambda p: p.update(passed=False, problems=["pixel_control_blind"]))
        self.assertEqual(self.run_main(NeverRuns()), 1)
        record = self.record("Haiku")
        self.assertEqual((record["status"], record["reason"], record["provider_called"]),
                         ("excluded", "preflight_failed", False))
        self.assertEqual(record["evidence"]["problems"], ["pixel_control_blind"])

    def assert_failed_proof_records_nothing(self, **edit):
        self.prepare(proof_edit=lambda p: p.update(passed=False, problems=["pixel_control_blind"], **edit))
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.out_dir / "Haiku.gate.json").exists())
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())

    def test_an_old_failed_proof_excludes_nobody(self):
        self.assert_failed_proof_records_nothing(date_utc="2026-01-01T00:00:00Z")

    def test_a_failed_proof_of_another_verifier_revision_excludes_nobody(self):
        self.assert_failed_proof_records_nothing(verifier_revision=1)

    def test_a_failed_proof_of_another_cli_excludes_nobody(self):
        self.assert_failed_proof_records_nothing(cli_sha256="0" * 64)

    def test_a_failed_proof_without_a_cli_hash_excludes_nobody(self):
        self.assert_failed_proof_records_nothing(cli_sha256=None)

    def test_a_failed_route_proof_of_another_route_excludes_nobody(self):
        self.prepare("deepseek", proof_edit=lambda p: p.update(
            passed=False, problems=["x"], route=dict(p["route"], base_url="https://elsewhere.invalid")))
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.out_dir / "DeepSeek.gate.json").exists())

    def test_a_failed_proof_of_another_model_excludes_nobody(self):
        self.prepare(proof_edit=lambda p: p.update(passed=False, problems=["x"], requested_model="claude-sonnet-5-5"))
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.out_dir / "Haiku.gate.json").exists())

    def test_a_proof_for_another_model_is_refused_with_nothing_sent(self):
        self.prepare(proof_edit=lambda p: p.update(requested_model="claude-sonnet-5-5"))
        with self.assertRaises(SystemExit) as raised:
            self.run_main(NeverRuns())
        self.assertIn("requested_model_differs_from_proof", str(raised.exception))
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())

    def test_a_native_proof_is_not_a_route_proof(self):
        self.prepare()
        route_args = ["--route", "deepseek", "--route-base-url", SPEC["base_urls"][0], "--key-file", str(self.key_file)]
        self.route, self.model, self.url, self.provider = "deepseek", SPEC["model"], SPEC["base_urls"][0], "deepseek"
        self.write_preconditions(utc())
        self.route = None  # argv() adds the route flags itself otherwise
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns(), *route_args)

    def test_a_proof_that_is_not_an_ag03_proof_is_refused(self):
        self.prepare()
        self.proof_path.write_text(json.dumps({"schema": "ag02-isolation-proof-3", "passed": True}), encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_main(NeverRuns())
        self.assertFalse((self.out_dir / "Haiku.claim.json").exists())

    def test_provider_routing_variables_refuse_a_native_session(self):
        self.prepare()
        with mock.patch.object(rg, "routing_problems", return_value=["ANTHROPIC_BASE_URL"]), \
                self.assertRaises(SystemExit):
            self.run_main(NeverRuns())


class ConfinementTests(GateSmokeCase):
    def test_the_command_is_the_production_one_with_the_smoke_caps_native(self):
        self.prepare()
        runner = Runner(record=self.tmp / "child.json")
        self.run_main(runner)
        call = runner.calls[0]
        argv = call["argv"]
        for flag in ("--no-session-persistence", "--strict-mcp-config", "--tools"):
            self.assertIn(flag, argv)
        self.assertEqual(argv[argv.index("--max-budget-usd") + 1], f"{gs.NATIVE_USD_CAP:g}")
        self.assertEqual(argv[argv.index("--effort") + 1], "max")
        self.assertEqual(call["prompt"], gate.SMOKE_PROMPT)
        self.assertEqual(call["kwargs"].get("max_context_tokens"), None)
        self.assertEqual(call["timeout"], 60.0)

    def test_the_server_is_synthetic_and_has_no_screen_input_or_stop_file_arguments(self):
        self.prepare()
        self.run_main(Runner())
        config = json.loads((self.out_dir / "Haiku" / "mcp.json").read_text(encoding="utf-8"))
        args = config["mcpServers"]["pixel"]["args"]
        self.assertIn("--synthetic", args)
        for forbidden in ("--admission", "--stop-file", "--attest-pid", "--attest-exe", "--ledger", "--max-live"):
            self.assertNotIn(forbidden, args)
        self.assertEqual(args[args.index("--synthetic-size") + 1], "1280x720")

    def test_a_route_has_the_token_cap_and_no_dollar_cap(self):
        self.prepare("deepseek")
        runner = Runner()
        self.run_main(runner)
        call = runner.calls[0]
        self.assertNotIn("--max-budget-usd", call["argv"])
        self.assertEqual(call["kwargs"]["max_context_tokens"], gs.ROUTE_CONTEXT_TOKENS)

    def test_the_key_is_only_in_the_childs_environment_never_in_an_argument_or_a_record(self):
        self.prepare("deepseek")
        child = self.tmp / "child.json"
        runner = Runner(record=child)
        self.run_main(runner)
        seen = json.loads(child.read_text(encoding="utf-8"))
        self.assertTrue(seen["key_in_env"])
        self.assertFalse(seen["key_in_argv"])
        self.assertFalse(any(KEY in json.dumps(c["argv"]) for c in runner.calls))
        everything = (self.stdout.getvalue() + self.stderr.getvalue()
                      + "".join(p.read_text(encoding="utf-8") for p in self.out_dir.rglob("*") if p.is_file()
                                and p.suffix in (".json", ".jsonl", ".lock")))
        self.assertNotIn(KEY, everything)

    def test_the_child_has_its_own_temp_directory_inside_the_work_directory(self):
        self.prepare()
        child = self.tmp / "child.json"
        self.run_main(Runner(record=child))
        temps = set(json.loads(child.read_text(encoding="utf-8"))["temp"].values())
        self.assertEqual(len(temps), 1)
        self.assertEqual(Path(temps.pop()), self.out_dir / "Haiku" / "cli-tmp")

    def test_a_widened_tool_list_in_the_stream_is_recorded_not_hidden(self):
        self.prepare()
        self.run_main(Runner("extra_tool"))
        session = self.record("Haiku")["evidence"]["session"]
        self.assertTrue(session["killed"] in ("init", "tool_surface"))


class RecordPrivacyTests(GateSmokeCase):
    def test_the_record_holds_no_answer_no_word_no_image_and_states_the_limitation(self):
        self.prepare()
        self.run_main(Runner())
        text = (self.out_dir / "Haiku.gate.json").read_text(encoding="utf-8")
        for word, _colour, _shape in __import__("pixel_capture").SYNTHETIC_SCENES:
            self.assertNotIn(word, text)
        for fragment in ("IMAGE 1", "VERDICT", "iVBOR", "base64", "WORD="):
            self.assertNotIn(fragment, text)
        record = json.loads(text)
        self.assertEqual(record["evidence"]["basis"], "stream_attested_with_proof")
        self.assertIn("stream-attested", record["evidence"]["limitation"])
        self.assertEqual(record["evidence"]["proof"]["cli_sha256"], self.proof_sha())
        self.assertEqual(record["evidence"]["session"]["images_asked"], 4)
        self.assertEqual(record["evidence"]["session"]["screenshots_seen_by_stream"], 4)
        self.assertEqual(record["evidence"]["session"]["screenshot_dims"], [[1280, 720]])

    def proof_sha(self):
        return json.loads(self.proof_path.read_text(encoding="utf-8"))["cli_sha256"]

    def test_a_refusal_text_is_classified_in_memory_and_never_stored(self):
        self.prepare()
        self.run_main(Runner("error_image"))
        text = (self.out_dir / "Haiku.gate.json").read_text(encoding="utf-8")
        self.assertNotIn("API Error", text)
        self.assertNotIn("not supported", text)

    def test_a_route_record_has_no_dollar_figure(self):
        self.prepare("deepseek")
        self.run_main(Runner())
        self.assertIsNone(self.record("DeepSeek")["evidence"]["session"]["cost_usd"])

    def test_only_the_closed_gate_keys_and_the_evidence_block_are_written(self):
        self.prepare()
        self.run_main(Runner())
        self.assertEqual(set(self.record("Haiku")) - {"evidence"},
                         {"schema", "model", "provider", "date_utc", "images", "preflight_passed", "requested_model",
                          "resolved_model", "route_base_url", "scope", "status", "per_image_ok", "provider_called"})


class ClassifierTests(unittest.TestCase):
    def metrics(self, **result):
        return argparse.Namespace(killed=None, init_problems=[], attestation={"model": "m"}, result=result or None)

    def test_the_classes(self):
        m = self.metrics(is_error=True, subtype="error_during_execution", result="400: image blocks are not supported")
        self.assertEqual(gs.provider_error_of({"exit_code": 1, "killed": None}, m), "provider_rejected_image")
        m = self.metrics(is_error=True, subtype="error_during_execution", result="fetch failed")
        self.assertEqual(gs.provider_error_of({"exit_code": 1, "killed": None}, m), "transport_error")
        m = self.metrics(is_error=True, subtype="error_during_execution", result="boom")
        self.assertEqual(gs.provider_error_of({"exit_code": 1, "killed": None}, m), "cli_error")
        self.assertEqual(gs.provider_error_of({"exit_code": 0, "killed": "timeout"}, self.metrics()), "timeout")
        self.assertEqual(gs.provider_error_of({"exit_code": 0, "killed": "token_cap"}, self.metrics()), "cli_error")
        self.assertIsNone(gs.provider_error_of({"exit_code": 0, "killed": None},
                                               self.metrics(is_error=False, subtype="success", result="ok")))

    def error(self, text):
        m = self.metrics(is_error=True, subtype="error_during_execution", result=text)
        return gs.provider_error_of({"exit_code": 1, "killed": None}, m)

    def test_a_stray_image_word_in_a_transport_error_is_a_transport_error(self):
        self.assertEqual(self.error("fetch failed while loading the image cache: ECONNRESET"), "transport_error")
        self.assertEqual(self.error("503 service unavailable (image service)"), "transport_error")
        self.assertEqual(self.error("API Error: connect ETIMEDOUT, base64 payload not sent"), "transport_error")

    def test_a_stray_image_word_without_a_rejection_is_a_cli_error(self):
        self.assertEqual(self.error("something failed near the image"), "cli_error")
        self.assertEqual(self.error("vision module crashed"), "cli_error")

    def test_a_rate_limit_that_names_an_image_is_not_a_rejection(self):
        self.assertEqual(self.error("429 too many requests for image input"), "transport_error")

    def test_an_explicit_rejection_of_the_image_is_one(self):
        self.assertEqual(self.error("400 invalid request: image content blocks are not supported"),
                         "provider_rejected_image")
        self.assertEqual(self.error("422 unprocessable: messages.0.content: unknown block type image"),
                         "provider_rejected_image")
        self.assertEqual(self.error("The model does not support image input"), "provider_rejected_image")

    def test_no_result_event_is_a_cli_error(self):
        m = argparse.Namespace(killed=None, init_problems=[], attestation={"model": "m"}, result=None)
        self.assertEqual(gs.provider_error_of({"exit_code": 0, "killed": None}, m), "cli_error")


if __name__ == "__main__":
    unittest.main()
