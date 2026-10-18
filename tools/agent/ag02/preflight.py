#!/usr/bin/env python3
"""AG-02 isolation preflight: a canary that proves what reaches the agent's context.

Run it once per Claude Code version and per controlled configuration directory, before any
session that shows the agent a screen; the runner refuses to start a goal without the proof file
it writes (``--isolation-proof``) or with a different configuration directory, model or version.

It makes two sessions of the exact production command (``run_goal.build_claude_argv``: ``--tools
""``, ``--strict-mcp-config``, ``--setting-sources ""``; the production MCP config, the proxy in
front of ``ommatix serve --source sim``) against a local stand-in for the Anthropic endpoint
(``fake_api.py``, 127.0.0.1) that records the request bodies:

* **A. the canary.** A synthetic world of harmless sentinels in a temporary directory outside the
  repository: a user CLAUDE.md, user rules, an auto-memory index, a skill, a command, an agent,
  hooks (which would write a marker file and print a sentinel if they ran), a second MCP server
  (which would write a marker if it started), ancestor and project CLAUDE.md files, a local
  CLAUDE.md, project settings, a project command and a project ``.mcp.json``. It passes only
  when no sentinel appears in any body, no hook marker and no extra-server marker exists and
  ``init`` lists exactly the three Ommatix tools and the one connected server. A positive control
  (settings sources on, the CLAUDE.md switch off) must leak at least one sentinel, or the canary
  is blind and the preflight fails.
* **B. the production configuration directory** (``--claude-config-dir``, the directory the goals
  will use). The same command runs with ``CLAUDE_CONFIG_DIR`` set to it. The proof records its
  identity and a manifest of the context-bearing items it holds (hashes only), and the preflight
  fails if any distinctive line of those items (CLAUDE.md, rules, memory) appears in the request.

No real provider is reached, by construction and by three controls rather than by assumption: the
child's environment is a minimal allowlist (no provider selector, endpoint, credential, profile,
proxy or telemetry variable of the operator's reaches it; the only credential is a dummy key for
the loopback stand-in); error reporting and auto-update are switched off; and the preflight refuses
to run when provider-routing variables are set in its own environment or a managed (policy)
configuration is found. It does not block the network: an outgoing-proxy block and the telemetry
switches were tried and rejected, because each removed `execute_action` from the CLI's tool list. What it cannot see
(for example a policy delivered some other way) is not covered, and the proof says so.

Differences from a production session, stated rather than hidden: stage A's configuration
directory is the canary's own; the endpoint is the stand-in; the key is a dummy.
"""

from __future__ import annotations

import argparse
import json
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
EGRESS_CONTROLS = [
    "child environment is a minimal allowlist: no provider selector, endpoint, credential, profile "
    "or telemetry variable of the operator's reaches it",
    "error reporting and auto-update are switched off; telemetry switches and an outgoing-proxy "
    "block were tried and rejected because each removed `execute_action` from the CLI's tool list",
    "refused to run with provider-routing variables set or a managed configuration found "
    "(best effort: policy delivered another way is not detected)",
]


class PreflightRefused(Exception):
    """The environment cannot honestly be called local; ``reasons`` are closed tokens or names."""

    def __init__(self, reasons):
        super().__init__("; ".join(reasons))
        self.reasons = list(reasons)


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


def decoded_text(body: bytes) -> str | None:
    """Every string of a captured request, decoded - keys and values, at any depth - joined by
    newlines: what the recipient reads, whatever escaping the serialisation used (Unicode escapes,
    escaped quotes and backslashes). ``None`` when the body is not JSON: an unreadable request
    cannot be certified clean."""
    try:
        value = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return None
    out: list[str] = []

    def walk(node):
        if isinstance(node, str):
            out.append(node)
        elif isinstance(node, dict):
            for key, item in node.items():
                out.append(str(key))
                walk(item)
        elif isinstance(node, list):
            for item in node:
                walk(item)
    walk(value)
    return "\n".join(out)


def request_texts(bodies: list[bytes]) -> tuple[str, int]:
    """(the decoded text of all the requests, how many could not be read)."""
    texts, unreadable = [], 0
    for body in bodies:
        text = decoded_text(body)
        if text is None:
            unreadable += 1
        else:
            texts.append(text)
    return "\n".join(texts), unreadable


def leaked(bodies: list[bytes], sentinels: dict) -> list[str]:
    raw = b"".join(bodies)
    decoded, _ = request_texts(bodies)
    return sorted(kind for kind, text in sentinels.items()
                  if text.encode("ascii") in raw or text in decoded)


USABLE_LINE = 16
SNIPPET_LINES = 50
MIN_WHOLE_FILE = 6


def context_snippets(config_dir: Path) -> tuple[list[str], list[str]]:
    """(distinctive lines of the context items in a configuration directory - CLAUDE.md, rules,
    memory - held in memory only, to look for in the decoded requests; the names of non-empty
    files that yield nothing testable). A file with no line of ``USABLE_LINE`` characters is
    tested by its whole stripped text when that has at least ``MIN_WHOLE_FILE`` characters;
    shorter than that it is reported, never counted as tested absence."""
    files = [config_dir / "CLAUDE.md", *sorted((config_dir / "rules").rglob("*.md")),
             *sorted(config_dir.glob("projects/*/memory/**/*.md"))]
    snippets: list[str] = []
    untestable: list[str] = []
    for file in files:
        if not file.is_file():
            continue
        text = file.read_text(encoding="utf-8", errors="replace")
        if not text.strip():
            continue
        lines = [ln.strip() for ln in text.splitlines() if len(ln.strip()) >= USABLE_LINE]
        if lines:
            snippets += lines[:SNIPPET_LINES]
        elif len(text.strip()) >= MIN_WHOLE_FILE:
            snippets.append(text.strip())
        else:
            untestable.append(file.name)
    return snippets, untestable


def one_session(args, config_dir: Path, cwd: Path, *, control: bool, log: Path,
                scratch_name: str, memory_dir: Path | None) -> dict:
    """One session. Production flags, or (control) the settings sources on and the CLAUDE.md
    switch off. `memory_dir` is the directory init's memory paths must lie under, if checked."""
    goals = rg.load_goals()
    mcp = rg.build_mcp_config(sys.executable, ["--dry-run-only", "--log", str(log)],
                              [str(args.exe), "serve", "--source", "sim", "--legacy"])
    mcp_path = log.with_name(scratch_name + ".mcp.json")
    mcp_path.write_text(json.dumps(mcp), encoding="utf-8")
    argv = rg.build_claude_argv(claude=args.claude, model=args.model, mcp_config=mcp_path,
                                system_instruction=goals["system_instruction"], max_budget_usd=0.5)
    with FakeApi() as api:
        env = rg.scrubbed_env({"CLAUDE_CONFIG_DIR": str(config_dir), "ANTHROPIC_BASE_URL": api.url,
                               "ANTHROPIC_API_KEY": "sk-ant-canary-0000000000",
                               **getattr(args, "extra_env", {})})
        if control:
            argv[argv.index("--setting-sources") + 1] = "user,project,local"
            env.pop("CLAUDE_CODE_DISABLE_CLAUDE_MDS", None)
        metrics = rg.StreamMetrics([], None, None, config_dir=memory_dir)
        cli = rg.run_claude(argv, rg.build_prompt({"prompt": "Say hi."}, 1), cwd, metrics, args.timeout,
                            env=env)
        bodies = list(api.bodies)
    return {"cli": cli, "metrics": metrics, "bodies": bodies, "attestation": metrics.attestation}


def refusals() -> list[str]:
    """Why this environment cannot honestly be called local (closed names, never values)."""
    import os
    return [f"routing_variable:{name}" for name in rg.routing_problems(os.environ)] + \
        rg.managed_config_found()


def run(args) -> dict:
    """The whole preflight; returns the proof (``passed`` says whether it holds). Raises
    ``PreflightRefused`` when the environment cannot be called local."""
    if (reasons := refusals()):
        raise PreflightRefused(reasons)
    production_dir = Path(args.claude_config_dir)
    problems = []
    with tempfile.TemporaryDirectory(prefix="ag02-canary-") as tmp:
        root = Path(tmp)
        world = build_world(root)
        a = one_session(args, world["home"], world["cwd"], control=False, log=root / "a.jsonl",
                        scratch_name="a", memory_dir=None)
        markers_production = sorted(k for k, m in world["markers"].items() if m.exists())
        for marker in world["markers"].values():
            marker.unlink(missing_ok=True)  # the control run may start them; start clean
        control = one_session(args, world["home"], world["cwd"], control=True, log=root / "c.jsonl",
                              scratch_name="c", memory_dir=None)
        markers_seen = sorted(k for k, m in world["markers"].items() if m.exists())
        a_leaks, control_leaks = leaked(a["bodies"], world["sentinels"]), leaked(
            control["bodies"], world["sentinels"])
        # B: the directory the goals will use, in a fresh empty working directory.
        scratch_b = root / "b" / "scratch"
        scratch_b.mkdir(parents=True)
        b = one_session(args, production_dir, scratch_b, control=False, log=root / "b.jsonl",
                        scratch_name="b", memory_dir=production_dir)
        b_snippets, b_untestable = context_snippets(production_dir)
        b_text, b_unreadable = request_texts(b["bodies"])
        b_leaked_lines = sum(1 for snippet in b_snippets if snippet in b_text)
        a_unreadable = request_texts(a["bodies"])[1]
        ancestors = rg.ancestor_context(scratch_b, production_dir)
    att_a, att_b = a["attestation"] or {}, b["attestation"] or {}
    for name, run_ in (("canary_session", a), ("production_dir_session", b)):
        if run_["cli"]["exit_code"] != 0 or run_["cli"]["killed"]:
            problems.append(f"{name}_failed")
        if not run_["bodies"]:
            problems.append(f"{name}_no_request_captured")
    if a_leaks:
        problems.append("sentinels_in_production_request")
    if markers_production:
        problems.append("hook_or_extra_server_ran_in_production")
    problems += [f"init_{p}" for p in rg.init_problems(a["attestation"], None)]
    problems += [f"production_dir_init_{p}" for p in rg.init_problems(b["attestation"], None, production_dir)]
    if b_leaked_lines:
        problems.append("production_dir_context_in_request")
    if b_untestable:
        problems.append("production_dir_context_file_untestable")
    if b_unreadable or a_unreadable:
        problems.append("request_unreadable")
    if ancestors:
        problems.append("ancestor_context_present")
    if not control_leaks:
        problems.append("control_blind")
    for key in ("claude_code_version", "model", "tools", "plugin_hashes", "agent_hashes", "init_keys_sha256"):
        if att_a.get(key) != att_b.get(key):
            problems.append(f"canary_and_production_dir_differ_in_{key}")
    try:
        manifest = rg.config_manifest(production_dir)
    except rg.ManifestRefused as refused:
        manifest = {}
        problems.append(f"context_manifest_refused_{refused}")
    return {
        "schema": rg.PROOF_SCHEMA, "verifier_revision": rg.VERIFIER_REVISION,
        "passed": not problems, "problems": problems,
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "claude_code_version": att_b.get("claude_code_version"), "model": att_b.get("model"),
        "requested_model": args.model, "tools": att_b.get("tools") or [],
        "plugins": att_b.get("plugins") or [], "agents": att_b.get("agents") or [],
        "plugin_hashes": att_b.get("plugin_hashes") or [], "agent_hashes": att_b.get("agent_hashes") or [],
        "init_keys_sha256": att_b.get("init_keys_sha256") or "",
        "config_identity": rg.config_identity(production_dir),
        "context_manifest": manifest,
        "canary_kinds": list(KINDS), "production_leaked_kinds": a_leaks,
        "control_leaked_kinds": control_leaks, "bodies": len(a["bodies"]),
        "body_bytes": sum(len(x) for x in a["bodies"]),
        "production_dir_context_lines_checked": len(b_snippets),
        "production_dir_context_files_untestable": len(b_untestable),
        "production_dir_context_lines_in_request": b_leaked_lines,
        "markers_after_production": markers_production, "markers_after_control": markers_seen,
        "egress_controls": EGRESS_CONTROLS,
        "differences_from_production": [
            "stage A uses the canary's own configuration directory", "ANTHROPIC_BASE_URL is a local stand-in",
            "ANTHROPIC_API_KEY is a dummy"],
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("--exe", type=Path, required=True, help="the ommatix binary (serves --source sim)")
    parser.add_argument("--model", required=True, help="the model the goals will be run with")
    parser.add_argument("--claude-config-dir", type=Path, required=True,
                        help="the controlled configuration directory the goals will use (must exist)")
    parser.add_argument("--out", type=Path, required=True,
                        help="where to write the proof (absolute, outside the repository)")
    parser.add_argument("--claude", default="claude")
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args(argv)
    args.exe = rg.absolute_arg(args.exe)
    args.out = rg.absolute_arg(args.out)
    args.claude_config_dir = rg.absolute_arg(args.claude_config_dir)
    for name in ("out", "claude_config_dir"):
        if (problem := proxy_module.path_problem(getattr(args, name))):
            parser.error(f"--{name.replace('_', '-')}: {problem}")
    if not args.claude_config_dir.is_dir():
        parser.error("--claude-config-dir must be an existing directory")
    try:
        proof = run(args)
    except PreflightRefused as refused:
        print(json.dumps({"passed": False, "refused": refused.reasons}))
        return 2
    args.out.write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps({k: proof[k] for k in ("passed", "problems", "claude_code_version", "model",
                                           "production_leaked_kinds", "control_leaked_kinds")}))
    return 0 if proof["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
