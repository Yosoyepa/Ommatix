//! # ommatix-adapter-mcp
//!
//! Driving adapter that lets an LLM agent use Ommatix through the
//! [Model Context Protocol](https://modelcontextprotocol.io): newline-delimited
//! JSON-RPC 2.0 over stdio. The agent reasons; Ommatix perceives and answers with
//! exact pixel coordinates (see bootstrap ADR B0004).
//!
//! - JSON-RPC envelope validation (`jsonrpc`) is separate from MCP method routing
//!   (`server`) and from the tool registry (`tools`), which owns schemas and handlers.
//! - Every wire type is a DTO defined here (`dto`); run-dependent measurements travel in
//!   `_meta` so `structuredContent` is reproducible.
//!
//! Perception tools: `perceive`, `list_elements`, `element_at`, `elements_in_rect`.
//! Actuation tools: `click`, `type_text`, `press_key` (each observes its effect).

mod dto;
mod jsonrpc;
mod server;
mod tools;

pub use server::{McpServer, PROTOCOL_VERSION};
