#!/usr/bin/env python3
"""AG-02 MCP budget proxy: a stdio passthrough between an agent and ``ommatix serve``.

The agent's MCP client starts this script as its MCP server; the script starts the engine
(the command after ``--``) and forwards every JSON-RPC line unchanged in both directions,
except for ``tools/call`` of ``execute_action``, which it checks first (fail closed):

* ``--dry-run-only`` (phase 1): a call whose ``dry_run`` is not ``true`` (or absent, the
  contract's default) is answered with a JSON-RPC error and never forwarded.
* ``--max-live N --ledger FILE --goal-id ID --max-live-goal K`` (phase 2): each call with
  ``dry_run: false`` is reserved in the ledger *before* it is forwarded, under an
  inter-process lock around the whole read/check/write, and the reservation is committed
  (flushed) to disk first. Once the session total reaches N, or the goal's own count reaches
  K, the call is answered with a JSON-RPC error and never forwarded. The goal's count lives
  in the ledger, under the goal id, so restarting the proxy for the same goal does not reset
  it. The ledger only accumulates; an unreadable, malformed or locked-out ledger refuses
  every live call. A crash between the reservation and the forward conservatively spends one
  request that was never delivered.
* Live calls are also limited to the actions in ``--live-actions`` (default ``click,type``)
  and a ``type`` text must be 1..``--max-text-len`` printable ASCII characters.
* A request whose JSON-RPC id is still outstanding is refused before any reservation, so a
  reply can never be attributed to the wrong call.

Anything the proxy cannot read the way the engine would (duplicate keys, ``NaN``, a JSON
batch, a non-boolean ``dry_run``, a ``tools/call`` notification) is refused, not forwarded.
``--log`` and ``--ledger`` must be absolute paths outside any repository checkout: a relative
path would mean different files to the agent's client, this script and the engine.

Privacy (AGENTS.md rule 7, ADR-0012): the JSONL log holds, per message, only closed
vocabularies and numbers: the time, direction, kind, a method and tool from a fixed list
(``other``/``unknown`` otherwise), byte size, the result status, error code and effect from the
contract's enums, ``input_delivered``, the element id only when the engine itself issued it in
an observation (``unknown`` otherwise), the revision, the length (never the text) of a ``type``
and the element count of an observation. For ``observe_screen`` it also logs whether the request
filtered or paged (``query_present``, ``limit``, ``cursor_present``: never the query or the cursor),
what the response says about its own completeness (``truncated``, ``total_elements``) and the
numbers of ``_meta`` ``io.ommatix/timings`` and ``io.ommatix/plan`` under their closed field names
(PE-13, PE-14 of the paper v0.4 review: a short element list is not proof of a poor detection, and a
round trip is not the engine's time). Every request with an id gets a ``request_index`` (shared by its
response) and every observation an ``observe_index``. Never a label, a value, an error message, a
typed text or any string the agent chose. The proxy never touches the operator's stop file.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

REFUSAL_CODE = -32001  # implementation-defined server error range of JSON-RPC
INVALID_REQUEST = -32600
LIVE_ACTIONS_DEFAULT = ("click", "type")
MAX_TEXT_LEN_DEFAULT = 16
PRINTABLE_ASCII = frozenset(chr(c) for c in range(0x20, 0x7F))
GOAL_ID = re.compile(r"^[A-Za-z0-9_-]{1,32}$")
LOCK_WAIT_S = 10.0
KNOWN_IDS_LIMIT = 5000

# Closed vocabularies: the only strings of the agent's or the engine's that the log may carry.
METHODS = frozenset({"initialize", "notifications/initialized", "notifications/cancelled", "ping",
                     "tools/list", "tools/call", "server/discover", "resources/list",
                     "prompts/list"})
TOOLS = frozenset({"observe_screen", "execute_action", "get_engine_metrics"})
ACTIONS = frozenset({"click", "type", "scroll"})
STATUSES = frozenset({"simulated", "rejected", "input_delivered", "effect_observed", "indeterminate"})
EFFECTS = frozenset({"none", "focus_changed", "value_updated", "scroll_moved", "visual_change",
                     "unknown"})
# `_meta` of an observation (docs/thesis/07 section 7.3): field names and the plan's names are closed.
TIMING_FIELDS = ("capture_us", "plan_us", "detect_us", "recognize_us", "accessibility_us", "fuse_us",
                 "track_us", "commit_us", "total_us", "response_build_us")
PLAN_COUNTS = ("region_count", "region_area_px", "inference_count", "controls_n", "words_n",
               "supports_n", "relations_n")
PLAN_WORK = frozenset({"full", "regions", "idle"})
PLAN_REASONS = frozenset({"no_committed_base", "geometry_changed", "closure_budget", "too_many_regions",
                          "area_budget", "experimental_baseline", "adaptive_cost"})
NUMBER_LIMIT = 10**15  # whole microseconds, pixels and counts: anything larger is not a measurement
ERROR_CODES = frozenset({
    "STALE_SNAPSHOT", "STALE_TARGET", "TARGET_NOT_FOUND", "AMBIGUOUS_TARGET", "NOT_ACTIONABLE",
    "POLICY_DENIED", "PERMISSION_DENIED", "CAPTURE_UNAVAILABLE", "INFERENCE_FAILED",
    "UNSUPPORTED_GEOMETRY", "INDETERMINATE_EFFECT", "REQUEST_CONFLICT", "CANCELLED",
    "LIMIT_EXCEEDED", "SESSION_MISMATCH", "EPOCH_MISMATCH"})


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


# ---- paths ---------------------------------------------------------------------------------

def inside_git_checkout(path: Path) -> bool:
    """Whether `path` (resolved: symlinks and junctions followed) lies in a directory tree with
    a ``.git`` entry (a repository checkout)."""
    here = Path(os.path.realpath(path))
    return any((parent / ".git").exists() for parent in [here, *here.parents])


def path_problem(value) -> str | None:
    """Why `value` is not an acceptable operator path, or None: it must be an absolute path in
    plain form (no drive-relative form, no extended-length or device prefix, no alternate data
    stream) and, once resolved, outside any repository checkout."""
    text = str(value)
    path = Path(text)
    if not path.is_absolute():
        return "must be an absolute path"
    if text.startswith(("\\\\?\\", "\\\\.\\", "//?/", "//./")):
        return "extended-length and device paths are not accepted"
    if ":" in text[2:] or ".." in path.parts:
        return "ambiguous path form"
    if inside_git_checkout(path):
        return "must be outside the repository checkout"
    return None


# ---- the ledger ----------------------------------------------------------------------------

class InterprocessLock:
    """An exclusive lock on ``<ledger>.lock`` held across the whole read/check/write."""

    def __init__(self, path: Path, wait_s: float = LOCK_WAIT_S):
        self.path, self.wait_s, self.handle = path, wait_s, None

    def __enter__(self):
        self.handle = open(self.path, "a+b")
        deadline = time.monotonic() + self.wait_s
        while True:
            try:
                self._lock()
                return self
            except OSError:
                if time.monotonic() >= deadline:
                    self.handle.close()
                    raise Refusal("LEDGER_LOCKED") from None
                time.sleep(0.02)

    def _lock(self):
        if os.name == "nt":
            import msvcrt
            self.handle.seek(0)
            msvcrt.locking(self.handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    def __exit__(self, *exc):
        try:
            if os.name == "nt":
                import msvcrt
                self.handle.seek(0)
                msvcrt.locking(self.handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.handle.fileno(), fcntl.LOCK_UN)
        finally:
            self.handle.close()


def read_ledger(path: Path) -> dict:
    """The ledger ``{"total": n, "goals": {id: n}}``; empty if the file does not exist. Raises
    ``ValueError`` if it is not exactly that shape."""
    try:
        text = path.read_text(encoding="ascii")
    except FileNotFoundError:
        return {"total": 0, "goals": {}}
    except UnicodeDecodeError:
        raise ValueError("corrupt ledger") from None
    data = json.loads(text)
    if (not isinstance(data, dict) or set(data) != {"v", "total", "goals"} or data["v"] != 1
            or not isinstance(data["total"], int) or isinstance(data["total"], bool)
            or data["total"] < 0 or not isinstance(data["goals"], dict)):
        raise ValueError("corrupt ledger")
    for key, value in data["goals"].items():
        if (not GOAL_ID.match(key) or not isinstance(value, int) or isinstance(value, bool)
                or value < 0):
            raise ValueError("corrupt ledger")
    return {"total": data["total"], "goals": dict(data["goals"])}


def write_ledger(path: Path, ledger: dict) -> None:
    """Writes through a uniquely named temporary file in the same directory, flushed to disk
    before the atomic replacement, so a crash leaves the old or the new ledger, never a mix."""
    body = json.dumps({"v": 1, "total": ledger["total"], "goals": ledger["goals"]},
                      sort_keys=True).encode("ascii")
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".", suffix=".tmp")
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(body)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def reserve(ledger_path: Path, goal_id: str, max_live: int, max_goal: int | None) -> None:
    """The whole transaction: lock, read, check both bounds, write, release. Raises ``Refusal``."""
    with InterprocessLock(ledger_path.with_name(ledger_path.name + ".lock")):
        try:
            ledger = read_ledger(ledger_path)
        except (OSError, ValueError):
            raise Refusal("LEDGER_UNREADABLE") from None
        spent_goal = ledger["goals"].get(goal_id, 0)
        if ledger["total"] >= max_live or (max_goal is not None and spent_goal >= max_goal):
            raise Refusal("LIVE_BUDGET_EXHAUSTED")
        ledger["total"] += 1
        ledger["goals"][goal_id] = spent_goal + 1
        try:
            write_ledger(ledger_path, ledger)
        except OSError:
            raise Refusal("LEDGER_UNWRITABLE") from None


class Policy:
    """What a ``tools/call`` of ``execute_action`` may do; the only state is the ledger file."""

    def __init__(self, *, dry_run_only: bool, max_live: int | None, goal_id: str | None,
                 max_goal: int | None, ledger: Path | None, live_actions=LIVE_ACTIONS_DEFAULT,
                 max_text_len: int = MAX_TEXT_LEN_DEFAULT):
        self.dry_run_only = dry_run_only
        self.max_live = max_live
        self.goal_id = goal_id
        self.max_goal = max_goal
        self.ledger = ledger
        self.live_actions = tuple(live_actions)
        self.max_text_len = max_text_len

    def check(self, arguments) -> bool:
        """Decide one ``execute_action``. Returns whether it is a live call that was reserved;
        raises ``Refusal`` when it must not be forwarded."""
        if not isinstance(arguments, dict):
            raise Refusal("ARGUMENTS_NOT_AN_OBJECT")
        if "dry_run" not in arguments or arguments["dry_run"] is True:
            return False
        if arguments["dry_run"] is not False:
            raise Refusal("DRY_RUN_NOT_BOOLEAN")
        if self.dry_run_only:
            raise Refusal("DRY_RUN_ONLY")
        if self.max_live is None or self.ledger is None or self.goal_id is None:
            raise Refusal("NO_LIVE_BUDGET")
        action = arguments.get("action")
        if action not in self.live_actions:
            raise Refusal("ACTION_NOT_ALLOWED_LIVE")
        if action == "type":
            text = arguments.get("text")
            if (not isinstance(text, str) or not 1 <= len(text) <= self.max_text_len
                    or not set(text) <= PRINTABLE_ASCII):
                raise Refusal("TEXT_NOT_ALLOWED_LIVE")
        reserve(self.ledger, self.goal_id, self.max_live, self.max_goal)
        return True


# ---- messages ------------------------------------------------------------------------------

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


def id_key(request_id):
    """A hashable key that keeps ``1`` and ``"1"`` apart; None if `request_id` is not an id."""
    if isinstance(request_id, bool) or not isinstance(request_id, (int, str)):
        return None
    return (type(request_id).__name__, request_id)


def _bounded_int(value, low=1, high=10**9):
    return value if isinstance(value, int) and not isinstance(value, bool) and low <= value <= high else None


def _count(value):
    """A whole number the engine could have measured (``0 <= n <= NUMBER_LIMIT``), else None."""
    return value if isinstance(value, int) and not isinstance(value, bool) \
        and 0 <= value <= NUMBER_LIMIT else None


def observation_request_meta(args) -> dict:
    """Whether an ``observe_screen`` request filtered or paged, never what it asked for."""
    args = args if isinstance(args, dict) else {}
    meta = {"query_present": isinstance(args.get("query"), str) and bool(args["query"]),
            "cursor_present": isinstance(args.get("cursor"), str) and bool(args["cursor"])}
    limit = _bounded_int(args.get("limit"), 1, 200)
    if limit is not None:
        meta["limit"] = limit
    return meta


def observation_response_meta(result: dict, structured: dict) -> dict:
    """What an ``observe_screen`` result says about its own completeness, and the numbers of its
    ``_meta`` (stage timings, plan), each under a closed field name."""
    meta: dict = {}
    total = _count(structured.get("total_elements"))
    if total is not None:
        meta["total_elements"] = total
    if isinstance(structured.get("truncated"), bool):
        meta["truncated"] = structured["truncated"]
    extra = result.get("_meta")
    if not isinstance(extra, dict):
        return meta
    timings = extra.get("io.ommatix/timings")
    if isinstance(timings, dict):
        meta["timings_us"] = {name: _count(timings.get(name)) for name in TIMING_FIELDS}
    plan = extra.get("io.ommatix/plan")
    if isinstance(plan, dict):
        work, reason = plan.get("work"), plan.get("full_refresh_reason")
        shapes = plan.get("input_shapes")
        truncated = plan.get("accessibility_truncated")
        meta["plan"] = {
            "work": work if isinstance(work, str) and work in PLAN_WORK else "other",
            "full_refresh_reason": None if reason is None else (
                reason if isinstance(reason, str) and reason in PLAN_REASONS else "other"),
            **{name: _count(plan.get(name)) for name in PLAN_COUNTS},
            "input_shapes_n": min(len(shapes), 1000) if isinstance(shapes, list) else None,
            "accessibility_truncated": truncated if isinstance(truncated, bool) else None}
    return meta


def request_meta(message, known_ids=frozenset()) -> dict:
    """The loggable facts of an agent message: closed vocabularies and numbers only."""
    meta: dict = {"kind": "other"}
    if not isinstance(message, dict):
        return meta
    method = message.get("method")
    meta["kind"] = "request" if "id" in message and method else "notification" if method else "other"
    if isinstance(method, str):
        meta["method"] = method if method in METHODS else "other"
    call = _tool_call_of(message)
    if call is not None:
        name = call[0]
        meta["tool"] = name if isinstance(name, str) and name in TOOLS else "unknown"
        args = call[1] if isinstance(call[1], dict) else {}
        if name == "observe_screen":
            meta.update(observation_request_meta(call[1]))
        if name == "execute_action":
            meta["action"] = args.get("action") if args.get("action") in ACTIONS else "invalid"
            dry_run = args.get("dry_run", True)
            meta["dry_run"] = dry_run if isinstance(dry_run, bool) else "invalid"
            target = args.get("target_id")
            meta["element_id"] = target if isinstance(target, str) and target in known_ids \
                else "unknown"
            revision = _bounded_int(args.get("expected_revision"))
            if revision is not None:
                meta["revision"] = revision
            if isinstance(args.get("text"), str):
                meta["text_len"] = min(len(args["text"]), 100_000)
    return meta


def response_meta(message, tool: str | None) -> dict:
    meta: dict = {"kind": "response"}
    if not isinstance(message, dict):
        return meta
    if isinstance(message.get("error"), dict):
        code = message["error"].get("code")
        meta["rpc_error_code"] = code if isinstance(code, int) and not isinstance(code, bool) else None
        return meta
    result = message.get("result")
    if not isinstance(result, dict):
        return meta
    meta["is_error"] = bool(result.get("isError"))
    structured = result.get("structuredContent")
    if isinstance(structured, dict):
        status = structured.get("status")
        if isinstance(status, str) and status in STATUSES:
            meta["status"] = status
        error = structured.get("error")
        if isinstance(error, dict) and isinstance(error.get("code"), str) \
                and error["code"] in ERROR_CODES:
            meta["error_code"] = error["code"]
        for flag in ("input_delivered", "verified"):
            if isinstance(structured.get(flag), bool):
                meta[flag] = structured[flag]
        effect = structured.get("observed_effect")
        if isinstance(effect, str) and effect in EFFECTS:
            meta["observed_effect"] = effect
        if tool == "observe_screen":
            if isinstance(structured.get("elements"), list):
                meta["elements"] = len(structured["elements"])
            meta.update(observation_response_meta(result, structured))
    return meta


def element_ids_of(message) -> set:
    """The ids the engine itself issued in an observation result."""
    try:
        elements = message["result"]["structuredContent"]["elements"]
    except (KeyError, TypeError):
        return set()
    return {e["target_id"] for e in elements if isinstance(e, dict)
            and isinstance(e.get("target_id"), str) and 0 < len(e["target_id"]) <= 64}


class Proxy:
    def __init__(self, engine_argv, policy: Policy, log_path: Path, *, stdin=None, stdout=None):
        self.engine_argv = list(engine_argv)
        self.policy = policy
        self.log_path = log_path
        self.stdin = stdin if stdin is not None else sys.stdin.buffer
        self.stdout = stdout if stdout is not None else sys.stdout.buffer
        self.out_lock = threading.Lock()
        self.log_lock = threading.Lock()
        self.state_lock = threading.Lock()  # pending ids and the engine-issued ids
        # id_key -> (tool or None, monotonic time sent, request_index, observe_index or None)
        self.pending: dict = {}
        self.requests_seen = 0  # requests with an id that were forwarded: the session's request index
        self.observes_seen = 0
        self.known_ids: set = set()
        self.live_spent_here = 0
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
        with self.state_lock:
            known = frozenset(self.known_ids)
        meta = request_meta(message, known)
        call = _tool_call_of(message)
        request_id = message.get("id") if isinstance(message, dict) else None
        key = id_key(request_id)
        is_request = isinstance(message, dict) and "method" in message and key is not None
        if call is not None and key is None:
            self.log("agent->proxy", size, **meta)
            self.refuse(None, "TOOL_CALL_WITHOUT_ID", INVALID_REQUEST)
            return
        if is_request:
            with self.state_lock:
                outstanding = key in self.pending
            if outstanding:
                self.log("agent->proxy", size, **meta)
                self.refuse(request_id, "DUPLICATE_ID", INVALID_REQUEST)
                return
        live = False
        if call is not None and call[0] == "execute_action":
            try:
                live = self.policy.check(call[1])
            except Refusal as refusal:
                self.log("agent->proxy", size, **meta)
                self.refuse(request_id, refusal.reason, tool="execute_action")
                return
        indices: dict = {}
        if is_request:
            with self.state_lock:
                self.requests_seen += 1
                indices["request_index"] = self.requests_seen
                if meta.get("tool") == "observe_screen":
                    self.observes_seen += 1
                    indices["observe_index"] = self.observes_seen
        self.log("agent->engine", size, live_counted=live, **meta, **indices)
        if live:
            self.live_spent_here += 1
        if is_request:
            with self.state_lock:
                self.pending[key] = (meta.get("tool"), time.monotonic(), indices["request_index"],
                                     indices.get("observe_index"))
        engine_in.write(raw if raw.endswith(b"\n") else raw + b"\n")
        engine_in.flush()

    def pump_engine(self, engine_out) -> None:
        for raw in engine_out:
            size = len(raw)
            try:
                message = parse_line(raw)
            except ValueError:
                message = None
            tool, started, request_index, observe_index = None, None, None, None
            if isinstance(message, dict) and "id" in message and "method" not in message:
                key = id_key(message["id"])
                if key is not None:
                    with self.state_lock:
                        tool, started, request_index, observe_index = self.pending.pop(
                            key, (None, None, None, None))
            if tool == "observe_screen":
                issued = element_ids_of(message)
                with self.state_lock:
                    if len(self.known_ids) + len(issued) <= KNOWN_IDS_LIMIT:
                        self.known_ids |= issued
            meta = response_meta(message, tool)
            if tool:
                meta["tool"] = tool
            if request_index is not None:
                meta["request_index"] = request_index
            if observe_index is not None:
                meta["observe_index"] = observe_index
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
            self.log("proxy", 0, kind="end", engine_exit=code,
                     live_spent_here=self.live_spent_here)
            self.log_file.close()
        return code


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run-only", action="store_true",
                      help="phase 1: refuse any call whose dry_run is not true")
    mode.add_argument("--max-live", type=int, metavar="N",
                      help="phase 2: the ledger's session total may not exceed N live calls")
    parser.add_argument("--goal-id", help="the goal's id in the ledger (required with --max-live)")
    parser.add_argument("--max-live-goal", type=int, metavar="K",
                        help="also bound this goal's live calls across restarts, in the ledger")
    parser.add_argument("--ledger", type=Path, help="ledger file (required with --max-live)")
    parser.add_argument("--log", type=Path, required=True,
                        help="metadata JSONL, an absolute path outside the repository checkout")
    parser.add_argument("--live-actions", default=",".join(LIVE_ACTIONS_DEFAULT))
    parser.add_argument("--max-text-len", type=int, default=MAX_TEXT_LEN_DEFAULT)
    parser.add_argument("engine", nargs=argparse.REMAINDER, help="-- ENGINE_COMMAND ...")
    args = parser.parse_args(argv)
    engine = args.engine[1:] if args.engine[:1] == ["--"] else args.engine
    if not engine:
        parser.error("the engine command is missing (after --)")
    args.engine = engine
    if args.max_live is not None:
        if args.max_live < 0 or args.ledger is None or not args.goal_id \
                or not GOAL_ID.match(args.goal_id):
            parser.error("--max-live needs N >= 0, --ledger and a --goal-id of 1-32 plain characters")
        if args.max_live_goal is not None and args.max_live_goal < 0:
            parser.error("--max-live-goal must be >= 0")
    for name in ("log", "ledger"):
        value = getattr(args, name)
        if value is not None and (problem := path_problem(value)):
            parser.error(f"--{name}: {problem}")
    return args


def main(argv=None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    policy = Policy(dry_run_only=args.dry_run_only, max_live=args.max_live, goal_id=args.goal_id,
                    max_goal=args.max_live_goal, ledger=args.ledger,
                    live_actions=tuple(a for a in args.live_actions.split(",") if a),
                    max_text_len=args.max_text_len)
    return Proxy(args.engine, policy, args.log).run()


if __name__ == "__main__":
    sys.exit(main())
