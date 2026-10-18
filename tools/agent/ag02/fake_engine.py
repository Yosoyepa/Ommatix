#!/usr/bin/env python3
"""A stand-in for ``ommatix serve`` for the AG-02 tests: line-delimited JSON-RPC on stdio.

It answers ``tools/call`` like the engine's shapes (``observe_screen`` with one synthetic
element whose label is a marker the privacy tests look for; ``execute_action`` with
``simulated`` or ``input_delivered``) and appends every line it receives, verbatim, to the
file named by ``--received``, so a test can prove what the proxy did NOT forward.
"""

import argparse
import json
import sys
import time

SECRET_LABEL = "ETIQUETA-SECRETA"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--received", required=True)
    parser.add_argument("--extra-tool", default=None,
                        help="list this fourth tool in tools/list (a server that offers more than three)")
    parser.add_argument("--delay-action-ms", type=int, default=0,
                        help="answer execute_action this late (tests of in-flight ids)")
    args = parser.parse_args()
    seen = open(args.received, "ab")
    for raw in sys.stdin.buffer:
        seen.write(raw if raw.endswith(b"\n") else raw + b"\n")
        seen.flush()
        try:
            message = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(message, dict) or "id" not in message:
            continue
        rid = message["id"]
        params = message.get("params") or {}
        if message.get("method") == "tools/list":
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"tools": [
                {"name": name, "description": f"fake {name}",
                 "inputSchema": {"type": "object", "properties": {}, "additionalProperties": True}}
                for name in ("observe_screen", "execute_action", "get_engine_metrics",
                             *([args.extra_tool] if args.extra_tool else []))]}}
        elif message.get("method") != "tools/call":
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"ok": True}}
        elif params.get("name") == "observe_screen":
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"isError": False, "structuredContent": {
                "schema_version": "1.0", "session_id": "fake", "snapshot_id": "s1", "geometry_epoch": 1,
                "elements": [{"target_id": "e1", "revision": 1, "label": SECRET_LABEL,
                              "actionability": "confirmed"}]}}}
        elif params.get("name") == "execute_action":
            live = (params.get("arguments") or {}).get("dry_run") is False
            time.sleep(args.delay_action_ms / 1000)
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"isError": False, "structuredContent": {
                "schema_version": "1.0", "request_id": "r", "target_id": "e1", "snapshot_id": "s1",
                "status": "input_delivered" if live else "simulated",
                "input_delivered": live, "verified": False}}}
        else:
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"isError": True, "structuredContent": {
                "schema_version": "1.0", "status": "rejected", "error": {
                    "code": "TARGET_NOT_FOUND", "message": SECRET_LABEL, "retryable": False,
                    "next_step": "observe_again"}}}}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
