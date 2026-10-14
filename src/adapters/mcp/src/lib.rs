//! # ommatix-adapter-mcp
//!
//! Driving adapter that lets an LLM agent use Ommatix through the
//! [Model Context Protocol](https://modelcontextprotocol.io): newline-delimited
//! JSON-RPC 2.0 over stdio. The agent reasons; Ommatix perceives and answers with
//! exact pixel coordinates (see ADR-0004).
//!
//! Tools: `perceive`, `list_elements`, `element_at`, `elements_in_rect`.

mod dto;
mod server;
mod tools;

pub use server::{McpServer, PROTOCOL_VERSION};
