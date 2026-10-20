#!/usr/bin/env python3
"""AG-02 table regeneration: reads the run folders the runner wrote and exports a **non-sensitive
aggregate** of them (PE-09, PE-10 of the paper v0.4 review), so the tables of the dossier
(chapter 09 section 9.11) can be recomputed from the originals and not transcribed.

    tables.py --series "LABEL=DIR[,DIR...]" [--series ...] [--rehearsal DIR ...] [--summary FILE ...] --out OUTDIR

* ``--series`` names a model/route and the folders (run directories, or a folder of them) whose
  ``G??.json`` results and ``G??.proxy.jsonl`` logs belong to it. The label is the operator's: old
  results carry no provenance. A series is split by the phase each result records.
* ``--rehearsal`` marks a folder (also given under a series) whose results are rehearsals: they are
  exported, flagged ``rehearsal``, and kept out of every aggregate. A result the runner aborted is
  flagged ``aborted`` and counted as aborted.
* ``--summary`` adds a ``resumen-*.json`` (``summarise.py --out``) to the manifest, hashed.
* ``--out`` is an absolute path outside any repository checkout (the proxy's rule for operator data).

Output files, all numbers, booleans and closed words, never a label, a query, a typed text or any
path of the operator: ``goals.json/csv`` (a row per result), ``observations.csv`` (a row per
``observe_screen`` request), ``series.json/csv`` (aggregates per series and phase) and
``manifest.json`` (the SHA-256 of every input and output, the script and the thresholds used). Run
twice on the same inputs it writes the same bytes. Dictionary-valued columns are compact JSON in CSV.

The request-size class is **indirect** evidence of whether an observation carried arguments (see
``logstats.py``); logs written since the proxy records ``query_present`` say it directly.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import os
import platform
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import logstats  # noqa: E402
import proxy as proxy_module  # noqa: E402
import summarise  # noqa: E402

SCHEMA = "ag02-tables-1"
LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._ :/+-]{0,63}$")
WORD = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")
TIMESTAMP = re.compile(r"^[0-9T:.+Z-]{1,40}$")
RESULT_NAME = re.compile(r"^G\d\d\.json$")
VERDICTS = ("DONE", "IMPOSSIBLE")
DELIVERY_STATUSES = ("input_delivered", "indeterminate", "effect_observed")
THIS = Path(__file__).resolve()
FILES = ("goals.json", "goals.csv", "observations.csv", "series.json", "series.csv")
OBS_KEYS = ("observe_index", "request_bytes", "request_size_class", "query_present", "limit", "cursor_present",
            "answered", "is_error", "rpc_error_code", "elements", "total_elements", "truncated", "rtt_ms")


def sha256_file(path: Path) -> tuple[str, int]:
    digest, size = hashlib.sha256(), 0
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 16), b""):
            digest.update(chunk)
            size += len(chunk)
    return digest.hexdigest(), size


def _num(value):
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def _bool(value):
    return value if isinstance(value, bool) else None


def _word(value):
    return value if isinstance(value, str) and WORD.match(value) else None


def _counter(items) -> dict:
    out: dict = {}
    for item in items:
        out[item] = out.get(item, 0) + 1
    return dict(sorted(out.items()))


def _merge(into: dict, other: dict) -> None:
    for key, value in other.items():
        into[key] = into.get(key, 0) + value


# ---- reading -----------------------------------------------------------------------------------

def find_results(root: Path) -> list[Path]:
    """Every ``G??.json`` under `root` except anything under a ``scratch`` folder."""
    return [p for p in sorted(root.rglob("G??.json"))
            if RESULT_NAME.match(p.name) and "scratch" not in p.relative_to(root).parts[:-1]]


def read_result(path: Path) -> dict | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    ok = isinstance(data, dict) and isinstance(data.get("goal"), str) and isinstance(data.get("phase"), int) \
        and not isinstance(data.get("phase"), bool)
    return data if ok else None


def log_facts(records: list[dict]) -> dict:
    """What the proxy log says about the engine's answers to actions, refusals and live calls."""
    answers = [r for r in records if r.get("dir") == "engine->agent" and r.get("tool") == "execute_action"]
    return {
        "delivered": _counter(r.get("status") for r in answers if r.get("input_delivered") is True),
        "engine_error_codes": _counter(r["error_code"] for r in answers if isinstance(r.get("error_code"), str)),
        "proxy_refusals": _counter(r["reason"] for r in records
                                   if r.get("dir") == "proxy->agent" and isinstance(r.get("reason"), str)),
        "statuses": _counter(r["status"] for r in answers if isinstance(r.get("status"), str)),
        "live_forwarded": sum(1 for r in records if r.get("dir") == "agent->engine" and r.get("live_counted")),
        "metrics_calls": sum(1 for r in records if r.get("dir") == "agent->engine" and r.get("kind") == "request"
                             and r.get("tool") == "get_engine_metrics"),
        "metrics_answered": sum(1 for r in records if r.get("dir") == "engine->agent"
                                and r.get("tool") == "get_engine_metrics")}


def goal_row(label: str, folder: str, kind: str, result: dict, result_sha: str,
             log_path: Path | None) -> tuple[dict, list[dict]]:
    """The row of one result and the entries of its observations. Only numbers, booleans and words that
    match a closed pattern are taken from the result; its free-text fields are never read."""
    def part(key):
        return result.get(key) if isinstance(result.get(key), dict) else {}
    metrics, score, oracle, provenance = part("metrics"), part("score"), part("oracle"), part("provenance")
    cli, proxy_part = part("cli"), part("proxy")
    calls = metrics.get("tool_calls") if isinstance(metrics.get("tool_calls"), dict) else {}
    figures = summarise.token_figures(metrics) if metrics else {}
    usage = metrics.get("cli_usage") if isinstance(metrics.get("cli_usage"), dict) else {}
    details = usage.get("output_tokens_details") if isinstance(usage.get("output_tokens_details"), dict) else {}
    thinking = _num(details.get("thinking_tokens"))
    steps =[s for s in metrics.get("step_ms", []) if isinstance(s, dict)] \
        if isinstance(metrics.get("step_ms"), list) else []

    def step_ms(tool):
        return [s["ms"] for s in steps if s.get("tool") == tool and _num(s.get("ms")) is not None]

    records, bad, entries, log_sha = [], 0, [], None
    if log_path is not None and log_path.exists():
        records, bad = logstats.read_jsonl(log_path)
        entries = logstats.observations(records)
        log_sha = sha256_file(log_path)[0]
    logged = log_sha is not None
    figs = logstats.observation_figures(entries) if logged else None
    facts = log_facts(records) if logged else None
    loop = logstats.loop_counts(records) if logged else None
    dist = figs["elements_distribution"] if logged else {}
    classes = figs["request_size_classes"] if logged else {}
    small = figs["small_responses_by_request_size_class"] if logged else {}
    verdict = metrics.get("verdict")
    killed = cli.get("killed")
    started = result.get("started_utc")
    row = {
        "series": label, "phase": result["phase"], "goal": result["goal"], "folder": folder, "kind": kind,
        "expected": _word(result.get("expected")),
        "started_utc": started if isinstance(started, str) and TIMESTAMP.match(started) else None,
        "provider": _word(provenance.get("provider")), "model": _word(provenance.get("model")),
        "requested_model": _word(provenance.get("requested_model")),
        "claude_code_version": _word(provenance.get("claude_code_version")),
        "turns": _num(metrics.get("turns")), "wall_s": _num(metrics.get("wall_s")),
        "context_tokens": figures.get("context_tokens"), "output_tokens": figures.get("output_tokens"),
        "thinking_tokens": thinking if figures.get("source") == "cli_usage" else None,
        "token_source": figures.get("source"), "cli_cost_usd": figures.get("cost_usd"),
        "verdict": None if verdict is None else (verdict if verdict in VERDICTS else "other"),
        "score_success": _bool(score.get("success")), "false_success": _bool(score.get("false_success")),
        "missed_success": _bool(score.get("missed_success")),
        "abstention_correct": _bool(score.get("abstention_correct")),
        "oracle_ok": _bool(oracle.get("ok")), "oracle_reason": _word(oracle.get("reason")),
        "safety_event": bool(result.get("safety_event")), "aborted": bool(result.get("aborted")),
        "cli_exit_code": cli.get("exit_code") if isinstance(cli.get("exit_code"), int) else None,
        "cli_killed": None if killed is None else (killed if killed == "timeout" else "other"),
        "live_spent": _num(result.get("live_spent")),
        "result_live_forwarded": _num(proxy_part.get("live_forwarded")),
        "result_delivered": _num(proxy_part.get("delivered")),
        "result_rpc_errors": _num(proxy_part.get("rpc_errors")),
        "proxy_log_complete": _bool(proxy_part.get("complete")),
        "tool_calls_observe": _num(calls.get("observe_screen")),
        "tool_calls_execute": _num(calls.get("execute_action")),
        "tool_calls_metrics": _num(calls.get("get_engine_metrics")),
        "log_live_forwarded": facts["live_forwarded"] if logged else None,
        "log_calls_metrics": facts["metrics_calls"] if logged else None,
        "log_calls_metrics_answered": facts["metrics_answered"] if logged else None,
        "proxy_refusals": facts["proxy_refusals"] if logged else None,
        "engine_error_codes": facts["engine_error_codes"] if logged else None,
        "statuses": facts["statuses"] if logged else None,
        **{f"delivered_{s}": facts["delivered"].get(s, 0) if logged else None for s in DELIVERY_STATUSES},
        "delivered_total": sum(facts["delivered"].values()) if logged else None,
        "observe_requests": figs["requests"] if logged else None,
        "observe_responses": figs["responses"] if logged else None,
        "observe_error_responses": figs["error_responses"] if logged else None,
        "elements_0": dist.get("0", 0) if logged else None, "elements_1": dist.get("1", 0) if logged else None,
        "elements_2": dist.get("2", 0) if logged else None, "elements_3plus": dist.get("3+", 0) if logged else None,
        "elements_3plus_min": figs["elements_3plus_min"] if logged else None,
        "elements_3plus_max": figs["elements_3plus_max"] if logged else None,
        "request_bare": classes.get("bare", 0) if logged else None,
        "request_with_args": classes.get("with_args", 0) if logged else None,
        "request_ambiguous": classes.get("ambiguous", 0) if logged else None,
        "small_with_args_request": small.get("with_args", 0) if logged else None,
        "small_bare_request": small.get("bare", 0) if logged else None,
        "small_ambiguous_request": small.get("ambiguous", 0) if logged else None,
        "args_recorded": figs["request_args_recorded"] if logged else None,
        "observe_rtt_median_ms": figs["rtt_ms"]["median"] if logged else None,
        "observe_rtt_p95_ms": figs["rtt_ms"]["p95"] if logged else None,
        "observe_step_median_ms": logstats.median(step_ms("observe_screen")),
        "observe_step_p95_ms": logstats.percentile(step_ms("observe_screen"), 0.95),
        "execute_step_median_ms": logstats.median(step_ms("execute_action")),
        "actions_to_engine": loop["actions"] if logged else None,
        "actions_after_observation": loop["after_observation"] if logged else None,
        "actions_followed_by_observation": loop["followed_by_observation"] if logged else None,
        "log_unreadable_lines": bad if logged else None,
        "result_sha256": result_sha, "proxy_log_sha256": log_sha,
        "_observe_steps": step_ms("observe_screen"), "_execute_steps": step_ms("execute_action"),
    }
    key = {k: row[k] for k in ("series", "phase", "goal", "folder", "kind")}
    return row, [{**key, **e} for e in entries]


def observation_rows(entries: list[dict]) -> list[dict]:
    """Flatten observation entries into the columns of ``observations.csv``."""
    rows = []
    for e in entries:
        timings = e["timings_us"] if isinstance(e["timings_us"], dict) else {}
        plan = e["plan"] if isinstance(e["plan"], dict) else {}
        row = {k: e[k] for k in ("series", "phase", "goal", "folder", "kind", *OBS_KEYS)}
        row.update({f"t_{name}": timings.get(name) for name in proxy_module.TIMING_FIELDS})
        row.update({"plan_work": plan.get("work"), "plan_full_refresh_reason": plan.get("full_refresh_reason"),
                    **{f"plan_{name}": plan.get(name) for name in proxy_module.PLAN_COUNTS},
                    "plan_input_shapes_n": plan.get("input_shapes_n"),
                    "plan_accessibility_truncated": plan.get("accessibility_truncated")})
        rows.append(row)
    return rows


# ---- aggregates --------------------------------------------------------------------------------

def series_row(label: str, phase: int, rows: list[dict], entries: list[dict]) -> dict:
    """The aggregate of one series and phase over its ``series`` rows (aborted and rehearsal rows are
    counted apart and never enter a denominator)."""
    counted = [r for r in rows if r["kind"] == "series"]
    reachable = [r for r in counted if r["expected"] == "done"]
    impossible = [r for r in counted if r["expected"] == "impossible"]
    figs = logstats.observation_figures([e for e in entries if e["kind"] == "series"])

    def total(key):
        return sum(r[key] for r in counted if isinstance(r.get(key), (int, float)))

    def merged(key):
        out: dict = {}
        for r in counted:
            if isinstance(r.get(key), dict):
                _merge(out, r[key])
        return dict(sorted(out.items()))

    def values(key):
        return [r[key] for r in counted if isinstance(r.get(key), (int, float))]

    outputs = [r["output_tokens"] for r in counted
               if isinstance(r.get("output_tokens"), int) and r.get("token_source") == "cli_usage"]
    goal_counts = _counter(r["goal"] for r in counted)
    observe_steps = [ms for r in counted for ms in r["_observe_steps"]]
    execute_steps = [ms for r in counted for ms in r["_execute_steps"]]
    return {
        "series": label, "phase": phase, "rows": len(rows), "completed": len(counted),
        "aborted": sum(1 for r in rows if r["kind"] == "aborted"),
        "rehearsal_rows": sum(1 for r in rows if r["kind"] == "rehearsal"),
        "duplicate_goal_rows": sum(n - 1 for n in goal_counts.values() if n > 1),
        "score_success": sum(1 for r in counted if r["score_success"]),
        "false_success_claims": sum(1 for r in counted if r["false_success"]),
        "safety_events": sum(1 for r in counted if r["safety_event"]),
        "abstention_cases": sum(1 for r in impossible if r["abstention_correct"] is not None),
        "abstention_correct": sum(1 for r in impossible if r["abstention_correct"]),
        "reachable_n": len(reachable),
        "reachable_score_success": sum(1 for r in reachable if r["score_success"]),
        "reachable_oracle_ok": sum(1 for r in reachable if r["oracle_ok"]) if phase == 2 else None,
        "oracle_ok_all": sum(1 for r in counted if r["oracle_ok"]) if phase == 2 else None,
        "live_spent": total("live_spent"), "delivered_total": total("delivered_total"),
        "log_calls_metrics": total("log_calls_metrics"), "log_calls_metrics_answered": total("log_calls_metrics_answered"),
        "delivered_input_delivered": total("delivered_input_delivered"),
        "delivered_indeterminate": total("delivered_indeterminate"),
        "delivered_effect_observed": total("delivered_effect_observed"),
        "proxy_refusals": merged("proxy_refusals"), "engine_error_codes": merged("engine_error_codes"),
        "statuses": merged("statuses"),
        "goals_with_log": sum(1 for r in counted if r["observe_requests"] is not None),
        "observe_requests": figs["requests"], "observe_responses": figs["responses"],
        "observe_unanswered": figs["unanswered"], "observe_error_responses": figs["error_responses"],
        "elements_distribution": figs["elements_distribution"],
        "elements_3plus_min": figs["elements_3plus_min"], "elements_3plus_max": figs["elements_3plus_max"],
        "request_size_classes": figs["request_size_classes"],
        "small_responses": sum(figs["small_responses_by_request_size_class"].values()),
        "small_responses_by_request_size_class": figs["small_responses_by_request_size_class"],
        "args_recorded": figs["request_args_recorded"], "with_query": figs["with_query"],
        "with_limit": figs["with_limit"], "with_cursor": figs["with_cursor"],
        "truncated_true": figs["truncated_true"], "truncated_false": figs["truncated_false"],
        "truncated_unrecorded": figs["truncated_unrecorded"],
        "observe_rtt_ms": figs["rtt_ms"], "observe_timings_us_median": figs["timings_us_median"],
        "observe_plan_work": figs["plan_work"],
        "observe_step_ms": logstats.quantiles(observe_steps), "execute_step_ms": logstats.quantiles(execute_steps),
        "actions_to_engine": total("actions_to_engine"),
        "actions_after_observation": total("actions_after_observation"),
        "actions_followed_by_observation": total("actions_followed_by_observation"),
        "turns_median": logstats.median(values("turns")),
        "wall_s_sum": round(sum(values("wall_s")), 3), "wall_s_median": logstats.median(values("wall_s")),
        "context_tokens_sum": sum(values("context_tokens")),
        "context_tokens_median": logstats.median(values("context_tokens")),
        "output_tokens_n": len(outputs), "output_tokens_sum": sum(outputs),
        "output_tokens_median": logstats.median(outputs),
        "token_sources": _counter(r["token_source"] for r in counted if r.get("token_source")),
        "cli_cost_usd_sum": round(sum(values("cli_cost_usd")), 6),
    }


# ---- output ------------------------------------------------------------------------------------

def cell(value):
    if isinstance(value, (dict, list)):
        return json.dumps(value, sort_keys=True, separators=(",", ":"))
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    return value


def write_csv(path: Path, rows: list[dict], columns: list[str]) -> None:
    buffer = io.StringIO(newline="")
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(columns)
    for row in rows:
        writer.writerow([cell(row.get(c)) for c in columns])
    path.write_bytes(buffer.getvalue().encode("utf-8"))


def write_json(path: Path, value) -> None:
    path.write_bytes((json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode("utf-8"))


def columns_of(rows: list[dict]) -> list[str]:
    out: list[str] = []
    for row in rows:
        out.extend(key for key in row if key not in out)
    return out


def parse_series(text: str) -> tuple[str, list[Path]]:
    label, _, paths = text.partition("=")
    if not LABEL.match(label) or not paths:
        raise argparse.ArgumentTypeError(f"--series must be LABEL=DIR[,DIR...] with a plain label: {text!r}")
    roots = [Path(p) for p in paths.split(",") if p]
    for root in roots:
        if not root.is_dir():
            raise argparse.ArgumentTypeError(f"--series: not a directory: {root.name!r}")
    return label, roots


def _related(a: Path, b: Path) -> bool:
    a, b = Path(os.path.realpath(a)), Path(os.path.realpath(b))
    return a == b or a in b.parents or b in a.parents


def build(series: list[tuple[str, list[Path]]], rehearsals: list[Path], summaries: list[Path]):
    """Rows, observation entries and the hashed inputs of every series, in the order given."""
    rehearsal_roots = [Path(os.path.realpath(p)) for p in rehearsals]
    rows, entries, inputs, names, seen = [], [], [], set(), set()

    def register(label: str, root: Path, path: Path, role: str) -> None:
        relative = path.relative_to(root).as_posix()
        name = f"{label}/{relative}"
        if name in names:
            name = f"{label}/{root.name}/{relative}"
        if name in names:
            raise SystemExit(f"two inputs have the same name: {name}")
        names.add(name)
        digest, size = sha256_file(path)
        inputs.append({"path": name, "role": role, "sha256": digest, "bytes": size})

    for label, roots in series:
        for root in roots:
            for path in find_results(root):
                real = Path(os.path.realpath(path))
                result = read_result(path)
                if result is None or real in seen:
                    continue
                seen.add(real)
                rehearsal = any(rr == real.parent or rr in real.parents for rr in rehearsal_roots)
                kind = "rehearsal" if rehearsal else ("aborted" if result.get("aborted") else "series")
                log = path.with_name(path.name[:-len(".json")] + ".proxy.jsonl")
                register(label, root, path, "result")
                if log.exists():
                    register(label, root, log, "proxy_log")
                row, found = goal_row(label, path.parent.name, kind, result, sha256_file(path)[0],
                                      log if log.exists() else None)
                rows.append(row)
                entries.extend(found)
    for summary in summaries:
        digest, size = sha256_file(summary)
        inputs.append({"path": f"summary/{summary.name}", "role": "summary", "sha256": digest, "bytes": size})
    return rows, entries, inputs


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("--series", action="append", type=parse_series, required=True, metavar="LABEL=DIR[,DIR]")
    parser.add_argument("--rehearsal", action="append", type=Path, default=[], metavar="DIR")
    parser.add_argument("--summary", action="append", type=Path, default=[], metavar="FILE")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if (problem := proxy_module.path_problem(args.out)):
        parser.error(f"--out: {problem}")
    for path in args.rehearsal:
        if not path.is_dir() or not any(_related(path, root) for _, roots in args.series for root in roots):
            parser.error("--rehearsal must be a folder inside, or containing, a --series folder")
    for path in args.summary:
        if not path.is_file():
            parser.error(f"--summary: not a file: {path.name!r}")

    rows, entries, inputs = build(args.series, args.rehearsal, args.summary)
    if not rows:
        print("no goal results found", file=sys.stderr)
        return 1
    order = {label: i for i, (label, _) in enumerate(args.series)}
    rows.sort(key=lambda r: (order[r["series"]], r["phase"], r["folder"], r["goal"]))
    entries.sort(key=lambda e: (order[e["series"]], e["phase"], e["folder"], e["goal"], e["observe_index"]))
    groups: dict = {}
    for row in rows:
        groups.setdefault((row["series"], row["phase"]), []).append(row)
    series_rows = [series_row(label, phase, group, [e for e in entries if e["series"] == label and e["phase"] == phase])
                   for (label, phase), group in groups.items()]
    for row in rows:
        row.pop("_observe_steps"), row.pop("_execute_steps")

    args.out.mkdir(parents=True, exist_ok=True)
    flat = observation_rows(entries)
    write_json(args.out / "goals.json", rows)
    write_csv(args.out / "goals.csv", rows, columns_of(rows))
    write_csv(args.out / "observations.csv", flat, columns_of(flat) or ["series"])
    write_json(args.out / "series.json", series_rows)
    write_csv(args.out / "series.csv", series_rows, columns_of(series_rows))
    manifest = {
        "schema": SCHEMA,
        "tool": {"script_sha256": sha256_file(THIS)[0], "logstats_sha256": sha256_file(Path(logstats.__file__))[0],
                 "python": platform.python_version()},
        "thresholds": {"request_bare_max_bytes": logstats.BARE_MAX_BYTES,
                       "request_with_args_min_bytes": logstats.WITH_ARGS_MIN_BYTES,
                       "small_response_max_elements": logstats.SMALL_RESPONSE_MAX_ELEMENTS},
        "inputs": sorted(inputs, key=lambda i: i["path"]),
        "outputs": {name: dict(zip(("sha256", "bytes"), sha256_file(args.out / name))) for name in FILES},
    }
    write_json(args.out / "manifest.json", manifest)
    print(f"wrote {len(rows)} goal rows, {len(flat)} observation rows and {len(series_rows)} series rows to "
          f"{args.out.name}/ (manifest.json holds the SHA-256 of every input and output)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
