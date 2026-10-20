#!/usr/bin/env python3
"""AG-03 / AG-02R slots, retries and stopped campaigns (ADR-0038 decision 19), the same in both arms.

A *slot* is (model, arm, phase, repetition, goal): 11 per series, 6 series per (model, arm, phase). Its
**primary outcome** is that of its **first valid attempt**. An attempt is *invalid* only when it ended
through a harness fault **before the agent's first model call** (an application that did not open, a
proof that did not load). A capture refusal, a gate refusal or a model-gate exclusion is never a fault to
retry around. An invalid attempt is retried **once, inside the same slot**; both records are kept; a
second invalid attempt makes the slot a *technical failure* (a non-success in the denominator, also
reported on its own line). After the first model call nothing is retried. ``n`` counts primary slots; the
raw attempts and the technical failures are reported beside them. On a campaign stop, the slots not yet
attempted are *not attempted*: not failures, not in n; a series with any is *incomplete*, is excluded from
the series-score contrasts and its block drops out of them (the contrast then has fewer than six blocks).
"""

from __future__ import annotations

import sys
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE)]

import pixel_plan as plan  # noqa: E402
import pixel_stats as stats  # noqa: E402

SlotKey = tuple  # (model, arm, phase, rep, goal)


@dataclass(frozen=True)
class Attempt:
    """What one attempt of a slot came to. Booleans and closed facts only."""
    model_called: bool = True               # did the agent make its first model call?
    harness_fault: bool = False             # the harness failed (app did not open, proof did not load)
    success: bool = False
    false_success: bool = False
    safety_event: bool = False              # a harness-guard violation: ends the campaign
    operator_stop: bool = False             # the operator's stop file: ends the campaign
    capture_refused: bool = False           # an outcome, never a reason to retry
    gate_refused: bool = False              # likewise
    excluded_model: bool = False            # likewise
    harness_halt: bool = False              # the harness cannot go on safely (its process tree is not verified gone)

    @property
    def invalid(self) -> bool:
        return (self.harness_fault and not self.model_called and not self.capture_refused
                and not self.gate_refused and not self.excluded_model)

    @property
    def ends_campaign(self) -> bool:
        return self.safety_event or self.operator_stop or self.harness_halt


@dataclass
class Slot:
    key: SlotKey
    attempts: list[Attempt] = field(default_factory=list)
    attempted: bool = False
    interrupted: bool = False  # an invalid attempt ended the campaign: no retry, no primary outcome, not in n

    @property
    def primary(self) -> Attempt | None:
        """The first valid attempt; None for a technical failure or a slot never attempted."""
        return next((a for a in self.attempts if not a.invalid), None)

    @property
    def technical_failure(self) -> bool:
        return self.attempted and self.primary is None and not self.interrupted

    @property
    def success(self) -> bool:
        return bool(self.primary and self.primary.success)


def execute_slot(key: SlotKey, run_attempt) -> Slot:
    """Runs a slot under the retry rule: one retry, inside the slot, only after an invalid attempt - and never
    after an attempt that ended the campaign (an operator stop or a safety event takes precedence over the one
    permitted retry; review of PR 101, R6): the slot is then *interrupted*, its raw record is kept."""
    slot = Slot(key=key, attempted=True)
    for _ in range(2):
        attempt = run_attempt(key)
        slot.attempts.append(attempt)
        if attempt.ends_campaign:
            slot.interrupted = attempt.invalid or not attempt.model_called  # ended before the model was asked
            break
        if not attempt.invalid:
            break
    return slot


def run_campaign(keys: list[SlotKey], run_attempt) -> list[Slot]:
    """The slots in order. Once any attempt ends the campaign (a safety event, the operator's stop), every
    later slot is recorded as not attempted."""
    slots, stopped = [], False
    for key in keys:
        if stopped:
            slots.append(Slot(key=key))
            continue
        slot = execute_slot(key, run_attempt)
        slots.append(slot)
        stopped = any(a.ends_campaign for a in slot.attempts)
    return slots


def tally(slots: list[Slot], model: str, arm: str, phase: int, goal: str) -> dict:
    """k of n for one goal: n counts primary slots attempted (a technical failure counts as a
    non-success); raw attempts and technical failures are reported on their own lines."""
    mine = [s for s in slots if s.key[0] == model and s.key[1] == arm and s.key[2] == phase and s.key[4] == goal]
    attempted = [s for s in mine if s.attempted and not s.interrupted]
    n, k = len(attempted), sum(1 for s in attempted if s.success)
    return {"k": k, "n": n, "wilson95": stats.wilson(k, n) if n else None,
            "raw_attempts": sum(len(s.attempts) for s in mine),
            "technical_failures": sum(1 for s in attempted if s.technical_failure),
            "interrupted": sum(1 for s in mine if s.interrupted),
            "not_attempted": sum(1 for s in mine if not s.attempted)}


def series_slots(slots: list[Slot], model: str, arm: str, phase: int, rep: int) -> list[Slot]:
    return [s for s in slots if s.key[:4] == (model, arm, phase, rep)]


def series_complete(slots: list[Slot], model: str, arm: str, phase: int, rep: int) -> bool:
    mine = series_slots(slots, model, arm, phase, rep)
    return len(mine) == len(plan.GOAL_IDS) and all(s.attempted and not s.interrupted for s in mine)


def series_score(slots: list[Slot], model: str, arm: str, phase: int, rep: int) -> int | None:
    """The series score (0 to 11), or None for an incomplete series (excluded from the contrasts)."""
    if not series_complete(slots, model, arm, phase, rep):
        return None
    return sum(1 for s in series_slots(slots, model, arm, phase, rep) if s.success)


def arm_contrast(slots: list[Slot], model: str, phase: int, repetitions: int = plan.REPETITIONS) -> dict:
    """Contrast (i), arm within a model: d_r = score(AG-03, r) - score(AG-02R, r); a block whose either
    series is incomplete is dropped."""
    diffs = []
    for rep in range(1, repetitions + 1):
        a, b = (series_score(slots, model, arm, phase, rep) for arm in ("AG-03", "AG-02R"))
        diffs.append(None if a is None or b is None else a - b)
    return stats.paired_contrast(diffs)


def model_contrast(slots: list[Slot], arm: str, phase: int, first: str, second: str,
                   repetitions: int = plan.REPETITIONS) -> dict:
    """Contrast (ii), model within an arm: d_r = score(first, r) - score(second, r)."""
    diffs = []
    for rep in range(1, repetitions + 1):
        a, b = (series_score(slots, model, arm, phase, rep) for model in (first, second))
        diffs.append(None if a is None or b is None else a - b)
    return stats.paired_contrast(diffs)
