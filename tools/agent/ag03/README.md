# AG-03: the pixel-agent baseline harness (offline steps)

Operator tooling for [ADR-0038](../../../docs/adr/0038-ag03-vision-baseline-and-ag02-repetitions.md)
(chapter 09 §9.12). It compares two complete interfaces on the same 11 goals and the same two test
applications as AG-02: the structured contract (AG-02, Ommatix over MCP) and pixels (AG-03, a
`screenshot` / `click` / `type_text` server that is **not** Ommatix). It is not part of the product: no
crate, engine path or wire format is touched, and the engine does not run in this arm.

## What is allowed to run today, and what is not

**Nothing in this directory has been run against a real screen, a real window, real input or a real
provider**, and the first three lines below are the reason the rest is not yet allowed to:

| Step (ADR-0038 §B) | State | Why |
|---|---|---|
| 0 offline probe | **done**, verdict *yes* | Local stand-in only; see "Step 0 result" |
| 1 `pixel_server.py`, guards, synthetic mode | implemented, unit-tested with fakes | The `SendInput`/`PrintWindow` paths are written and **never executed** |
| 2 fidelity and isolation of `PrintWindow` on synthetic windows | **not done, not allowed yet** | Needs the owner: it puts synthetic windows on the screen. Without its admission record the server returns **no image** |
| 3 preflight and proof (native and routes) | implemented, tested against the stand-in | Run `pixel_preflight.py` with the owner; no provider is reached |
| 3b gate smoke per model | **not done, not allowed yet** | A real provider call with synthetic images: needs the owner's go-ahead, OI-2 reading recorded for the provider, and OI-1 (the Claude account's plan and model-improvement setting) before the Claude smoke |
| 4 runner | implemented, tested with a fake application, sampler and Claude | Phase 2 needs `--live`, the owner present, a consent per series |
| 5 independent review of the `SendInput` helper and the guards | **not done: blocks every live run** | A Codex review in a visible Orca terminal; file in `docs/reviews/` |
| 6 owner-run phase 1, then phase 2 | **not done** | The plan rows `AG-03` and `AG-02R` stay `NO_EJECUTADO` |

Do not run `pixel_runner.py` without `--plan` until steps 2, 3b and 5 are recorded.

## Files

| File | What it is |
|---|---|
| `pixel_guards.py` | The guards of decision 4 as **pure functions** of a window snapshot; the two-name executable constant |
| `pixel_server.py` | The stdio MCP server with exactly `screenshot`, `click`, `type_text`; `--synthetic` mode; the AG-02 ledger imported, not copied |
| `pixel_win32.py` | `ctypes` bindings behind an interface and the **pure** `SendInput` structure builders; imports call no Win32 function |
| `pixel_capture.py` | The capture backend interface, the admission record, `CAPTURE_REFUSED`, PNG encoding, the synthetic drawn images |
| `pixel_agent.py` | The command, the pinned instruction (SHA-256), the request verdict (images only in the `screenshot` result), the stream reader |
| `pixel_preflight.py` | The isolation proof `ag03-isolation-proof-1`, native and `--route mimo|deepseek`, with positive controls |
| `pixel_gate.py` | The records of the per-model gate smoke (pass, fail, exclusion); never calls a provider |
| `pixel_runner.py` | The `run_goal.py`-style runner; AG-02's oracle and scoring unchanged |
| `pixel_plan.py`, `pixel_stats.py`, `pixel_slots.py` | The pre-registered orders and fingerprint rule; the interval and exact-test helpers; the slot, retry and campaign-stop rules |
| `probe_image.py` | Step 0: does Claude Code forward an MCP image block? |
| `fake_*.py`, `test_*.py` | Fakes (clock, operating system, sender, capture backends, Messages API, `claude`) and the tests |

Run the tests with `python -m unittest discover -s tools/agent -p "test_*.py"` (what CI runs) or
`python -m unittest discover -s tools/agent/ag03`. They need no screen, no network beyond `127.0.0.1`
and no provider, and **no test ever injects real input** (AGENTS.md §6).

## Step 0 result (19 October 2026, Claude Code 2.1.304)

`python tools/agent/ag03/probe_image.py --claude-config-dir EMPTY_DIR --out RESULT.json` runs the real
Claude Code CLI against a loopback stand-in with the pixel server in synthetic mode (a drawn PNG; nothing
captured). For each of four sizes (320×200, 1280×720, 1920×1080, 2560×1440) it reads the Messages
requests the CLI actually sent:

* the tool result's image **is forwarded as an image block inside the `tool_result`** (base64 PNG),
  not as text and not dropped;
* it is **byte-identical** to what the tool returned up to 1920×1080;
* at 2560×1440 the CLI **downscales and re-encodes** it (2000×1125); the harness therefore caps an image's
  long edge at 1568 pixels itself (`pixel_capture.MAX_IMAGE_EDGE`), so the CLI never rescales and the click
  mapping stays exact;
* **every earlier screenshot is retained** in later requests (requests carried 0, 1, 2 and 3 images), which is
  why the estimated image-token share counts every appearance (decision 9).

This says nothing about whether a *provider* accepts the image or the model reads it: that is the gate.

## The coordinate actuator

`click(x, y)` is one left click at a pixel of the **last screenshot** (the window's client area, scaled to
at most 1568 px on the long edge; the server maps it back). `type_text(text)` is 1 to 16 printable ASCII
characters. Nothing else can be expressed: no double or right click, drag, wheel, Enter, Tab or shortcut.

Fail-closed, in this order (`pixel_server._click` / `_type_text` / `_deliver`): the stop file is absent;
the phase allows it (phase 1: `simulated`, no operating-system call, the hit test labels the click);
a screenshot of at most 10 s exists; **the ledger reserves** (AG-02's `proxy.reserve`: same file, per-goal
and per-session caps, an unreadable ledger refuses everything); the foreground window is the process this
run started, one of the two test executables (never `explorer.exe` or `applicationframehost.exe`), not a
helper window, not elevated, on the primary monitor, not iconified, and still the window and geometry of the
screenshot; the point is inside the client rectangle and `WindowFromPoint` resolves to the same top-level
window (`type_text`: the keyboard-focus window belongs to it); no modifier held; then **the delivery check**,
a fresh reading of the clock and the system immediately before the batch of all of the stop file, the age,
the modifiers and the window evidence. A refusal after the reservation **keeps it**.

## Privacy

Images exist only in memory, to answer the call. The server's log, the runner's results and the proofs hold
counts, pixel sizes, byte sizes, latencies, closed codes, labels of the goal's fixed vocabulary and
booleans: never image bytes, a hash of an image, screen text or a typed text; no stderr is stored
(`test_privacy.py`). What leaves the machine is the pixels of the attested window of the two synthetic test
applications to Anthropic, DeepSeek and Xiaomi (only after a model passes its gate), under the owner's
decision O1; **nothing here deletes a screenshot remotely**. Outputs belong outside the checkout
(`%LOCALAPPDATA%\Ommatix\runs\ag03`).

## Interpretations the implementation made (the architect should confirm them)

1. **Safety events** (decision 8): the refusals of guards 5 to 8 and a click outside the window
   (`pixel_runner.GUARD_VIOLATION_CODES`) end the series and create the operator's stop file; a stop, a stale
   or missing screenshot, an exhausted budget, a cap, a bad argument and a capture refusal do not.
2. The ledger reservation happens after the cheap checks (stop, phase, age) and **before** the window
   evidence is read, so a refusal at guards 5 to 7 also keeps it.
3. The pooling key excludes the start time and the proof's date (`pixel_plan.RECORDED_ONLY`): the route proof is
   regenerated before each run and the repetitions span days.
4. The screenshot is the client area (`PW_CLIENTONLY`); a window moved or resized after the screenshot refuses
   (`GEOMETRY_CHANGED`) instead of clicking at a stale mapping.
5. The read-only hit test (UI Automation point lookup of the control's name, reduced to the vocabulary) is
   also run in phase 2 when available, for the wrong-target-click metric; it never reaches the agent.
