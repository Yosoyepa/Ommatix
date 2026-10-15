# 0028. Let the operator purge the engine's state on demand, through a purge file that is honoured once per instance

- **Status:** Accepted (2026-10-15)
- **Date:** 2026-10-15
- **Approval:** Accepted by the project owner on 2026-10-15 (chapter 06 §6.7, decision 2: on-demand deletion is required). The mechanism is off by default and changes no guarantee accepted today: it adds one optional setting and one check at the request boundary, and it changes neither SEC-04 nor chapter 08 §8.4.

## Context

Chapter 08 §8.4 says the user must be able to delete the engine's state and cache. Since 1.0.53
the engine deletes them when the MCP session ends, however it ends (SEC-07, BDD-39). A session
that goes on has no way to delete its state: a supervised run the operator wants to reset
without ending it, for instance. Chapter 08 §8.4 records this as a limit, and chapter 06 §6.7
keeps the box for the stop, permissions and deletion open until the owner decides (decision 2).

The operator already steers the engine through files that the engine reads and the agent
cannot write: the stop file (`[safety] stop_file`, chapter 08 §8.2) and the window pin file
(ADR-0020). An on-demand purge should use the same channel. An MCP tool would give the agent a
way to trigger a purge, and a command on stdin would share the protocol channel (ADR-0002);
neither fits the rule that the agent does not set the operator's controls (ADR-0007).

Three constraints shape the mechanism. A purge must never interrupt a request, since a request
runs on the snapshot it started with (ADR-0006). No identifier may be issued again after a
purge (SEC-07). And the rules as they stand forbid production code from deleting files: SEC-04
checks that no production source calls a file removal, and chapter 08 §8.4 says that no
production code deletes files and that the operator's files are not touched. The mechanism
must work without changing either rule.

## Decision

1. **One optional setting.** `[safety] purge_file` names a path, resolved like `stop_file`
   (relative to the configuration file). It is empty by default, which means there is no purge
   file. The dossier template carries it commented, as `purge_file = ""`.
2. **The operator asks; the engine answers at a request boundary.** When the operator creates
   the file, the engine purges at its next request boundary. The MCP adapter calls
   `Engine::honour_purge_request` before it handles each message, and never while a message is
   being handled. The purge is `Engine::purge`, the operation that runs when the session ends
   (chapter 08 §8.4). The engine then writes one line on stderr:
   `ommatix: the engine's state and cache were deleted on the operator's request`.
3. **Once per instance, and the engine never deletes the file.** An instance of the file is
   told apart by its modification time and size. The engine honours each instance once. A
   purge is repeated only for a new instance: a file whose modification time or size changed,
   or a file that was gone at a request boundary and has been created again. A file whose
   metadata cannot be read is honoured once while it exists. The file stays where the operator
   put it, as the stop file does. To re-arm the purge the operator removes the file and lets one
   request go through, so that the engine sees the file gone, and then creates it again.
4. **Fail closed on presence.** A purge file whose presence cannot be established counts as
   present, as the stop file does. The engine then purges once, and does not repeat the purge
   while the same instance stays.
5. **Nothing changes for the agent.** No tool, argument, result field or error code is added,
   and no schema changes. The agent sees a purge only as `STALE_SNAPSHOT` for an action on the
   previous snapshot (ADR-0025), and by observing again.
6. **A purge request grants nothing.** It is not a stop and it changes no part of the local
   policy the agent is judged against. Only a file the operator creates starts a purge.

## Alternatives considered

- **Delete the purge file once it has been honoured.** Rejected for this proposal: production
  code would then remove a file, which SEC-04 forbids and chapter 08 §8.4 says does not happen,
  and the operator's file would become a token the engine consumes. The instance rule gives the
  same once-only behaviour without deleting anything. A later ADR can still choose consumption,
  and would then change both rules.
- **A purge tool in MCP.** Rejected: the agent must not be able to trigger or observe a purge,
  and a new tool would change the contract (ADR-0002).
- **A purge at the gate, or during an observation.** Rejected: the request would lose the
  snapshot it is checking. The boundary between requests is the only point at which no request
  holds state.
- **A timer, or a purge when the snapshot grows old.** Rejected: the dossier asks for neither,
  and the snapshot window already bounds what an agent may rely on (ADR-0025).
- **A signal, or a command on stdin.** Rejected: signals are not a Windows mechanism (ADR-0008),
  and stdin is the protocol channel (ADR-0002).

## Consequences and cost

- The operator can delete the engine's state and cache without ending the session. The
  deletion keeps the rules of SEC-07: the configuration, the operator's files and the identifier
  counters survive, and the next observation issues new identifiers.
- With the setting off, the boundary check returns before it touches the file system. With the
  setting on, each MCP message costs one existence check and one metadata read of the purge
  file. That is a file-system call on the message path, not on the perception path. It has not
  been measured on the reference machine (ADR-0010).
- Residual limits. Two instances written at the same time, with the same size, look like one;
  the operator avoids this by letting one request pass between removing the file and creating it
  again. Frames the engine dropped before its last observation are not overwritten (chapter 08
  §8.4), and a forced termination still deletes nothing.
- The file is read by the policy adapter in the composition root. The application depends only
  on the `PolicyStore` port, which gains two methods with defaults, so adapters that do not
  implement them are unaffected (AGENTS.md §3).

## Required validation

- SEC-07 (`docs/thesis/matrices/plan_pruebas.csv`): the `sec_07_*` tests in
  `src/application/src/engine/tests/purge.rs` and in `src/adapters/mcp/tests/sec_07_deletion.rs`,
  which cover the boundary, one purge per instance, a request that is never interrupted, the
  refusal of actions on the purged snapshot with nothing delivered, and identifiers that are never
  reused.
- The policy tests in `src/app/src/policy.rs` (one purge per instance, the file left in place,
  nothing else in the policy changed, the notice sentence) and in `src/app/src/config.rs` (the
  setting is optional, relative to the configuration and validated), and the wiring test in
  `src/app/src/wiring.rs`.
- BDD-53 (`docs/thesis/features/07_retencion.feature`): the purge through the MCP contract, with
  `STALE_SNAPSHOT` and no event delivered, and the file kept.
- SEC-04 and DOC-01 (`src/acceptance/tests/repository.rs`) pass without change: production code
  still removes no file and prints only where it may.

## Reversibility

Removing the setting and the boundary call returns the engine to the deletion at the end of the
session alone, as in 1.0.53. Consuming the file, or making the purge mandatory, is a new
decision: it changes SEC-04 and chapter 08 §8.4.

## Provenance

Chapter 08 §8.4 ("el usuario debe poder borrar estado y caché") and the limit it records;
decision 2 of chapter 06 §6.7 (version 1.0.58). It refines ADR-0007 (operator controls read
from files the agent cannot write), ADR-0006 (transactional snapshots and the identifiers they
issue) and ADR-0025 (the window in which a snapshot may be relied on). It supersedes none of
them.

## Resumen (ES)

El operador puede pedir al motor que borre su estado y su caché sin terminar la sesión:
crea el archivo que nombra `[safety] purge_file`, apagado por defecto. El motor lo atiende en
la frontera de la siguiente petición, nunca durante una, y una vez por cada instancia del
archivo (fecha de modificación y tamaño). No borra el archivo, porque SEC-04 y el capítulo 08
§8.4 prohíben que el código de producción borre archivos. El agente no gana ninguna
herramienta, argumento ni campo de contrato: sólo ve `STALE_SNAPSHOT` en su siguiente acción,
como tras cualquier caducidad. El responsable la aprobó el 15 de octubre de 2026: el
borrado bajo demanda es obligatorio (capítulo 06 §6.7, decisión 2).
