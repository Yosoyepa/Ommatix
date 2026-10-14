//! The MCP server: the engine behind a mutex plus the connection loop over a byte stream.

use std::io;
use std::sync::{Arc, Mutex};

use ommatix_application::Engine;
use rmcp::model::Tool;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::config::McpConfig;
use crate::protocol::Connection;
use crate::tools;
use crate::transport::{Frame, LineReader};

/// Read buffer per connection; an oversized line is discarded in chunks of this size.
const READ_CHUNK: usize = 8 * 1024;

/// Serves the Ommatix tools over MCP.
///
/// Cheap to clone: clones share the engine (single writer behind a mutex) and the tool
/// descriptors. Each served stream is its own [`Connection`] with its own profile.
#[derive(Clone)]
#[allow(missing_debug_implementations)] // holds the engine (trait objects)
pub struct McpServer {
    engine: Arc<Mutex<Engine>>,
    config: McpConfig,
    tools: Arc<[Tool]>,
}

impl McpServer {
    pub fn new(engine: Engine, config: McpConfig) -> Self {
        Self::with_shared_engine(Arc::new(Mutex::new(engine)), config)
    }

    /// Serves an engine the caller also keeps (e.g. to read metrics locally).
    pub fn with_shared_engine(engine: Arc<Mutex<Engine>>, config: McpConfig) -> Self {
        Self {
            engine,
            config,
            tools: tools::descriptors().into(),
        }
    }

    pub fn config(&self) -> McpConfig {
        self.config
    }

    /// The shared engine.
    pub fn engine(&self) -> Arc<Mutex<Engine>> {
        Arc::clone(&self.engine)
    }

    /// A new, transport-independent connection (no profile chosen yet).
    pub fn connect(&self) -> Connection {
        Connection::new(
            Arc::clone(&self.engine),
            self.config,
            Arc::clone(&self.tools),
        )
    }

    /// Serves one connection on the process's stdin/stdout until stdin closes. Only
    /// protocol messages are written to stdout.
    pub async fn serve_stdio(self) -> io::Result<()> {
        self.serve(tokio::io::stdin(), tokio::io::stdout()).await
    }

    /// Serves one connection over any byte stream: one JSON-RPC message per line in each
    /// direction, handled in arrival order. Returns at end of input or on an I/O error.
    ///
    /// Tool calls run the engine synchronously on the calling task.
    pub async fn serve<R, W>(self, reader: R, mut writer: W) -> io::Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut connection = self.connect();
        let mut lines = LineReader::new(
            BufReader::with_capacity(READ_CHUNK, reader),
            self.config.message_limit(),
        );
        loop {
            let response = match lines.next_frame().await? {
                Frame::Eof => break,
                Frame::Message(message) => connection.handle_message(&message),
                Frame::Oversized => Some(connection.oversized_message_response()),
            };
            if let Some(mut line) = response {
                line.push('\n');
                writer.write_all(line.as_bytes()).await?;
                writer.flush().await?;
            }
        }
        writer.flush().await
    }
}
