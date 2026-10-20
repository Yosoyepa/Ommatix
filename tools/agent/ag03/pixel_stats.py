#!/usr/bin/env python3
"""AG-03 / AG-02R statistics helpers (ADR-0038 decision 17), standard library only.

Every interval and test here has a stated estimand and a stated assumption, and none is presented as
unconditional:

* ``wilson`` - the Wilson 95% interval of one goal's success probability, *assuming the repetitions of
  a goal are independent runs of one process* (descriptive);
* ``t_interval`` - the 95% t interval of a series-score mean (df = n - 1; wide with n = 6; descriptive);
* ``sign_flip`` - the paired sign-flip randomisation test over the pre-registered blocks: under the null
  that the two labels are exchangeable inside a block, all 2^n sign assignments are equally likely; the
  smallest attainable two-sided p is 2 / 2^n (0.03125 for n = 6, absent ties);
* ``clopper_pearson_upper`` / ``event_bound`` - exact upper bounds for events, **conditional** on the
  stated independence. The pooled 0-of-18 bound is refused (``PooledBoundRefused``): 18 series are not
  18 independent draws. The countermodels of the ADR are functions here so that a test reproduces them.
"""

from __future__ import annotations

import itertools
import math
import statistics

# t(0.975, df) for df = 1..30; beyond that the normal 1.96 is within 0.04 and is not used for n = 6.
T_975 = (12.7062, 4.3027, 3.1824, 2.7764, 2.5706, 2.4469, 2.3646, 2.3060, 2.2622, 2.2281, 2.2010, 2.1788, 2.1604,
         2.1448, 2.1314, 2.1199, 2.1098, 2.1009, 2.0930, 2.0860, 2.0796, 2.0739, 2.0687, 2.0639, 2.0595, 2.0555,
         2.0518, 2.0484, 2.0452, 2.0423)
Z_975 = 1.959964


class PooledBoundRefused(ValueError):
    """A bound that treats dependent series as independent draws is not produced."""


def wilson(k: int, n: int, z: float = Z_975) -> tuple[float, float]:
    """The Wilson score interval of k successes in n trials (n >= 1)."""
    if not 0 <= k <= n or n < 1:
        raise ValueError("need 0 <= k <= n and n >= 1")
    p = k / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return max(0.0, centre - half), min(1.0, centre + half)


def t_interval(values: list[float]) -> dict:
    """List, mean, range and the 95% t interval of the mean (df = n - 1)."""
    n = len(values)
    if n < 2:
        raise ValueError("a t interval needs at least two values")
    if n - 1 > len(T_975):
        raise ValueError("df beyond the table: extend T_975")
    mean = statistics.fmean(values)
    sem = statistics.stdev(values) / math.sqrt(n)
    half = T_975[n - 2] * sem
    return {"values": list(values), "n": n, "mean": mean, "min": min(values), "max": max(values),
            "df": n - 1, "ci95": (mean - half, mean + half),
            "note": "t interval of the mean: wide with this n and descriptive"}


def sign_flip_distribution(diffs: list[float]) -> list[float]:
    """The sums of all 2^n sign assignments of the within-block differences (the reference
    distribution under exchangeable labels). Exact; n is small by design."""
    if not 1 <= len(diffs) <= 20:
        raise ValueError("between 1 and 20 blocks")
    return [sum(s * d for s, d in zip(signs, diffs)) for signs in itertools.product((1, -1), repeat=len(diffs))]


def sign_flip(diffs: list[float]) -> dict:
    """The paired sign-flip randomisation test, two-sided: the share of the 2^n assignments whose
    |sum| is at least the observed |sum|. Floating point ties are decided with a relative tolerance."""
    dist = sign_flip_distribution(diffs)
    observed = abs(sum(diffs))
    tol = 1e-9 * max(1.0, observed)
    extreme = sum(1 for s in dist if abs(s) >= observed - tol)
    n = len(diffs)
    return {"n_blocks": n, "assignments": len(dist), "extreme": extreme, "p_two_sided": extreme / len(dist),
            "min_attainable_p": 2 / 2 ** n,
            "all_same_sign": all(d > 0 for d in diffs) or all(d < 0 for d in diffs)}


def paired_contrast(diffs: list[float | None]) -> dict:
    """A paired contrast over the blocks that exist: ``None`` is a block dropped because a series was
    incomplete (decision 19); the contrast then has fewer blocks and says so."""
    used = [d for d in diffs if d is not None]
    out = {"blocks_planned": len(diffs), "blocks_used": len(used), "diffs": used}
    if len(used) >= 2:
        out |= {"mean_diff": statistics.fmean(used), "t_ci95": t_interval(used)["ci95"], "sign_flip": sign_flip(used)}
    return out


def clopper_pearson_upper(k: int, n: int, alpha: float = 0.05) -> float:
    """The exact upper endpoint of the two-sided (1 - alpha) interval for k events in n trials."""
    if not 0 <= k <= n or n < 1:
        raise ValueError("need 0 <= k <= n and n >= 1")
    if k == n:
        return 1.0
    if k == 0:
        return 1 - (alpha / 2) ** (1 / n)

    def cdf(p: float) -> float:  # P(X <= k) for X ~ Binomial(n, p)
        return sum(math.comb(n, i) * p ** i * (1 - p) ** (n - i) for i in range(k + 1))
    lo, hi = k / n, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if cdf(mid) > alpha / 2 else (lo, mid)
    return (lo + hi) / 2


def event_bound(k: int, n: int, *, unit: str, repetitions: int = 6, goals: int = 11) -> dict:
    """An upper bound on an event probability, with the model it is conditional on stated. Units:

    ``repetition`` - n repetitions, an event in any series run in the repetition (the primary statement);
    ``series_of_one_model_arm`` - at most ``repetitions`` series of one model and arm, assumed independent;
    ``goal_attempt`` - at most ``goals * repetitions`` goal-attempts of one model and arm, assumed independent.

    ``pooled_models`` (the three models' series as independent draws) is never produced, and neither is
    any ``n`` larger than the unit's own bound (0 of 18 series, say)."""
    limits = {"repetition": repetitions, "series_of_one_model_arm": repetitions,
              "goal_attempt": goals * repetitions}
    conditions = {"repetition": "repetitions are independent draws; the statement is about a repetition, not a model",
                  "series_of_one_model_arm": "that model's series are independent (a conditional illustration)",
                  "goal_attempt": "goal-attempts are independent trials (the independent-trial model)"}
    if unit == "pooled_models":
        raise PooledBoundRefused("the three models are not pooled as independent draws: 18 series are not 18 trials")
    if unit not in limits:
        raise ValueError(f"unknown unit {unit!r}")
    if n > limits[unit]:
        raise PooledBoundRefused(f"n = {n} exceeds the {limits[unit]} units of '{unit}': that would pool dependent series")
    return {"unit": unit, "k": k, "n": n, "upper_95_two_sided": clopper_pearson_upper(k, n),
            "conditional_on": conditions[unit]}


def prob_zero_events_when_shared(p_event: float, independent_units: int) -> float:
    """Countermodels of the ADR: when the events of a unit share one cause of probability ``p_event``,
    the chance of zero events across ``independent_units`` independent units is (1 - p)^units."""
    return (1 - p_event) ** independent_units


def correlated_series_counterexample() -> dict:
    """Six independent series whose eleven goals share one event of probability 0.2: zero events over the
    66 goal-attempts has probability 0.8^6, so the independent-trial 5.4% is not a 95% bound."""
    return {"p_zero_events": prob_zero_events_when_shared(0.2, 6),
            "independent_trial_bound_0_of_66": clopper_pearson_upper(0, 66)}


def cross_model_counterexample() -> dict:
    """Six repetitions in which the three models' series share one event of probability 0.3: zero pooled
    events has probability 0.7^6, and a pooled 0-of-18 endpoint (18.5%) would exclude the true 0.3."""
    pooled = clopper_pearson_upper(0, 18)
    return {"p_zero_events": prob_zero_events_when_shared(0.3, 6), "pooled_endpoint_0_of_18": pooled,
            "excludes_the_true_probability": pooled < 0.3}


def median_and_range(values: list[float]) -> dict:
    """Tokens, wall time and steps are reported as median and range over the repetitions."""
    if not values:
        raise ValueError("no values")
    return {"median": statistics.median(values), "min": min(values), "max": max(values), "n": len(values)}
