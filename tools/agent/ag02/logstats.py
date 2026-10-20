#!/usr/bin/env python3
"""AG-02 proxy-log statistics: the pairing of observation requests with their responses, request-size
classes and the observation loop, shared by ``summarise.py`` and ``tables.py``.

It reads the proxy's metadata log (``<goal>.proxy.jsonl``: closed vocabularies and numbers, see
``proxy.py``) and nothing else; it never sees a label, a query or any screen value. Logs from before
the evidence fields (``query_present``, ``request_index``, ``timings_us``...) are read too: what they do
not hold is reported as unrecorded, never as false.

**Request-size classes are indirect evidence.** A bare ``observe_screen`` request (no arguments) is 542
to 548 bytes in the AG-02 series (Claude Code 2.1.302/2.1.303, any of the three models); one that
carries ``query``, ``limit`` or ``cursor`` is at least 553 bytes. The class is a statement about a
byte count, not about the request's content, and holds only for logs written by that client; logs
written since the evidence fields exist say it directly (``query_present``) and take precedence.
"""

from __future__ import annotations

import json
import statistics
from pathlib import Path

BARE_MAX_BYTES = 548
WITH_ARGS_MIN_BYTES = 553
SMALL_RESPONSE_MAX_ELEMENTS = 2  # "0 to 2 elements": the range the dossier reports as a puzzle


def read_jsonl(path: Path) -> tuple[list[dict], int]:
    """The JSON objects of a log and how many lines were not one (a torn last line, say)."""
    records, bad = [], 0
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return [], 0
    for line in text.splitlines():
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except ValueError:
            bad += 1
            continue
        if isinstance(value, dict):
            records.append(value)
        else:
            bad += 1
    return records, bad


def size_class(size) -> str:
    """``bare`` (<= 548 bytes), ``with_args`` (>= 553) or ``ambiguous`` (between them)."""
    if not isinstance(size, int) or isinstance(size, bool):
        return "unknown"
    if size <= BARE_MAX_BYTES:
        return "bare"
    return "with_args" if size >= WITH_ARGS_MIN_BYTES else "ambiguous"


def percentile(values: list, q: float):
    """The nearest-rank percentile ``summarise.py`` uses, so that p95 figures agree."""
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, round(q * (len(ordered) - 1)))]


def median(values: list):
    return statistics.median(values) if values else None


def quantiles(values: list) -> dict:
    return {"n": len(values), "median": median(values), "p95": percentile(values, 0.95),
            "max": max(values) if values else None}


def observations(records: list[dict]) -> list[dict]:
    """One entry per forwarded ``observe_screen`` request, in order, with its response if it came.

    Requests and responses pair by ``request_index`` when the log has it and otherwise by order
    (the agent waits for each reply, and an unanswered request can only be the last one). The entry
    holds numbers and booleans only; ``None`` means the log did not record it.
    """
    requests = [r for r in records if r.get("dir") == "agent->engine" and r.get("tool") == "observe_screen"
                and r.get("kind") == "request"]
    responses = [r for r in records if r.get("dir") == "engine->agent" and r.get("tool") == "observe_screen"]
    by_index = {r["request_index"]: r for r in responses if isinstance(r.get("request_index"), int)}
    queue = [r for r in responses if not isinstance(r.get("request_index"), int)]
    out = []
    for position, request in enumerate(requests, start=1):
        index = request.get("request_index")
        response = by_index.get(index) if isinstance(index, int) else (queue.pop(0) if queue else None)
        entry = {"observe_index": request.get("observe_index", position),
                 "request_bytes": request.get("bytes"),
                 "request_size_class": size_class(request.get("bytes")),
                 "query_present": request.get("query_present"), "limit": request.get("limit"),
                 "cursor_present": request.get("cursor_present"),
                 "answered": response is not None,
                 "is_error": None, "rpc_error_code": None, "elements": None, "total_elements": None,
                 "truncated": None, "rtt_ms": None, "timings_us": None, "plan": None}
        if response is not None:
            entry["is_error"] = bool(response.get("is_error")) or "rpc_error_code" in response
            entry["rpc_error_code"] = response.get("rpc_error_code")
            entry["elements"] = response.get("elements")
            entry["total_elements"] = response.get("total_elements")
            entry["truncated"] = response.get("truncated")
            entry["rtt_ms"] = response.get("latency_ms")
            entry["timings_us"] = response.get("timings_us")
            entry["plan"] = response.get("plan")
        out.append(entry)
    return out


def element_bucket(count) -> str:
    return "unrecorded" if not isinstance(count, int) else (str(count) if count <= 2 else "3+")


def observation_figures(entries: list[dict]) -> dict:
    """Counts, distributions and quantiles over observation entries (one goal, one series...)."""
    answered = [e for e in entries if e["answered"]]
    counted = [e for e in answered if isinstance(e["elements"], int)]
    buckets: dict[str, int] = {}
    for e in counted:
        key = element_bucket(e["elements"])
        buckets[key] = buckets.get(key, 0) + 1
    large = [e["elements"] for e in counted if e["elements"] > SMALL_RESPONSE_MAX_ELEMENTS]
    classes: dict[str, int] = {}
    small_by_class: dict[str, int] = {}
    for e in entries:
        classes[e["request_size_class"]] = classes.get(e["request_size_class"], 0) + 1
    for e in counted:
        if e["elements"] <= SMALL_RESPONSE_MAX_ELEMENTS:
            small_by_class[e["request_size_class"]] = small_by_class.get(e["request_size_class"], 0) + 1
    recorded = [e for e in entries if e["query_present"] is not None]
    timings = [e["timings_us"] for e in answered if isinstance(e["timings_us"], dict)]
    stage_median = {}
    for name in sorted({k for t in timings for k in t}):
        values = [t[name] for t in timings if isinstance(t.get(name), int)]
        stage_median[name] = median(values)
    works: dict[str, int] = {}
    for e in answered:
        if isinstance(e["plan"], dict):
            works[str(e["plan"].get("work"))] = works.get(str(e["plan"].get("work")), 0) + 1
    rtts = [e["rtt_ms"] for e in answered if isinstance(e["rtt_ms"], (int, float))]
    return {
        "requests": len(entries), "responses": len(answered), "unanswered": len(entries) - len(answered),
        "error_responses": sum(1 for e in answered if e["is_error"]),
        "elements_distribution": dict(sorted(buckets.items())),
        "elements_3plus_min": min(large) if large else None,
        "elements_3plus_max": max(large) if large else None,
        "request_size_classes": dict(sorted(classes.items())),
        "small_responses_by_request_size_class": dict(sorted(small_by_class.items())),
        "request_args_recorded": len(recorded),
        "with_query": sum(1 for e in recorded if e["query_present"]),
        "with_limit": sum(1 for e in recorded if e["limit"] is not None),
        "with_cursor": sum(1 for e in recorded if e["cursor_present"]),
        "truncated_true": sum(1 for e in answered if e["truncated"] is True),
        "truncated_false": sum(1 for e in answered if e["truncated"] is False),
        "truncated_unrecorded": sum(1 for e in answered if e["truncated"] is None),
        "rtt_ms": quantiles(rtts),
        "timings_us_median": stage_median,
        "plan_work": dict(sorted(works.items())),
    }


def loop_counts(records: list[dict]) -> dict:
    """The observation loop of the dossier's method (chapter 09 section 9.11.1): of the actions that
    reached the engine, how many came right after an ``observe_screen`` whose response had **arrived
    without error before the action was sent**, and how many were followed right away by one. Every
    ``tools/call`` of the agent in the log is a call of the sequence, those the proxy refused included.

    The proxy lets an agent send a call while another is pending, so a response is paired with its
    request by ``request_index`` when the log has it, keeping the position at which it arrived; a log
    from before the indices pairs them in order. Either way the response must precede the action."""
    calls = []
    for position, r in enumerate(records):
        if r.get("dir") in ("agent->engine", "agent->proxy") and r.get("method") == "tools/call" \
                and r.get("kind") == "request":
            calls.append({"tool": r.get("tool"), "forwarded": r["dir"] == "agent->engine",
                          "position": position, "request_index": r.get("request_index")})
    by_index, sequential = {}, []  # observation responses: (position, error?) by index, or in order
    for position, r in enumerate(records):
        if r.get("dir") == "engine->agent" and r.get("tool") == "observe_screen":
            answer = (position, bool(r.get("is_error")) or "rpc_error_code" in r)
            index = r.get("request_index")
            if isinstance(index, int) and not isinstance(index, bool):
                by_index[index] = answer
            else:
                sequential.append(answer)
    arrived_at = {}  # request position -> position of its response, when that response was no error
    queue = iter(sequential)
    for call in calls:
        if call["tool"] != "observe_screen" or not call["forwarded"]:
            continue
        index = call["request_index"]
        if isinstance(index, int) and not isinstance(index, bool):
            answer = by_index.get(index)
        else:
            answer = next(queue, None)
        if answer is not None and not answer[1]:
            arrived_at[call["position"]] = answer[0]
    actions = after = followed = 0
    for i, call in enumerate(calls):
        if call["tool"] != "execute_action" or not call["forwarded"]:
            continue
        actions += 1
        previous = calls[i - 1] if i > 0 else None
        if previous is not None and previous["tool"] == "observe_screen" \
                and arrived_at.get(previous["position"], call["position"]) < call["position"]:
            after += 1
        if i + 1 < len(calls) and calls[i + 1]["tool"] == "observe_screen":
            followed += 1
    return {"actions": actions, "after_observation": after, "followed_by_observation": followed}
