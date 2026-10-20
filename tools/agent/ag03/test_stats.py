"""AG-03 / AG-02R statistics (ADR-0038 decision 17): the numbers the ADR and chapter 09 section 9.12.2
state are reproduced, the sign-flip reference distribution has 64 assignments, and a pooled 0-of-18
bound is refused, with both correlated counterexamples as arithmetic."""

import math
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE)]

import pixel_stats as st  # noqa: E402


class WilsonTests(unittest.TestCase):
    def test_the_six_repetition_intervals_of_the_dossier(self):
        expected = {6: (0.61, 1.00), 5: (0.44, 0.97), 4: (0.30, 0.90), 3: (0.19, 0.81), 2: (0.10, 0.70),
                    1: (0.03, 0.56), 0: (0.00, 0.39)}
        for k, (lo, hi) in expected.items():
            got = st.wilson(k, 6)
            self.assertAlmostEqual(got[0], lo, places=2, msg=f"{k}/6 lower")
            self.assertAlmostEqual(got[1], hi, places=2, msg=f"{k}/6 upper")

    def test_two_of_six_cannot_be_separated_from_six_of_six(self):
        low, high = st.wilson(2, 6), st.wilson(6, 6)
        self.assertLess(low[0], high[0])
        self.assertGreater(low[1], high[0])  # the intervals overlap: 0.10-0.70 against 0.61-1.00

    def test_invalid_inputs_are_refused(self):
        for k, n in ((7, 6), (-1, 6), (0, 0)):
            with self.assertRaises(ValueError):
                st.wilson(k, n)


class TIntervalTests(unittest.TestCase):
    def test_df_five_uses_the_t_quantile_2_5706(self):
        out = st.t_interval([6, 7, 7, 8, 6, 7])
        self.assertEqual(out["df"], 5)
        self.assertAlmostEqual(out["mean"], 41 / 6)
        sem = (sum((v - out["mean"]) ** 2 for v in out["values"]) / 5) ** 0.5 / math.sqrt(6)
        self.assertAlmostEqual(out["ci95"][1] - out["mean"], 2.5706 * sem, places=6)
        self.assertEqual((out["min"], out["max"]), (6, 8))

    def test_a_single_value_has_no_interval(self):
        with self.assertRaises(ValueError):
            st.t_interval([5])


class SignFlipTests(unittest.TestCase):
    def test_the_reference_distribution_has_sixty_four_assignments(self):
        self.assertEqual(len(st.sign_flip_distribution([1, 2, 3, 4, 5, 6])), 64)

    def test_the_smallest_attainable_two_sided_p_is_2_over_64(self):
        out = st.sign_flip([1, 2, 3, 4, 5, 6])
        self.assertEqual(out["extreme"], 2)
        self.assertEqual(out["p_two_sided"], 0.03125)
        self.assertEqual(out["min_attainable_p"], 0.03125)
        self.assertTrue(out["all_same_sign"])

    def test_the_next_most_extreme_outcome_gives_4_over_64(self):
        out = st.sign_flip([10, 20, 30, 40, 50, -1])
        self.assertEqual(out["p_two_sided"], 0.0625)
        self.assertFalse(out["all_same_sign"])

    def test_below_five_percent_only_if_all_six_differences_share_a_sign(self):
        for diffs in ([5, 4, 3, 2, 1, -1], [3, 3, 3, 3, -3, 3], [1, -2, 3, -4, 5, 6]):
            self.assertGreaterEqual(st.sign_flip(diffs)["p_two_sided"], 0.05, diffs)
        self.assertLess(st.sign_flip([1, 1, 2, 3, 3, 1])["p_two_sided"], 0.05)

    def test_a_negative_all_same_sign_outcome_is_equally_extreme(self):
        self.assertEqual(st.sign_flip([-1, -2, -3, -4, -5, -6])["p_two_sided"], 0.03125)

    def test_ties_make_the_p_larger_not_smaller(self):
        self.assertGreater(st.sign_flip([0, 0, 0, 0, 0, 0])["p_two_sided"], 0.99)

    def test_a_contrast_with_a_dropped_block_says_how_many_blocks_it_used(self):
        out = st.paired_contrast([2, 1, None, 3, 1, 2])
        self.assertEqual((out["blocks_planned"], out["blocks_used"]), (6, 5))
        self.assertEqual(out["sign_flip"]["assignments"], 32)
        self.assertEqual(out["sign_flip"]["min_attainable_p"], 2 / 32)

    def test_no_unpaired_924_arrangement_test_exists(self):
        self.assertFalse(any("permutation" in n.lower() or "unpaired" in n.lower() for n in dir(st)))


class EventBoundTests(unittest.TestCase):
    def test_zero_of_six_repetitions_is_45_9_percent(self):
        self.assertAlmostEqual(st.clopper_pearson_upper(0, 6), 0.459, places=3)
        out = st.event_bound(0, 6, unit="repetition")
        self.assertAlmostEqual(out["upper_95_two_sided"], 0.459, places=3)
        self.assertIn("independent", out["conditional_on"])

    def test_per_model_and_arm_zero_of_six_is_a_conditional_illustration(self):
        out = st.event_bound(0, 6, unit="series_of_one_model_arm")
        self.assertAlmostEqual(out["upper_95_two_sided"], 0.459, places=3)
        self.assertIn("conditional illustration", out["conditional_on"])

    def test_the_goal_attempt_bound_is_labelled_as_the_independent_trial_model(self):
        out = st.event_bound(0, 66, unit="goal_attempt")
        self.assertAlmostEqual(out["upper_95_two_sided"], 0.054, places=3)
        self.assertIn("independent trials", out["conditional_on"])

    def test_the_pooled_zero_of_eighteen_bound_is_refused(self):
        with self.assertRaises(st.PooledBoundRefused):
            st.event_bound(0, 18, unit="series_of_one_model_arm")
        with self.assertRaises(st.PooledBoundRefused) as raised:
            st.event_bound(0, 18, unit="pooled_models")
        self.assertIn("not pooled", str(raised.exception))
        with self.assertRaises(ValueError):
            st.event_bound(0, 6, unit="something_else")
        with self.assertRaises(st.PooledBoundRefused):
            st.event_bound(0, 18, unit="repetition")
        with self.assertRaises(st.PooledBoundRefused):
            st.event_bound(0, 67, unit="goal_attempt")

    def test_the_exact_endpoint_for_positive_counts_matches_the_binomial_definition(self):
        upper = st.clopper_pearson_upper(1, 6)
        cdf = sum(math.comb(6, i) * upper ** i * (1 - upper) ** (6 - i) for i in range(2))
        self.assertAlmostEqual(cdf, 0.025, places=6)
        self.assertEqual(st.clopper_pearson_upper(6, 6), 1.0)

    def test_the_nominal_pooled_endpoint_is_18_5_percent_but_never_produced(self):
        self.assertAlmostEqual(st.clopper_pearson_upper(0, 18), 0.185, places=3)  # the arithmetic exists ...
        with self.assertRaises(st.PooledBoundRefused):                            # ... the helper will not claim it
            st.event_bound(0, 18, unit="pooled_models")


class CounterexampleTests(unittest.TestCase):
    def test_correlated_series_zero_events_in_66_goal_attempts_has_probability_0_8_to_the_6(self):
        out = st.correlated_series_counterexample()
        self.assertAlmostEqual(out["p_zero_events"], 0.8 ** 6)
        self.assertAlmostEqual(out["p_zero_events"], 0.262, places=3)
        self.assertAlmostEqual(out["independent_trial_bound_0_of_66"], 0.054, places=3)
        # 5.4% would be claimed as a 95% bound, yet zero events is a 26% outcome when the true p is 0.2
        self.assertGreater(out["p_zero_events"], 0.05)

    def test_cross_model_correlated_repetitions_would_be_misled_by_a_pooled_bound(self):
        out = st.cross_model_counterexample()
        self.assertAlmostEqual(out["p_zero_events"], 0.7 ** 6)
        self.assertAlmostEqual(out["p_zero_events"], 0.118, places=3)
        self.assertAlmostEqual(out["pooled_endpoint_0_of_18"], 0.185, places=3)
        self.assertTrue(out["excludes_the_true_probability"])  # 18.5% < 30%

    def test_median_and_range(self):
        out = st.median_and_range([5, 1, 3])
        self.assertEqual((out["median"], out["min"], out["max"], out["n"]), (3, 1, 5, 3))


if __name__ == "__main__":
    unittest.main()
