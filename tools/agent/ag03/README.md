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
| 4 runner | implemented and connected to the slot, plan, fingerprint and gate machinery; tested through `main()` with a fake application, sampler and Claude | One run is one series (a model, a phase, a repetition); phase 2 needs `--live`, the owner present, a consent per series |
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
| `pixel_runner.py` | The `run_goal.py`-style runner and the production entry point of one series (slots, retry, gate, pool); AG-02's oracle and scoring unchanged |
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

Fail-closed, in this order (`pixel_server._click` / `_type_text` / `_deliver`):

1. the operator's stop file is absent (**unreadable is a refusal**; only a genuine absence passes);
2. the phase allows it (phase 1: `simulated`, no input; the read-only scoring hit test runs only after the
   target is attested);
3. a screenshot of at most 10 s exists;
4. **the ledger reserves** (AG-02's `proxy.reserve`: same file, per-goal and per-session caps, an unreadable
   ledger refuses everything);
5. to 7. the foreground window is the process this run started, one of the two test executables (never
   `explorer.exe` or `applicationframehost.exe`), with a readable class that is not a helper window, not
   elevated, on the primary monitor, not iconified and still the window and geometry of the screenshot; the
   point is inside the client rectangle and `WindowFromPoint` resolves to the same top-level window
   (`type_text`: the keyboard-focus window belongs to it); no modifier held. The key states count only with **positive evidence that the read could observe** them: `GetAsyncKeyState` returns 0 both for a key that is up and for a read that failed, so zero is "none held" only when this thread's desktop is, by name, the one that receives input, that input desktop could be opened with the hook and journal rights as well as read access (what reading another process's input needs), and this process runs at medium integrity or above (UIPI). Any of those missing is `MODIFIERS_UNREADABLE`, never "none held";
6. then the read-only scoring hit test (inside that scope), the preparation of the native batch, and
7. **the delivery check**: a fresh reading of the clock and the system immediately before the one `SendInput`
   call of all of the stop file, the age, the modifiers and the window evidence. For a click it also holds the
   **virtual-desktop geometry** (origin and size) the batch's absolute coordinates were normalised on: the
   delivery reading must report exactly that, or nothing is sent (`DESKTOP_CHANGED`, `DESKTOP_UNREADABLE`).
   The system reading takes its slow facts first and reads the **foreground window again as its very last
   query**, so a window that lost the foreground during the acquisition is refused; keyboard focus is asked of the
   current foreground thread (thread id 0), not of the thread of a window that may have been replaced; and a
   reading that took more than 1 s on the injected clock is `EVIDENCE_STALE`.

**What remains, stated plainly.** No sequence of user-mode queries makes the operating system's dispatch atomic:
some time always passes between the last query and the `SendInput` call. These checks narrow that window; they do
not close it, and the step-6 owner session must report it (ADR-0038, review of step 5).

A refusal after the reservation **keeps it**. A failed Win32 read is unknown evidence, never affirmative
(an empty class name, a NULL ancestor, a failed bitmap selection each refuse).

### What a refusal means (decided by the architect, 2026-10-20; classified by guard and stage)

| Guard | Examples | Meaning |
|---|---|---|
| 5, 6, 7 and the window/foreground/focus/modifier parts of the delivery check; a click outside the window or the image; unreadable system evidence | `FOREGROUND_NOT_TARGET_PROCESS`, `POINT_WINDOW_MISMATCH`, `MODIFIER_HELD`, `MODIFIERS_UNREADABLE`, `POINT_OUTSIDE_CLIENT`, `DESKTOP_CHANGED`, `DESKTOP_UNREADABLE`, `EVIDENCE_STALE`, `OS_READ_FAILED` | **Terminal safety event**: the server latches (`SESSION_TERMINATED` to every later call, screenshots included), an independent watcher (not the CLI's next output line) kills the **whole owned job/tree at once, with no grace**, within a few hundred milliseconds, the stop file is created by exclusive create (never overwritten or deleted) and the series ends |
| A failed, partial or uncertain `SendInput` | `INPUT_FAILED` | **Terminal, uncertain delivery**: logged `delivered: "uncertain"` with the inserted count, never reported as nothing sent, never retried |
| Items 1 and the stop part of the delivery check (present or unreadable) | `STOP_ENGAGED`, `STOP_UNREADABLE` | **Operator stop**: the campaign ends; not a safety event; the runner neither creates nor deletes the operator's file |
| Item 3 and the age part of the delivery check; item 4 (budget); arguments, caps, a capture refusal | `SCREENSHOT_STALE`, `LIVE_BUDGET_EXHAUSTED`, `CAPTURE_REFUSED` | Ordinary scored refusals: the session goes on |

## One run is one series

`pixel_runner.py` runs **one series**: one model, one phase, one repetition (`--repetition`), one consent. It
runs the 11 goals in the pre-registered order of `pixel_plan`, through `pixel_slots`: the primary slot, the one
permitted retry of an attempt that failed **before the session was launched** (a stop or a safety event takes
precedence over the retry; after launch the model may have been asked, so no later classification - unclean
init, timeout, parse - earns a retry or discards a recorded terminal state), raw artifacts per attempt
(`<series>/<goal>/attempt<n>/`), technical failures, not-attempted slots after a campaign stop, and
`series.json` with the counts.

* **Gate**: `--gate-record` must be this model's passing record, bound to the requested model, its attested
  (resolved) identity and, on a route, the endpoint; only the owner's accepted set runs (Haiku 5.5,
  `deepseek-flash`, `mimo-v2.6-flash`). A mismatch is refused before any application or session.
* **Lock**: a single exclusive `campaign.lock` in `--campaign-dir`, held for the whole process lifetime. A second executor
  (a series or a diagnostic) is refused outright - no waiting, no takeover, no guess about whether the owner is alive. A lock
  left by a hard crash is removed **by hand** by the operator, after checking that nothing of the campaign is still running.
* **Interruption**: on every exit path (normal, exception, Ctrl+C, SIGTERM, Ctrl+Break) the owned job/tree is killed and
  verified gone **before** the lock is released, and the cleanup itself is best effort and idempotent (every step is attempted; **no signal is ignored or remapped
  during it** - a Ctrl+C that lands inside simply propagates and leaves the session unverified). The lock is released
  **only on positive proof** of quiescence (an error out of `Popen` proves no child only when it is a refusal that cannot follow a creation - the program or directory is not there; any other error may come from `Popen`'s clean-up after a successful `CreateProcess`, so it keeps the lock), and the owned tree lives in a Windows job object created with
  `KILL_ON_JOB_CLOSE`, so the kernel ends it if the harness dies; **ownership is recorded before the CLI exists**
  (the counter is raised before `Popen`; the CLI is created suspended, put in the job and only then resumed), so an
  interruption at any point of the start - inside `Popen`, the job set-up, the native resume or the bookkeeping after
  it - keeps the lock unless the CLI is positively shown gone; if that cannot be verified, the lock is **kept** and the runner says so:
  check that nothing of the campaign is running, then remove it by hand. The watcher's effects (stop file, kill) go through
  one gate that is closed before `run_claude` returns, so nothing can act on an ended session.
* **Logs**: a server log that cannot be read or parsed completely is a terminal **uncertain delivery** (deliveries unknown,
  not zero): the tree is killed, the stop is engaged and the attempt cannot score a success. Only a log that does not exist
  yet is not an event.
* **Claims**: every attempt is claimed atomically in `--campaign-dir` before it runs and settled after, so a
  different `--out`, a restart or a second invocation never re-runs a settled primary slot nor grants a new
  attempt; an interrupted series resumes from its claims (an attempt that never recorded an outcome counts as
  consumed, of unknown progress); a settled or stopped series is refused. `--diagnostic` runs a series outside
  the primary pool: it writes only its own directory and never reads, creates or is constrained by the primary pool.
* **Fingerprint**: built from the actual arguments (timeout, dollar or token cap, session budget, caps, effort,
  versions, instruction, backend) and the **measured** primary-monitor resolution and numeric scaling, read only in a per-monitor v2 DPI context
  verified at the **process** level (and the reading thread's) in the runner's own process - no thread-level fallback - and
  recorded beside them (an unmeasured or unverified display is refused); it must equal the campaign's (the refusal is recorded in `not_pooled.jsonl`).
* A terminated attempt (timeout, token cap, nonzero exit, provider error, capture refusal) cannot score a
  success; AG-02's raw answer is kept as `score_ag02` and the OS oracle as `oracle_only_ok`.
* Native Claude keeps the registered 0.50 USD cap and effort `max`; a provider route has the context-token cap
  and effort `max` in its child environment, and reports the image-token estimate as unavailable.

## Privacy

Images exist only in memory, to answer the call. The server's log, the runner's results and the proofs hold
counts, pixel sizes, byte sizes, latencies, closed codes, labels of the goal's fixed vocabulary and
booleans: never image bytes, a hash of an image, screen text or a typed text; no stderr is stored
(`test_privacy.py`). What leaves the machine is the pixels of the attested window of the two synthetic test
applications to Anthropic, DeepSeek and Xiaomi (only after a model passes its gate), under the owner's
decision O1; **nothing here deletes a screenshot remotely**. Outputs belong outside the checkout
(`%LOCALAPPDATA%\Ommatix\runs\ag03`).

## Implementation readings, as decided by the architect

1. Safety events, operator stops and ordinary refusals: the table above (terminal, classified by guard and stage).
2. The ledger reservation happens after the cheap checks and **before** the window evidence is read, so a refusal at
   guards 5 to 7 also keeps it; the order of the live path is pinned by `test_review101.R5OrderTests`.
3. The pooling key excludes the start time and the proof's date (`pixel_plan.RECORDED_ONLY`); everything
   experimental is in the key, built from the effective arguments.
4. The screenshot is the client area (`PW_CLIENTONLY`), at most 1568 px on the long edge; a window moved or resized
   after the screenshot refuses (`GEOMETRY_CHANGED`). This is conditional on a **verified** per-monitor v2 DPI context
   (read, never inferred from an error code) and on the future real step 2.
