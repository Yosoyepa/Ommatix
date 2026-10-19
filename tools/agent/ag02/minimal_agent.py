#!/usr/bin/env python3
"""AG-02 minimal agent: a tool-calling loop of our own for an OpenAI-compatible endpoint (ADR-0037).

It speaks the OpenAI-compatible Chat Completions API to ONE configured endpoint and offers the model
EXACTLY the three Ommatix tools, bridged over MCP stdio to the budget proxy (``proxy.py``) given in
``--mcp-config``. It has no other capability by construction: no shell, file, web or image code
exists in it, no skill, plugin, memory or instruction file is read, the request carries only the
system instruction, the goal prompt (stdin), the conversation so far and the three tool
definitions, and the only network peer is the endpoint (no redirect is followed, no proxy variable
is honoured, a non-loopback endpoint must be https).

Fail closed at every boundary (the PR-93 review): a provider reply is read with a byte bound and
parsed strictly (no duplicate key, no ``NaN``, bounded nesting) and its whole message is validated
**before** anything is emitted or forwarded - one function call at most, a function-typed call with a
unique id, an allowed tool name, arguments that are a bounded JSON object matching the tool's schema
(and, in the dry-run-only phase, ``dry_run: true``); a reply that fails any of that ends the session
with a fixed token and forwards nothing. Usage is read from every reply and accounted at once;
a missing or malformed usage ends the session; the token cap (and a dollar cap, if prices are given)
is checked before every request against a conservative reservation (the request's size in bytes,
which bounds its tokens, plus the explicit completion ceiling sent as ``max_tokens``) and after every
reply, final ones included. The key file must lie outside any repository checkout.

Output: one JSON object per line on stdout in the stream shape the runner already reads (the
``init`` / ``assistant`` / ``user`` / ``result`` events of ``run_goal.StreamMetrics``), so the
runner, its scoring and its metrics are the same for every agent. stderr carries metadata only
(HTTP status, byte sizes), never a body, a label or a key. The API key is read from ``--key-file``
and sent in one header.

Standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

AGENT_VERSION = "1.1.0"
SERVER_NAME = "ommatix"
TOOLS = ("observe_screen", "execute_action", "get_engine_metrics")
LOOPBACK = ("127.0.0.1", "localhost", "::1")
# The only body keys the request may ever carry. ``--extra-param`` may add the reasoning and sampling
# parameters below and nothing else; the model's identity, the messages, the tools and the completion
# ceiling are not configurable that way.
BASE_KEYS = ("model", "messages", "tools", "tool_choice", "stream", "max_tokens")
EXTRA_KEYS = ("reasoning_effort", "thinking", "temperature", "top_p")
AUTH_STYLES = ("bearer", "api-key")
MESSAGE_KEYS = ("role", "content", "tool_calls", "tool_call_id", "reasoning_content")
MAX_RESULT_CHARS = 200_000
# Bounds on what a provider may send (the PR-93 review): checked before allocation or dispatch.
MAX_RESPONSE_BYTES = 4 * 1024 * 1024
MAX_TEXT_CHARS = 200_000
MAX_ARGUMENT_CHARS = 16_000
MAX_CALLS = 1
MAX_JSON_DEPTH = 64
DEFAULT_COMPLETION_CEILING = 32_768
CALL_ID = re.compile(r"^[A-Za-z0-9_.:-]{1,64}$")
SCHEMA_ANNOTATIONS = frozenset({"$schema", "title", "description", "default", "examples"})
# Tokens tell the budget about a model that cannot be priced: never free.
TOKEN_PLAN_PREFIXES = ("tp-", "ttp-")


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


# ---- strict JSON ----------------------------------------------------------------------------

def _no_duplicates(pairs):
    seen = {}
    for key, value in pairs:
        if key in seen:
            raise ValueError("duplicate key")
        seen[key] = value
    return seen


def _no_constant(name):
    raise ValueError("non-JSON constant")


def json_depth(value) -> int:
    """The nesting depth of a decoded JSON value, found without recursion (and stopped early past the
    limit), so the answer does not depend on the platform's decoder or recursion limit."""
    deepest, stack = 0, [(value, 1)]
    while stack:
        node, depth = stack.pop()
        deepest = max(deepest, depth)
        if deepest > MAX_JSON_DEPTH:
            break
        children = node.values() if isinstance(node, dict) else node if isinstance(node, list) else ()
        stack.extend((child, depth + 1) for child in children if isinstance(child, (dict, list)))
    return deepest


def strict_loads(text, token: str):
    """``json.loads`` that refuses a duplicate object key, ``NaN`` / ``Infinity``, a nesting past
    ``MAX_JSON_DEPTH`` (whatever the decoder itself would have allowed) and a nesting deep enough to
    exhaust the decoder; every failure is the one fixed `token`."""
    try:
        value = json.loads(text, object_pairs_hook=_no_duplicates, parse_constant=_no_constant)
    except (ValueError, RecursionError):
        raise AgentError(token) from None
    if json_depth(value) > MAX_JSON_DEPTH:
        raise AgentError(token)
    return value


# ---- paths ----------------------------------------------------------------------------------

def key_path_problem(path) -> str | None:
    """Why `path` cannot hold the key: it must be an absolute path in plain form (no drive-relative
    form, no extended-length or device prefix, no alternate data stream, no ``..``) that, resolved
    (symbolic links and junctions followed), lies outside any repository checkout - the same rule as
    the runner's, applied here because this is the boundary that opens the file."""
    text = str(path)
    p = Path(text)
    if not p.is_absolute():
        return "key_path_not_absolute"
    if text.startswith(("\\\\?\\", "\\\\.\\", "//?/", "//./")):
        return "key_path_extended_form"
    if ":" in text[2:] or ".." in p.parts:
        return "key_path_ambiguous"
    here = Path(os.path.realpath(p))
    if any((parent / ".git").exists() for parent in [here, *here.parents]):
        return "key_path_inside_checkout"
    return None


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
    if (problem := key_path_problem(path)) is not None:
        raise AgentError(problem)
    try:
        key = path.read_text(encoding="utf-8").strip()
    except OSError:
        raise AgentError("key_file_unreadable") from None
    if not key or "\n" in key or "\r" in key or any(ord(c) < 0x20 for c in key):
        raise AgentError("key_file_malformed")
    return key


# ---- tool arguments: bounded, strict, schema-checked ---------------------------------------------

def _same(a, b) -> bool:
    return type(a) is type(b) and a == b


def _type_ok(rule, value) -> bool:
    kinds = rule if isinstance(rule, list) else [rule]
    for kind in kinds:
        if kind == "object" and isinstance(value, dict):
            return True
        if kind == "array" and isinstance(value, list):
            return True
        if kind == "string" and isinstance(value, str):
            return True
        if kind == "boolean" and isinstance(value, bool):
            return True
        if kind == "null" and value is None:
            return True
        if kind == "integer" and isinstance(value, int) and not isinstance(value, bool):
            return True
        if kind == "number" and isinstance(value, (int, float)) and not isinstance(value, bool):
            return True
    return False


def schema_ok(schema, value) -> bool:
    """Whether `value` satisfies the JSON Schema subset the Ommatix tools use (type, enum, const,
    properties, required, additionalProperties, items, min/max length and bounds, allOf, anyOf, oneOf,
    not, if/then/else). A keyword outside the subset is a fixed refusal, not a pass."""
    if schema is True:
        return True
    if schema is False:
        return False
    if not isinstance(schema, dict):
        raise AgentError("schema_unsupported")
    for key, rule in schema.items():
        if key in SCHEMA_ANNOTATIONS or key in ("then", "else"):
            continue
        if key == "type":
            if not _type_ok(rule, value):
                return False
        elif key == "enum":
            if not any(_same(value, option) for option in rule):
                return False
        elif key == "const":
            if not _same(value, rule):
                return False
        elif key == "properties":
            if isinstance(value, dict):
                for name, sub in rule.items():
                    if name in value and not schema_ok(sub, value[name]):
                        return False
        elif key == "required":
            if isinstance(value, dict) and not all(name in value for name in rule):
                return False
        elif key == "additionalProperties":
            if isinstance(value, dict):
                extra = [k for k in value if k not in schema.get("properties", {})]
                if rule is False and extra:
                    return False
                if isinstance(rule, dict) and not all(schema_ok(rule, value[k]) for k in extra):
                    return False
        elif key == "items":
            if isinstance(value, list) and not all(schema_ok(rule, item) for item in value):
                return False
        elif key in ("minLength", "maxLength"):
            if isinstance(value, str) and (len(value) < rule if key == "minLength" else len(value) > rule):
                return False
        elif key in ("minimum", "maximum"):
            if isinstance(value, (int, float)) and not isinstance(value, bool) and (
                    value < rule if key == "minimum" else value > rule):
                return False
        elif key == "allOf":
            if not all(schema_ok(sub, value) for sub in rule):
                return False
        elif key == "anyOf":
            if not any(schema_ok(sub, value) for sub in rule):
                return False
        elif key == "oneOf":
            if sum(1 for sub in rule if schema_ok(sub, value)) != 1:
                return False
        elif key == "not":
            if schema_ok(rule, value):
                return False
        elif key == "if":
            branch = schema.get("then") if schema_ok(rule, value) else schema.get("else")
            if branch is not None and not schema_ok(branch, value):
                return False
        else:
            raise AgentError("schema_keyword_unsupported")
    return True


def validate_call(message: dict, seen_ids: set, schemas: dict, dry_run_only: bool):
    """The one tool call of a provider message, fully validated, or None when it has none. Every
    deviation is a fixed token and the caller forwards nothing."""
    calls = message.get("tool_calls")
    if calls is None or calls == []:
        return None
    if not isinstance(calls, list):
        raise AgentError("tool_calls_not_a_list")
    if len(calls) > MAX_CALLS:
        raise AgentError("multiple_tool_calls")
    call = calls[0]
    if not isinstance(call, dict) or set(call) - {"id", "type", "function"} or call.get("type") != "function":
        raise AgentError("tool_call_shape")
    call_id = call.get("id")
    if not isinstance(call_id, str) or not CALL_ID.match(call_id):
        raise AgentError("tool_call_id_invalid")
    if call_id in seen_ids:
        raise AgentError("tool_call_id_repeated")
    function = call.get("function")
    if not isinstance(function, dict) or set(function) != {"name", "arguments"}:
        raise AgentError("tool_call_shape")
    name = function["name"]
    if not isinstance(name, str) or name not in TOOLS:
        raise AgentError("tool_call_unknown_name")
    raw = function["arguments"]
    if not isinstance(raw, str):
        raise AgentError("arguments_not_a_string")
    if len(raw) > MAX_ARGUMENT_CHARS:
        raise AgentError("arguments_too_large")
    arguments = strict_loads(raw, "arguments_unreadable")
    if not isinstance(arguments, dict):
        raise AgentError("arguments_not_an_object")
    if not schema_ok(schemas.get(name, True), arguments):
        raise AgentError("arguments_violate_the_schema")
    if dry_run_only and name == "execute_action" and arguments.get("dry_run") is not True:
        raise AgentError("live_argument_in_dry_run_phase")
    return {"id": call_id, "name": name, "arguments": arguments, "raw": raw}


def validate_text(value, token: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise AgentError(token)
    if len(value) > MAX_TEXT_CHARS:
        raise AgentError(token)
    return value


# ---- the budget -------------------------------------------------------------------------------

class Budget:
    """Tokens (context read plus output written) and, when prices are known, dollars. A request is
    allowed only if its reservation - its body's size in bytes, an upper bound of its tokens, plus the
    completion ceiling - fits in what is left; usage is accounted from every reply, final ones included,
    and a reply without a valid usage is refused rather than counted as free."""

    def __init__(self, max_tokens: int, ceiling: int, price=None, max_usd=None):
        self.max_tokens, self.ceiling, self.price, self.max_usd = max_tokens, ceiling, price, max_usd
        self.totals = {"input_tokens": 0, "output_tokens": 0, "cache_read_input_tokens": 0,
                       "cache_creation_input_tokens": 0}

    @property
    def tokens(self) -> int:
        t = self.totals
        return t["input_tokens"] + t["cache_read_input_tokens"] + t["output_tokens"]

    @property
    def usd(self) -> float | None:
        return cost_of(self.totals, self.price)

    def can_afford(self, request_bytes: int) -> bool:
        reserve = request_bytes + self.ceiling
        if self.tokens + reserve > self.max_tokens:
            return False
        if self.max_usd is not None:
            fresh, cached, out = self.price
            worst = (request_bytes * max(fresh, cached) + self.ceiling * out) / 1_000_000
            if (self.usd or 0.0) + worst > self.max_usd:
                return False
        return True

    def account(self, reply: dict) -> dict:
        usage = usage_of(reply)
        for key, value in usage.items():
            self.totals[key] += value
        return usage

    @property
    def exhausted(self) -> bool:
        return self.tokens > self.max_tokens or (
            self.max_usd is not None and (self.usd or 0.0) > self.max_usd)


def usage_of(reply: dict) -> dict:
    """Chat usage in the stream's cache-aware shape: ``input_tokens`` is the part not read from the
    cache, ``cache_read_input_tokens`` the cached part (OpenAI's ``prompt_tokens_details.cached_tokens``
    or DeepSeek's ``prompt_cache_hit_tokens``), ``output_tokens`` the completion total (reasoning
    included, as the providers count it). A reply without valid ``prompt_tokens`` and
    ``completion_tokens`` is refused: an unknown balance is not a free one."""
    usage = reply.get("usage")
    if not isinstance(usage, dict):
        raise AgentError("usage_missing_or_invalid")

    def number(value):
        return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None
    prompt, completion = number(usage.get("prompt_tokens")), number(usage.get("completion_tokens"))
    if prompt is None or completion is None:
        raise AgentError("usage_missing_or_invalid")
    cached = number(usage.get("prompt_cache_hit_tokens")) or 0
    details = usage.get("prompt_tokens_details")
    if not cached and isinstance(details, dict):
        cached = number(details.get("cached_tokens")) or 0
    cached = min(cached, prompt)
    return {"input_tokens": prompt - cached, "output_tokens": completion,
            "cache_read_input_tokens": cached, "cache_creation_input_tokens": 0}


def cost_of(usage: dict, price) -> float | None:
    """USD from ``--price`` (per million tokens: fresh input, cached input, output), or None."""
    if not price:
        return None
    fresh, cached, out = price
    return round((usage["input_tokens"] * fresh + usage["cache_read_input_tokens"] * cached
                  + usage["output_tokens"] * out) / 1_000_000, 8)


# ---- MCP -----------------------------------------------------------------------------------------

class McpBridge:
    """Line-delimited JSON-RPC over the proxy's stdio (legacy profile: ``initialize``)."""

    def __init__(self, command: list[str], env: dict | None = None):
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, env=env)
        self.next_id = 1
        self.calls = 0

    def _send(self, message: dict) -> None:
        self.proc.stdin.write((json.dumps(message) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method: str, params: dict) -> dict:
        rid, self.next_id = self.next_id, self.next_id + 1
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        line = self.proc.stdout.readline()
        if not line:
            raise AgentError("mcp_server_closed")
        reply = strict_loads(line, "mcp_reply_unreadable")
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
        self.calls += 1
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


def tool_definitions(listed: list[dict]) -> tuple[list[dict], list[str], dict]:
    """(the three chat tool definitions, the names the server listed, the input schemas by name).
    The agent offers a tool only if the server listed it; a server that lists anything but the
    three is reported through the names and the init event, and the runner ends the session."""
    names = sorted(str(t.get("name")) for t in listed if isinstance(t, dict))
    defs, schemas = [], {}
    for tool in sorted((t for t in listed if isinstance(t, dict) and t.get("name") in TOOLS),
                       key=lambda t: TOOLS.index(t["name"])):
        schema = tool.get("inputSchema") if isinstance(tool.get("inputSchema"), dict) else {
            "type": "object", "properties": {}}
        schemas[tool["name"]] = schema
        defs.append({"type": "function", "function": {
            "name": tool["name"], "description": str(tool.get("description") or ""), "parameters": schema}})
    return defs, names, schemas


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


# ---- the provider -----------------------------------------------------------------------------------

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):  # the recipient is the configured endpoint, no other
        return None


def build_opener() -> urllib.request.OpenerDirector:
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())


def chat_request(opener, url: str, headers: dict, data: bytes, timeout: float) -> dict:
    """POST `data`; read at most ``MAX_RESPONSE_BYTES`` (a declared or undeclared body past it is a
    fixed refusal before any parsing); parse strictly. No body text ever reaches an error."""
    request = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with opener.open(request, timeout=timeout) as reply:
            declared = reply.headers.get("Content-Length") if getattr(reply, "headers", None) else None
            if declared is not None and declared.isdigit() and int(declared) > MAX_RESPONSE_BYTES:
                raise AgentError("reply_too_large")
            raw = reply.read(MAX_RESPONSE_BYTES + 1)
    except urllib.error.HTTPError as error:
        note(f"agent: http_status={error.code} request_bytes={len(data)}")
        raise AgentError(f"http_{error.code}") from None
    except (urllib.error.URLError, OSError, TimeoutError):
        raise AgentError("network_error") from None
    if len(raw) > MAX_RESPONSE_BYTES:
        raise AgentError("reply_too_large")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise AgentError("reply_unreadable") from None
    value = strict_loads(text, "reply_unreadable")
    if not isinstance(value, dict):
        raise AgentError("reply_unreadable")
    return value


def choice_message(reply: dict) -> dict:
    choices = reply.get("choices")
    if not isinstance(choices, list) or not choices or not isinstance(choices[0], dict) \
            or not isinstance(choices[0].get("message"), dict):
        raise AgentError("reply_has_no_message")
    return choices[0]["message"]


def run(args, prompt: str, bridge_factory=McpBridge, opener=None) -> int:
    started = time.monotonic()
    key = load_key(args.key_file)
    if key.startswith(TOKEN_PLAN_PREFIXES):
        # A Token Plan key's terms forbid scripts and application backends: only coding tools. Judged on
        # the key, not on the provider label the agent was started with.
        raise AgentError("token_plan_key_not_for_scripts")
    mcp = json.loads(Path(args.mcp_config).read_text(encoding="utf-8"))["mcpServers"][SERVER_NAME]
    headers = {"Content-Type": "application/json"}
    if args.auth_style == "bearer":
        headers["Authorization"] = f"Bearer {key}"
    else:
        headers["api-key"] = key
    url = args.base_url.rstrip("/") + "/chat/completions"
    opener = opener or build_opener()
    bridge = bridge_factory([mcp["command"], *mcp.get("args", [])])
    budget = Budget(args.max_total_tokens, args.completion_ceiling, args.price, args.max_usd)
    turns, subtype, final_text, is_error = 0, "success", "", False
    seen_ids: set = set()
    try:
        defs, listed_names, schemas = tool_definitions(bridge.start())
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
            body = {"model": args.model, "messages": messages, "tools": defs, "tool_choice": "auto",
                    "stream": False, "max_tokens": args.completion_ceiling, **args.extra_param}
            data = json.dumps(body, ensure_ascii=False).encode("utf-8")
            if not budget.can_afford(len(data)):
                subtype = "error_max_budget_usd"  # the stream's name for "a budget was exhausted"
                break
            reply = chat_request(opener, url, headers, data, args.request_timeout)
            turns += 1
            usage = budget.account(reply)
            message = choice_message(reply)
            content = validate_text(message.get("content"), "content_invalid")
            reasoning = validate_text(message.get("reasoning_content"), "reasoning_invalid")
            call = validate_call(message, seen_ids, schemas, args.dry_run_only)
            kept = {"role": "assistant", "content": content}
            if call is not None:
                seen_ids.add(call["id"])
                kept["tool_calls"] = [{"id": call["id"], "type": "function",
                                       "function": {"name": call["name"], "arguments": call["raw"]}}]
            if reasoning is not None:
                kept["reasoning_content"] = reasoning
            messages.append(kept)
            blocks = []
            if content:
                blocks.append({"type": "text", "text": content})
                final_text = content
            if call is not None:
                blocks.append({"type": "tool_use", "id": call["id"],
                               "name": f"mcp__{SERVER_NAME}__{call['name']}", "input": call["arguments"]})
            emit({"type": "assistant", "message": {"id": f"msg_{turns}", "content": blocks, "usage": usage}})
            if budget.exhausted:  # a reply that overran the cap ends the goal, final replies included
                subtype = "error_max_budget_usd"
                break
            if call is None:
                break
            text, error = result_text(bridge.call(call["name"], call["arguments"]))
            messages.append({"role": "tool", "tool_call_id": call["id"], "content": text})
            emit({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": call["id"],
                 "content": [{"type": "text", "text": text}], "is_error": error}]}})
    except AgentError as error:
        subtype, is_error, final_text = "error_during_execution", True, error.token
    finally:
        bridge.close()
    emit({"type": "result", "subtype": subtype, "is_error": is_error or subtype != "success",
          "result": final_text, "num_turns": turns, "usage": budget.totals,
          "total_cost_usd": budget.usd, "duration_ms": round((time.monotonic() - started) * 1000)})
    return 0 if not is_error else 1


def _price(text: str):
    try:
        values = tuple(float(x) for x in text.split(","))
    except ValueError:
        raise argparse.ArgumentTypeError("price_not_numbers") from None
    if len(values) != 3 or not all(math.isfinite(v) and v >= 0 for v in values):
        raise argparse.ArgumentTypeError("price_needs_three_finite_nonnegative_numbers")
    return values


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
    p.add_argument("--dry-run-only", action="store_true",
                   help="phase 1: a tool call whose dry_run is not true is refused before the bridge")
    p.add_argument("--max-turns", type=int, default=40)
    p.add_argument("--max-total-tokens", type=int, default=400_000,
                   help="stop when the context tokens read plus output tokens written would exceed this")
    p.add_argument("--completion-ceiling", type=int, default=DEFAULT_COMPLETION_CEILING,
                   help="max_tokens sent with every request (the completion ceiling of the reservation)")
    p.add_argument("--max-usd", type=float, help="stop when the spend would exceed this (needs --price)")
    p.add_argument("--request-timeout", type=float, default=120.0)
    p.add_argument("--price", type=_price, help="USD per million tokens: fresh input,cached input,output")
    args = p.parse_args(argv)
    args.key_file = Path(os.path.abspath(os.fspath(args.key_file)))
    args.mcp_config = Path(os.path.abspath(os.fspath(args.mcp_config)))
    if (problem := key_path_problem(args.key_file)):
        p.error(problem)
    if (problem := endpoint_problem(args.base_url)):
        p.error(problem)
    try:
        args.extra_param = json.loads(args.extra_param)
    except ValueError:
        p.error("extra_param_not_json")
    if (problem := extra_param_problem(args.extra_param)):
        p.error(problem)
    if args.max_total_tokens < 1 or args.completion_ceiling < 1 or args.max_turns < 1:
        p.error("limits_must_be_positive")
    if args.max_usd is not None and (args.price is None or not math.isfinite(args.max_usd) or args.max_usd < 0):
        p.error("max_usd_needs_a_finite_nonnegative_cap_and_prices")
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
