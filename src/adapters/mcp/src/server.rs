//! The MCP server: the engine behind a mutex plus the connection loop over a byte stream.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::Engine;
use rmcp::model::Tool;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::config::McpConfig;
use crate::protocol::Connection;
use crate::tools;
use crate::transport::{Frame, LineReader};

/// Read buffer per connection; an oversized line is discarded in chunks of this size.
pub(crate) const READ_CHUNK: usize = 8 * 1024;

/// The marker of a connection ended by its listener's stop: a distinct outcome of a read or of
/// the dispatch check, never an end of input, so that whatever a reader has accumulated is
/// discarded with the connection and nothing more is handed on or written back.
#[derive(Debug)]
struct Stopped;

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the listener stopped")
    }
}

impl std::error::Error for Stopped {}

/// The error that ends a connection at its listener's stop.
pub(crate) fn stopped() -> io::Error {
    io::Error::other(Stopped)
}

/// Whether `error` is [`stopped`]. Only the pipe's listener tells the stop apart.
#[cfg(all(windows, feature = "broker-pipe"))]
pub(crate) fn is_stopped(error: &io::Error) -> bool {
    matches!(error.get_ref(), Some(inner) if inner.is::<Stopped>())
}

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
    /// Tool calls run the engine synchronously on the calling task. The connection is the
    /// engine's session: when it ends, however it ends, the engine deletes its state and cache
    /// ([`McpServer::purge`]).
    pub async fn serve<R, W>(self, reader: R, mut writer: W) -> io::Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = LineReader::new(
            BufReader::with_capacity(READ_CHUNK, reader),
            self.config.message_limit(),
        );
        let outcome = exchange(&mut self.connect(), &mut lines, &mut writer).await;
        self.purge();
        outcome
    }

    /// Deletes the engine's state and cache (chapter 08 §8.4): the snapshot, its evidence and
    /// base frame, the pending regions and the request journal. The engine stays usable and
    /// publishes nothing until the next observation; identifiers are never issued again.
    pub fn purge(&self) {
        self.engine
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .purge();
    }
}

/// What the message loop does with one frame read from the stream.
pub(crate) enum Step {
    /// The input ended: the connection is over.
    End,
    /// Nothing to write (a notification, a client response, a blank line).
    Silent,
    /// One response line, with its `\n`, to write and flush.
    Write(String),
}

/// The protocol of the bounded message loop (ADR-0015) for one frame: a message is handled by
/// the connection, an oversized line is answered with `-32600` and the connection survives it,
/// the end of input ends the loop. Both loops below call it, so they answer alike.
pub(crate) fn step(connection: &mut Connection, frame: Frame) -> Step {
    let response = match frame {
        Frame::Eof => return Step::End,
        Frame::Message(message) => connection.handle_message(&message),
        Frame::Oversized => Some(connection.oversized_message_response()),
    };
    match response {
        Some(mut line) => {
            line.push('\n');
            Step::Write(line)
        }
        None => Step::Silent,
    }
}

/// The bounded message loop (ADR-0015) of one connection over a byte stream: one JSON-RPC
/// message per line in each direction, handled in arrival order, and no incoming line longer
/// than the reader's limit buffered. Returns at end of input or on an I/O error. The stdio
/// server and the broker's connections share it; [`exchange_blocking`] is the same loop over
/// blocking streams.
pub(crate) async fn exchange<R, W>(
    connection: &mut Connection,
    lines: &mut LineReader<R>,
    writer: &mut W,
) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        match step(connection, lines.next_frame().await?) {
            Step::End => break,
            Step::Silent => {}
            Step::Write(line) => {
                writer.write_all(line.as_bytes()).await?;
                writer.flush().await?;
            }
        }
    }
    writer.flush().await
}

/// [`exchange`] over a blocking reader and writer, for a transport that gives each connection a
/// thread (ADR-0043): the same framing, the same [`step`] for every frame, one reply written and
/// flushed at a time.
pub(crate) fn exchange_blocking<R, W>(
    connection: &mut Connection,
    lines: &mut LineReader<R>,
    writer: &mut W,
) -> io::Result<()>
where
    R: io::BufRead,
    W: io::Write,
{
    exchange_blocking_until(connection, lines, writer, &|| false)
}

/// [`exchange_blocking`] with its listener's stop (D40): `stopped` is asked once per frame, as
/// the last act before the frame is handed to the connection (a message, or an oversized line's
/// reply). When it answers true, nothing is handed on or written back and the loop ends with
/// [`stopped`]. A frame that passed the check before the stop runs to its end.
pub(crate) fn exchange_blocking_until<R, W>(
    connection: &mut Connection,
    lines: &mut LineReader<R>,
    writer: &mut W,
    stopped: &dyn Fn() -> bool,
) -> io::Result<()>
where
    R: io::BufRead,
    W: io::Write,
{
    loop {
        let frame = lines.next_frame_blocking()?;
        if frame != Frame::Eof && stopped() {
            return Err(self::stopped());
        }
        match step(connection, frame) {
            Step::End => break,
            Step::Silent => {}
            Step::Write(line) => {
                writer.write_all(line.as_bytes())?;
                writer.flush()?;
            }
        }
    }
    writer.flush()
}
