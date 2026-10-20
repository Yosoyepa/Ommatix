#!/usr/bin/env python3
"""AG-03 Step 0 (ADR-0038 section B): does Claude Code forward an MCP ``image`` block to the model?

The offline probe. It runs the real Claude Code CLI - the production command of ``pixel_agent`` - against
the **local stand-in** (``fake_messages.ScriptedMessages`` on 127.0.0.1, a scripted model that calls
``screenshot`` three times and then answers) with the pixel server in **synthetic mode** (a drawn PNG of
a fake window; nothing is captured, nothing is read from the operating system, nothing is sent). No
provider is reached: the child's environment is the AG-02 preflight's scrubbed one (no provider
selector, endpoint, credential or profile of the operator's; a dummy key for the loopback stand-in), and
the probe refuses to run when provider-routing variables are set or a managed configuration is found.

For each requested image size it records, from the Messages requests the CLI really sent: whether the
tool result's image arrived as an image block (and not as text, and not dropped), its media type, format,
dimensions and byte size, whether the bytes are identical to the ones the tool returned (compared in
memory; no hash is written), whether it was re-encoded or downscaled, and how many earlier screenshots
each later request still carries (the retained history). The result file holds numbers and booleans only.

``python probe_image.py --claude-config-dir DIR --out RESULT.json`` (DIR an existing, empty directory
outside the repository; RESULT.json absolute and outside it).
"""

from __future__ import annotations

import argparse
import base64
import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_messages  # noqa: E402
import pixel_agent as pa  # noqa: E402
import pixel_capture as cap  # noqa: E402
import preflight as ag02_preflight  # noqa: E402
import proxy  # noqa: E402
import run_goal as rg  # noqa: E402

SCHEMA = "ag03-step0-probe-1"
SIZES = ((320, 200), (1280, 720), (1920, 1080), (2560, 1440))
SHOTS = 3
DUMMY_KEY = "sk-ant-canary-0000000000"


def script(request: dict, n: int) -> dict:
    if n < SHOTS:
        return {"tool": f"mcp__{pa.SERVER_NAME}__screenshot", "input": {}}
    return {"text": "VERDICT: DONE"}


def analyse(requests: list[dict], width: int, height: int) -> dict:
    """The verdict facts for one session's captured requests (pure; the tests call it directly)."""
    source = cap.synthetic_png(0, width, height)[0]
    per_request, first = [], None
    as_text = False
    for request in requests:
        try:
            body = json.loads(request["body"])
        except ValueError:
            continue
        if not isinstance(body, dict) or "tools" not in body:
            continue
        images = [i for i in pa.image_blocks(body) if i["where"] == "tool_result:screenshot"]
        per_request.append(len(images))
        as_text = as_text or bool(pa.PNG_IN_TEXT.search(pa.decoded_text(body)))
        if images and first is None:
            first = (body, images[0])
    out = {"requested_size": [width, height], "requests_with_tools": len(per_request),
           "images_per_request": per_request, "image_as_text": as_text, "forwarded": first is not None}
    if first is not None:
        body, image = first
        data = _first_image_bytes(body)
        out |= {"media_type": image["media_type"], "format": image["format"], "width": image["width"],
                "height": image["height"], "bytes": image["bytes"],
                "identical_to_source": data == source,
                "reencoded": data != source,
                "downscaled": bool(image["width"]) and (image["width"], image["height"]) != (width, height),
                "source_bytes": len(source)}
    return out


def _first_image_bytes(body: dict) -> bytes:
    for message in body.get("messages", []):
        content = message.get("content") if isinstance(message, dict) else None
        for block in content if isinstance(content, list) else []:
            if isinstance(block, dict) and block.get("type") == "tool_result":
                for part in block.get("content") or []:
                    if isinstance(part, dict) and part.get("type") == "image":
                        return base64.b64decode(part["source"]["data"])
    return b""


def verdict_of(sessions: list[dict]) -> tuple[str, list[str]]:
    """``yes`` (every size reached the model as an image block), ``no`` (none did) or ``partial``."""
    problems = []
    forwarded = [s["forwarded"] for s in sessions]
    if not any(forwarded):
        problems.append("no_image_forwarded")
        return "no", problems
    if not all(forwarded):
        problems.append("some_sizes_not_forwarded")
        return "partial", problems
    if any(s["image_as_text"] for s in sessions):
        problems.append("image_also_as_text")
    return "yes", problems


def run_session(args, width: int, height: int, config_dir: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="ag03-probe-") as tmp, \
            fake_messages.ScriptedMessages(script) as api:
        tmp = Path(tmp)
        log = tmp / "pixel.jsonl"
        mcp = pa.build_mcp_config(sys.executable, ["--synthetic", "--synthetic-size", f"{width}x{height}",
                                                   "--log", str(log)])
        mcp_path = tmp / "mcp.json"
        mcp_path.write_text(json.dumps(mcp), encoding="utf-8")
        argv = pa.build_claude_argv(claude=args.claude, model=args.model, mcp_config=mcp_path,
                                    system_instruction=pa.SYSTEM_INSTRUCTION, max_budget_usd=0.5)
        env = rg.scrubbed_env({"CLAUDE_CONFIG_DIR": str(config_dir), "ANTHROPIC_BASE_URL": api.url,
                               "ANTHROPIC_API_KEY": DUMMY_KEY})
        scratch = tmp / "scratch"
        scratch.mkdir()
        metrics = pa.PixelStream([], None, None)
        cli = rg.run_claude(argv, pa.build_prompt({"prompt": "Describe the window."}, 1), scratch, metrics,
                            args.timeout, env=env)
        facts = analyse(list(api.requests), width, height)
        summary = metrics.summary()
        facts |= {"cli_exit": cli.get("exit_code"), "cli_killed": cli.get("killed"),
                  "tool_calls": summary["tool_calls"], "screenshots_seen_by_stream": summary["screenshots"],
                  "init_problems": metrics.init_problems,
                  "claude_code_version": (metrics.attestation or {}).get("claude_code_version"),
                  "model": (metrics.attestation or {}).get("model"),
                  "gets": sorted(set(api.gets)), "paths": sorted(set(api.paths))}
        return facts


def run(args) -> dict:
    if (reasons := ag02_preflight.refusals()):
        raise ag02_preflight.PreflightRefused(reasons)
    sessions = [run_session(args, w, h, args.claude_config_dir) for w, h in SIZES]
    verdict, problems = verdict_of(sessions)
    versions = sorted({s["claude_code_version"] for s in sessions if s["claude_code_version"]})
    return {"schema": SCHEMA, "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "claude_code_versions": versions, "requested_model": args.model, "verdict": verdict,
            "problems": problems, "provider_reached": False,
            "stand_in": "fake_messages.ScriptedMessages on 127.0.0.1", "mode": "pixel server --synthetic",
            "sessions": sessions}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], allow_abbrev=False)
    parser.add_argument("--claude-config-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--model", default="claude-haiku-5-5")
    parser.add_argument("--claude", default="claude")
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args(argv)
    args.claude_config_dir, args.out = rg.absolute_arg(args.claude_config_dir), rg.absolute_arg(args.out)
    for name in ("claude_config_dir", "out"):
        if (problem := proxy.path_problem(getattr(args, name))):
            parser.error(f"--{name.replace('_', '-')}: {problem}")
    if not args.claude_config_dir.is_dir():
        parser.error("--claude-config-dir must be an existing directory")
    try:
        result = run(args)
    except ag02_preflight.PreflightRefused as refused:
        print(json.dumps({"verdict": None, "refused": refused.reasons}))
        return 2
    args.out.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({"verdict": result["verdict"], "problems": result["problems"],
                      "claude_code_versions": result["claude_code_versions"]}))
    return 0 if result["verdict"] == "yes" else 1


if __name__ == "__main__":
    sys.exit(main())
