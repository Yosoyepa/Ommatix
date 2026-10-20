"""Regressions for the Codex re-review of PR 101 at d04f21b (F1-F6; docs/reviews/2026-10-20-pr101-ag03-harness.md).

Written and run RED against d04f21b before each fix (the red output of every finding is kept per class). Fakes,
a tiny silent Python child and the owned-process-tree runner only: no screen, window, input or provider.

Rules these tests pin: a terminal state (safety event, uncertain delivery, operator stop) is never discarded or
retried by a later classification (init, timeout, parse); the stop file is created only if absent and never
overwritten or deleted; a model runs only with its own passing gate record bound to that model and route."""

import contextlib
import io
import json
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_plan as plan  # noqa: E402
import pixel_runner as pr  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import run_goal as rg  # noqa: E402
from test_campaign import Series  # noqa: E402

TERMINAL_LINE = json.dumps({"tool": "click", "ok": False, "code": "FOREGROUND_NOT_TARGET_PROCESS",
                            "reserved": True, "terminal": "safety"})


# ---- F1: a terminal state is never discarded by a later classification -----------------------------------

class F1TerminalSurvivesInitTests(unittest.TestCase):
    def test_a_terminal_state_with_a_missing_init_ends_the_campaign_and_is_never_retried(self):
        for behaviour, stop_created in (("noinit_safety", True), ("noinit_operator_stop", False),
                                        ("noinit_uncertain", True)):
            order = plan.goal_order(1)
            s = Series(self, script={(order[2], 1): behaviour})
            with self.subTest(behaviour):
                self.assertEqual(s.run(), 2)
                self.assertEqual([goal for goal, _ in s.calls], order[:3])  # no later app, no later model session
                self.assertEqual(s.app_calls[order[2]], 1)                  # the slot's attempt is not retried
                counts = s.summary()["counts"]
                self.assertEqual((counts["not_attempted"], counts["raw_attempts"]), (8, 3))
                self.assertEqual(s.stop.exists(), stop_created)
                raw = s.summary()["slots"][2]["raw"][0]
                self.assertTrue(raw["model_called"])
                self.assertFalse(raw["harness_fault"])

    def test_an_uncertain_delivery_and_its_reservation_stay_in_the_raw_result_when_the_init_is_missing(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "noinit_uncertain"})
        s.run()
        result = json.loads((s.series_dir / order[0] / "attempt1" / "result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["pixel_server"]["uncertain_delivery"], 1)
        self.assertEqual(result["pixel_server"]["live_forwarded"], 1)
        self.assertGreaterEqual(result["pixel_server"]["delivered"], 1)  # never reported as nothing sent
        self.assertEqual(result["safety_event"], "uncertain_delivery")

    def test_a_model_response_without_init_is_not_a_precall_attempt_and_is_not_retried(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[1], 1): "noinit_response"})
        self.assertEqual(s.run(), 0)
        self.assertEqual(s.app_calls[order[1]], 1)
        slot = s.summary()["slots"][1]
        self.assertTrue(slot["raw"][0]["model_called"])
        self.assertEqual(s.summary()["counts"]["technical_failures"], 0)

    def test_an_unclean_init_is_not_a_proof_that_the_model_was_never_called(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[1], 1): "bad_init"})
        self.assertEqual(s.run(), 0)
        self.assertEqual(s.app_calls[order[1]], 1)  # unknown progress earns no retry
        self.assertTrue(s.summary()["slots"][1]["raw"][0]["model_called"])
        self.assertEqual(s.summary()["counts"]["technical_failures"], 0)


# ---- F2: termination does not wait for another stdout line -----------------------------------------------

CHILD = textwrap.dedent('''
    import json, pathlib, sys, time
    print(json.dumps({"type": "assistant", "message": {"id": "m0", "content": [],
                                                       "usage": {"input_tokens": 1, "output_tokens": 1}}}), flush=True)
    time.sleep(0.4)
    exec(pathlib.Path(sys.argv[1]).read_text())
    time.sleep(120)
''')


class F2SilentTerminationTests(unittest.TestCase):
    def run_child(self, action: str, hook_factory):
        scratch = tempfile.TemporaryDirectory(prefix="ag03-f2-")
        self.addCleanup(scratch.cleanup)
        tmp = Path(scratch.name)
        self.log, self.stop = tmp / "pixel.jsonl", tmp / "stop.flag"
        (tmp / "child.py").write_text(CHILD, encoding="utf-8")
        (tmp / "action.py").write_text(action.replace("@LOG@", repr(str(self.log))).replace("@STOP@", repr(str(self.stop))),
                                       encoding="utf-8")
        metrics = rg.StreamMetrics([], None, None)
        started = time.monotonic()
        ran = rg.run_claude([sys.executable, str(tmp / "child.py"), str(tmp / "action.py")], "", tmp, metrics, 60.0,
                            should_stop=hook_factory(self.log, self.stop))
        return ran, time.monotonic() - started

    def test_a_terminal_state_written_after_the_last_stdout_line_stops_the_owned_tree_promptly(self):
        ran, elapsed = self.run_child(
            "import pathlib; pathlib.Path(@LOG@).write_text(" + repr(TERMINAL_LINE + "\n") + ")",
            lambda log, stop: pr.session_stop_hook(log, stop))
        self.assertEqual(ran["killed"], "terminal")
        self.assertLess(elapsed, 20)
        self.assertTrue(self.stop.exists())      # created at once, by the hook, before any report or cleanup
        self.assertEqual(self.stop.read_bytes(), b"")

    def test_an_operator_stop_engaged_during_silence_ends_the_session_and_leaves_the_file_alone(self):
        ran, elapsed = self.run_child("import pathlib; pathlib.Path(@STOP@).write_bytes(b'operator-marker')",
                                      lambda log, stop: pr.session_stop_hook(log, stop))
        self.assertEqual(ran["killed"], "operator_stop")
        self.assertLess(elapsed, 20)
        self.assertEqual(self.stop.read_bytes(), b"operator-marker")  # neither created, truncated nor deleted

    def test_an_unreadable_stop_file_during_silence_fails_closed(self):
        def denied(path):
            raise PermissionError("denied")
        ran, elapsed = self.run_child("pass", lambda log, stop: pr.session_stop_hook(log, stop, stat=denied))
        self.assertEqual(ran["killed"], "stop_unreadable")
        self.assertLess(elapsed, 20)

    def test_the_hook_reports_nothing_while_all_is_quiet(self):
        with tempfile.TemporaryDirectory() as tmp:
            hook = pr.session_stop_hook(Path(tmp) / "pixel.jsonl", Path(tmp) / "stop.flag")
            self.assertIsNone(hook())

    def test_run_goal_engages_the_stop_before_it_reports_or_cleans_up(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "safety"})
        s.run()
        self.assertTrue(s.stop.exists())  # the fake session called the hook, which engaged the stop at once
        result = json.loads((s.series_dir / order[0] / "attempt1" / "result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["cli"]["killed"], "terminal")


# ---- F3: the stop file is created only if absent --------------------------------------------------------

class F3StopFileOwnershipTests(unittest.TestCase):
    def test_a_preexisting_stop_file_is_left_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as tmp:
            stop = Path(tmp) / "stop.flag"
            stop.write_bytes(b"operator \x00 marker")
            self.assertEqual(pr.engage_stop(stop), "already")
            self.assertEqual(stop.read_bytes(), b"operator \x00 marker")

    def test_an_absent_stop_file_is_created_empty_exactly_once_across_concurrent_observations(self):
        with tempfile.TemporaryDirectory() as tmp:
            stop = Path(tmp) / "stop.flag"
            results, barrier = [], threading.Barrier(16)

            def observe():
                barrier.wait()
                results.append(pr.engage_stop(stop))
            threads = [threading.Thread(target=observe) for _ in range(16)]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            self.assertEqual(results.count("created"), 1)
            self.assertEqual(results.count("already"), 15)
            self.assertEqual(stop.read_bytes(), b"")

    def test_a_creation_that_cannot_be_confirmed_is_reported_as_failed(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(pr.engage_stop(Path(tmp) / "missing-dir" / "stop.flag"), "failed")

    def test_a_safety_stop_never_truncates_a_file_the_operator_created_in_the_race(self):
        order = plan.goal_order(1)
        s = Series(self, script={(order[0], 1): "safety_race"})
        self.assertEqual(s.run(), 2)
        self.assertEqual(s.stop.read_bytes(), b"operator-marker")

    def test_no_check_then_truncate_sequence_remains(self):
        source = (HERE / "pixel_runner.py").read_text(encoding="utf-8")
        self.assertNotIn("stop_file.write_text", source)
        self.assertNotIn("stop.write_text", source)


# ---- F4: the gate belongs to this model, this resolved identity and this route ---------------------------

class F4GateBindingTests(unittest.TestCase):
    def refused(self, **kw):
        s = Series(self, **kw)
        with self.assertRaises(SystemExit):
            s.run()
        self.assertEqual(s.calls, [])
        self.assertEqual(s.app_calls, {})
        self.assertFalse(s.series_dir.exists())

    def test_a_haiku_gate_with_a_sonnet_model_refuses_before_any_session(self):
        # the proof, the init and the model are sonnet; the gate record is Haiku's
        self.refused(model_id="claude-sonnet-5-5", gate_requested="claude-haiku-5-5", gate_resolved="claude-haiku-5-5")

    def test_a_model_outside_the_accepted_set_is_refused_even_with_a_matching_gate(self):
        self.refused(model_id="claude-sonnet-5-5")

    def test_a_gate_whose_requested_model_differs_from_the_one_being_run_is_refused(self):
        self.refused(gate_requested="claude-haiku-5-4")

    def test_a_gate_whose_resolved_model_differs_from_the_attested_one_is_refused(self):
        self.refused(gate_resolved="claude-haiku-5-5-preview")

    def test_a_gate_for_another_route_endpoint_is_refused(self):
        self.refused(route="deepseek", gate_base_url="https://api.deepseek.com/anthropic-other")

    def test_a_gate_without_a_model_identity_is_refused(self):
        s = Series(self)
        path = Path(s.argv[s.argv.index("--gate-record") + 1])
        record = json.loads(path.read_text(encoding="utf-8"))
        path.write_text(json.dumps(record | {"requested_model": None, "resolved_model": None}), encoding="utf-8")
        with self.assertRaises(SystemExit):
            s.run()
        self.assertEqual(s.calls, [])

    def test_the_supported_alias_runs_through_its_attested_resolved_identity(self):
        # requested alias deepseek-flash, resolved by the attested init/proof to the provider's concrete model
        s = Series(self, route="deepseek", resolved_id="deepseek-v4.1-flash-1017",
                   gate_requested="deepseek-flash", gate_resolved="deepseek-v4.1-flash-1017")
        self.assertEqual(s.run(), 0)
        fp = s.summary()["fingerprint"]
        self.assertEqual((fp["requested_model"], fp["resolved_model"]), ("deepseek-flash", "deepseek-v4.1-flash-1017"))

    def test_the_alias_gate_does_not_cover_another_resolution(self):
        self.refused(route="deepseek", resolved_id="deepseek-v4.2-flash", gate_requested="deepseek-flash",
                     gate_resolved="deepseek-v4.1-flash-1017")

    def test_the_accepted_set_is_the_owners_decision_o2(self):
        self.assertEqual(pr.ACCEPTED_MODELS, {"Haiku": ("claude-haiku-5-5",), "DeepSeek": ("deepseek-flash",),
                                              "MiMo": ("mimo-v2.6-flash",)})


# ---- F5: a primary slot is claimed once, whatever --out says ---------------------------------------------

class F5PrimaryClaimTests(unittest.TestCase):
    def other_out(self, s, name="out2"):
        return ["--out", str(s.root / name)]

    def test_changing_out_does_not_execute_the_same_primary_slots_again(self):
        first = Series(self)
        self.assertEqual(first.run(), 0)
        again = Series(self, root=first.root)
        with self.assertRaises(SystemExit) as raised:
            again.run(extra=self.other_out(first))
        self.assertIn("already settled", str(raised.exception))
        self.assertEqual(again.calls, [])
        self.assertEqual(len(first.calls), 11)

    def test_a_deliberate_diagnostic_run_is_recorded_outside_the_primary_pool(self):
        first = Series(self)
        first.run()
        claims = first.root / "campaign" / "claims"
        before = {p.name: p.read_bytes() for p in claims.rglob("*") if p.is_file()}
        diag = Series(self, root=first.root)
        self.assertEqual(diag.run(extra=[*self.other_out(first), "--diagnostic"]), 0)
        summary = json.loads(((first.root / "out2") / "Haiku-ag03-phase1-rep1" / "series.json").read_text())
        self.assertTrue(summary["diagnostic"])
        after = {p.name: p.read_bytes() for p in claims.rglob("*") if p.is_file() and "diagnostic" not in str(p)}
        self.assertEqual(before, after)  # the primary claims are untouched
        self.assertEqual(len(diag.calls), 11)

    def test_a_restarted_series_resumes_without_rerunning_settled_slots_or_regranting_attempts(self):
        order = plan.goal_order(1)
        first = Series(self, script={(order[4], 1): "crash"})
        with self.assertRaises(RuntimeError):
            first.run()
        self.assertEqual([g for g, _ in first.calls], order[:5])
        second = Series(self, root=first.root)
        self.assertEqual(second.run(extra=self.other_out(first)), 0)
        self.assertEqual([g for g, _ in second.calls], order[5:])  # settled slots are not run again
        summary = json.loads(((first.root / "out2") / "Haiku-ag03-phase1-rep1" / "series.json").read_text())
        counts = summary["counts"]
        # the attempt that died with the process is consumed (unknown progress is not a pre-call fault): no retry
        self.assertEqual((counts["primary_n"], counts["raw_attempts"], counts["successes"]), (11, 11, 10))

    def test_a_stopped_campaign_stays_stopped_for_a_second_invocation(self):
        order = plan.goal_order(1)
        first = Series(self, script={(order[2], 1): "operator_stop"})
        self.assertEqual(first.run(), 2)
        again = Series(self, root=first.root)
        with self.assertRaises(SystemExit):
            again.run(extra=self.other_out(first))
        self.assertEqual(again.calls, [])

    def test_concurrent_claims_of_one_attempt_have_exactly_one_winner(self):
        with tempfile.TemporaryDirectory() as tmp:
            claim_dir = Path(tmp) / "claims"
            claim_dir.mkdir()
            wins, barrier = [], threading.Barrier(12)

            def claim():
                barrier.wait()
                wins.append(pr.claim_attempt(claim_dir, "G01", 1))
            threads = [threading.Thread(target=claim) for _ in range(12)]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            self.assertEqual(wins.count(True), 1)

    def test_the_claim_does_not_depend_on_the_output_path(self):
        source = (HERE / "pixel_runner.py").read_text(encoding="utf-8")
        self.assertIn("claim_dir(", source)


# ---- F6: the display fingerprint is measured, not asserted -----------------------------------------------

class FakeDisplayApi:
    def __init__(self, primary=(0, 0, 2560, 1440), dpi=96, virtual=(0, 0, 2560, 1440)):
        self._primary, self._dpi, self._virtual = primary, dpi, virtual

    def primary_monitor_rect(self):
        return self._primary

    def primary_effective_dpi(self):
        return self._dpi

    def virtual_screen(self):
        return self._virtual


class F6DisplayTests(unittest.TestCase):
    def test_the_decoder_reads_numeric_scaling_from_the_effective_dpi(self):
        for dpi, scaling in ((96, 100), (120, 125), (144, 150), (192, 200)):
            self.assertEqual(w32.decode_display((0, 0, 2560, 1440), dpi),
                             {"resolution": "2560x1440", "scaling": scaling}, dpi)

    def test_unknown_readings_are_unknown_not_a_default(self):
        for rect, dpi in (((0, 0, 0, 0), 96), (None, 96), ((0, 0, 1920, 1080), 0), ((0, 0, 1920, 1080), None),
                          ((0, 0, 1920, 1080), -96)):
            self.assertIsNone(w32.decode_display(rect, dpi), (rect, dpi))

    def test_the_primary_monitor_is_measured_not_the_virtual_desktop(self):
        api = FakeDisplayApi(primary=(0, 0, 1920, 1080), dpi=120, virtual=(-1920, 0, 3840, 1080))
        self.assertEqual(w32.measure_primary_display(api), {"resolution": "1920x1080", "scaling": 125})

    def test_a_secondary_monitor_layout_does_not_stand_in_for_the_primary(self):
        one = w32.measure_primary_display(FakeDisplayApi(virtual=(0, 0, 2560, 1440)))
        two = w32.measure_primary_display(FakeDisplayApi(virtual=(-3840, 0, 6400, 2160)))
        self.assertEqual(one, two)

    def test_the_production_reader_is_not_a_constant(self):
        source = (HERE / "pixel_runner.py").read_text(encoding="utf-8")
        self.assertNotIn("verified at start", source)
        facts = pr.real_display_facts(FakeDisplayApi(dpi=144))
        self.assertEqual((facts["resolution"], facts["scaling"]), ("2560x1440", 150))

    def test_the_same_resolution_with_a_different_scaling_changes_the_pool_key(self):
        a = Series(self, display={"resolution": "2560x1440", "scaling": 100, "capture_backend": "printwindow"})
        a.run()
        pool = a.root / "campaign" / "campaign-Haiku-AG-03-phase1.json"
        stored = json.loads(pool.read_text(encoding="utf-8"))
        self.assertEqual(stored["monitor_scaling"], 100)
        # same campaign directory, same everything else, only the scaling differs
        b2 = Series(self, root=a.root, rep=2, display={"resolution": "2560x1440", "scaling": 125,
                                                       "capture_backend": "printwindow"})
        with self.assertRaises(SystemExit) as raised:
            b2.run()
        self.assertIn("monitor_scaling", str(raised.exception))
        self.assertEqual(b2.calls, [])

    def test_unknown_scaling_or_resolution_is_refused_before_any_session(self):
        for display in ({"resolution": "2560x1440", "scaling": None, "capture_backend": "printwindow"},
                        {"resolution": None, "scaling": 100, "capture_backend": "printwindow"},
                        {"resolution": "2560x1440", "scaling": 100, "capture_backend": None}):
            s = Series(self, display=display)
            with self.subTest(display), self.assertRaises(SystemExit) as raised:
                s.run()
            self.assertIn("display", str(raised.exception))
            self.assertEqual(s.calls, [])
            self.assertFalse(s.series_dir.exists())


if __name__ == "__main__":
    unittest.main()
