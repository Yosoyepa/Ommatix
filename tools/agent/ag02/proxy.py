#!/usr/bin/env python3
"""AG-02 MCP budget proxy: a stdio passthrough between an agent and ``ommatix serve``.

The agent's MCP client starts this script as its MCP server; the script starts the engine
(the command after ``--``) and forwards every JSON-RPC line unchanged in both directions,
except for ``tools/call`` of ``execute_action``, which it checks first (fail closed):

* ``--dry-run-only`` (phase 1): a call whose ``dry_run`` is not ``true`` (or absent, the
  contract's default) is answered with a JSON-RPC error and never forwarded.
* ``--max-live N`` (phase 2): each call with ``dry_run: false`` is counted in the ledger file
  *before* it is forwarded; once the ledger holds N, the call is answered with a JSON-RPC
  error and never forwarded. ``--max-live-per-run K`` bounds this process alone. The ledger
  only accumulates across goals; an unreadable ledger refuses every live call.
* Live calls are also limited to the actions in ``--live-actions`` (default ``click,type``)
  and a ``type`` text must be 1..``--max-text-len`` printable ASCII characters.

Anything the proxy cannot read the way the engine would (duplicate keys, ``NaN``, a JSON
batch, a non-boolean ``dry_run``, a ``tools/call`` notification) is refused, not forwarded.

Privacy (AGENTS.md rule 7, ADR-0012): the JSONL log (outside the repository) holds, per
message, only the time, direction, kind, method or tool, byte size, result status or error
code, ``input_delivered``, ``observed_effect``, the element id and revision of an action, the
length (never the text) of a ``type`` and the element count of an observation. Never a label,
a value, an error message or a typed text. The proxy never touches the operator's stop file.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

REFUSAL_CODE = -32001  # implementation-defined server error range of JSON-RPC
INVALID_REQUEST = -32600
LIVE_ACTIONS_DEFAULT = ("click", "type")
MAX_TEXT_LEN_DEFAULT = 16
PRINTABLE_ASCII = frozenset(chr(c) for c in range(0x20, 0x7F))


class Refusal(Exception):
    """The proxy answers instead of forwarding. ``reason`` is a fixed token (no content)."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def _no_duplicates(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError("duplicate key")
        out[key] = value
    return out


def _no_constant(_name):
    raise ValueError("non-JSON constant")


def parse_line(raw: bytes):
    """The JSON value of one line, read strictly (see the module doc); ``None`` if it is not
    valid JSON at all. Raises ``ValueError`` for what the engine might read differently."""
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return None
    if not text.strip():
        return None
    try:
        return json.loads(text, object_pairs_hook=_no_duplicates, parse_constant=_no_constant)
    except ValueError as error:
        if "duplicate key" in str(error) or "non-JSON constant" in str(error):
            raise
        return None


def inside_git_checkout(path: Path) -> bool:
    """Whether `path` lies in a directory tree with a ``.git`` entry (a repository checkout)."""
    here = Path(os.path.abspath(path))
    return any((parent / ".git").exists() for parent in [here, *here.parents])


def read_ledger(path: Path) -> int:
    """The ledger's total; 0 if the file does not exist. Raises ``ValueError`` if corrupt."""
    try:
        text = path.read_text(encoding="ascii").strip()
    except FileNotFoundError:
        return 0
    if not text.isascii() or not text.isdigit() or len(text) > 9:
        raise ValueError("corrupt ledger")
    return int(text)


def write_ledger(path: Path, total: int) -> None:
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(str(total), encoding="ascii")
    os.replace(tmp, path)


class Policy:
    """What a ``tools/call`` of ``execute_action`` may do; the only state is the ledger."""

    def __init__(self, *, dry_run_only: bool, max_live: int | None, per_run: int | None,
                 ledger: Path | None, live_actions=LIVE_ACTIONS_DEFAULT,
                 max_text_len: int = MAX_TEXT_LEN_DEFAULT):
        self.dry_run_only = dry_run_only
        self.max_live = max_live
        self.per_run = per_run
        self.ledger = ledger
        self.live_actions = tuple(live_actions)
        self.max_text_len = max_text_len
        self.spent_here = 0
        self.lock = threading.Lock()

    def check(self, arguments) -> bool:
        """Decide one ``execute_action``. Returns whether it is a live call that was counted;
        raises ``Refusal`` when it must not be forwarded."""
        if not isinstance(arguments, dict):
            raise Refusal("ARGUMENTS_NOT_AN_OBJECT")
        if "dry_run" not in arguments or arguments["dry_run"] is True:
            return False
        if arguments["dry_run"] is not False:
            raise Refusal("DRY_RUN_NOT_BOOLEAN")
        if self.dry_run_only:
            raise Refusal("DRY_RUN_ONLY")
        if self.max_live is None or self.ledger is None:
            raise Refusal("NO_LIVE_BUDGET")
        action = arguments.get("action")
        if action not in self.live_actions:
            raise Refusal("ACTION_NOT_ALLOWED_LIVE")
        if action == "type":
            text = arguments.get("text")
            if (not isinstance(text, str) or not 1 <= len(text) <= self.max_text_len
                    or not set(text) <= PRINTABLE_ASCII):
                raise Refusal("TEXT_NOT_ALLOWED_LIVE")
        with self.lock:
            try:
                total = read_ledger(self.ledger)
            except (OSError, ValueError):
                raise Refusal("LEDGER_UNREADABLE") from None
            if total >= self.max_live or (self.per_run is not None
                                          and self.spent_here >= self.per_run):
                raise Refusal("LIVE_BUDGET_EXHAUSTED")
            try:
                write_ledger(self.ledger, total + 1)
            except OSError:
                raise Refusal("LEDGER_UNWRITABLE") from None
            self.spent_here += 1
        return True


def refusal_message(request_id, code: int, reason: str) -> bytes:
    body = {"jsonrpc": "2.0", "id": request_id,
            "error": {"code": code, "message": f"ommatix-ag02-proxy refused: {reason}",
                      "data": {"proxy_refusal": reason}}}
    return (json.dumps(body, separators=(",", ":")) + "\n").encode("utf-8")


def _tool_call_of(message):
    """(name, arguments) of a ``tools/call`` request/notification, else None."""
    if isinstance(message, dict) and message.get("method") == "tools/call":
        params = message.get("params")
        if isinstance(params, dict):
            return params.get("name"), params.get("arguments")
        return None, None
    return None


def request_meta(message) -> dict:
    """The loggable facts of an agent message: never labels, values or typed text."""
    meta: dict = {"kind": "other"}
    if not isinstance(message, dict):
        return meta
    method = message.get("method")
    meta["kind"] = "request" if "id" in message and method else "notification" if method else "other"
    if isinstance(method, str):
        meta["method"] = method[:64]
    call = _tool_call_of(message)
    if call is not None and isinstance(call[0], str):
        meta["tool"] = call[0][:64]
        args = call[1] if isinstance(call[1], dict) else {}
        if call[0] == "execute_action":
            meta["action"] = args.get("action") if args.get("action") in (
                "click", "type", "scroll") else "invalid"
            meta["dry_run"] = args.get("dry_run", True) if isinstance(
                args.get("dry_run", True), bool) else "invalid"
            if isinstance(args.get("target_id"), str):
                meta["element_id"] = args["target_id"][:64]
            if isinstance(args.get("expected_revision"), int):
                meta["revision"] = args["expected_revision"]
            if isinstance(args.get("text"), str):
                meta["text_len"] = len(args["text"])
    return meta


def response_meta(message, tool: str | None) -> dict:
    meta: dict = {"kind": "response"}
    if not isinstance(message, dict):
        return meta
    if "error" in message and isinstance(message["error"], dict):
        meta["rpc_error_code"] = message["error"].get("code") if isinstance(
            message["error"].get("code"), int) else None
        return meta
    result = message.get("result")
    if not isinstance(result, dict):
        return meta
    meta["is_error"] = bool(result.get("isError"))
    structured = result.get("structuredContent")
    if isinstance(structured, dict):
        status = structured.get("status")
        if isinstance(status, str):
            meta["status"] = status[:32]
        error = structured.get("error")
        if isinstance(error, dict) and isinstance(error.get("code"), str):
            meta["error_code"] = error["code"][:32]
        if isinstance(structured.get("input_delivered"), bool):
            meta["input_delivered"] = structured["input_delivered"]
        if isinstance(structured.get("verified"), bool):
            meta["verified"] = structured["verified"]
        if isinstance(structured.get("observed_effect"), str):
            meta["observed_effect"] = structured["observed_effect"][:32]
        if tool == "observe_screen" and isinstance(structured.get("elements"), list):
            meta["elements"] = len(structured["elements"])
    return meta


class Proxy:
    def __init__(self, engine_argv, policy: Policy, log_path: Path, *, stdin=None, stdout=None):
        self.engine_argv = list(engine_argv)
        self.policy = policy
        self.log_path = log_path
        self.stdin = stdin if stdin is not None else sys.stdin.buffer
        self.stdout = stdout if stdout is not None else sys.stdout.buffer
        self.out_lock = threading.Lock()
        self.log_lock = threading.Lock()
        self.pending: dict = {}  # id -> (tool, monotonic time sent)
        self.t0 = time.monotonic()
        self.log_file = None

    def log(self, direction: str, size: int, **fields) -> None:
        record = {"t_ms": round((time.monotonic() - self.t0) * 1000, 1), "dir": direction,
                  "bytes": size, **fields}
        with self.log_lock:
            self.log_file.write(json.dumps(record, ensure_ascii=True) + "\n")
            self.log_file.flush()

    def to_agent(self, data: bytes) -> None:
        with self.out_lock:
            self.stdout.write(data)
            self.stdout.flush()

    def refuse(self, request_id, reason: str, code: int = REFUSAL_CODE, **fields) -> None:
        data = refusal_message(request_id, code, reason)
        self.log("proxy->agent", len(data), kind="refusal", reason=reason, **fields)
        self.to_agent(data)

    def handle_agent_line(self, raw: bytes, engine_in) -> None:
        size = len(raw)
        try:
            message = parse_line(raw)
        except ValueError:
            self.refuse(None, "AMBIGUOUS_JSON", INVALID_REQUEST)
            return
        if isinstance(message, list):
            self.refuse(None, "BATCH_NOT_ALLOWED", INVALID_REQUEST)
            return
        meta = request_meta(message)
        call = _tool_call_of(message)
        request_id = message.get("id") if isinstance(message, dict) else None
        live = False
        if call is not None:
            if request_id is None or isinstance(request_id, bool) or not isinstance(
                    request_id, (int, str)):
                self.log("agent->proxy", size, **meta)
                self.refuse(None, "TOOL_CALL_WITHOUT_ID", INVALID_REQUEST)
                return
            if call[0] == "execute_action":
                try:
                    live = self.policy.check(call[1])
                except Refusal as refusal:
                    self.log("agent->proxy", size, **meta)
                    self.refuse(request_id, refusal.reason, **{"tool": "execute_action"})
                    return
        self.log("agent->engine", size, live_counted=live, **meta)
        if request_id is not None and isinstance(request_id, (int, str)) and not isinstance(
                request_id, bool) and isinstance(message, dict) and "method" in message:
            self.pending[request_id] = (meta.get("tool"), time.monotonic())
        engine_in.write(raw if raw.endswith(b"\n") else raw + b"\n")
        engine_in.flush()

    def pump_engine(self, engine_out) -> None:
        for raw in engine_out:
            size = len(raw)
            try:
                message = parse_line(raw)
            except ValueError:
                message = None
            tool, started = None, None
            if isinstance(message, dict) and "id" in message and "method" not in message:
                rid = message["id"]
                if isinstance(rid, (int, str)) and not isinstance(rid, bool):
                    tool, started = self.pending.pop(rid, (None, None))
            meta = response_meta(message, tool)
            if tool:
                meta["tool"] = tool
            if started is not None:
                meta["latency_ms"] = round((time.monotonic() - started) * 1000, 1)
            self.log("engine->agent", size, **meta)
            self.to_agent(raw if raw.endswith(b"\n") else raw + b"\n")

    def run(self) -> int:
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self.log_file = open(self.log_path, "a", encoding="utf-8")
        flags = 0x08000000 if os.name == "nt" else 0  # CREATE_NO_WINDOW
        engine = subprocess.Popen(self.engine_argv, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=None, creationflags=flags)
        pump = threading.Thread(target=self.pump_engine, args=(engine.stdout,), daemon=True)
        pump.start()
        try:
            for raw in self.stdin:
                if engine.poll() is not None:
                    break
                try:
                    self.handle_agent_line(raw, engine.stdin)
                except (BrokenPipeError, OSError):
                    break
        finally:
            try:
                engine.stdin.close()
            except OSError:
                pass
            try:
                code = engine.wait(timeout=30)
            except subprocess.TimeoutExpired:
                engine.kill()
                code = engine.wait()
            pump.join(timeout=5)
            self.log("proxy", 0, kind="end", engine_exit=code, live_spent_here=self.policy.spent_here)
            self.log_file.close()
        return code


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run-only", action="store_true",
                      help="phase 1: refuse any call whose dry_run is not true")
    mode.add_argument("--max-live", type=int, metavar="N",
                      help="phase 2: the ledger's total may not exceed N live calls")
    parser.add_argument("--max-live-per-run", type=int, metavar="K",
                        help="also bound the live calls of this process alone")
    parser.add_argument("--ledger", type=Path, help="ledger file (required with --max-live)")
    parser.add_argument("--log", type=Path, required=True,
                        help="metadata JSONL, outside the repository checkout")
    parser.add_argument("--live-actions", default=",".join(LIVE_ACTIONS_DEFAULT))
    parser.add_argument("--max-text-len", type=int, default=MAX_TEXT_LEN_DEFAULT)
    parser.add_argument("engine", nargs=argparse.REMAINDER, help="-- ENGINE_COMMAND ...")
    args = parser.parse_args(argv)
    engine = args.engine[1:] if args.engine[:1] == ["--"] else args.engine
    if not engine:
        parser.error("the engine command is missing (after --)")
    args.engine = engine
    if args.max_live is not None:
        if args.max_live < 0 or args.ledger is None:
            parser.error("--max-live needs N >= 0 and --ledger")
    if args.max_live_per_run is not None and args.max_live_per_run < 0:
        parser.error("--max-live-per-run must be >= 0")
    for path in (args.log, args.ledger):
        if path is not None and inside_git_checkout(path):
            parser.error("log and ledger must be outside the repository checkout")
    return args


def main(argv=None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    policy = Policy(dry_run_only=args.dry_run_only, max_live=args.max_live,
                    per_run=args.max_live_per_run, ledger=args.ledger,
                    live_actions=tuple(a for a in args.live_actions.split(",") if a),
                    max_text_len=args.max_text_len)
    return Proxy(args.engine, policy, args.log).run()


if __name__ == "__main__":
    sys.exit(main())
