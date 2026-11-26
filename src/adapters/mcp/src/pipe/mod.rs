//! A local Windows named pipe that serves [`Broker`](crate::Broker) connections, and the client
//! that opens it (ADR-0043, proposed). Compiled only with the cargo feature `broker-pipe`; its
//! operating-system parts only on Windows. Nothing in the executable uses it yet.
//!
//! - [`PipeServer`] binds a pipe name and serves each connection on its own blocking thread
//!   with [`Broker::serve_sync_with_handshake`](crate::Broker::serve_sync_with_handshake):
//!   first instance only (decision 4), remote clients refused (decision 3), a descriptor that
//!   grants the owner only (decision 5), the broker's bound on clients taken at accept and a
//!   refusal that never blocks it (decision 7), one thread per connection (decision 2).
//! - [`connect`] opens the pipe at identification level, overlapped, and retries a busy pipe a
//!   bounded number of times (decision 12). It is the one place in the product that opens a
//!   handle with `std::fs`, under the one bounded exception to SEC-04's source check.
//! - [`relay`] writes the handshake line (decision 9) and then copies two byte streams to and
//!   from the pipe at once, without reading what it copies.
//!
//! Nothing here writes to `stdout` or `stderr`. Errors are values that carry an error kind and
//! an operating-system code at most: never screen text, agent text or a ticket.

#[cfg(all(windows, feature = "broker-pipe"))]
mod client;
mod relay;
#[cfg(all(windows, feature = "broker-pipe"))]
mod server;

use std::fmt;
use std::io;
use std::time::Duration;

#[cfg(all(windows, feature = "broker-pipe"))]
pub use client::connect;
pub use relay::{RELAY_CHUNK, RelayEnd, RelayError, relay};
#[cfg(all(windows, feature = "broker-pipe"))]
pub use server::{OWNER_ONLY_SDDL, PipeServer, PipeStats, ShutdownReport};

/// The client and server end of the pipe: a duplex byte stream. `&PipeStream` reads and
/// writes, so one thread can read while another writes.
#[cfg(all(windows, feature = "broker-pipe"))]
pub type PipeStream = interprocess::os::windows::named_pipe::DuplexPipeStream<
    interprocess::os::windows::named_pipe::pipe_mode::Bytes,
>;

/// The prefix of every local pipe path.
pub const PIPE_PREFIX: &str = r"\\.\pipe\";

/// The longest name after [`PIPE_PREFIX`]; Windows allows 256 characters for the whole path.
pub const MAX_PIPE_NAME: usize = 128;

/// How many times [`connect`] opens a pipe whose every instance is busy before it gives up
/// (ADR-0043, decision 12). With [`CONNECT_PAUSE`] between tries, about two seconds.
pub const CONNECT_ATTEMPTS: u32 = 40;

/// The pause between two tries of [`connect`] on a busy pipe.
pub const CONNECT_PAUSE: Duration = Duration::from_millis(50);

/// The most connections a [`PipeServer`] tells at once that the engine is full, each on a
/// short-lived thread of its own (ADR-0043, decision 7). Past it, a connection beyond the bound
/// is closed with no line.
pub const MAX_REFUSING: usize = 4;

/// The one line a connection beyond the bound on clients receives before it is closed: a
/// JSON-RPC error without an identifier, since no request was read, with the code of an
/// internal error (chapter 07's codes; no new one).
pub const FULL_LINE: &str = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error: the engine serves the most clients it allows; nothing was read"}}"#;

/// A pipe's name: [`PIPE_PREFIX`], which the type adds, and a name of 1 to [`MAX_PIPE_NAME`]
/// ASCII letters, digits, `-`, `_` and `.`. With no `\`, `/` or `:` the path can be nothing but
/// a local pipe (never a file, a device or a remote pipe), and the client file opens nothing
/// else; with ASCII only it reads the same in every code page; 128 characters leave the whole
/// path well under Windows' 256. Which name an engine uses (fixed per user session, ADR-0043
/// decision 4) is chosen by the caller (D38).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PipeName {
    path: String,
}

impl PipeName {
    /// The pipe `\\.\pipe\<name>`.
    ///
    /// # Errors
    ///
    /// [`PipeError::InvalidName`] for an empty name, a longer one, or one with any other
    /// character (a path separator included).
    pub fn new(name: &str) -> Result<Self, PipeError> {
        let valid = (1..=MAX_PIPE_NAME).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if !valid {
            return Err(PipeError::InvalidName);
        }
        Ok(Self {
            path: format!("{PIPE_PREFIX}{name}"),
        })
    }

    /// The whole path, `\\.\pipe\<name>`.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The name after [`PIPE_PREFIX`].
    pub fn name(&self) -> &str {
        &self.path[PIPE_PREFIX.len()..]
    }
}

/// Why a [`PipeServer`] could not start or stopped serving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeError {
    /// The name is not one [`PipeName::new`] accepts.
    InvalidName,
    /// The name is taken: a pipe of that name already exists, so this listener cannot create
    /// its first instance (ADR-0043, decision 4). Another engine of this user session serves
    /// it, or another process took the name first.
    NameTaken,
    /// The security descriptor could not be built.
    Descriptor,
    /// A thread could not be started.
    Thread(io::ErrorKind),
    /// The operating system refused another call, with its error kind and code.
    Os {
        kind: io::ErrorKind,
        code: Option<i32>,
    },
}

impl PipeError {
    /// The error of creating the first instance: access denied (5) means the name exists.
    pub(crate) fn of_bind(error: &io::Error) -> Self {
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
            Self::NameTaken
        } else {
            Self::of_io(error)
        }
    }

    /// An operating-system error, by kind and code only.
    pub(crate) fn of_io(error: &io::Error) -> Self {
        Self::Os {
            kind: error.kind(),
            code: error.raw_os_error(),
        }
    }
}

impl fmt::Display for PipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => write!(
                f,
                "a pipe name is 1 to {MAX_PIPE_NAME} ASCII letters, digits, '-', '_' or '.'"
            ),
            Self::NameTaken => f.write_str(
                "the pipe's name is taken: another engine serves it, or another process took it",
            ),
            Self::Descriptor => f.write_str("the pipe's security descriptor could not be built"),
            Self::Thread(kind) => write!(f, "a thread could not be started ({kind:?})"),
            Self::Os { kind, code } => match code {
                Some(code) => write!(f, "the pipe failed: {kind:?} (os error {code})"),
                None => write!(f, "the pipe failed: {kind:?}"),
            },
        }
    }
}

impl std::error::Error for PipeError {}

/// Why [`connect`] did not connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    /// No pipe of that name exists: no engine is listening (os error 2). Never retried.
    NoEngine,
    /// Every instance of the pipe stayed busy (os error 231) through every try.
    Busy,
    /// Any other failure, with its error kind and code.
    Os {
        kind: io::ErrorKind,
        code: Option<i32>,
    },
}

impl ConnectError {
    /// The error of the last open.
    pub(crate) fn of_open(error: &io::Error) -> Self {
        match error.raw_os_error() {
            Some(ERROR_FILE_NOT_FOUND) => Self::NoEngine,
            Some(ERROR_PIPE_BUSY) => Self::Busy,
            code => Self::Os {
                kind: error.kind(),
                code,
            },
        }
    }
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEngine => f.write_str("no engine is listening on the pipe"),
            Self::Busy => f.write_str("every instance of the pipe stayed busy; try again"),
            Self::Os { kind, code } => match code {
                Some(code) => write!(
                    f,
                    "the pipe could not be opened: {kind:?} (os error {code})"
                ),
                None => write!(f, "the pipe could not be opened: {kind:?}"),
            },
        }
    }
}

impl std::error::Error for ConnectError {}

/// `ERROR_FILE_NOT_FOUND`: no pipe of that name.
pub(crate) const ERROR_FILE_NOT_FOUND: i32 = 2;
/// `ERROR_ACCESS_DENIED`: on creating a first instance, a pipe of that name exists.
pub(crate) const ERROR_ACCESS_DENIED: i32 = 5;
/// `ERROR_PIPE_BUSY`: every instance of the pipe is connected.
pub(crate) const ERROR_PIPE_BUSY: i32 = 231;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_28_a_pipe_name_is_a_local_pipe_and_nothing_else() {
        let name = PipeName::new("ommatix-broker.1_A").unwrap();
        assert_eq!(name.path(), r"\\.\pipe\ommatix-broker.1_A");
        assert_eq!(name.name(), "ommatix-broker.1_A");
        assert!(PipeName::new(&"a".repeat(MAX_PIPE_NAME)).is_ok());
        for bad in [
            "",
            &"a".repeat(MAX_PIPE_NAME + 1),
            r"..\x",
            "a/b",
            r"a\b",
            "a b",
            "a:b",
            "é",
            "a\0",
            "a\n",
        ] {
            assert_eq!(PipeName::new(bad), Err(PipeError::InvalidName), "{bad:?}");
        }
    }

    #[test]
    fn it_26_errors_name_their_cause_by_code_and_never_carry_text_from_outside() {
        let taken = io::Error::from_raw_os_error(ERROR_ACCESS_DENIED);
        assert_eq!(PipeError::of_bind(&taken), PipeError::NameTaken);
        let other = io::Error::from_raw_os_error(87);
        assert_eq!(
            PipeError::of_bind(&other),
            PipeError::Os {
                kind: other.kind(),
                code: Some(87)
            }
        );
        assert_eq!(
            ConnectError::of_open(&io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND)),
            ConnectError::NoEngine
        );
        assert_eq!(
            ConnectError::of_open(&io::Error::from_raw_os_error(ERROR_PIPE_BUSY)),
            ConnectError::Busy
        );
        let custom = io::Error::other("Zephyrquartz ticket Qv7-Tkt_secret");
        let text = format!(
            "{} {:?} {} {:?}",
            PipeError::of_io(&custom),
            PipeError::of_io(&custom),
            ConnectError::of_open(&custom),
            ConnectError::of_open(&custom)
        );
        assert!(
            !text.contains("Zephyrquartz") && !text.contains("Qv7"),
            "{text}"
        );
        assert_eq!(
            ConnectError::NoEngine.to_string(),
            "no engine is listening on the pipe"
        );
    }

    #[test]
    fn it_29_the_full_line_is_one_json_rpc_error_without_an_identifier() {
        assert!(!FULL_LINE.contains('\n') && !FULL_LINE.contains('\r'));
        let value: serde_json::Value = serde_json::from_str(FULL_LINE).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert!(value["id"].is_null());
        assert_eq!(value["error"]["code"], -32603);
        assert!(value.get("result").is_none());
    }
}
