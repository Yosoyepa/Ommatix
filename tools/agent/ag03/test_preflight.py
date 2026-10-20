"""AG-03 isolation preflight and gate (ADR-0038 decision 6), against a scripted stand-in for the claude CLI
and a local stand-in for the API: no real model, no provider, no network beyond 127.0.0.1, no screen.
Positive controls: every broken copy (an extra tool, an image outside ``screenshot``, an oversized image,
a sentinel, ...) must make the preflight fail."""

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
import pixel_gate as gate  # noqa: E402
import pixel_plan as plan  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import run_goal as rg  # noqa: E402

DEEPSEEK_URL = agentcli.CLAUDE_ROUTES["deepseek"]["base_urls"][0]
MIMO_URL = agentcli.CLAUDE_ROUTES["mimo"]["base_urls"][0]
ROUTES = {"deepseek": ("deepseek-flash", DEEPSEEK_URL), "mimo": ("mimo-v2.6-flash", MIMO_URL)}


class Base(unittest.TestCase):
    def run_preflight(self, mode="normal", *, route=None, files=None, refusals=()):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-pf-")
        self.addCleanup(scratch.cleanup)
        tmp = Path(scratch.name)
        self.argv_file, self.env_file = tmp / "argv.jsonl", tmp / "env.json"
        self.production = tmp / "production"
        self.production.mkdir()
        for name, text in (files or {}).items():
            (self.production / name).write_text(text, encoding="utf-8")
        extra = {"FAKE_CLAUDE_MODE": mode, "FAKE_CLAUDE_ARGV_FILE": str(self.argv_file),
                 "FAKE_CLAUDE_ENV_FILE": str(self.env_file), "FAKE_CLAUDE_LEAK_DIR": str(self.production)}
        model, url = ROUTES[route] if route else ("claude-haiku-5-5", None)
        args = argparse.Namespace(exe=Path(sys.executable), model=model,
                                  claude=[sys.executable, str(HERE / "fake_claude_pixel.py")], timeout=60,
                                  claude_config_dir=self.production, extra_env=extra, route=route,
                                  route_base_url=url)
        self.server_args = []
        real_build = pa.build_mcp_config

        def spy(python, server_args):
            self.server_args.append(list(server_args))
            return real_build(python, server_args)
        with mock.patch.object(pf.ag02, "refusals", return_value=list(refusals)), \
                mock.patch.object(pa, "build_mcp_config", side_effect=spy):
            return pf.run(args)


class NativePreflightTests(Base):
    def test_a_confined_session_passes_and_the_control_proves_the_canary_can_see_a_leak(self):
        proof = self.run_preflight()
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["schema"], "ag03-isolation-proof-1")
        self.assertEqual(proof["tools"], sorted(pa.ALLOWED_TOOL_NAMES))
        self.assertEqual(proof["production_leaked_kinds"], [])
        self.assertIn("user_claude_md", proof["control_leaked_kinds"])
        self.assertEqual(proof["pixel"]["request_tools"], sorted(pa.ALLOWED_TOOL_NAMES))
        self.assertTrue(proof["pixel"]["images_only_in_screenshot_result"])
        self.assertGreaterEqual(proof["pixel"]["screenshot_result_images"], 1)
        self.assertEqual(proof["pixel"]["control_blind"], [])
        self.assertNotIn("route", proof)

    def test_every_positive_control_detects_its_broken_request(self):
        proof = self.run_preflight()
        self.assertEqual(sorted(proof["pixel"]["control_detected"]),
                         sorted(["extra_tool", "dropped_tool", "image_outside_screenshot",
                                 "image_in_another_tools_result", "oversized_image", "too_wide_image",
                                 "not_a_png", "image_as_text", "sentinel"]))

    def test_the_proof_binds_the_server_code_and_the_pinned_instruction(self):
        proof = self.run_preflight()
        self.assertEqual(proof["pixel"]["server_sha256"], pf.file_sha256(pf.SERVER_PATH))
        self.assertEqual(proof["pixel"]["instruction_sha256"], pa.SYSTEM_INSTRUCTION_SHA256)
        self.assertEqual(pa.instruction_sha256(), pa.SYSTEM_INSTRUCTION_SHA256)

    def test_a_real_capture_never_happens_in_the_preflight(self):
        proof = self.run_preflight()
        self.assertFalse(proof["real_capture_in_this_proof"])
        self.assertTrue(self.server_args)
        for server_args in self.server_args:
            self.assertIn("--synthetic", server_args)
            self.assertNotIn("--attest-pid", server_args)
            self.assertNotIn("--admission", server_args)

    def test_the_production_run_uses_the_pixel_command_and_the_control_widens_only_the_sources(self):
        self.run_preflight()
        runs = [json.loads(line) for line in self.argv_file.read_text().splitlines()]
        production, control = runs[0], runs[1]
        self.assertEqual(production[production.index("--setting-sources") + 1], "")
        self.assertEqual(production[production.index("--tools") + 1], "")
        self.assertEqual(production[production.index("--allowedTools") + 1], ",".join(pa.ALLOWED_TOOL_NAMES))
        self.assertEqual(production[production.index("--system-prompt") + 1], pa.SYSTEM_INSTRUCTION)
        self.assertEqual(control[control.index("--setting-sources") + 1], "user,project,local")

    def test_the_loader_accepts_the_proof_and_ag02_rejects_it(self):
        proof = self.run_preflight()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            self.assertEqual(pf.load_isolation_proof(path)["schema"], pf.PROOF_SCHEMA)
            with self.assertRaises(SystemExit):
                rg.load_isolation_proof(path)  # AG-02's loader does not accept an AG-03 proof

    def test_the_ag03_loader_rejects_a_proof_of_another_schema_or_revision_or_with_a_blind_control(self):
        proof = self.run_preflight()
        broken = {"schema": proof | {"schema": "ag02-isolation-proof-3"},
                  "revision": proof | {"verifier_revision": proof["verifier_revision"] + 1},
                  "failed": proof | {"passed": False},
                  "blind": proof | {"pixel": proof["pixel"] | {"control_blind": ["sentinel"]}},
                  "capture": proof | {"real_capture_in_this_proof": True},
                  "other_tools": proof | {"tools": ["mcp__ommatix__observe_screen"]}}
        with tempfile.TemporaryDirectory() as tmp:
            for name, body in broken.items():
                path = Path(tmp) / f"{name}.json"
                path.write_text(json.dumps(body), encoding="utf-8")
                with self.subTest(name), self.assertRaises(SystemExit):
                    pf.load_isolation_proof(path)

    def test_the_proof_does_not_cover_a_run_with_another_server_or_instruction(self):
        proof = self.run_preflight()
        args = argparse.Namespace(model="claude-haiku-5-5")
        self.assertEqual(pf.proof_problems(proof, args, self.production, None), [])
        with mock.patch.object(pf, "file_sha256", return_value="0" * 64):
            self.assertIn("pixel_server_differs_from_proof", pf.proof_problems(proof, args, self.production, None))
        with mock.patch.object(pa, "instruction_sha256", return_value="0" * 64):
            self.assertIn("instruction_differs_from_proof", pf.proof_problems(proof, args, self.production, None))
        self.assertIn("requested_model_differs_from_proof",
                      pf.proof_problems(proof, argparse.Namespace(model="other"), self.production, None))


class BrokenSessionTests(Base):
    def assert_fails(self, mode, token, **kw):
        proof = self.run_preflight(mode, **kw)
        self.assertFalse(proof["passed"], f"{mode} should fail")
        self.assertIn(token, proof["problems"], proof["problems"])
        return proof

    def test_an_extra_tool_fails_at_init(self):
        # The session is ended at once at init (as in AG-02), so no request is sent; the request-level
        # extra-tool verdict is exercised by the positive control "extra_tool" above.
        self.assert_fails("extra_tool", "init_tools_differ")

    def test_an_image_from_any_other_source_fails(self):
        self.assert_fails("image_elsewhere", "pixel_image_outside_screenshot")
        self.assert_fails("image_in_click", "pixel_image_outside_screenshot")

    def test_an_oversized_a_too_wide_and_a_non_png_image_fail(self):
        self.assert_fails("image_oversized", "pixel_image_too_large")
        self.assert_fails("image_wide", "pixel_image_too_wide")
        self.assert_fails("image_jpeg", "pixel_image_not_png")

    def test_an_image_sent_as_text_fails(self):
        self.assert_fails("image_text", "pixel_image_as_text")

    def test_a_session_whose_requests_never_carried_a_screenshot_fails(self):
        proof = self.assert_fails("no_image", "pixel_no_screenshot_result_in_requests")
        self.assertIn("pixel_controls_without_a_real_request", proof["problems"])

    def test_a_sentinel_in_the_production_request_fails(self):
        proof = self.assert_fails("leak", "sentinels_in_production_request")
        self.assertIn("pixel_sentinel_in_request", proof["problems"])
        self.assertIn("user_claude_md", proof["production_leaked_kinds"])

    def test_a_canary_that_cannot_see_a_leak_even_in_the_control_fails(self):
        self.assert_fails("blind", "control_blind")

    def test_a_hook_that_runs_in_production_fails(self):
        self.assert_fails("hooks", "hook_or_extra_server_ran_in_production")

    def test_no_captured_request_means_no_evidence(self):
        self.assert_fails("silent", "canary_session_no_request_captured")

    def test_the_production_directorys_own_context_in_the_request_fails(self):
        self.assert_fails("leak", "production_dir_context_in_request",
                          files={"CLAUDE.md": "The operator's private instruction line number one.\n"})

    def test_a_verdict_that_cannot_see_an_oversized_image_makes_the_preflight_fail_as_blind(self):
        real = pa.check_pixel_requests

        def blind_to_size(requests, **kw):
            problems, facts = real(requests, **kw)
            return [p for p in problems if p != "image_too_large"], facts
        with mock.patch.object(pa, "check_pixel_requests", side_effect=blind_to_size):
            proof = self.run_preflight()
        self.assertFalse(proof["passed"])
        self.assertIn("pixel_control_blind", proof["problems"])
        self.assertIn("oversized_image", proof["pixel"]["control_blind"])

    def test_the_preflight_refuses_to_run_when_the_environment_is_not_local(self):
        with self.assertRaises(pf.PixelRefused) as caught:
            self.run_preflight(refusals=["routing_variable:ANTHROPIC_BEDROCK_BASE_URL"])
        self.assertIn("routing_variable:ANTHROPIC_BEDROCK_BASE_URL", caught.exception.reasons)


class RoutePreflightTests(Base):
    def test_a_provider_route_session_passes_with_the_image_verdict_of_this_arm(self):
        proof = self.run_preflight(route="deepseek")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["route"]["provider"], "deepseek")
        self.assertEqual(proof["route"]["base_url"], DEEPSEEK_URL)
        self.assertEqual(proof["route_verifier_revision"], pf.ROUTE_VERIFIER_REVISION)
        self.assertEqual(proof["route_control_blind"], [])
        self.assertTrue(proof["route_control_detected"])
        self.assertGreaterEqual(proof["pixel"]["screenshot_result_images"], 1)

    def test_the_mimo_route_passes_too_with_its_own_proof_facts(self):
        proof = self.run_preflight(route="mimo")
        self.assertTrue(proof["passed"], proof["problems"])
        self.assertEqual(proof["route"], {"provider": "mimo", "base_url": MIMO_URL, "model": "mimo-v2.6-flash",
                                          "effort": "max", "auth": "bearer"})
        self.assertEqual(proof["pixel"]["control_blind"], [])
        self.assertEqual(proof["route_control_blind"], [])

    def test_ag02s_route_verdict_would_have_failed_this_run_and_ag03s_does_not(self):
        # AG-02's route rule is "any image block fails"; the images of this arm are legitimate.
        proof = self.run_preflight(route="deepseek")
        requests_with_images = proof["pixel"]["screenshot_result_images"]
        self.assertGreaterEqual(requests_with_images, 1)
        self.assertNotIn("route_image_in_request", proof["problems"])

    def test_each_route_break_fails_the_preflight(self):
        for mode, token in (("route_other_model", "route_request_model_differs"),
                            ("route_key_in_body", "route_key_in_body"), ("route_bad_effort", "route_effort_differs"),
                            ("route_x_api_key", "route_x_api_key_header_differs"),
                            ("route_proxy_authorization", "route_header_not_expected"),
                            ("route_cookie", "route_header_not_expected")):
            with self.subTest(mode=mode):
                proof = self.run_preflight(mode, route="deepseek")
                self.assertFalse(proof["passed"])
                self.assertIn(token, proof["problems"], proof["problems"])

    def test_an_image_outside_screenshot_fails_on_a_route_too(self):
        proof = self.run_preflight("image_elsewhere", route="deepseek")
        self.assertIn("pixel_image_outside_screenshot", proof["problems"])

    def test_the_route_proof_is_bound_to_its_route_and_a_native_proof_is_not_a_route_proof(self):
        proof = self.run_preflight(route="deepseek")
        args = argparse.Namespace(model="deepseek-flash")
        facts = {"provider": "deepseek", "base_url": DEEPSEEK_URL, "model": "deepseek-flash", "effort": "max",
                 "auth": "bearer"}
        self.assertEqual(pf.proof_problems(proof, args, self.production, facts), [])
        self.assertIn("route_differs_from_proof", pf.proof_problems(proof, args, self.production, facts | {"model": "x"}))
        self.assertIn("route_proof_for_a_native_run", pf.proof_problems(proof, args, self.production, None))


class GateTests(unittest.TestCase):
    PRE = {"privacy_reading_recorded": True, "oi1_recorded": True, "owner_go_ahead": True}
    GOOD = "\n".join(f"IMAGE {n + 1}: WORD={w} SHAPE={s}" for n, (w, s) in enumerate(gate.expected_answers())) \
        + "\nVERDICT: DONE"

    def test_a_model_that_names_every_drawn_word_and_shape_passes(self):
        record = gate.gate_record("claude-haiku-5-5", "anthropic", preflight_passed=True,
                                  preconditions=self.PRE, answer=self.GOOD)
        self.assertEqual(record["status"], "passed")
        self.assertEqual(record["per_image_ok"], [True] * gate.SMOKE_IMAGES)
        self.assertTrue(record["provider_called"])

    def test_a_guess_that_gets_some_images_wrong_is_excluded_and_recorded(self):
        wrong = self.GOOD.replace(gate.expected_answers()[1][0], "ZZZ")
        record = gate.gate_record("m", "deepseek", preflight_passed=True, preconditions=self.PRE, answer=wrong)
        self.assertEqual((record["status"], record["reason"]), ("excluded", "smoke_failed"))
        self.assertEqual(record["per_image_ok"], [True, False, True, True])

    def test_the_same_answer_for_every_image_cannot_pass(self):
        word, shape = gate.expected_answers()[0]
        same = "\n".join(f"IMAGE {n}: WORD={word} SHAPE={shape}" for n in range(1, 5))
        self.assertFalse(all(gate.per_image_ok(same)))

    def test_a_missing_or_malformed_answer_fails(self):
        for answer in (None, "", "I see a window.", "IMAGE 1: nothing"):
            self.assertFalse(any(gate.per_image_ok(answer)), answer)

    def test_a_provider_that_rejects_images_is_excluded_with_its_closed_reason(self):
        record = gate.gate_record("m", "mimo", preflight_passed=True, preconditions=self.PRE,
                                  provider_error="provider_rejected_image")
        self.assertEqual((record["status"], record["reason"]), ("excluded", "provider_rejected_image"))
        other = gate.gate_record("m", "mimo", preflight_passed=True, preconditions=self.PRE, provider_error="weird")
        self.assertEqual(other["reason"], "cli_error")

    def test_a_failed_preflight_excludes_without_calling_the_provider(self):
        record = gate.gate_record("m", "deepseek", preflight_passed=False, preconditions=self.PRE, answer=self.GOOD)
        self.assertEqual((record["status"], record["reason"], record["provider_called"]),
                         ("excluded", "preflight_failed", False))

    def test_the_preconditions_must_be_recorded_before_the_smoke_runs(self):
        for provider, missing in (("anthropic", {"privacy_reading_recorded": True, "owner_go_ahead": True}),
                                  ("deepseek", {"owner_go_ahead": True}),
                                  ("mimo", {"privacy_reading_recorded": True})):
            record = gate.gate_record("m", provider, preflight_passed=True, preconditions=missing, answer=self.GOOD)
            with self.subTest(provider):
                self.assertEqual(record["status"], "not_run")
                self.assertFalse(record["provider_called"])
        claude = gate.gate_record("m", "anthropic", preflight_passed=True,
                                  preconditions={"privacy_reading_recorded": True, "owner_go_ahead": True})
        self.assertIn("oi1_not_recorded", claude["missing"])
        deepseek = gate.gate_record("m", "deepseek", preflight_passed=True,
                                    preconditions={"privacy_reading_recorded": True, "owner_go_ahead": True},
                                    answer=self.GOOD)
        self.assertEqual(deepseek["status"], "passed")  # OI-1 is Claude's alone

    def test_a_record_holds_no_text_of_the_models_answer(self):
        record = gate.gate_record("m", "deepseek", preflight_passed=True, preconditions=self.PRE,
                                  answer=self.GOOD + "\nSEGRETO-MARCADOR-777")
        text = json.dumps(record)
        self.assertNotIn("SEGRETO-MARCADOR-777", text)
        for word, _ in gate.expected_answers():
            self.assertNotIn(word, text)

    def test_an_excluded_model_loses_its_ag03_arm_in_the_schedule(self):
        records = [gate.gate_record("Haiku", "anthropic", preflight_passed=True, preconditions=self.PRE,
                                    answer=self.GOOD),
                   gate.gate_record("MiMo", "mimo", preflight_passed=True, preconditions=self.PRE,
                                    provider_error="provider_rejected_image")]
        rows = plan.schedule(excluded=gate.excluded_arms(records))
        self.assertTrue(all("AG-03" in r["arms"] for r in rows if r["model"] == "Haiku"))
        self.assertTrue(all("AG-03" not in r["arms"] for r in rows if r["model"] == "MiMo"))

    def test_the_smoke_prompt_is_pinned_and_the_images_contrast(self):
        self.assertEqual(len(set(gate.expected_answers())), gate.SMOKE_IMAGES)
        self.assertIn("four times", gate.SMOKE_PROMPT)
        pngs = {cap.synthetic_png(i, 64, 48)[0] for i in range(gate.SMOKE_IMAGES)}
        self.assertEqual(len(pngs), gate.SMOKE_IMAGES)


if __name__ == "__main__":
    unittest.main()
