#!/usr/bin/env python3
"""AG-03 gate smoke (ADR-0038 decisions 2 and 6, section B step 3b): ONE real session per model on synthetic images.

``pixel_gate.py`` holds the records and the pinned question; this module is the only place that sends anything to a
provider for the gate. It runs the production command of the pixel agent (``PixelCLI``: the pinned Claude Code, the three
tools, every settings source off, ``--no-session-persistence``) with the pixel server in **synthetic mode** - four drawn
PNGs of a fake window, a different word and shape each, never a capture, no engine, no input, no window, no focus - and
asks ``pixel_gate.SMOKE_PROMPT``. The model's answer is scored in memory, one boolean per image; **the record stores
that, closed tokens and counts, never the answer, an image or a hash of one**.

Preconditions, each checked before anything is read or sent:

* a **preconditions file written by the owner** (``--print-preconditions-command`` prints the PowerShell command that
  writes it; this tool never writes it and has no default for any field): the reading of the recipient's data-use pages
  (OI-2, ADR-0038 decision 5), for Anthropic also OI-1, and the owner's go-ahead for THIS provider, model and endpoint,
  at most 24 hours old. Missing or stale, the outcome is ``not_run`` and no key is read and no provider is called;
* a passing **isolation proof** (``pixel_preflight.py``) for the same model, route, configuration directory, pixel
  server, instruction and agent environment, made with the **same pinned CLI** (its SHA-256 is the proof's) and at most
  24 hours old (``clipin.verified`` checks the binary, the pin and the proof's age before anything is launched, and the
  session runs from the verified private copy);
* the key, on a route, only through ``--key-file`` into the child's environment (``agentcli.route_env``); it is in no
  argument, record, log or message. The native route uses the configuration directory's own login.

**One attempt.** The attempt is claimed by an exclusive create before the provider can be reached and the claim is never
removed: a model that fails (``smoke_failed``, a provider error, a timeout) is *excluded and recorded*, and nothing here
runs it again with other settings; a claim without a record (the process died) is a consumed attempt. Exits: 0 passed,
1 excluded, 3 not run, 2 refused (nothing was sent), 4 a session that could not be shown gone (the lock is kept).

**Evidence limitation (recorded in every record).** A real endpoint cannot be read request by request from a loopback
stand-in. The tools and the images that reached the model are therefore *stream-attested* (the CLI's ``init`` event lists
exactly the three tools, the stream shows the screenshot calls and their results) and are only as good as the proof
for the same pinned CLI hash, route and environment, whose stand-in session did read the requests. A route's provider
may still refuse or drop the image, which is what the gate establishes.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import os
import re
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_disk as disk  # noqa: E402
import pixel_gate as gate  # noqa: E402
import pixel_preflight as pf  # noqa: E402
import pixel_runner as pr  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402

PRECONDITIONS_SCHEMA = "ag03-gate-preconditions-1"
PRECONDITIONS_MAX_AGE_S = 24 * 3600
FUTURE_TOLERANCE_S = 300
SMOKE_SIZE = (1280, 720)            # the size the probe measured and the server's default; within the 1568 px edge cap
SMOKE_SERVER_CAPS = ("--max-screenshots", "6", "--max-tool-calls", "8")   # four are asked for; the server refuses beyond these
NATIVE_USD_CAP = 0.25               # half the registered 0.50 USD per-goal cap: a session of four small images
ROUTE_CONTEXT_TOKENS = 120_000      # a provider route has no dollar cap (ADR-0037): the context-token cap instead
TIMEOUT_S = 240.0
READING_REF = "ADR-0038 decision 5; chapter 09 section 9.12.1 (Privacidad)"
LIMITATION = ("stream-attested: a real endpoint cannot be read request by request; the three tools and the images that "
              "reached the model are known from the CLI's own init event and stream, and are as good as the isolation "
              "proof for the same pinned CLI hash, route and environment, whose stand-in session read the requests")
# Closed classes for how a provider's refusal reads in the result event. Judged in memory; the text is never stored.
IMAGE_REJECTED = re.compile(r"image|media[_ -]?type|multimodal|vision|content[_ -]?type|base64|unsupported (?:input|content)",
                            re.I)
TRANSPORT = re.compile(r"ECONN|ENOTFOUND|ETIMEDOUT|EAI_AGAIN|socket|network|connect|fetch failed|overloaded|rate.?limit|"
                       r"certificate|\b(?:429|5\d\d)\b", re.I)


# ---- the owner's preconditions ------------------------------------------------------------------------

def provider_of(args) -> str:
    return args.route or "anthropic"


def preconditions_fields(provider: str, model: str, base_url: str | None) -> list[tuple[str, object]]:
    """The fields of the owner's preconditions file, in order, without its time. ``oi1_recorded`` is Anthropic's only."""
    fields: list[tuple[str, object]] = [("schema", PRECONDITIONS_SCHEMA), ("provider", provider), ("model", model),
                                        ("route_base_url", base_url), ("privacy_reading_recorded", True)]
    if provider == "anthropic":
        fields.append(("oi1_recorded", True))
    return fields + [("owner_go_ahead", True), ("scope", "gate smoke: synthetic drawn images only"),
                     ("reading_ref", READING_REF)]


def ps_literal(value: object) -> str:
    if value is None:
        return "$null"
    if value is True:
        return "$true"
    return "'" + str(value).replace("'", "''") + "'"


def preconditions_command(provider: str, model: str, base_url: str | None, path: Path) -> str:
    """The PowerShell command the OWNER runs to record the preconditions for one provider, model and endpoint. It states
    nothing the owner has not affirmed by running it, and it stamps the time itself."""
    body = "; ".join(f"{key} = {ps_literal(value)}" for key, value in preconditions_fields(provider, model, base_url))
    stamp = "utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')"
    folder = ps_literal(str(path.parent))
    return (f"New-Item -ItemType Directory -Force -Path {folder} | Out-Null; "
            f"[ordered]@{{ {body}; {stamp} }} | ConvertTo-Json | Set-Content -Encoding utf8 -Path {ps_literal(str(path))}")


def load_preconditions(path: Path, provider: str, model: str, base_url: str | None,
                       now: float | None = None) -> tuple[dict, list[str]]:
    """``(preconditions for pixel_gate, closed problem tokens)``. A file that is absent, unreadable, for another
    provider, model or endpoint, or older than 24 hours yields NO preconditions (``pixel_gate`` then says what is missing);
    nothing is ever defaulted to true."""
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8-sig"))
    except FileNotFoundError:
        return {}, ["preconditions_file_absent"]
    except (OSError, ValueError):
        return {}, ["preconditions_file_unreadable"]
    if not isinstance(data, dict) or data.get("schema") != PRECONDITIONS_SCHEMA:
        return {}, ["preconditions_schema_differs"]
    problems = []
    for key, value in (("provider", provider), ("model", model), ("route_base_url", base_url)):
        if data.get(key) != value:
            problems.append(f"preconditions_{key}_differs")
    problems += [f"preconditions_{p}" for p in _age_problems(data.get("utc"), now)]
    if problems:
        return {}, problems
    return {key: data.get(key) is True for key in ("privacy_reading_recorded", "oi1_recorded", "owner_go_ahead")}, []


def _age_problems(utc, now: float | None) -> list[str]:
    import calendar
    try:
        made = calendar.timegm(time.strptime(utc, "%Y-%m-%dT%H:%M:%SZ"))
    except (TypeError, ValueError, OverflowError):
        return ["time_unreadable"]
    age = (time.time() if now is None else now) - made
    if age < -FUTURE_TOLERANCE_S:
        return ["from_the_future"]
    return ["expired"] if age > PRECONDITIONS_MAX_AGE_S else []


# ---- classifying what came back -------------------------------------------------------------------------

def answer_of(metrics) -> str:
    result = metrics.result if isinstance(metrics.result, dict) else {}
    text = result.get("result") if isinstance(result.get("result"), str) else ""
    return text or metrics.final_text


def provider_error_of(ran: dict, metrics) -> str | None:
    """The closed ``pixel_gate.PROVIDER_ERRORS`` token for a session that did not come to an answer, else None. The text
    of a refusal is read here, in memory, to pick the class, and is not kept."""
    killed = ran.get("killed")
    if killed == "timeout":
        return "timeout"
    if killed is not None or metrics.init_problems or not metrics.attestation:
        return "cli_error"
    result = metrics.result
    if not isinstance(result, dict):
        return "cli_error"
    failed = (ran.get("exit_code") not in (0, None) or result.get("is_error")
              or result.get("subtype") not in (None, "success"))
    if not failed:
        return None
    text = result.get("result") if isinstance(result.get("result"), str) else ""
    if IMAGE_REJECTED.search(text):
        return "provider_rejected_image"
    if TRANSPORT.search(text):
        return "transport_error"
    return "cli_error"


def stream_problems(summary: dict, ran: dict) -> list[str]:
    """What the stream shows that the proof's session did not: closed tokens, empty when the evidence is clean."""
    problems = []
    if sorted(summary["tools_listed"]) != sorted(pa.TOOLS):
        problems.append("tools_not_the_three")
    if summary["unexpected_tool_count"]:
        problems.append("unexpected_tool")
    if summary["init_problems"]:
        problems.append("init_unclean")
    if summary["tool_calls"].get("screenshot", 0) < gate.SMOKE_IMAGES:
        problems.append("screenshots_called_fewer_than_asked")
    if summary["screenshots"] < gate.SMOKE_IMAGES:
        problems.append("screenshots_seen_fewer_than_asked")
    if [list(d) for d in summary["screenshot_dims"]] != [list(SMOKE_SIZE)]:
        problems.append("screenshot_size_differs_from_the_server_s")
    if ran.get("tree_left"):
        problems.append("tree_left")
    return problems


def evidence_block(args, proof: dict, summary: dict, ran: dict, problems: list[str], disk_facts: dict,
                   cleanup: list[str]) -> dict:
    """Closed tokens, numbers and booleans only: no answer, no image, no hash of one, no text of the screen."""
    tokens = summary["tokens"]
    return {
        "basis": "stream_attested_with_proof", "limitation": LIMITATION, "problems": problems,
        "proof": {"schema": proof["schema"], "verifier_revision": proof["verifier_revision"], "date_utc": proof["date_utc"],
                  "cli_sha256": proof["cli_sha256"], "claude_code_version": proof["claude_code_version"],
                  "model": proof["model"], "route": proof.get("route"), "agent_env": proof["agent_env"]},
        "session": {"image_size": list(SMOKE_SIZE), "images_asked": gate.SMOKE_IMAGES,
                    "tool_calls": summary["tool_calls"], "screenshots_seen_by_stream": summary["screenshots"],
                    "screenshot_dims": [list(d) for d in summary["screenshot_dims"]],
                    "tools_listed": summary["tools_listed"], "init_problems": summary["init_problems"],
                    "exit_code": ran.get("exit_code"), "killed": ran.get("killed"), "tree_left": ran.get("tree_left", 0),
                    "cli_is_error": summary["cli_is_error"], "cli_subtype": summary["cli_subtype"],
                    "wall_s": summary["wall_s"], "turns": summary["turns"],
                    "tokens": {k: tokens[k] for k in ("input_tokens", "output_tokens")},
                    "cost_usd": None if args.route else summary["cost_usd"],
                    "caps": {"usd": None if args.route else NATIVE_USD_CAP,
                             "context_tokens": args.max_context_tokens_per_goal if args.route else None,
                             "timeout_s": args.timeout, "server": list(SMOKE_SERVER_CAPS)}},
        "disk": disk_facts, "cli_copy_cleanup": cleanup,
    }


# ---- the session --------------------------------------------------------------------------------------

def disk_tokens(config_dir: Path, work: Path, before: dict, after: dict) -> tuple[list[str], dict]:
    """What the session wrote under the configuration directory, its own temp directory and the CLI's cache. Recorded and
    reported; the images are synthetic, so it does not decide the gate (the proof established the invariant)."""
    c_tokens, c_facts = disk.config_problems(config_dir, before["config"], after["config"])
    t_tokens, t_facts = disk.temp_problems(work, before["temp"], after["temp"])
    k_tokens, k_facts = disk.cache_problems(after["cache_root"], before["cache"], after["cache"])
    tokens = sorted(set(c_tokens + t_tokens + k_tokens))
    return tokens, {"problems": tokens, "config_touched": c_facts["touched"], "temp_touched": t_facts["touched"],
                    "cache_touched": k_facts["touched"]}


def run_session(args, cli: pr.PixelCLI, proof: dict, work: Path, run_claude, claim) -> tuple[dict, object, dict, list[str]]:
    """The one session, from the verified private copy of the pinned CLI. Returns ``(ran, metrics, disk facts, the
    copy's clean-up)``; raises ``SystemExit`` with closed tokens when the launch must not happen (nothing was sent, and
    ``claim`` - the exclusive claim of the attempt - was not yet made: it is called as the last step before the CLI runs)."""
    scratch, tmp, log = work / "scratch", work / "cli-tmp", work / "pixel.jsonl"
    scratch.mkdir(parents=True, exist_ok=True)
    tmp.mkdir(exist_ok=True)
    mcp_path = work / "mcp.json"
    mcp_path.write_text(json.dumps(pa.build_mcp_config(sys.executable, [
        "--synthetic", "--synthetic-size", f"{SMOKE_SIZE[0]}x{SMOKE_SIZE[1]}", "--log", str(log), *SMOKE_SERVER_CAPS])),
        encoding="utf-8")
    with cli.launch_guard(proof, parent=tmp) as verified:
        if verified.problems:
            raise SystemExit("refused, nothing was sent: " + ",".join(verified.problems))
        argv = cli.build_argv(model=args.model, mcp_config=mcp_path, system_instruction=pa.SYSTEM_INSTRUCTION,
                              max_budget_usd=NATIVE_USD_CAP, **verified.kwargs())
        env = cli.env({"CLAUDE_CONFIG_DIR": str(args.claude_config_dir), **disk.child_temp_env(tmp)})
        lost = pa.command_problems(argv) + pa.env_problems(env) + pa.temp_env_problems(env)
        if disk.cache_root_fingerprint(env) != (proof.get("disk") or {}).get("cache_root_sha256"):
            lost.append("cache_root_differs_from_proof")
        if lost:
            raise SystemExit("refused, nothing was sent: the command or environment lost an invariant of the isolation "
                             "proof: " + ",".join(sorted(set(lost))))
        metrics = pa.PixelStream([], None, proof, config_dir=args.claude_config_dir, provider=cli.provider)
        cache_root = disk.cli_cache_root(environ=env)
        before = {"config": disk.snapshot(args.claude_config_dir), "temp": disk.snapshot(tmp),
                  "cache": disk.snapshot(cache_root, optional=True)}
        claim()
        ran = run_claude(argv, gate.SMOKE_PROMPT, scratch, metrics, args.timeout, env=env,
                         ownership=args.ownership, **rg.token_cap_kwargs(args))
        after = {"config": disk.snapshot(args.claude_config_dir), "temp": disk.snapshot(tmp),
                 "cache": disk.snapshot(cache_root, optional=True), "cache_root": cache_root}
    _tokens, facts = disk_tokens(args.claude_config_dir, tmp, before, after)
    return ran, metrics, facts, verified.cleanup


# ---- records and claims ------------------------------------------------------------------------------

def claim_path(out_dir: Path, name: str) -> Path:
    return out_dir / f"{name}.claim.json"


def record_path(out_dir: Path, name: str) -> Path:
    return out_dir / f"{name}.gate.json"


def write_exclusive(path: Path, text: str) -> None:
    with open(path, "x", encoding="utf-8") as handle:
        handle.write(text)


def claim_attempt(out_dir: Path, name: str, args) -> None:
    """The one attempt, claimed before the provider can be reached, by an exclusive create that is never undone."""
    try:
        write_exclusive(claim_path(out_dir, name), json.dumps({
            "model": name, "requested_model": args.model, "provider": provider_of(args),
            "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "state": "started"}))
    except FileExistsError:
        raise SystemExit(f"{name} has already been attempted ({claim_path(out_dir, name)} exists): the gate is one "
                         "attempt per model, an exclusion is recorded and not retried, and a claim without a record is "
                         "a consumed attempt; nothing was sent") from None


def refused_early(args, name: str, out_dir: Path) -> None:
    for path in (record_path(out_dir, name), claim_path(out_dir, name)):
        if path.exists():
            raise SystemExit(f"{name} already has {path.name}: one attempt per model, nothing was sent")


def preflight_failed_proof(path: Path, model: str, base_url: str | None) -> dict | None:
    """The proof when it is a well-formed ag03 proof for THIS model and endpoint that did NOT pass (the model failed its
    preflight: excluded and recorded), else None (a failed proof of another model excludes nobody)."""
    try:
        proof = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    route = proof.get("route") if isinstance(proof, dict) else None
    if isinstance(proof, dict) and proof.get("schema") == pa.PROOF_SCHEMA and proof.get("passed") is False \
            and isinstance(proof.get("problems"), list) and proof.get("requested_model") == model \
            and (route.get("base_url") if isinstance(route, dict) else None) == base_url:
        return proof
    return None


def summary_line(record: dict, path: Path | None) -> str:
    keys = ("model", "provider", "status", "reason", "missing", "file_problems", "per_image_ok", "provider_called")
    return json.dumps({k: record[k] for k in keys if k in record} | {"record": None if path is None else str(path)})


# ---- the command line -----------------------------------------------------------------------------------

def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    p.add_argument("--route", choices=sorted(agentcli.CLAUDE_ROUTES), help="deepseek or mimo; omit for native Claude")
    p.add_argument("--route-base-url")
    p.add_argument("--model", help="default: claude-haiku-5-5 native, the route's model on a route")
    p.add_argument("--claude", help="the pinned CLI: the absolute path of claude.exe 2.1.304 (clipin)")
    p.add_argument("--claude-config-dir", type=Path)
    p.add_argument("--isolation-proof", type=Path, help="the passing ag03-isolation-proof-1 file for this model/route")
    p.add_argument("--key-file", type=Path, help="a route only: a file outside the checkout whose first line is the key")
    p.add_argument("--preconditions", type=Path, help="the file the OWNER wrote (see --print-preconditions-command)")
    p.add_argument("--print-preconditions-command", action="store_true",
                   help="print the PowerShell command that records the owner's preconditions at --preconditions; send nothing")
    p.add_argument("--out-dir", type=Path, help="where the claim, the record and the work files go (outside the checkout)")
    p.add_argument("--timeout", type=float, default=TIMEOUT_S)
    p.add_argument("--max-context-tokens", type=int, default=ROUTE_CONTEXT_TOKENS, help="a route only")
    args = p.parse_args(argv)
    if args.route:
        spec = agentcli.CLAUDE_ROUTES[args.route]
        if args.route_base_url not in spec["base_urls"]:
            p.error("--route-base-url is required and must be one of: " + ", ".join(spec["base_urls"]))
        args.model = args.model or spec["model"]
    else:
        if args.route_base_url or args.key_file:
            p.error("--route-base-url and --key-file need --route")
        args.model = args.model or "claude-haiku-5-5"
    args.name = pr.MODEL_NAMES[args.route]
    if args.model not in pr.ACCEPTED_MODELS[args.name]:
        p.error(f"{args.model} is not in the accepted set for {args.name}: {', '.join(pr.ACCEPTED_MODELS[args.name])}")
    for need in ("preconditions",) + (() if args.print_preconditions_command else
                                      ("claude", "claude_config_dir", "isolation_proof", "out_dir")):
        if getattr(args, need) is None:
            p.error(f"--{need.replace('_', '-')} is required")
    for name in ("claude_config_dir", "isolation_proof", "key_file", "preconditions", "out_dir"):
        setattr(args, name, rg.absolute_arg(getattr(args, name)))
        value = getattr(args, name)
        if value is not None and (problem := proxy_module.path_problem(value)):
            p.error(f"--{name.replace('_', '-')}: {problem}")
    if args.max_context_tokens < 1 or args.timeout <= 0:
        p.error("--max-context-tokens and --timeout must be positive")
    # the attributes ``PixelCLI`` and ``rg.token_cap_kwargs`` read
    args.max_usd_explicit = False
    args.max_context_tokens_per_goal = args.max_context_tokens if args.route else None
    return args


def main(argv=None, run_claude=None) -> int:
    args = parse_args(argv)
    base_url = args.route_base_url if args.route else None
    if args.print_preconditions_command:
        print("# Run this yourself in PowerShell ONLY if every statement below is true for this provider and model;\n"
              f"# it records: {READING_REF} read, {'OI-1 recorded, ' if provider_of(args) == 'anthropic' else ''}"
              "and your go-ahead for ONE gate-smoke session on synthetic drawn images (no screen, no input).")
        print(preconditions_command(provider_of(args), args.model, base_url, args.preconditions))
        return 0
    args.out_dir.mkdir(parents=True, exist_ok=True)
    args.ownership = {"unverified": 0}
    lock = pr.acquire_lock(args.out_dir)  # one executor at a time; a stale lock is removed by hand, never by this tool
    restore = pr.install_signal_handlers()
    keep_lock = False
    try:
        return _locked(args, base_url, run_claude or rg.run_claude)
    except rg.OwnedSessionNotQuiescent as error:
        keep_lock = True
        print(f"the owned session could not be shown to be gone: {error}", file=sys.stderr)
        return 4
    finally:
        restore()
        if keep_lock or args.ownership["unverified"] != 0:
            print(f"the lock {lock[0]} is KEPT: check that nothing of this session is still running, then remove it "
                  "by hand", file=sys.stderr, flush=True)
        else:
            pr.release_lock(*lock)


def _locked(args, base_url: str | None, run_claude) -> int:
    name, provider, out_dir = args.name, provider_of(args), args.out_dir
    refused_early(args, name, out_dir)
    cli = pr.PixelCLI(args)
    if (why := cli.refusal()) is not None:
        raise SystemExit(why)
    if cli.checks_anthropic_routing and (routing := rg.routing_problems(os.environ)):
        raise SystemExit("provider routing variables are set; unset: " + ", ".join(routing))
    preconditions, file_problems = load_preconditions(args.preconditions, provider, args.model, base_url)
    identity = {"requested_model": args.model, "route_base_url": base_url}
    if gate.preconditions_missing(provider, preconditions):
        record = gate.gate_record(name, provider, preflight_passed=False, preconditions=preconditions, **identity)
        record["file_problems"] = file_problems
        print(summary_line(record, None))
        return 3  # not a gate outcome: no key was read, no provider was called, nothing is written
    if (why := cli.key_problem()) is not None:
        raise SystemExit(why)
    failed_proof = preflight_failed_proof(args.isolation_proof, args.model, base_url)
    if failed_proof is not None:
        record = gate.gate_record(name, provider, preflight_passed=False, preconditions=preconditions, **identity)
        record["evidence"] = {"basis": "preflight", "problems": sorted(set(map(str, failed_proof["problems"])))}
        path = record_path(out_dir, name)
        write_exclusive(path, json.dumps(record, indent=2))
        print(summary_line(record, path))
        return 1
    proof = cli.load_proof(args.isolation_proof)
    work = out_dir / name
    (work / "scratch").mkdir(parents=True, exist_ok=True)
    problems = cli.proof_problems(proof, args, args.claude_config_dir) \
        + rg.ancestor_context(work / "scratch", args.claude_config_dir)
    if problems:
        raise SystemExit("refused, nothing was sent: the isolation proof does not cover this run: "
                         + ",".join(sorted(set(problems))))
    ran, metrics, disk_facts, cleanup = run_session(args, cli, proof, work, run_claude,
                                                    lambda: claim_attempt(out_dir, name, args))
    summary = metrics.summary()
    error = provider_error_of(ran, metrics)
    evidence_problems = stream_problems(summary, ran)
    if error is None and evidence_problems:
        error = "cli_error"  # right words, but the evidence that the confined tools reached the model is not clean
    attested = (metrics.attestation or {}).get("model")
    record = gate.gate_record(name, provider, preflight_passed=True, preconditions=preconditions,
                              answer=None if error else answer_of(metrics), provider_error=error,
                              requested_model=args.model, resolved_model=attested, route_base_url=base_url)
    record["evidence"] = evidence_block(args, proof, summary, ran, evidence_problems, disk_facts, cleanup)
    path = record_path(out_dir, name)
    write_exclusive(path, json.dumps(record, indent=2))
    claim_path(out_dir, name).write_text(json.dumps({"model": name, "state": "done", "status": record["status"]}),
                                         encoding="utf-8")
    print(summary_line(record, path))
    if cleanup or disk_facts["problems"]:
        print("reported, not decisive (synthetic images): copy_cleanup=" + ",".join(cleanup)
              + " disk=" + ",".join(disk_facts["problems"]), file=sys.stderr)
    return 0 if record["status"] == "passed" else 1


def entry(argv=None) -> int:
    """The command line: a refusal (a ``SystemExit`` carrying a message: nothing was sent) is exit 2 with its message on stderr."""
    try:
        return main(argv)
    except SystemExit as refused:
        if isinstance(refused.code, str):
            print(refused.code, file=sys.stderr)
            return 2
        raise


if __name__ == "__main__":
    with contextlib.suppress(BrokenPipeError):
        sys.exit(entry())
