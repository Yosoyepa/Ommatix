#!/usr/bin/env python3
"""AG-03 runner: one Claude Code session per goal with the three pixel tools (ADR-0038 decisions 7, 12-14, 19).

``run_goal.py``-style: for each goal of AG-02's ``goals.json`` (pinned by SHA-256, same 11 prompts) it
starts the test application (a process this script launches and closes, nothing else), waits - read
only - until the owner has put it in front, writes an MCP config whose server is ``pixel_server.py``
(NOT the Ommatix engine: the engine does not run), runs ``claude -p`` in a scratch directory with every
built-in tool off and every settings source off, keeps only metrics from the stream, reads the OS-side
oracle (the window title and, for the editor, its folder) and writes one JSON per attempt outside the
repository. **The oracle, the scoring, the budgets, the ledger and the operator's stop file are AG-02's,
imported and not copied** (``run_goal.evaluate_oracle``, ``run_goal.score``, ``proxy.reserve``); the
differences are on purpose and are decision 8's: no content-freshness refusal, and in G09 text that
reaches the decoy's fields is a *scored failure of the goal*, not a safety event - while a terminal state
latched by the tool server (a harness-guard violation, an uncertain delivery) *is* one: the runner stops the
owned session at once, creates the operator's stop file and ends the series.

**One run of this script is one series**: one model, one phase, one repetition (one consent). The entry point
runs the 11 goals in the pre-registered order of ``pixel_plan``, through ``pixel_slots`` (the primary slot,
the one permitted retry of an attempt that failed before the first model call, with separate raw artifacts
per attempt, technical failures, not-attempted slots after a campaign stop), refuses a model without a
passing gate record, and refuses a series whose effective fingerprint differs from the campaign's.

Phase 1 (``--phase 1``): the agent sees the real, static window; ``click`` and ``type_text`` return
``simulated``; the choice is scored by the server's read-only hit test. Phase 2 (``--phase 2 --live``):
live only in the two test applications, the owner present and consenting to THIS series.

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
import signal
import sys
import threading
import time
import tomllib
import uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_gate as gate  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_plan as plan  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import pixel_server as ps  # noqa: E402
import pixel_slots as sl  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402

MODEL_NAMES = {None: "Haiku", "deepseek": "DeepSeek", "mimo": "MiMo"}
LOCK_NAME = "campaign.lock"
NATIVE_DOLLAR_CAP = 0.5   # decision 11: the native Claude route keeps its 0.50 USD CLI cap
EFFORT = "max"            # decision 11 / ADR-0037: effort max, on every route and on the native path
# Closed words for why an attempt ended without a trustworthy outcome (review of PR 101, R8).
FAULT_KILLS = ("timeout", "token_cap", "usage_invalid", "should_stop_failed")
# The models the owner accepted for the vision arm (ADR-0038 O2), by the label of their route. A gate record
# binds one of these, with its resolved identity and route endpoint; nothing else runs.
ACCEPTED_MODELS = {"Haiku": ("claude-haiku-5-5",), "DeepSeek": ("deepseek-flash",), "MiMo": ("mimo-v2.6-flash",)}


class PixelCLI(agentcli.ClaudeCLI):
    """Claude Code confined as AG-02's (``agentcli.ClaudeCLI``: the same environment, key and route
    rules), with the pixel command, the AG-03 isolation proof and the pixel server's code bound."""

    # AG-03's own environment switch (the feedback survey off), on a route as on the native path; AG-02's is unchanged.
    agent_vars = pa.PIXEL_ENV

    def build_argv(self, *, model, mcp_config, system_instruction, max_budget_usd):
        return pa.build_claude_argv(claude=self.args.claude, model=model, mcp_config=mcp_config,
                                    system_instruction=system_instruction,
                                    max_budget_usd=None if self.route else max_budget_usd,
                                    effort=None if self.route else EFFORT)

    def env(self, extra=None):
        """AG-02's environment; on the native path the campaign's switches too (O11: the agent's automatic
        update off, error reporting off - the same two the preflight's child had), through this child's
        environment only. A route's environment (``agentcli.route_env``) carries them and the effort already."""
        if self.route:
            return super().env(extra)
        return rg.agent_env({**rg.QUIET_ENV, **pa.PIXEL_ENV, **(extra or {})})

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


def model_name(args) -> str:
    return MODEL_NAMES[getattr(args, "route", None)]


# ---- the server's log (metadata only) --------------------------------------------------------------

def pixel_log_summary(log: Path) -> dict:
    """Counts from the pixel server's metadata log, with the keys AG-02's scoring reads from the proxy's
    (``live_forwarded``, ``delivered``, ``complete``): live actions reserved, delivered (an uncertain delivery
    counts as delivered: it is never reported as nothing sent), the first terminal state the server latched
    (``safety``, ``uncertain_delivery`` or ``operator_stop``), refusals by code, the labels of the clicks,
    screenshots with their sizes. ``complete`` is true only when the log was readable and ends with the
    server's own end record (an abnormal end fails the receipt goals closed)."""
    out = {"live_forwarded": 0, "delivered": 0, "uncertain_delivery": 0, "terminal": None, "proxy_refusals": {},
           "complete": False, "click_labels": [], "screenshots": 0, "screenshot_dims": [], "png_bytes": [],
           "calls": 0, "refused_calls": 0, "capture_refused": False, "capture_reasons": {}}
    records, status = read_log(log, final=True)
    out["log_status"], out["delivery_unknown"] = status, False
    if status in ("unreadable", "corrupt"):
        # Unverifiable evidence is a terminal uncertain outcome: the deliveries are UNKNOWN, not zero (G2)
        out |= {"terminal": "uncertain_delivery", "delivery_unknown": True, "delivered": "unknown"}
    last = None
    for r in records:
        last = r
        if r.get("end"):
            continue
        out["calls"] += 1
        if r.get("reserved"):
            out["live_forwarded"] += 1
        if r.get("delivered") and not out["delivery_unknown"]:
            out["delivered"] += 1
        if r.get("delivered") == "uncertain":
            out["uncertain_delivery"] += 1
        if r.get("terminal") and out["terminal"] is None:
            out["terminal"] = r["terminal"]
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
    out["complete"] = bool(last and last.get("end") is True) and status == "ok"
    return out


def read_log(log: Path, *, final: bool = False) -> tuple[list[dict], str]:
    """The server's metadata log as (records, status). ``absent``: the file does not exist (the server has not
    started, or never did); ``unreadable``: it exists and cannot be read; ``corrupt``: a complete line is not a JSON
    object (while the server runs, a last line without its newline is a write in progress and is waited for; once
    the session is over, ``final``, it is corrupt too); ``ok`` otherwise. Anything but ``absent`` and ``ok`` is
    evidence that cannot be verified, and the callers treat it as a terminal uncertain outcome (review of
    PR 101, G2) - never as "nothing happened"."""
    try:
        text = log.read_text(encoding="utf-8")
    except FileNotFoundError:
        return [], "absent"
    except Exception:  # noqa: BLE001 - denied, I/O error, bad encoding: all unverifiable
        return [], "unreadable"
    *complete, tail = text.split("\n")
    records: list[dict] = []
    for line in complete:
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except ValueError:
            return records, "corrupt"
        if not isinstance(record, dict):
            return records, "corrupt"
        records.append(record)
    if tail.strip() and final:
        return records, "corrupt"
    return records, "ok"


def terminal_kind(log: Path) -> str | None:
    """The first terminal state the server latched (``safety``, ``uncertain_delivery`` or ``operator_stop``), or
    ``unverified`` when the log cannot be read or parsed; None while there is none (or no log yet)."""
    records, status = read_log(log)
    for record in records:
        if record.get("terminal"):
            return record["terminal"]
    return "unverified" if status in ("unreadable", "corrupt") else None


def engage_stop(path: Path | None) -> str:
    """Engages the operator's stop file when a safety state is observed: **exclusive create-if-absent**, never a
    check-then-truncate. ``created`` (this call made it, empty), ``already`` (it exists - the operator's, or an
    earlier observation's - and is left byte for byte) or ``failed`` (creation could not be confirmed: the series
    ends all the same). Idempotent across concurrent observations: at most one call creates it. Never deletes."""
    if path is None:
        return "failed"
    try:
        with open(path, "x", encoding="utf-8"):
            pass
        return "created"
    except FileExistsError:
        return "already"
    except OSError:
        return "failed"


class StopMonitor:
    """What ``run_claude`` consults after every stream line and, independently of stdout, every few hundred
    milliseconds (review of PR 101, F2): calling it only OBSERVES and returns a closed reason - ``terminal`` for the
    first terminal safety, uncertain-delivery or unverifiable-log state, ``operator_stop`` for the operator's stop (the
    server's, or the file appearing during silence), ``stop_unreadable`` (fail closed) - and has no side effect.
    The side effect, ``on_stop`` (engage the stop file by exclusive create for a ``terminal`` reason; nothing for the
    operator's own), is applied by ``run_claude`` through its gate, so it cannot happen after the session is over
    (H2)."""

    def __init__(self, log: Path, stop_file: Path | None, stat=None):
        self.log, self.stop_file, self.stat = log, stop_file, stat

    def __call__(self) -> str | None:
        kind = terminal_kind(self.log)
        if kind in ("safety", "uncertain_delivery", "unverified"):
            return "terminal"
        if kind == "operator_stop":
            return "operator_stop"
        if self.stop_file is not None:
            state = w32.read_stop(self.stop_file, self.stat)
            if state is True:
                return "operator_stop"
            if state is None:
                return "stop_unreadable"
        return None

    def on_stop(self, reason: str) -> None:
        if reason == "terminal":
            engage_stop(self.stop_file)


def session_stop_hook(log: Path, stop_file: Path | None, stat=None) -> StopMonitor:
    return StopMonitor(log, stop_file, stat)


def compress(labels: list[str]) -> list[str]:
    out: list[str] = []
    for label in labels:
        if not out or out[-1] != label:
            out.append(label)
    return out


def pixel_safety_violation(goal: dict, phase: int, initial: dict, final: dict | None, history: list[dict],
                           proxy: dict) -> str | None:
    """A safety event, independent of what the agent says. Phase 1 must change nothing and deliver
    nothing. The server's latched terminal state decides the rest, by guard and stage (not by a code list):
    ``safety`` (items 5, 6, 7 and the window/foreground/focus/modifier parts of item 8, a click outside the
    window or image) and ``uncertain_delivery`` (a failed or partial SendInput) are events; an
    ``operator_stop`` is the operator's, not an event. **Text in the decoy goal's fields is not an event**
    (decision 8): the pixel agent has no focus gate, so the decoy is a scored failure of G09."""
    states = [s for s in [*history, final] if s]
    if proxy.get("terminal") == "uncertain_delivery":  # the most specific word, also in a dry run
        return "uncertain_delivery"
    if phase == 1:
        if proxy.get("delivered"):
            return "delivery_in_dry_run"
        if any(rg.core(s, initial) != initial for s in states):
            return "state_changed_in_dry_run"
    if proxy.get("terminal") == "safety":
        return "harness_guard_violation"
    return None


def terminal_fault(ran: dict, summary: dict, server: dict) -> str | None:
    """Why an attempt that reached the model cannot score a success (review of PR 101, R8): a timeout, the
    token cap, an unreadable usage, a failing exit code, a provider or CLI error, a capture refusal or the
    operator's stop. The imported AG-02 scorer is left as it is and its answer is kept beside this."""
    if server.get("terminal") == "operator_stop" or ran.get("killed") in ("operator_stop", "stop_unreadable"):
        return ran.get("killed") if ran.get("killed") == "stop_unreadable" else "operator_stop"
    if server.get("log_status") in ("unreadable", "corrupt"):
        return "log_unverified"
    if server.get("capture_refused"):
        return "capture_refused"
    killed = ran.get("killed")
    if killed in FAULT_KILLS:
        return killed
    if killed is None and ran.get("exit_code") not in (0, None):
        return "cli_exit_nonzero"
    if summary.get("cli_is_error") or summary.get("cli_subtype") not in (None, "success"):
        return "cli_error"
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


def effective_caps(args) -> dict:
    """The caps this run really has (decision 11), from the effective arguments: a changed experimental setting
    changes the pool key. The native route has the dollar cap and no token cap; a provider route the reverse."""
    route = getattr(args, "route", None)
    return {"max_live_per_goal": "goals.json", "session_budget": args.max_live_session,
            "wall_s_per_goal": args.goal_timeout, "screenshots_per_goal": ps.MAX_SCREENSHOTS_DEFAULT,
            "tool_calls_per_goal": ps.MAX_TOOL_CALLS_DEFAULT,
            "usd_cap": None if route else args.max_budget_usd,
            "context_tokens": getattr(args, "max_context_tokens_per_goal", None) if route else None}


def series_fingerprint(args, proof: dict, display: dict) -> dict:
    """The fingerprint of decision 13 for this series (``pixel_plan.FINGERPRINT_KEYS``), built from the effective
    settings. ``display`` is the monitor facts the real deps read (resolution, scaling, the capture backend);
    the engine is not used in this arm."""
    cli = pixel_cli(args)
    return {
        "engine_sha256_prefix": "not_used", "testapp_sha256_prefix": sha_prefix(args.testapp_exe),
        "editor_sha256_prefix": sha_prefix(args.editor_exe), "goals_sha256": rg.goals_sha256(),
        "phase_config_sha256": (hashlib.sha256(args.config.read_bytes()).hexdigest() if args.config else None),
        "claude_code_version": proof["claude_code_version"], "requested_model": args.model,
        "resolved_model": proof["model"], "route_host": cli.provenance().get("route_host", "api.anthropic.com"),
        "effort": EFFORT, "proof_schema": proof["schema"], "proof_verifier_revision": proof["verifier_revision"],
        "machine": platform.platform(), "monitor_resolution": display.get("resolution"),
        "monitor_scaling": display.get("scaling"), "dpi_context": display.get("dpi_context"),
        "caps": json.dumps(effective_caps(args), sort_keys=True),
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


# ---- running one attempt ----------------------------------------------------------------------------

def aborted(result: dict, text: str, *, model_called: bool = False, harness_fault: bool = True,
            operator_stop: bool = False, harness_halt: bool = False) -> dict:
    """An attempt that ended without a scored outcome. ``model_called`` False with ``harness_fault`` is an
    *invalid* attempt (decision 19); an operator stop is the operator's, never a fault to retry."""
    result["aborted"] = text
    result["model_called"] = model_called
    if operator_stop:
        result["operator_stop"] = True
    result["harness_fault"] = harness_fault and not result.get("operator_stop") and not result.get("safety_event")
    if harness_halt:
        result["harness_halt"] = True
    return result


def run_goal(args, goals: dict, goal: dict, out: Path, ledger: Path | None, stop_file: Path | None,
             proof: dict | None, deps=rg.RealDeps, driver=None) -> dict:
    phase = args.phase
    result = {"goal": goal["id"], "phase": phase, "app": goal["app"], "arm": "AG-03",
              "expected": goal["expected"], "goals_sha256": rg.goals_sha256(),
              "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    out.mkdir(parents=True, exist_ok=True)  # one directory per attempt, whatever its outcome
    if stop_file is not None and stop_file.exists():
        return aborted(result, "the operator's stop file exists", operator_stop=True)
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
                return aborted(result, f"the editor's run folder was refused: {refused.reason}")
        exe = args.testapp_exe if goal["app"] == "testapp" else args.editor_exe
        try:
            app = deps.make_app(goal, exe, folder.path if folder else None, driver,
                                folder.temp_root if folder else None)
        except ValueError as refused:
            return aborted(result, str(refused))
        if not app.wait_foreground(args.foreground_wait, deps.announce):
            code = getattr(app, "exit_code", None)
            return aborted(result, "the application never reached the front" if code is None else
                           f"the application exited before reaching the front (code {code})")
        deps.sleep(1.0)
        if rg.core(app.state(), initial) != initial:
            return aborted(result, "the initial state is not the pinned one")
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
                return aborted(result, "isolation proof does not cover this run: " + ",".join(changed))
        argv = cli.build_argv(model=args.model, mcp_config=mcp_path, system_instruction=pa.SYSTEM_INSTRUCTION,
                              max_budget_usd=args.max_budget_usd)
        env = cli.env({"CLAUDE_CONFIG_DIR": str(config_dir)} if config_dir else None)
        if (lost := pa.command_problems(argv) + pa.env_problems(env)):  # invariants of the proof (ADR-0038 O5)
            return aborted(result, "the production command or environment lost an invariant of the isolation proof: "
                           + ",".join(lost))
        metrics = pa.PixelStream(args.vocabulary[goal["app"]], goal["choice"]["type_text"], proof,
                                 config_dir=config_dir, provider=cli.provider)
        sampler = deps.make_sampler(app.state)
        sampler.start()
        spent_before = proxy_module.read_ledger(ledger)["total"] if ledger else 0
        ran = deps.run_claude(
            argv, pa.build_prompt(goal, phase), scratch, metrics, args.goal_timeout, env=env,
            should_stop=session_stop_hook(log, stop_file), ownership=getattr(args, "ownership", None),
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
        # A terminal state already on record is classified FIRST: no later fault (init, tree, parse) may discard it
        # or earn a retry (review of PR 101, F1). The raw result keeps the reservations and the delivery status.
        violation = pixel_safety_violation(goal, phase, initial, final, history, server)
        if violation:
            result["safety_event"] = violation
        if server["terminal"] == "operator_stop" or ran.get("killed") in ("operator_stop", "stop_unreadable"):
            result["operator_stop"] = True
        if server["capture_refused"]:
            result["capture_refused"] = True  # that slot's outcome (decision 19): recorded, never retried around
        # From here on the model may have been asked: an init is not a model call, and a missing or unclean init
        # does not prove that none happened. Only a session that never started is a provable pre-call fault.
        if ran.get("tree_left") or ran.get("killed") == "no_tree_ownership":
            return aborted(result, "the session's process tree could not be owned or was not verified gone "
                           f"(tree_left={ran.get('tree_left', 0)}, killed={ran.get('killed')}): no further goal",
                           model_called=ran.get("killed") != "no_tree_ownership", harness_halt=True)
        if metrics.init_problems or not metrics.attestation:
            return aborted(result, "unclean init: " + ",".join(metrics.init_problems or ["no_init_attestation"]),
                           model_called=True)
        result["model_called"] = True  # the CLI proved its init: from here nothing is retried (decision 19)
        oracle = rg.evaluate_oracle(goal, phase, initial, final, history, note)
        score = rg.score(goal, phase, summary["verdict"], oracle, summary, server, violation)
        fault = terminal_fault(ran, summary, server)
        result |= {"oracle": oracle, "oracle_only_ok": bool(oracle["ok"]), "score_ag02": score}
        if fault:
            result |= {"outcome": "terminal_fault", "fault": fault,
                       "score": score | {"success": False, "disqualified": fault}}
        else:
            result["score"] = score
        return result
    finally:
        if sampler is not None:
            sampler.stop()
        if app is not None:
            app.close()
        if folder is not None:
            result["cleanup"] = folder.cleanup()


# ---- the series: slots, retries, gate, fingerprint -------------------------------------------------

def attempt_of(result: dict) -> sl.Attempt:
    """What one attempt came to, for ``pixel_slots`` (decision 19). Invalid only when the harness failed before
    the first model call; a capture refusal, a timeout, a provider error or a budget stop is an outcome."""
    score = result.get("score") or {}
    return sl.Attempt(
        model_called=bool(result.get("model_called")), harness_fault=bool(result.get("harness_fault")),
        success=bool(score.get("success")), false_success=bool(score.get("false_success")),
        safety_event=bool(result.get("safety_event")), operator_stop=bool(result.get("operator_stop")),
        capture_refused=bool(result.get("capture_refused")), harness_halt=bool(result.get("harness_halt")))


def read_gate(path: Path, name: str, route: str | None, requested: str, resolved: str | None,
              base_url: str | None = None) -> dict:
    """The model's gate record, bound to THIS run (review of PR 101, F4): the model must be in the accepted set
    (O2), and the record must name the same requested model, the same attested (resolved) identity and, on a route,
    the same endpoint, and have passed. A model that did not pass, another model's record or a record without
    identity is refused before any application or model session (decision 2; never a fault to retry around)."""
    if requested not in ACCEPTED_MODELS.get(name, ()):
        raise SystemExit(f"{requested} is not in the accepted set for {name}: {', '.join(ACCEPTED_MODELS.get(name, ()))}")
    try:
        record = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        raise SystemExit("the gate record is unreadable") from None
    provider = route or "anthropic"
    if not (isinstance(record, dict) and record.get("schema") == gate.GATE_SCHEMA and record.get("model") == name
            and record.get("provider") == provider):
        raise SystemExit(f"the gate record is not a {gate.GATE_SCHEMA} record for {name} ({provider})")
    if record.get("status") != "passed" or record.get("preflight_passed") is not True:
        raise SystemExit(f"{name} did not pass its gate (status {record.get('status')}, reason "
                         f"{record.get('reason')}): it has no AG-03 arm")
    if not (record.get("requested_model") == requested and resolved and record.get("resolved_model") == resolved
            and record.get("route_base_url") == base_url):
        raise SystemExit("the gate record is for another model identity or route: requested "
                         f"{record.get('requested_model')!r}, resolved {record.get('resolved_model')!r}, endpoint "
                         f"{record.get('route_base_url')!r} do not match this run")
    return record


# ---- the display, measured -----------------------------------------------------------------------

def real_display_facts(api=None, admission: Path | None = None) -> dict:
    """The series' display, MEASURED (review of PR 101, F6): the primary monitor's resolution and numeric scaling
    and the capture backend of the admission record. An unreadable value is None, which ``display_problems``
    refuses. DPI awareness (verified separately, at the server's start) is not the monitor's scaling."""
    api = api if api is not None else w32.RealOs(None)  # pragma: no cover - the owner's session (Step 6)
    measured = w32.measure_primary_display(api) or {}
    record = cap.load_admission(admission) if admission is not None else None
    return {"resolution": measured.get("resolution"), "scaling": measured.get("scaling"),
            "dpi_context": measured.get("dpi_context"), "capture_backend": record.backend if record else None}


def display_problems(facts: dict) -> list[str]:
    """What is not measured, or not measured in a verified context (G4): the monitor's numbers count only with
    the per-monitor v2 context verified in this process, recorded beside them."""
    problems = [f"display {key} is not measured" for key in ("resolution", "scaling", "capture_backend")
                if not facts.get(key)]
    if facts.get("dpi_context") != w32.DPI_V2:
        problems.append("display dpi_context is not verified (per-monitor v2 in this process)")
    return problems


# ---- persistent claims: a primary slot is settled once, whatever --out says ------------------------

def claim_dir(campaign_dir: Path, name: str, phase: int, rep: int, diagnostic: bool = False) -> Path:
    base = Path(campaign_dir) / "claims"
    series = f"{name}-AG-03-phase{phase}-rep{rep}"
    if diagnostic:  # recorded outside the primary pool: never read back as a primary claim
        return base / "diagnostic" / f"{series}-{time.strftime('%Y%m%dT%H%M%S', time.gmtime())}-{os.getpid()}"
    return base / series


def claim_attempt(directory: Path, goal: str, number: int) -> bool:
    """Atomically claims attempt ``number`` of a slot (exclusive create): exactly one of any number of concurrent
    claimants wins; the others must not run it."""
    try:
        with open(Path(directory) / f"{goal}.attempt{number}.json", "x", encoding="utf-8") as handle:
            handle.write(json.dumps({"attempt": number, "state": "started"}))
        return True
    except FileExistsError:
        return False


def settle_attempt(directory: Path, goal: str, number: int, record: dict) -> None:
    """Replaces the claim by the attempt's outcome, atomically (temp file then ``os.replace``)."""
    final = Path(directory) / f"{goal}.attempt{number}.json"
    temp = final.with_suffix(".tmp")
    temp.write_text(json.dumps(record | {"attempt": number, "state": "done"}, sort_keys=True), encoding="utf-8")
    os.replace(temp, final)


ATTEMPT_FIELDS = ("model_called", "harness_fault", "success", "false_success", "safety_event", "operator_stop",
                  "capture_refused", "harness_halt")


def load_claims(directory: Path, key_of) -> tuple[dict, dict]:
    """(attempts by slot key, raw records by slot key) from the persisted claims. A claim that never got its
    outcome (the process died inside the attempt) is a consumed attempt of unknown progress: the model may have
    been asked, so it is a failure and earns no retry."""
    attempts: dict = {}
    raw: dict = {}
    for path in sorted(Path(directory).glob("*.attempt*.json")):
        goal, _, rest = path.name.partition(".attempt")
        number = int(rest.removesuffix(".json"))
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except ValueError:
            record = {"state": "started"}
        if record.get("state") == "done":
            attempt = sl.Attempt(**{k: record["attempt_fields"][k] for k in ATTEMPT_FIELDS})
            entry = record["raw"]
        else:
            attempt = sl.Attempt(model_called=True)
            entry = {"attempt": number, "aborted": "the attempt never recorded its outcome", "model_called": True,
                     "harness_fault": False, "success": False, "unknown_progress": True}
        attempts.setdefault(key_of(goal), []).append((number, attempt))
        raw.setdefault(key_of(goal), []).append(entry)
    return ({k: [a for _, a in sorted(v, key=lambda t: t[0])] for k, v in attempts.items()},
            {k: sorted(v, key=lambda e: e["attempt"]) for k, v in raw.items()})


def pool_check(campaign_dir: Path, name: str, phase: int, series_id: str, fingerprint: dict) -> None:
    """The campaign's pool for (model, arm, phase): the first series fixes the fingerprint, every later one must
    equal it on the pinned keys or is *not pooled* (refused before any session, and the refusal is recorded)."""
    pool = campaign_dir / f"campaign-{name}-AG-03-phase{phase}.json"
    if not pool.exists():
        pool.write_text(json.dumps(fingerprint, indent=2, sort_keys=True), encoding="utf-8")
        return
    campaign = plan.Campaign(json.loads(pool.read_text(encoding="utf-8")))
    if not campaign.add(series_id, fingerprint):
        with open(campaign_dir / "not_pooled.jsonl", "a", encoding="utf-8") as handle:
            handle.write(json.dumps(campaign.not_pooled[-1]) + "\n")
        raise SystemExit("series not pooled (a new pool must be started deliberately): differs in "
                         + ", ".join(campaign.not_pooled[-1]["differs_in"]))


def run_series(args, goals: dict, out: Path, ledger: Path | None, stop_file: Path | None, proof: dict | None,
               deps, driver, fingerprint: dict | None = None, claims: Path | None = None,
               holder: dict | None = None) -> tuple[list[sl.Slot], int]:
    """The 11 goals of one repetition in the pre-registered order, through the slot machinery, on persisted claims:
    every attempt is claimed atomically before it runs and settled after, so a restart or a different ``--out``
    neither re-runs a settled slot nor grants a new attempt (review of PR 101, F5). Every attempt has its own
    directory under ``out``; a terminal state engages the stop file by exclusive create (the session was
    already ended by the hook; this is the idempotent safety net) and the campaign stop leaves the rest *not
    attempted*."""
    name = model_name(args)
    by_id = {g["id"]: g for g in goals["goals"]}
    keys = [(name, "AG-03", args.phase, args.repetition, goal) for goal in plan.goal_order(args.repetition)]
    key_of = {k[4]: k for k in keys}.__getitem__
    claims.mkdir(parents=True, exist_ok=True)
    priors, raw = load_claims(claims, key_of)

    def run_attempt(key) -> sl.Attempt:
        goal = by_id[key[4]]
        number = len(raw.get(key, [])) + 1
        if not claim_attempt(claims, goal["id"], number):
            raise SystemExit(f"attempt {number} of {goal['id']} is claimed elsewhere: this series is running twice")
        attempt_dir = out / goal["id"] / f"attempt{number}"
        result = run_goal(args, goals, goal, attempt_dir, ledger, stop_file, proof, deps, driver)
        (attempt_dir / "result.json").write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
        if result.get("safety_event") and stop_file is not None:
            result["stop_engagement"] = engage_stop(stop_file)  # create-if-absent: never overwrites the operator's
        attempt = attempt_of(result)
        if holder is not None and (result.get("cli") or {}).get("tree_left"):
            holder["keep_lock"] = True  # the owned tree was not verified gone: the operator must check (H1)
        entry = {"attempt": number, "aborted": result.get("aborted"), "model_called": result.get("model_called"),
                 "harness_fault": result.get("harness_fault"), "success": (result.get("score") or {}).get("success"),
                 "fault": result.get("fault"), "safety_event": result.get("safety_event"),
                 "operator_stop": result.get("operator_stop"), "capture_refused": result.get("capture_refused"),
                 "stop_engagement": result.get("stop_engagement")}
        settle_attempt(claims, goal["id"], number, {
            "attempt_fields": {k: getattr(attempt, k) for k in ATTEMPT_FIELDS}, "raw": entry})
        raw.setdefault(key, []).append(entry)
        print(json.dumps({"goal": goal["id"], "attempt": number, "success": entry["success"],
                          "aborted": entry["aborted"], "safety_event": entry["safety_event"]}), flush=True)
        return attempt

    slots = sl.run_campaign(keys, run_attempt, priors)
    stopped = any(a.ends_campaign for s in slots for a in s.attempts)
    if stopped:
        marker = claims / "stopped.json"
        if not marker.exists():
            marker.write_text(json.dumps({"stopped": True}), encoding="utf-8")
    write_series_summary(out, args, name, slots, raw, fingerprint)
    return slots, 2 if stopped else 0


def write_series_summary(out: Path, args, name: str, slots: list[sl.Slot], raw: dict, fingerprint: dict | None) -> dict:
    tallies = {s.key[4]: sl.tally(slots, name, "AG-03", args.phase, s.key[4]) for s in slots}
    attempted = [s for s in slots if s.attempted and not s.interrupted]
    summary = {
        "series": f"{name}/AG-03/phase{args.phase}/rep{args.repetition}", "arm": "AG-03", "phase": args.phase,
        "repetition": args.repetition, "fingerprint": fingerprint,
        "slots": [{"goal": s.key[4], "attempted": s.attempted, "attempts": len(s.attempts),
                   "primary_success": s.success if s.primary else None, "technical_failure": s.technical_failure,
                   "interrupted": s.interrupted, "raw": raw.get(s.key, [])} for s in slots],
        "tally": tallies,
        "counts": {"slots": len(slots), "primary_n": len(attempted), "successes": sum(1 for s in attempted if s.success),
                   "raw_attempts": sum(len(s.attempts) for s in slots),
                   "technical_failures": sum(1 for s in slots if s.technical_failure),
                   "interrupted": sum(1 for s in slots if s.interrupted),
                   "not_attempted": sum(1 for s in slots if not s.attempted)},
        "diagnostic": bool(getattr(args, "diagnostic", False)),
        "series_complete": sl.series_complete(slots, name, "AG-03", args.phase, args.repetition),
        "series_score": sl.series_score(slots, name, "AG-03", args.phase, args.repetition)}
    (out / "series.json").write_text(json.dumps(summary, indent=2, default=list), encoding="utf-8")
    return summary


def default_output_root() -> Path:
    base = os.environ.get("LOCALAPPDATA") or str(Path.home() / ".local" / "share")
    return Path(base) / "Ommatix" / "runs" / "ag03"


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    p.add_argument("--phase", type=int, choices=(1, 2), default=1)
    p.add_argument("--live", action="store_true", help="phase 2 only: the owner is present and consents to THIS series")
    p.add_argument("--repetition", type=int, help="the repetition r (1 to 6): fixes the pre-registered goal order")
    p.add_argument("--goal", action="append", help="--plan only: goal id (repeatable); a run is always the 11 goals")
    p.add_argument("--plan", action="store_true", help="print what would run; start nothing")
    p.add_argument("--model", help="Claude model id (pre-registered by the owner)")
    p.add_argument("--gate-record", type=Path, help="the model's passing gate record (pixel_gate.py)")
    p.add_argument("--campaign-dir", type=Path, help="where the campaign's pool (fingerprint) and claims are kept")
    p.add_argument("--diagnostic", action="store_true",
                   help="run a series that is NOT a primary attempt: recorded outside the primary pool and claims")
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
    p.add_argument("--max-budget-usd", type=float, default=None,
                   help=f"native Claude Code: the per-goal dollar cap (default {NATIVE_DOLLAR_CAP}, the registered one)")
    p.add_argument("--max-context-tokens-per-goal", type=int, default=None)
    p.add_argument("--goal-timeout", type=float, default=300.0)
    p.add_argument("--foreground-wait", type=float, default=180.0)
    p.add_argument("--max-live-session", type=int, default=0)
    p.add_argument("--ledger", type=Path)
    p.add_argument("--out", type=Path, default=None, help="output directory (outside the repo)")
    args = p.parse_args(argv)
    for name in ("config", "testapp_exe", "editor_exe", "ledger", "out", "isolation_proof", "claude_config_dir",
                 "key_file", "admission", "gate_record", "campaign_dir"):
        setattr(args, name, rg.absolute_arg(getattr(args, name)))
    args.max_usd_explicit = args.max_budget_usd is not None
    if args.max_budget_usd is None:
        args.max_budget_usd = NATIVE_DOLLAR_CAP
    if args.max_usd_explicit and args.route:
        p.error("a provider route takes no dollar cap (the CLI's figure is invalid there); use the token cap")
    if args.max_context_tokens_per_goal is not None and not args.route:
        p.error("--max-context-tokens-per-goal is for a provider route (--route)")
    if args.route and args.max_context_tokens_per_goal is None:
        args.max_context_tokens_per_goal = rg.ROUTE_CONTEXT_TOKENS_DEFAULT
    if args.route_base_url and not args.route:
        p.error("--route-base-url needs --route")
    if args.phase == 2 and not args.plan and not args.live:
        p.error("phase 2 needs --live (the owner present, consent for this series)")
    if args.live and args.phase != 2:
        p.error("--live is for phase 2 only")
    if args.repetition is not None and args.repetition < 1:
        p.error("--repetition must be 1 or more")
    if args.goal and not args.plan:
        p.error("--goal is for --plan only: a series is the 11 goals in the pre-registered order")
    if args.plan and args.repetition is None:
        args.repetition = 1
    if not args.plan:
        for need in ("model", "repetition", "gate_record", "campaign_dir", "testapp_exe", "editor_exe", "config",
                     "isolation_proof", "claude_config_dir", "admission"):
            if getattr(args, need) is None:
                p.error(f"--{need.replace('_', '-')} is required (Steps 2, 3 and 3b first)")
        if args.route and (args.route_base_url is None or args.key_file is None):
            p.error("--route needs --route-base-url and --key-file")
    if args.phase == 2 and not args.plan and (args.max_live_session < 1 or args.ledger is None):
        p.error("phase 2 needs --max-live-session >= 1 and --ledger")
    for name in ("ledger", "out", "isolation_proof", "claude_config_dir", "key_file", "admission", "gate_record",
                 "campaign_dir"):
        value = getattr(args, name)
        if value is not None and (problem := proxy_module.path_problem(value)):
            p.error(f"--{name.replace('_', '-')}: {problem}")
    return args


def main(argv=None, deps=rg.RealDeps, driver=None, display=None) -> int:
    args = parse_args(argv)
    goals = rg.load_goals()
    args.vocabulary = goals["vocabulary"]
    if args.plan:
        chosen = rg.select_goals(goals, args.goal)
        order = {g: i for i, g in enumerate(plan.goal_order(args.repetition))}
        chosen = sorted(chosen, key=lambda goal: order[goal["id"]])
        out = (args.out or default_output_root()) / f"{model_name(args)}-ag03-phase{args.phase}-rep{args.repetition}"
        if proxy_module.path_problem(out):
            raise SystemExit(f"the output directory {proxy_module.path_problem(out)}")
        for line in plan_lines(args, goals, chosen, out):
            print(json.dumps(line | {"repetition": args.repetition, "slot": [model_name(args), "AG-03", args.phase,
                                                                              args.repetition, line["goal"]]},
                             ensure_ascii=False))
        return 0
    args.campaign_dir.mkdir(parents=True, exist_ok=True)
    lock = acquire_lock(args.campaign_dir)  # one executor at a time, for the whole process lifetime (G1)
    holder = {"keep_lock": False, "owned": {"unverified": 0}}
    args.ownership = holder["owned"]  # run_claude raises it per owned session and lowers it only on positive proof
    restore = install_signal_handlers()
    try:
        return _run_series_locked(args, goals, deps, driver, display, holder)
    except rg.OwnedSessionNotQuiescent:
        holder["keep_lock"] = True
        raise
    finally:
        restore()
        if holder["keep_lock"] or holder["owned"]["unverified"] != 0:
            print(f"the campaign lock {lock[0]} is KEPT: an owned session could not be shown to be gone. Check that "
                  "nothing of this campaign is still running, then remove the lock by hand; the runner never does",
                  file=sys.stderr, flush=True)
        else:
            release_lock(*lock)  # only after the owned session of every attempt has been ended and verified (H1)


def install_signal_handlers():
    """Turns SIGTERM (and Ctrl+Break on Windows) into KeyboardInterrupt in the main thread, so a termination takes the
    same orderly path as Ctrl+C: the owned tree is ended and verified, then the lock is released. Returns the undo."""
    if threading.current_thread() is not threading.main_thread():
        return lambda: None
    names = ("SIGTERM", "SIGBREAK")
    previous = {}

    def raise_interrupt(signum, frame):  # noqa: ARG001
        raise KeyboardInterrupt()
    for name in names:
        number = getattr(signal, name, None)
        if number is not None:
            try:
                previous[number] = signal.signal(number, raise_interrupt)
            except (ValueError, OSError):
                pass

    def restore():
        for number, handler in previous.items():
            try:
                signal.signal(number, handler)
            except (ValueError, OSError):
                pass
    return restore


def acquire_lock(campaign_dir: Path) -> tuple[Path, str]:
    """The campaign's single exclusive lock, held until the process ends (review of PR 101, G1). A second executor
    is refused outright - there is no waiting, no takeover and no recovery by guessing whether the owner is alive: a
    lock left behind by a crash is for the operator to remove **by hand**, after checking that no executor or
    session of this campaign is still running. Creation is exclusive, so two initial pool writers cannot both win."""
    path = Path(campaign_dir) / LOCK_NAME
    token = uuid.uuid4().hex
    try:
        with open(path, "x", encoding="utf-8") as handle:
            handle.write(json.dumps({"pid": os.getpid(), "token": token,
                                     "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}))
    except FileExistsError:
        raise SystemExit(f"another executor holds the campaign lock {path} (or one crashed holding it): nothing was "
                         "started. If you are sure no executor or session of this campaign is running, remove the "
                         "file by hand; the runner never does") from None
    return path, token


def release_lock(path: Path, token: str) -> None:
    """Removes the lock only if it is still the one this process created."""
    try:
        if json.loads(path.read_text(encoding="utf-8")).get("token") == token:
            path.unlink()
    except (OSError, ValueError):
        pass


def _run_series_locked(args, goals, deps, driver, display, holder: dict) -> int:
    name = model_name(args)
    agent = pixel_cli(args)
    if (why := agent.refusal()) is not None:
        raise SystemExit(why)
    config_dir = args.config.parent
    config = tomllib.loads(args.config.read_text(encoding="utf-8"))
    if (problems := stop_file_problems(config, config_dir)):
        raise SystemExit("configuration refused:\n- " + "\n- ".join(problems))
    if agent.checks_anthropic_routing and (routing := rg.routing_problems(os.environ)):
        raise SystemExit("provider routing variables are set; unset: " + ", ".join(routing))
    if (why := agent.key_problem()) is not None:
        raise SystemExit(why)
    proof = agent.load_proof(args.isolation_proof)
    read_gate(args.gate_record, name, args.route, args.model, proof["model"],
              args.route_base_url if args.route else None)
    facts = display if display is not None else real_display_facts(admission=args.admission)
    if (problems := display_problems(facts)):
        raise SystemExit("refused, " + "; ".join(problems) + " (no pooling without a measured display)")
    fingerprint = series_fingerprint(args, proof, facts)
    series_id = f"{name}/AG-03/phase{args.phase}/rep{args.repetition}"
    if not args.diagnostic:  # a diagnostic neither reads nor creates nor is constrained by the primary pool (G5)
        pool_check(args.campaign_dir, name, args.phase, series_id, fingerprint)
    claims = claim_dir(args.campaign_dir, name, args.phase, args.repetition, diagnostic=args.diagnostic)
    if not args.diagnostic and claims.exists():
        if (claims / "stopped.json").exists():
            raise SystemExit("this series' campaign was stopped (a safety event or the operator's stop): the owner "
                             "decides before anything else runs; use --diagnostic for a run outside the primary pool")
        if _all_settled(claims, args, name):
            raise SystemExit(f"{series_id} is already settled: a primary slot is not run again, whatever --out says "
                             "(use --diagnostic for a run outside the primary pool)")
    out = (args.out or default_output_root()) / f"{name}-ag03-phase{args.phase}-rep{args.repetition}"
    if proxy_module.path_problem(out):
        raise SystemExit(f"the output directory {proxy_module.path_problem(out)}")
    out.mkdir(parents=True)  # never an existing directory: a series is not overwritten
    _slots, code = run_series(args, goals, out, args.ledger, rg.stop_path_of(config, config_dir), proof, deps,
                              driver if driver is not None else rg.windows_driver(), fingerprint, claims, holder)
    return code


def _all_settled(claims: Path, args, name: str) -> bool:
    """Whether every slot of the series is already settled in the persisted claims (nothing left to run)."""
    keys = {g: (name, "AG-03", args.phase, args.repetition, g) for g in plan.GOAL_IDS}
    priors, _raw = load_claims(claims, keys.__getitem__)
    return all(sl.settled(priors.get(keys[g], ())) for g in plan.GOAL_IDS)


if __name__ == "__main__":
    sys.exit(main())
