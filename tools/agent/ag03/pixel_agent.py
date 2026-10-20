#!/usr/bin/env python3
"""AG-03 shared agent definitions: the command, the pinned instruction, the stream and the request verdict.

* The agent is Claude Code, confined exactly as AG-02's (``--tools ""``, ``--strict-mcp-config``,
  ``--setting-sources ""``, controlled configuration directory), with an MCP server whose three tools are
  ``screenshot``, ``click`` and ``type_text`` (``pixel_server.py``) instead of the Ommatix ones.
* ``SYSTEM_INSTRUCTION`` is the baseline's one pre-registered instruction (O12: no tuning). It mirrors
  AG-02's ``goals.json`` instruction sentence for sentence and differs only where the interface does
  (tool names, pixel coordinates instead of ids); it is pinned by SHA-256.
* ``image_blocks`` / ``check_pixel_requests`` are the request-level verdict of ADR-0038 decision 6:
  every request that offers tools offers exactly the three pixel tools, and **image blocks appear only
  inside the tool result of ``screenshot``, as PNG, within the byte and dimension limits**. The AG-02
  verdict (``image block => fail``) is untouched and is not reused.
* ``PixelStream`` reads Claude Code's stream like AG-02's ``StreamMetrics`` (counts, statuses, codes,
  tokens, timings, booleans) plus the screenshot metadata of decision 9. It keeps no image bytes: a
  screenshot's dimensions are read from the PNG header of the base64 prefix and the rest is dropped.
"""

from __future__ import annotations

import base64
import hashlib
import json
import math
import re
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_capture as cap  # noqa: E402
import pixel_server as ps  # noqa: E402
import run_goal as rg  # noqa: E402

SERVER_NAME = ps.SERVER_NAME
TOOLS = ps.TOOLS
ALLOWED_TOOL_NAMES = ps.ALLOWED_TOOL_NAMES
PROOF_SCHEMA = "ag03-isolation-proof-1"
# Raised whenever this verifier is made stricter; a proof of an older revision is refused. AG-02's
# proofs (``ag02-isolation-proof-3``) are a different schema and stay valid. Revision 2: the proof records and checks what the
# synthetic sessions leave on disk (``pixel_disk``), the ``--no-session-persistence`` invariant and the agent's environment.
VERIFIER_REVISION = 2
# The pixel agent's own environment switch (AG-02's ``QUIET_ENV`` and so its proofs are unchanged): the CLI's feedback survey
# off, so that no survey prompt, and nothing it would write for one, can touch a session. Part of the proof.
PIXEL_ENV = {"CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY": "1"}
NO_PERSISTENCE_FLAG = "--no-session-persistence"

SYSTEM_INSTRUCTION = (
    "You control a desktop only through three tools: screenshot, click and type_text. You see the "
    "application window only through screenshots. Act on the screen only by the pixel coordinates of your "
    "latest screenshot; never invent coordinates. After every action, call screenshot again before you "
    "decide what comes next. Never claim a goal is achieved unless your latest screenshot shows it. If the "
    "goal cannot be done (the element does not exist, is disabled, or your actions keep failing or cannot "
    "be confirmed), do not retry blindly and do not improvise another goal: stop. The message that gives "
    "you the goal states the active phase; follow it exactly. End your final message with exactly one "
    "verdict line: `VERDICT: DONE` if the goal is achieved and you observed it, or `VERDICT: IMPOSSIBLE` "
    "if it cannot be done.")
SYSTEM_INSTRUCTION_SHA256 = "24982efacba432f6048b113035777d8313cd09618cc34f7c9c378668d9986992"
PHASE_LINES = {
    1: ("PHASE 1 (dry run): your click and type_text calls will not change the screen; describe the actions "
        "you would take by making those calls, and finish with `VERDICT: DONE` only if the goal is feasible "
        "as stated, `VERDICT: IMPOSSIBLE` otherwise."),
    2: "PHASE 2 (live): your click and type_text calls act on the application now.",
}


def instruction_sha256(text: str = SYSTEM_INSTRUCTION) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def build_prompt(goal: dict, phase: int) -> str:
    """The user message: the active phase, then the goal (AG-02's, unchanged in form)."""
    return f"{PHASE_LINES[phase]}\n\nGOAL: {goal['prompt']}"


def build_mcp_config(python: str, server_args: list[str]) -> dict:
    return {"mcpServers": {SERVER_NAME: {"command": python, "args": [str(HERE / "pixel_server.py"), *server_args]}}}


def build_claude_argv(*, claude, model: str, mcp_config: Path, system_instruction: str,
                      max_budget_usd: float | None, effort: str | None = "max") -> list[str]:
    """AG-02's command with the pixel tools allowed (``run_goal.build_claude_argv`` allows the Ommatix
    ones). Prompt through stdin; every settings source off; no unrestricted mode. ``effort`` is the registered
    ``max`` on the native path (a provider route pins it in its child environment instead and passes None);
    ``max_budget_usd`` is the registered 0.50 USD cap on the native path and None on a provider route."""
    head = list(claude) if isinstance(claude, (list, tuple)) else [claude]
    return [*head, "-p", "--model", model, "--output-format", "stream-json", "--verbose",
            "--mcp-config", str(mcp_config), "--strict-mcp-config",
            "--tools", "", "--allowedTools", ",".join(ALLOWED_TOOL_NAMES),
            "--permission-mode", "dontAsk", "--permission-prompts", "none",
            "--system-prompt", system_instruction, "--no-session-persistence",
            "--disable-slash-commands", "--setting-sources", "",
            *([] if max_budget_usd is None else ["--max-budget-usd", f"{max_budget_usd:g}"]),
            *([] if effort is None else ["--effort", effort])]


def command_problems(argv) -> list[str]:
    """Closed tokens for a pixel command that no longer holds the invariant of ADR-0038 O5: without
    ``--no-session-persistence`` Claude Code writes every MCP image result in clear under the configuration directory
    (measured 2026-10-20, ``pixel_disk``). Checked by the preflight and again before every launch."""
    return [] if NO_PERSISTENCE_FLAG in list(argv) else ["production_command_persists_session"]


TEMP_ENV_NAMES = ("TEMP", "TMP", "TMPDIR", "CLAUDE_CODE_TMPDIR")


def temp_env_problems(env: dict) -> list[str]:
    """Closed tokens for a child environment that does not give the session a temp directory of its own: the four variables
    that decide where the CLI writes its temporary and image files must name ONE non-empty directory (R106-1)."""
    values = {env.get(name) for name in TEMP_ENV_NAMES}
    return [] if len(values) == 1 and next(iter(values)) else ["child_temp_not_isolated"]


def env_problems(env: dict) -> list[str]:
    """Closed tokens for an environment that lacks the pixel agent's switches (``PIXEL_ENV``)."""
    return [] if all(env.get(k) == v for k, v in PIXEL_ENV.items()) else ["agent_environment_lacks_pixel_switches"]


# ---- images in a request -----------------------------------------------------------------------

def image_info(data: bytes) -> tuple[str, int, int] | None:
    """(format, width, height) of a PNG or JPEG from its header; None for anything else."""
    if (info := cap.png_info(data)) is not None:
        return "png", info[0], info[1]
    if data[:3] == b"\xff\xd8\xff":
        i = 2
        while i + 9 < len(data):
            if data[i] != 0xFF:
                i += 1
                continue
            marker = data[i + 1]
            if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
                i += 2
                continue
            length = struct.unpack(">H", data[i + 2:i + 4])[0]
            if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
                height, width = struct.unpack(">HH", data[i + 5:i + 9])
                return "jpeg", width, height
            i += 2 + length
    return None


def _tool_names_by_id(body: dict) -> dict:
    names = {}
    for message in body.get("messages", []) if isinstance(body.get("messages"), list) else []:
        content = message.get("content") if isinstance(message, dict) else None
        for block in content if isinstance(content, list) else []:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                names[block.get("id")] = block.get("name")
    return names


def image_blocks(body: dict) -> list[dict]:
    """Every image block of a Messages request body with where it sits (``tool_result:<tool>`` or
    ``other``) and its facts: declared media type, decoded size and header dimensions. The bytes are
    decoded in memory and dropped."""
    found: list[dict] = []
    names = _tool_names_by_id(body)

    def facts(block: dict, where: str) -> None:
        source = block.get("source") if isinstance(block.get("source"), dict) else {}
        try:
            data = base64.b64decode(source.get("data", ""), validate=True) if source.get("type") == "base64" else b""
        except ValueError:
            data = b""
        info = image_info(data)
        found.append({"where": where, "media_type": source.get("media_type"), "bytes": len(data),
                      "format": info[0] if info else None, "width": info[1] if info else None,
                      "height": info[2] if info else None})

    def walk(node, where: str) -> None:
        if isinstance(node, dict):
            if node.get("type") == "image":
                facts(node, where)
                return
            if node.get("type") == "tool_result":
                inner = f"tool_result:{short_tool(names.get(node.get('tool_use_id')))}"
                walk(node.get("content"), inner)
                return
            for value in node.values():
                walk(value, where)
        elif isinstance(node, list):
            for item in node:
                walk(item, where)

    walk(body, "other")
    return found


PNG_IN_TEXT = re.compile(r"iVBORw0KGgo[A-Za-z0-9+/=]{200,}")


def short_tool(full: str | None) -> str:
    return full.removeprefix(f"mcp__{SERVER_NAME}__") if isinstance(full, str) else "unknown"


def check_pixel_requests(requests: list[dict], *, sentinels=(), effort: str | None = None) -> tuple[list[str], dict]:
    """(closed problem tokens, facts) for the Messages requests a pixel session sent: the tool list is
    exactly the three pixel tools, images appear only inside the result of ``screenshot`` as PNG within
    the limits, no image travels as text and no canary sentinel is in any request."""
    problems: list[str] = []
    if not requests:
        return ["no_request_captured"], {"requests": 0}
    wanted = sorted(ALLOWED_TOOL_NAMES)
    offered, screenshot_results, max_dim, max_bytes, per_request = False, 0, 0, 0, []
    efforts: set = set()
    for request in requests:
        try:
            body = json.loads(request["body"])
        except ValueError:
            problems.append("request_unreadable")
            continue
        if not isinstance(body, dict):
            problems.append("request_unreadable")
            continue
        if "tools" in body:
            offered = True
            efforts.add(str((body.get("output_config") or {}).get("effort")))
            names = sorted(str(t.get("name")) for t in body["tools"] if isinstance(t, dict)) \
                if isinstance(body["tools"], list) else []
            if names != wanted:
                problems.append("tools_not_the_three")
        images = image_blocks(body)
        per_request.append(sum(1 for i in images if i["where"] == "tool_result:screenshot"))
        for image in images:
            if image["where"] != "tool_result:screenshot":
                problems.append("image_outside_screenshot")
                continue
            screenshot_results += 1
            if image["format"] != "png" or image["media_type"] != "image/png":
                problems.append("image_not_png")
            if image["bytes"] > cap.MAX_IMAGE_BYTES:
                problems.append("image_too_large")
            if not image["width"] or max(image["width"], image["height"]) > cap.MAX_IMAGE_EDGE:
                problems.append("image_too_wide")
            max_dim = max(max_dim, image["width"] or 0, image["height"] or 0)
            max_bytes = max(max_bytes, image["bytes"])
        text = decoded_text(body)
        if PNG_IN_TEXT.search(text):
            problems.append("image_as_text")
        if any(s in text for s in sentinels):
            problems.append("sentinel_in_request")
    if not offered:
        problems.append("no_request_offers_tools")
    if effort is not None and efforts != {effort}:
        problems.append("effort_differs")
    return sorted(set(problems)), {"requests": len(requests), "request_tools": wanted if offered else [],
                                   "screenshot_result_images": screenshot_results,
                                   "max_image_edge": max_dim, "max_image_bytes": max_bytes,
                                   "images_per_request": per_request}


def decoded_text(node) -> str:
    """Every string of a request body (keys and values), joined by newlines, *except* the base64 data
    of proper image blocks: what the recipient reads as text."""
    out: list[str] = []

    def walk(value):
        if isinstance(value, str):
            out.append(value)
        elif isinstance(value, dict):
            if value.get("type") == "image" and isinstance(value.get("source"), dict):
                value = {k: v for k, v in value.items() if k != "source"}
            for key, item in value.items():
                out.append(str(key))
                walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)
    walk(node)
    return "\n".join(out)


def image_token_estimate(images_per_request: list[list[tuple[int, int]]], provider: str) -> int | None:
    """The estimated image-token share (decision 9): every appearance of an image in the history sent
    with each request, by the recipient's documented formula (Anthropic: ceil(w/28) * ceil(h/28), 1 196
    for 1280 by 720). A provider with no documented formula gets ``None`` ("unavailable"). No pixels are
    kept for this: ``images_per_request`` is the list of (width, height) per request."""
    if provider != "anthropic":
        return None
    return sum(math.ceil(w / 28) * math.ceil(h / 28) for images in images_per_request for w, h in images)


# ---- the init attestation of a pixel session ----------------------------------------------------

def init_problems(att: dict | None, proof: dict | None, config_dir: Path | None = None) -> list[str]:
    """``run_goal.init_problems`` for the pixel tool set: exactly the three pixel tools, the one
    connected ``pixel`` server, nothing loaded."""
    if att is None:
        return ["no_init_attestation"]
    problems = []
    version = att.get("claude_code_version")
    if not isinstance(version, str) or not rg.VERSION.match(version):
        problems.append("version_missing")
    elif proof is not None and version != proof["claude_code_version"]:
        problems.append("version_differs_from_proof")
    model = att.get("model")
    if not isinstance(model, str) or not rg.MODEL_NAME.match(model):
        problems.append("model_missing")
    elif proof is not None and model != proof["model"]:
        problems.append("model_differs_from_proof")
    if att.get("tools") != sorted(ALLOWED_TOOL_NAMES):
        problems.append("tools_differ")
    if att.get("mcp_servers") != [(SERVER_NAME, "connected")]:
        problems.append("mcp_servers_differ")
    for key in ("skills", "slash_commands", "additional_directories"):
        if att.get(key) != []:
            problems.append(f"{key}_loaded")
    if (token := rg.memory_problem(att.get("memory_paths"), config_dir)):
        problems.append(token)
    if proof is not None:
        if att.get("plugins") != sorted(proof["plugins"]) or att.get("plugin_hashes") != sorted(proof["plugin_hashes"]):
            problems.append("plugins_differ")
        if att.get("agents") != sorted(proof["agents"]) or att.get("agent_hashes") != sorted(proof["agent_hashes"]):
            problems.append("agents_differ")
        if att.get("init_keys_sha256") != proof["init_keys_sha256"]:
            problems.append("init_fields_differ")
    return problems


# ---- the stream ----------------------------------------------------------------------------------

PIXEL_STATUSES = ("simulated", "input_delivered")


def tool_short_name(name: str) -> str:
    for full, short in zip(ALLOWED_TOOL_NAMES, TOOLS):
        if name == full:
            return short
    return rg.UNKNOWN


def _header_dims(data_b64: str) -> tuple[int, int] | None:
    """Dimensions from the PNG header of a base64 prefix; the rest of the image is never decoded."""
    try:
        head = base64.b64decode(data_b64[:48] + "=" * (-len(data_b64[:48]) % 4))
    except ValueError:
        return None
    return cap.png_info(head + b"\0" * max(0, 33 - len(head)))


class PixelStream(rg.StreamMetrics):
    """``StreamMetrics`` for the pixel tools. Keeps counts, closed codes, token numbers, timings,
    screenshot dimensions and booleans; never a pixel, a hash of one, a coordinate list or a text."""

    def __init__(self, *a, provider: str | None = None, **k):
        super().__init__(*a, **k)
        # The recipient of the pixels, from the configured route (``PixelCLI.provider``), never from the init
        # event (whose decoder defaults a missing provider to Anthropic) and never from a model name.
        self.recipient = provider
        self.screenshots: list[dict] = []
        self.primitives = {"click": 0, "type_text": 0}
        self.refusals: dict[str, int] = {}
        self.shots_by_message: dict[str, int] = {}
        self.images_per_message: list[list[tuple[int, int]]] = []
        self._shots_so_far: list[tuple[int, int]] = []

    @property
    def unexpected_tools(self) -> list[str]:
        return [t for t in (self.attestation or {}).get("tools") or [] if t not in ALLOWED_TOOL_NAMES]

    def feed(self, line: str) -> None:
        before = self.attestation
        super().feed(line)
        if self.attestation is not None and self.attestation is not before:  # an init event: judge it by the pixel tools
            self.init_problems = init_problems(self.attestation, self.proof, self.config_dir)

    def _assistant(self, message: dict, now: float) -> None:
        if isinstance(message.get("usage"), dict) and message.get("id") and message["id"] not in self.usage_by_message:
            # every appearance of an image in the retained history, one request per assistant message
            self.images_per_message.append(list(self._shots_so_far))
        super()._assistant(message, now)

    def _tool_use(self, block: dict, now: float) -> None:
        name = tool_short_name(str(block.get("name")))
        self.tool_calls[name] = self.tool_calls.get(name, 0) + 1
        record = {"tool": name, "t_sent": now}
        arguments = block.get("input") if isinstance(block.get("input"), dict) else {}
        if name in self.primitives:
            self.primitives[name] += 1
        if name == "type_text":
            text = arguments.get("text")
            record["action"] = {"action": "type", "dry_run": True, "label": "unknown",
                                "text_ok": (text == self.expected_text) if isinstance(text, str) else None,
                                "observed_before": False}
        self.pending[str(block.get("id"))] = record

    def _user(self, message: dict, now: float) -> None:
        for block in message.get("content", []) if isinstance(message.get("content"), list) else []:
            if not isinstance(block, dict) or block.get("type") != "tool_result":
                continue
            record = self.pending.pop(str(block.get("tool_use_id")), None)
            if record is None:
                continue
            step = {"tool": record["tool"], "ms": round((now - record["t_sent"]) * 1000, 1),
                    "is_error": bool(block.get("is_error"))}
            parts = block.get("content") if isinstance(block.get("content"), list) else []
            for part in parts:
                if not isinstance(part, dict):
                    continue
                if part.get("type") == "image" and record["tool"] == "screenshot":
                    source = part.get("source") if isinstance(part.get("source"), dict) else {}
                    dims = _header_dims(source.get("data", "")) if isinstance(source.get("data"), str) else None
                    if dims:
                        self.screenshots.append({"width": dims[0], "height": dims[1]})
                        self._shots_so_far.append(dims)
                        step["width"], step["height"] = dims
                elif part.get("type") == "text" and isinstance(part.get("text"), str):
                    body = _json_or_none(part["text"])
                    if isinstance(body, dict):
                        if body.get("status") in PIXEL_STATUSES:
                            step["status"] = body["status"]
                        error = body.get("error")
                        if isinstance(error, dict) and error.get("code") in ps.RESULT_CODES:
                            step["error_code"] = error["code"]
                            self.refusals[error["code"]] = self.refusals.get(error["code"], 0) + 1
            if "action" in record:
                self.actions.append(record["action"] | {k: step[k] for k in ("status", "error_code") if k in step})
            self.steps.append(step)

    def summary(self) -> dict:
        base = super().summary()
        provider = self.recipient
        base |= {
            "tools_listed": [tool_short_name(t) for t in (self.attestation or {}).get("tools") or []],
            "unexpected_tool_count": len(self.unexpected_tools),
            "screenshots": len(self.screenshots),
            "screenshot_dims": sorted({(s["width"], s["height"]) for s in self.screenshots}),
            "primitive_input_attempts": sum(self.primitives.values()),
            "primitives": dict(self.primitives),
            "refusal_codes": dict(sorted(self.refusals.items())),
            "image_token_estimate": image_token_estimate(self.images_per_message, provider),
            "image_token_estimate_basis": ("every retained appearance per request, Anthropic formula"
                                           if provider == "anthropic" else
                                           "unavailable: no documented formula for this recipient"),
        }
        base.pop("chosen_labels", None)
        base.pop("actions_observed_after", None)
        return base


def _json_or_none(text: str):
    try:
        return json.loads(text)
    except ValueError:
        return None
