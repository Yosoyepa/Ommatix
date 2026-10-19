# 0038. Prepare AG-03 (a vision-agent baseline) and repetitions of AG-02

- **Status:** Accepted
- **Date:** 2026-10-19
- **Approval:** The project owner, 2026-10-19 (O1, O2, O5, O7; O3-O12 per the architect's defaults stated to the owner; the owner may revise those defaults). The owner asked for both experiments to be **prepared, not run**: running either is a separate, later decision with the owner present and his consent for each session. Acceptance changes no test status or checkbox of chapter 06 §6.7.

## Context

[ADR-0036](0036-llm-only-agent-experiment-on-the-test-applications.md) and
[ADR-0037](0037-ag02-other-providers-through-a-confinable-agent.md) ran an LLM-only agent (Haiku 5.5,
DeepSeek, MiMo) on 11 fixed goals of the two test applications, with only the three Ommatix tools and
never a pixel. The recorded results (chapter 09 §9.11) have two gaps that stop them from supporting a
claim beyond a preprint:

1. **No baseline.** Ommatix's claim is computer use "without a VLM". Nothing shows what a vision agent
   does on the same goals, so no figure says whether the structured contract is cheaper, faster or more
   reliable than pixels. ADR-0036 decision 3 named this experiment AG-03 and left it for later.
2. **No variance.** Every model has **one** series per phase. Phase 2 scores are 6/11 (Haiku), 7/11
   (DeepSeek) and 7/11 (MiMo); the dossier itself says a one-goal difference between single runs does not
   distinguish the models. Failures also came from `STALE_SNAPSHOT` and `INDETERMINATE_EFFECT` chains and
   from a timeout, which are the kind of thing that varies from run to run.

The owner decided on 2026-10-19 to prepare both: **AG-03**, a VLM baseline on the same goals and
applications, and **AG-02R**, at least five repetitions of AG-02 per model. This ADR is the design.
Running either is a separate, later decision with the owner present.
The owner then decided the points recorded under "Owner decisions" below (2026-10-19).

What the owner has (2026-10-18/19): API keys only for DeepSeek and MiMo, and a Claude subscription (no
Anthropic API key). Codex and OpenCode are refused (ADR-0037 decision 6). Nothing below was run, no
provider was reached, and no screen was captured while writing it.

## Decision

### Owner decisions of 2026-10-19

Decided by the owner: **O1** the pixel exception is **accepted** as bounded in decision 5 (attested target
window only, held in memory). **O2** the vision arm is **every available model**: Claude Haiku 5.5
(native) and DeepSeek V4.1 Flash and MiMo V2.6 Flash through the confined Claude Code routes of
ADR-0037, **each gated** by its preflight and by **one real smoke on a synthetic image** that proves the
provider accepts image tool results; a model that fails its gate is **excluded and the exclusion is
recorded** (decision 2). Codex with GPT-6 Luna **stays refused**: its `view_image` reads local files,
its tool surface is wider than the experiment allows and it reads skills from the real home (ADR-0037
decision 6). The pixel exception therefore extends to exactly three recipients: **Anthropic, DeepSeek and
Xiaomi**. **O5** screenshots are held in memory only. **O7** n = 5 per model.

Defaults of the architect, stated to the owner and recorded here as such (the owner may revise them):
**O3** is subsumed by O2; **O4** a Python `ctypes` actuator with its own review before any live run;
**O6** G09 decoy text is a scored failure, not a safety event; **O8** one consent per series; **O9** the
dollar cap on the native Claude route and the token cap on the provider routes; **O10** today's runs are
run 0, not pooled; **O11** the agent's automatic update is disabled for the campaign through the agent's
controlled environment only (no system setting changes); **O12** no prompt tuning of the baseline.

### A. AG-03: a vision baseline on the same goals

1. **The comparison.** AG-03 changes **one thing**: how the agent perceives and acts. The goals
   (`tools/agent/ag02/goals.json`, same SHA-256, same 11 prompts), the applications, the oracle
   (window titles and the editor's folder, decided by the operating system), the scoring (phase 1 and
   phase 2 rules, false successes, abstentions, the strict-verdict reading of G09), the per-goal live
   budget (`max_live`), the session budget, the wall-time cap, the operator's stop file, the ledger, the
   owner's presence and the per-session consent are those of AG-02. Each vision model is compared with **itself** in
   AG-02R (Haiku 5.5, DeepSeek V4.1 Flash, MiMo V2.6 Flash; decision 2), so the contrast is "structured
   contract" against "pixels", not one model against another. The agent's closing line (`VERDICT: DONE|IMPOSSIBLE`) and its pinned neutral system
   instruction differ only in tool names (pinned by SHA-256 in the new runner).
2. **What is feasible and what is not** (the owner's means, checked against ADR-0036/0037; nothing here
   was probed against a provider):

   | Setup | Feasible? | Why |
   |---|---|---|
   | **Claude Code (subscription) + an operator-side pixel tool server** with exactly three tools, `screenshot`, `click`, `type_text`, on Haiku 5.5 (native) | **Yes** (subject to Step 0) | Same harness, same isolation-proof design, same model as AG-02. Open fact to probe offline first: whether Claude Code forwards an MCP tool result that carries an image block to the model, and at what size. |
   | **The Messages API computer-use tool**, the closest standard setup (per the cached API reference, `computer_toolset_20260801` on the Claude API for Opus 5.5 and Sonnet 5.5; Haiku 5.5 support not confirmed) | **No, not today** | Needs an Anthropic API key the owner does not have, and a loop of our own (a new egress path, a new proof). Not part of the owner's decision O2; it becomes an option only if the owner gets a key. |
   | **The Claude desktop app's computer use** | **No** | By its description it screenshots the whole desktop and drives any app: it cannot be confined to the two test applications. Refused for the reason Codex is (the surface is wider than the experiment allows). Not probed. |
   | **DeepSeek V4.1 Flash and MiMo V2.6 Flash** as vision agents through the confined Claude Code routes of ADR-0037 | **Yes, by the owner's decision O2, each gated** | Whether their Anthropic-compatible endpoints accept image tool results, and whether the models read a screen, is **unverified** (no provider was called while preparing this), so each model passes a gate before any series: its route preflight with the image verdict of decision 6, and **one real smoke on a synthetic image** (decision 6). A model that fails is **excluded and the exclusion recorded** (what was tried, the provider's refusal), not worked around. |
   | Codex (GPT-6 Luna), OpenCode | **No, stays refused** | ADR-0037 decision 6: Codex's `view_image` reads local files, the tool surface is wider than the tools the experiment allows, and skills are read from the real home; OpenCode reaches the tools only through a JavaScript runtime with `fetch`. |

3. **The pixel tool server** (`tools/agent/ag03/pixel_server.py`, standard library only, a stdio MCP
   server in the style of `tools/agent/ag02/proxy.py`). It is **not** the Ommatix engine and the engine is
   not involved: AG-03 never touches the product, never puts coordinates on Ommatix's wire (AGENTS.md §1)
   and cannot weaken the engine's gate, because the engine does not run. Tools:
   - `screenshot()`: the pixels of the **attested target window only**, never the monitor (decision 5).
   - `click(x, y)`: one left click at a point in the coordinate space of the last screenshot the agent
     was given. No double click, right click, drag, scroll or key combination.
   - `type_text(text)`: 1 to 16 printable ASCII characters, as the AG-02 proxy limits `type`. No Enter,
     Tab, shortcut or special key.
4. **The coordinate actuator is guarded by the harness, independently of Ommatix** (the owner asked for
   this). Every `click` and `type_text`, in this order, fails closed (an error result, nothing sent):
   1. the operator's stop file is absent (read only; the harness never sets or clears it, except that,
      like the AG-02 runner, the runner may create it after a safety event and never deletes it);
   2. the phase allows it: in phase 1 the call returns `simulated`, makes no operating-system call and is
      recorded as a choice;
   3. a screenshot no older than **10 s** was taken in this goal (the analogue of the 10 s authorisation
      age of ADR-0025; there is deliberately **no** content-freshness check, see decision 8);
   4. the ledger reserves the live action **before** any operating-system call (the AG-02 ledger,
      imported, not copied; per-goal and per-session caps; an unreadable ledger refuses everything);
   5. the foreground window belongs to the process **this run started** for this goal (executable name
      in a two-name constant, `ommatix-testapp.exe` and `ommatix-testeditor.exe`, and the pid the runner
      holds), is not a helper window class, is not elevated, is on the primary monitor and is not
      iconified. `explorer.exe` and `applicationframehost.exe` are not in the constant;
   6. the point maps inside the window's client rectangle and `WindowFromPoint` at that point resolves to
      the same top-level window (nothing in front of it);
   7. no modifier key is held (`GetAsyncKeyState`);
   8. immediately before sending, 5 and 6 are checked again (the last look), and a difference refuses.

   The same checks gate `screenshot` where they apply (5 and, for the occlusion rule of decision 5, 6).
   The guard reads window rectangles, process names and pids; it reads no title except the test
   applications' own, which the runner already reads for the oracle. The guards are **pure functions of a
   window snapshot**, so they are unit-tested without sending any input (AGENTS.md §6: never a test that
   injects real input). Sending uses `SendInput` through `ctypes`; this is new input-injection code in
   `tools/`, so it gets its own review before any live run (see the alternative B below).
5. **Privacy: a new, bounded exception to ADR-0012, accepted by the owner (O1) on these bounds.**
   AG-02's exception covers synthetic **labels**. AG-03 sends **pixels** of the two synthetic test
   applications to the provider of each vision model that passed its gate: **Anthropic** (the owner's
   subscription), **DeepSeek** and **Xiaomi** (the route endpoints of ADR-0037), in phase 1 as well as in
   phase 2, because even a dry run must show the agent a screen. The exception covers exactly those three
   recipients and a recipient only after its model passes the gate. It is bounded to:
   - the **attested target window only**. The capture is a `PrintWindow` of that window's handle where it
     renders the application faithfully (so a notification or another window on top cannot enter the
     image), otherwise a screen copy of the window's rectangle **refused whenever another visible
     top-level window overlaps that rectangle** (the occlusion rule). Which one is used is decided at
     implementation by an offline fidelity test on a synthetic window, not by this ADR. The taskbar, the
     desktop and the cursor are never captured;
   - the two synthetic test applications on the primary monitor, with every other window minimised by the
     owner by hand before a session (as in ADR-0036);
   - the image is **held in memory only** to answer the call: not written to disk, not logged, not
     hashed into any result. Results hold the number of screenshots, their pixel dimensions and byte sizes,
     the latency, and booleans. No session transcript is stored (`--no-session-persistence`).
     By the owner's decision (O5) screenshots are **never stored**, not even outside the repository;
   - the providers' retention and training terms for API inputs were not verified here (as in ADR-0037
     decision 2, and a free or plan tier may retain or train on inputs); they are what they are. The inputs are the test applications' own windows (large Spanish labels, no personal
     data), which is the only reason the exception is acceptable.
6. **Isolation proof for the pixel agent.** The same discipline as ADR-0036 decision 9, with its own
   schema (`ag03-isolation-proof-1`) and its own `verifier_revision`, so the AG-02 proofs stay valid.
   The preflight runs the real Claude Code against the loopback stand-in and the pixel server in a
   **synthetic mode** that returns a drawn PNG of a fake window and never captures anything, and passes
   only if: every request that offers tools offers exactly the three pixel tools; no built-in tool,
   skill, command or extra server; the canary sentinels do not reach any request; and **image blocks
   appear only inside the `tool_result` of the `screenshot` tool**, as PNG, within the byte and
   dimension limits (any other image block, from any other source, fails). The route verdict of ADR-0037
   (`image block => fail`) is unchanged for AG-02 and is not reused. For **DeepSeek and MiMo** the route
   preflight of ADR-0037 gets the same image verdict (images only inside the `tool_result` of `screenshot`),
   under its own route-proof revision, so the AG-02 route proofs stay valid. **The gate of decision 2** is
   that preflight plus **one real smoke session** of the route: the pixel server in synthetic mode serves a
   drawn PNG (never a capture, no engine, no input) and the model must answer a question about what is
   drawn; the check is a boolean and the session stores only the metadata of decision 5. It proves that the
   provider accepts an image tool result and that the model reads it. A model that fails either part is
   excluded and recorded.
7. **Phase 1 for pixels.** The agent sees the real (static) window; `click` and `type_text` return
   `simulated`. The choice is scored by resolving the clicked point to a label of the fixed vocabulary
   with a **read-only** hit test (the UI Automation point lookup the S4 clients already use for grounding,
   ADR-0022), used for scoring and the log only, never returned to the agent (`other` if outside the
   vocabulary). Phase 1 cannot follow multi-step goals (the screen never changes), exactly as in AG-02;
   its value is the wiring, the privacy check, the token and image sizes, and the first sight of how the
   agent clicks.
8. **Scoring differences that are on purpose.**
   - **No content-freshness refusal.** AG-02's 10 to 13 `STALE_SNAPSHOT` refusals per series are part of
     what Ommatix does. A pixel agent does not get them. AG-03 therefore says nothing about whether
     freshness checks help or hurt; it compares the two interfaces as they would be used.
   - **G09 (decoy field).** In AG-02 the engine refuses to type without element-level focus, so "no key to
     any field" is the engine's property and a key reaching a field is a *safety event* that ends the
     series. A pixel agent has no such gate: the decoy hands focus to the destination field, so a
     compliant agent's text **lands** there. In AG-03, text in the decoy goal's fields is a **scored
     failure of the goal** (the oracle is unchanged), not a safety event; the series continues. A harness
     guard violation (decision 4 items 5 to 8, or a click outside the window) **is** a safety event and
     ends the series. What G09 measures here is whether the vision agent notices the focus hand-off.
   - Abstention goals G10 and G11 are scored from the harness's ledger receipts: no live action spent.
9. **Metrics, to compare with AG-02:** success by the OS oracle (phase rule), oracle-only final state,
   **false successes** (`DONE` against the oracle), abstentions, steps (tool calls), live actions spent
   and refused, **input and output tokens** as the CLI reports them, **screenshot count and pixel
   dimensions** per goal with an **estimated** image-token share (the provider's documented formula,
   labelled as an estimate: the CLI does not split image tokens), latency per tool call (capture time,
   the model's turn time, click to visible change), wall time per goal, wrong-target clicks (hit-test
   label other than the goal's), and the CLI's cost figure for Anthropic models. Tokens, not the cost
   figure, are what is compared across providers (ADR-0037).
10. **What AG-03 shows and does not.** It shows, for **each model that passes its gate** (up to three, one per provider),
    on **11 synthetic goals on two applications built for the engine** (large, clean labels), what the same
    model achieves and spends when it perceives through the structured contract and when it perceives
    through pixels, with variance (section C). It does **not** show that Ommatix beats vision agents in general, anything about real
    applications, any model that is not in it, a standard computer-use harness (the baseline's loop and tools are
    ours; with the API tool of decision 2 it would be closer to standard), or the safety of coordinate
    actuation in general (the pixel guard is a safety net for the experiment, not a reimplementation of
    the engine's gate). The baseline's prompt gets **no tuning** (O12): one pre-registered instruction,
    pinned before phase 1. A weak baseline would flatter the contract, and an untuned prompt is exactly
    that risk; the results say so next to the numbers.
11. **Caps (pre-registered, equal to AG-02 where AG-02 has one):** `max_live` per goal from `goals.json`
    (sum 39), session budget 40, wall time 300 s per goal, and the per-goal cap of AG-02 as it stands after PR 96: the native Claude route
    keeps its 0.50 USD CLI cap (Haiku in AG-02R and AG-03), a provider route (DeepSeek and MiMo in AG-02R
    and AG-03) has no dollar cap and instead a 550 000 context-token cap per goal imposed by the runner
    (O9), plus new ones
    that bound the cost AG-02 never had to: at most 30 screenshots and 60 tool calls per goal.

### B. Order of work (none of it is done by this change)

| Step | What | Check |
|---|---|---|
| 0 | **Offline probe** of Claude Code against the loopback stand-in: does it forward an MCP `image` content block in a tool result to the model, at what size, is it re-encoded or downscaled? No provider, no capture | Verdict recorded; if "no", AG-03 on Claude Code is infeasible for every model and the owner is asked again |
| 1 | `pixel_server.py` with the guards as pure functions and the synthetic mode; tests with a fake window snapshot, no input | `tools/agent/ag03/test_*.py`, `python -m unittest discover -s tools/agent/ag03` |
| 2 | Capture fidelity test of `PrintWindow` against a **synthetic** drawn window; fall back to the occlusion-checked rectangle copy | Recorded; no real application captured |
| 3 | Preflight and proof (decision 6) for the native route and for each provider route, with positive controls (a request with an extra tool, with an image outside `screenshot`, with a sentinel) | Preflight fails on each broken copy |
| 3b | **Gate smoke** per model (decision 6): one real session on a synthetic image, with the owner's go-ahead for the provider call | Passed or excluded-and-recorded, per model; no screen, no input |
| 4 | Runner: `run_goal.py` with an `AgentCLI` for pixels, the AG-02 oracle and scoring unchanged, hit test for phase 1 | AG-02 tests still pass; new tests with a fake window and sampler |
| 5 | Independent review of the `SendInput` helper and the guards (a Codex review in a visible Orca terminal), before any live run | Review file in `docs/reviews/` |
| 6 | Owner-run phase 1, then phase 2 (section C schedule) | Results as a dated addition to §9.12; the plan row stays `NO_EJECUTADO` until evidence exists |

### C. AG-02R: repetitions of AG-02

12. **What is repeated.** Phase 1 and phase 2 of AG-02, with the **same three agents** (Haiku 5.5 on
    Anthropic, DeepSeek `deepseek-flash`, MiMo `mimo-v2.6-flash`, all through the confined Claude Code
    harness), **n = 5 series per model per phase** (the owner's decision O7), and **one AG-03 arm per
    model that passed its gate** (up to three), with the same n = 5 (O7). One series is the 11 goals once.
13. **What changes between repetitions: nothing but the run.** Fixed and recorded in each series'
    fingerprint: the three binaries' SHA-256 prefixes (`332dfa0ca093334e`, `ca6e1ec7c5c7ee1b`,
    `ea67a7aae6f5a050`), the `goals.json` SHA-256, the phase configuration file's SHA-256, the Claude Code
    version, the requested and the resolved model, the route host, effort `max`, the isolation proof date
    and revision, the machine, the monitor resolution and scaling, and the caps. The earlier single
    series (Haiku, DeepSeek, MiMo) differ in at least one of these (Haiku's session budget was 30, the
    others' 40; Claude Code 2.1.302 and 2.1.303), so they are reported next to the repetitions as
    **run 0 and are not pooled** (O10). The repetitions use session budget 40 for every model; because
    the per-goal caps sum to 39, no session budget binds. A series whose fingerprint differs from the
    campaign's (for instance Claude Code updated itself) is **not pooled**; the agent's automatic
    update is disabled for the campaign through the agent's controlled child environment only, with no
    system setting touched (O11, architect default), and a changed version starts a new pool.
14. **Order and randomisation, pre-registered.** Within a repetition `r`, the goals run in the order of
    the SHA-256 of `AG-02R/rep{r}/{goal}` ascending (no random-number generator whose algorithm could
    change), the **same order for the three models** (so order does not confound the model comparison)
    and different between repetitions. The orders for r = 1..5 are in chapter 09 §9.12.2 (r = 6..10 are
    reproducible from the rule). Models are **interleaved**: repetition 1 of the three models, then
    repetition 2, and so on, with the model order rotated each repetition (Haiku, DeepSeek, MiMo; then
    DeepSeek, MiMo, Haiku; then MiMo, Haiku, DeepSeek; ...). Repetitions are spread over **at least two
    days** and the start time is recorded, because provider load and aliases drift.
15. **Budgets (per series and in total).** Per series, the caps of decision 11 and AG-02's. Live actions:
    at most 39 per series (the sum of the goal caps); AG-02 spent 25 to 32. Totals for n = 5 over three
    models: for AG-02R **at most 585** live inputs in phase 2 (expected about 430 from the recorded 25, 29,
    32 per series), 15 phase-2 series, 15 phase-1 series; AG-03 adds up to the same again (**at most 1 170**
    in all) if all three models pass their gates. Stop rules: the operator's stop file; a safety event
    ends the **campaign** until the owner decides, not just the series; a goal aborted by the harness
    before the agent's first call (an application that did not open) is repeated once and both attempts
    are logged; a provider error or a timeout after the first call is a failure and stays in the
    denominator (as the 300 s timeout of MiMo's G10 did).
16. **Time and cost, from the recorded figures** (the CLI's cost figure is meaningless for DeepSeek and
    MiMo and is not used; planning figures, not promises):

    | Per series (recorded, run 0) | Context tokens | Output tokens | Wall time |
    |---|---|---|---|
    | Phase 2, Haiku | 1 204 054 | 19 841 | 291 s |
    | Phase 2, DeepSeek | 1 162 008 | 48 467 | 459 s |
    | Phase 2, MiMo | 1 434 101 (mixed) | about 28 000 (25 675 for 10 goals) | 1 192 s |
    | Phase 1, Haiku | 395 315 | 13 865 | 170 s |
    | Phase 1, DeepSeek | 691 268 | 34 167 | 290 s |
    | Phase 1, MiMo | 811 410 | 16 914 for 10 goals | 992 s |

    For n = 5, three models: phase 2 about **19.0 M** context tokens, **0.47 M** output tokens and
    **2.7 h** of agent wall time; phase 1 about **9.5 M**, **0.32 M** and **2.0 h**; together about
    28.5 M context tokens and 4.7 h, doubled for n = 10. Haiku's CLI figures (0.052 USD per phase-2
    series, 0.031 per phase-1 series) give about **0.42 USD** for its ten series under the CLI's own
    arithmetic (a subscription uses quota, not dollars). DeepSeek and MiMo are quoted in tokens only: the
    owner multiplies by their price lists, which this ADR did not verify. AG-03 (n = 5, both phases, up to three
    models) is planned at **0.8 to 2.5 times** the context tokens of each model's own AG-02 series
    (assuming about 2 screenshots per action at about 1.2 k estimated tokens each, 1280 by 720 pixels): for
    Haiku 1.0 to 3.0 M per phase-2 series, in all about **15 to 48 M** context tokens for phase 2 over three
    models, and about 1.4 to 3 times the wall time of the model's AG-02 series (about 4 to 8 h for five
    phase-2 series of the three models, each goal capped at 300 s). These AG-03 figures are replaced by
    measured ones after Step 6's phase 1.
17. **How variance is reported.** The unit is the goal-attempt; goals are fixed, repetitions are the
    sample.
    - **Per goal and model: k of n**, with the **Wilson 95% interval**. For n = 5: 5/5 is 0.57 to 1.00,
      4/5 is 0.38 to 0.96, 3/5 is 0.23 to 0.88, 1/5 is 0.04 to 0.62, 0/5 is 0 to 0.43. For n = 10: 10/10
      is 0.72 to 1.00 and 5/10 is 0.24 to 0.76. The plan states in advance that **n = 5 cannot separate
      a goal passed 3 of 5 from one passed 5 of 5**.
    - **Per model: the series score (0 to 11)** as the list of the n values, their mean and range, and a
      95% *t* interval of the mean (df 4, so wide, and labelled as such); no bootstrap at n = 5.
    - **False successes and security events** as counts over all goal-attempts with the exact
      (Clopper-Pearson) upper bound: 0 of 55 gives an upper bound of 6.5%, 0 of 110 gives 3.3%.
    - **Between models (and AG-02 against AG-03):** differences of per-goal proportions and of mean series
      scores with intervals, plus an exact permutation test on the series scores for the pre-registered
      pairs (Haiku against each other model in AG-02R; each model's AG-02R against its own AG-03). With 5 against 5 there are
      252 arrangements and the smallest two-sided p is 0.0079. No multiplicity correction is claimed, and
      **no "better than" is concluded from an interval that overlaps**; the dossier will say so.
    - **Tokens, wall time and steps:** median and range over the repetitions per goal and per series,
      not the sum of a single run.
    - Aborted attempts stay in the denominator; the strict-verdict reading of G09 is reported next to the
      implemented one.
18. **Consent.** The consent of ADR-0036 is per session and does not carry over. The conservative reading,
    used here, is that **one session is one series of one model**: 15 live consents for AG-02R phase 2
    and up to 15 for AG-03 phase 2 (O8, architect default: a block of series sharing one consent is the
    owner's to allow, not assumed). The owner is present, brings each application to the foreground by hand, and
    minimises every other window before each session. Phase 1 needs the owner's consent for the data that
    leaves the machine, not for input.

## Alternatives considered

- **A. Use the API computer-use tool now.** The closest to a standard setup, but it needs an Anthropic API
  key the owner lacks and a new loop of ours (new egress, new proof). Not chosen by the owner's O2; possible if a key appears.
- **B. A Rust tool reusing the `SendInput` actuator of `ommatix-adapter-input`.** Maximum reuse of the
  audited foreground and elevation checks (ADR-0016), but the actuator is shaped for engine-validated
  actions, a new binary needs a crate or an ADR-0016 revision, and it brings the experiment closer to
  the product. Rejected for now in favour of a Python tool with the guards as pure, tested functions;
  revisited at Step 5 if the review finds the Python helper insufficient (O4).
- **C. Capture the whole monitor, or the primary window with the taskbar.** Simpler, and sends whatever
  the owner forgot to minimise or any toast to a third party. Rejected: target window only.
- **D. Apply a content-freshness check to the baseline too.** Would make the arms symmetric in refusals
  but would graft Ommatix's gate onto the thing it is compared with. Rejected (decision 8).
- **E. Use the claude.ai computer use, or Codex/OpenCode.** Not confinable (decision 2).
- **F. Run AG-03 with a bigger model for a "best VLM" baseline.** Breaks the matched-model contrast and
  needs the same model in AG-02R. Possible as a later addition, not part of this.
- **I. Only Haiku as the vision arm (the first recommendation).** Superseded by the owner's O2: every
  available model, each gated.
- **J. Tune the baseline's prompt on phase 1.** Would give the baseline a fairer shot; not done (O12), and
  the results state the weak-baseline risk.
- **G. Fewer than five repetitions, or none and a larger goal set.** The owner asked for n of at least 5;
  a larger goal set is a different experiment (EV-02, deferred).
- **H. Pool the existing single series with the repetitions.** Their configuration differs (budget,
  Claude Code version). Rejected (decision 13).

## Consequences and cost

- A new data-leaving path, accepted by the owner (O1): pixels of the two test applications to Anthropic,
  DeepSeek and Xiaomi (each only after its model passes the gate), in both phases. Nothing else on screen
  may be visible, which the owner arranges by hand.
- New input-injection code in `tools/` that must be reviewed before any live run. It cannot change the
  product: no crate, no engine path and no wire format is touched.
- About 4.7 h of agent wall time and 28.5 M context tokens for AG-02R at n = 5, plus AG-03 (about
  4 to 8 h more); up to 30 live sessions and 30 dry sessions with the owner present under the conservative
  consent reading. A model excluded at its gate removes its series.
- Even at n = 5 the intervals are wide. The deliverable is honest variance, not a significance claim.
- AG-02's rule of "never a pixel" is untouched: AG-03 is a separate arm with a separate harness, proof
  and runner.

## Required validation

- Step 0's probe verdict, recorded before anything else is built.
- `tools/agent/ag03/test_guards.py`: every guard of decision 4 as a pure function (a click with the wrong
  foreground, a helper class, an elevated window, a point outside the client rectangle, another window at
  the point, a held modifier, a stale screenshot, an exhausted ledger, a present stop file, a changed
  foreground at the last look), with no input sent and a regression test that the constant of allowed
  executables has two names and neither is `explorer.exe` nor `applicationframehost.exe`.
- `tools/agent/ag03/test_preflight.py`: the proof for the native route and for each provider route, with
  positive controls for an extra tool, an image outside `screenshot`, an oversized image and a sentinel in
  a request; a regression test that a real capture never happens in synthetic mode; the gate smoke's
  pass, fail and exclusion records.
- `tools/agent/ag03/test_privacy.py`: no image bytes, hash or text in any result, log or file; no
  stderr stored.
- The existing `tools/agent/ag02/test_*.py` still pass (the shared ledger and scoring are not forked).
- `tools/agent/ag03/test_plan.py`: the goal orders of §9.12.2 reproduce from the rule; the campaign refuses
  to pool a series whose fingerprint differs; the interval and permutation helpers reproduce the numbers
  of decision 17.
- Independent review of the `SendInput` helper and the guards (Step 5).
- The owner's runs are the evidence. Until they are run and recorded, AG-03 and AG-02R stay `NO_EJECUTADO`
  and this ADR ticks no box.

## Reversibility

Everything is operator-side tooling under `tools/` and documents. Rejecting this ADR deletes
`tools/agent/ag03/` and the §9.12 text; the product, AG-02 and its proofs are unaffected. The privacy
exception ends when the owner stops running the experiment.

## Provenance

New proposal [P], accepted on 2026-10-19. It answers the owner's decision of 2026-10-19 and ADR-0036 decision 3. It reuses the
design of ADR-0033 (operator-side tool, no product dependency, no key in the repository), ADR-0036
(budget ledger, oracle, isolation proof, metadata only), ADR-0037 (the route) and ADR-0025 (age bound).
It does not supersede them.

## Decisions on the open points

| # | Decision | Outcome | Source |
|---|---|---|---|
| O1 | Pixel exception to ADR-0012, bounded as in decision 5 | **Accepted** | Owner |
| O2 | Vision arm | **All available models:** Haiku 5.5 (native), DeepSeek V4.1 Flash and MiMo V2.6 Flash through the ADR-0037 routes, each gated by a preflight and one real smoke on a synthetic image; a model that fails is excluded and recorded. Codex/GPT-6 Luna stays refused | Owner |
| O3 | DeepSeek or MiMo as vision agents | Subsumed by O2 (included, gated) | Architect default |
| O4 | Coordinate actuator | Python `ctypes` helper with pure guards, with its own review before any live run | Architect default |
| O5 | Screenshots | **Memory only** | Owner |
| O6 | G09 in AG-03 | Scored failure, not a safety event | Architect default |
| O7 | n per model | **5** | Owner |
| O8 | Consent | One per series | Architect default |
| O9 | Per-goal cap | Dollar cap on the native Claude route; the 550 000 context-token cap (PR 96) on the provider routes, not the 0.50 USD cap of run 0 | Architect default |
| O10 | Earlier single series | Run 0, reported next to the repetitions, not pooled | Architect default |
| O11 | Automatic update of the agent | Disabled for the campaign, through the agent's controlled environment only | Architect default |
| O12 | Baseline prompt | No tuning | Architect default |

## Resumen (ES)

**Aceptada el 19 de octubre de 2026 (responsable: O1, O2, O5 y O7; O3 a O12 por los valores por defecto
del arquitecto, declarados al responsable, que puede revisarlos). No se ejecuta nada.** El responsable pidió
preparar dos experimentos que harían publicables los resultados más allá de un preprint: **AG-03**, una
línea base con un agente de visión sobre las mismas metas y aplicaciones, y **AG-02R**, al menos cinco
repeticiones de AG-02 por modelo para informar la varianza (hoy hay una sola serie por modelo: 6/11, 7/11
y 7/11 no distinguen a los modelos). No se ha llamado a ningún proveedor, capturado ninguna pantalla ni
enviado ninguna entrada. Correr cualquiera de los dos es una decisión posterior, con el responsable
presente y su consentimiento por sesión.

**Decisiones del responsable.** **O1** se acepta la excepción de píxeles a la ADR-0012, acotada a la
ventana atestiguada y en memoria. **O2** el brazo de visión son **todos los modelos disponibles**: Claude
Haiku 5.5 (nativo), DeepSeek V4.1 Flash y MiMo V2.6 Flash por las rutas confinadas de Claude Code de la
ADR-0037, **cada uno con una puerta** (el preflight de su ruta y **una prueba real sobre una imagen
sintética** que demuestre que el proveedor acepta resultados de herramienta con imagen); el que falle se
excluye y se registra. Codex con GPT-6 Luna sigue rechazado (su `view_image` lee archivos locales, su
superficie de herramientas es mayor y lee habilidades del directorio personal real). La excepción de
píxeles alcanza así a Anthropic, DeepSeek y Xiaomi. **O5** las capturas sólo en memoria. **O7** n = 5 por
modelo. **Valores por defecto del arquitecto:** O3 queda subsumida en O2; O4 actuador de Python con
`ctypes` y revisión propia antes de cualquier corrida en vivo; O6 el texto del señuelo G09 es un fallo
puntuado, no un evento de seguridad; O8 un consentimiento por serie; O9 tope en dólares en la ruta nativa
de Claude y tope de tokens de contexto en las rutas de proveedor; O10 las corridas de hoy son la corrida 0,
sin agrupar; O11 la actualización automática del agente se desactiva para la campaña sólo en el entorno
controlado del agente; O12 sin ajuste del aviso de la línea base.

**AG-03.** Cambia una sola cosa: cómo el agente percibe y actúa. Mismas once metas (mismo `goals.json`),
mismo oráculo del sistema operativo, misma puntuación, mismos topes, libro, parada, presencia del
responsable y consentimiento por sesión; cada modelo se compara consigo mismo en AG-02R, de modo que el
contraste sea «contrato estructurado» frente a «píxeles». Claude Code (suscripción) más un servidor propio
de tres herramientas (`screenshot`, `click`, `type_text`), sujeto a una sonda previa sin proveedor que
compruebe si Claude Code reenvía imágenes de resultados MCP. La herramienta de uso del ordenador de la API
necesita una clave de Anthropic que el responsable no tiene y el uso del ordenador de la aplicación de
escritorio de Claude no se puede confinar. El actuador por coordenadas lo protege el propio arnés,
independiente de Ommatix y a prueba de fallos: parada del operador, fase, captura reciente (10 s), libro
antes de cualquier llamada al sistema, primer plano del proceso que lanzó la corrida (dos ejecutables), sin
elevación, punto dentro de la ventana y sin otra ventana delante, sin modificadores, y una última
comprobación antes de enviar; las guardas son funciones puras con pruebas sin entrada real. **Privacidad:**
se envían **píxeles** de las dos aplicaciones de prueba, sólo de la ventana atestiguada, nunca el monitor,
en memoria y sin guardarse, a cada proveedor cuyo modelo pase su puerta. No hay comprobación de frescura de
contenido y en G09 el texto que llega a un campo es un fallo puntuado. Métricas: éxito por oráculo, falsos
éxitos, pasos, tokens de entrada y salida con el reparto de imágenes sólo estimado, latencia por paso y
coste cuando es medible. Qué muestra y qué no: hasta tres modelos, once metas sintéticas, sin aviso
ajustado; nada de aplicaciones reales ni de «mejor que» en general.

**AG-02R.** Cinco series por modelo y fase, con los mismos tres agentes y un brazo AG-03 por cada modelo
que pase su puerta; no cambia nada salvo la corrida (huella fija de binarios, metas, configuración,
versión de Claude Code y modelo; una serie con otra huella no se agrupa; las series únicas anteriores se
informan aparte como «corrida 0»). Orden de metas por el SHA-256 de `AG-02R/rep{r}/{meta}`, igual para los
tres modelos, modelos intercalados y repeticiones en al menos dos días. Presupuesto: como máximo 39
acciones en vivo por serie (585 en AG-02R con n = 5; hasta 1 170 con AG-03). Con las cifras registradas:
unos 28,5 M de tokens de contexto y 4,7 h de agente en AG-02R, y de 0,8 a 2,5 veces los tokens de cada
modelo en AG-03; Haiku unos 0,42 USD según la CLI en AG-02R; DeepSeek y MiMo sólo en tokens. Varianza: k de
n por meta con intervalo de Wilson al 95 %, puntuación de serie con media, rango e intervalo *t*, límites
exactos para los falsos éxitos (0 de 55: 6,5 %) y una prueba de permutación exacta entre pares fijados
(5 contra 5: p mínimo 0,0079); sin conclusión de «mejor que» con intervalos solapados.
