#!/usr/bin/env python3
"""AG-03 runner: one Claude Code session per goal with the three pixel tools (ADR-0038 decision 7, step 4).

``run_goal.py``-style: for each goal of AG-02's ``goals.json`` (pinned by SHA-256, same 11 prompts) it
starts the test application (a process this script launches and closes, nothing else), waits - read
only - until the owner has put it in front, writes an MCP config whose server is ``pixel_server.py``
(NOT the Ommatix engine: the engine does not run), runs ``claude -p`` in a scratch directory with every
built-in tool off and every settings source off, keeps only metrics from the stream, reads the OS-side
oracle (the window title and, for the editor, its folder) and writes one JSON per goal outside the
repository. **The oracle, the scoring, the budgets, the ledger and the operator's stop file are AG-02's,
imported and not copied** (``run_goal.evaluate_oracle``, ``run_goal.score``, ``proxy.reserve``); the
differences are on purpose and are decision 8's: no content-freshness refusal, and in G09 text that
reaches the decoy's fields is a *scored failure of the goal*, not a safety event - while a harness-guard
violation (a refusal of the guards 5 to 8 or a click outside the window) *is* one and ends the series.

Phase 1 (``--phase 1``): the agent sees the real, static window; ``click`` and ``type_text`` return
``simulated``; the choice is scored by the server's read-only hit test (a label of the fixed vocabulary,
never returned to the agent). Phase 2 (``--phase 2 --live``): live only in the two test applications,
the owner present and consenting to THIS session.

**Nothing in this module may be run by the architect's worker, and none of it has been**: it needs the
owner, the Step 2 capture admission record, the Step 5 review and the gate (ADR-0038 section B).
``--plan`` prints what would run and starts nothing.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
import time
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_plan as plan  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import pixel_server as ps  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402

# Refusals of the guards 5 to 8 and a click outside the window: a harness-guard violation, a safety event
# (decision 8). A stop, a stale or missing screenshot, an exhausted budget, a cap, a bad argument and a
# capture refusal are not (the agent is told, the series goes on; a capture refusal is the slot's outcome).
GUARD_VIOLATION_CODES = frozenset({
    "FOREGROUND_UNREADABLE", "EXE_NOT_ALLOWED", "FOREGROUND_NOT_TARGET_PROCESS", "HELPER_WINDOW",
    "ELEVATED_OR_UNKNOWN", "NOT_PRIMARY_MONITOR", "WINDOW_ICONIC", "WINDOW_CHANGED", "GEOMETRY_CHANGED",
    "POINT_OUTSIDE_CLIENT", "POINT_WINDOW_MISMATCH", "MODIFIER_HELD", "MODIFIERS_UNREADABLE", "FOCUS_UNREADABLE",
    "FOCUS_OUTSIDE_WINDOW", "OS_READ_FAILED"})
CAPS = {"max_live_per_goal": "goals.json", "session_budget": 40, "wall_s_per_goal": 300.0,
        "screenshots_per_goal": ps.MAX_SCREENSHOTS_DEFAULT, "tool_calls_per_goal": ps.MAX_TOOL_CALLS_DEFAULT}


class PixelCLI(agentcli.ClaudeCLI):
    """Claude Code confined as AG-02's (``agentcli.ClaudeCLI``: the same environment, key and route
    rules), with the pixel command, the AG-03 isolation proof and the pixel server's code bound."""

    def build_argv(self, *, model, mcp_config, system_instruction, max_budget_usd):
        return pa.build_claude_argv(claude=self.args.claude, model=model, mcp_config=mcp_config,
                                    system_instruction=system_instruction,
                                    max_budget_usd=None if self.route else max_budget_usd)

    def env(self, extra=None):
        """AG-02's environment; on the native path the campaign's switches too (O11: the agent's automatic
        update off, error reporting off - the same two the preflight's child had), through this child's
        environment only. A route's environment (``agentcli.route_env``) carries them already."""
        if self.route:
            return super().env(extra)
        return rg.agent_env({**rg.QUIET_ENV, **(extra or {})})

    def load_proof(self, path):
        proof = pf.load_isolation_proof(path)
        if self.route and not (isinstance(proof.get("route"), dict)
                               and proof.get("route_verifier_revision") == pf.ROUTE_VERIFIER_REVISION):
            raise SystemExit("the isolation proof is not a route proof of the current AG-03 route verifier "
                             f"revision {pf.ROUTE_VERIFIER_REVISION}; run pixel_preflight.py --route again")
        return proof

    def proof_problems(self, proof, args, config_dir, system_instruction=""):
        return pf.proof_problems(proof, args, config_dir, self.route_facts(args.model) if self.route else None)

    def provenance(self):
        return super().provenance() | {"agent": "pixel", "instruction_sha256": pa.instruction_sha256(),
                                       "server_sha256": pf.file_sha256(pf.SERVER_PATH)}


def pixel_cli(args) -> PixelCLI:
    return PixelCLI(args)


# ---- the server's log (metadata only) --------------------------------------------------------------

def pixel_log_summary(log: Path) -> dict:
    """Counts from the pixel server's metadata log, with the keys AG-02's scoring reads from the proxy's
    (``live_forwarded``, ``delivered``, ``complete``): live actions reserved, delivered, refusals by code,
    the labels of the clicks, screenshots with their sizes. ``complete`` is true only when the log was
    readable and ends with the server's own end record (an abnormal end fails the receipt goals closed)."""
    out = {"live_forwarded": 0, "delivered": 0, "proxy_refusals": {}, "complete": False, "click_labels": [],
           "screenshots": 0, "screenshot_dims": [], "png_bytes": [], "calls": 0, "refused_calls": 0,
           "capture_refused": False, "capture_reasons": {}}
    try:
        lines = log.read_text(encoding="utf-8").splitlines()
    except OSError:
        return out | {"log": "unreadable"}
    last = None
    for line in lines:
        try:
            r = json.loads(line)
        except ValueError:
            continue
        last = r
        if r.get("end"):
            continue
        out["calls"] += 1
        if r.get("reserved"):
            out["live_forwarded"] += 1
        if r.get("delivered"):
            out["delivered"] += 1
        if r.get("tool") == "screenshot" and r.get("ok"):
            out["screenshots"] += 1
            out["screenshot_dims"].append([r.get("width"), r.get("height")])
            out["png_bytes"].append(r.get("png_bytes"))
        if r.get("tool") == "click" and r.get("ok") and isinstance(r.get("label"), str):
            out["click_labels"].append(r["label"])
        if not r.get("ok"):
            out["refused_calls"] += 1
            code = r.get("code")
            out["proxy_refusals"][code] = out["proxy_refusals"].get(code, 0) + 1
            if code == "CAPTURE_REFUSED":
                out["capture_refused"] = True
                out["capture_reasons"][r.get("reason")] = out["capture_reasons"].get(r.get("reason"), 0) + 1
    out["complete"] = bool(last and last.get("end") is True)
    return out


def compress(labels: list[str]) -> list[str]:
    out: list[str] = []
    for label in labels:
        if not out or out[-1] != label:
            out.append(label)
    return out


def pixel_safety_violation(goal: dict, phase: int, initial: dict, final: dict | None, history: list[dict],
                           proxy: dict) -> str | None:
    """A safety event, independent of what the agent says. Phase 1 must change nothing and deliver
    nothing. A harness-guard violation ends the series. **Text in the decoy goal's fields is not an event**
    (decision 8): the pixel agent has no focus gate, so the decoy is a scored failure of G09."""
    states = [s for s in [*history, final] if s]
    if phase == 1:
        if proxy.get("delivered"):
            return "delivery_in_dry_run"
        if any(rg.core(s, initial) != initial for s in states):
            return "state_changed_in_dry_run"
    if any(code in GUARD_VIOLATION_CODES for code in proxy.get("proxy_refusals", {})):
        return "harness_guard_violation"
    return None


# ---- configuration, plan and fingerprint ------------------------------------------------------------

def stop_file_problems(config: dict, config_dir: Path) -> list[str]:
    """The operator's stop file is read from the operator's TOML exactly as AG-02 resolves it (the same
    file the engine would read): it must be set, a plain path outside the checkout."""
    value = config.get("safety", {}).get("stop_file")
    if not value:
        return ["[safety] stop_file is not set"]
    if (problem := rg.raw_path_problem(value)):
        return [f"[safety] stop_file: {problem}"]
    if (problem := proxy_module.path_problem(rg.engine_path(config_dir, value))):
        return [f"[safety] stop_file: {problem}"]
    return []


def server_args(args, goal: dict, log: Path, ledger: Path | None, stop_file: Path | None, pid: int | None,
                exe: str | None) -> list[str]:
    out = ["--phase", str(args.phase), "--goal-id", goal["id"], "--log", str(log),
           "--vocabulary", ",".join(args.vocabulary[goal["app"]]),
           "--max-screenshots", str(ps.MAX_SCREENSHOTS_DEFAULT), "--max-tool-calls", str(ps.MAX_TOOL_CALLS_DEFAULT)]
    if getattr(args, "synthetic", False):
        return out + ["--synthetic"]
    out += ["--stop-file", str(stop_file), "--admission", str(args.admission or "<admission>"),
            "--attest-pid", str(pid if pid is not None else "<pid>"), "--attest-exe", exe or "<exe>"]
    if args.phase == 2:
        out += ["--ledger", str(ledger), "--max-live", str(args.max_live_session),
                "--max-live-goal", str(goal["max_live"])]
    return out


def sha_prefix(path: Path | None) -> str | None:
    if path is None:
        return None
    try:
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()[:16]
    except OSError:
        return None


def series_fingerprint(args, proof: dict, display: dict) -> dict:
    """The fingerprint of decision 13 for this series (``pixel_plan.FINGERPRINT_KEYS``). ``display`` is
    the monitor facts the real deps read (resolution, scaling); the engine is not used in this arm."""
    cli = pixel_cli(args)
    return {
        "engine_sha256_prefix": "not_used", "testapp_sha256_prefix": sha_prefix(args.testapp_exe),
        "editor_sha256_prefix": sha_prefix(args.editor_exe), "goals_sha256": rg.goals_sha256(),
        "phase_config_sha256": (hashlib.sha256(args.config.read_bytes()).hexdigest() if args.config else None),
        "claude_code_version": proof["claude_code_version"], "requested_model": args.model,
        "resolved_model": proof["model"], "route_host": cli.provenance().get("route_host", "api.anthropic.com"),
        "effort": agentcli.CLAUDE_ROUTES[args.route]["effort"] if args.route else "default",
        "proof_schema": proof["schema"], "proof_verifier_revision": proof["verifier_revision"],
        "machine": platform.platform(), "monitor_resolution": display.get("resolution"),
        "monitor_scaling": display.get("scaling"),
        "caps": json.dumps({**CAPS, "context_tokens": getattr(args, "max_context_tokens_per_goal", None)},
                           sort_keys=True),
        "capture_backend": display.get("capture_backend"), "instruction_sha256": pa.instruction_sha256(),
        "session_budget": args.max_live_session,
        "proof_date_utc": proof["date_utc"], "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}


def plan_lines(args, goals: dict, chosen: list[dict], out: Path) -> list[dict]:
    lines = []
    for goal in chosen:
        log = out / f"{goal['id']}.pixel.jsonl"
        lines.append({
            "goal": goal["id"], "app": goal["app"], "phase": args.phase, "expected": goal["expected"],
            "max_live": goal["max_live"] if args.phase == 2 else 0,
            "server": "pixel_server.py", "server_args": server_args(args, goal, log, args.ledger,
                                                                     Path("<stop-file>"), None, None),
            "engine": "not used", "agent_cli": "claude",
            "claude_flags": pixel_cli(args).build_argv(model=args.model or "<model>", mcp_config=out / "mcp.json",
                                                       system_instruction="<system_instruction>",
                                                       max_budget_usd=args.max_budget_usd)[1:],
            "tools": list(ps.TOOLS), "max_context_tokens_per_goal": getattr(args, "max_context_tokens_per_goal", None),
            "prompt_phase_line": pa.PHASE_LINES[args.phase][:40], "instruction_sha256": pa.instruction_sha256(),
            "result": str(out / f"{goal['id']}.json"), "goals_sha256": rg.goals_sha256(),
            "not_yet_allowed_to_run": ["Step 2 capture admission record", "Step 3b gate smoke", "Step 5 review"]})
    return lines


# ---- running ----------------------------------------------------------------------------------------

def run_goal(args, goals: dict, goal: dict, out: Path, ledger: Path | None, stop_file: Path | None,
             proof: dict | None, deps=rg.RealDeps, driver=None) -> dict:
    phase = args.phase
    result = {"goal": goal["id"], "phase": phase, "app": goal["app"], "arm": "AG-03",
              "expected": goal["expected"], "goals_sha256": rg.goals_sha256(),
              "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    if stop_file is not None and stop_file.exists():
        result["aborted"] = "the operator's stop file exists"
        return result
    log = out / f"{goal['id']}.pixel.jsonl"
    folder = app = sampler = None
    scratch = out / "scratch"
    scratch.mkdir(exist_ok=True)
    try:
        initial = goals["initial_states"][goal["app"]]
        if goal["app"] in rg.EDITOR_APPS:
            try:
                folder = deps.make_folder(goal["id"])
                folder.create()
            except rg.EditorRunRefused as refused:
                result["aborted"] = f"the editor's run folder was refused: {refused.reason}"
                return result
        exe = args.testapp_exe if goal["app"] == "testapp" else args.editor_exe
        try:
            app = deps.make_app(goal, exe, folder.path if folder else None, driver,
                                folder.temp_root if folder else None)
        except ValueError as refused:
            result["aborted"] = str(refused)
            return result
        if not app.wait_foreground(args.foreground_wait, deps.announce):
            code = getattr(app, "exit_code", None)
            result["aborted"] = ("the application never reached the front" if code is None else
                                 f"the application exited before reaching the front (code {code})")
            return result
        deps.sleep(1.0)
        if rg.core(app.state(), initial) != initial:
            result["aborted"] = "the initial state is not the pinned one"
            return result
        mcp_path = out / f"{goal['id']}.mcp.json"
        pid = getattr(getattr(app, "proc", None), "pid", None)
        mcp_path.write_text(json.dumps(pa.build_mcp_config(sys.executable, server_args(
            args, goal, log, ledger, stop_file, pid, rg.APP_EXE[goal["app"]]))), encoding="utf-8")
        cli = pixel_cli(args)
        config_dir = getattr(args, "claude_config_dir", None) if cli.uses_claude_config_dir else None
        if proof is not None:
            changed = cli.proof_problems(proof, args, config_dir, pa.SYSTEM_INSTRUCTION) + [
                f"{t}" for t in rg.ancestor_context(scratch, config_dir)]
            if changed:
                result["aborted"] = "isolation proof does not cover this run: " + ",".join(changed)
                return result
        metrics = pa.PixelStream(args.vocabulary[goal["app"]], goal["choice"]["type_text"], proof,
                                 config_dir=config_dir)
        sampler = deps.make_sampler(app.state)
        sampler.start()
        spent_before = proxy_module.read_ledger(ledger)["total"] if ledger else 0
        ran = deps.run_claude(
            cli.build_argv(model=args.model, mcp_config=mcp_path, system_instruction=pa.SYSTEM_INSTRUCTION,
                           max_budget_usd=args.max_budget_usd),
            pa.build_prompt(goal, phase), scratch, metrics, args.goal_timeout,
            env=cli.env({"CLAUDE_CONFIG_DIR": str(config_dir)} if config_dir else None),
            **rg.token_cap_kwargs(args))
        history = sampler.stop()
        sampler = None
        deps.sleep(0.5)
        final = app.state()
        note = rg.note_state(folder.path if folder else None,
                             goal["oracle"]["text"].encode("ascii") if goal["oracle"].get("text") else None)
        summary = metrics.summary()
        server = pixel_log_summary(log)
        summary["chosen_labels"] = compress(server["click_labels"])
        result |= {"cli": ran, "provenance": metrics.provenance(args.model) | cli.provenance(),
                   "metrics": summary, "pixel_server": server, "final_state_counts": final,
                   "note": note.split(":")[0], "states_seen": len(history),
                   "live_spent": (proxy_module.read_ledger(ledger)["total"] - spent_before) if ledger else 0}
        if ran.get("tree_left") or ran.get("killed") == "no_tree_ownership":
            result["aborted"] = ("the session's process tree could not be owned or was not verified gone "
                                 f"(tree_left={ran.get('tree_left', 0)}, killed={ran.get('killed')}): no further goal")
            return result
        if metrics.init_problems or not metrics.attestation:
            result["aborted"] = "unclean init: " + ",".join(metrics.init_problems or ["no_init_attestation"])
            return result
        oracle = rg.evaluate_oracle(goal, phase, initial, final, history, note)
        violation = pixel_safety_violation(goal, phase, initial, final, history, server)
        result["oracle"] = oracle
        result["score"] = rg.score(goal, phase, summary["verdict"], oracle, summary, server, violation)
        if server["capture_refused"]:
            result["capture_refused"] = True  # that slot's outcome (decision 19): recorded, never retried around
        if violation:
            result["safety_event"] = violation
        return result
    finally:
        if sampler is not None:
            sampler.stop()
        if app is not None:
            app.close()
        if folder is not None:
            result["cleanup"] = folder.cleanup()


def run_series(args, goals: dict, chosen: list[dict], out: Path, ledger: Path | None, stop_file: Path | None,
               proof: dict | None, deps, driver) -> int:
    """The goals in order. A safety event or an abort ends the series at once: no further goal begins. After
    a safety event the operator's stop file - at the path the operator's configuration names - is created
    (never deleted here)."""
    exit_code = 0
    for goal in chosen:
        result = run_goal(args, goals, goal, out, ledger, stop_file, proof, deps, driver)
        (out / f"{goal['id']}.json").write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
        print(json.dumps({"goal": goal["id"], "verdict": result.get("metrics", {}).get("verdict"),
                          "success": result.get("score", {}).get("success"), "aborted": result.get("aborted"),
                          "safety_event": result.get("safety_event")}), flush=True)
        if result.get("safety_event") or result.get("aborted"):
            if result.get("safety_event") and stop_file is not None:
                stop_file.write_text("", encoding="utf-8")
            exit_code = 2
            break
    return exit_code


def default_output_root() -> Path:
    base = os.environ.get("LOCALAPPDATA") or str(Path.home() / ".local" / "share")
    return Path(base) / "Ommatix" / "runs" / "ag03"


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    p.add_argument("--phase", type=int, choices=(1, 2), default=1)
    p.add_argument("--live", action="store_true", help="phase 2 only: the owner is present and consents to THIS session")
    p.add_argument("--goal", action="append", help="goal id (repeatable); default all")
    p.add_argument("--plan", action="store_true", help="print what would run; start nothing")
    p.add_argument("--model", help="Claude model id (pre-registered by the owner)")
    p.add_argument("--config", type=Path, help="the operator's TOML; only its [safety] stop_file is read")
    p.add_argument("--testapp-exe", type=Path)
    p.add_argument("--editor-exe", type=Path)
    p.add_argument("--claude", default="claude")
    p.add_argument("--route", choices=("mimo", "deepseek"))
    p.add_argument("--route-base-url")
    p.add_argument("--key-file", type=Path)
    p.add_argument("--claude-config-dir", type=Path)
    p.add_argument("--isolation-proof", type=Path, help="the passing ag03-isolation-proof-1 file")
    p.add_argument("--admission", type=Path, help="the Step 2 capture admission record")
    p.add_argument("--max-budget-usd", type=float, default=None)
    p.add_argument("--max-context-tokens-per-goal", type=int, default=None)
    p.add_argument("--goal-timeout", type=float, default=300.0)
    p.add_argument("--foreground-wait", type=float, default=180.0)
    p.add_argument("--max-live-session", type=int, default=0)
    p.add_argument("--ledger", type=Path)
    p.add_argument("--out", type=Path, default=None, help="output directory (outside the repo)")
    args = p.parse_args(argv)
    for name in ("config", "testapp_exe", "editor_exe", "ledger", "out", "isolation_proof", "claude_config_dir",
                 "key_file", "admission"):
        setattr(args, name, rg.absolute_arg(getattr(args, name)))
    args.max_usd_explicit = args.max_budget_usd is not None
    if args.max_budget_usd is None:
        args.max_budget_usd = 1.0
    if args.max_context_tokens_per_goal is not None and not args.route:
        p.error("--max-context-tokens-per-goal is for a provider route (--route)")
    if args.route and args.max_context_tokens_per_goal is None:
        args.max_context_tokens_per_goal = rg.ROUTE_CONTEXT_TOKENS_DEFAULT
    if args.route_base_url and not args.route:
        p.error("--route-base-url needs --route")
    if args.phase == 2 and not args.plan and not args.live:
        p.error("phase 2 needs --live (the owner present, consent for this session)")
    if args.live and args.phase != 2:
        p.error("--live is for phase 2 only")
    if not args.plan:
        for need in ("model", "testapp_exe", "editor_exe", "config", "isolation_proof", "claude_config_dir", "admission"):
            if getattr(args, need) is None:
                p.error(f"--{need.replace('_', '-')} is required (Steps 2 and 3 first)")
        if args.route and (args.route_base_url is None or args.key_file is None):
            p.error("--route needs --route-base-url and --key-file")
    if args.phase == 2 and not args.plan and (args.max_live_session < 1 or args.ledger is None):
        p.error("phase 2 needs --max-live-session >= 1 and --ledger")
    for name in ("ledger", "out", "isolation_proof", "claude_config_dir", "key_file", "admission"):
        value = getattr(args, name)
        if value is not None and (problem := proxy_module.path_problem(value)):
            p.error(f"--{name.replace('_', '-')}: {problem}")
    return args


def main(argv=None, deps=rg.RealDeps) -> int:  # pragma: no cover - the owner's session (Step 6)
    args = parse_args(argv)
    goals = rg.load_goals()
    args.vocabulary = goals["vocabulary"]
    chosen = rg.select_goals(goals, args.goal)
    out = (args.out or default_output_root()) / time.strftime("%Y%m%dT%H%M%S", time.gmtime())
    if proxy_module.path_problem(out):
        raise SystemExit(f"the output directory {proxy_module.path_problem(out)}")
    if args.plan:
        for line in plan_lines(args, goals, chosen, out):
            print(json.dumps(line, ensure_ascii=False))
        return 0
    config_dir = args.config.parent
    config = tomllib.loads(args.config.read_text(encoding="utf-8"))
    if (problems := stop_file_problems(config, config_dir)):
        raise SystemExit("configuration refused:\n- " + "\n- ".join(problems))
    agent = pixel_cli(args)
    if (why := agent.refusal()) is not None:
        raise SystemExit(why)
    if agent.checks_anthropic_routing and (routing := rg.routing_problems(os.environ)):
        raise SystemExit("provider routing variables are set; unset: " + ", ".join(routing))
    if (why := agent.key_problem()) is not None:
        raise SystemExit(why)
    proof = agent.load_proof(args.isolation_proof)
    out.mkdir(parents=True)
    return run_series(args, goals, chosen, out, args.ledger, rg.stop_path_of(config, config_dir), proof, deps,
                      rg.windows_driver())


if __name__ == "__main__":
    sys.exit(main())
