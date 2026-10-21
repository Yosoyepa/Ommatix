#!/usr/bin/env python3
"""A scripted stand-in for the ``claude`` CLI, for the AG-03 preflight tests (the pixel twin of
``tools/agent/ag02/fake_claude.py``).

It prints a stream-json ``init`` event, an ``assistant`` tool call and a ``user`` tool result that carries
a synthetic PNG (so the stream reader sees a screenshot), posts ONE request to ``ANTHROPIC_BASE_URL`` the
way a real client would (the tool list, the tool call, the tool result with the image, the prompt plus
whatever context a real CLI would have loaded) and prints a ``result`` event. ``FAKE_CLAUDE_MODE``:

``normal``: loads nothing in the production flags; ``leak`` loads context regardless; ``blind`` loads
nothing even in the control; ``hooks`` runs the configuration directory's hook commands; ``extra_tool``
lists an extra tool at init and in the request; ``silent`` sends no request; ``garbled`` sends a body that
is not JSON. The modes that break exactly one image rule: ``image_elsewhere`` (an image in a user message),
``image_in_click`` (the image is the result of ``click``), ``image_oversized``, ``image_wide``,
``image_jpeg``, ``image_text`` (the PNG base64 as text), ``no_image`` (no screenshot result at all).
With ``ANTHROPIC_AUTH_TOKEN`` in its environment (a route) it also sends both credential headers and the
effort. The ``route_*`` modes of ``ag02/fake_claude.py`` that apply (``route_other_model``,
``route_key_in_body``, ``route_bad_effort``, ``route_x_api_key``, ``route_proxy_authorization``,
``route_cookie``) break one of those. The on-disk modes write what the real CLI writes WITHOUT
``--no-session-persistence``, in its cache or in its temp directory: ``persist_tool_results`` (an image under
``projects/<cwd>/<session>/tool-results/``), ``persist_transcript`` (a project transcript carrying the image in base64),
``persist_clean_transcript`` (a transcript without one), ``cache_log_image`` and ``cache_log_clean`` (a debug log of the MCP
server under ``FAKE_CLAUDE_CACHE_ROOT``, with and without image data), ``temp_image`` / ``tmpdir_image`` / ``temp_clean``
(an image, or a clean state file, under the child's ``TEMP`` / ``CLAUDE_CODE_TMPDIR``). Records argv and environment names like the AG-02
stand-in.
"""

import base64
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import pixel_capture as cap  # noqa: E402

PREFIX = "mcp__pixel__"
TOOLS = [PREFIX + n for n in ("screenshot", "click", "type_text")]


def flag(argv, name):
    return argv[argv.index(name) + 1] if name in argv else None


def b64(data: bytes) -> str:
    return base64.b64encode(data).decode("ascii")


def write_disk_state(mode: str, home: Path) -> None:
    """What the real CLI leaves on disk when it persists the session (see the module docstring)."""
    png = cap.synthetic_png(0, 64, 48)[0]
    project = home / "projects" / "scratch-cwd"
    if mode == "persist_tool_results":
        (project / "0ecf8087" / "tool-results").mkdir(parents=True, exist_ok=True)
        (project / "0ecf8087" / "tool-results" / "mcp-pixel-blob-1.png").write_bytes(png)
    elif mode in ("persist_transcript", "persist_clean_transcript"):
        project.mkdir(parents=True, exist_ok=True)
        content = b64(png) if mode == "persist_transcript" else "hello"
        (project / "0ecf8087.jsonl").write_text(json.dumps({"message": content}) + "\n", encoding="utf-8")
    elif mode in ("temp_image", "temp_clean", "tmpdir_image"):
        variable = "CLAUDE_CODE_TMPDIR" if mode == "tmpdir_image" else "TEMP"
        folder = Path(os.environ[variable]) / "claude" / ("state" if mode == "temp_clean" else "images")
        folder.mkdir(parents=True, exist_ok=True)
        if mode == "temp_clean":
            (folder / "cache-break-state.json").write_text("{}", encoding="utf-8")
        else:
            (folder / "synthetic.png").write_bytes(png)
    elif mode in ("cache_log_image", "cache_log_clean") and os.environ.get("FAKE_CLAUDE_CACHE_ROOT"):
        log = Path(os.environ["FAKE_CLAUDE_CACHE_ROOT"]) / "C--scratch-cwd" / "mcp-logs-pixel"
        log.mkdir(parents=True, exist_ok=True)
        debug = "connected" if mode == "cache_log_clean" else b64(png)
        (log / "2026-10-20T00-00-00-000Z.jsonl").write_text(
            json.dumps({"debug": debug, "cwd": str(Path.cwd()), "sessionId": "s", "timestamp": "t"}) + "\n",
            encoding="utf-8")


def init_event(argv, home: Path, tools) -> dict:
    """The ``init`` event of the stand-in (shared with ``fake_claude_smoke``, so that both carry the same keys)."""
    return {"type": "system", "subtype": "init", "claude_code_version": "2.1.304",
            "model": flag(argv, "--model"), "tools": tools,
            "mcp_servers": [{"name": "pixel", "status": "connected"}], "skills": [],
            "slash_commands": [], "plugins": [{"name": "p1"}], "agents": ["a1"],
            "memory_paths": {"auto": str(home / "projects" / "scratch" / "memory")},
            "additional_directories": []}


def main() -> int:
    argv = sys.argv[1:]
    mode = os.environ.get("FAKE_CLAUDE_MODE", "normal")
    if (record := os.environ.get("FAKE_CLAUDE_ARGV_FILE")):
        with open(record, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(argv) + "\n")
    if (env_file := os.environ.get("FAKE_CLAUDE_ENV_FILE")):
        with open(env_file, "w", encoding="utf-8") as handle:
            json.dump({"names": sorted(os.environ), "base_url": os.environ.get("ANTHROPIC_BASE_URL"),
                       "api_key": os.environ.get("ANTHROPIC_API_KEY"),
                       "feedback_survey": os.environ.get("CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY"),
                       "temp": {k: os.environ.get(k) for k in ("TEMP", "TMP", "TMPDIR", "CLAUDE_CODE_TMPDIR")},
                       "xdg_cache_home": os.environ.get("XDG_CACHE_HOME")}, handle)
    prompt = sys.stdin.read()
    sources = flag(argv, "--setting-sources")
    home = Path(os.environ.get("CLAUDE_CONFIG_DIR", ""))
    leak_dir = os.environ.get("FAKE_CLAUDE_LEAK_DIR")
    loads = mode != "blind" and (mode == "leak" or sources not in ("", None)
                                 or (mode == "leak_dir" and leak_dir and str(home) == leak_dir))
    write_disk_state(mode, home)
    context = ""
    if loads:
        files = [home / "CLAUDE.md", *sorted((home / "rules").glob("*.md")),
                 *sorted((home / "projects").glob("*/memory/MEMORY.md"))]
        here = Path.cwd()
        for folder in [here, *here.parents]:
            files += [folder / "CLAUDE.md", folder / "CLAUDE.local.md"]
        context = "\n".join(f.read_text(encoding="utf-8") for f in files if f.is_file())
    if mode == "hooks":
        settings = json.loads((home / "settings.json").read_text(encoding="utf-8"))
        for entries in settings.get("hooks", {}).values():
            for entry in entries:
                for hook in entry["hooks"]:
                    subprocess.run(hook["command"], shell=True, capture_output=True, check=False)
    tools = TOOLS + (["Bash"] if mode == "extra_tool" else [])
    print(json.dumps(init_event(argv, home, tools)), flush=True)
    png = cap.synthetic_png(0, 64, 48)[0]
    image = {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64(png)}}
    text = {"type": "text", "text": "Screenshot of the application window, 64 by 48 pixels."}
    result_content = [image, text]
    tool_name = PREFIX + "screenshot"
    extra_messages = []
    if mode == "no_image":
        result_content = [text]
    elif mode == "image_in_click":
        tool_name = PREFIX + "click"
    elif mode == "image_oversized":
        result_content = [{"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                                       "data": b64(png + bytes(cap.MAX_IMAGE_BYTES + 1))}}, text]
    elif mode == "image_wide":
        wide = cap.png_encode(cap.MAX_IMAGE_EDGE + 200, 1, bytes((cap.MAX_IMAGE_EDGE + 200) * 3))
        result_content = [{"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                                       "data": b64(wide)}}, text]
    elif mode == "image_jpeg":
        result_content = [{"type": "image", "source": {"type": "base64", "media_type": "image/jpeg",
                                                       "data": b64(b"\xff\xd8\xff\xe0" + bytes(60))}}, text]
    elif mode == "image_elsewhere":
        extra_messages.append({"role": "user", "content": [image]})
    elif mode == "image_text":
        extra_messages.append({"role": "user", "content": b64(png)})
    print(json.dumps({"type": "assistant", "message": {"id": "m1", "usage": {"input_tokens": 3, "output_tokens": 1},
                                                      "content": [{"type": "tool_use", "id": "t1", "name": tool_name,
                                                                   "input": {}}]}}), flush=True)
    print(json.dumps({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1",
                                                                "content": result_content}]}}), flush=True)
    answer = "VERDICT: DONE"
    if mode != "silent":
        payload = {"model": flag(argv, "--model"), "stream": False, "system": flag(argv, "--system-prompt"),
                   "tools": [{"name": n} for n in tools]
                            + ([{"name": "DeferredToolPlaceholder"}] if mode == "deferred_placeholder"
                               and os.environ.get("ENABLE_TOOL_SEARCH") != "false" else []),
                   "messages": [{"role": "user", "content": prompt + "\n" + context},
                                {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": tool_name,
                                                                   "input": {}}]},
                                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1",
                                                              "content": result_content}]},
                                *extra_messages]}
        headers = {"content-type": "application/json"}
        token = os.environ.get("ANTHROPIC_AUTH_TOKEN")
        effort = flag(argv, "--effort") or os.environ.get("CLAUDE_CODE_EFFORT_LEVEL")
        payload["output_config"] = {"effort": "low" if mode == "native_bad_effort" else effort}
        if token:
            payload |= {"thinking": {"type": "adaptive"},
                        "output_config": {"effort": "low" if mode == "route_bad_effort" else effort}}
            if mode == "route_other_model":
                payload["model"] = "claude-haiku-5-5"
            if mode == "route_key_in_body":
                payload["messages"].append({"role": "user", "content": token})
            headers |= {"authorization": f"Bearer {token}",
                        "x-api-key": "other-" + token if mode == "route_x_api_key" else token}
            if mode == "route_proxy_authorization":
                headers["proxy-authorization"] = "Bearer a-different-credential-1"
            if mode == "route_cookie":
                headers["cookie"] = "session=a-different-credential-2"
        body = json.dumps(payload).encode()
        if mode == "garbled":
            body = b"this is not json {"
        request = urllib.request.Request(os.environ["ANTHROPIC_BASE_URL"] + "/v1/messages", data=body, headers=headers)
        with urllib.request.urlopen(request, timeout=30) as reply:
            blocks = json.loads(reply.read())["content"]
            answer = next((b["text"] for b in blocks if b.get("type") == "text"), answer)
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": answer,
                      "num_turns": 1, "total_cost_usd": 0.0}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
