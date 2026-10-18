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


def summarise(results: list[dict]) -> dict:
    by_goal: dict = {}
    phases: dict = {}
    for r in results:
        phase = str(r["phase"])
        entry = by_goal.setdefault(r["goal"], {}).setdefault(phase, {
            "runs": 0, "success": 0, "false_success": 0, "aborted": 0, "tokens": [], "cost_usd": [],
            "live_spent": 0})
        total = phases.setdefault(phase, {
            "attempted": 0, "completed": 0, "aborted": 0, "success": 0, "false_success_claims": 0,
            "abstention_cases": 0, "abstention_correct": 0, "live_actions_spent": 0, "delivered": 0,
            "proxy_refusals": {}, "engine_error_codes": {}, "statuses": {}, "safety_events": 0,
            "tokens_per_goal": [], "tokens_per_message": [], "tool_ms": {}, "wall_s": []})
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
        tokens = r.get("metrics", {}).get("tokens", {})
        entry["tokens"].append(tokens.get("input_tokens", 0) + tokens.get("output_tokens", 0))
        if isinstance(r.get("metrics", {}).get("cost_usd"), (int, float)):
            entry["cost_usd"].append(r["metrics"]["cost_usd"])
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
        total["tokens_per_goal"].append(entry["tokens"][-1])
        for message in r.get("metrics", {}).get("tokens_per_message", []):
            total["tokens_per_message"].append((message.get("input") or 0) + (message.get("output") or 0))
        for step in r.get("metrics", {}).get("step_ms", []):
            total["tool_ms"].setdefault(step["tool"], []).append(step["ms"])
        if isinstance(r.get("metrics", {}).get("wall_s"), (int, float)):
            total["wall_s"].append(r["metrics"]["wall_s"])
    for total in phases.values():
        goal_tokens, message_tokens = total.pop("tokens_per_goal"), total.pop("tokens_per_message")
        total["tokens_per_goal_median"] = statistics.median(goal_tokens) if goal_tokens else None
        total["tokens_per_message_median"] = statistics.median(message_tokens) if message_tokens else None
        total["tool_latency_ms"] = {
            tool: {"n": len(v), "median": statistics.median(v), "p95": _percentile(v, 0.95)}
            for tool, v in sorted(total.pop("tool_ms").items())}
        wall = total.pop("wall_s")
        total["wall_s_median"] = statistics.median(wall) if wall else None
    for phase_map in by_goal.values():
        for entry in phase_map.values():
            tokens = entry.pop("tokens")
            entry["tokens_median"] = statistics.median(tokens) if tokens else None
            cost = entry.pop("cost_usd")
            entry["cost_usd_total"] = round(sum(cost), 4) if cost else None
    return {"goals": dict(sorted(by_goal.items())), "phases": dict(sorted(phases.items()))}


def render(summary: dict) -> str:
    lines = ["AG-02 summary", ""]
    lines.append(f"{'goal':<6}{'phase':<7}{'runs':<6}{'success':<9}{'false':<7}{'aborted':<9}"
                 f"{'live':<6}{'tokens(med)':<12}")
    for goal, phases in summary["goals"].items():
        for phase, e in sorted(phases.items()):
            lines.append(f"{goal:<6}{phase:<7}{e['runs']:<6}{e['success']:<9}{e['false_success']:<7}"
                         f"{e['aborted']:<9}{e['live_spent']:<6}{str(e['tokens_median']):<12}")
    for phase, t in summary["phases"].items():
        lines += ["", f"phase {phase}: {t['success']}/{t['attempted']} attempts succeeded "
                      f"({t['completed']} completed, {t['aborted']} aborted); "
                      f"false-success claims {t['false_success_claims']}; "
                      f"abstention correct {t['abstention_correct']}/{t['abstention_cases']}; "
                      f"live actions spent {t['live_actions_spent']}; delivered {t['delivered']}; "
                      f"safety events {t['safety_events']}",
                  f"  refusals by the proxy: {t['proxy_refusals'] or 'none'}",
                  f"  engine error codes: {t['engine_error_codes'] or 'none'}",
                  f"  tokens per goal (median) {t['tokens_per_goal_median']}, per message (median) "
                  f"{t['tokens_per_message_median']}, wall s (median) {t['wall_s_median']}"]
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
