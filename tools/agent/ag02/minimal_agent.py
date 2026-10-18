#!/usr/bin/env python3
"""AG-02 minimal agent: a confinable tool-calling loop of our own, for providers whose CLIs cannot
be confined to the three Ommatix tools (ADR-0037).

It speaks the OpenAI-compatible Chat Completions API to ONE configured endpoint and offers the model
EXACTLY the three Ommatix tools, bridged over MCP stdio to the budget proxy (``proxy.py``) given in
``--mcp-config``. It has no other capability by construction: no shell, file, web or image code
exists in it, no skill, plugin, memory or instruction file is read, the request carries only the
system instruction, the goal prompt (stdin), the conversation so far and the three tool
definitions, and the only network peer is the endpoint (no redirect is followed, no proxy variable
is honoured, a non-loopback endpoint must be https).

Output: one JSON object per line on stdout in the stream shape the runner already reads (the
``init`` / ``assistant`` / ``user`` / ``result`` events of ``run_goal.StreamMetrics``), so the
runner, its scoring and its metrics are the same for every agent. stderr carries metadata only
(HTTP status, byte sizes, token counts), never a body, a label or a key. The API key is read from
``--key-file`` (outside the repository), sent in one header and never printed.

Standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

AGENT_VERSION = "1.0.0"
SERVER_NAME = "ommatix"
TOOLS = ("observe_screen", "execute_action", "get_engine_metrics")
LOOPBACK = ("127.0.0.1", "localhost", "::1")
# The only body keys the request may ever carry. ``--extra-param`` may add the sampling and reasoning
# parameters below and nothing else; the model's identity, the messages and the tools are not
# configurable that way.
BASE_KEYS = ("model", "messages", "tools", "tool_choice", "stream")
EXTRA_KEYS = ("reasoning_effort", "thinking", "max_tokens", "max_completion_tokens", "temperature", "top_p")
AUTH_STYLES = ("bearer", "api-key")
MESSAGE_KEYS = ("role", "content", "tool_calls", "tool_call_id", "reasoning_content")
MAX_RESULT_CHARS = 200_000


class AgentError(Exception):
    """A failure with a closed token; the token is what reaches the stream, never a body."""

    def __init__(self, token: str):
        super().__init__(token)
        self.token = token


def emit(event: dict) -> None:
    sys.stdout.write(json.dumps(event, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def note(text: str) -> None:
    sys.stderr.write(text + "\n")
    sys.stderr.flush()


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def endpoint_problem(url: str) -> str | None:
    """Why `url` cannot be the endpoint: only https, or http to a loopback address (the preflight's
    stand-in), with no credentials in it and no query or fragment."""
    parts = urllib.parse.urlsplit(url)
    if parts.username or parts.password or parts.query or parts.fragment:
        return "endpoint_has_credentials_query_or_fragment"
    if parts.scheme == "https" and parts.hostname:
        return None
    if parts.scheme == "http" and parts.hostname in LOOPBACK:
        return None
    return "endpoint_must_be_https_or_loopback"


def extra_param_problem(params) -> str | None:
    if not isinstance(params, dict):
        return "extra_param_not_an_object"
    for key in params:
        if key not in EXTRA_KEYS:
            return "extra_param_key_not_allowed"
    return None


def load_key(path: Path) -> str:
    try:
        key = path.read_text(encoding="utf-8").strip()
    except OSError:
        raise AgentError("key_file_unreadable") from None
    if not key or "\n" in key or "\r" in key or any(ord(c) < 0x20 for c in key):
        raise AgentError("key_file_malformed")
    return key


class McpBridge:
    """Line-delimited JSON-RPC over the proxy's stdio (legacy profile: ``initialize``)."""

    def __init__(self, command: list[str], env: dict | None = None):
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, env=env)
        self.next_id = 1

    def _send(self, message: dict) -> None:
        self.proc.stdin.write((json.dumps(message) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method: str, params: dict) -> dict:
        rid, self.next_id = self.next_id, self.next_id + 1
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        line = self.proc.stdout.readline()
        if not line:
            raise AgentError("mcp_server_closed")
        try:
            reply = json.loads(line)
        except ValueError:
            raise AgentError("mcp_reply_unreadable") from None
        if not isinstance(reply, dict) or reply.get("id") != rid:
            raise AgentError("mcp_reply_id_mismatch")
        return reply

    def start(self) -> list[dict]:
        reply = self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                            "clientInfo": {"name": "ag02-minimal-agent",
                                                           "version": AGENT_VERSION}})
        if "result" not in reply:
            raise AgentError("mcp_initialize_failed")
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listed = self.request("tools/list", {})
        tools = (listed.get("result") or {}).get("tools")
        if not isinstance(tools, list):
            raise AgentError("mcp_tools_list_unreadable")
        return tools

    def call(self, name: str, arguments: dict) -> dict:
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self) -> None:
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def tool_definitions(listed: list[dict]) -> tuple[list[dict], list[str]]:
    """(the three chat tool definitions, the names the server listed). The agent offers a tool only
    if the server listed it; a server that lists anything but the three is reported through the
    names and the init event, and the runner ends the session."""
    names = sorted(str(t.get("name")) for t in listed if isinstance(t, dict))
    defs = []
    for tool in sorted((t for t in listed if isinstance(t, dict) and t.get("name") in TOOLS),
                       key=lambda t: TOOLS.index(t["name"])):
        schema = tool.get("inputSchema") if isinstance(tool.get("inputSchema"), dict) else {
            "type": "object", "properties": {}}
        defs.append({"type": "function", "function": {
            "name": tool["name"], "description": str(tool.get("description") or ""), "parameters": schema}})
    return defs, names


def result_text(reply: dict) -> tuple[str, bool]:
    """(the text the model reads for one MCP reply, is_error). Only text is forwarded: an image or
    any other block is replaced by a fixed refusal, so no pixel can reach the model."""
    if "error" in reply:
        return json.dumps({"error": "rpc_error"}), True
    result = reply.get("result") or {}
    content = result.get("content")
    texts, refused = [], False
    if isinstance(content, list):
        for block in content:
            if isinstance(block, dict) and block.get("type") == "text" and isinstance(block.get("text"), str):
                texts.append(block["text"])
            else:
                refused = True
    text = "".join(texts)
    if not text and isinstance(result.get("structuredContent"), (dict, list)):
        text = json.dumps(result["structuredContent"])
    if refused:
        text += ("" if not text else "\n") + json.dumps({"note": "non_text_content_refused"})
    return text[:MAX_RESULT_CHARS], bool(result.get("isError")) or "error" in reply


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):  # the recipient is the configured endpoint, no other
        return None


def build_opener() -> urllib.request.OpenerDirector:
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())


def chat_request(opener, url: str, headers: dict, body: dict, timeout: float) -> dict:
    data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    request = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with opener.open(request, timeout=timeout) as reply:
            raw = reply.read()
    except urllib.error.HTTPError as error:
        note(f"agent: http_status={error.code} request_bytes={len(data)}")
        raise AgentError(f"http_{error.code}") from None
    except (urllib.error.URLError, OSError, TimeoutError):
        raise AgentError("network_error") from None
    try:
        value = json.loads(raw)
    except ValueError:
        raise AgentError("reply_unreadable") from None
    if not isinstance(value, dict):
        raise AgentError("reply_unreadable")
    return value


def usage_of(reply: dict) -> dict:
    """Chat usage in the stream's cache-aware shape: ``input_tokens`` is the part not read from the
    cache, ``cache_read_input_tokens`` the cached part (OpenAI's ``prompt_tokens_details.cached_tokens``
    or DeepSeek's ``prompt_cache_hit_tokens``), ``output_tokens`` the completion total (reasoning
    included, as the providers count it)."""
    usage = reply.get("usage") if isinstance(reply.get("usage"), dict) else {}

    def number(value):
        return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else 0
    prompt = number(usage.get("prompt_tokens"))
    cached = number(usage.get("prompt_cache_hit_tokens"))
    details = usage.get("prompt_tokens_details")
    if not cached and isinstance(details, dict):
        cached = number(details.get("cached_tokens"))
    cached = min(cached, prompt)
    return {"input_tokens": prompt - cached, "output_tokens": number(usage.get("completion_tokens")),
            "cache_read_input_tokens": cached, "cache_creation_input_tokens": 0}


def assistant_message(choice_message: dict) -> dict:
    """The assistant message kept for the next request: its text, its tool calls (function calls
    only) and the provider's ``reasoning_content`` (kept because thinking models need it back in the
    same turn); nothing else the reply carried."""
    kept = {"role": "assistant", "content": choice_message.get("content")
            if isinstance(choice_message.get("content"), str) else None}
    calls = []
    for call in choice_message.get("tool_calls") or []:
        if isinstance(call, dict) and isinstance(call.get("function"), dict):
            calls.append({"id": str(call.get("id")), "type": "function", "function": {
                "name": str(call["function"].get("name")),
                "arguments": call["function"].get("arguments") if isinstance(
                    call["function"].get("arguments"), str) else "{}"}})
    if calls:
        kept["tool_calls"] = calls
    if isinstance(choice_message.get("reasoning_content"), str):
        kept["reasoning_content"] = choice_message["reasoning_content"]
    return kept


def run(args, prompt: str, bridge_factory=McpBridge, opener=None) -> int:
    started = time.monotonic()
    mcp = json.loads(Path(args.mcp_config).read_text(encoding="utf-8"))["mcpServers"][SERVER_NAME]
    key = load_key(args.key_file)
    headers = {"Content-Type": "application/json"}
    if args.auth_style == "bearer":
        headers["Authorization"] = f"Bearer {key}"
    else:
        headers["api-key"] = key
    url = args.base_url.rstrip("/") + "/chat/completions"
    opener = opener or build_opener()
    bridge = bridge_factory([mcp["command"], *mcp.get("args", [])])
    totals = {"input_tokens": 0, "output_tokens": 0, "cache_read_input_tokens": 0,
              "cache_creation_input_tokens": 0}
    turns, subtype, final_text, is_error = 0, "success", "", False
    try:
        defs, listed_names = tool_definitions(bridge.start())
        emit({"type": "system", "subtype": "init", "cli": "minimal-agent", "cli_version": AGENT_VERSION,
              "provider": args.provider, "model": args.model,
              "tools": [f"mcp__{SERVER_NAME}__{n}" for n in listed_names],
              "mcp_servers": [{"name": SERVER_NAME, "status": "connected"}], "skills": [],
              "slash_commands": [], "plugins": [], "agents": [], "memory_paths": {},
              "additional_directories": []})
        if sorted(d["function"]["name"] for d in defs) != sorted(TOOLS) or sorted(listed_names) != sorted(TOOLS):
            raise AgentError("tool_surface_not_the_three")
        messages = [{"role": "system", "content": args.system_prompt},
                    {"role": "user", "content": prompt}]
        while True:
            if turns >= args.max_turns:
                subtype = "error_max_turns"
                break
            if totals["input_tokens"] + totals["cache_read_input_tokens"] + totals["output_tokens"] \
                    > args.max_total_tokens:
                subtype = "error_max_budget_usd"  # the stream's name for "a budget was exhausted"
                break
            body = {"model": args.model, "messages": messages, "tools": defs, "tool_choice": "auto",
                    "stream": False, **args.extra_param}
            reply = chat_request(opener, url, headers, body, args.request_timeout)
            turns += 1
            usage = usage_of(reply)
            for k, v in usage.items():
                totals[k] += v
            choices = reply.get("choices")
            if not isinstance(choices, list) or not choices or not isinstance(choices[0], dict) \
                    or not isinstance(choices[0].get("message"), dict):
                raise AgentError("reply_has_no_message")
            kept = assistant_message(choices[0]["message"])
            messages.append(kept)
            blocks = []
            if kept["content"]:
                blocks.append({"type": "text", "text": kept["content"]})
                final_text = kept["content"]
            calls = kept.get("tool_calls", [])
            for call in calls:
                try:
                    arguments = json.loads(call["function"]["arguments"] or "{}")
                except ValueError:
                    arguments = None
                blocks.append({"type": "tool_use", "id": call["id"],
                               "name": f"mcp__{SERVER_NAME}__{call['function']['name']}",
                               "input": arguments if isinstance(arguments, dict) else {}})
            emit({"type": "assistant", "message": {"id": f"msg_{turns}", "content": blocks, "usage": usage}})
            if not calls:
                break
            results = []
            for call in calls:
                name = call["function"]["name"]
                try:
                    arguments = json.loads(call["function"]["arguments"] or "{}")
                except ValueError:
                    arguments = None
                if name not in TOOLS or not isinstance(arguments, dict):
                    text, error = json.dumps({"error": "unknown_tool_or_bad_arguments"}), True
                else:
                    text, error = result_text(bridge.call(name, arguments))
                messages.append({"role": "tool", "tool_call_id": call["id"], "content": text})
                results.append({"type": "tool_result", "tool_use_id": call["id"],
                                "content": [{"type": "text", "text": text}], "is_error": error})
            emit({"type": "user", "message": {"content": results}})
    except AgentError as error:
        subtype, is_error, final_text = "error_during_execution", True, error.token
    finally:
        bridge.close()
    emit({"type": "result", "subtype": subtype, "is_error": is_error or subtype != "success",
          "result": final_text, "num_turns": turns, "usage": totals,
          "total_cost_usd": cost_of(totals, args.price), "duration_ms": round((time.monotonic() - started) * 1000)})
    return 0 if not is_error else 1


def cost_of(usage: dict, price) -> float | None:
    """USD from ``--price`` (per million tokens: fresh input, cached input, output), or None."""
    if not price:
        return None
    fresh, cached, out = price
    return round((usage["input_tokens"] * fresh + usage["cache_read_input_tokens"] * cached
                  + usage["output_tokens"] * out) / 1_000_000, 8)


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    p.add_argument("--base-url", required=True, help="the OpenAI-compatible base URL (https, or loopback)")
    p.add_argument("--model", required=True)
    p.add_argument("--provider", required=True, help="a label for the results (deepseek, mimo, ...)")
    p.add_argument("--key-file", type=Path, required=True, help="a file outside the repository holding the key")
    p.add_argument("--auth-style", choices=AUTH_STYLES, default="bearer")
    p.add_argument("--mcp-config", type=Path, required=True)
    p.add_argument("--system-prompt", required=True)
    p.add_argument("--extra-param", default="{}", help="JSON object: " + ", ".join(EXTRA_KEYS))
    p.add_argument("--max-turns", type=int, default=40)
    p.add_argument("--max-total-tokens", type=int, default=400_000,
                   help="stop when the context tokens read plus output tokens written exceed this")
    p.add_argument("--request-timeout", type=float, default=120.0)
    p.add_argument("--price", help="USD per million tokens: fresh input,cached input,output")
    args = p.parse_args(argv)
    for name in ("key_file", "mcp_config"):
        setattr(args, name, Path(os.path.abspath(os.fspath(getattr(args, name)))))
    if (problem := endpoint_problem(args.base_url)):
        p.error(problem)
    try:
        args.extra_param = json.loads(args.extra_param)
    except ValueError:
        p.error("extra_param_not_json")
    if (problem := extra_param_problem(args.extra_param)):
        p.error(problem)
    args.price = tuple(float(x) for x in args.price.split(",")) if args.price else None
    if args.price is not None and len(args.price) != 3:
        p.error("price_needs_three_numbers")
    return args


def main(argv=None) -> int:
    args = parse_args(argv)
    prompt = sys.stdin.buffer.read().decode("utf-8")
    try:
        return run(args, prompt)
    except AgentError as error:
        emit({"type": "result", "subtype": "error_during_execution", "is_error": True,
              "result": error.token, "num_turns": 0})
        return 1


if __name__ == "__main__":
    sys.exit(main())
