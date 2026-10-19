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
The coordinator ruled the same day that the two must stay refused (fail closed) and that the other
providers be reached through a loop of our own.

The owner also told the coordinator on 2026-10-18 that API keys exist **only for DeepSeek and MiMo**;
GPT-6 Luna is available only through the Codex subscription, with no API key.

## Decision

1. **Models and providers.** AG-02 may run, in the same phases and under the same rules as ADR-0036:
   - **DeepSeek V4.1 Flash 1017**, effort max, through DeepSeek's own API (`https://api.deepseek.com`,
     OpenAI-compatible Chat Completions). DeepSeek's documentation names the model `deepseek-flash`
     and says it "is now served by DeepSeek-V4.1-Flash-1017". The effort is `reasoning_effort: "max"`
     with `thinking: {"type": "enabled"}`.
   - **Xiaomi MiMo V2.6 Flash**, effort max (free tier as the owner has it), through Xiaomi's API
     (`https://api.xiaomimimo.com/v1`, OpenAI-compatible; the key goes in an `api-key` header). The
     pages read name `mimo-v2.6-pro` only: the model id `mimo-v2.6-flash` and the spelling of effort
     max are **unverified**, so the owner supplies them (`--model`, `--extra-param`) and the first
     dry run shows whether the provider accepts them.
   - **OpenAI GPT-6 Luna** is **authorised but not runnable**: it exists for the owner only through
     the Codex subscription, and Codex cannot be confined (decision 6). It is recorded in the code
     (`AUTHORISED_NOT_RUNNABLE`) and runs only if Codex ever passes its probe.
   SubQ stays under [ADR-0033](0033-external-decision-model-for-offline-agent-experiments.md).
2. **The privacy exception of ADR-0036 is extended to exactly these two providers.** The observations
   of `ommatix-testapp.exe` and `ommatix-testeditor.exe` (synthetic labels) are the model's input and
   leave the machine for the provider's API; nothing else may be on screen, which the owner arranges
   by hand before a session. Each provider's data-use position, as far as it was checked, travels with
   the results (`PROVIDERS[...]["caveat"]`): DeepSeek's API documentation has no data-use statement
   and its terms were not read (unverified); Xiaomi's quick-start has none either, and a free tier may
   retain or train on inputs (unverified). Neither statement changes the rule: the inputs are
   synthetic test-application labels, which is the only reason the exception is acceptable. Zen, the
   OpenCode aggregator, was **not** adopted as a route: it would add a recipient, and its page says
   that during their free period MiMo models' "collected data may be used to improve the model".
3. **The agent for these providers is `tools/agent/ag02/minimal_agent.py`** (standard library only):
   - It speaks Chat Completions to **one** endpoint, offers the model **exactly the three Ommatix
     tools** (their definitions come from the engine's `tools/list`, through the budget proxy) and
     has no other capability by construction: no shell, file, web or image code exists in it; no
     skill, plugin, memory or instruction file is read; the request carries only the fixed system
     instruction of `goals.json`, the goal prompt and the conversation, plus the closed list of
     request parameters (`reasoning_effort`, `thinking`, `max_tokens`, `max_completion_tokens`,
     `temperature`, `top_p`) that `--extra-param` may set and nothing that can touch the model, the
     messages or the tools.
   - Only text reaches the model: an MCP result's non-text block is replaced by a fixed note. A tool
     name outside the three is answered with an error and never forwarded. If the server lists
     anything but the three tools, the session reports it at `init` and the runner ends it before any
     request is sent.
   - The endpoint is https (or `http` on a loopback address, for the preflight's stand-in); no
     redirect is followed, no proxy variable is honoured, and the child's environment is the operating
     system's basics only. The key is read from a file outside the repository (`--key-file`), sent in
     one header and never printed, logged, put in an argument or an environment variable.
   - HTTP errors reach the stream as a status code (`http_500`), never a body, because a provider may
     echo its input in an error. The stream is the one the runner already reads (`init`, `assistant`,
     `user`, `result`), so the runner, the proxy, the oracle and the scoring are the same for every
     agent. `reasoning_content` of an assistant message is kept for the next request of the same turn
     (the providers ask for it back with tool calls) and nothing else of the reply is kept.
   - Budgets: `--max-turns` (40) and `--max-total-tokens` (400 000 context plus output tokens per
     goal) on top of the proxy's live-action budget; a limit ends the goal with the stream's
     `error_max_turns` / `error_max_budget_usd`. Cost is reported only if `--price` is given.
4. **An `AgentCLI` interface** (`tools/agent/ag02/agentcli.py`) gives `run_goal.py` the agent's
   command, environment, proof and provenance, so the runner is not forked: `--agent-cli claude`
   (default; ADR-0036, behaviour unchanged), `minimal` (with `--provider deepseek|mimo`), `codex` and
   `opencode` (probes only).
5. **The minimal agent's preflight** (`preflight_cli.py --agent-cli minimal`) reads the real request
   it sends to a stand-in on 127.0.0.1 in a world of sentinels (instruction files in the working
   directory, its ancestors and a stand-in home, a skill, a dotenv file, and provider keys and a proxy
   in the parent environment) and passes only if: every body has exactly the fixed keys plus the
   configured parameters; the model, the fixed instruction and the prompt are exactly those; later
   messages are assistant and tool messages with text content only; the tools are exactly the three
   definitions, the same in every request; no sentinel and no key is in any body; the only headers are
   the expected ones with the key in one; the only path is `<provider path>/chat/completions`; the
   history is only appended to; the agent's source passes a static check (import allowlist; no
   `eval`, `exec`, `open`; `subprocess` only to start the proxy); and a **positive control** feeds the
   verifier seven broken copies of a real request (an extra tool, a dropped tool, a sentinel, an image
   part, an extra key, another model, another instruction) and fails the preflight if any goes unseen.
   The proof (`ag02-isolation-proof-minimal-1`) binds the agent's source SHA-256, version, provider,
   production endpoint, authentication style, model, request parameters, instruction and the init
   facts; the runner refuses a run that differs, and checks every session's `init` against it. It
   differs from production in stated ways only: the endpoint is a stand-in (the path below the host is
   the production one), the key is a dummy and the model's answers are scripted.
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
7. **Provenance and the summary.** A result carries the agent (`cli`), the provider, the model the
   session reported and the one requested, the agent's version (and, for the minimal agent, the SHA-256
   of its source, the endpoint's host and the names of the request parameters). `summarise.py` groups
   by `<agent>:<provider>/<model>` and prints one block per group, never one blended table.
8. **Keys.** Plain files outside the repository, one key per file, first line, no other content, e.g.
   `%LOCALAPPDATA%\Ommatix\keys\deepseek.key` and `mimo.key`; the path is a command-line argument
   (`--key-file`), the content is read only by the agent. **No key exists yet; this change makes no
   real call.**
9. **Phases and consent are those of ADR-0036**: the multi-model runs start in dry run (phase 1) after
   this change is reviewed; phase 2 needs the owner present and a consent for that session, per
   model; a consent never carries over. AG-02 stays `NO_EJECUTADO`; this ADR ticks no box.

## What the multi-model results will and will not show

Same fixed instruction, same goals, same oracle and same proxy for every agent: that is what makes
the figures comparable in kind. They are **not** a clean comparison of models: the Claude Code runs
use Claude Code's own loop, prompts and tool schema handling, the others use ours; sampling, thinking
and output limits differ by provider. A difference between rows can come from the model, the loop or
the provider's tool-calling quirks; AG-02 makes no claim of "better than". The minimal agent is only
tested against our stand-in: the first real dry run may show provider differences the stand-in cannot
(for instance MiMo's `reasoning_content` handling or its effort parameter), and those would be fixed
in the agent and its proof, not worked around in the runner.

## Alternatives considered

- **Run Codex and OpenCode with a stricter configuration or a wrapper.** The tools that remain
  (`view_image`, `execute`+`fetch`, deferred MCP) are chosen by the CLIs, not by configuration;
  wrapping them cannot remove a tool from the request. Rejected: fail closed.
- **Accept `view_image` under a read-denying permission profile, or `execute` without `fetch`.**
  The coordinator ruled against tolerating either: the rule is exactly three tools.
- **A general client library (LiteLLM, the OpenAI SDK).** A large dependency tree for the one request
  shape we need; the confinement proof would have to cover it. Rejected for a standard-library loop
  whose source hash is bound to the proof.
- **OpenCode Zen as the route to MiMo and DeepSeek.** One more recipient, with a stated
  data-improvement clause for the MiMo free models. Rejected; the providers' own APIs are the
  recipients named in decision 1.
- **Forking the runner for the new agents.** Rejected: the budget proxy, oracle and scoring stay
  shared; only the agent differs.

## Consequences and cost

- Two more recipients of synthetic test-application labels (DeepSeek, Xiaomi) with their caveats
  unverified; the owner decides to run each session knowing so.
- A loop of our own to maintain, covered by tests and bound to the proof by its hash; any edit to
  `minimal_agent.py` invalidates every proof until the preflight is repeated.
- Codex and OpenCode stay refused, and GPT-6 Luna stays unrunnable, until a version passes.
- Probes run on 2026-10-18 on the author's PC (Codex 0.163.0, OpenCode 2.0.12): the verdicts above,
  and the minimal agent's preflight against the real `ommatix serve --source sim` for both providers
  (passed, seven of seven control mutations seen). No provider was reached and no screen or input was
  used. Starting `opencode` commands started a background `opencode serve --service` process, which
  the author stopped; the owner may find one after running the probe.

## Required validation

- `tools/agent/ag02/test_multimodel.py`: the endpoint and parameter rules; the key file; usage in
  both providers' spellings; non-text content refused; the assistant message kept; the agent
  end to end against the stand-in and the real proxy and a fake engine (exactly three tools in every
  request, instruction and prompt, auth header per provider, the key nowhere else, path below the
  host, an invented tool never forwarded, `reasoning_content` carried, a fourth listed tool ending the
  session before any request, turn and token limits, usage as the runner reads it, the prompt off the
  command line, the environment scrubbed); hostile endpoints (error body, redirect, malformed reply);
  the request verifier against real requests and against each broken copy; the static check; the
  preflight on the real agent and on leaky or widened copies of it; the proof loader and every
  binding; the init comparison; the provider table; the runner (`--plan`, refusals, a goal run end to
  end with the minimal agent and its provenance); the Codex and OpenCode verdicts on request bodies
  shaped like the installed versions'; the builders; the per-model summary.
- `tools/agent/ag02/test_run_goal.py` (existing, the Claude path unchanged apart from the provenance
  fields) and `test_summarise.py`.
- The owner's multi-model sessions are the evidence of the experiment; until they are run and
  recorded, AG-02 stays `NO_EJECUTADO`.

## Resumen (ES)

**Decisión del responsable (18 de octubre de 2026).** AG-02 se extiende a modelos de otros
proveedores como agente, con las mismas reglas de la ADR-0036: sólo las tres herramientas de Ommatix,
nunca píxeles ni imágenes, ninguna otra capacidad, sólo las aplicaciones de prueba, `dry_run` antes de
entrada real, presupuesto, libro y parada, el responsable presente y consentimiento por sesión.
Modelos: DeepSeek V4.1 Flash 1017 (esfuerzo máximo, `deepseek-flash` en la API de DeepSeek), Xiaomi
MiMo V2.6 Flash (esfuerzo máximo, nivel gratuito; el identificador del modelo y el parámetro de
esfuerzo no están en las páginas leídas y los aporta el responsable) y GPT-6 Luna, **autorizado pero
no ejecutable**: el responsable sólo lo tiene con la suscripción de Codex, sin clave de API, y Codex no
se puede confinar. La excepción de privacidad de la ADR-0036 (las observaciones de las aplicaciones de
prueba son etiquetas sintéticas) se extiende exactamente a DeepSeek y Xiaomi, con su salvedad: la
documentación de ninguno declara el uso de los datos y el nivel gratuito puede conservarlos o
entrenar con ellos (sin verificar); sólo es aceptable porque las entradas son etiquetas sintéticas.
SubQ sigue bajo la ADR-0033.

**Sondeos de Codex y OpenCode (18 de octubre de 2026).** Con un servidor local en 127.0.0.1 que
guarda la petición real y sin llamar a ningún proveedor: Codex 0.163.0, con la configuración más
estricta que existe, sigue ofreciendo `view_image`, herramientas de recursos MCP y un `tool_search`
tras el que quedan diferidas las herramientas de Ommatix, y lee habilidades del `~/.agents/skills` real
(`HOME` y `USERPROFILE` se ignoran en Windows); no hay ninguna clave que quite `view_image` ni las
habilidades. OpenCode 2.0.12 sólo llega a las herramientas MCP por `execute`, un entorno de JavaScript
que expone `fetch` (HTTP), lista las habilidades reales del operador y, salvo que se niegue por
nombre, un espacio `browser` de 45 herramientas. Ni `view_image` ni `execute`+`fetch` se toleran como
«inertes»: ambos CLI quedan **rechazados por defecto** (`tool_surface_wider_than_three`,
`skills_from_real_home`) y el ejecutor no los lanza.

**El agente mínimo.** `minimal_agent.py` (biblioteca estándar) habla Chat Completions con un único
punto de acceso, ofrece exactamente las tres herramientas (las definiciones vienen del `tools/list`
del motor, a través del proxy de presupuesto), no tiene capacidad alguna más (sin shell, archivos,
web ni imágenes; no lee habilidades, plugins, memoria ni archivos de instrucciones), sólo deja pasar
texto al modelo, no sigue redirecciones ni usa variables de proxy, tiene un entorno mínimo y lee la
clave de un archivo fuera del repositorio que nunca imprime. Emite el mismo flujo que ya lee el
ejecutor, de modo que proxy, oráculo y puntuación son los mismos. Su preflight (`preflight_cli.py`)
lee la petición real en un mundo de centinelas y exige: claves del cuerpo exactas, modelo, instrucción
y prompt exactos, mensajes sólo de texto, exactamente las tres definiciones, ningún centinela ni clave
en el cuerpo, las únicas cabeceras esperadas y la ruta del proveedor; un control positivo con siete
copias rotas de una petición real que el verificador debe ver. La prueba liga el hash del código del
agente, el proveedor, el punto de acceso, el modelo, los parámetros, la instrucción y los hechos de
`init`, y el ejecutor rechaza una corrida que difiera. Los resultados llevan agente, proveedor, modelo
y versión, y `summarise.py` agrupa por modelo sin mezclarlos. Las cifras no son una comparación limpia
de modelos (el bucle, el muestreo y el razonamiento difieren) y AG-02 no afirma «mejor que». Las
claves son archivos fuera del repositorio (por ejemplo `deepseek.key` y `mimo.key`); aún no existen y
este cambio no hace ninguna llamada real. AG-02 sigue `NO_EJECUTADO`.
