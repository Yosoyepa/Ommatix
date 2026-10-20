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
# The input schemas the real engine lists (Ommatix 1.0), copied so the agent's validator is tested on them.
# The numbers-only `_meta` of a real observation (`io.ommatix/timings`, `io.ommatix/plan`).
OBSERVE_META = {
    "io.ommatix/timings": {"capture_us": 1000, "plan_us": 10, "detect_us": 2000, "recognize_us": 3000,
                           "accessibility_us": 4000, "fuse_us": 50, "track_us": 20, "commit_us": 30,
                           "total_us": 10110, "response_build_us": 40},
    "io.ommatix/plan": {"work": "full", "full_refresh_reason": "no_committed_base", "region_count": 0,
                        "region_area_px": 0, "inference_count": 1, "input_shapes": None,
                        "accessibility_truncated": False, "controls_n": 1, "words_n": 0,
                        "supports_n": 0, "relations_n": 0}}
REAL_SCHEMAS = {
    "observe_screen": {
        "additionalProperties": False, "type": "object", "required": [],
        "properties": {"cursor": {"maxLength": 128, "minLength": 1, "type": "string"},
                       "include_values": {"default": False, "type": "boolean"},
                       "limit": {"default": 100, "maximum": 200, "minimum": 1, "type": "integer"},
                       "query": {"maxLength": 256, "type": "string"}}},
    "get_engine_metrics": {"additionalProperties": False, "properties": {}, "required": [], "type": "object"},
    "execute_action": {
        "additionalProperties": False, "type": "object",
        "allOf": [{"else": {"not": {"required": ["text"]}},
                   "if": {"properties": {"action": {"const": "type"}}, "required": ["action"]},
                   "then": {"required": ["text"]}},
                  {"else": {"not": {"anyOf": [{"required": ["direction"]}, {"required": ["steps"]}]}},
                   "if": {"properties": {"action": {"const": "scroll"}}, "required": ["action"]},
                   "then": {"required": ["direction", "steps"]}}],
        "properties": {"action": {"enum": ["click", "type", "scroll"]}, "direction": {"enum": ["up", "down"]},
                       "dry_run": {"default": True, "type": "boolean"},
                       "expected_effect": {"default": "none", "enum": [
                           "none", "focus_changed", "value_updated", "scroll_moved", "visual_change"]},
                       "expected_revision": {"minimum": 1, "type": "integer"},
                       "geometry_epoch": {"minimum": 1, "type": "integer"},
                       "request_id": {"maxLength": 64, "minLength": 1, "type": "string"},
                       "session_id": {"maxLength": 64, "minLength": 1, "type": "string"},
                       "snapshot_id": {"maxLength": 64, "minLength": 1, "type": "string"},
                       "steps": {"maximum": 5, "minimum": 1, "type": "integer"},
                       "target_id": {"maxLength": 64, "minLength": 1, "type": "string"},
                       "text": {"maxLength": 2048, "minLength": 1, "type": "string"}},
        "required": ["request_id", "session_id", "snapshot_id", "geometry_epoch", "target_id",
                     "expected_revision", "action"]},
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--received", required=True)
    parser.add_argument("--real-schemas", action="store_true",
                        help="list the input schemas of the real engine (observe_screen, execute_action)")
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
                 "inputSchema": (REAL_SCHEMAS.get(name) if args.real_schemas and name in REAL_SCHEMAS else
                                 {"type": "object", "properties": {}, "additionalProperties": True})}
                for name in ("observe_screen", "execute_action", "get_engine_metrics",
                             *([args.extra_tool] if args.extra_tool else []))]}}
        elif message.get("method") != "tools/call":
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"ok": True}}
        elif params.get("name") == "observe_screen":
            reply = {"jsonrpc": "2.0", "id": rid, "result": {"isError": False, "structuredContent": {
                "schema_version": "1.0", "session_id": "fake", "snapshot_id": "s1", "geometry_epoch": 1,
                "elements": [{"target_id": "e1", "revision": 1, "label": SECRET_LABEL,
                              "actionability": "confirmed"}],
                "total_elements": 1, "truncated": False},
                "_meta": OBSERVE_META}}
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
