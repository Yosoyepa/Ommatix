#!/usr/bin/env python3
"""AG-02 summary: reads the per-goal JSON files of one or more run directories and reports
success by goal and phase, false-success claims (the agent says DONE, the OS oracle says no),
abstention correctness, live actions spent, refusals by code, tokens per step and per goal and
latency. It reads metrics only (the runner never writes screen text) and prints to stdout;
``--out`` writes the same summary as JSON (outside the repository)."""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import proxy as proxy_module  # noqa: E402


def load_results(paths: list[Path]) -> list[dict]:
    results = []
    for path in paths:
        files = sorted(path.glob("G*.json")) if path.is_dir() else [path]
        for file in files:
            if file.name.endswith((".mcp.json",)):
                continue
            try:
                data = json.loads(file.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                continue
            if isinstance(data, dict) and "goal" in data and "phase" in data:
                results.append(data)
    return results


def _percentile(values: list[float], q: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, round(q * (len(ordered) - 1)))]


def _add(counter: dict, key: str, n: int = 1) -> None:
    counter[key] = counter.get(key, 0) + n


def _int(value) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None


def token_figures(metrics: dict) -> dict:
    """The goal's token figures. **Context tokens** are everything the model read: fresh input +
    cache creation + cache read - the CLI's own totals for the session (``cli_usage``, from the final
    ``result`` event). **Output tokens** are the CLI's total, thinking included (the per-message counts
    in the stream are provisional placeholders). ``cost_usd`` is the CLI's. Without ``cli_usage`` the
    stream's per-message sums stand in (``source: "stream"``, output then understated). The old
    non-cached figure (fresh input + output only) is kept as a labelled extra, never as the headline."""
    usage = metrics.get("cli_usage")
    source = "cli_usage"
    if not (isinstance(usage, dict) and _int(usage.get("input_tokens")) is not None
            and _int(usage.get("output_tokens")) is not None):
        usage, source = metrics.get("tokens") or {}, "stream"
    fresh = _int(usage.get("input_tokens")) or 0
    creation = _int(usage.get("cache_creation_input_tokens")) or 0
    read = _int(usage.get("cache_read_input_tokens")) or 0
    output = _int(usage.get("output_tokens")) or 0
    cost = metrics.get("cost_usd")
    return {"context_tokens": fresh + creation + read, "output_tokens": output,
            "uncached_input_plus_output_tokens": fresh + output, "source": source,
            "cost_usd": cost if isinstance(cost, (int, float)) and not isinstance(cost, bool) else None}


def _median(values):
    return statistics.median(values) if values else None


def summarise(results: list[dict]) -> dict:
    by_goal: dict = {}
    phases: dict = {}
    for r in results:
        phase = str(r["phase"])
        entry = by_goal.setdefault(r["goal"], {}).setdefault(phase, {
            "runs": 0, "success": 0, "false_success": 0, "aborted": 0, "context": [], "output": [],
            "uncached": [], "cost_usd": [], "live_spent": 0})
        total = phases.setdefault(phase, {
            "attempted": 0, "completed": 0, "aborted": 0, "success": 0, "false_success_claims": 0,
            "abstention_cases": 0, "abstention_correct": 0, "live_actions_spent": 0, "delivered": 0,
            "proxy_refusals": {}, "engine_error_codes": {}, "statuses": {}, "safety_events": 0,
            "context_per_goal": [], "output_per_goal": [], "uncached_per_goal": [], "cost_usd_values": [],
            "context_per_message": [], "token_sources": {}, "tool_ms": {}, "wall_s": []})
        entry["runs"] += 1
        total["attempted"] += 1  # every attempt stays in the phase's denominator
        if r.get("aborted"):
            entry["aborted"] += 1
            total["aborted"] += 1
            continue
        total["completed"] += 1
        score = r.get("score", {})
        entry["success"] += bool(score.get("success"))
        entry["false_success"] += bool(score.get("false_success"))
        figures = token_figures(r.get("metrics", {}))
        entry["context"].append(figures["context_tokens"])
        entry["output"].append(figures["output_tokens"])
        entry["uncached"].append(figures["uncached_input_plus_output_tokens"])
        if figures["cost_usd"] is not None:
            entry["cost_usd"].append(figures["cost_usd"])
        entry["live_spent"] += r.get("live_spent", 0)

        total["success"] += bool(score.get("success"))
        total["false_success_claims"] += bool(score.get("false_success"))
        if r.get("expected") == "impossible":
            total["abstention_cases"] += 1
            total["abstention_correct"] += bool(score.get("abstention_correct"))
        total["live_actions_spent"] += r.get("live_spent", 0)
        total["safety_events"] += bool(r.get("safety_event"))
        proxy = r.get("proxy", {})
        total["delivered"] += proxy.get("delivered", 0)
        for key, field in (("proxy_refusals", "proxy_refusals"), ("engine_error_codes", "engine_error_codes"),
                           ("statuses", "statuses")):
            for code, n in proxy.get(field, {}).items():
                _add(total[key], code, n)
        total["context_per_goal"].append(figures["context_tokens"])
        total["output_per_goal"].append(figures["output_tokens"])
        total["uncached_per_goal"].append(figures["uncached_input_plus_output_tokens"])
        _add(total["token_sources"], figures["source"])
        if figures["cost_usd"] is not None:
            total["cost_usd_values"].append(figures["cost_usd"])
        for message in r.get("metrics", {}).get("tokens_per_message", []):
            total["context_per_message"].append((_int(message.get("input")) or 0)
                                                + (_int(message.get("cache_creation")) or 0)
                                                + (_int(message.get("cache_read")) or 0))
        for step in r.get("metrics", {}).get("step_ms", []):
            total["tool_ms"].setdefault(step["tool"], []).append(step["ms"])
        if isinstance(r.get("metrics", {}).get("wall_s"), (int, float)):
            total["wall_s"].append(r["metrics"]["wall_s"])
    for total in phases.values():
        context, output = total.pop("context_per_goal"), total.pop("output_per_goal")
        uncached, costs = total.pop("uncached_per_goal"), total.pop("cost_usd_values")
        per_message = total.pop("context_per_message")
        total["context_tokens_total"], total["context_tokens_per_goal_median"] = sum(context), _median(context)
        total["output_tokens_total"], total["output_tokens_per_goal_median"] = sum(output), _median(output)
        total["cost_usd_total"] = round(sum(costs), 8) if costs else None
        total["context_tokens_per_message_median"] = _median(per_message)
        # The old figure (fresh input + output, cached tokens left out), kept only as a labelled extra.
        total["uncached_input_plus_output_tokens_per_goal_median_extra"] = _median(uncached)
        total["tool_latency_ms"] = {
            tool: {"n": len(v), "median": statistics.median(v), "p95": _percentile(v, 0.95)}
            for tool, v in sorted(total.pop("tool_ms").items())}
        wall = total.pop("wall_s")
        total["wall_s_median"] = statistics.median(wall) if wall else None
    for phase_map in by_goal.values():
        for entry in phase_map.values():
            context, output, uncached = entry.pop("context"), entry.pop("output"), entry.pop("uncached")
            entry["context_tokens_median"] = _median(context)
            entry["output_tokens_median"] = _median(output)
            entry["uncached_input_plus_output_tokens_median_extra"] = _median(uncached)
            cost = entry.pop("cost_usd")
            entry["cost_usd_total"] = round(sum(cost), 8) if cost else None
    return {"goals": dict(sorted(by_goal.items())), "phases": dict(sorted(phases.items()))}


def render(summary: dict) -> str:
    lines = ["AG-02 summary", ""]
    lines.append(f"{'goal':<6}{'phase':<7}{'runs':<6}{'success':<9}{'false':<7}{'aborted':<9}"
                 f"{'live':<6}{'context tok(med)':<18}{'output tok(med)':<17}{'cost USD':<10}")
    for goal, phases in summary["goals"].items():
        for phase, e in sorted(phases.items()):
            lines.append(f"{goal:<6}{phase:<7}{e['runs']:<6}{e['success']:<9}{e['false_success']:<7}"
                         f"{e['aborted']:<9}{e['live_spent']:<6}{str(e['context_tokens_median']):<18}"
                         f"{str(e['output_tokens_median']):<17}{str(e['cost_usd_total']):<10}")
    for phase, t in summary["phases"].items():
        lines += ["", f"phase {phase}: {t['success']}/{t['attempted']} attempts succeeded "
                      f"({t['completed']} completed, {t['aborted']} aborted); "
                      f"false-success claims {t['false_success_claims']}; "
                      f"abstention correct {t['abstention_correct']}/{t['abstention_cases']}; "
                      f"live actions spent {t['live_actions_spent']}; delivered {t['delivered']}; "
                      f"safety events {t['safety_events']}",
                  f"  refusals by the proxy: {t['proxy_refusals'] or 'none'}",
                  f"  engine error codes: {t['engine_error_codes'] or 'none'}",
                  f"  context tokens (fresh input + cache creation + cache read): total "
                  f"{t['context_tokens_total']}, per goal (median) {t['context_tokens_per_goal_median']}, "
                  f"per message (median) {t['context_tokens_per_message_median']}",
                  f"  output tokens (thinking included): total {t['output_tokens_total']}, per goal "
                  f"(median) {t['output_tokens_per_goal_median']}; cost USD (the CLI's): "
                  f"{t['cost_usd_total']}; figures from: {t['token_sources']}",
                  f"  [extra, old figure: fresh input + output without cached tokens, per goal "
                  f"(median) {t['uncached_input_plus_output_tokens_per_goal_median_extra']}]",
                  f"  wall s (median) {t['wall_s_median']}"]
        for tool, v in t["tool_latency_ms"].items():
            lines.append(f"  {tool}: n={v['n']} median {v['median']} ms p95 {v['p95']} ms")
    return "\n".join(lines)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("paths", nargs="+", type=Path, help="run directories or goal JSON files")
    parser.add_argument("--out", type=Path, help="write the summary as JSON here")
    args = parser.parse_args(argv)
    if args.out is not None:
        # Resolved once, against the directory this script was started from.
        args.out = Path(os.path.abspath(os.fspath(args.out)))
        if (problem := proxy_module.path_problem(args.out)):
            parser.error(f"--out: {problem}")
    results = load_results(args.paths)
    if not results:
        print("no goal results found", file=sys.stderr)
        return 1
    summary = summarise(results)
    print(render(summary))
    if args.out:
        args.out.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
