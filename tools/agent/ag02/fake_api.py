#!/usr/bin/env python3
"""A local stand-in for the Anthropic Messages API, for the AG-02 isolation preflight.

It listens on 127.0.0.1 only, records the raw body of every request in memory (the preflight
searches them for canary sentinels; nothing is written to disk by this module) and answers
``POST /v1/messages`` with a one-sentence assistant message that ends in ``VERDICT: DONE``,
streamed (server-sent events) or not, as the client asked. Every other path answers 404.
The bodies it records are the synthetic canary session's, never a real one.
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


class FakeApi:
    """``with FakeApi() as api:`` then ``api.url`` and, afterwards, ``api.bodies``."""

    def __init__(self):
        self.bodies: list[bytes] = []
        self.paths: list[str] = []
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
                outer.paths.append(self.path.split("?")[0])
                if self.path.split("?")[0] != "/v1/messages":
                    self._reply(404, b"{}", "application/json")
                    return
                outer.bodies.append(body)
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
