#!/usr/bin/env python3
"""AG-03 / AG-02R pre-registered orders and the fingerprint rule (ADR-0038 decisions 13 and 14).

Goal order of repetition ``r``: the goals sorted ascending by the SHA-256 of ``AG-02R/rep{r}/{goal}`` -
no random-number generator whose algorithm could change. The same order for every model and both arms,
different between repetitions. Within a repetition the models, and within a block the two arms, are
sorted ascending by the SHA-256 of ``AG-02R/order/rep{r}/{model}`` and
``AG-02R/arm/rep{r}/{model}/{arm}``. A model excluded at its gate is removed from the rows; a block
loses its AG-03 arm. ``test_plan`` reproduces the tables of chapter 09 section 9.12.2 from these rules.

The fingerprint (decision 13) pins what must not change between the series that are pooled: a series
whose fingerprint differs from the campaign's is *not pooled*. ``started_utc`` and the proof's date are
recorded in a series' fingerprint but are not part of the pooling key (they differ for every series by
construction: the route proof is regenerated before each run, and repetitions are spread over days).
"""

from __future__ import annotations

import hashlib

MODELS = ("Haiku", "DeepSeek", "MiMo")
ARMS = ("AG-03", "AG-02R")
GOAL_IDS = tuple(f"G{i:02d}" for i in range(1, 12))
REPETITIONS = 6

# What is fixed and recorded in each series' fingerprint (decision 13).
FINGERPRINT_KEYS = (
    "engine_sha256_prefix", "testapp_sha256_prefix", "editor_sha256_prefix", "goals_sha256", "phase_config_sha256",
    "claude_code_version", "requested_model", "resolved_model", "route_host", "effort", "proof_schema",
    "proof_verifier_revision", "machine", "monitor_resolution", "monitor_scaling", "caps", "capture_backend",
    "instruction_sha256", "session_budget")
# Recorded, never compared when pooling.
RECORDED_ONLY = ("proof_date_utc", "started_utc")


def _key(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def goal_order(rep: int, goals=GOAL_IDS) -> list[str]:
    return sorted(goals, key=lambda goal: _key(f"AG-02R/rep{rep}/{goal}"))


def model_order(rep: int, models=MODELS) -> list[str]:
    return sorted(models, key=lambda model: _key(f"AG-02R/order/rep{rep}/{model}"))


def arm_order(rep: int, model: str, arms=ARMS) -> list[str]:
    return sorted(arms, key=lambda arm: _key(f"AG-02R/arm/rep{rep}/{model}/{arm}"))


def schedule(repetitions: int = REPETITIONS, models=MODELS, excluded=()) -> list[dict]:
    """The blocks in the order they run: all blocks of repetition 1, then 2, ... ``excluded`` maps a
    model name to the arms it lacks (a model that failed its gate has no AG-03 arm)."""
    out = []
    for rep in range(1, repetitions + 1):
        goals = goal_order(rep)
        for model in model_order(rep, models):
            arms = [a for a in arm_order(rep, model) if a not in dict(excluded).get(model, ())]
            out.append({"rep": rep, "model": model, "arms": arms, "goals": goals})
    return out


class FingerprintMismatch(ValueError):
    """A series that does not share the campaign's fingerprint is not pooled (closed ``keys`` list)."""

    def __init__(self, keys):
        super().__init__("series not pooled: " + ", ".join(keys))
        self.keys = list(keys)


def differences(a: dict, b: dict) -> list[str]:
    """The pooling keys in which two fingerprints differ (a missing key differs)."""
    return [k for k in FINGERPRINT_KEYS if a.get(k) != b.get(k) or k not in a or k not in b]


class Campaign:
    """The pool of a campaign: the first fingerprint fixes it; a series with any other is refused."""

    def __init__(self, fingerprint: dict):
        missing = [k for k in FINGERPRINT_KEYS if k not in fingerprint]
        if missing:
            raise FingerprintMismatch(missing)
        self.fingerprint = {k: fingerprint[k] for k in FINGERPRINT_KEYS}
        self.series: list[dict] = []
        self.not_pooled: list[dict] = []

    def add(self, series_id: str, fingerprint: dict) -> bool:
        """Pools a series if its fingerprint equals the campaign's; otherwise records it as not pooled."""
        diff = differences(self.fingerprint, fingerprint)
        if diff:
            self.not_pooled.append({"series": series_id, "differs_in": diff})
            return False
        self.series.append({"series": series_id, "started_utc": fingerprint.get("started_utc")})
        return True

    def require(self, series_id: str, fingerprint: dict) -> None:
        if not self.add(series_id, fingerprint):
            raise FingerprintMismatch(self.not_pooled[-1]["differs_in"])
