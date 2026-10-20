#!/usr/bin/env python3
"""A scripted stand-in for the Anthropic Messages API, on 127.0.0.1 only (AG-03 probe, preflight, tests).

``fake_api.FakeApi`` answers every Messages request with one fixed text. The AG-03 steps need a model
that *calls tools*: ``ScriptedMessages(script)`` answers the Nth request that offers tools with the
Nth step - ``{"tool": "mcp__pixel__screenshot", "input": {...}}`` (a ``tool_use`` block) or
``{"text": "..."}`` - and a request with no tools (a title, a quota probe) with plain text. A step
list may be a callable ``f(request_body: dict, n: int) -> step`` for scripts that depend on the
request. Every recognised POST is kept in memory only: ``requests`` (path, lower-cased header names,
body bytes). Nothing is written to disk and nothing leaves the machine.
"""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

FINAL_TEXT = "VERDICT: DONE"


def _usage(output: int = 5) -> dict:
    return {"input_tokens": 10, "output_tokens": output}


def _blocks(step: dict, n: int) -> tuple[list[dict], str]:
    if "tool" in step:
        return [{"type": "tool_use", "id": f"toolu_{n:04d}", "name": step["tool"],
                 "input": step.get("input", {})}], "tool_use"
    return [{"type": "text", "text": step.get("text", FINAL_TEXT)}], "end_turn"


def message_json(model: str, step: dict, n: int) -> dict:
    content, stop = _blocks(step, n)
    return {"id": f"msg_stub_{n:04d}", "type": "message", "role": "assistant", "model": model, "content": content,
            "stop_reason": stop, "stop_sequence": None, "usage": _usage()}


def message_sse(model: str, step: dict, n: int) -> bytes:
    full = message_json(model, step, n)
    start = full | {"content": [], "stop_reason": None, "usage": _usage(1)}
    events = [("message_start", {"type": "message_start", "message": start})]
    for index, block in enumerate(full["content"]):
        if block["type"] == "tool_use":
            events += [("content_block_start", {"type": "content_block_start", "index": index,
                                                "content_block": block | {"input": {}}}),
                       ("content_block_delta", {"type": "content_block_delta", "index": index,
                                                "delta": {"type": "input_json_delta",
                                                          "partial_json": json.dumps(block["input"])}})]
        else:
            events += [("content_block_start", {"type": "content_block_start", "index": index,
                                                "content_block": {"type": "text", "text": ""}}),
                       ("content_block_delta", {"type": "content_block_delta", "index": index,
                                                "delta": {"type": "text_delta", "text": block["text"]}})]
        events.append(("content_block_stop", {"type": "content_block_stop", "index": index}))
    events += [("message_delta", {"type": "message_delta",
                                  "delta": {"stop_reason": full["stop_reason"], "stop_sequence": None},
                                  "usage": {"output_tokens": 5}}),
               ("message_stop", {"type": "message_stop"})]
    return b"".join(f"event: {name}\ndata: {json.dumps(data)}\n\n".encode() for name, data in events)


class ScriptedMessages:
    """``with ScriptedMessages(script) as api:`` then ``api.url``; afterwards ``api.requests``."""

    def __init__(self, script):
        self.script = script
        self.requests: list[dict] = []
        self.paths: list[str] = []
        self.gets: list[str] = []
        self._tool_requests = 0
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _reply(self, code, body, kind):
                self.send_response(code)
                self.send_header("Content-Type", kind)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                length = int(self.headers.get("Content-Length") or 0)
                body = self.rfile.read(length) if length else b""
                path = self.path.split("?")[0]
                outer.paths.append(path)
                if path != "/v1/messages":
                    self._reply(404, b"{}", "application/json")
                    return
                outer.requests.append({"path": path, "body": body,
                                       "headers": {k.lower(): v for k, v in self.headers.items()}})
                try:
                    request = json.loads(body)
                except ValueError:
                    request = {}
                request = request if isinstance(request, dict) else {}
                model = str(request.get("model", "stub"))
                if request.get("tools"):
                    n = outer._tool_requests
                    outer._tool_requests += 1
                    if callable(outer.script):
                        step = outer.script(request, n)
                    else:
                        step = outer.script[n] if n < len(outer.script) else {"text": FINAL_TEXT}
                else:
                    n, step = 9000 + len(outer.requests), {"text": "title"}
                if request.get("stream"):
                    self._reply(200, message_sse(model, step, n), "text/event-stream")
                else:
                    self._reply(200, json.dumps(message_json(model, step, n)).encode(), "application/json")

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
