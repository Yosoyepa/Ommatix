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
//! - [`Broker`] serves several connections on one engine under a single input lease and each
//!   client's grant (ADR-0039 and ADR-0040, proposed); each [`BrokerConnection`] speaks the
//!   same protocol through a [`Connection`]. A client asks for the lease with the `lease`
//!   argument of `observe_screen` and learns where it stands under [`LEASE_META_KEY`].
//!   A connection is served over asynchronous streams ([`BrokerConnection::serve`]) or blocking
//!   ones ([`BrokerConnection::serve_sync`]) with the same loop. A client is admitted with a
//!   grant handed in by the caller ([`Broker::connect`]), or by a one-use ticket on the first
//!   line of its connection ([`Broker::admit`], [`Broker::serve_with_handshake`],
//!   [`Broker::serve_sync_with_handshake`]), whose grant is asked of an
//!   [`ommatix_application::GrantStore`] on every call (ADR-0040, ADR-0043, proposed). A
//!   handshake reads a bounded number of bytes, and a listener counts a connection whose
//!   handshake is not settled against the bound on clients with a [`Reservation`]
//!   ([`Broker::reserve`]). Nothing in the executable constructs one yet; only tests do (this crate's and the
//!   acceptance suite's scenarios): the binary serves stdio only, and no transport serves a
//!   broker yet.
//! - With the cargo feature `broker-pipe`, on Windows, `pipe` serves a broker over a local
//!   named pipe and opens that pipe as a client (ADR-0043, proposed): `pipe::PipeServer`,
//!   `pipe::connect` and `pipe::relay`. Off by default; nothing in the executable uses it yet.
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
//! go only in the result `_meta`: the stage timings of an observation under
//! [`TIMINGS_META_KEY`] with whether its accessibility evidence is complete under
//! [`ACCESSIBILITY_META_KEY`], and the stage timings of an action (gate, freshness capture,
//! delivery, focus confirmation, verification) under [`ACTION_TIMINGS_META_KEY`]; on a broker's
//! connection, where the client stands with respect to the input lease under
//! [`LEASE_META_KEY`] (never on the stdio server, which accepts `observe_screen`'s `lease`
//! argument and ignores it). Expected domain
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

mod broker;
pub mod config;
pub mod dto;
#[cfg(feature = "broker-pipe")]
pub mod pipe;
pub mod protocol;
pub mod schemas;
mod server;
mod tools;
mod transport;

pub use broker::{
    Admission, Broker, BrokerConnection, BrokerError, BrokerState, MAX_SPENT_TICKETS, Reservation,
    TICKET_MAX_LEN, TICKET_MIN_LEN,
};
pub use config::{DEFAULT_MAX_MESSAGE_BYTES, McpConfig};
pub use protocol::{
    Connection, LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION, Profile, SERVER_NAME,
};
pub use server::McpServer;
pub use tools::{
    ACCESSIBILITY_META_KEY, ACTION_TIMINGS_META_KEY, EXECUTE_ACTION, GET_ENGINE_METRICS,
    LEASE_META_KEY, OBSERVE_SCREEN, PLAN_META_KEY, TIMINGS_META_KEY,
};
