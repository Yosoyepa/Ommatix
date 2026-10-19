# 0037. Run AG-02 with other providers' models through an agent we can confine

- **Status:** Accepted
- **Date:** 2026-10-18
- **Approval:** The project owner, 2026-10-18. The extension is accepted only on the choices listed under Decision. Acceptance changes no test status or checkbox of chapter 06 §6.7.

## Context

[ADR-0036](0036-llm-only-agent-experiment-on-the-test-applications.md) runs the LLM-only agent
experiment (AG-02) with Claude, through Claude Code, with only the three Ommatix MCP tools. The owner
decided on 2026-10-18 to extend AG-02 to other providers' models as the agent, **under exactly the
rules of ADR-0036**: only the three tools, no pixels or images ever, no other capability, the test
applications only, a dry run before any live run, a budget, a ledger and the operator's stop, the
owner present, and the owner's consent for each session.

The owner named three models and, at first, three command-line agents to run them in: DeepSeek V4.1
Flash 1017 (effort max) and MiMo V2.6 Flash (effort max) through OpenCode, and GPT-6 Luna (effort
max) through Codex CLI and/or OpenCode. ADR-0036's central control is that the agent's tool surface
is **proved** to be the three tools by looking at what the agent really sends, not by what its
documentation says. The author probed both command-line agents the way ADR-0036 probes Claude Code
(a stand-in on 127.0.0.1 that keeps the real request; no provider reached). Neither can be confined.
The coordinator ruled the same day that the two must stay refused (fail closed).

The owner also told the coordinator that API keys exist **only for DeepSeek and MiMo** (GPT-6 Luna is
available only through the Codex subscription, with no API key), and later the same day that the MiMo
key is a **Token Plan** key (prefix `tp-`) whose terms allow coding tools such as Claude Code only, not
scripts or application backends, and whose endpoint is a regional Token Plan host, not
`api.xiaomimimo.com`. The owner therefore decided to run **both** providers through the confined Claude
Code harness pointed at each provider's Anthropic-compatible endpoint, which also gives every model the
same agent loop. The independent review of PR 93 (Codex, CHANGES REQUIRED, 1 P1 and 5 P2) found six
defects in the first answer to the problem, a minimal agent of our own; the owner's decision and the
review are both answered here.

## Decision

1. **Models and routes.** AG-02 may run, in the same phases and under the same rules as ADR-0036, with
   the **confined Claude Code harness** (`--agent-cli claude --route mimo|deepseek`):
   - **DeepSeek V4.1 Flash 1017**, effort max: model `deepseek-flash` (DeepSeek's documentation says it
     "is now served by DeepSeek-V4.1-Flash-1017"), endpoint `https://api.deepseek.com/anthropic`.
   - **Xiaomi MiMo V2.6 Flash**, effort max: model `mimo-v2.6-flash`, endpoint one of the Token Plan
     hosts `https://token-plan-{sgp,cn,ams}.xiaomimimo.com/anthropic`, **named explicitly by the
     operator** (`--route-base-url`; no default, and `api.xiaomimimo.com` is not in the list). A Token
     Plan key (`tp-`/`ttp-`) is refused with any other URL and a Token Plan URL with any other key.
   - **OpenAI GPT-6 Luna** is **authorised but not runnable**: it exists for the owner only through the
     Codex subscription, and Codex cannot be confined (decision 6). It is recorded in the code
     (`AUTHORISED_NOT_RUNNABLE`) and runs only if Codex ever passes its probe.
   SubQ stays under [ADR-0033](0033-external-decision-model-for-offline-agent-experiments.md).
2. **The privacy exception of ADR-0036 is extended to exactly these two providers.** The observations
   of `ommatix-testapp.exe` and `ommatix-testeditor.exe` (synthetic labels) are the model's input and
   leave the machine for the provider's API; nothing else may be on screen, which the owner arranges
   by hand before a session. Each provider's data-use position, as far as it was checked, travels with
   the results (`CLAUDE_ROUTES[...]["caveat"]`): neither provider's API documentation has a data-use
   statement and their terms were not read (unverified), and a free or plan tier may retain or train
   on inputs. That does not change the rule: the inputs are synthetic test-application labels, which is
   the only reason the exception is acceptable. The OpenCode aggregator Zen was **not** adopted as a
   route: it would add a recipient, and its page says that during their free period MiMo models'
   "collected data may be used to improve the model".
3. **The route.** The harness is Claude Code with every ADR-0036 control unchanged (no built-in tool,
   `--strict-mcp-config`, `--setting-sources ""`, the controlled configuration directory, the budget
   proxy, the isolation proof). What a route adds is the child's environment, built by one function,
   `agentcli.route_env`, used by the preflight (with the stand-in's URL and a dummy token) and by
   production (with the provider's URL and the key):
   - The environment is the operating system's basics and **nothing of the operator's own**, plus
     exactly `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, the model in
     `ANTHROPIC_MODEL`, `ANTHROPIC_SMALL_FAST_MODEL`, the three `ANTHROPIC_DEFAULT_*_MODEL` and
     `CLAUDE_CODE_SUBAGENT_MODEL` (so no request names another model), `CLAUDE_CODE_EFFORT_LEVEL=max`,
     the configuration directory and the two ADR-0036 switches. The key is read from `--key-file` (a
     file outside the checkout, validated by the same path rule as every other path) and appears in
     that environment and nowhere else: never in an argument, a log, a result or the proof. The
     operator's own routing variables are still refused (`routing_problems`); the harness-controlled
     route is the only one allowed, and `route_env_problems` checks the final child environment: the
     routing variables present are `ANTHROPIC_BASE_URL` alone, naming one of the route's endpoints.
   - `ANTHROPIC_API_KEY` carries the same value as `ANTHROPIC_AUTH_TOKEN` because Claude Code 2.1.303,
     with only the token set, drops `execute_action` from its tool list (observed on the stand-in;
     another instance of the dependence of the CLI's tool surface on its authentication state recorded
     in ADR-0036 decision 9). With both set it lists the three tools and sends the secret in
     `Authorization: Bearer` and `x-api-key`, to the one endpoint the route names.
   - Effort max is `CLAUDE_CODE_EFFORT_LEVEL=max` (DeepSeek's documented setting), seen in every
     request as `output_config.effort`. **Thinking is whatever Claude Code sends**: version 2.1.303
     always sends `thinking: {"type": "adaptive"}`, and no environment variable makes it send
     `{"type": "enabled"}` (the form MiMo's pages suggest); whether either provider honours or even
     accepts it, and whether the models then think, is **unverified**. The first smoke reported zero
     thinking tokens for DeepSeek, which is not evidence either way.
   - **The route preflight** (`preflight.py --route`) runs the real Claude Code through the same two
     sessions as ADR-0036 decision 9 (the canary and the controlled directory) against the stand-in
     on the route's environment and adds a verdict on the **real Messages requests**: every request that
     offers tools offers exactly the three Ommatix ones; every request names the route's model and no
     other; none carries an image block; the credential headers are `Authorization: Bearer <token>` and,
     when sent, `x-api-key: <token>` only, and the token is in no other header and no body (decoded
     strings, so a JSON escape hides nothing); the effort is the route's. A positive control feeds that
     verdict eight broken copies of a real request and fails the preflight if any goes unseen. The proof
     carries the route (provider, production endpoint, model, effort), the request facts and
     `route_verifier_revision`; the runner refuses a route run whose proof is native, lacks the route, or
     binds another endpoint or model, and a native run with a route proof. The native Claude proof and
     its `verifier_revision` are untouched, so existing ADR-0036 proofs stay valid.
   - **Smoke.** `run_goal.py --smoke --route …` runs one trivial dry-run session against `ommatix serve
     --source sim` (no screen, no input) and only under the route's passing proof.
4. **An `AgentCLI` interface** (`tools/agent/ag02/agentcli.py`) gives `run_goal.py` the agent's
   command, environment, proof and provenance, so the runner is not forked: `--agent-cli claude`
   (default; with `--route` the routes above), `minimal`, `codex` and `opencode` (probes only).
5. **The minimal agent is kept as code and is not the route of either provider.**
   `tools/agent/ag02/minimal_agent.py` (standard library only) speaks OpenAI-compatible Chat Completions
   to one endpoint, offers the model exactly the three tools and has no other capability by construction;
   its request-level preflight and proof (`preflight_cli.py --agent-cli minimal`, schema
   `ag02-isolation-proof-minimal-1`, **revision 2**) bind its source hash. It is **refused for MiMo
   Token Plan keys**, by the runner before anything starts and by the agent itself (`tp-` is not for
   scripts), and DeepSeek goes through the route. The review of PR 93 asked for six corrections and the
   choice was to **fix all six**, so the agent can still be used (for a provider with an ordinary key and
   a Chat Completions endpoint) without a second review of the same defects:
   - *R1* The preflight verifier matched sentinels and the key on the raw bytes, so a JSON escape hid a
     context-reading agent. It now inspects every decoded string of every request, keys included, counts
     the key in the headers separately (exactly the authentication value, nowhere else), and has a control
     with an escaped sentinel; a stricter verifier raises the revision and invalidates older proofs.
   - *R2* The dollar budget was silently ignored and the token check was a post-hoc `>` that final replies
     skipped. A dollar cap is enforced only with valid `--price` (the runner refuses it otherwise); before
     every request the reservation (the body's size in bytes, an upper bound of its tokens, plus the
     explicit `max_tokens` ceiling sent with every request) must fit what is left; usage is accounted
     after every reply, final ones included, and an overrun ends the goal as budget-exhausted without
     executing its call; a missing or malformed usage ends the session.
   - *R3* Malformed, duplicate and parallel calls reached the bridge. The whole provider message is
     validated before anything is emitted or forwarded: at most one call, `type: function`, a unique
     id of plain characters, an allowed name, arguments that are a bounded JSON-object string (strict:
     no duplicate key, no `NaN`) matching the tool's schema, and, in the dry-run-only phase,
     `dry_run: true` stated; nothing is repaired into a default.
   - *R4* Replies and payloads were unbounded: the body is read up to a byte limit plus one (declared
     or not), nesting is bounded by the agent itself (64 levels, measured without recursion, so it does not depend on the
     platform's decoder) and refused with a fixed token, and text, reasoning and
     arguments have size limits, all before anything is retained.
   - *R5* The proof hashed the requested parameters, not the captured ones. The verifier now requires
     every configured value, with its type, in every request (and the ceiling), and the proof hashes the
     captured values.
   - *R6* The agent's own CLI accepted a key inside a checkout. The path rule (absolute, plain, resolved
     outside any checkout) now runs where the file is opened.
6. **Codex CLI and OpenCode are implemented as probes that fail closed.** `preflight_cli.py
   --agent-cli codex|opencode` builds the strictest configuration found for the installed version,
   runs the real CLI against the stand-in with the Ommatix MCP server, and reads the tool list and
   the context in the request. The runner refuses both: `--agent-cli codex|opencode` cannot run a
   goal, a passing probe would still need a stream reader (a new ADR revision), and the verdicts of
   the installed versions are failures:
   - **Codex 0.163.0** (isolated `CODEX_HOME`; `shell_tool`, `unified_exec`, `multi_agent`, `goals`
     off; `web_search` disabled; `tools.update_plan` and `tools.experimental_request_user_input`
     disabled; a catalog entry for the model with `apply_patch_tool_type`, `multi_agent_version`,
     `tool_mode` and image input removed): the request still offers `view_image`, the MCP resource
     tools and a `tool_search` through which the Ommatix tools are deferred, i.e. not exactly the three
     (`tool_surface_wider_than_three`). No configuration key removes `view_image` (it is not a
     feature, not a `[tools]` key, and a text-only catalog does not remove it). The bundled skills
     and the operator's `~/.agents/skills` are listed in the request, and `HOME` and `USERPROFILE`
     are ignored on Windows, so the real home is read (`skills_from_real_home`); no switch found
     (`skills.enabled`, `features.skills`, `skip_host_skill_discovery`). Copying the model's bundled
     catalog entry unchanged into a custom catalog also gives it a JavaScript `exec` tool with nested
     `apply_patch` and `view_image` and a `spawn_agent` namespace, which is why the entry is edited.
   - **OpenCode 2.0.12** (isolated home through `USERPROFILE`, which does move its config and data;
     every built-in action denied; web search, LSP, formatter, snapshots, sharing and update
     off): the MCP tools are reachable **only through `execute`**, a JavaScript code runtime that
     exposes `fetch` (HTTP) to the model, never as direct tools; the model still sees the
     operator's own skills from the real home until the `skill` action is denied; a desktop `browser`
     namespace (45 tools) and an `opencode` namespace appear in the runtime's catalog unless denied by
     name; top-level `policies` had no effect; and the title request carries the goal prompt to
     `small_model`. `execute` with `fetch` is a web capability: not tolerated.
   Neither `view_image` nor `execute`+`fetch` is accepted as "inert": ADR-0036's rule is exactly the
   three tools. A later version, or a configuration found later, may pass the probe; the runner
   would still need a stream reader for it.
7. **Provenance and the summary.** A result carries the agent (`cli`), the provider (`mimo`/`deepseek`
   for a route, with the endpoint's host), the model the session reported and the one requested, and
   the agent's version (for the minimal agent also the source SHA-256, the endpoint's host and the names
   of the request parameters). `summarise.py` groups by `<agent>:<provider>/<model>` and prints one
   block per group, never one blended table.
8. **Keys.** Plain files outside the repository, one key per file, first line, no other content, e.g.
   `%LOCALAPPDATA%\Ommatix\keys\deepseek.key` and `mimo.key`; the path is a command-line argument
   (`--key-file`), the content is read only to build the child's environment (or the minimal agent's
   header). The owner's files exist; nothing printed, logged or stored contains them (checked on the
   smoke outputs and the configuration directories).
9. **Phases and consent are those of ADR-0036**: the multi-model runs start in dry run (phase 1) after
   the review of this change; phase 2 needs the owner present and a consent for that session, per
   model; a consent never carries over. AG-02 stays `NO_EJECUTADO`; this ADR ticks no box.

## What the multi-model results will and will not show

Same fixed instruction, same goals, same oracle, same proxy and **the same agent loop** (Claude Code) for
every model: that makes the figures comparable in kind. They are still not a clean comparison of models:
each provider's Anthropic-compatibility layer, its handling of Claude Code's adaptive thinking and its
caching differ, and Claude Code's own cost figure for a model it does not price is meaningless (the
smokes reported USD 0.019 and 0.027 for sessions of a few thousand tokens; tokens are the usable figure).
A difference between rows can come from the model, the provider's layer or its tool-calling quirks;
AG-02 makes no claim of "better than".

## Alternatives considered

- **Run Codex and OpenCode with a stricter configuration or a wrapper.** The tools that remain
  (`view_image`, `execute`+`fetch`, deferred MCP) are chosen by the CLIs, not by configuration;
  wrapping them cannot remove a tool from the request. Rejected: fail closed.
- **Accept `view_image` under a read-denying permission profile, or `execute` without `fetch`.**
  The coordinator ruled against tolerating either: the rule is exactly three tools.
- **The minimal agent as the route (the first answer).** Rejected by the owner's decision: the MiMo Token
  Plan key is for coding tools only, and one loop for all models makes the comparison fairer. Kept as
  code, hardened, refused for `tp-` keys.
- **Setting only `ANTHROPIC_AUTH_TOKEN`.** The documented configuration, but it removes `execute_action`
  from the tool list of this Claude Code version (see decision 3); rejected, with the proof showing both
  variables, the headers and the tools.
- **Pointing the harness at the provider with the operator's own environment.** Rejected: the
  environment is built, not inherited, so no stray provider variable, proxy or credential reaches it.
- **OpenCode Zen as the route to MiMo and DeepSeek.** One more recipient, with a stated
  data-improvement clause for the MiMo free models. Rejected.
- **A general client library (LiteLLM, the OpenAI SDK).** A large dependency tree the confinement proof
  would have to cover. Rejected.
- **Forking the runner for the new agents.** Rejected: the budget proxy, oracle and scoring stay
  shared; only the agent differs.

## Consequences and cost

- Two more recipients of synthetic test-application labels (DeepSeek, Xiaomi) with their caveats
  unverified; the owner decides to run each session knowing so.
- A route is only as good as its preflight: it is renewed when Claude Code, the model, the endpoint or
  the configuration directory changes. The native proof is not affected.
- A loop of our own to maintain (the minimal agent), covered by tests and bound to its proof by its
  hash; any edit to `minimal_agent.py` invalidates its proofs until the preflight is repeated.
- Codex and OpenCode stay refused, and GPT-6 Luna stays unrunnable, until a version passes.
- Runs on 2026-10-18 on the author's PC (Claude Code 2.1.303; Codex 0.163.0; OpenCode 2.0.12): the
  probe verdicts above; the route preflights for MiMo (Singapore host) and DeepSeek against the stand-in,
  with the real Claude Code and the real `ommatix serve --source sim` (passed; eight of eight control
  mutations seen); the minimal agent's preflights (passed); and **one real smoke session per provider**
  after its preflight passed, against `ommatix serve --source sim` (no screen, no input): each ended
  `DONE` with the three tools listed, `ommatix` connected, one `observe_screen` and one dry-run
  `execute_action` (`simulated`), no proxy refusal and no unexpected tool. These are wiring checks, not
  results. Starting `opencode` commands started a background `opencode serve --service` process, which
  the author stopped.

## Required validation

- `tools/agent/ag02/test_prreview93.py` (written to fail on the reviewed head 65a83c9: against it the 78
  tests give 48 failures and 42 errors, with the new fakes): R1 decoded matching and the full preflight
  with an escaping, context-reading copy of the agent; R2 exact-boundary, final-reply, missing-usage and
  dollar-cap budgets; R3 every malformed, duplicate, parallel and schema-violating call with zero bridge
  calls and one-to-one metrics, and the dry-run-only phase; R4 oversized declared, undeclared and
  streamed bodies, oversized arguments and text, nesting and call lists, with no response text stored;
  R5 changed and mistyped values and the full preflight with a lower effort; R6 the agent's own CLI,
  links and junctions; the Token Plan key rules; and the route (environment, endpoints, keys, proofs,
  the request verdict and its controls, the preflight end to end with the scripted Claude, smoke
  conditions, no key in any plan or provenance).
- `tools/agent/ag02/test_multimodel.py`: the agent end to end, the verifier, the preflight and proof of
  the minimal agent, the Codex and OpenCode verdicts on request bodies shaped like the installed
  versions', the runner integration and the per-model summary.
- `tools/agent/ag02/test_run_goal.py`, `test_preflight.py` and `test_summarise.py` (existing; the Claude
  path unchanged apart from the provenance fields).
- The owner's multi-model sessions are the evidence of the experiment; until they are run and
  recorded, AG-02 stays `NO_EJECUTADO`.

## Resumen (ES)

**Decisión del responsable (18 de octubre de 2026).** AG-02 se extiende a modelos de otros
proveedores como agente, con las mismas reglas de la ADR-0036: sólo las tres herramientas de Ommatix,
nunca píxeles ni imágenes, ninguna otra capacidad, sólo las aplicaciones de prueba, `dry_run` antes de
entrada real, presupuesto, libro y parada, el responsable presente y consentimiento por sesión.
Modelos: DeepSeek V4.1 Flash 1017 (`deepseek-flash`, esfuerzo máximo) y Xiaomi MiMo V2.6 Flash
(`mimo-v2.6-flash`, esfuerzo máximo) y GPT-6 Luna, **autorizado pero no ejecutable**: el responsable
sólo lo tiene con la suscripción de Codex, sin clave de API, y Codex no se puede confinar. La clave de
MiMo es de un Token Plan (`tp-`), cuyas condiciones permiten sólo herramientas de programación como
Claude Code, no guiones ni servicios; por eso **ambos proveedores se ejecutan con el arnés confinado de
Claude Code apuntado al punto de acceso compatible con Anthropic de cada uno** (DeepSeek:
`https://api.deepseek.com/anthropic`; MiMo: un servidor regional del Token Plan, que el operador nombra
explícitamente, sin valor por defecto y sin `api.xiaomimimo.com`). Una clave `tp-` se rechaza con
cualquier otra URL. La excepción de privacidad de la ADR-0036 (las observaciones de las aplicaciones de
prueba son etiquetas sintéticas) se extiende exactamente a DeepSeek y Xiaomi; la documentación de ninguno
declara el uso de los datos (sin verificar): sólo es aceptable porque las entradas son sintéticas. SubQ
sigue bajo la ADR-0033.

**La ruta.** El entorno del hijo lo construye una sola función (`route_env`, la misma en el preflight y en
producción): sólo lo básico del sistema y nada del entorno del operador, más exactamente la URL, la clave
(en `ANTHROPIC_AUTH_TOKEN` y en `ANTHROPIC_API_KEY`, porque con sólo el token esta versión de Claude Code
quita `execute_action` de su lista), el modelo en todas las variables de modelo y el esfuerzo máximo
(`CLAUDE_CODE_EFFORT_LEVEL=max`). La clave se lee de un archivo fuera del repositorio y sólo aparece en
ese entorno. Los controles de la ADR-0036 no cambian. Claude Code 2.1.303 envía siempre
`thinking: {"type": "adaptive"}`; que los proveedores lo acepten o piensen es **sin verificar**. El
preflight de la ruta ejecuta el Claude Code real contra el servidor local y verifica en las peticiones
reales: exactamente las tres herramientas, sólo el modelo de la ruta, ninguna imagen, sólo las cabeceras
de credencial esperadas y el esfuerzo; con un control positivo de ocho copias rotas. La prueba lleva la
ruta y el ejecutor la compara; la prueba nativa de la ADR-0036 no cambia. Se hizo **una sesión real de
humo por proveedor**, tras su preflight, contra `ommatix serve --source sim` (sin pantalla ni entrada):
ambas terminaron con `DONE`, las tres herramientas y ninguna incidencia; son comprobaciones de cableado,
no resultados.

**Sondeos de Codex y OpenCode.** Codex 0.163.0, con la configuración más estricta que existe, sigue
ofreciendo `view_image`, herramientas de recursos MCP y un `tool_search` tras el que quedan diferidas las
herramientas de Ommatix, y lee habilidades del `~/.agents/skills` real (`HOME` y `USERPROFILE` se
ignoran en Windows). OpenCode 2.0.12 sólo llega a las herramientas MCP por `execute`, un entorno de
JavaScript que expone `fetch` (HTTP), y lista las habilidades reales del operador mientras no se niegue
la acción `skill` (y un espacio `browser` de 45 herramientas mientras no se niegue por nombre). Ni
`view_image` ni `execute`+`fetch` se toleran como «inertes»: ambos CLI quedan **rechazados por defecto**
(`tool_surface_wider_than_three`; Codex además `skills_from_real_home`) y el ejecutor no los lanza.

**El agente mínimo y la revisión del PR 93.** `minimal_agent.py` (biblioteca estándar, Chat Completions)
se conserva como código, no es la ruta de ningún proveedor y se **rechaza con claves `tp-` de MiMo**. La
revisión pidió seis correcciones y se eligió **corregir las seis**: (R1) el verificador busca centinelas
y clave en las cadenas decodificadas de cada petición, no en los bytes; (R2) el tope en dólares sólo se
acepta con precios y se aplica, y el de tokens se reserva antes de cada petición y se contabiliza tras
cada respuesta, también la final; una respuesta sin uso válido termina la sesión; (R3) el mensaje del
proveedor se valida entero antes de emitir o reenviar nada (una llamada como máximo, identificador único,
nombre permitido, argumentos estrictos que cumplan el esquema y, en la fase 1, `dry_run: true`); (R4)
límites de bytes, anidamiento, texto y argumentos antes de retener nada; (R5) la prueba liga los valores
que las peticiones llevaron, no los pedidos; (R6) la ruta de la clave se valida donde se abre el archivo.
Las nuevas pruebas fallaban contra la cabeza revisada (48 fallos y 42 errores de 78). Los resultados
llevan agente, proveedor, modelo y versión, y `summarise.py` agrupa por modelo sin mezclarlos. Las cifras
no son una comparación limpia de modelos (la capa de compatibilidad, el pensamiento y la caché difieren
por proveedor) y el coste que informa Claude Code para un modelo que no conoce no sirve; AG-02 no afirma
«mejor que». AG-02 sigue `NO_EJECUTADO`.
