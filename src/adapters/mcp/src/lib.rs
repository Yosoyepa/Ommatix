//! # ommatix-adapter-mcp
//!
//! Driving adapter that serves the **Ommatix 1.0** tool contract (thesis chapter 07) to LLM
//! agents over the [Model Context Protocol](https://modelcontextprotocol.io): one JSON-RPC
//! 2.0 message per line on stdin/stdout. Agents act by element ID and revision, never by
//! coordinates.
//!
//! ## Public API
//!
//! ```text
//! use ommatix_adapter_mcp::{McpConfig, McpServer};
//!
//! let server = McpServer::new(engine, McpConfig { legacy_enabled: false, max_message_bytes: 65_536 });
//! server.serve_stdio().await?;            // the binary
//! server.clone().serve(reader, writer).await?; // any AsyncRead/AsyncWrite (tests)
//! let mut conn = server.connect();        // synchronous core: one message in, one line out
//! let reply: Option<String> = conn.handle_message(br#"{"jsonrpc":"2.0","id":1,...}"#);
//! ```
//!
//! - [`McpServer`] owns the [`Engine`](ommatix_application::Engine) behind a `Mutex`
//!   (single writer) and the tool descriptors; [`McpServer::serve`] runs one connection over
//!   a byte stream, [`McpServer::serve_stdio`] over the process's stdio.
//! - [`Connection`] is the transport-independent protocol core: envelope validation,
//!   profile selection and routing ([`protocol`] documents every rule and error code).
//! - [`McpConfig`] is the `[mcp]` configuration section.
//! - [`dto`] holds the wire DTOs and their mapping to the application contract types.
//! - [`schemas`] embeds the contract's JSON schemas from `docs/thesis/contratos/esquemas/`.
//!
//! ## Profiles
//!
//! MCP **2026-07-28** is always served: no handshake, per-request `_meta`
//! (`protocolVersion`, `clientCapabilities`), `server/discover`, `resultType: "complete"` on
//! results and `ttlMs`/`cacheScope` on `server/discover` and `tools/list`. MCP
//! **2025-11-25** (`initialize`/`notifications/initialized`) is served only when
//! [`McpConfig::legacy_enabled`] is set. A connection settles on one profile with its first
//! selecting request and never mixes the two rule sets.
//!
//! ## Tools and results
//!
//! Exactly `execute_action`, `get_engine_metrics` and `observe_screen` (listed in that
//! order), with the dossier's input and output schemas. A result carries the Ommatix object
//! as `structuredContent` and the same JSON as its single text block. Run-dependent values
//! (stage timings) go only in the result `_meta` under [`TIMINGS_META_KEY`]. Expected domain
//! errors are tool results with `isError: true`; malformed messages, unknown methods or
//! tools, arguments that violate an input schema and version problems are JSON-RPC errors.
//!
//! ## Limits
//!
//! No incoming message larger than [`McpConfig::max_message_bytes`] (64 KiB) is buffered:
//! it is discarded and answered with `-32600`. No outgoing line exceeds the same limit: an
//! observation page that would is replaced by a `LIMIT_EXCEEDED` tool error
//! (`next_step: reduce_request`). stdout carries protocol messages only.
//!
//! ## Use of `rmcp`
//!
//! The wire types (results, tools, error data, protocol versions, cache hints) are `rmcp`'s
//! model types. Its service loop and stdio transport are not used: they have no message size
//! limit, end the connection on a notification sent before the lifecycle starts, and let a
//! session mix `initialize` with per-request metadata. The connection state machine here
//! replaces them.

pub mod config;
pub mod dto;
pub mod protocol;
pub mod schemas;
mod server;
mod tools;
mod transport;

pub use config::{DEFAULT_MAX_MESSAGE_BYTES, McpConfig};
pub use protocol::{
    Connection, LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION, Profile, SERVER_NAME,
};
pub use server::McpServer;
pub use tools::{EXECUTE_ACTION, GET_ENGINE_METRICS, OBSERVE_SCREEN, TIMINGS_META_KEY};
