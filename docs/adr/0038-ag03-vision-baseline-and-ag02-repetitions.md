# 0038. Prepare AG-03 (a vision-agent baseline) and repetitions of AG-02

- **Status:** Accepted
- **Date:** 2026-10-19
- **Approval:** The project owner, 2026-10-19 (O1, O2, O5, O7; O7 revised from n = 5 to n = 6 later the same day; O3-O12 per the architect's defaults stated to the owner; the owner may revise those defaults). The owner asked for both experiments to be **prepared, not run**: running either is a separate, later decision with the owner present and his consent for each session. Acceptance changes no test status or checkbox of chapter 06 §6.7.

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
applications, and **AG-02R**, repetitions of AG-02 per model (n = 5 at first, n = 6 after the owner's revision of O7). This ADR is the design.
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
Xiaomi**. **O5** screenshots are held in memory only. **O7** n = 6 per model (first decided as 5, revised to 6 the same day).

Defaults of the architect, stated to the owner and recorded here as such (the owner may revise them):
**O3** is subsumed by O2; **O4** a Python `ctypes` actuator with its own review before any live run;
**O6** G09 decoy text is a scored failure, not a safety event; **O8** one consent per series; **O9** the
dollar cap on the native Claude route and the token cap on the provider routes; **O10** today's runs are
run 0, not pooled; **O11** the agent's automatic update is disabled for the campaign through the agent's
controlled environment only (no system setting changes); **O12** no prompt tuning of the baseline.

### A. AG-03: a vision baseline on the same goals

1. **The comparison.** AG-03 compares **two complete interfaces**, not one isolated factor. Observation, the action
   primitives (Ommatix's `type` is the focusing click plus the keys in one reserved action, the pixel arm has
   a separate `click` and `type_text`), the freshness gate and the G09 safety classification all differ on
   purpose (decision 8), so the result supports a comparison of the two interfaces as wholes and **no causal
   claim about pixels alone**. What is held fixed is the rest: the goals
   (`tools/agent/ag02/goals.json`, same SHA-256, same 11 prompts), the applications, the oracle
   (window titles and the editor's folder, decided by the operating system), the scoring (phase 1 and
   phase 2 rules, false successes, abstentions, the strict-verdict reading of G09), the per-goal live
   budget (`max_live`), the session budget, the wall-time cap, the operator's stop file, the ledger, the
   owner's presence and the per-session consent. Each vision model is compared with **itself** in
   AG-02R (Haiku 5.5, DeepSeek V4.1 Flash, MiMo V2.6 Flash; decision 2), so the contrast is "structured
   contract" against "pixels", not one model against another.
   The agent's closing line (`VERDICT: DONE|IMPOSSIBLE`) and its pinned neutral system
   instruction differ only in tool names (pinned by SHA-256 in the new runner).
2. **What is feasible and what is not** (the owner's means, checked against ADR-0036/0037; nothing here
   was probed against a provider):

   | Setup | Feasible? | Why |
   |---|---|---|
   | **Claude Code (subscription) + an operator-side pixel tool server** with exactly three tools, `screenshot`, `click`, `type_text`, on Haiku 5.5 (native) | **Yes** (subject to Step 0) | Same harness, same isolation-proof design, same model as AG-02. Open fact to probe offline first: whether Claude Code forwards an MCP tool result that carries an image block to the model, and at what size. |
   | **The Messages API computer-use tool**, the closest standard setup (the public documentation, read on 2026-10-19, lists Haiku 5.5 and Sonnet 5.5 for `computer_toolset_20260801` on the Claude API) | **No, not today** | Needs an Anthropic API key the owner does not have, and a loop of our own (a new egress path, a new proof). Not part of the owner's decision O2; it becomes an option only if the owner gets a key. |
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
   8. **the delivery check**, immediately before each actual input batch and **after** the reservation and
      any blocking work (the ledger lock, the attestation), with a fresh reading of the injected monotonic
      clock and of the operating system: **all of** the stop file (absent), the screenshot age (at most 10 s
      *now*, not when item 3 ran), held modifiers (none) and the window and foreground evidence of
      items 5 and 6. A *batch* is one `SendInput` call: for `click` its move-and-click events, for
      `type_text` all its key events (at most 16) in a single call. The evidence for `click` is item 5 plus
      item 6 at the click's point. `type_text` has no point, so its evidence is item 5 plus the
      **keyboard-focus window**: the focus window of the attested foreground thread (`GetGUIThreadInfo`)
      must be the attested top-level window or one of its descendants, in the same process. That does not say
      which field has focus; that is what G09 measures. Any difference refuses and records the refusal,
      and **the reservation already made is kept** (the budget is spent, as a refused engine action keeps
      its reservation in the AG-02 ledger).

   The same checks gate `screenshot` where they apply (item 5, and the capture conditions of decision 5).
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
   - the **attested target window only, enforced by the capture backend itself**: the returned buffer must
     contain only pixels the target application rendered. **The only admitted backend is an app-rendered
     one**: `PrintWindow` of the window's handle into an app-owned bitmap, where the window renders itself into
     the destination device context (Microsoft's `PrintWindow` contract), so compositor and underlay pixels
     (other windows, the desktop, a translucent or backdrop area) are not copied. It is admitted for the
     campaign only after the Step 2 **fidelity and isolation tests** pass on synthetic windows: an opaque,
     a translucent, a system-backdrop and a rounded-corner target, and a **foreign window that overlaps the
     target only while the buffer is being acquired** (no foreign pixel may appear). The backend that ran is
     part of each series' fingerprint. **A screen copy of the window's rectangle is not admitted, in any form
     and not as a fallback.** A desktop-source copy transfers the source rectangle's pixels and does not make
     a target-owned surface, and endpoint attestations (opacity, z-order and geometry before and after the
     copy) cannot prove what was on screen *during* the copy: a foreign window that appears after the first
     enumeration and hides before the second leaves both reads identical while its pixels are in the buffer.
     If `PrintWindow` is not faithful or not isolated in the Step 2 tests, fails at run time or returns a
     result of the wrong size, the capture is **refused** with `CAPTURE_REFUSED` and **no image is
     returned**; if no admitted backend exists, AG-03 cannot capture and stops. A runtime check cannot prove
     the isolation of an admitted backend, so none is claimed: the guarantee rests on the backend's documented
     behaviour as established by the Step 2 tests. The taskbar, the desktop and the cursor are never
     captured;
   - the two synthetic test applications on the primary monitor, with every other window minimised by the
     owner by hand before a session (as in ADR-0036). This reduces what could appear; it is not the
     guarantee, which is the capture backend above;
   - **local storage** (the owner's O5): the pixel server holds the image **in memory only** to answer the call; it writes nothing to disk, logs nothing and hashes nothing into any result. Results hold the number of screenshots, their pixel dimensions and byte sizes, the latency and booleans, and the session is started with `--no-session-persistence`, the CLI's documented control for session transcripts. Whether the CLI also keeps image copies on this machine under these flags is an open measurement (see the Anthropic reading and Required validation); until it is measured, this bullet's claim covers the pixel server only;
   - **provider retention and training** are a separate matter. This machine does not control them, they
     are part of the data-use scope the owner accepted under O1, and **nothing here deletes a screenshot
     remotely**: once sent, it is held under each recipient's own policy. What the published policies say
     (read on 2026-10-19 from the public pages; summaries, not legal advice):
     - *Anthropic*, Claude Code on the owner's **subscription**. For consumer plans (Free, Pro, Max) the
       data-usage page says the user chooses whether data is used to improve future models; with the
       setting on, retention is 5 years, with it off, 30 days; the setting is changed at
       claude.ai/settings/data-privacy-controls. Commercial and API terms differ (30 days, no training
       by default) and are not the owner's account type as far as is known. **OI-1, answered by the owner on 2026-10-20:** the account is a consumer subscription with "Help improve our
       AI models" **off**, as the owner checked that day (claude.ai → Settings → Privacy). Nothing else is
       known about the account. The reading below is for that case only. It comes from public pages read on
       2026-10-20 with a web-fetch tool that returns verbatim segments, not checked against a browser copy.
       Each source gives its displayed date.
       - *Setting.* The settings article
         (https://privacy.claude.com/en/articles/12109829-how-do-i-change-my-model-improvement-privacy-settings;
         displayed date August 3, 2026; read 2026-10-20) names the setting "Help Improve our AI models". It
         says: "If you decide to turn off the model training setting, we will not use any new chats and coding
         sessions you have with Claude for future model training", and "we will stop using your previously
         stored chats and coding sessions in future model training runs". It also says "Your data will still
         be included in model training that has already started and in models that have already been trained".
         Its safety exception: "If our safety classifiers flag your conversations, they may still be used to
         improve our internal trust and safety models, detect harmful content, enforce our policies, or
         advance our safety research."
       - *Routine retention.* The data-usage page (https://code.claude.com/docs/en/data-usage; no date
         displayed; read 2026-10-20) states "Users who don't allow data use for model improvement: 30-day
         retention period" for Claude Code data. The retention article
         (https://privacy.claude.com/en/articles/10023548-how-long-do-you-store-my-data; displayed date July
         1, 2026; read 2026-10-20) gives 30 days only for deleted conversations ("Deleted from our back-end
         storage systems within 30 days") and no period for ordinary chats that are not deleted. The Privacy
         Policy keeps personal data "for as long as reasonably necessary". The pages do not say how the 30-day
         figures relate.
       - *Flagged content and safety scores.* The retention article says that when a chat or session "is
         flagged by our automated trust and safety systems as violating our Usage Policy", "We retain inputs
         and outputs for up to 2 years and trust and safety classification scores for up to 7 years". The
         training article (https://privacy.claude.com/en/articles/10023580-is-my-data-used-for-model-training;
         displayed date March 16, 2026; read 2026-10-20) says flagged conversations may be used "to improve
         our ability to detect and enforce our Usage Policy, including training models for use by our
         Safeguards team".
       - *Feedback.* The retention article says that for feedback "we retain data associated with that
         submission for 5 years". The training article says feedback is stored "for up to 5 years" and that
         Anthropic may "train our AI models as permitted under applicable laws". The data-usage page says
         transcripts shared with `/feedback`, `/bug` or `/share` "are retained for 5 years".
       - *Legal, disputes, enforcement and anonymized data.* The retention article says: "In all cases, we may
         retain chats and coding sessions as required by law, to resolve disputes, … or as necessary to combat
         violations of our Usage Policy." For anonymized or de-identified data it says "we may retain this
         information for longer". Neither gives a period.
       - *Opt-in programs.* The training article names an explicit opt-in, "By otherwise explicitly opting in
         to training (e.g. joining our Trusted Tester Program)". None is known for this account.
       - *Policies, landing page and Consumer Terms.* The Privacy Policy
         (https://www.anthropic.com/legal/privacy; effective September 10, 2026 as read; read 2026-10-20) says
         Inputs and Outputs may be used to train "unless you opt out through your account settings", and
         points to its privacy center for the detailed periods. Its introduction covers "when you use our
         website, Claude.ai, or other Anthropic products and services" and does not name Claude Code. The
         retention article's scope note does: "This article is about our consumer products such as Claude
         Free, Pro, Max and when accounts from those plans use Claude Code." The Privacy Center landing page
         (https://privacy.claude.com/, reached by redirect from privacy.anthropic.com; read 2026-10-20) lists
         the consumer articles and states no retention or training term itself. The Consumer Terms
         (https://www.anthropic.com/legal/consumer-terms; effective October 8, 2025 as read; read 2026-10-20)
         state no retention period in days or years for consumer data.
       - *Recorded as.* With the setting off, Anthropic states that it will not use new or previously stored
         chats and coding sessions for future model training, subject to the safety exception and to training
         already in progress. The uses the pages still state for this case are safety-flagged conversations
         and the feedback the user gives. Stated periods: 30 days for Claude Code data (data-usage page) and
         for deleted conversations (retention article); up to 2 years for flagged inputs and outputs and 7
         years for classification scores; 5 years for feedback data and for `/feedback`, `/bug` and `/share`
         transcripts; up to 6 months for survey transcripts shared under the separate explicit step. Not
         quantified: retention for legal, dispute and enforcement purposes, anonymized data, and ordinary
         chats that are not deleted ("as long as reasonably necessary"). The setting can be changed at any
         time (retention article: "You can change your privacy settings with respect to model training at any
         time"). This is the owner's state on 2026-10-20; nothing here deletes a screenshot remotely.
       - *Survey transcript upload, and this runner.* The data-usage page separates the rating from an
         optional follow-up: "Nothing is uploaded unless you explicitly select Yes." It also says: "Your
         responses to this survey, including session transcripts submitted after the rating prompt, do not
         impact your data training preferences and cannot be used to train our AI models." The runner
         (`tools/agent/ag03/pixel_agent.py`, lines 84-91) runs `claude -p` with one task prompt on stdin,
         `--no-session-persistence`, `--disable-slash-commands`, `--permission-prompts none` and only the
         three pixel tools, and gives no further input. From that code no upload is initiated by these runs.
         This is an inference from the code, not a measurement, and no environment switch or code change is
         added on that basis.
       - *Local transcript cache.* The CLI reference (https://code.claude.com/docs/en/cli-reference; read
         2026-10-20) says of `--no-session-persistence`: "Disable session persistence so sessions are not
         saved to disk and cannot be resumed. Print mode only." The directory page
         (https://code.claude.com/docs/en/claude-directory; read 2026-10-20) says that the environment
         variable skips writing transcripts and prompt history in any mode, and that in non-interactive mode
         one can "pass --no-session-persistence alongside -p"; it also says that `CLAUDE_CONFIG_DIR` moves
         every `~/.claude` path under the chosen directory. The runner passes the flag and, on the Claude Code
         route, sets `CLAUDE_CONFIG_DIR` to the directory given with `--claude-config-dir` (`pixel_runner.py`, line 752 for the flag and lines 449
         and 464 for the use; `ClaudeCLI` sets `uses_claude_config_dir`, `tools/agent/ag02/agentcli.py`, line 205). The
         same page lists under `projects/<project>/<session>/tool-results/` "full-size copies of images that
         MCP tools return", removed by the cleanup sweep after `cleanupPeriodDays` ("The default is 30 days
         and the minimum is 1"). Neither page says whether the flag keeps those copies out of that folder. The
         design therefore does not claim that every CLI cache or log is absent. The on-disk result of the
         pinned CLI with these flags is an open measurement (see Required validation); until it is measured,
         "not written to disk" describes the pixel server only.
     - *DeepSeek*, API key. Read on 2026-10-19 (public pages, no key, no model call). The **Open Platform
       Terms of Service**
       (https://cdn.deepseek.com/policies/en-US/deepseek-open-platform-terms-of-service.html; released
       2026-04-22, effective 2026-04-29) say the customer keeps its rights in Inputs, assign Outputs to the
       customer and allow using Inputs and Outputs to train other models. They are **silent** on whether
       DeepSeek trains on API inputs, give **no retention period and no deletion period** for API inputs and outputs (section 3.3 only requires
       the developer to answer end-user requests such as erasure and points to a DeepSeek contact under
       section 11) and **no processing location**, and refer to the DeepSeek Privacy Policy
       (https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html; last update 2026-02-10), which
       says personal data including inputs may be used to train its models, with a right to opt out by
       e-mail (privacy@deepseek.com), keeps account data while the account exists, stores data in the
       People's Republic of China, and does not address API inputs specifically. Recorded as: **training
       use of API inputs not stated by the terms and possible under the privacy policy (treated as
       possible); retention period not stated (unknown); location the PRC per the privacy policy.**
     - *Xiaomi*, MiMo Token Plan key. Read on 2026-10-19 (public pages, no key, no model call). Xiaomi
       publishes **two platform privacy policies with different scopes**. The **mainland-China** policy
       (https://privacy.mi.com/XiaomiMiMoPlatform/en_GB/; effective 2026-10-10) says personal information is
       stored within the People's Republic of China, but that statement applies to use within the PRC, and
       the policy says use outside the PRC is governed by the privacy terms published for services outside
       it; it says Xiaomi will not use the text content provided for model training or any other purpose
       without prior consent and gives no retention period beyond "the period necessary" (the English
       locale does not make it the overseas policy). The **overseas** policy
       (https://privacy.mi.com/XiaomiMiMoPlatformos/en_GB/; updated 2026-06-25), issued by Xiaomi Technology
       Netherlands B.V. and Xiaomi Technologies Singapore Pte. Ltd. with their affiliates for the MiMo API
       open platform and tools such as MiMo Code, says the content the user provides, images included, is
       used to process requests and generate output and that **Xiaomi will not use it for model training or
       any other purposes**; it gives **no retention day count** (personal information is kept for the period
       needed for the stated purposes or required by law and is deleted or anonymised once the purpose is
       fulfilled); and it says data is hosted in the Netherlands and Singapore (for users in the EEA, the
       UK and Switzerland, stored in the Netherlands with possible transfer to Singapore; for other users,
       stored in Singapore with possible transfer to the Netherlands). **Onward transfers are therefore
       possible, and no promise that none occurs is recorded.** It also lists sharing with a payment
       partner (Waffo Hong Kong Limited) for recharge and refunds and with Google for online search (the
       query and the IP address). The platform's own privacy entry (https://mimo.mi.com/docs/terms/privacy-policy)
       is reported by the independent review of PR 97 to route international users to the overseas policy
       and mainland users to the mainland one; this author could not retrieve that page, which is rendered
       in the browser. **Neither policy names the Token Plan.** The **Token Plan page**
       (https://mimo.mi.com/docs/en-US/tokenplan/Token%20Plan/subscription) says nothing about data
       handling, retention, training or location; it links the Terms of Service (User Agreement) and the
       Privacy Policy and limits the plan quota to programming tools. The **Open Platform Service
       Agreement** (https://mimo.mi.com/docs/terms/user-agreement; updated 2026-07-07 as the retrieved
       excerpts report) is a page rendered in the browser whose full text could not be fetched, so only
       excerpts found by search are recorded: Xiaomi is the processor and the developer the controller,
       data processed through the services is the customer's business data, Xiaomi must stop processing or
       delete personal information within a "reasonable period" (no day count) after the service ends, the
       purpose is complete or the customer asks, and for services outside mainland China data is stored on
       servers in Europe and Singapore; the excerpts do not address training. A separate policy for the
       MiMo desktop product (https://mimo.xiaomi.com/legal/privacy-policy; updated 2026-09-16) says inputs
       and outputs may be used for training with an in-app opt-out and storage in Singapore; it covers the
       desktop product, not the API. The three API-platform documents (overseas policy, mainland policy,
       agreement excerpts) are consistent once their scopes are read: overseas use is hosted in the
       Netherlands and Singapore, the PRC storage statement is for use within the PRC. Recorded as:
       **training: Xiaomi states it does not use submitted content for model training (overseas policy; the
       mainland policy requires prior consent); retention: purpose- and law-based, no day count (the
       number of days is unknown); location: Netherlands and Singapore with possible onward transfers
       (overseas policy and agreement excerpts); applicability to this owner's Token Plan account (which
       region the account is under and which policy the platform applies to a Token Plan key) is not
       established: unknown.**

     These readings, with their URLs and the date read, are a **precondition of each provider's gate
     smoke**, and are repeated if a page's date changes before the smoke; the Claude gate smoke also needs
     OI-1 recorded, which the owner did on 2026-10-20 (above). Where a page is silent the entry stays *unknown*, inside the scope accepted under O1.

     The inputs are the test applications' own windows (large Spanish labels, no personal data), which is
     the only reason the exception is acceptable whatever a recipient retains.
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
   that preflight plus **one real smoke session** of the route: the pixel server in synthetic mode serves
   **several contrasting** drawn PNGs (different drawn words and shapes, with a question that changes with
   the image, so a guess is unlikely to pass; never a capture, no engine, no input) and the model must
   answer each; the check is a boolean per image and the session stores only the metadata of decision 5. It
   establishes transport and a few image-dependent answers, **not competence at GUI tasks**. A model that
   fails either part is excluded and recorded.
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
   **false successes** (`DONE` against the oracle), abstentions, steps (tool calls) **and primitive input
   attempts** (clicks and `type_text` calls; Ommatix's `type` is reported both as one action and as its
   primitives), live actions spent and refused, **input and output tokens** as the CLI reports them with
   AG-02's definition of context (fresh input + cache creation + cache read), **screenshot count and pixel
   dimensions** per goal, and an **estimated** image-token share: it counts every appearance of an image in
   the request history retained across turns, not only the unique screenshot calls, and uses the
   recipient's documented formula (for Anthropic, ceil(width/28) times ceil(height/28), 1 196 for
   1280 by 720); for a provider with no documented formula the share is reported as **unavailable**. No
   pixels are stored for this. Also latency per tool call (capture time, the model's turn time, click to
   visible change), wall time per goal, wrong-target clicks (hit-test label other than the goal's) and the
   CLI's cost figure for Anthropic models. Tokens, not the cost figure, are compared across providers
   (ADR-0037). The G09 scoring difference of decision 8 is shown next to any safety-event comparison.
10. **What AG-03 shows and does not.** It shows, for **each model that passes its gate** (up to three, one per provider),
    on **11 synthetic goals on two applications built for the engine** (large, clean labels), what the same
    model achieves and spends when it perceives through the structured contract and when it perceives
    through pixels, with variance (section C). It supports a comparison of complete interfaces, not a causal claim about pixels alone
    (decision 1). It does **not** show that Ommatix beats vision agents in general, anything about real
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
| 2 | Fidelity and isolation tests of the app-rendered backend (decision 5) against **synthetic** windows: opaque, translucent, system-backdrop, rounded-corner, and a foreign window present only during acquisition with identical states before and after; admit `PrintWindow` only if all pass, otherwise AG-03 cannot capture and stops | Recorded; no real application captured |
| 3 | Preflight and proof (decision 6) for the native route and for each provider route, with positive controls (a request with an extra tool, with an image outside `screenshot`, with a sentinel) | Preflight fails on each broken copy |
| 3b | **Gate smoke** per model (decision 6): one real session on a synthetic image, with the owner's go-ahead for the provider call | Passed or excluded-and-recorded, per model; no screen, no input |
| 4 | Runner: `run_goal.py` with an `AgentCLI` for pixels, the AG-02 oracle and scoring unchanged, hit test for phase 1 | AG-02 tests still pass; new tests with a fake window and sampler |
| 5 | Independent review of the `SendInput` helper and the guards (a Codex review in a visible Orca terminal), before any live run | Review file in `docs/reviews/` |
| 6 | Owner-run phase 1, then phase 2 (section C schedule) | Results as a dated addition to §9.12; the plan row stays `NO_EJECUTADO` until evidence exists |

### C. AG-02R: repetitions of AG-02

12. **What is repeated, and the blocks.** Phase 1 and phase 2 of AG-02, with the **same three agents** (Haiku
    5.5 on Anthropic, DeepSeek `deepseek-flash`, MiMo `mimo-v2.6-flash`, all through the confined Claude Code
    harness), **n = 6 series per model per phase** (the owner's decision O7, superseding n = 5), and **one AG-03 arm per model
    that passed its gate** (up to three), with the same n = 6. One series is the 11 goals once. The unit of
    randomisation is a **block**: repetition `r` of model `m`, containing its **two arms**, the AG-02R
    series and the AG-03 series of that model (the second only if the model passed its gate). Blocks are
    what the contrasts of decision 17 pair on.
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
    and different between repetitions. The orders for r = 1..6 are in chapter 09 §9.12.2 (any further
    repetition is reproducible from the rule). Blocks are **interleaved** by repetition: all blocks of repetition 1, then
    repetition 2, and so on. Within a repetition the order of the models, and within a block the order of its two
    arms (run back to back), is ascending SHA-256 of `AG-02R/order/rep{r}/{model}` and
    `AG-02R/arm/rep{r}/{model}/{arm}`, pre-registered with the goal orders in chapter 09 §9.12.2. It is a
    randomisation scheme and not a rotation because the exact tests of decision 17 need one. Repetitions are spread over **at least two
    days** and the start time is recorded, because provider load and aliases drift.
15. **Budgets (per series and in total).** Per series, the caps of decision 11 and AG-02's. Live actions:
    at most 39 per series (the sum of the goal caps); AG-02 spent 25 to 32. Totals for n = 6 over three
    models: for AG-02R **at most 702** live inputs in phase 2 (expected about 516 from the recorded 25, 29,
    32 per series), 18 phase-2 series, 18 phase-1 series; AG-03 adds up to the same again (**at most 1 404**
    in all) if all three models pass their gates. Stop rules: the operator's stop file; a safety event ends the **campaign** until the
    owner decides, not just the series. Retries, aborts and what a stopped campaign leaves unattempted are
    decision 19's.
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

    For n = 6, three models: phase 2 about **22.8 M** context tokens, **0.58 M** output tokens (MiMo's full-series figure of about 28 000 used) and
    **3.2 h** of agent wall time; phase 1 about **11.4 M**, **0.39 M** (MiMo with 10 of 11 goals) and **2.4 h**; together about
    34.2 M context tokens and 5.7 h. Haiku's CLI figures (0.052 USD per phase-2
    series, 0.031 per phase-1 series) give about **0.50 USD** for its twelve series under the CLI's own
    arithmetic (a subscription uses quota, not dollars). DeepSeek and MiMo are quoted in tokens only: the
    owner multiplies by their price lists, which this ADR did not verify. AG-03 (n = 6, both phases, up to
    three models) is planned at **0.8 to 2.5 times** the context tokens of each model's own AG-02 series
    (assuming about 2 screenshots per action at about 1.2 k estimated tokens each, 1280 by 720 pixels): for
    Haiku 1.0 to 3.0 M per phase-2 series, in all about **18 to 57 M** context tokens for phase 2 over three
    models, and about 1.4 to 3 times the wall time of the model's AG-02 series (about 4.5 to 9.7 h for six
    phase-2 series of the three models, each goal capped at 300 s). These AG-03 figures are replaced by
    measured ones after Step 6's phase 1.
17. **How variance is reported.** Goals are fixed and repetitions are the sample. Every interval and test
    below has a stated estimand and a stated assumption; none is presented as unconditional.
    - **Per goal, model and arm: k of n** (n is the number of primary slots attempted, decision 19) with the
      **Wilson 95% interval** for that goal's success probability, *assuming the repetitions of a goal are
      independent runs of one process*; provider and day drift threaten that assumption, so the interval is
      descriptive. For n = 6: 6/6 is 0.61 to 1.00, 5/6 is 0.44 to 0.97, 4/6 is 0.30 to 0.90, 3/6 is 0.19 to
      0.81, 2/6 is 0.10 to 0.70, 1/6 is 0.03 to 0.56, 0/6 is 0 to 0.39. The plan states in advance that
      **n = 6 cannot separate a goal passed 2 of 6 from one passed 6 of 6**.
    - **Per model and arm: the series score (0 to 11)** as the list of the six values, their mean and
      range, and a 95% *t* interval of the mean (df 5, so wide, and labelled as such; descriptive).
    - **Contrasts and exact tests, on the pre-registered blocks.** Contrast (i), *arm within a model*:
      `d_r = score(AG-03, r) - score(AG-02R, r)` for the six blocks of one model. Contrast (ii), *model
      within an arm*: Haiku against each other model, `d_r` from the two models' series of repetition `r`
      (their blocks share the repetition's goal order and day window). The estimand is the mean over
      repetitions of the within-block difference. The test is a **paired sign-flip randomisation test**:
      under the null that the two labels are exchangeable inside each block, which the randomised order of
      decision 14 is there to justify, the 2^6 = 64 sign assignments are equally likely, and **the smallest
      attainable two-sided p is 2/64 = 0.03125** (absent ties). So with n = 6 **p < 0.05 is attainable
      only if all six within-block differences have the same sign** (the next most extreme outcome gives
      4/64 = 0.0625); the plan states this up front. Contrasts are reported as paired differences with the
      exact distribution and the *t* interval of the mean difference, as descriptive evidence. The unpaired
      6-against-6 test (924 arrangements) is **not** used, because it ignores the pairing the design builds
      in.
    - **Events (false successes, safety events): counts first, bounds only as conditional models.** The
      event estimand is stated each time. The primary statement is **repetition-level**: the number of the
      six repetitions in which **at least one event occurred in any series run in that repetition** (all
      models and arms), which lets the models and arms of a repetition share dependence (goal order, day
      window, provider load). The exact (Clopper-Pearson, two-sided 95%) upper endpoint for the probability
      that a repetition contains an event is **45.9% for 0 of 6 repetitions**; it is conditional on the
      repetitions being independent draws, which day-to-day drift can violate, and it is a statement about
      "a repetition", not about a model. Per model and arm the counts are reported with the same 0-of-6
      calculation (45.9%) only as a **conditional model illustration**: it assumes that model's six series
      are independent. **The three models are not pooled as 18 independent draws**: 18 series are not 18
      independent trials, so no 0-of-18 bound (18.5%) is claimed. The goal-attempt bound (0 of 66 per
      model and arm gives 5.4%) assumes independent goal-attempts and is reported only **labelled as that
      independent-trial model**. None is unconditional. Two countermodels, exact arithmetic and not results:
      (i) six independent series in which all eleven goals share one event of probability 20% give zero
      events over the 66 goal-attempts with probability 0.8^6 = 26.2%, so 5.4% is not a 95% bound under
      that dependence; (ii) six independent repetitions in which the three models' series of a repetition
      share one event of probability 30% give zero pooled events with probability 0.7^6 = 11.8%, and then
      a pooled 18.5% endpoint would exclude the true 30%.
    - **Tokens, wall time and steps:** median and range over the repetitions per goal and per series, not
      the sum of a single run.
    - The strict-verdict reading of G09 is reported next to the implemented one. Slots, aborts and
      unattempted goals are counted by decision 19.
18. **Consent.** The consent of ADR-0036 is per session and does not carry over. The conservative reading,
    used here, is that **one session is one series of one model**: 18 live consents for AG-02R phase 2
    and up to 18 for AG-03 phase 2 (O8, architect default: a block of series sharing one consent is the
    owner's to allow, not assumed). The owner is present, brings each application to the foreground by hand,
    and minimises every other window before each session. Phase 1 needs the owner's consent for the data
    that leaves the machine, not for input.
19. **The primary slot, retries and stopped campaigns** (fixed before data collection, the same in both
    arms). A **slot** is (model, arm, phase, repetition, goal): 11 per series, 6 series per (model, arm, phase). A slot's **primary outcome** is that of its **first valid attempt**. An attempt is *invalid* only
    if it ended through a harness fault **before the agent's first model call** (an application that did
    not open, a proof that did not load). A capture refusal (`CAPTURE_REFUSED`), a gate refusal or a
    model-gate exclusion is **not** a fault to retry around: it is recorded as it happened (after the first
    model call it is that slot's outcome; a model excluded at its gate removes its slots), and the retry
    grants no bypass of any of them. An invalid attempt is **retried once,
    inside the same slot** (the retry is part of the slot, not a diagnostic run); both records are kept. A
    second invalid attempt makes the slot a *technical failure*. After the agent's first model call nothing
    is retried: a provider error, a timeout, a budget stop or a refusal is that slot's outcome, a failure (as
    MiMo's 300 s timeout in G10 was). **Denominator:** k of n counts primary slots; a slot with an invalid
    first attempt and a valid retry counts once, with the retry's outcome; a technical failure counts as a
    non-success in the denominator and is also reported on its own line. **Raw attempts** (every attempt,
    valid or not) and technical failures are reported separately in every table, beside the primary counts.
    **Campaign stop:** the slots not yet attempted are reported as *not attempted*, are not failures and are
    not in n; a series with any not-attempted slot is *incomplete*, is excluded from the series-score
    contrasts and drops its block from them (the contrast then has fewer than six blocks and says so), and is
    shown separately.

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
- **G. Fewer than six repetitions, or none and a larger goal set.** The owner decided n = 6 (first 5);
  a larger goal set is a different experiment (EV-02, deferred).
- **K. Keep a rectangle screen copy as a fallback, or as a conditionally admitted backend.** Rejected (reviews of PR 97): a rectangle can contain desktop pixels through a translucent, backdrop or rounded area, or through a foreign window present only during the copy, and endpoint attestations cannot rule that out. No rectangle backend is admitted.
- **L. Test the 6-against-6 contrast with the unpaired permutation test (924 arrangements).** Rejected
  (review of PR 97): the design pairs by block, so the exchangeable unit is the within-block label and the
  reference distribution has 64 sign assignments (decision 17).
- **H. Pool the existing single series with the repetitions.** Their configuration differs (budget,
  Claude Code version). Rejected (decision 13).

## Consequences and cost

- A new data-leaving path, accepted by the owner (O1): pixels of the two test applications to Anthropic,
  DeepSeek and Xiaomi (each only after its model passes the gate), in both phases. Nothing else on screen
  may be visible, which the owner arranges by hand.
- New input-injection code in `tools/` that must be reviewed before any live run. It cannot change the
  product: no crate, no engine path and no wire format is touched.
- About 5.7 h of agent wall time and 34.2 M context tokens for AG-02R at n = 6, plus AG-03 (about
  4.5 to 9.7 h more); up to 36 live sessions and 36 dry sessions with the owner present under the conservative
  consent reading. A model excluded at its gate removes its series.
- Even at n = 6 the intervals are wide. The deliverable is honest variance, not a significance claim.
- AG-02's rule of "never a pixel" is untouched: AG-03 is a separate arm with a separate harness, proof
  and runner.

## Required validation

- **On-disk check of the pinned CLI, before the first series (added after the review of PR 103).** With the pinned CLI, the production flags (`--no-session-persistence`, `--strict-mcp-config`, `--tools ""`, the isolated configuration directory), and a synthetic-mode run with no capture and no provider (for example the Step 0 probe, `tools/agent/ag03/probe_image.py`), the isolated directory must be listed afterwards and the result recorded: whether any image bytes or image files appear under `projects/`, including `tool-results/`. If they do, decision 5's statement that the image is not written to disk does not hold for the CLI, and the owner decides the mitigation before any series. This change does not run the check.
- Step 0's probe verdict, recorded before anything else is built.
- `tools/agent/ag03/test_guards.py`: every guard of decision 4 as a pure function (a click with the wrong
  foreground, a helper class, an elevated window, a point outside the client rectangle, another window at
  the point, a held modifier, a stale screenshot, an exhausted ledger, a present stop file, a changed
  foreground at the delivery check), with no input sent and a regression test that the constant of allowed
  executables has two names and neither is `explorer.exe` nor `applicationframehost.exe`. With a **fake
  clock and fake operating-system state**: the trace of the review (screenshot at t = 0, age check at
  9.9 s, ledger wait to 11.9 s, stop engaged at 10 s) must end in a refusal with no input; a modifier
  pressed, a foreground change or a focus window outside the attested one between the initial guard and
  delivery must refuse for `click` and for `type_text`; and in every refusal the reservation is kept.
- `tools/agent/ag03/test_capture.py`: against fake windows and fake backends, a translucent target, a
  system-backdrop target, a rounded-corner target, a failing `PrintWindow`, a result of the wrong size and
  the **sequence in which the states before and after the capture are identical and a foreign window exists
  only during acquisition** each give `CAPTURE_REFUSED` and no image from a desktop-source fake backend
  (which is never admitted); the admission test rejects any backend whose output contains the foreign
  pixels; an app-rendered fake with an isolated output gives the buffer; there is no fallback between
  backends. No real screen is needed.
- `tools/agent/ag03/test_preflight.py`: the proof for the native route and for each provider route, with
  positive controls for an extra tool, an image outside `screenshot`, an oversized image and a sentinel in
  a request; a regression test that a real capture never happens in synthetic mode; the gate smoke's
  pass, fail and exclusion records.
- `tools/agent/ag03/test_privacy.py`: no image bytes, hash or text in any result, log or file; no
  stderr stored.
- The existing `tools/agent/ag02/test_*.py` still pass (the shared ledger and scoring are not forked).
- `tools/agent/ag03/test_plan.py`: the goal, model and arm orders of §9.12.2 reproduce from their rules;
  the campaign refuses to pool a series whose fingerprint differs.
- `tools/agent/ag03/test_stats.py`: the interval helpers reproduce decision 17's numbers; the paired
  sign-flip reference distribution has 64 assignments and a minimum two-sided p of 0.03125 (and 0.0625 for the
  next most extreme outcome); the repetition-level endpoint for 0 of 6 is 45.9% and the helper refuses to
  produce a pooled 0-of-18 bound; and two **counterexamples** show why: a **correlated-series** one (six
  series in which the eleven goals share one event of probability 0.2: zero events over 66 goal-attempts
  has probability 0.8^6) and a **cross-model correlated-repetition** one (six repetitions in which the
  three models' series share one event of probability 0.3: zero pooled events has probability 0.7^6, and a
  pooled 18.5% endpoint would exclude the true 0.3).
- `tools/agent/ag03/test_slots.py`: a fake runner with one pre-call abort and its retry in one slot and a
  later campaign stop asserts the primary denominator (the slot counts once, with the retry's outcome),
  the separate raw-attempt and technical-failure counts, the not-attempted slots and the incomplete series
  dropping out of the series-score contrasts.
- Independent review of the `SendInput` helper and the guards (Step 5).
- The owner's runs are the evidence. Until they are run and recorded, AG-03 and AG-02R stay `NO_EJECUTADO`
  and this ADR ticks no box.

## Reversibility

Everything is operator-side tooling under `tools/` and documents. Rejecting this ADR deletes
`tools/agent/ag03/` and the §9.12 text; the product, AG-02 and its proofs are unaffected. The privacy
exception ends when the owner stops running the experiment. **What was already sent stays under each
recipient's retention policy; stopping, or reverting this ADR, deletes nothing remotely.**

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
| O7 | n per model | **6** (owner, 2026-10-19; supersedes n = 5) | Owner |
| O8 | Consent | One per series | Architect default |
| O9 | Per-goal cap | Dollar cap on the native Claude route; the 550 000 context-token cap (PR 96) on the provider routes, not the 0.50 USD cap of run 0 | Architect default |
| O10 | Earlier single series | Run 0, reported next to the repetitions, not pooled | Architect default |
| O11 | Automatic update of the agent | Disabled for the campaign, through the agent's controlled environment only | Architect default |
| O12 | Baseline prompt | No tuning | Architect default |

Owner items opened by the review of PR 97 and their answers (2026-10-19; OI-1 on 2026-10-20):

- **OI-1** Tell us the Claude account's plan and its model-improvement setting
  (claude.ai/settings/data-privacy-controls). **Answered by the owner on 2026-10-20:** a consumer
  subscription with "Help improve our AI models" off, as checked that day; nothing else is known about the
  account. The reading of the policies for that case is in decision 5. This records OI-1, which the Claude vision
  gate smoke needed; no other decision changes.
- **OI-2** Read the public DeepSeek Open Platform terms and privacy policy and the Xiaomi MiMo Token Plan
  and API platform terms. **Answered yes and done** on 2026-10-19 (public pages only, no key, no model
  call): what they say, with URL and date, is in decision 5, with *unknown* kept where they are silent (the Xiaomi account was corrected after the third review, R6: the mainland and overseas policies have different scopes and do not contradict each other).
  The reading is a precondition of each provider's gate smoke.
- **OI-3** The consequence of n = 5 (no paired contrast could reach p < 0.05). **Answered:** the owner
  changed O7 to n = 6, which this ADR now uses (smallest two-sided p 0.03125).

## Resumen (ES)

**Aceptada el 19 de octubre de 2026 (responsable: O1, O2, O5 y O7, con O7 revisada de n = 5 a n = 6 más
tarde el mismo día; O3 a O12 por los valores por defecto del arquitecto, declarados al responsable, que puede
revisarlos). No se ejecuta nada.** El responsable pidió preparar dos experimentos que harían publicables los
resultados más allá de un preprint: **AG-03**, una línea base con un agente de visión sobre las mismas metas y
aplicaciones, y **AG-02R**, repeticiones de AG-02 por modelo (n = 6) para informar la varianza (hoy hay una
sola serie por modelo: 6/11, 7/11 y 7/11 no distinguen a los modelos). No se ha llamado a ningún proveedor,
capturado ninguna pantalla ni enviado ninguna entrada. Correr cualquiera de los dos es una decisión
posterior, con el responsable presente y su consentimiento por sesión.

**Decisiones del responsable.** **O1** se acepta la excepción de píxeles a la ADR-0012, acotada a la
ventana atestiguada y en memoria. **O2** el brazo de visión son **todos los modelos disponibles**: Claude
Haiku 5.5 (nativo), DeepSeek V4.1 Flash y MiMo V2.6 Flash por las rutas confinadas de Claude Code de la
ADR-0037, **cada uno con una puerta** (el preflight de su ruta y **una prueba real sobre imágenes
sintéticas** que demuestre que el proveedor acepta resultados de herramienta con imagen); el que falle se
excluye y se registra. Codex con GPT-6 Luna sigue rechazado (su `view_image` lee archivos locales, su
superficie de herramientas es mayor y lee habilidades del directorio personal real). La excepción de
píxeles alcanza así a Anthropic, DeepSeek y Xiaomi. **O5** las capturas sólo en memoria. **O7** n = 6 por
modelo (decidida como 5 y revisada a 6 el mismo día). **Valores por defecto del arquitecto:** O3 queda
subsumida en O2; O4 actuador de Python con `ctypes` y revisión propia antes de cualquier corrida en vivo; O6
el texto del señuelo G09 es un fallo puntuado, no un evento de seguridad; O8 un consentimiento por serie; O9
tope en dólares en la ruta nativa de Claude y tope de tokens de contexto en las rutas de proveedor; O10 las
corridas de hoy son la corrida 0, sin agrupar; O11 la actualización automática del agente se desactiva para
la campaña sólo en el entorno controlado del agente; O12 sin ajuste del aviso de la línea base.

**AG-03.** Compara **dos interfaces completas**, no un único factor: la observación, las primitivas de
acción (el `type` de Ommatix es el clic de enfoque más las teclas en una acción reservada; el brazo de
píxeles tiene `click` y `type_text` separados), la puerta de frescura y la clasificación de seguridad de
G09 difieren a propósito, de modo que el resultado no sostiene una afirmación causal sobre los píxeles
solos. Se mantienen iguales las once metas (mismo `goals.json`), el oráculo del sistema operativo, la
puntuación, los topes, el libro, la parada, la presencia del responsable y el consentimiento por sesión; cada
modelo se compara consigo mismo en AG-02R. Claude Code (suscripción) más un servidor propio de tres
herramientas (`screenshot`, `click`, `type_text`), sujeto a una sonda previa sin proveedor que compruebe si
Claude Code reenvía imágenes de resultados MCP. La herramienta de uso del ordenador de la API figura hoy en
la documentación pública con Haiku 5.5 y Sonnet 5.5, pero necesita una clave de Anthropic que el
responsable no tiene; el uso del ordenador de la aplicación de escritorio de Claude no se puede confinar.
**Frontera de píxeles (revisión del PR 97, R1):** la garantía es del propio motor de captura y **el único
motor admitido es uno que dibuja la aplicación**: `PrintWindow` de la ventana, que se dibuja a sí misma en el
contexto de destino, sin píxeles del compositor ni de debajo. Se admite para la campaña sólo tras unas
pruebas de fidelidad y aislamiento con ventanas sintéticas (opaca, translúcida, con fondo del sistema, de
esquinas redondeadas y una ventana ajena que solapa el objetivo **sólo mientras se adquiere el buffer**, sin
que aparezca ningún píxel ajeno). **No se admite ninguna copia de pantalla del rectángulo de la ventana, ni
como alternativa:** una copia del escritorio transfiere los píxeles del origen y las atestaciones de los
extremos no prueban lo que había en pantalla *durante* la copia (una ventana ajena que aparece tras la primera
enumeración y se oculta antes de la segunda deja idénticos los dos extremos con sus píxeles dentro del
buffer). Si `PrintWindow` no es fiel o no está aislado, falla en ejecución o devuelve un tamaño inesperado,
**se rechaza la captura** (`CAPTURE_REFUSED`) y no se devuelve ninguna imagen; si no hay motor admitido,
AG-03 no puede capturar y se detiene. Una comprobación en ejecución no puede probar el aislamiento de un
motor admitido, y no se afirma ninguna: la garantía descansa en lo que las pruebas establecen. **Entrega
(R2):** tras la reserva y cualquier espera, justo antes de cada lote de entrada se vuelven a comprobar la
parada del operador, la edad de la captura (10 s), los modificadores y la evidencia de ventana y primer
plano; para `type_text`, que no tiene punto, la ventana con el foco del teclado debe pertenecer a la ventana
atestiguada. La reserva ya hecha se conserva si rechaza. El resto de guardas del actuador por coordenadas
son las del diseño original (parada, fase, libro antes de cualquier llamada, primer plano del proceso
lanzado, sin elevación, punto dentro de la ventana, sin modificadores). **Privacidad (R3):** el servidor de píxeles mantiene la imagen **sólo en memoria** para responder a la llamada: no la escribe en disco, no la registra ni la somete a hash en ningún resultado. Si la propia CLI guarda copias de la imagen en este equipo con esas marcas (`--no-session-persistence` y el directorio de configuración aislado), es una medición abierta: quedan **pendientes de la comprobación en disco** antes de cualquier serie (decisión 5 y validación requerida); hasta medirla, esta afirmación cubre sólo al servidor de píxeles. La retención y el entrenamiento del proveedor, que esta máquina no controla y nada borra a distancia, son otra cuestión y forman parte del alcance de uso de datos que el responsable aceptó en O1. Anthropic (Claude Code con suscripción, planes de consumo): con la
mejora de modelos activada, retención de 5 años; sin ella, 30 días; **OI-1, respondida por el responsable el 20 de octubre de 2026**: cuenta de suscripción de consumo con el ajuste de mejora de modelos desactivado («Help Improve our AI models», comprobado ese día); nada más se conoce de la cuenta. Lectura de las páginas públicas del 20 de octubre de 2026, con URL y fecha en la decisión 5: con el ajuste desactivado, Anthropic dice que no usará para entrenamientos futuros las conversaciones y sesiones de código nuevas ni las guardadas, con excepciones: el material marcado para revisión de seguridad y el que el usuario comunica (por ejemplo, con la retroalimentación); el entrenamiento ya en curso y los modelos ya entrenados no se ven afectados. Plazos declarados: 30 días para los datos de Claude Code según la página de uso de datos (el artículo de retención da 30 días sólo para las conversaciones borradas; las páginas no dicen cómo encajan); hasta 2 años para entradas y salidas marcadas y 7 años para las puntuaciones de clasificación de seguridad; 5 años para la retroalimentación y para las transcripciones de `/feedback`, `/bug` y `/share`; hasta 6 meses para transcripciones de encuesta compartidas tras un consentimiento explícito. Sin plazo declarado: la retención por ley, disputas o cumplimiento, y los datos anonimizados. La subida de la encuesta y la caché local se tratan en la decisión 5; queda abierta la medición de si la CLI guarda copias de imágenes en `tool-results/` con esas marcas. El ajuste puede cambiarse en cualquier momento, y este registro describe el estado del 20 de octubre de 2026. DeepSeek (leído el 19 de octubre de 2026): los términos de la
plataforma abierta (vigentes desde el 29 de abril de 2026) dejan al cliente sus derechos sobre las entradas y
permiten entrenar otros modelos con ellas, pero callan si DeepSeek entrena con las entradas de la API y no dan plazo de retención ni de borrado para las entradas de la API (la sección 3.3 sólo obliga al desarrollador a atender las solicitudes de los usuarios finales, como la de borrado, y remite a un contacto de DeepSeek) ni lugar de tratamiento; su política de privacidad (10 de febrero de 2026) permite
entrenar con datos personales, incluidas entradas, con exclusión por correo, y almacena en China sin tratar
la API: entrenamiento posible, retención desconocida, lugar China según la política. Xiaomi (leído el 19 de octubre de 2026) publica **dos políticas de privacidad de la plataforma con alcances distintos**: la de **China continental** (vigente desde el 10 de octubre de 2026), cuyo almacenamiento en la República Popular China se aplica al uso dentro de China y que remite el uso fuera de China a las condiciones publicadas para los servicios fuera de ella, y la **internacional** (https://privacy.mi.com/XiaomiMiMoPlatformos/en_GB/, actualizada el 25 de junio de 2026), que cubre la plataforma de API y herramientas como MiMo Code, dice que el contenido proporcionado, imágenes incluidas, se usa para procesar la petición y generar la salida y que **Xiaomi no lo usará para entrenar modelos ni para otros fines**, no fija un número de días de retención (se conserva el tiempo que exigen las finalidades o la ley y se borra o anonimiza al cumplirse la finalidad) y aloja los datos en Países Bajos y Singapur (Países Bajos para usuarios del EEE, el Reino Unido y Suiza, con posible transferencia a Singapur; Singapur para los demás, con posible transferencia a los Países Bajos): **las transferencias posteriores son posibles y no se registra ninguna promesa de que no ocurran**. La política de continente exige consentimiento previo para entrenar y no fija retención. Ninguna de las dos nombra el Token Plan, cuya página no dice nada de datos. Del acuerdo de la plataforma abierta (actualizado el 7 de julio de 2026) sólo se obtuvieron extractos (la página se dibuja en el navegador): Xiaomi encargada y desarrollador responsable, datos de negocio del cliente, borrado en un «plazo razonable» sin días y, fuera de China continental, servidores de Europa y Singapur; no tratan el entrenamiento. Leídas con su alcance, las tres no se contradicen. Se registra: **entrenamiento: Xiaomi declara que no usa el contenido para entrenar; retención por finalidad y ley, sin número de días (desconocido); lugar: Países Bajos y Singapur con posibles transferencias; la aplicabilidad a la cuenta del Token Plan del responsable (región de la cuenta y política que la plataforma le aplica) no está establecida: desconocida.** Estas lecturas, con su URL y fecha, son una precondición de la prueba real de la puerta de cada
proveedor y se repiten si cambia la fecha de una página. Métricas: éxito por oráculo, falsos éxitos, pasos y
primitivas de entrada, tokens (contexto como en AG-02) con el reparto de imágenes sólo estimado (cuenta cada
aparición de la imagen en el historial retenido; fórmula de Anthropic ceil(ancho/28) por ceil(alto/28),
1 196 para 1280 por 720; no disponible para un proveedor sin fórmula documentada), latencia y coste cuando
es medible. Qué muestra y qué no: hasta tres modelos, once metas sintéticas, sin aviso ajustado; nada de
aplicaciones reales ni de «mejor que» en general. La prueba real de la puerta usa varias imágenes
sintéticas contrastadas: prueba el transporte y unas pocas respuestas que dependen de la imagen, no
competencia en tareas de interfaz.

**AG-02R.** Seis series por modelo y fase, con los mismos tres agentes y un brazo AG-03 por cada modelo que
pase su puerta; no cambia nada salvo la corrida (huella fija de binarios, metas, configuración, versión de
Claude Code, modelo y motor de captura; una serie con otra huella no se agrupa; las series únicas anteriores
se informan aparte como «corrida 0»). **Bloques (R4):** una repetición de un modelo, con sus dos brazos; el
orden de las metas, de los modelos dentro de la repetición y de los brazos dentro del bloque sale del
SHA-256 de reglas preinscritas, igual para los tres modelos en las metas. Presupuesto: como máximo 39
acciones en vivo por serie (702 en AG-02R con n = 6; hasta 1 404 con AG-03). Con las cifras registradas:
unos 34,2 M de tokens de contexto y 5,7 h de agente en AG-02R, y de 0,8 a 2,5 veces los tokens de cada
modelo en AG-03; Haiku unos 0,50 USD según la CLI en AG-02R; DeepSeek y MiMo sólo en tokens. **Varianza
(R4):** k de n por meta con intervalo de Wilson (supone repeticiones independientes y es descriptivo),
puntuación de serie con media, rango e intervalo *t*; los contrastes (brazo dentro de un modelo; modelo
dentro de un brazo, Haiku frente a los otros) se emparejan por bloque con una prueba de aleatorización de
cambio de signo con 2^6 = 64 asignaciones, de modo que **el menor p bilateral alcanzable es 0,03125 y con
n = 6 sólo llega a p < 0,05 si las seis diferencias dentro del bloque tienen el mismo signo** (el siguiente
resultado más extremo da 0,0625); se informan diferencias pareadas y su distribución exacta como evidencia
descriptiva, no la prueba no pareada de 924 disposiciones. **Sucesos (falsos éxitos, eventos de seguridad):**
primero los recuentos; la declaración primaria es **por repetición** (cuántas de las seis repeticiones
tuvieron algún suceso en cualquier serie corrida en ella, todos los modelos y brazos), que admite la
dependencia compartida dentro de una repetición: 0 de 6 repeticiones da una cota superior exacta de
Clopper-Pearson al 95 % bilateral de 45,9 % para la probabilidad de que una repetición contenga un suceso,
**condicionada a que las repeticiones sean independientes** (la deriva entre días puede violarlo). Por
modelo y brazo, el mismo cálculo 0 de 6 (45,9 %) es sólo una **ilustración condicional** que supone
independientes las seis series de ese modelo; **no se agrupan los tres modelos como 18 sorteos
independientes** y no se afirma ninguna cota 0 de 18. La cota por intento de meta (0 de 66 por modelo y
brazo: 5,4 %) supone ensayos independientes y se rotula así; ninguna es incondicional. Contraejemplos
(aritmética exacta): seis series independientes en las que las once metas comparten un suceso de
probabilidad 0,2 dan cero sucesos en 66 intentos con probabilidad 0,8^6 = 26,2 %; seis repeticiones
independientes en las que las series de los tres modelos de una repetición comparten un suceso de
probabilidad 0,3 dan cero sucesos agrupados con probabilidad 0,7^6 = 11,8 %, y entonces una cota agrupada de
18,5 % excluiría el 30 % verdadero. **Casilla de resultado (R5):** una casilla es (modelo, brazo, fase,
repetición, meta); su resultado primario es el del primer intento válido (un intento es inválido sólo si el
arnés falló antes de la primera llamada al modelo); un intento inválido se repite una vez dentro de la misma
casilla y un segundo fallo es un fallo técnico, que cuenta como no éxito en el denominador y se informa
aparte; tras la primera llamada al modelo nada se repite, y un rechazo de captura, un rechazo de la puerta o
la exclusión de un modelo no son fallos que reintentar. Los intentos brutos y los fallos técnicos se
informan por separado; las casillas no intentadas tras una parada de la campaña no son fallos ni entran en
n, y la serie incompleta sale de los contrastes de puntuación de serie.

**Tareas del responsable (PR 97).** OI-1, respondida el 20 de octubre de 2026: cuenta de suscripción de consumo con el ajuste de mejora de modelos
desactivado (lectura en la decisión 5 y en el párrafo R3 de este resumen; la precondición de la prueba de Claude queda cumplida).
OI-2, respondida sí y hecha el 19 de octubre de 2026: los términos y políticas públicos de DeepSeek y de
Xiaomi están leídos y registrados arriba. OI-3, respondida: el responsable cambió O7 a n = 6.
