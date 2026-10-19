#!/usr/bin/env python3
"""A local stand-in for the model APIs the AG-02 agents speak, for the isolation preflights.

Three wire formats, all on 127.0.0.1 and none reaching anything else: the Anthropic Messages API
(``/v1/messages``, Claude Code), the OpenAI-compatible Chat Completions API (``.../chat/completions``,
the minimal agent and OpenCode) and the OpenAI Responses API (``.../responses``, Codex). For the
last two, ``script`` lists what the model "says" to a request that offers tools - ``{"tool": name,
"args": {...}}`` or ``{"text": "..."}`` - and a request with no tools (a title, say) gets plain text.
Every recognised POST is kept: ``bodies`` (raw bytes) and ``requests`` (path, lower-cased header
names and values, the body), so a preflight can inspect the real request, its headers and its tool
list. Only the preflight's own dummy key and synthetic canary text ever reach it.

Records are in memory only; nothing is written to disk by this module. Every other path answers 404
(``gets`` and ``paths`` keep what was asked, so a preflight can see what an agent tried to reach).
"""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ANSWER = "VERDICT: DONE"


def _message(model: str) -> dict:
    return {"id": "msg_canary", "type": "message", "role": "assistant", "model": model,
            "content": [{"type": "text", "text": ANSWER}], "stop_reason": "end_turn",
            "stop_sequence": None, "usage": {"input_tokens": 10, "output_tokens": 5}}


def _sse(model: str) -> bytes:
    start = _message(model) | {"content": [], "stop_reason": None}
    start["usage"] = {"input_tokens": 10, "output_tokens": 1}
    events = [
        ("message_start", {"type": "message_start", "message": start}),
        ("content_block_start", {"type": "content_block_start", "index": 0,
                                 "content_block": {"type": "text", "text": ""}}),
        ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                 "delta": {"type": "text_delta", "text": ANSWER}}),
        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
        ("message_delta", {"type": "message_delta",
                           "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                           "usage": {"output_tokens": 5}}),
        ("message_stop", {"type": "message_stop"}),
    ]
    return b"".join(f"event: {name}\ndata: {json.dumps(data)}\n\n".encode() for name, data in events)


def _chat_reply(model: str, step: dict, n: int, stream: bool) -> tuple[bytes, str]:
    """A Chat Completions reply: the scripted text or one function call, with cache-aware usage.
    Step keys beyond ``text`` / ``tool`` / ``args``: ``message`` (the whole assistant message, as written),
    ``usage`` (replaces the usage object; ``None`` drops it) and ``reasoning``."""
    message = {"role": "assistant", "content": step.get("text")}
    if "reasoning" in step:  # a thinking model's reasoning_content, which the next request must carry back
        message["reasoning_content"] = step["reasoning"]
    finish = "stop"
    if "tool" in step:
        message["content"] = None
        message["tool_calls"] = [{"id": f"call_{n}", "type": "function", "function": {
            "name": step["tool"], "arguments": json.dumps(step.get("args", {}))}}]
        finish = "tool_calls"
    if "message" in step:
        message = step["message"]
    usage = {"prompt_tokens": 100 + n, "completion_tokens": 5, "total_tokens": 105 + n,
             "prompt_tokens_details": {"cached_tokens": 0}}
    if "usage" in step:
        usage = step["usage"]
    if not stream:
        reply = {"id": f"chatcmpl-{n}", "object": "chat.completion", "created": 1, "model": model,
                 "choices": [{"index": 0, "message": message, "finish_reason": finish}]}
        if usage is not None:
            reply["usage"] = usage
        return json.dumps(reply).encode(), "application/json"
    base = {"id": f"chatcmpl-{n}", "object": "chat.completion.chunk", "created": 1, "model": model}
    delta = {"role": "assistant", "content": message.get("content") or ""}
    if "tool_calls" in message:
        delta["tool_calls"] = [dict(message["tool_calls"][0], index=0)]
    chunks = [base | {"choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
              base | {"choices": [{"index": 0, "delta": {}, "finish_reason": finish}]},
              base | {"choices": [], "usage": usage}]
    return (b"".join(f"data: {json.dumps(c)}\n\n".encode() for c in chunks) + b"data: [DONE]\n\n",
            "text/event-stream")


def _responses_reply(model: str) -> bytes:
    """A Responses API stream: one assistant message (the Codex probe only needs a request)."""
    item = {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": ANSWER, "annotations": []}]}
    events = [
        ("response.created", {"type": "response.created", "response": {
            "id": "resp_1", "object": "response", "model": model, "status": "in_progress", "output": []}}),
        ("response.output_item.added", {"type": "response.output_item.added", "output_index": 0,
                                         "item": item | {"status": "in_progress", "content": []}}),
        ("response.output_text.delta", {"type": "response.output_text.delta", "item_id": "msg_1",
                                         "output_index": 0, "content_index": 0, "delta": ANSWER}),
        ("response.output_item.done", {"type": "response.output_item.done", "output_index": 0, "item": item}),
        ("response.completed", {"type": "response.completed", "response": {
            "id": "resp_1", "object": "response", "model": model, "status": "completed", "output": [item],
            "usage": {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 0}, "output_tokens": 5,
                      "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 15}}}),
    ]
    return b"".join(f"event: {n}\ndata: {json.dumps(d)}\n\n".encode() for n, d in events)


class FakeApi:
    """``with FakeApi() as api:`` then ``api.url`` and, afterwards, ``api.bodies`` / ``api.requests``."""

    def __init__(self, script: list[dict] | None = None):
        self.bodies: list[bytes] = []
        self.paths: list[str] = []
        self.requests: list[dict] = []
        self.gets: list[str] = []
        self.script = list(script or [])
        self._steps = 0
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):  # silence
                pass

            def _reply(self, code: int, body: bytes, content_type: str) -> None:
                self.send_response(code)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                length = int(self.headers.get("Content-Length") or 0)
                body = self.rfile.read(length) if length else b""
                path = self.path.split("?")[0]
                outer.paths.append(path)
                if path.endswith("/chat/completions") or path.endswith("/responses"):
                    outer.bodies.append(body)
                    outer.requests.append({"path": path, "headers": {k.lower(): v for k, v in self.headers.items()},
                                           "body": body})
                    try:
                        request = json.loads(body)
                    except ValueError:
                        request = {}
                    request = request if isinstance(request, dict) else {}
                    model = str(request.get("model", "m"))
                    if path.endswith("/responses"):
                        self._reply(200, _responses_reply(model), "text/event-stream")
                        return
                    if request.get("tools"):
                        step = (outer.script[outer._steps] if outer._steps < len(outer.script)
                                else {"text": ANSWER})
                        outer._steps += 1
                    else:
                        step = {"text": "title"}
                    if "raw" in step:  # bytes sent as they are, optionally with a lying or absent length
                        self.send_response(200)
                        self.send_header("Content-Type", "application/json")
                        if not step.get("undeclared"):
                            self.send_header("Content-Length", str(step.get("declared_length", len(step["raw"]))))
                        self.end_headers()
                        try:
                            self.wfile.write(step["raw"])
                        except OSError:
                            pass
                        self.close_connection = True
                        return
                    data, kind = _chat_reply(model, step, outer._steps, bool(request.get("stream")))
                    self._reply(200, data, kind)
                    return
                if path != "/v1/messages":
                    self._reply(404, b"{}", "application/json")
                    return
                outer.bodies.append(body)
                outer.requests.append({"path": path, "headers": {k.lower(): v for k, v in self.headers.items()},
                                       "body": body})
                try:
                    request = json.loads(body)
                except ValueError:
                    request = {}
                model = request.get("model", "claude-canary") if isinstance(request, dict) else "x"
                if isinstance(request, dict) and request.get("stream"):
                    self._reply(200, _sse(model), "text/event-stream")
                else:
                    self._reply(200, json.dumps(_message(model)).encode(), "application/json")

            def do_GET(self):
                outer.gets.append(self.path.split("?")[0])
                self._reply(404, b"{}", "application/json")

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.server.server_address[1]}"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()
