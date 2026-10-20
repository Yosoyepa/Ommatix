"""AG-03 / AG-02R slots (ADR-0038 decision 19): the primary denominator, retries inside the slot, the
separate raw-attempt and technical-failure counts, not-attempted slots after a campaign stop, and an
incomplete series dropping out of the series-score contrasts. A fake runner stands in for the agent."""

import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_plan as plan  # noqa: E402
import pixel_slots as sl  # noqa: E402

OK = sl.Attempt(success=True)
FAIL = sl.Attempt()
PRECALL = sl.Attempt(model_called=False, harness_fault=True)


def keys(model="Haiku", arm="AG-03", phase=2, reps=(1,)):
    return [(model, arm, phase, rep, goal) for rep in reps for goal in plan.goal_order(rep)]


class Script:
    """A fake runner: the Nth call for a key answers from ``plan`` (a dict key -> list of attempts)."""

    def __init__(self, answers, default=OK):
        self.answers, self.default, self.calls = answers, default, []

    def __call__(self, key):
        self.calls.append(key)
        queue = self.answers.get(key)
        return queue.pop(0) if queue else self.default


class RetryRuleTests(unittest.TestCase):
    def test_a_pre_call_abort_is_retried_once_inside_the_slot_and_counts_once_with_the_retry_outcome(self):
        key = ("Haiku", "AG-03", 2, 1, "G01")
        slot = sl.execute_slot(key, Script({key: [PRECALL, OK]}))
        self.assertEqual(len(slot.attempts), 2)
        self.assertTrue(slot.success)
        self.assertEqual(slot.primary, OK)
        self.assertFalse(slot.technical_failure)

    def test_two_invalid_attempts_are_a_technical_failure_a_non_success_in_the_denominator(self):
        key = ("Haiku", "AG-03", 2, 1, "G01")
        slot = sl.execute_slot(key, Script({key: [PRECALL, PRECALL, OK]}))
        self.assertEqual(len(slot.attempts), 2)  # never a third
        self.assertTrue(slot.technical_failure)
        self.assertFalse(slot.success)
        self.assertEqual(sl.tally([slot], "Haiku", "AG-03", 2, "G01")["n"], 1)

    def test_nothing_is_retried_after_the_first_model_call(self):
        key = ("Haiku", "AG-03", 2, 1, "G01")
        for attempt in (FAIL, sl.Attempt(model_called=True, harness_fault=True),  # a provider error or timeout
                        sl.Attempt(capture_refused=True), sl.Attempt(gate_refused=True)):
            with self.subTest(attempt=attempt):
                runner = Script({key: [attempt, OK]})
                slot = sl.execute_slot(key, runner)
                self.assertEqual(len(slot.attempts), 1)
                self.assertEqual(len(runner.calls), 1)
                self.assertFalse(slot.success)

    def test_a_capture_refusal_a_gate_refusal_and_an_exclusion_are_never_invalid_even_before_a_call(self):
        for field in ("capture_refused", "gate_refused", "excluded_model"):
            attempt = sl.Attempt(model_called=False, harness_fault=True, **{field: True})
            self.assertFalse(attempt.invalid, field)
            slot = sl.execute_slot(("m", "a", 1, 1, "G01"), Script({}, default=attempt))
            self.assertEqual(len(slot.attempts), 1, field)

    def test_the_retry_cannot_bypass_a_capture_refusal(self):
        key = ("Haiku", "AG-03", 2, 1, "G01")
        runner = Script({key: [sl.Attempt(model_called=False, harness_fault=True, capture_refused=True), OK]})
        slot = sl.execute_slot(key, runner)
        self.assertFalse(slot.success)
        self.assertEqual(len(runner.calls), 1)


class CampaignStopTests(unittest.TestCase):
    def setUp(self):
        self.all_keys = keys("Haiku", "AG-03", 2, reps=(1,)) + keys("Haiku", "AG-02R", 2, reps=(1,))
        self.third = self.all_keys[2]
        self.runner = Script({self.all_keys[0]: [PRECALL, OK], self.third: [sl.Attempt(safety_event=True)]})
        self.slots = sl.run_campaign(self.all_keys, self.runner)

    def test_a_safety_event_stops_the_campaign_and_later_slots_are_not_attempted(self):
        attempted = [s for s in self.slots if s.attempted]
        self.assertEqual(len(attempted), 3)
        self.assertEqual(len([s for s in self.slots if not s.attempted]), 19)
        self.assertEqual(len(self.runner.calls), 4)  # slot 0 twice (pre-call abort and retry), slots 1 and 2

    def test_the_slot_with_the_retry_counts_once_with_the_retry_outcome(self):
        t = sl.tally(self.slots, "Haiku", "AG-03", 2, self.all_keys[0][4])
        self.assertEqual((t["k"], t["n"], t["raw_attempts"], t["technical_failures"]), (1, 1, 2, 0))

    def test_raw_attempts_and_technical_failures_are_separate_from_the_primary_counts(self):
        keys_ = keys("DeepSeek", "AG-02R", 1, reps=(1,))
        runner = Script({keys_[0]: [PRECALL, PRECALL], keys_[1]: [PRECALL, OK]})
        slots = sl.run_campaign(keys_[:2], runner)
        first = sl.tally(slots, "DeepSeek", "AG-02R", 1, keys_[0][4])
        second = sl.tally(slots, "DeepSeek", "AG-02R", 1, keys_[1][4])
        self.assertEqual((first["k"], first["n"], first["raw_attempts"], first["technical_failures"]), (0, 1, 2, 1))
        self.assertEqual((second["k"], second["n"], second["raw_attempts"], second["technical_failures"]), (1, 1, 2, 0))

    def test_not_attempted_slots_are_not_failures_and_not_in_n(self):
        goal = self.all_keys[-1][4]
        t = sl.tally(self.slots, "Haiku", "AG-02R", 2, goal)
        self.assertEqual((t["k"], t["n"], t["not_attempted"]), (0, 0, 1))
        self.assertIsNone(t["wilson95"])

    def test_a_series_with_a_not_attempted_slot_is_incomplete(self):
        self.assertFalse(sl.series_complete(self.slots, "Haiku", "AG-03", 2, 1))
        self.assertIsNone(sl.series_score(self.slots, "Haiku", "AG-03", 2, 1))

    def test_the_operators_stop_file_also_ends_the_campaign(self):
        runner = Script({self.all_keys[0]: [sl.Attempt(operator_stop=True)]})
        slots = sl.run_campaign(self.all_keys, runner)
        self.assertEqual(sum(1 for s in slots if s.attempted), 1)


class SeriesContrastTests(unittest.TestCase):
    def full(self, model, arm, rep, score, phase=2):
        ks = keys(model, arm, phase, reps=(rep,))
        runner = Script({k: [OK if i < score else FAIL] for i, k in enumerate(ks)})
        return sl.run_campaign(ks, runner)

    def test_a_complete_series_scores_its_successes(self):
        self.assertEqual(sl.series_score(self.full("Haiku", "AG-03", 1, 7), "Haiku", "AG-03", 2, 1), 7)

    def test_an_incomplete_series_drops_its_block_out_of_the_arm_contrast(self):
        slots = []
        for rep, (a, b) in enumerate([(7, 5), (6, 5), (8, 6), (7, 6), (7, 5), (9, 6)], start=1):
            slots += self.full("Haiku", "AG-03", rep, a)
            slots += self.full("Haiku", "AG-02R", rep, b)
        full = sl.arm_contrast(slots, "Haiku", 2)
        self.assertEqual((full["blocks_planned"], full["blocks_used"]), (6, 6))
        self.assertEqual(full["diffs"], [2, 1, 2, 1, 2, 3])
        self.assertEqual(full["sign_flip"]["p_two_sided"], 0.03125)
        # rep 3's AG-03 series stops after four goals: the series is incomplete and the block drops out
        partial = [s for s in slots if not (s.key[1] == "AG-03" and s.key[3] == 3)]
        stopped = sl.run_campaign(keys("Haiku", "AG-03", 2, reps=(3,)),
                                  Script({keys("Haiku", "AG-03", 2, reps=(3,))[3]: [sl.Attempt(safety_event=True)]}))
        slots2 = partial + stopped
        reduced = sl.arm_contrast(slots2, "Haiku", 2)
        self.assertEqual((reduced["blocks_planned"], reduced["blocks_used"]), (6, 5))
        self.assertEqual(reduced["sign_flip"]["assignments"], 32)

    def test_the_model_contrast_pairs_the_series_of_the_same_repetition(self):
        slots = []
        for rep, (a, b) in enumerate([(7, 6), (6, 6), (8, 7), (7, 5), (7, 6), (9, 8)], start=1):
            slots += self.full("Haiku", "AG-02R", rep, a)
            slots += self.full("DeepSeek", "AG-02R", rep, b)
        out = sl.model_contrast(slots, "AG-02R", 2, "Haiku", "DeepSeek")
        self.assertEqual(out["diffs"], [1, 0, 1, 2, 1, 1])


if __name__ == "__main__":
    unittest.main()
