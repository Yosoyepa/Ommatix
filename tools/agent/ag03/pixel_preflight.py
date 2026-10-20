#!/usr/bin/env python3
"""AG-03 isolation preflight (ADR-0038 decision 6): the proof for the pixel agent, native and per route.

The same discipline as ``tools/agent/ag02/preflight.py`` (ADR-0036 decision 9) with its own schema
(``ag03-isolation-proof-1``) and its own ``verifier_revision``, so AG-02's proofs stay valid and neither
accepts the other's. Two sessions of the exact production command (``pixel_agent.build_claude_argv``)
run against a local stand-in for the Anthropic endpoint (``fake_messages.ScriptedMessages``, 127.0.0.1)
with the pixel server in **synthetic mode** - drawn PNGs of a fake window; **nothing is captured, no
screen is read, nothing is sent** - and a scripted model that calls ``screenshot``, ``click`` and
``type_text``:

* **A. the canary** - the synthetic world of sentinels of AG-02's preflight (user, ancestor, project and
  local CLAUDE.md, rules, memory, skill, command, agent, hooks, extra MCP servers). It passes only when
  no sentinel appears in any request, no hook or extra server ran, ``init`` lists exactly the three pixel
  tools and the one connected server, and the positive control (settings sources on) leaks.
* **B. the production configuration directory** (``--claude-config-dir``): none of its context lines may
  appear in the request.

On top of that, a **request-level verdict** (``pixel_agent.check_pixel_requests``, native and route):
every request that offers tools offers exactly ``screenshot``, ``click`` and ``type_text``; **image blocks
appear only inside the tool result of ``screenshot``, as PNG, within the byte and dimension limits** (an
image from any other source - another tool's result, the prompt - fails; so does an image sent as text);
no sentinel is in any request. AG-02's route verdict (``image block => fail``) is not reused. A positive
control feeds the verdict mutated copies of a real request (an extra tool, a dropped tool, an image
outside ``screenshot``, an image in another tool's result, an oversized image, a too-wide image, a non-PNG
image, an image as text, a sentinel; on a route also the AG-02 route mutations) and the preflight fails if
any goes unseen. ``--route mimo|deepseek`` runs the same sessions through the route's child environment
(``agentcli.route_env``, the function production uses) and adds the route's header, model and effort
checks.

No real provider is reached, by the controls of AG-02's preflight (a scrubbed child environment with a
dummy key for the loopback stand-in; refusal to run when routing variables are set or a managed
configuration is found) and not by assumption. The route verdict establishes what the harness *sends*;
whether the provider accepts it is the gate smoke (``pixel_gate.py``, Step 3b, needs the owner).
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import agentcli  # noqa: E402
import fake_messages  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_disk as disk  # noqa: E402
import preflight as ag02  # noqa: E402
import proxy as proxy_module  # noqa: E402
import run_goal as rg  # noqa: E402
from preflight_cli import decoded_strings  # noqa: E402

PROOF_SCHEMA = pa.PROOF_SCHEMA
VERIFIER_REVISION = pa.VERIFIER_REVISION
# The route verdict's own revision (separate from AG-02's ``agentcli.ROUTE_VERIFIER_REVISION``).
ROUTE_VERIFIER_REVISION = 1
PixelRefused = ag02.PreflightRefused
SERVER_PATH = HERE / "pixel_server.py"


def file_sha256(path: Path) -> str:
    """SHA-256 with line endings normalised (a Windows checkout may hold CRLF where the repository holds LF)."""
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


def script(request: dict, n: int) -> dict:
    """The scripted model: a screenshot, a click, a typed word, a second screenshot, then the verdict."""
    steps = [{"tool": f"mcp__{pa.SERVER_NAME}__screenshot", "input": {}},
             {"tool": f"mcp__{pa.SERVER_NAME}__click", "input": {"x": 10, "y": 10}},
             {"tool": f"mcp__{pa.SERVER_NAME}__type_text", "input": {"text": "hola"}},
             {"tool": f"mcp__{pa.SERVER_NAME}__screenshot", "input": {}}]
    return steps[n] if n < len(steps) else {"text": "VERDICT: DONE"}


def one_session(args, config_dir: Path, cwd: Path, *, control: bool, log: Path, scratch_name: str,
                memory_dir: Path | None) -> dict:
    """One session. Production flags, or (control) the settings sources on and the CLAUDE.md switch off."""
    mcp = pa.build_mcp_config(sys.executable, ["--synthetic", "--synthetic-size", "640x360", "--log", str(log)])
    mcp_path = log.with_name(scratch_name + ".mcp.json")
    mcp_path.write_text(json.dumps(mcp), encoding="utf-8")
    route = getattr(args, "route", None)
    argv = pa.build_claude_argv(claude=args.claude, model=args.model, mcp_config=mcp_path,
                                system_instruction=pa.SYSTEM_INSTRUCTION, max_budget_usd=None if route else 0.5,
                                effort=None if route else "max")
    with fake_messages.ScriptedMessages(script) as api:
        if route:
            env = agentcli.route_env(route, api.url, ag02.ROUTE_DUMMY_TOKEN, args.model, config_dir) | pa.PIXEL_ENV | \
                getattr(args, "extra_env", {})
            env_problems = agentcli.route_env_problems(env, route, agentcli.CLAUDE_ROUTES[route]["base_urls"],
                                                       pa.PIXEL_ENV)
        else:
            env_problems = []
            env = rg.scrubbed_env({"CLAUDE_CONFIG_DIR": str(config_dir), "ANTHROPIC_BASE_URL": api.url,
                                   "ANTHROPIC_API_KEY": "sk-ant-canary-0000000000", **pa.PIXEL_ENV,
                                   **getattr(args, "extra_env", {})})
        if control:
            argv[argv.index("--setting-sources") + 1] = "user,project,local"
            env.pop("CLAUDE_CODE_DISABLE_CLAUDE_MDS", None)
        metrics = pa.PixelStream([], None, None, config_dir=memory_dir)
        # What the session leaves on disk (O5): the configuration directory and the CLI's cache, before and after.
        cache_root = disk.cli_cache_root()
        before_config, before_cache = disk.snapshot(config_dir), disk.snapshot(cache_root)
        cli = rg.run_claude(argv, pa.build_prompt({"prompt": "Say hi."}, 1), cwd, metrics, args.timeout, env=env)
        config_tokens, config_facts = disk.config_problems(config_dir, before_config, disk.snapshot(config_dir))
        cache_tokens, cache_facts = disk.cache_problems(cache_root, before_cache, disk.snapshot(cache_root))
        requests = list(api.requests)
        bodies = [r["body"] for r in requests]
    # The command this session really ran (the control widens only ``--setting-sources``) and its environment.
    command_tokens = pa.command_problems(argv) + pa.env_problems(env)
    return {"cli": cli, "metrics": metrics, "bodies": bodies, "requests": requests, "env_problems": env_problems,
            "attestation": metrics.attestation, "tool_calls": metrics.summary()["tool_calls"],
            "disk_problems": sorted(set(config_tokens + cache_tokens + command_tokens)),
            "disk_facts": {"config_touched": config_facts["touched"], "cache_touched": cache_facts["touched"],
                           "image_data": any(t.endswith("image_data") for t in config_tokens + cache_tokens)}}


# ---- the route's request verdict (AG-02's headers/model/effort, without its image rule) ---------------------

def check_route_headers(requests: list[dict], *, model: str, token: str, effort: str) -> tuple[list[str], dict]:
    """Header, model, credential and effort checks of ``preflight.check_route_requests`` - exactly the
    reviewed header names, the credential only in the two reviewed headers, only the route's model and
    effort - without its tools and image rules, which ``pixel_agent.check_pixel_requests`` replaces."""
    problems: list[str] = []
    models, efforts, names = set(), set(), set()
    for request in requests:
        headers = request["headers"]
        names |= set(headers)
        if set(headers) - ag02.ROUTE_ALLOWED_HEADERS:
            problems.append("header_not_expected")
        if request["path"] != "/v1/messages":
            problems.append("request_path_differs")
        if headers.get("authorization") != f"Bearer {token}":
            problems.append("auth_header_differs")
        if headers.get("x-api-key") not in (None, token):
            problems.append("x_api_key_header_differs")
        if any(token in value for name, value in headers.items() if name not in ("authorization", "x-api-key")):
            problems.append("key_in_another_header")
        try:
            body = json.loads(request["body"])
        except ValueError:
            continue
        if not isinstance(body, dict):
            continue
        if token in "\n".join(decoded_strings(body)):
            problems.append("key_in_body")
        models.add(str(body.get("model")))
        efforts.add(str((body.get("output_config") or {}).get("effort")))
    if models != {model}:
        problems.append("request_model_differs")
    if efforts != {effort}:
        problems.append("effort_differs")
    return sorted(set(problems)), {"request_header_names": sorted(names), "request_models": sorted(models),
                                   "request_effort": sorted(efforts)}


# ---- positive controls: broken copies of a real request the verdict must see ------------------------------

def _clone(body):
    return json.loads(json.dumps(body))


def _first_screenshot_result(body: dict):
    for message in body.get("messages", []):
        content = message.get("content") if isinstance(message, dict) else None
        for block in content if isinstance(content, list) else []:
            if isinstance(block, dict) and block.get("type") == "tool_result":
                for part in block.get("content") or []:
                    if isinstance(part, dict) and part.get("type") == "image":
                        return block, part
    return None, None


def pixel_control_mutations(request: dict, sentinel: str) -> dict[str, dict]:
    """Copies of one real request that carries a screenshot result, each broken in one way the verdict
    must see. Returns ``name -> request`` (the body re-serialised)."""
    body = json.loads(request["body"])

    def variant(change):
        mutated = _clone(body)
        change(mutated)
        return request | {"body": json.dumps(mutated).encode()}

    def b64(data: bytes) -> str:
        return base64.b64encode(data).decode("ascii")

    def image_block(data: bytes, media_type="image/png"):
        return {"type": "image", "source": {"type": "base64", "media_type": media_type, "data": b64(data)}}

    def rename_screenshot_tool(b):
        block, _ = _first_screenshot_result(b)
        for message in b["messages"]:
            for item in message.get("content") if isinstance(message.get("content"), list) else []:
                if item.get("type") == "tool_use" and item.get("id") == block["tool_use_id"]:
                    item["name"] = f"mcp__{pa.SERVER_NAME}__click"

    def swap_image(b, new_part):
        _, part = _first_screenshot_result(b)
        part.clear()
        part.update(new_part)

    small = cap.synthetic_png(0, 64, 48)[0]
    return {
        "extra_tool": variant(lambda b: b["tools"].append({"name": "Bash", "description": "", "input_schema": {}})),
        "dropped_tool": variant(lambda b: b["tools"].pop()),
        "image_outside_screenshot": variant(lambda b: b["messages"].append(
            {"role": "user", "content": [image_block(small)]})),
        "image_in_another_tools_result": variant(rename_screenshot_tool),
        "oversized_image": variant(lambda b: swap_image(b, image_block(
            small + bytes(cap.MAX_IMAGE_BYTES + 1)))),
        "too_wide_image": variant(lambda b: swap_image(b, image_block(
            cap.png_encode(cap.MAX_IMAGE_EDGE + 100, 1, bytes((cap.MAX_IMAGE_EDGE + 100) * 3))))),
        "not_a_png": variant(lambda b: swap_image(b, image_block(b"\xff\xd8\xff\xe0" + bytes(40), "image/jpeg"))),
        "image_as_text": variant(lambda b: b["messages"].append({"role": "user", "content": b64(small)})),
        "sentinel": variant(lambda b: b["messages"].append({"role": "user", "content": sentinel})),
    }


def screenshot_request(requests: list[dict]) -> dict | None:
    """The request with the most screenshot results in it (the best subject for the controls)."""
    best, best_count = None, 0
    for request in requests:
        try:
            body = json.loads(request["body"])
        except ValueError:
            continue
        count = sum(1 for i in pa.image_blocks(body) if i["where"] == "tool_result:screenshot") \
            if isinstance(body, dict) and "tools" in body else 0
        if count > best_count:
            best, best_count = request, count
    return best


def run(args) -> dict:
    """The whole preflight; returns the proof (``passed`` says whether it holds). Raises ``PixelRefused``
    when the environment cannot honestly be called local."""
    if (reasons := ag02.refusals()):
        raise PixelRefused(reasons)
    production_dir = Path(args.claude_config_dir)
    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ag03-canary-") as tmp:
        root = Path(tmp)
        world = ag02.build_world(root)
        a = one_session(args, world["home"], world["cwd"], control=False, log=root / "a.jsonl", scratch_name="a",
                        memory_dir=None)
        markers_production = sorted(k for k, m in world["markers"].items() if m.exists())
        for marker in world["markers"].values():
            marker.unlink(missing_ok=True)
        control = one_session(args, world["home"], world["cwd"], control=True, log=root / "c.jsonl",
                              scratch_name="c", memory_dir=None)
        markers_seen = sorted(k for k, m in world["markers"].items() if m.exists())
        a_leaks = ag02.leaked(a["bodies"], world["sentinels"])
        control_leaks = ag02.leaked(control["bodies"], world["sentinels"])
        scratch_b = root / "b" / "scratch"
        scratch_b.mkdir(parents=True)
        b = one_session(args, production_dir, scratch_b, control=False, log=root / "b.jsonl", scratch_name="b",
                        memory_dir=production_dir)
        b_snippets, b_untestable = ag02.context_snippets(production_dir)
        b_text, b_unreadable = ag02.request_texts(b["bodies"])
        b_leaked_lines = sum(1 for snippet in b_snippets if snippet in b_text)
        a_unreadable = ag02.request_texts(a["bodies"])[1]
        ancestors = rg.ancestor_context(scratch_b, production_dir)
    route = getattr(args, "route", None)
    production_requests = a["requests"] + b["requests"]
    sentinel_values = list(world["sentinels"].values())
    found, facts = pa.check_pixel_requests(production_requests, sentinels=sentinel_values,
                                           effort=None if route else "max")
    problems += [f"pixel_{p}" for p in found]
    if not facts.get("screenshot_result_images"):
        problems.append("pixel_no_screenshot_result_in_requests")
    route_facts, route_detected, route_blind = {}, [], []
    if route:
        spec = agentcli.CLAUDE_ROUTES[route]
        found, route_facts = check_route_headers(production_requests, model=args.model,
                                                 token=ag02.ROUTE_DUMMY_TOKEN, effort=spec["effort"])
        problems += [f"route_{p}" for p in found]
        for session in (a, control, b):
            problems += [f"route_env_{p}" for p in session["env_problems"]]
    # the positive controls: broken copies of a real request, which the verdict must see
    detected, blind = [], []
    subject = screenshot_request(production_requests)
    pixel_sentinel = "CANARY-PIXEL-" + hashlib.sha256(repr(time.time()).encode()).hexdigest()[:12]
    if subject is not None:
        for name, mutated in pixel_control_mutations(subject, pixel_sentinel).items():
            seen, _ = pa.check_pixel_requests([mutated], sentinels=[pixel_sentinel])
            (detected if seen else blind).append(name)
        if route:
            for name, mutated in ag02.route_control_mutations(subject, ag02.ROUTE_DUMMY_TOKEN).items():
                seen = check_route_headers([mutated], model=args.model, token=ag02.ROUTE_DUMMY_TOKEN,
                                           effort=agentcli.CLAUDE_ROUTES[route]["effort"])[0]
                seen += pa.check_pixel_requests([mutated])[0]
                (route_detected if seen else route_blind).append(name)
    else:
        problems.append("pixel_controls_without_a_real_request")
    if blind:
        problems.append("pixel_control_blind")
    if route_blind:
        problems.append("route_control_blind")
    disk_facts = {"config_files_touched": {}, "cache_files_touched": 0, "image_data_found": False}
    for name, session in (("canary", a), ("control", control), ("production_dir", b)):
        # what a session wrote is named by session; a lost command or environment invariant is one finding
        problems += [f"{name}_{token}" if token.startswith("disk_") else token for token in session["disk_problems"]]
        disk_facts["config_files_touched"][name] = session["disk_facts"]["config_touched"]
        disk_facts["cache_files_touched"] += session["disk_facts"]["cache_touched"]
        disk_facts["image_data_found"] = disk_facts["image_data_found"] or session["disk_facts"]["image_data"]
    att_a, att_b = a["attestation"] or {}, b["attestation"] or {}
    for name, run_ in (("canary_session", a), ("production_dir_session", b)):
        if run_["cli"]["exit_code"] != 0 or run_["cli"]["killed"]:
            problems.append(f"{name}_failed")
        if not run_["bodies"]:
            problems.append(f"{name}_no_request_captured")
        if not run_["tool_calls"].get("screenshot"):
            problems.append(f"{name}_no_screenshot_call")
    if a_leaks:
        problems.append("sentinels_in_production_request")
    if markers_production:
        problems.append("hook_or_extra_server_ran_in_production")
    problems += [f"init_{p}" for p in pa.init_problems(a["attestation"], None)]
    problems += [f"production_dir_init_{p}" for p in pa.init_problems(b["attestation"], None, production_dir)]
    if b_leaked_lines:
        problems.append("production_dir_context_in_request")
    if b_untestable:
        problems.append("production_dir_context_file_untestable")
    if b_unreadable or a_unreadable:
        problems.append("request_unreadable")
    if ancestors:
        problems.append("ancestor_context_present")
    if not control_leaks:
        problems.append("control_blind")
    for key in ("claude_code_version", "model", "tools", "plugin_hashes", "agent_hashes", "init_keys_sha256"):
        if att_a.get(key) != att_b.get(key):
            problems.append(f"canary_and_production_dir_differ_in_{key}")
    try:
        manifest = rg.config_manifest(production_dir)
    except rg.ManifestRefused as refused:
        manifest = {}
        problems.append(f"context_manifest_refused_{refused}")
    proof = {
        "schema": PROOF_SCHEMA, "verifier_revision": VERIFIER_REVISION, "passed": not problems,
        "problems": sorted(set(problems)), "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "claude_code_version": att_b.get("claude_code_version"), "model": att_b.get("model"),
        "requested_model": args.model, "tools": att_b.get("tools") or [], "plugins": att_b.get("plugins") or [],
        "agents": att_b.get("agents") or [], "plugin_hashes": att_b.get("plugin_hashes") or [],
        "agent_hashes": att_b.get("agent_hashes") or [], "init_keys_sha256": att_b.get("init_keys_sha256") or "",
        "config_identity": rg.config_identity(production_dir), "context_manifest": manifest,
        "canary_kinds": list(ag02.KINDS), "production_leaked_kinds": a_leaks, "control_leaked_kinds": control_leaks,
        "bodies": len(a["bodies"]), "body_bytes": sum(len(x) for x in a["bodies"]),
        "production_dir_context_lines_checked": len(b_snippets),
        "production_dir_context_files_untestable": len(b_untestable),
        "production_dir_context_lines_in_request": b_leaked_lines,
        "markers_after_production": markers_production, "markers_after_control": markers_seen,
        "egress_controls": ag02.EGRESS_CONTROLS,
        "differences_from_production": [
            "stage A uses the canary's own configuration directory", "the API endpoint is a local stand-in",
            "the key is a dummy", "the pixel server runs in synthetic mode: drawn PNGs, nothing captured"],
        "pixel": {"server_sha256": file_sha256(SERVER_PATH), "instruction_sha256": pa.instruction_sha256(),
                  "server_mode_in_proof": "synthetic", "request_tools": facts.get("request_tools", []),
                  "screenshot_result_images": facts.get("screenshot_result_images", 0),
                  "max_image_edge_seen": facts.get("max_image_edge", 0),
                  "max_image_bytes_seen": facts.get("max_image_bytes", 0),
                  "limits": {"edge": cap.MAX_IMAGE_EDGE, "bytes": cap.MAX_IMAGE_BYTES, "format": "png"},
                  "images_only_in_screenshot_result": not any(p.startswith("pixel_image") for p in problems),
                  "control_detected": detected, "control_blind": blind},
        "real_capture_in_this_proof": False,
        # What the synthetic sessions left on disk (ADR-0038 O5; ``pixel_disk``). ``debug_log_location`` is where the CLI
        # keeps the MCP server's debug log: outside the configuration directory, not deleted by the harness, and its
        # content for a real screen is NOT measured.
        "disk": disk_facts | {"no_session_persistence_flag": not any(
            "production_command_persists_session" in s["disk_problems"] for s in (a, control, b)),
            "debug_log_location": "<cache>/claude-cli-nodejs/Cache/<cwd>/mcp-logs-pixel/ (outside the configuration directory)",
            "debug_log_deleted_by_harness": False},
        "agent_env": dict(pa.PIXEL_ENV),
    }
    if route:
        proof |= {"route": {"provider": route, "base_url": args.route_base_url, "model": args.model,
                            "effort": agentcli.CLAUDE_ROUTES[route]["effort"], "auth": "bearer"},
                  "route_verifier_revision": ROUTE_VERIFIER_REVISION, "route_control_detected": route_detected,
                  "route_control_blind": route_blind, **route_facts,
                  "route_differences_from_production": [
                      "the base URL is the stand-in (the production URL is bound in `route`)",
                      "the token is a dummy; the model's answers are scripted"]}
    return proof


def load_isolation_proof(path: Path) -> dict:
    """The proof ``run`` wrote, or ``SystemExit``: this schema, a pass, this revision, exactly the three
    pixel tools, the controls that detected and none that were blind, no real capture."""
    try:
        proof = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        raise SystemExit("the isolation proof is unreadable") from None
    revision = proof.get("verifier_revision") if isinstance(proof, dict) else None
    pixel = proof.get("pixel") if isinstance(proof, dict) else None
    ok = (isinstance(proof, dict) and proof.get("schema") == PROOF_SCHEMA and proof.get("passed") is True
          and type(revision) is int and revision == VERIFIER_REVISION
          and isinstance(proof.get("claude_code_version"), str) and rg.VERSION.match(proof["claude_code_version"])
          and isinstance(proof.get("model"), str) and rg.MODEL_NAME.match(proof["model"])
          and isinstance(proof.get("requested_model"), str)
          and sorted(proof.get("tools", [])) == sorted(pa.ALLOWED_TOOL_NAMES)
          and all(isinstance(proof.get(k), list) and all(isinstance(x, str) for x in proof[k])
                  for k in ("plugins", "agents", "plugin_hashes", "agent_hashes", "canary_kinds", "control_leaked_kinds"))
          and proof["canary_kinds"] and proof["control_leaked_kinds"]
          and isinstance(proof.get("init_keys_sha256"), str) and isinstance(proof.get("config_identity"), str)
          and isinstance(proof.get("context_manifest"), dict) and isinstance(proof.get("date_utc"), str)
          and isinstance(pixel, dict) and pixel.get("control_detected") and pixel.get("control_blind") == []
          and pixel.get("images_only_in_screenshot_result") is True and pixel.get("screenshot_result_images", 0) >= 1
          and proof.get("real_capture_in_this_proof") is False
          and isinstance(proof.get("disk"), dict) and proof["disk"].get("no_session_persistence_flag") is True
          and proof["disk"].get("image_data_found") is False and proof.get("agent_env") == pa.PIXEL_ENV)
    if not ok:
        raise SystemExit(f"the isolation proof is not a passing {PROOF_SCHEMA} file with verifier revision "
                         f"{VERIFIER_REVISION}; run pixel_preflight.py again")
    return proof


def proof_problems(proof: dict, args, config_dir: Path | None, route_facts: dict | None) -> list[str]:
    """Whether the proof is for THIS run: model, configuration directory, the pixel server's code, the
    pinned instruction and, on a route, the route facts. Closed tokens."""
    problems = rg.proof_problems(proof, args, config_dir)
    pixel = proof["pixel"]
    if pixel.get("server_sha256") != file_sha256(SERVER_PATH):
        problems.append("pixel_server_differs_from_proof")
    if pixel.get("instruction_sha256") != pa.instruction_sha256():
        problems.append("instruction_differs_from_proof")
    if proof.get("agent_env") != pa.PIXEL_ENV:
        problems.append("agent_env_differs_from_proof")
    if route_facts is None:
        if proof.get("route") is not None:
            problems.append("route_proof_for_a_native_run")
    else:
        if proof.get("route") != route_facts:
            problems.append("route_differs_from_proof")
        if proof.get("route_verifier_revision") != ROUTE_VERIFIER_REVISION:
            problems.append("route_verifier_revision_differs")
    return problems


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("--model", help="the model the goals will be run with (with --route: the route's)")
    parser.add_argument("--claude-config-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--claude", default="claude")
    parser.add_argument("--timeout", type=float, default=180.0)
    parser.add_argument("--route", choices=sorted(agentcli.CLAUDE_ROUTES))
    parser.add_argument("--route-base-url")
    args = parser.parse_args(argv)
    if args.route:
        spec = agentcli.CLAUDE_ROUTES[args.route]
        if args.route_base_url not in spec["base_urls"]:
            parser.error("--route-base-url is required and must be one of: " + ", ".join(spec["base_urls"]))
        args.model = args.model or spec["model"]
    elif args.route_base_url:
        parser.error("--route-base-url needs --route")
    if not args.model:
        parser.error("--model is required (without --route)")
    args.out = rg.absolute_arg(args.out)
    args.claude_config_dir = rg.absolute_arg(args.claude_config_dir)
    for name in ("out", "claude_config_dir"):
        if (problem := proxy_module.path_problem(getattr(args, name))):
            parser.error(f"--{name.replace('_', '-')}: {problem}")
    if not args.claude_config_dir.is_dir():
        parser.error("--claude-config-dir must be an existing directory")
    try:
        proof = run(args)
    except PixelRefused as refused:
        print(json.dumps({"passed": False, "refused": refused.reasons}))
        return 2
    args.out.write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps({k: proof[k] for k in ("passed", "problems", "claude_code_version", "model",
                                           "production_leaked_kinds", "control_leaked_kinds")}))
    return 0 if proof["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
