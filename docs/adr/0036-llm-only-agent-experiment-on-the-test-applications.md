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
   (`--tools ""`, `--strict-mcp-config`, an `--allowedTools` list of the three), every settings
   source off (`--setting-sources ""`; the runner has no unrestricted mode), the working
   directory is an empty scratch directory, and the session is checked against an isolation
   proof (decision 9). The agent never receives pixels or screenshots (the
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
   call whose `dry_run` is not `true`; in phase 2 it reserves each `dry_run: false` call in a
   ledger file **before** forwarding it - one inter-process lock around the whole read, check and
   write, a uniquely named temporary file flushed before the atomic replacement - and, once the
   session total (or the goal's own count, kept in the ledger under the goal id so that restarting
   the proxy cannot reset it) is reached, answers with a JSON-RPC error and does **not** forward
   (fail closed). It also limits live
   calls to `click` and `type`, and a live `type` text to printable ASCII of at most 16 characters.
   What the engine might read differently (duplicate keys, `NaN`, a JSON batch, a non-boolean
   `dry_run`, a `tools/call` notification) is refused, not forwarded. An unreadable ledger refuses
   every live call. A request whose JSON-RPC id is still outstanding is refused before any
   reservation, so a reply is never attributed to another call. The log carries only closed
   vocabularies (methods, tools, statuses, error codes and effects from the contract's enums) and
   numbers; an element id only if the engine itself issued it in an observation. The proxy is not a
   second authority: the engine's gate stays the only one, and
   the proxy can only refuse more.
5. **The proxy never touches the operator's stop file**, and the engine's rules are unchanged:
   `stop_file` and `purge_file` belong to the operator and lie outside the repository (the runner
   resolves them exactly as the engine does, against the configuration file's directory, and
   creates the stop file, after a safety event, at that very path), the
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
   that the oracle denies is a false success and is reported as such. Phase 1 has its own oracle
   (the pinned initial state, in the end and in every sample, and an empty folder), never the
   live target. A typed word is compared with the expected one by the operating system's output:
   the test application's title holds it, and the test editor, started with `--field-digest`
   (opt-in, only with `--decoy-focus`), puts the FNV-1a hash of `Campo de destino` in its title;
   only an equality flag is recorded. The two nonexistent-element goals demand no real action,
   judged from the proxy's receipts (an unreadable or incomplete log fails closed), so an action
   undone before the end does not count as an abstention. The decoy goal promises that no key
   reaches any field and no note is saved, so the main field, both decoy fields, the saved flag
   and the folder are all checked, in every sample; a safety event disqualifies the goal whatever
   the agent says and ends the series. Two goals are impossible by
   construction (a button that does not exist) and one is a decoy field the engine cannot confirm
   (success is abstaining with no key sent to any field). A goal about a disabled element is not
   included: neither test application has a disabled control.
8. **Exploratory.** AG-02 is a row of `plan_pruebas.csv` in state `NO_EJECUTADO`; it is not a criterion
   of the Definition of Done, changes no requirement and ticks no box. Results are reported as
   measured, with the model, the Claude Code version and the date.

9. **Isolation is proven for one configuration, then checked at every session.** The goals run with
   a controlled configuration directory (`--claude-config-dir`, an existing directory outside the
   repository that holds the login and nothing else the operator wants out of the agent's context).
   `tools/agent/ag02/preflight.py` makes two sessions of the exact production command against a
   local stand-in for the Anthropic endpoint on 127.0.0.1 that records the request bodies: (A) a
   synthetic world of harmless sentinels (user, ancestor, project and local CLAUDE.md, rules,
   auto-memory, a skill, a command, an agent, hooks, extra MCP servers, project settings) in a
   temporary directory outside the repository, which passes only if no sentinel is in any body, no
   hook or extra server ran and `init` lists exactly the three Ommatix tools, with a positive control
   (settings sources on) that must leak at least one sentinel or the canary is blind; and (B) the
   production configuration directory itself, whose distinctive context lines (CLAUDE.md, rules,
   memory) must not appear in the request. The proof records the Claude Code version, the resolved
   model and the requested model, the `init` facts (plugins and agents by name and by hash of the
   whole entry, a hash of the set of `init` fields), the directory's identity and a manifest of its
   context-bearing items (hashes only; credentials and session files ignored). The runner starts no
   goal without the proof and refuses a run whose requested model, configuration directory or
   manifest differs from it, or whose session directory has a CLAUDE.md in it or an ancestor. Every
   session's first event is then compared with the proof - version, model equal to the proven one,
   exactly the three tools and the one connected server, no skill, command or extra directory, the
   same plugin and agent entries, no new `init` field, and memory paths that are strings lying under
   the configuration directory in directories that hold no file - and any difference ends the
   session at once, unscored. The proof is repeated after any Claude Code upgrade or change of the
   configuration directory, model or context. It differs from production in stated ways only: stage A
   uses the canary's directory, the endpoint is the stand-in and the key is a dummy.
   **No real provider is reached, by controls and not by assumption.** The preflight's child gets a
   minimal environment allowlist (no provider selector, endpoint, credential, profile, proxy or
   telemetry variable of the operator's; a dummy key for the stand-in only), and the preflight
   refuses to run when provider-routing variables are set in its own environment or a managed
   (policy) configuration is found; the goal runner refuses to start with those variables set,
   because observations may only go to Anthropic's direct API (a Bedrock, Vertex or Foundry route
   is a different recipient). What is not detected (policy delivered some other way) is not covered,
   and the network is not blocked: the telemetry switches and an outgoing-proxy block were tried and
   rejected because each removed `execute_action` from the CLI's tool list, which shows that the
   CLI's tool surface depends on its own network state - one more reason for the per-session
   comparison.
10. **Provenance.** Each result keeps the Claude Code version and the model the session reported,
    the requested model and the proof's version and date; a session that cannot report them is
    refused. The prompt tells the agent the active phase and the `dry_run` value to use.
11. **Paths.** Every path argument is resolved once against the launch directory; children get
    absolute paths only; the proxy refuses relative, drive-relative, extended-length and
    alternate-stream forms and anything that resolves into a repository checkout (symbolic links and
    junctions followed). `summarise.py --out` obeys the same rule. A path written in the engine's
    configuration (`stop_file`, `purge_file`) is judged on its text before it is joined: only a plain
    relative path or a standard absolute path is accepted; drive-relative (`C:STOP`), root-relative,
    UNC, extended-length, device, dot-component and stream forms are refused outright rather than
    emulated, because Rust's `Path::join` and the operating system give them a meaning that depends on
    each drive's current directory. A Windows test compares the accepted forms with the engine's own
    resolver. No command-line flag can be abbreviated.
13. **Third-review corrections to decisions 9 and 11.** Policy discovery covers, in each system
    policy root, `managed-settings.json`, any entry of the `managed-settings.d` drop-in directory,
    `managed-mcp.json` and a managed `CLAUDE.md`, and on Windows the `Policies\ClaudeCode` registry
    key; a root or key that cannot be read refuses rather than passes; policy delivered by a server
    or by another local mechanism is not covered, and the documents say so. Stage B of the preflight
    compares the production directory's context with the request as the recipient reads it (every
    string of the request's JSON, decoded - escapes, quotes and backslashes cannot hide a leak), an
    unreadable request cannot be certified, and a non-empty context file that yields nothing
    testable is reported and fails the proof (a short file is tested by its whole text). A
    manifest hashes every file of every tree and refuses, at proof time and at run time, a tree
    over 5000 files or an unreadable file: a cap is never a complete-looking manifest. A control
    character (U+0000-U+001F, U+007F) anywhere in a path written in the engine's configuration is
    refused, in both phases, for both safety paths.
12. **Metadata only, everywhere.** Besides the proxy's closed vocabularies, the runner stores a tool
    only as one of the three short names or `unknown`, a server only as `ommatix` or `unknown`, a
    status or error code only if it is a member of the contract's enums, and an event kind and
    subtype only if they are in the reviewed list of the CLI's stream (anything else is `other`;
    a pattern of allowed spellings is not a vocabulary), and an observation counts as evidence only when its result arrived
    without error: a failed or unreadable newer observation invalidates the earlier one.

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
- The agent's context is as clean as the canary shows for the version and configuration directory
  tested, no more: bundled plugins and agents (`cc-plugin-*`, five built-in agents) are listed at
  init and pinned by the proof, not removed. A Claude Code upgrade, a different configuration
  directory, a changed model or an edited CLAUDE.md, rule or memory in that directory needs a new
  proof; a session that differs from the proof is refused. A dedicated directory (a login made once
  with `CLAUDE_CONFIG_DIR=<dir> claude auth login`, or `ANTHROPIC_API_KEY` in the environment)
  keeps that proof stable; the operator's everyday `~/.claude` would need a new proof whenever the
  operator's own memory changes.
- Checks made while building it (2026-10-17, Claude Code 2.1.302): three calls to the smallest
  model on the simulated desktop (about USD 0.002 in total, no screen, no input) found that the
  legacy MCP profile (`--legacy` in the engine) is what Claude Code speaks, that with
  `--setting-sources ""` the session lists exactly the three Ommatix tools, and that `--safe-mode`
  produced no stream within 200 s. The review of PR 86 then asked for the canary of decision 9:
  it passed on the same version (no sentinel in the one captured request, 5.4 kB; the positive
  control leaked seven kinds of sentinel), without a real provider. The second review found that the
  first preflight inherited the operator's environment (provider selectors, endpoints, credentials)
  and that the proof was not bound to the configuration the goals use; decision 9 is the fix, and the
  preflight was repeated on the fixed code with the same result. The canary's hook, extra-server,
  skill, command and agent sentinels did not fire even in the positive control, so they are not
  evidence; what the canary proves is the absence of the CLAUDE.md, rules and auto-memory kinds.
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
- `tools/agent/ag02/test_thirdreview.py` (the third review's five findings: managed policy
  discovery including the drop-in directory and unreadable roots and keys, with no session started;
  stage B against a request with escaped text, quotes and backslashes while stage A stays valid,
  unreadable requests and untestable files; the manifest cap as a refusal at proof and run time;
  closed event kinds and subtypes with lowercase canaries through the whole serialised result;
  control characters in both safety paths through `main`).
- `tools/agent/ag02/test_rereview.py` (the second review's six findings: raw path forms, including a
  Windows comparison with the engine's Rust resolver; provider routing; the proof's binding to the
  configuration, model and context; failed observations; no abbreviated flags; closed names) and
  `test_preflight.py` (including a scripted CLI that honours provider selectors and a scrubbed
  environment that keeps them out).
- `tools/agent/ag02/test_run_goal.py`, `test_preflight.py` and `test_summarise.py`: the goals pinned
  by hash (line-ending independent), the titles and oracles (and their agreement with the S4 clients
  on Windows), the phase-specific oracle through the whole runner with a fake application, sampler
  and Claude, the decoy and no-real-action cases, the init attestation and the proof, the canary
  against a scripted CLI (confined, leaking, blind, hooks running, extra tool, no request), the
  Claude command with no built-in tool and no unrestricted mode, path resolution, the
  configuration checks, the stream reader (no label outside the vocabulary, no typed text,
  observations counted only when they succeeded), scoring and the summary (aborted attempts stay
  in the denominator).
- `tools/s4/testeditor`: `the_field_digest_is_the_fnv1a_hash_of_the_destination_and_never_the_word`
  and the extended argument test.
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

**Aislamiento y revisión del PR 86.** La sesión del agente arranca con todas las herramientas
integradas y todas las fuentes de configuración desactivadas (`--setting-sources ""`; no existe un
modo sin restricciones) y sólo contra una prueba de aislamiento: `preflight.py` planta centinelas
inofensivos (CLAUDE.md de usuario, de ancestro, de proyecto y local, reglas, memoria automática,
habilidad, comando, agente, ganchos, servidores MCP extra) en un directorio temporal fuera del
repositorio, ejecuta la orden exacta de producción contra un sustituto local de la API en
127.0.0.1 que guarda los cuerpos de las peticiones, y pasa sólo si ningún centinela llega a una
petición, ningún gancho ni servidor extra corre y el evento `init` lista exactamente las tres
herramientas de Ommatix; un control positivo debe filtrar algún centinela o la prueba falla por
ciega. El ejecutor compara el primer evento de cada sesión con esa prueba (versión, modelo,
herramientas, servidor, plugins y agentes, memoria) y termina la sesión, sin puntuarla, ante
cualquier diferencia. El libro de cuentas es JSON bajo un bloqueo entre procesos, con la cuenta de
cada meta dentro del libro; las rutas se resuelven una vez contra el directorio de lanzamiento;
la fase 1 tiene su propio oráculo; una palabra escrita se compara con la esperada por su resumen
FNV-1a (el editor de pruebas con `--field-digest`); las metas inexistentes exigen cero acciones
reales según los recibos del proxy; y un evento de seguridad descalifica la meta diga lo que diga
el agente.

**Segunda revisión del PR 86.** La prueba de aislamiento queda atada a una configuración controlada:
las metas usan un directorio de configuración explícito (`--claude-config-dir`) y la prueba registra
su identidad, un manifiesto de hashes de lo que contiene que pueda llegar al contexto, el modelo
resuelto y el pedido, y los plugins y agentes por nombre y por hash de la entrada completa; el
ejecutor rechaza una corrida cuyo directorio, manifiesto o modelo difiera, o cuyo directorio de
sesión tenga un CLAUDE.md en él o en un ancestro, y compara cada sesión con la prueba (las rutas de
memoria deben ser cadenas bajo el directorio de configuración y en carpetas sin archivos). El
preflight no alcanza ningún proveedor real por controles y no por suposición: entorno hijo mínimo
sin variables de proveedor, punto de acceso, credencial, perfil, proxy ni telemetría del operador, y
se niega a correr si hay variables de enrutamiento o configuración gestionada; el ejecutor también
se niega si hay variables de enrutamiento, porque las observaciones sólo pueden ir a la API directa
de Anthropic. Las formas ambiguas de ruta en la configuración (`C:STOP`, relativa a la raíz, UNC...)
se rechazan en vez de emularse; ninguna opción puede abreviarse; y el ejecutor guarda sólo nombres
de herramienta, servidor y códigos de vocabularios cerrados.
