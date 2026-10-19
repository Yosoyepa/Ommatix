#!/usr/bin/env python3
"""AG-02 preflights for the agents other than Claude Code (ADR-0037).

Every verdict is read off the **real request** the agent sends to a stand-in on 127.0.0.1
(``fake_api.py``, which keeps the raw body and the header names), never off what a CLI says about
itself. No provider is reached: the endpoint is the stand-in and the key is a dummy.

``--agent-cli minimal`` (the confinable path)
    Runs ``minimal_agent.py`` through the budget proxy in front of ``ommatix serve --source sim`` in a
    synthetic world of harmless sentinels (instruction files in the working directory and its
    ancestors, in a stand-in home, a skill, dotenv and provider-key variables in the *parent*
    environment) and proves, request by request: the body carries exactly the fixed keys, the
    model, the fixed system instruction and the goal prompt, then only assistant and tool messages;
    the tools are exactly the three Ommatix definitions, the same in every request; no content part
    is anything but text; no sentinel and no key is in any body; the only headers are the expected
    ones, the key in exactly one; the only path is ``<base path>/chat/completions``. A positive
    control feeds the verifier mutated copies of a captured request (an extra tool, a sentinel, an
    image part, an extra key, another model) and fails the preflight if any goes unflagged. The proof
    binds the agent's source hash, version, provider, endpoint, model, request parameters,
    instruction and tool definitions; the runner refuses a run that differs from it. A static check
    of the agent's source (import allowlist, no ``eval``/``exec``/``open``) is recorded with it.

``--agent-cli codex`` and ``--agent-cli opencode`` (probes that fail closed)
    Build the strictest configuration found for the installed version (isolated home, every
    switch that exists), run the real CLI against the stand-in with the Ommatix MCP server and read
    the tool list and the context in the request. A tool beyond the three Ommatix ones is
    ``tool_surface_wider_than_three``; a skills block in the request is ``skills_from_real_home``.
    The probe's file has ``passed: false`` while any problem remains, and the runner refuses it.
"""

from __future__ import annotations

import argparse
import ast
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import agentcli  # noqa: E402
import minimal_agent as ma  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402
from fake_api import FakeApi  # noqa: E402

KINDS = ("cwd_claude_md", "cwd_agents_md", "cwd_local_md", "ancestor_claude_md", "ancestor_agents_md",
         "home_claude_md", "home_agents_md", "home_skill", "dotenv", "env_openai_key", "env_deepseek_key",
         "env_mimo_key", "env_anthropic_key", "env_proxy")
ENV_KINDS = {"env_openai_key": "OPENAI_API_KEY", "env_deepseek_key": "DEEPSEEK_API_KEY",
             "env_mimo_key": "MIMO_API_KEY", "env_anthropic_key": "ANTHROPIC_API_KEY"}
ALLOWED_HEADERS = frozenset({"host", "content-type", "content-length", "accept-encoding", "user-agent",
                             "connection"})
ALLOWED_IMPORTS = frozenset({"__future__", "argparse", "hashlib", "json", "math", "os", "re", "subprocess", "sys",
                             "time", "urllib", "pathlib"})
FORBIDDEN_CALLS = frozenset({"eval", "exec", "compile", "__import__", "open", "input"})
DUMMY_KEY = "sk-canary-0000000000"
PROBE_SCRIPT = [{"tool": "observe_screen", "args": {}}, {"text": "VERDICT: DONE"}]
EGRESS_CONTROLS = [
    "the agent's endpoint is the stand-in (loopback) and its key a dummy; no provider is reached",
    "the child's environment is the operating system's basics only (no key, proxy or provider variable)",
    "the agent follows no redirect and honours no proxy variable (source hash bound to the proof)",
]


class Refused(Exception):
    """The environment cannot honestly be called local; ``reasons`` are closed tokens."""

    def __init__(self, reasons):
        super().__init__("; ".join(reasons))
        self.reasons = list(reasons)


# ---- the synthetic world -------------------------------------------------------------------

def build_world(root: Path) -> dict:
    """Sentinels in files the agent could read if it read anything, and in environment variables it
    would inherit if its environment were not scrubbed. Returns the sentinels by kind, the working
    directory, the stand-in home and the environment to put in the *parent* process."""
    token = secrets.token_hex(6)
    sent = {kind: f"CANARY-{kind.upper().replace('_', '-')}-{token}" for kind in KINDS}
    home = root / "home"
    work = root / "work"
    cwd = work / "project" / "scratch"
    for d in (home / ".claude", home / ".codex", home / ".agents" / "skills" / "canary-skill", cwd):
        d.mkdir(parents=True, exist_ok=True)
    files = {work / "CLAUDE.md": "ancestor_claude_md", work / "AGENTS.md": "ancestor_agents_md",
             cwd / "CLAUDE.md": "cwd_claude_md", cwd / "AGENTS.md": "cwd_agents_md",
             cwd / "CLAUDE.local.md": "cwd_local_md", cwd / ".env": "dotenv",
             home / ".claude" / "CLAUDE.md": "home_claude_md", home / ".codex" / "AGENTS.md": "home_agents_md",
             home / ".agents" / "skills" / "canary-skill" / "SKILL.md": "home_skill"}
    for path, kind in files.items():
        path.write_text(sent[kind] + "\n", encoding="utf-8")
    env = {name: sent[kind] for kind, name in ENV_KINDS.items()}
    env["HTTPS_PROXY"] = "http://127.0.0.1:9/" + sent["env_proxy"]
    return {"sentinels": sent, "cwd": cwd, "home": home, "env": env}


def basic_env(extra: dict | None = None) -> dict:
    """The operating system's basics from the current environment and nothing else."""
    wanted = {b.upper() for b in rg.BASIC_ENV}
    return {k: v for k, v in os.environ.items() if k.upper() in wanted} | (extra or {})


# ---- the minimal agent: static evidence and the request verifier ------------------------------

def static_problems(source: str) -> list[str]:
    """What the agent's source must not contain, judged on its syntax tree (best-effort evidence
    that it has no capability beyond its one endpoint and the proxy; the source hash binds it)."""
    tree = ast.parse(source)
    problems = []
    imports = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imports |= {a.name.split(".")[0] for a in node.names}
        elif isinstance(node, ast.ImportFrom):
            imports.add((node.module or "").split(".")[0])
        elif isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in FORBIDDEN_CALLS:
            problems.append(f"forbidden_call_{node.func.id}")
        elif isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name):
            if node.value.id == "os" and node.attr not in ("path", "fspath"):
                problems.append(f"os_{node.attr}")
            if node.value.id == "subprocess" and node.attr not in ("Popen", "PIPE", "DEVNULL", "TimeoutExpired"):
                problems.append(f"subprocess_{node.attr}")
    problems += [f"import_{name}" for name in sorted(imports - ALLOWED_IMPORTS)]
    return problems


def _text_of(content) -> str | None:
    return content if isinstance(content, str) else None


def decoded_strings(value):
    """Every string of a decoded JSON value, dictionary keys included, at any depth: what the
    recipient reads, whatever escaping the serialisation used (the PR-93 review: ``\\u0043`` hides a
    sentinel from a search of the raw bytes)."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for key, item in value.items():
            yield str(key)
            yield from decoded_strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from decoded_strings(item)


def check_chat_requests(requests: list[dict], *, model: str, expected_extra: dict, ceiling: int,
                        system_instruction: str, prompt: str, sentinels: dict, dummy_key: str,
                        auth_style: str, base_path: str) -> tuple[list[str], dict]:
    """(problems as closed tokens, facts) for the Chat Completions requests the minimal agent sent.
    Each request is ``{"path", "headers", "body"}``; bodies are bytes. `expected_extra` is the full
    parameter object the operator asked for: every configured value must be in every request, equal and
    of the same type (the proof is bound to the values the requests carried, not the ones requested)."""
    problems: list[str] = []
    facts: dict = {"requests": len(requests)}
    if not requests:
        return ["no_request_captured"], facts
    auth_name = "authorization" if auth_style == "bearer" else "api-key"
    expected_headers = ALLOWED_HEADERS | {auth_name}
    expected_key = f"Bearer {dummy_key}" if auth_style == "bearer" else dummy_key
    want_keys = set(ma.BASE_KEYS) | set(expected_extra)
    tool_hashes, previous, request_keys, extra_hashes = set(), None, None, set()
    for request in requests:
        raw = request["body"]
        if request["path"] != base_path.rstrip("/") + "/chat/completions":
            problems.append("request_path_differs")
        names = set(request["headers"])
        if names - expected_headers:
            problems.append("header_not_expected")
        if request["headers"].get(auth_name) != expected_key:
            problems.append("key_header_differs")
        # The key may appear in exactly one header, as exactly the expected value, and nowhere else.
        if any(dummy_key in value for name, value in request["headers"].items()
               if name != auth_name or value != expected_key):
            problems.append("key_in_another_header")
        raw_text = raw.decode("utf-8", "replace")
        header_text = "\n".join(v for n, v in request["headers"].items() if n != auth_name)
        if any(text in raw_text or text in header_text for text in sentinels.values()):
            problems.append("sentinel_in_request")
        if dummy_key.encode() in raw:
            problems.append("key_in_body")
        try:
            body = json.loads(raw)
        except ValueError:
            problems.append("request_unreadable")
            continue
        if not isinstance(body, dict):
            problems.append("request_unreadable")
            continue
        decoded = "\n".join(decoded_strings(body))
        if any(text in decoded for text in sentinels.values()):
            problems.append("sentinel_in_request")
        if dummy_key in decoded:
            problems.append("key_in_body")
        request_keys = sorted(body)
        if set(body) != want_keys:
            problems.append("body_keys_differ")
        if body.get("model") != model:
            problems.append("model_differs")
        if body.get("stream") is not False or body.get("tool_choice") != "auto":
            problems.append("stream_or_tool_choice_differs")
        wanted = {**expected_extra, "max_tokens": ceiling}
        if any(k not in body or agentcli.canonical_sha(body[k]) != agentcli.canonical_sha(v)
               for k, v in wanted.items()):
            problems.append("param_values_differ")
        else:
            extra_hashes.add(agentcli.canonical_sha({k: body[k] for k in expected_extra}))
        tools = body.get("tools")
        if not (isinstance(tools, list) and [t.get("function", {}).get("name") for t in tools if isinstance(t, dict)]
                == list(ma.TOOLS) and all(isinstance(t, dict) and t.get("type") == "function"
                                          and set(t) == {"type", "function"} for t in tools)):
            problems.append("tools_not_the_three")
        else:
            tool_hashes.add(agentcli.canonical_sha(tools))
        messages = body.get("messages")
        if not (isinstance(messages, list) and len(messages) >= 2):
            problems.append("messages_unreadable")
            continue
        if messages[0] != {"role": "system", "content": system_instruction} \
                or messages[1] != {"role": "user", "content": prompt}:
            problems.append("opening_messages_differ")
        for message in messages[2:]:
            if not isinstance(message, dict) or message.get("role") not in ("assistant", "tool") \
                    or set(message) - set(ma.MESSAGE_KEYS):
                problems.append("message_not_allowed")
                continue
            if message.get("content") is not None and _text_of(message.get("content")) is None:
                problems.append("non_text_content")
            for call in message.get("tool_calls") or []:
                if not (isinstance(call, dict) and call.get("type") == "function"
                        and isinstance(call.get("function"), dict) and call["function"].get("name") in ma.TOOLS):
                    problems.append("tool_call_unknown_name")
        if previous is not None and messages[:len(previous)] != previous:
            problems.append("history_rewritten")
        previous = messages
    if len(tool_hashes) > 1:
        problems.append("tool_definitions_change")
    if len(extra_hashes) > 1:
        problems.append("param_values_change")
    facts |= {"tool_definitions_sha256": next(iter(tool_hashes)) if len(tool_hashes) == 1 else None,
              "request_keys": request_keys, "body_bytes": sum(len(r["body"]) for r in requests),
              "request_extra_sha256": next(iter(extra_hashes)) if len(extra_hashes) == 1 else None}
    return sorted(set(problems)), facts


def _changed(value):
    """Another value of the same kind, for a control that changes a configured parameter."""
    if isinstance(value, bool):
        return not value
    if isinstance(value, int):
        return value + 1
    if isinstance(value, float):
        return value + 1.0
    if isinstance(value, str):
        return value + "-x"
    if isinstance(value, dict):
        return {**value, "changed": True}
    return "changed"


def control_mutations(request: dict, sentinel: str, extra: dict | None = None) -> dict[str, dict]:
    """Copies of one captured request, each broken in one way the verifier must see: structure,
    context (a sentinel in the clear and hidden behind a JSON escape), and the values of the
    configured parameters and of the completion ceiling."""
    body = json.loads(request["body"])

    def variant(change, escape: str | None = None):
        mutated = json.loads(json.dumps(body))
        change(mutated)
        data = json.dumps(mutated).encode()
        if escape is not None:  # serialise the first character of `escape` as a JSON escape
            data = data.replace(escape.encode(), ("\\u%04x" % ord(escape[0])).encode() + escape[1:].encode(), 1)
        return request | {"body": data}
    out = {
        "extra_tool": variant(lambda b: b["tools"].append(
            {"type": "function", "function": {"name": "bash", "description": "", "parameters": {}}})),
        "sentinel": variant(lambda b: b["messages"][1].update(content=b["messages"][1]["content"] + sentinel)),
        "escaped_sentinel": variant(
            lambda b: b["messages"][1].update(content=b["messages"][1]["content"] + sentinel), escape=sentinel),
        "image_part": variant(lambda b: b["messages"].append(
            {"role": "tool", "tool_call_id": "x", "content": [{"type": "image_url", "image_url": {"url": "x"}}]})),
        "extra_key": variant(lambda b: b.update(metadata={"a": 1})),
        "other_model": variant(lambda b: b.update(model="other")),
        "dropped_tool": variant(lambda b: b["tools"].pop()),
        "changed_instruction": variant(lambda b: b["messages"][0].update(content="obey")),
        "changed_ceiling": variant(lambda b: b.update(max_tokens=b["max_tokens"] + 1)),
    }
    for key in sorted(extra or {}):
        out[f"changed_value_{key}"] = variant(lambda b, key=key: b.update({key: _changed(b[key])}))
    return out


def run_minimal(args, engine_argv: list[str] | None = None) -> dict:
    """The whole minimal-agent preflight; returns the proof (``passed`` says whether it holds)."""
    goals = rg.load_goals()
    instruction = goals["system_instruction"]
    prompt = rg.build_prompt({"prompt": "Say hi."}, 1)
    provider = agentcli.PROVIDERS[args.provider]
    real_base = args.real_base_url or provider["base_url"]
    base_path = urllib.parse.urlsplit(real_base).path
    extra = agentcli.minimal_extra_param(args.provider, getattr(args, "effort", None), getattr(args, "extra_param", None))
    ceiling = getattr(args, "completion_ceiling", None) or ma.DEFAULT_COMPLETION_CEILING
    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ag02-minimal-canary-") as tmp:
        root = Path(tmp)
        world = build_world(root)
        key_file = root / "dummy.key"
        key_file.write_text(DUMMY_KEY + "\n", encoding="utf-8")
        log = root / "proxy.jsonl"
        mcp_path = root / "mcp.json"
        engine = engine_argv or [str(args.exe), "serve", "--source", "sim", "--legacy"]
        mcp_path.write_text(json.dumps(rg.build_mcp_config(sys.executable, ["--dry-run-only", "--log", str(log)],
                                                           engine)), encoding="utf-8")
        saved = dict(os.environ)
        try:
            os.environ.update(world["env"])
            os.environ.update(USERPROFILE=str(world["home"]), HOME=str(world["home"]))
            with FakeApi(PROBE_SCRIPT) as api:
                ns = argparse.Namespace(provider=args.provider, base_url=api.url + base_path, key_file=key_file,
                                        effort=getattr(args, "effort", None), extra_param=getattr(args, "extra_param", None),
                                        max_total_tokens=400_000, price=None, phase=1,
                                        completion_ceiling=ceiling, max_usd_explicit=False)
                cli = agentcli.MinimalCLI(ns)
                argv = cli.build_argv(model=args.model, mcp_config=mcp_path, system_instruction=instruction,
                                      max_budget_usd=0.5)
                env = cli.env()
                metrics = rg.StreamMetrics([], None, None)
                ran = rg.run_claude(argv, prompt, world["cwd"], metrics, args.timeout, env=env)
                requests = list(api.requests)
                stray = [p for p in api.paths if not p.endswith("/chat/completions")] + list(api.gets)
        finally:
            os.environ.clear()
            os.environ.update(saved)
        found, facts = check_chat_requests(
            requests, model=args.model, expected_extra=extra, ceiling=ceiling, system_instruction=instruction,
            prompt=prompt, sentinels=world["sentinels"], dummy_key=DUMMY_KEY, auth_style=provider["auth_style"],
            base_path=base_path)
        problems += found
        if facts.get("request_extra_sha256") != agentcli.canonical_sha(extra):
            problems.append("param_hash_differs")
        if stray:
            problems.append("agent_reached_another_path")
        if ran["exit_code"] != 0 or ran["killed"]:
            problems.append("agent_session_failed")
        if len(requests) < 2:
            problems.append("fewer_than_two_requests")
        problems += [f"init_{p}" for p in rg.init_problems(metrics.attestation, None)]
        problems += [f"static_{p}" for p in static_problems(agentcli.MINIMAL_AGENT.read_text(encoding="utf-8"))]
        # Positive control: the verifier must see each broken variant of a real request.
        detected, blind = [], []
        if requests:
            for name, mutated in control_mutations(requests[0], world["sentinels"]["cwd_claude_md"], extra).items():
                found_here, _ = check_chat_requests(
                    [mutated], model=args.model, expected_extra=extra, ceiling=ceiling,
                    system_instruction=instruction, prompt=prompt, sentinels=world["sentinels"],
                    dummy_key=DUMMY_KEY, auth_style=provider["auth_style"], base_path=base_path)
                (detected if found_here else blind).append(name)
        if blind:
            problems.append("control_blind")
    att = metrics.attestation or {}
    return {
        "schema": agentcli.MINIMAL_PROOF_SCHEMA, "verifier_revision": agentcli.MINIMAL_VERIFIER_REVISION,
        "passed": not problems, "problems": sorted(set(problems)),
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "agent_sha256": __import__("hashlib").sha256(agentcli.MINIMAL_AGENT.read_bytes()).hexdigest(),
        "claude_code_version": att.get("claude_code_version"), "provider": args.provider, "base_url": real_base,
        "auth_style": provider["auth_style"], "model": att.get("model"), "requested_model": args.model,
        # The hash of the values the captured requests carried (cross-checked against the requested ones above).
        "extra_param_sha256": facts.get("request_extra_sha256"), "completion_ceiling": ceiling,
        "system_instruction_sha256": agentcli.sha256_text(instruction),
        "tools": att.get("tools") or [], "plugins": [], "agents": [], "plugin_hashes": [], "agent_hashes": [],
        "init_keys_sha256": att.get("init_keys_sha256") or "",
        "tool_definitions_sha256": facts.get("tool_definitions_sha256"), "request_keys": facts.get("request_keys"),
        "requests_checked": facts.get("requests"), "canary_kinds": list(KINDS), "control_detected": detected,
        "control_blind": blind, "egress_controls": EGRESS_CONTROLS,
        "differences_from_production": [
            "the endpoint is a stand-in on 127.0.0.1 (the path below the host is the production one)",
            "the key is a dummy", "the model's answers are scripted"],
    }


# ---- Codex and OpenCode: probes ---------------------------------------------------------------

def codex_surface(body: dict) -> list[str]:
    """Every tool the Codex request offers, from ``tools`` and from ``additional_tools`` input items
    (namespaces expanded), as names; nested ``exec`` declarations (code mode) are expanded too."""
    names: list[str] = []

    def walk(tool):
        if not isinstance(tool, dict):
            return
        kind, name = tool.get("type"), tool.get("name")
        if kind == "namespace":
            for inner in tool.get("tools", []):
                walk(inner | {"name": f"{name}.{inner.get('name')}"} if isinstance(inner, dict) else inner)
        else:
            names.append(str(name or kind))
            if name == "exec" and isinstance(tool.get("description"), str):
                names.extend(f"exec>{n}" for n in re.findall(r"### `(\w+)`", tool["description"]))
    for tool in body.get("tools", []):
        walk(tool)
    for item in body.get("input", []):
        if isinstance(item, dict) and item.get("type") == "additional_tools":
            for tool in item.get("tools", []):
                walk(tool)
    return sorted(set(names))


def codex_has_skills(body: dict) -> bool:
    for item in body.get("input", []):
        for part in item.get("content", []) if isinstance(item, dict) and isinstance(item.get("content"), list) else []:
            text = part.get("text", "") if isinstance(part, dict) else ""
            if "<skills_instructions>" in text and re.search(r"^- [\w.-]+: ", text, re.M):
                return True
    return False


def opencode_surface(body: dict) -> list[str]:
    """Function tools of the request, plus what the code-mode runtime reaches: the namespaces of its
    catalog and ``execute+fetch`` when the runtime can make HTTP requests."""
    names = []
    for tool in body.get("tools", []):
        function = tool.get("function", {}) if isinstance(tool, dict) else {}
        names.append(str(function.get("name")))
        if function.get("name") == "execute" and "fetch" in str(function.get("description")):
            names.append("execute+fetch")
    system = "\n".join(str(m.get("content")) for m in body.get("messages", []) if m.get("role") == "system")
    names += [f"catalog:{ns}" for ns, _ in re.findall(r"^- (\S+) \((\d+) tools", system, re.M)]
    return sorted(set(names))


def opencode_has_skills(body: dict) -> bool:
    system = "\n".join(str(m.get("content")) for m in body.get("messages", []) if m.get("role") == "system")
    return bool(re.search(r"<available_skills>\s*<skill>", system))


def probe_verdict(surface: list[str], skills: bool, allowed: set[str]) -> list[str]:
    extras = [n for n in surface if n not in allowed]
    problems = agentcli.surface_problems(extras, skills)
    if not allowed <= set(surface):
        problems.append("ommatix_tools_not_offered_directly")
    return problems


def toml_str(text: str) -> str:
    return "'" + text.replace("'", "") + "'"


def codex_config(port: int, model: str, catalog: Path, mcp_command: str, mcp_args: list[str]) -> str:
    """The strictest Codex configuration found for 0.163.0 (every switch that exists)."""
    return f'''model = "{model}"
model_provider = "local"
web_search = "disabled"
model_catalog_json = {toml_str(catalog.as_posix())}
[features]
shell_tool = false
unified_exec = false
multi_agent = false
goals = false
[tools.experimental_request_user_input]
enabled = false
[tools.update_plan]
enabled = false
[model_providers.local]
name = "local"
base_url = "http://127.0.0.1:{port}/v1"
wire_api = "responses"
env_key = "LOCAL_KEY"
[mcp_servers.ommatix]
command = {toml_str(Path(mcp_command).as_posix())}
args = [{", ".join(toml_str(a.replace(chr(92), "/")) for a in mcp_args)}]
'''


def opencode_config(port: int, model: str, mcp_command: list[str]) -> dict:
    """The strictest OpenCode configuration found for 2.0.x: every built-in action denied, the MCP
    server and code mode allowed, nothing else switched on."""
    return {"model": f"local/{model}", "small_model": f"local/{model}", "autoupdate": False, "update": "disable",
            "share": "disabled", "snapshots": False, "lsp": False, "formatter": False, "websearch": False,
            "provider": {"local": {"npm": "@ai-sdk/openai-compatible", "name": "local",
                                   "options": {"baseURL": f"http://127.0.0.1:{port}/v1", "apiKey": "dummy"},
                                   "models": {model: {"name": model, "limit": {"context": 100000, "output": 8000}}}}},
            "mcp": {"ommatix": {"type": "local", "command": mcp_command, "enabled": True}},
            "permission": {"*": "deny", "execute": "allow", "ommatix": "allow"}}


def cli_version(binary: str) -> str:
    try:
        out = subprocess.run([binary, "--version"], capture_output=True, timeout=60, env=basic_env()).stdout
    except (OSError, subprocess.SubprocessError):
        return "unknown"
    match = re.search(r"\d+\.\d+\.\d+", out.decode("utf-8", "replace"))
    return match.group(0) if match else "unknown"


def probe_codex(args, engine_argv: list[str] | None = None) -> dict:
    binary = shutil.which("codex")
    if binary is None:
        raise Refused(["codex_not_installed"])
    with tempfile.TemporaryDirectory(prefix="ag02-codex-probe-") as tmp:
        root = Path(tmp)
        home, work = root / "home", root / "work"
        home.mkdir()
        work.mkdir()
        env = basic_env({"CODEX_HOME": str(home), "LOCAL_KEY": DUMMY_KEY})
        models = subprocess.run([binary, "debug", "models"], capture_output=True, timeout=120, env=env).stdout
        entry = next((m for m in json.loads(models)["models"] if m["slug"] == args.model), None)
        if entry is None:
            raise Refused(["model_not_in_the_bundled_catalog"])
        # The bundled entry with the tool families that can be switched off in the catalog removed.
        entry = {**entry, "supports_parallel_tool_calls": False, "multi_agent_version": None,
                 "apply_patch_tool_type": None, "input_modalities": ["text"], "experimental_supported_tools": []}
        entry.pop("tool_mode", None)
        catalog = root / "catalog.json"
        catalog.write_text(json.dumps({"models": [entry]}), encoding="utf-8")
        engine = engine_argv or [str(args.exe), "serve", "--source", "sim", "--legacy"]
        mcp_args = [str(HERE / "proxy.py"), "--dry-run-only", "--log", str(root / "proxy.jsonl"), "--", *engine]
        with FakeApi() as api:
            port = api.server.server_address[1]
            (home / "config.toml").write_text(codex_config(port, args.model, catalog, sys.executable, mcp_args),
                                              encoding="utf-8")
            subprocess.run([binary, "exec", "--json", "--ephemeral", "--skip-git-repo-check", "-C", str(work), "-"],
                           input=b"Say hi.", capture_output=True, timeout=args.timeout, env=env)
            requests = [json.loads(r["body"]) for r in api.requests if r["path"].endswith("/responses")]
    if not requests:
        return probe_result("codex", cli_version(binary), args.model, ["no_request_captured"], [], 0)
    surface = sorted(set().union(*(codex_surface(b) for b in requests)))
    allowed = {f"mcp__{rg.SERVER_NAME}__{t}" for t in rg.TOOLS}
    problems = probe_verdict(surface, any(codex_has_skills(b) for b in requests), allowed)
    return probe_result("codex", cli_version(binary), args.model, problems, surface, len(requests))


def probe_opencode(args, engine_argv: list[str] | None = None) -> dict:
    binary = shutil.which("opencode.cmd") or shutil.which("opencode")
    if binary is None:
        raise Refused(["opencode_not_installed"])
    with tempfile.TemporaryDirectory(prefix="ag02-opencode-probe-") as tmp:
        root = Path(tmp)
        home, work = root / "home", root / "work"
        (home / ".config" / "opencode").mkdir(parents=True)
        work.mkdir()
        engine = engine_argv or [str(args.exe), "serve", "--source", "sim", "--legacy"]
        command = [sys.executable, str(HERE / "proxy.py"), "--dry-run-only", "--log", str(root / "proxy.jsonl"), "--", *engine]
        env = basic_env({"USERPROFILE": str(home), "HOME": str(home), "APPDATA": str(home / "AppData" / "Roaming"),
                         "LOCALAPPDATA": str(home / "AppData" / "Local")})
        with FakeApi() as api:
            port = api.server.server_address[1]
            (home / ".config" / "opencode" / "opencode.json").write_text(
                json.dumps(opencode_config(port, args.model, command)), encoding="utf-8")
            proc = subprocess.Popen([binary, "run", "--standalone", "--format", "json", "Say hi."], cwd=work, env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                proc.communicate(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)], capture_output=True)
                proc.communicate()
            requests = [json.loads(r["body"]) for r in api.requests if r["path"].endswith("/chat/completions")]
    main_requests = [b for b in requests if b.get("tools")]
    if not main_requests:
        return probe_result("opencode", cli_version(binary), args.model, ["no_request_captured"], [], len(requests))
    surface = sorted(set().union(*(opencode_surface(b) for b in main_requests)))
    allowed = {f"ommatix_{t}" for t in rg.TOOLS}
    problems = probe_verdict(surface, any(opencode_has_skills(b) for b in main_requests), allowed)
    return probe_result("opencode", cli_version(binary), args.model, problems, surface, len(requests))


def probe_result(cli: str, version: str, model: str, problems: list[str], surface: list[str], requests: int) -> dict:
    return {"schema": agentcli.CLI_PROOF_SCHEMA, "agent_cli": cli, "cli_version": version, "model": model,
            "passed": not problems, "problems": sorted(set(problems)), "tool_surface": surface,
            "requests_seen": requests, "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "egress_controls": EGRESS_CONTROLS[:2]}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("--agent-cli", choices=("minimal", "codex", "opencode"), required=True)
    parser.add_argument("--exe", type=Path, required=True, help="the ommatix binary (serves --source sim)")
    parser.add_argument("--model", help="the model the goals will be run with (default: the provider's)")
    parser.add_argument("--provider", choices=sorted(agentcli.PROVIDERS), help="minimal agent only")
    parser.add_argument("--effort", choices=("max",))
    parser.add_argument("--extra-param")
    parser.add_argument("--real-base-url", help="minimal agent: a different production base URL to bind (default: the provider's)")
    parser.add_argument("--out", type=Path, required=True, help="where to write the proof (absolute, outside the repository)")
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args(argv)
    args.exe, args.out = rg.absolute_arg(args.exe), rg.absolute_arg(args.out)
    if (problem := proxy_module.path_problem(args.out)):
        parser.error(f"--out: {problem}")
    if args.extra_param is not None:
        args.extra_param = json.loads(args.extra_param)
    try:
        if args.agent_cli == "minimal":
            if not args.provider:
                parser.error("--agent-cli minimal needs --provider")
            args.model = args.model or agentcli.PROVIDERS[args.provider]["default_model"]
            proof = run_minimal(args)
        else:
            if not args.model:
                parser.error(f"--agent-cli {args.agent_cli} needs --model")
            proof = (probe_codex if args.agent_cli == "codex" else probe_opencode)(args)
    except Refused as refused:
        print(json.dumps({"passed": False, "refused": refused.reasons}))
        return 2
    args.out.write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps({k: proof[k] for k in ("passed", "problems") if k in proof}
                     | ({"tool_surface": proof["tool_surface"]} if "tool_surface" in proof else {})))
    return 0 if proof["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
