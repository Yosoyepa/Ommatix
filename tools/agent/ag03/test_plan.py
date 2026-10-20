"""AG-03 / AG-02R plan (ADR-0038 decisions 13 and 14): the goal, model and arm orders of chapter 09
section 9.12.2 reproduce from their rules, and a series whose fingerprint differs is not pooled."""

import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_plan as plan  # noqa: E402

GOAL_TABLE = {
    1: "G11 G05 G06 G04 G02 G03 G08 G10 G01 G09 G07", 2: "G04 G05 G08 G06 G03 G07 G02 G01 G10 G09 G11",
    3: "G05 G06 G02 G04 G09 G10 G01 G03 G11 G08 G07", 4: "G05 G10 G03 G07 G08 G06 G11 G09 G04 G01 G02",
    5: "G08 G09 G11 G06 G04 G05 G03 G01 G07 G02 G10", 6: "G04 G03 G08 G05 G06 G09 G10 G07 G11 G02 G01"}
MODEL_TABLE = {
    1: (("DeepSeek", "Haiku", "MiMo"), {"Haiku": ("AG-03", "AG-02R"), "DeepSeek": ("AG-02R", "AG-03"), "MiMo": ("AG-03", "AG-02R")}),
    2: (("DeepSeek", "Haiku", "MiMo"), {"Haiku": ("AG-03", "AG-02R"), "DeepSeek": ("AG-02R", "AG-03"), "MiMo": ("AG-02R", "AG-03")}),
    3: (("Haiku", "DeepSeek", "MiMo"), {"Haiku": ("AG-03", "AG-02R"), "DeepSeek": ("AG-03", "AG-02R"), "MiMo": ("AG-03", "AG-02R")}),
    4: (("DeepSeek", "Haiku", "MiMo"), {"Haiku": ("AG-03", "AG-02R"), "DeepSeek": ("AG-02R", "AG-03"), "MiMo": ("AG-03", "AG-02R")}),
    5: (("DeepSeek", "MiMo", "Haiku"), {"Haiku": ("AG-02R", "AG-03"), "DeepSeek": ("AG-02R", "AG-03"), "MiMo": ("AG-03", "AG-02R")}),
    6: (("Haiku", "MiMo", "DeepSeek"), {"Haiku": ("AG-03", "AG-02R"), "DeepSeek": ("AG-02R", "AG-03"), "MiMo": ("AG-02R", "AG-03")})}


def good_fingerprint(**changes):
    base = {"engine_sha256_prefix": "332dfa0ca093334e", "testapp_sha256_prefix": "ca6e1ec7c5c7ee1b",
            "editor_sha256_prefix": "ea67a7aae6f5a050", "goals_sha256": "c280652a" + "0" * 56,
            "phase_config_sha256": "ab" * 32, "claude_code_version": "2.1.304", "requested_model": "claude-haiku-5-5",
            "resolved_model": "claude-haiku-5-5", "route_host": "api.anthropic.com", "effort": "max",
            "proof_schema": "ag03-isolation-proof-1", "proof_verifier_revision": 1, "machine": "owner-pc",
            "monitor_resolution": "2560x1440", "monitor_scaling": "100%", "caps": "39/40/300/30/60",
            "capture_backend": "printwindow", "instruction_sha256": "24982efa", "session_budget": 40,
            "proof_date_utc": "2026-10-20T10:00:00Z", "started_utc": "2026-10-20T10:05:00Z"}
    return base | changes


class OrderTests(unittest.TestCase):
    def test_the_goal_orders_of_the_dossier_reproduce_from_the_sha256_rule(self):
        for rep, row in GOAL_TABLE.items():
            self.assertEqual(" ".join(plan.goal_order(rep)), row, f"repetition {rep}")

    def test_the_goal_order_is_different_between_repetitions_and_a_permutation_of_the_goals(self):
        orders = [tuple(plan.goal_order(r)) for r in range(1, 7)]
        self.assertEqual(len(set(orders)), 6)
        self.assertTrue(all(sorted(o) == list(plan.GOAL_IDS) for o in orders))

    def test_the_model_and_arm_orders_of_the_dossier_reproduce(self):
        for rep, (models, arms) in MODEL_TABLE.items():
            self.assertEqual(tuple(plan.model_order(rep)), models, f"repetition {rep}")
            for model, order in arms.items():
                self.assertEqual(tuple(plan.arm_order(rep, model)), order, f"repetition {rep} {model}")

    def test_any_further_repetition_is_reproducible_from_the_rule(self):
        self.assertEqual(plan.goal_order(7), plan.goal_order(7))
        self.assertEqual(sorted(plan.goal_order(7)), list(plan.GOAL_IDS))

    def test_a_model_excluded_at_its_gate_is_removed_and_a_block_loses_its_ag03_arm(self):
        rows = plan.schedule(excluded={"MiMo": ("AG-03",)})
        mimo = [r for r in rows if r["model"] == "MiMo"]
        self.assertTrue(all(r["arms"] == ["AG-02R"] for r in mimo))
        without = plan.schedule(models=("Haiku", "DeepSeek"))
        self.assertFalse(any(r["model"] == "MiMo" for r in without))
        self.assertEqual([r["model"] for r in without if r["rep"] == 1], ["DeepSeek", "Haiku"])

    def test_blocks_are_interleaved_by_repetition_and_share_the_goal_order(self):
        rows = plan.schedule()
        self.assertEqual([r["rep"] for r in rows], [r for r in range(1, 7) for _ in range(3)])
        for rep in range(1, 7):
            goals = {tuple(r["goals"]) for r in rows if r["rep"] == rep}
            self.assertEqual(len(goals), 1)


class FingerprintTests(unittest.TestCase):
    def test_an_identical_fingerprint_is_pooled_even_though_the_start_time_and_proof_date_differ(self):
        campaign = plan.Campaign(good_fingerprint())
        self.assertTrue(campaign.add("s1", good_fingerprint(started_utc="2026-10-21T09:00:00Z",
                                                            proof_date_utc="2026-10-21T08:50:00Z")))
        self.assertEqual(campaign.not_pooled, [])

    def test_every_pinned_key_alone_keeps_a_series_out_of_the_pool(self):
        for key in plan.FINGERPRINT_KEYS:
            campaign = plan.Campaign(good_fingerprint())
            with self.subTest(key=key):
                self.assertFalse(campaign.add("s", good_fingerprint(**{key: "changed"})))
                self.assertEqual(campaign.not_pooled[-1]["differs_in"], [key])

    def test_a_claude_code_update_starts_a_new_pool(self):
        campaign = plan.Campaign(good_fingerprint())
        self.assertFalse(campaign.add("after-update", good_fingerprint(claude_code_version="2.1.305")))
        with self.assertRaises(plan.FingerprintMismatch) as raised:
            campaign.require("again", good_fingerprint(claude_code_version="2.1.305"))
        self.assertEqual(raised.exception.keys, ["claude_code_version"])

    def test_a_fingerprint_missing_a_key_is_not_pooled_and_cannot_start_a_campaign(self):
        incomplete = good_fingerprint()
        del incomplete["capture_backend"]
        self.assertFalse(plan.Campaign(good_fingerprint()).add("s", incomplete))
        with self.assertRaises(plan.FingerprintMismatch):
            plan.Campaign(incomplete)

    def test_the_start_time_and_the_proof_date_are_recorded_but_never_compared(self):
        self.assertEqual(set(plan.RECORDED_ONLY), {"proof_date_utc", "started_utc"})
        self.assertTrue(set(plan.RECORDED_ONLY).isdisjoint(plan.FINGERPRINT_KEYS))
        self.assertFalse(plan.differences(good_fingerprint(), good_fingerprint(started_utc="x", proof_date_utc="y")))

    def test_the_capture_backend_and_the_instruction_are_part_of_the_fingerprint(self):
        self.assertIn("capture_backend", plan.FINGERPRINT_KEYS)
        self.assertIn("instruction_sha256", plan.FINGERPRINT_KEYS)
        self.assertIn("goals_sha256", plan.FINGERPRINT_KEYS)


if __name__ == "__main__":
    unittest.main()
