#!/usr/bin/env python3
"""AG-02 isolation preflight: a canary that proves what reaches the agent's context.

Run it once per Claude Code version, before any session that shows the agent a screen; the
runner refuses to start a goal without the proof file it writes (``--isolation-proof``).

It builds, in a temporary directory outside the repository, a synthetic world full of harmless
sentinels: a user CLAUDE.md, user rules, an auto-memory index, a skill, a command, an agent, hooks
(which would write a marker file and print a sentinel if they ran), a second MCP server (which
would write a marker if it started), ancestor and project CLAUDE.md files, a local CLAUDE.md,
project settings, a project command and a project ``.mcp.json``. It then runs the exact production
command - ``run_goal.build_claude_argv`` with ``--tools ""``, ``--strict-mcp-config`` and
``--setting-sources ""``, the production MCP config (the proxy in front of ``ommatix serve
--source sim``), the production environment and an empty scratch working directory under the
canary tree - with only the Anthropic endpoint replaced by a local stand-in on 127.0.0.1
(``fake_api.py``) that records the request bodies. No real API call is made and no real
instruction of the operator is used.

It passes only when, for the production run: the session exits cleanly; at least one request
body was captured; no sentinel appears in any body; no hook marker and no extra-server marker
exists; and the ``init`` event lists exactly the three Ommatix tools and the one connected
server and loads no skill, command or extra directory. A positive control then runs the same
world with the settings sources on and the CLAUDE.md switch off: it must leak at least one
sentinel, or the canary is blind and the preflight fails.

Differences from a production session, stated rather than hidden: ``CLAUDE_CONFIG_DIR`` points at
the canary's own configuration directory (so the operator's real one is never read or modified),
``ANTHROPIC_BASE_URL`` points at the stand-in and ``ANTHROPIC_API_KEY`` is a dummy. The
operator's real configuration is covered by the runner's check of the ``init`` event of every
session against this proof, not by this canary.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import secrets
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402
from fake_api import FakeApi  # noqa: E402

KINDS = ("user_claude_md", "user_rules", "auto_memory", "skill", "command", "agent",
         "hook_context", "ancestor_claude_md", "project_claude_md", "project_local_md",
         "project_command", "parent_claude_md")
HOOK_EVENTS = ("SessionStart", "UserPromptSubmit", "PreToolUse", "Stop")
HOOK_SCRIPT = '''import sys
marker, text = sys.argv[1], sys.argv[2]
with open(marker, "a", encoding="utf-8") as handle:
    handle.write("ran\\n")
print(text)
'''
MCP_SCRIPT = '''import sys
with open(sys.argv[1], "a", encoding="utf-8") as handle:
    handle.write("started\\n")
for _ in sys.stdin:
    pass
'''


def project_slug(path: Path) -> str:
    """Claude Code's project directory name for a working directory: every character outside
    letters and digits becomes a dash (best effort; the control run shows whether it was right)."""
    return re.sub(r"[^A-Za-z0-9]", "-", str(path))


def build_world(root: Path) -> dict:
    """The canary tree. Returns the sentinels by kind and the marker files that must stay absent."""
    token = secrets.token_hex(6)
    sent = {kind: f"CANARY-{kind.upper().replace('_', '-')}-{token}" for kind in KINDS}
    home, work = root / "home", root / "work"
    project = work / "project"
    cwd = project / "scratch"
    for d in (home / "rules", home / "skills" / "canary-skill", home / "commands", home / "agents",
              cwd / ".claude" / "commands", project, work):
        d.mkdir(parents=True, exist_ok=True)
    markers = {"hook": root / "hook.marker", "hook_project": root / "hook-project.marker",
               "mcp_user": root / "mcp-user.marker", "mcp_project": root / "mcp-project.marker"}
    (root / "hook.py").write_text(HOOK_SCRIPT, encoding="utf-8")
    (root / "mcp.py").write_text(MCP_SCRIPT, encoding="utf-8")
    py = sys.executable

    def hooks(marker: Path) -> dict:
        cmd = f'"{py}" "{root / "hook.py"}" "{marker}" "{sent["hook_context"]}"'
        return {e: [{"hooks": [{"type": "command", "command": cmd}]}] for e in HOOK_EVENTS}

    def mcp(marker: Path) -> dict:
        return {"canary": {"command": py, "args": [str(root / "mcp.py"), str(marker)]}}

    (home / "CLAUDE.md").write_text(sent["user_claude_md"] + "\n", encoding="utf-8")
    (home / "rules" / "canary.md").write_text(sent["user_rules"] + "\n", encoding="utf-8")
    (home / "skills" / "canary-skill" / "SKILL.md").write_text(
        f"---\nname: canary-skill\ndescription: {sent['skill']}\n---\n{sent['skill']}\n", encoding="utf-8")
    (home / "commands" / "canary-cmd.md").write_text(sent["command"] + "\n", encoding="utf-8")
    (home / "agents" / "canary-agent.md").write_text(
        f"---\nname: canary-agent\ndescription: {sent['agent']}\n---\n{sent['agent']}\n", encoding="utf-8")
    (home / "settings.json").write_text(json.dumps({"hooks": hooks(markers["hook"])}), encoding="utf-8")
    (home / ".claude.json").write_text(json.dumps({"mcpServers": mcp(markers["mcp_user"])}),
                                       encoding="utf-8")
    memory = home / "projects" / project_slug(cwd) / "memory"
    memory.mkdir(parents=True, exist_ok=True)
    (memory / "MEMORY.md").write_text("- " + sent["auto_memory"] + "\n", encoding="utf-8")
    (work / "CLAUDE.md").write_text(sent["ancestor_claude_md"] + "\n", encoding="utf-8")
    (project / "CLAUDE.md").write_text(sent["parent_claude_md"] + "\n", encoding="utf-8")
    (cwd / "CLAUDE.md").write_text(sent["project_claude_md"] + "\n", encoding="utf-8")
    (cwd / "CLAUDE.local.md").write_text(sent["project_local_md"] + "\n", encoding="utf-8")
    (cwd / ".claude" / "settings.json").write_text(
        json.dumps({"hooks": hooks(markers["hook_project"])}), encoding="utf-8")
    (cwd / ".claude" / "commands" / "pcmd.md").write_text(sent["project_command"] + "\n", encoding="utf-8")
    (cwd / ".mcp.json").write_text(json.dumps({"mcpServers": mcp(markers["mcp_project"])}),
                                   encoding="utf-8")
    return {"sentinels": sent, "markers": markers, "home": home, "cwd": cwd}


def leaked(bodies: list[bytes], sentinels: dict) -> list[str]:
    blob = b"".join(bodies)
    return sorted(kind for kind, text in sentinels.items() if text.encode("ascii") in blob)


def one_session(args, world: dict, *, control: bool, log: Path, scratch_name: str) -> dict:
    """One session of the canary world. Production flags, or (control) the settings sources on
    and the CLAUDE.md switch off."""
    goals = rg.load_goals()
    mcp = rg.build_mcp_config(sys.executable, ["--dry-run-only", "--log", str(log)],
                              [str(args.exe), "serve", "--source", "sim", "--legacy"])
    mcp_path = log.with_name(scratch_name + ".mcp.json")
    mcp_path.write_text(json.dumps(mcp), encoding="utf-8")
    argv = rg.build_claude_argv(claude=args.claude, model=args.model, mcp_config=mcp_path,
                                system_instruction=goals["system_instruction"], max_budget_usd=0.5)
    env = rg.agent_env()
    if control:
        index = argv.index("--setting-sources")
        argv[index + 1] = "user,project,local"
        env.pop("CLAUDE_CODE_DISABLE_CLAUDE_MDS", None)
    with FakeApi() as api:
        env |= {"CLAUDE_CONFIG_DIR": str(world["home"]), "ANTHROPIC_BASE_URL": api.url,
                "ANTHROPIC_API_KEY": "sk-ant-canary-0000000000"}
        metrics = rg.StreamMetrics([], None, None)
        cli = rg.run_claude(argv, rg.build_prompt({"prompt": "Say hi."}, 1), world["cwd"], metrics,
                            args.timeout, env=env)
        bodies = list(api.bodies)
    return {"cli": cli, "metrics": metrics, "bodies": bodies, "leaked": leaked(bodies, world["sentinels"]),
            "attestation": metrics.attestation}


def run(args) -> dict:
    """The whole preflight; returns the proof (``passed`` says whether it holds)."""
    with tempfile.TemporaryDirectory(prefix="ag02-canary-") as tmp:
        root = Path(tmp)
        world = build_world(root)
        production = one_session(args, world, control=False, log=root / "production.jsonl",
                                 scratch_name="production")
        markers_production = sorted(k for k, m in world["markers"].items() if m.exists())
        for marker in world["markers"].values():
            marker.unlink(missing_ok=True)  # the control run may start them; start clean
        control = one_session(args, world, control=True, log=root / "control.jsonl",
                              scratch_name="control")
        markers_seen = sorted(k for k, m in world["markers"].items() if m.exists())
    att = production["attestation"] or {}
    problems = []
    cli = production["cli"]
    if cli["exit_code"] != 0 or cli["killed"]:
        problems.append("production_session_failed")
    if not production["bodies"]:
        problems.append("no_request_captured")
    if production["leaked"]:
        problems.append("sentinels_in_production_request")
    if markers_production:
        problems.append("hook_or_extra_server_ran_in_production")
    structural = rg.init_problems(production["attestation"], None)
    problems += [f"init_{p}" for p in structural]
    if not control["leaked"]:
        problems.append("control_blind")
    return {
        "schema": rg.PROOF_SCHEMA, "passed": not problems, "problems": problems,
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "claude_code_version": att.get("claude_code_version"), "model": att.get("model"),
        "requested_model": args.model, "tools": att.get("tools") or [],
        "plugins": att.get("plugins") or [], "agents": att.get("agents") or [],
        "memory_keys": att.get("memory_keys") or [],
        "canary_kinds": list(KINDS), "production_leaked_kinds": production["leaked"],
        "control_leaked_kinds": control["leaked"], "bodies": len(production["bodies"]),
        "body_bytes": sum(len(b) for b in production["bodies"]),
        "markers_after_production": markers_production, "markers_after_control": markers_seen,
        "differences_from_production": [
            "CLAUDE_CONFIG_DIR is the canary's own directory", "ANTHROPIC_BASE_URL is a local stand-in",
            "ANTHROPIC_API_KEY is a dummy", "no real API call was made"],
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--exe", type=Path, required=True, help="the ommatix binary (serves --source sim)")
    parser.add_argument("--model", required=True)
    parser.add_argument("--out", type=Path, required=True,
                        help="where to write the proof (absolute, outside the repository)")
    parser.add_argument("--claude", default="claude")
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args(argv)
    args.exe = rg.absolute_arg(args.exe)
    args.out = rg.absolute_arg(args.out)
    if (problem := proxy_module.path_problem(args.out)):
        parser.error(f"--out: {problem}")
    proof = run(args)
    args.out.write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps({k: proof[k] for k in ("passed", "problems", "claude_code_version", "model",
                                           "production_leaked_kinds", "control_leaked_kinds")}))
    return 0 if proof["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
