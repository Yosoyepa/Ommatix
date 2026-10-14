# 0004. MCP over stdio as the agent interface

- **Status:** Accepted, amended by [0012](0012-settled-actions-and-wire-contract.md)
- **Date:** 2026-10-14

## Context

The thesis decouples *perception* (fast, deterministic, in Rust) from *reasoning* (an LLM
agent). Agents need a standard way to discover and call perception capabilities without
custom integration code per agent framework.

## Decision

Expose Ommatix as a **Model Context Protocol server over stdio** (newline-delimited
JSON-RPC 2.0), protocol revision `2025-06-18` with fallback to older revisions requested by
the client. MVP tools: `perceive`, `list_elements`, `element_at`, `elements_in_rect`.
Results carry both `structuredContent` and a text copy; tool failures use `isError: true`
so the agent can recover.

The server is implemented directly on `serde_json` (≈200 lines) instead of an SDK, because
the MVP needs only `initialize`, `ping`, `tools/list` and `tools/call`.

## Consequences

- Works with any MCP client (Claude Desktop/Code, agent SDKs) by pointing at the binary.
- No async runtime on the hot path; one request is handled at a time.
- Wire DTOs live in the adapter, so the domain can evolve without breaking agents.
- When we need resources, notifications, progress or HTTP transport, migrate to the
  official Rust SDK (`rmcp`) inside this adapter only.

## Alternatives considered

- **`rmcp` from day one** — more features than needed, pulls `tokio` into the binary.
- **Custom HTTP/gRPC API** — every agent framework would need a bespoke client.
- **Screenshots straight to a VLM** — what Ommatix exists to avoid: slow, costly,
  non-deterministic coordinates.

## Resumen (ES)

La interfaz con el agente es un servidor MCP por stdio (JSON-RPC 2.0 por líneas). Así la
percepción (Rust) queda desacoplada del razonamiento (LLM) y cualquier cliente MCP puede
usar Ommatix. El MVP implementa el subconjunto mínimo del protocolo sobre `serde_json`; si
se necesitan más capacidades se migrará al SDK oficial `rmcp`, cambiando solo este adaptador.
