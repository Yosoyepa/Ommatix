#!/usr/bin/env python3
"""A scripted stand-in for the ``claude`` CLI, for the AG-02 preflight tests.

It reads the flags the runner passes, prints a stream-json ``init`` event and a ``result`` event,
and posts one request to ``ANTHROPIC_BASE_URL`` whose body holds the user's prompt plus whatever
context a real CLI would have loaded: with the settings sources on (the preflight's positive
control), or in mode ``leak``, the CLAUDE.md files of the configuration directory, the working
directory and its ancestors, and the configuration directory's rules. ``FAKE_CLAUDE_MODE``:

``normal`` loads nothing in the production flags; ``leak`` loads context regardless;
``blind`` loads nothing even in the control; ``hooks`` runs the configuration directory's hook
commands; ``extra_tool`` lists an extra tool at init; ``silent`` sends no request.
``FAKE_CLAUDE_ARGV_FILE`` receives one JSON line with the argv. ``FAKE_CLAUDE_ENV_FILE`` receives the
names of every environment variable it got and the two values a stand-in run needs. Like the real
CLI, it honours provider-selection variables: with ``CLAUDE_CODE_USE_BEDROCK`` or a Bedrock/Vertex/
Foundry endpoint in its environment it would send the request there instead; it only records that
(``FAKE_CLAUDE_PROVIDER_FILE``) and sends nothing.
"""

import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

TOOLS = ["mcp__ommatix__observe_screen", "mcp__ommatix__execute_action", "mcp__ommatix__get_engine_metrics"]


def flag(argv, name):
    return argv[argv.index(name) + 1] if name in argv else None


def main() -> int:
    argv = sys.argv[1:]
    mode = os.environ.get("FAKE_CLAUDE_MODE", "normal")
    record = os.environ.get("FAKE_CLAUDE_ARGV_FILE")
    if record:
        with open(record, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(argv) + "\n")
    env_file = os.environ.get("FAKE_CLAUDE_ENV_FILE")
    if env_file:
        with open(env_file, "w", encoding="utf-8") as handle:
            json.dump({"names": sorted(os.environ), "base_url": os.environ.get("ANTHROPIC_BASE_URL"),
                       "api_key": os.environ.get("ANTHROPIC_API_KEY")}, handle)
    selectors = [n for n in os.environ if n.startswith(("CLAUDE_CODE_USE_", "ANTHROPIC_BEDROCK_",
                                                         "ANTHROPIC_VERTEX_", "ANTHROPIC_FOUNDRY_"))]
    if selectors and os.environ.get("FAKE_CLAUDE_PROVIDER_FILE"):
        with open(os.environ["FAKE_CLAUDE_PROVIDER_FILE"], "w", encoding="utf-8") as handle:
            handle.write("would have routed to a provider\n")
    prompt = sys.stdin.read()
    sources = flag(argv, "--setting-sources")
    home = Path(os.environ.get("CLAUDE_CONFIG_DIR", ""))
    loads = mode != "blind" and (mode == "leak" or sources not in ("", None))
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
    print(json.dumps({"type": "system", "subtype": "init", "claude_code_version": "2.1.302",
                      "model": flag(argv, "--model"), "tools": tools,
                      "mcp_servers": [{"name": "ommatix", "status": "connected"}], "skills": [],
                      "slash_commands": [], "plugins": [{"name": "p1"}], "agents": ["a1"],
                      "memory_paths": {"auto": str(home / "projects" / "scratch" / "memory")},
                      "additional_directories": []}), flush=True)
    answer = "VERDICT: DONE"
    if mode != "silent":
        body = json.dumps({"model": flag(argv, "--model"), "stream": False,
                           "system": flag(argv, "--system-prompt"),
                           "messages": [{"role": "user", "content": prompt + "\n" + context}]}).encode()
        request = urllib.request.Request(os.environ["ANTHROPIC_BASE_URL"] + "/v1/messages", data=body,
                                         headers={"content-type": "application/json"})
        with urllib.request.urlopen(request, timeout=30) as reply:
            answer = json.loads(reply.read())["content"][0]["text"]
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": answer,
                      "num_turns": 1, "total_cost_usd": 0.0}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
