#!/usr/bin/env python3
"""A scripted stand-in for the ``claude`` CLI for the gate-smoke tests (``gate_smoke.py``): it reaches no network.

It prints an ``init`` event with the same keys as the preflight stand-in (``fake_claude_pixel.init_event``), then
``FAKE_SMOKE_SHOTS`` (default 4) ``screenshot`` calls whose results carry a small drawn PNG, then a ``result`` event whose
answer depends on ``FAKE_SMOKE_MODE``:

``good``: the four lines the smoke asks for, with the word and shape of each scene; ``wrong``: another word per image;
``garbled``: no ``IMAGE n:`` lines (``FAKE_SMOKE_SIZE`` WxH sets the size of the drawn images); ``error_image`` / ``error_transport`` / ``error_other``: an error result with a refusal
text of that class; ``extra_tool``: a fourth tool at init; ``hang``: sleeps past the timeout.

Records its argv, and the NAMES (never the values) of its environment, to ``FAKE_SMOKE_RECORD``; ``FAKE_SMOKE_SECRET``
names a value that must not be in the argv (the test's key).
"""

import base64
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_claude_pixel as fcp  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_gate as gate  # noqa: E402


def answer_for(mode: str) -> str:
    lines = []
    for n, (word, shape) in enumerate(gate.expected_answers(), 1):
        if mode == "wrong":
            word = "ZZZ"
        lines.append(f"IMAGE {n}: WORD={word} SHAPE={shape}")
    return "\n".join(lines) + "\nVERDICT: DONE"


def main() -> int:
    argv = sys.argv[1:]
    mode = os.environ.get("FAKE_SMOKE_MODE", "good")
    shots = int(os.environ.get("FAKE_SMOKE_SHOTS", str(gate.SMOKE_IMAGES)))
    if (record := os.environ.get("FAKE_SMOKE_RECORD")):
        secret = os.environ.get("FAKE_SMOKE_SECRET")
        with open(record, "w", encoding="utf-8") as handle:
            json.dump({"argv": argv, "env_names": sorted(os.environ), "prompt_seen": False,
                       "key_in_argv": bool(secret) and any(secret in a for a in argv),
                       "key_in_env": bool(secret) and secret in os.environ.values(),
                       "temp": {k: os.environ.get(k) for k in ("TEMP", "TMP", "TMPDIR", "CLAUDE_CODE_TMPDIR")}}, handle)
    prompt = sys.stdin.read()
    home = Path(os.environ.get("CLAUDE_CONFIG_DIR", ""))
    tools = fcp.TOOLS + (["Bash"] if mode == "extra_tool" else [])
    print(json.dumps(fcp.init_event(argv, home, tools)), flush=True)
    if mode == "hang":
        time.sleep(600)
    width, height = (int(v) for v in os.environ.get("FAKE_SMOKE_SIZE", "1280x720").split("x"))
    for n in range(shots):
        png = cap.synthetic_png(n, width, height)[0]
        call = {"type": "tool_use", "id": f"t{n}", "name": fcp.PREFIX + "screenshot", "input": {}}
        print(json.dumps({"type": "assistant", "message": {"id": f"m{n}", "usage": {"input_tokens": 100, "output_tokens": 5},
                                                          "content": [call]}}), flush=True)
        content = [{"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                                "data": base64.b64encode(png).decode("ascii")}},
                   {"type": "text", "text": "Screenshot of the application window."}]
        print(json.dumps({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": f"t{n}",
                                                                    "content": content}]}}), flush=True)
    if mode.startswith("error_"):
        text = {"error_image": "API Error: 400 invalid request: image content blocks are not supported",
                "error_transport": "API Error: connect ECONNRESET", "error_other": "API Error: 401 unauthorized"}[mode]
        print(json.dumps({"type": "result", "subtype": "error_during_execution", "is_error": True, "result": text,
                          "num_turns": shots, "total_cost_usd": 0.0}), flush=True)
        return 1
    answer = "I cannot read these images." if mode == "garbled" else answer_for(mode)
    print(json.dumps({"type": "assistant", "message": {"id": "mf", "usage": {"input_tokens": 100, "output_tokens": 40},
                                                      "content": [{"type": "text", "text": answer}]}}), flush=True)
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": answer,
                      "num_turns": shots + 1, "total_cost_usd": 0.01, "prompt_chars": len(prompt)}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
