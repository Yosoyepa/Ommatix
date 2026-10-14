# 0015. MCP transport on rmcp wire types with a bounded message loop

- **Status:** Proposed
- **Date:** 2026-10-14
- **Approval:** Pending (project owner)

## Context

[ADR-0002](0002-mcp-stdio-with-explicit-profiles.md) chose MCP over stdio, the modern profile
2026-07-28 through a compatible SDK and the legacy profile 2025-11-25 as an opt-in. The SDD
([thesis chapter 02 §2.4](../thesis/02_SDD_ARQUITECTURA_MVP.md)) prefers the `rmcp` SDK, pinned
to a version compatible with the agreed profile [S23], and forbids a home-made JSON-RPC
implementation as a substitute for MCP conformance. The contract
([chapter 07 §7.1–7.2](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md)) adds obligations the
transport must meet: at most 64 KiB per message, no unbounded buffers, no reply to a
notification, one set of rules per declared or negotiated profile [S01–S05].

S2b pinned `rmcp` 3.5.1 and found four places where its service loop and its stdio transport
(`AsyncRwTransport`) do not meet those obligations:

1. **No message size limit.** Lines are read with an unbounded `read_until`, so one long line
   is buffered whole, whatever its size.
2. **Early notifications end the connection.** A notification sent before the lifecycle
   starts closes the session instead of being ignored.
3. **Profiles can be mixed.** One connection may combine `initialize` with the per-request
   `_meta` of the modern profile; the two rule sets are not kept apart.
4. **Unparsable input is dropped silently.** The client gets no `-32700` and cannot tell a
   lost message from a slow one.

## Decision

Use `rmcp` 3.5.1 for the **wire types** and write the **message loop** in the adapter:

- The types that go on the wire come from `rmcp::model`: `DiscoverResult`, `ListToolsResult`,
  `CallToolResult`, `Tool`, `ErrorData`, `ProtocolVersion`, `CacheScope` and the legacy
  `InitializeResult`. Field names, casing and optional fields follow the SDK, not our own
  reading of the specification.
- `ommatix-adapter-mcp` owns the loop: a line reader that never buffers more than the limit
  (a longer line is consumed and discarded in chunks and answered with `-32600`), a
  synchronous connection state machine (`Connection`) that validates the JSON-RPC envelope,
  locks the connection onto one profile with its first selecting request, answers unparsable
  input with `-32700`, never answers a notification, and caps outgoing lines at the same
  64 KiB (an observation page that would exceed it becomes a `LIMIT_EXCEEDED` tool error).
- Modern 2026-07-28 is always served; legacy 2025-11-25 only when `[mcp] legacy_enabled` (or
  the CLI flag) is set. Arguments that violate a tool's input schema are JSON-RPC `-32602`;
  domain errors are tool results with `isError: true`, `code`, `retryable` and `next_step`.
  Stage timings go in the result `_meta` under `io.ommatix/timings`, never in
  `structuredContent`.

The loop covers framing, envelopes and routing. It is not a reimplementation of the MCP data
model, which stays `rmcp`'s.

## Alternatives considered

- **`rmcp`'s service loop and stdio transport as they are:** breaks the 64 KiB limit (an
  unbounded buffer on untrusted input), closes connections a conformant client may keep open,
  and admits mixed profiles. Rejected.
- **A wrapper transport that enforces the limit in front of `rmcp`'s loop:** fixes (1) only;
  (2)–(4) live in the service loop itself.
- **A fully home-made JSON-RPC and MCP layer:** forbidden by SDD §2.4 and gives up the SDK's
  model types, which are the part most likely to drift with the specification.
- **Patching or forking `rmcp`:** a fork has to be maintained; upstream changes are welcome
  but cannot be a precondition for S2b.

## Consequences and cost

The adapter keeps about 800 lines of protocol code (`protocol.rs`, `transport.rs`) that an SDK
would otherwise provide, and every rule in it is ours to keep correct. The contract tests
CT-05..CT-10 pin each rule. Because the wire shapes come from `rmcp`, an SDK upgrade can change
them under us: **every `rmcp` upgrade re-checks the four limitations above** (if the SDK now
handles one correctly, the matching part of the loop may go) and re-runs CT-01..CT-10 and the
dossier examples. Conformance with a real MCP client and pinned client versions is still
pending (chapter 07 §7.1; ADR-0002's required validation): the loop is tested against the
dossier's JSONL examples and synthetic messages, not yet against a host.

## Required validation

CT-05..CT-10 (profiles, notifications, tool listing, tool versus protocol errors, size limits,
invalid inputs) and the 17 cases of `contratos/validacion_casos.json`, in
`src/adapters/mcp/tests/`. Before the decision is trusted: CT-05..CT-10 against a pinned real
client in S0/S3, as ADR-0002 requires.

## Reversibility

If a later `rmcp` release bounds message size, ignores early notifications, keeps profiles
apart and reports parse errors, replace the loop with the SDK's service and keep the tool
handlers and DTOs; the engine and the domain do not change. A real client that rejects the
SDK's wire shapes would instead be a reason to revisit the pinned `rmcp` version.

## Provenance

S2b implementation of the contract (thesis chapter 07). Refines
[ADR-0002](0002-mcp-stdio-with-explicit-profiles.md); applies SDD §2.4 [S23] and the transport
rules of chapter 07 §7.2 [S01–S05].

## Resumen (ES)

El adaptador MCP usa los tipos de wire de `rmcp` 3.5.1 (`DiscoverResult`, `ListToolsResult`,
`CallToolResult`, `Tool`, `ErrorData`, `ProtocolVersion`, `CacheScope`) y un bucle de mensajes
propio y acotado, porque el bucle de servicio de `rmcp` no limita el tamaño de los mensajes,
cierra la conexión ante una notificación temprana, deja mezclar las reglas legacy y modernas en
una conexión y descarta en silencio la entrada no analizable. Cada actualización de `rmcp`
vuelve a comprobar esas cuatro limitaciones; la conformidad con un cliente real sigue
pendiente (capítulo 07 §7.1).
