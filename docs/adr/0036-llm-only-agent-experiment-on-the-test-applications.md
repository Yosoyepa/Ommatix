# 0036. Run an LLM-only agent experiment (AG-02) on the two test applications

- **Status:** Accepted
- **Date:** 2026-10-17
- **Approval:** The project owner, 2026-10-17. The experiment is accepted only on the choices listed under Decision. Acceptance changes no test status or checkbox of chapter 06 §6.7.

## Context

Ommatix exists to give an agent computer use efficiently **without a vision-language model**
(VLM). No recorded run shows that yet. The supervised clients of S4 follow a fixed sequence of
labels (S4_ACEPTACION_REDUCIDA.md: they measure the engine and the action loop, not an agent);
AG-01 ([ADR-0033](0033-external-decision-model-for-offline-agent-experiments.md), chapter 09
§9.10) only scored one-shot choices offline, on synthetic scenes, with a decision model that
does not run a loop; EV-02 (450 planned task runs) is deferred. The open question is whether an
LLM that never sees a pixel, given only the three tools of the Ommatix 1.0 contract
(`observe_screen`, `execute_action`, `get_engine_metrics`), completes goals, says so honestly when
it cannot, and respects the loop the contract asks for (act by element id and revision, observe
again after each action).

Answering it sends observations to a third party: the agent runs in Claude Code and every tool
result goes to Anthropic's API as the model's input. ADR-0012 keeps screen values on the machine
by default. The experiment is therefore an explicit, bounded exception, and it must not weaken
any gate of the engine (ADR-0007, ADR-0020, ADR-0025).

## Decision

The owner chose the following (2026-10-17), and the experiment runs under these conditions only:

1. **Agent.** Claude, through Claude Code as the MCP client, with **only** the three Ommatix MCP
   tools. No Bash, file, web or image tool: the session starts with every built-in tool off
   (`--tools ""`, `--strict-mcp-config`, an `--allowedTools` list of the three), the runner reads
   the tool list the session reports and kills it if anything else is present, and the working
   directory is an empty scratch directory. The agent never receives pixels or screenshots (the
   engine does not send them: `allow_screenshots_to_client = false`). The observations of the test
   applications - synthetic labels of `ommatix-testapp.exe` and `ommatix-testeditor.exe` - **are sent
   to Anthropic's API as the agent's input**; that is the privacy exception of this ADR, and it
   covers nothing else. No other window may be on screen: the owner minimises them by hand before a
   session. **SubQ is not used**; ADR-0033 is unchanged.
2. **Phases.** Phase 1 is `dry_run` only: no input is delivered; it scores the choice and the
   loop. Phase 2 is live **only** in `ommatix-testapp.exe` and `ommatix-testeditor.exe`, with a
   pre-registered budget of live actions, a ledger, the operator's external stop file, the owner
   present and the owner's consent for that session (a consent does not carry over to the next
   session).
3. **Baseline.** A comparison with a VLM agent is a later experiment (AG-03), not part of this
   one. AG-02 makes no claim of "better than"; it records what an LLM-only agent does.

The implementation follows from those choices:

4. **A budget proxy sits between the agent and the engine** (`tools/agent/ag02/proxy.py`, standard
   library only). It forwards JSON-RPC unchanged except `execute_action`: in phase 1 it refuses any
   call whose `dry_run` is not `true`; in phase 2 it counts each `dry_run: false` call in a ledger
   file **before** forwarding it and, once the ledger holds the session budget (or the goal's own
   bound), answers with a JSON-RPC error and does **not** forward (fail closed). It also limits live
   calls to `click` and `type`, and a live `type` text to printable ASCII of at most 16 characters.
   What the engine might read differently (duplicate keys, `NaN`, a JSON batch, a non-boolean
   `dry_run`, a `tools/call` notification) is refused, not forwarded. An unreadable ledger refuses
   every live call. The proxy is not a second authority: the engine's gate stays the only one, and
   the proxy can only refuse more.
5. **The proxy never touches the operator's stop file**, and the engine's rules are unchanged:
   `stop_file` and `purge_file` belong to the operator and lie outside the repository, the
   foreground application is attested, `allowed_applications` lists exactly the two test
   applications, accessibility evidence is `uia`, `max_snapshot_age_ms` is at most 10000, and live
   input needs `--source dxgi --input sendinput`. The runner may create the stop file after a
   safety event (as the S4 clients do) and never deletes it. Bringing an application to the front
   is the owner's, by hand; the runner only reads the foreground.
6. **Metadata only.** The proxy log, the runner's stream reader and the per-goal result hold times,
   byte sizes, tool names, statuses, error codes, `input_delivered`, element ids and revisions, token
   counts, cost and booleans. Never observation text, labels, values, error messages or typed
   text. A label is recorded only when it belongs to the fixed vocabulary of the goal's
   application (`other` otherwise); a typed text is compared in memory and recorded as a boolean.
   Outputs lie outside the repository (`%LOCALAPPDATA%\Ommatix\runs\ag02\...`); the Claude session is
   started with `--no-session-persistence`, so no transcript is stored.
7. **Fixed goals, independent oracle.** `tools/agent/ag02/goals.json` (11 goals, pinned by SHA-256 in
   the runner) holds the goals and their ground truth. Success is decided by the operating system -
   the test applications' window titles (counter, switch, page, character counts) and the editor's
   folder - the same oracles as the S4 clients. The agent's claim never counts; a claim of success
   that the oracle denies is a false success and is reported as such. Two goals are impossible by
   construction (a button that does not exist) and one is a decoy field the engine cannot confirm
   (success is abstaining with no key sent to any field). A goal about a disabled element is not
   included: neither test application has a disabled control.
8. **Exploratory.** AG-02 is a row of `plan_pruebas.csv` in state `NO_EJECUTADO`; it is not a criterion
   of the Definition of Done, changes no requirement and ticks no box. Results are reported as
   measured, with the model, the Claude Code version and the date.

## Alternatives considered

- **A fixed-sequence client again.** Already done (S4); it measures the engine, not an agent.
  Rejected.
- **Run the agent with the usual Claude Code tools and ask it not to use them.** An instruction is
  not a control. Rejected in favour of removing the tools and checking the reported tool list.
- **Enforce the budget only in the engine.** Adding a counter to the engine would change the
  product for an experiment. The proxy keeps the engine as it is and can only refuse more.
- **Give the agent screenshots as a fallback.** It would defeat the purpose (no VLM) and send pixels
  to a third party. Rejected.
- **Use the live Notepad or the Calculator.** Rejected: the goals need controls with a known
  oracle, and the owner's own windows must not appear in a third party's input. The test
  applications are ours and synthetic.

## Consequences and cost

- A first measurement of the product's central claim, with its limits stated: no VLM baseline,
  two synthetic applications, one machine, one model per run, a handful of goals.
- Observations of the two test applications reach Anthropic's API during a session. Anything else on
  screen would reach it too, which is why the owner arranges the screen before a session.
- The agent's context in Claude Code is not guaranteed to be empty of the operator's own settings:
  the session reports counts of what it loaded (`init_counts` in each result), and the dossier
  records them.
- One real check was made while building it (2026-10-17, Claude Code 2.1.302, three calls to the
  smallest model, about USD 0.002 in total, on the simulated desktop with no screen and no input):
  the legacy MCP profile (`--legacy` in the engine) is what Claude Code speaks; with
  `--setting-sources ""` the session lists exactly the three Ommatix tools and a dry run through the
  proxy is `simulated`; `--safe-mode` produced no stream within 200 s and is not the default.
- The runner is Windows-specific only where it reads the foreground and the window titles; its
  tests run on Linux in CI against a fake engine.

## Required validation

- `tools/agent/ag02/test_proxy.py`: budget refusal without forwarding, the dry-run-only refusal,
  accumulation across processes, per-process bound, a corrupt ledger, live action and text limits,
  the fail-closed cases, passthrough unchanged and in order, a metadata-only log, and the stop file
  untouched.
- `tools/agent/ag02/test_rehearsal.py`: the proxy and a fake agent script against a fake engine
  and, when a binary is available, against `ommatix serve --source sim` (the simulated desktop),
  proving the same three behaviours end to end.
- `tools/agent/ag02/test_run_goal.py` and `test_summarise.py`: the goals pinned by hash, the
  titles and oracles (and their agreement with the S4 clients on Windows), the Claude command with no
  built-in tool, the configuration checks, the stream reader (no label outside the vocabulary, no
  typed text), scoring (false success, abstention) and the summary.
- The owner's phase 1 and phase 2 sessions are the evidence of the experiment; until they are run
  and recorded, AG-02 stays `NO_EJECUTADO`.

## Resumen (ES)

**Decisión del responsable (17 de octubre de 2026).** Se ejecuta AG-02: un agente sólo LLM (Claude,
con Claude Code como cliente MCP) que dispone únicamente de las tres herramientas de Ommatix
(`observe_screen`, `execute_action`, `get_engine_metrics`), sin Bash, archivos, web ni imágenes, y que
nunca recibe píxeles. Las observaciones de las aplicaciones de prueba (etiquetas sintéticas) se envían
a la API de Anthropic como entrada del agente; es la excepción de privacidad de esta ADR y no cubre
nada más: ninguna otra ventana debe estar en pantalla (el responsable las minimiza a mano). SubQ no se
usa. Fase 1: sólo `dry_run` (puntúa la elección y el bucle). Fase 2: entrada real sólo en
`ommatix-testapp.exe` y `ommatix-testeditor.exe`, con presupuesto pre-registrado, libro de cuentas,
archivo de parada externo del operador, el responsable presente y consentimiento por sesión. La
comparación con un VLM es un experimento posterior (AG-03).

**Implementación.** Un proxy de presupuesto (`tools/agent/ag02/proxy.py`) entre el agente y el motor
reenvía JSON-RPC sin cambios salvo `execute_action`: en la fase 1 rechaza toda llamada con `dry_run`
distinto de `true`; en la fase 2 cuenta cada llamada `dry_run: false` en el libro antes de reenviarla y,
agotado el presupuesto, responde con un error JSON-RPC sin reenviar. No toca el archivo de parada, que
es del operador. Sólo registra metadatos (nunca texto de observación, etiquetas, valores ni textos
escritos). Once metas fijas (`goals.json`, fijadas por SHA-256), con el oráculo del sistema operativo
(títulos de ventana y carpeta del editor) como verdad: la afirmación del agente nunca cuenta. No hay
meta de elemento deshabilitado porque ninguna aplicación de prueba tiene un control deshabilitado.
AG-02 es exploratorio: una fila `NO_EJECUTADO` de `plan_pruebas.csv`, sin criterio de la definición de
hecho ni casilla.
