#!/usr/bin/env python3
"""AG-03 model gate (ADR-0038 decisions 2 and 6): the records of the gate smoke, offline.

A vision arm exists for a model only if it passes its **gate**: the route preflight of
``pixel_preflight.py`` plus **one real smoke session** in which the pixel server in synthetic mode serves
several contrasting drawn PNGs (a different word and shape each time, never a capture, no engine, no
input) and the model must answer a question that changes with each image. The check is a boolean per
image; the record stores only that, the metadata of decision 5 and closed tokens - never the model's
text. It establishes transport and a few image-dependent answers, **not competence at GUI tasks**.

This module never calls a provider: running the smoke (Step 3b) needs the owner's go-ahead for the
provider call, and the preconditions below. A model that fails either part is *excluded and the
exclusion recorded* (what was tried, the closed reason); it is not worked around, and a campaign
schedule drops its AG-03 arm (``excluded_arms``).
"""

from __future__ import annotations

import re
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_capture as cap  # noqa: E402

GATE_SCHEMA = "ag03-gate-1"
SMOKE_IMAGES = 4
# Pinned (never tuned): the question changes with the image because each image draws another word and shape.
SMOKE_PROMPT = (
    "Call screenshot four times, one call after another. After the fourth, write four lines, one per "
    "screenshot in order, each in exactly this form: `IMAGE n: WORD=<the word drawn in the image> "
    "SHAPE=<square, circle or triangle>`. Then write `VERDICT: DONE`.")
PROVIDERS = ("anthropic", "deepseek", "mimo")
PROVIDER_ERRORS = ("provider_rejected_image", "transport_error", "timeout", "cli_error")
REASONS = ("preflight_failed", "provider_rejected_image", "transport_error", "timeout", "cli_error",
           "smoke_failed", "preconditions_missing")
ANSWER_LINE = re.compile(r"IMAGE\s*(\d+)\s*:\s*WORD\s*=\s*([A-Za-z]+)\s+SHAPE\s*=\s*([A-Za-z]+)", re.I)


def expected_answers(count: int = SMOKE_IMAGES) -> list[tuple[str, str]]:
    """The (word, shape) drawn in screenshot 1..count of a synthetic-mode server."""
    return [(cap.SYNTHETIC_SCENES[i % len(cap.SYNTHETIC_SCENES)][0],
             cap.SYNTHETIC_SCENES[i % len(cap.SYNTHETIC_SCENES)][2]) for i in range(count)]


def per_image_ok(answer: str | None, count: int = SMOKE_IMAGES) -> list[bool]:
    """A boolean per image: the model named the drawn word and shape for that image. The last line given
    for an image number counts; a missing or malformed line is a False."""
    found: dict[int, tuple[str, str]] = {}
    for match in ANSWER_LINE.finditer(answer or ""):
        found[int(match.group(1))] = (match.group(2).upper(), match.group(3).lower())
    return [found.get(n + 1) == (word, shape) for n, (word, shape) in enumerate(expected_answers(count))]


def preconditions_missing(provider: str, preconditions: dict) -> list[str]:
    """Closed tokens for what must be recorded before this provider's smoke may run: the reading of the
    recipient's published data-use pages (OI-2, with URL and date, repeated if a page's date changes)
    and, for Anthropic, the account's plan and model-improvement setting (OI-1)."""
    if provider not in PROVIDERS:
        return ["unknown_provider"]
    missing = []
    if preconditions.get("privacy_reading_recorded") is not True:
        missing.append("privacy_reading_not_recorded")
    if provider == "anthropic" and preconditions.get("oi1_recorded") is not True:
        missing.append("oi1_not_recorded")
    if preconditions.get("owner_go_ahead") is not True:
        missing.append("owner_go_ahead_not_recorded")
    return missing


def gate_record(model: str, provider: str, *, preflight_passed: bool, preconditions: dict,
                answer: str | None = None, provider_error: str | None = None, requested_model: str | None = None,
                resolved_model: str | None = None, route_base_url: str | None = None) -> dict:
    """The record of one model's gate: ``passed``, ``excluded`` (with a closed reason) or ``not_run``
    (preconditions missing, no provider was called). Booleans, counts and closed tokens only."""
    base = {"schema": GATE_SCHEMA, "model": model, "provider": provider if provider in PROVIDERS else "unknown",
            "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "images": SMOKE_IMAGES,
            "preflight_passed": bool(preflight_passed), "requested_model": requested_model,
            "resolved_model": resolved_model, "route_base_url": route_base_url, "scope": "transport and image-dependent answers; "
            "not competence at GUI tasks"}
    if (missing := preconditions_missing(provider, preconditions)):
        return base | {"status": "not_run", "reason": "preconditions_missing", "missing": missing,
                       "provider_called": False}
    if not preflight_passed:
        return base | {"status": "excluded", "reason": "preflight_failed", "provider_called": False}
    if provider_error is not None:
        reason = provider_error if provider_error in PROVIDER_ERRORS else "cli_error"
        return base | {"status": "excluded", "reason": reason, "provider_called": True}
    oks = per_image_ok(answer)
    if all(oks):
        return base | {"status": "passed", "per_image_ok": oks, "provider_called": True}
    return base | {"status": "excluded", "reason": "smoke_failed", "per_image_ok": oks, "provider_called": True}


def excluded_arms(records: list[dict]) -> dict[str, tuple[str, ...]]:
    """The AG-03 arm every model that did not pass its gate lacks, for ``pixel_plan.schedule``."""
    return {r["model"]: ("AG-03",) for r in records if r.get("status") != "passed"}
