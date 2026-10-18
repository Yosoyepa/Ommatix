#!/usr/bin/env python3
"""A scripted stand-in for the agent's MCP client, for the AG-02 dry rehearsal.

It speaks line-delimited JSON-RPC on stdio (the proxy's stdin/stdout), legacy profile
(``initialize``) or the modern one (``_meta`` per request), observes once, then sends one dry
run and ``--live-calls`` live clicks on the first confirmed element. It writes a metadata-only
report (statuses, error codes, proxy refusals) to ``--report``. It sends no input to anything:
what a live call does is up to the engine behind the proxy (the simulated desktop).
"""

import argparse
import json
import sys

META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "ag02-fake-agent", "version": "1"}}


class Wire:
    def __init__(self, modern: bool):
        self.modern, self.next_id = modern, 1

    def request(self, method: str, params: dict) -> dict:
        rid, self.next_id = self.next_id, self.next_id + 1
        if self.modern:
            params = {"_meta": META, **params}
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method,
                                     "params": params}) + "\n")
        sys.stdout.flush()
        line = sys.stdin.readline()
        reply = json.loads(line)
        assert reply.get("id") == rid, "reply id mismatch"
        return reply

    def notify(self, method: str) -> None:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
        sys.stdout.flush()

    def call(self, name: str, arguments: dict) -> dict:
        return self.request("tools/call", {"name": name, "arguments": arguments})


def outcome(reply: dict) -> dict:
    if "error" in reply:
        return {"rpc_error": reply["error"].get("data", {}).get("proxy_refusal", reply["error"].get("code"))}
    body = reply["result"].get("structuredContent", {})
    return {"status": body.get("status"), "error_code": (body.get("error") or {}).get("code"),
            "input_delivered": body.get("input_delivered")}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", required=True)
    parser.add_argument("--profile", choices=("legacy", "modern"), default="legacy")
    parser.add_argument("--live-calls", type=int, default=3)
    args = parser.parse_args()
    wire = Wire(args.profile == "modern")
    if args.profile == "legacy":
        wire.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                    "clientInfo": {"name": "ag02-fake-agent", "version": "1"}})
        wire.notify("notifications/initialized")
    report = {"observe": None, "dry": None, "live": []}
    observed = wire.call("observe_screen", {"limit": 50})
    body = observed["result"]["structuredContent"]
    report["observe"] = {"elements": len(body.get("elements", []))}
    element = next((e for e in body["elements"] if e.get("actionability") == "confirmed"),
                   body["elements"][0])

    def action(n: int, dry_run: bool) -> dict:
        return {"request_id": f"fake-{n}", "session_id": body["session_id"],
                "snapshot_id": body["snapshot_id"], "geometry_epoch": body["geometry_epoch"],
                "target_id": element["target_id"], "expected_revision": element["revision"],
                "action": "click", "dry_run": dry_run, "expected_effect": "visual_change"}

    report["dry"] = outcome(wire.call("execute_action", action(0, True)))
    for n in range(1, args.live_calls + 1):
        report["live"].append(outcome(wire.call("execute_action", action(n, False))))
    with open(args.report, "w", encoding="utf-8") as handle:
        json.dump(report, handle)
    return 0


if __name__ == "__main__":
    sys.exit(main())
