//! `attach` (ADR-0043, decisions 8, 9 and 12; proposed): relays this process's standard input
//! and output to the engine that `serve` runs with `[mcp] transport = "pipe"`, so that an MCP
//! host keeps starting a stdio server per agent while every agent reaches the one engine.
//!
//! It reads the same configuration as `serve` and derives the pipe's name the same way
//! ([`Config::pipe_name`]). It reads the ticket, if any, from the file `--ticket-file` names
//! (bounded; exactly one ticket and nothing else, no line break), opens the pipe with the
//! adapter's identification-level client, sends the handshake line (empty without a ticket)
//! and relays both streams with the adapter's relay until either side ends. It starts no
//! process, creates no file, writes nothing on stdout but the relayed bytes, and its one stderr
//! line (printed by `main.rs`) says which side ended or that no engine listens: never the
//! ticket, the content of a file or the traffic.
//!
//! **No stream goes to the pipe crate's flushing thread.** The crate keeps one process-wide
//! thread that flushes a dropped stream it counts as unflushed and only then closes it; a
//! flush waits until the peer has read everything, so one stream dropped behind an engine that
//! does not read would hold every later dropped stream of the process. The server avoids it by
//! dropping every stream with `assume_flushed()`; `attach` does the same: its stream is wrapped
//! so that whichever thread drops it last marks it flushed first ([`Unlingering`]), and the
//! process exits as soon as the relay has settled, without waiting for the direction still
//! blocked on a read.
//!
//! **Nothing goes through Rust's global stdout buffer.** The relay writes the engine's bytes to
//! a duplicate of the standard output's handle, an ordinary file without a buffer
//! (`win::raw_stdout`), and nothing else on the `attach` path writes to `std::io::stdout()`. So
//! no byte can be pending in that buffer at exit, and the runtime's cleanup in
//! `std::process::exit` (pinned 1.99.0: `io::cleanup` takes the stdout lock with `try_lock`
//! and, if it gets it, replaces the line writer, whose drop writes what it held) finds it empty
//! and writes nothing. `main` does not flush stdout either. The exit's bound: at once after the
//! host's input ends; at most `DRAIN_WAIT` (5 s) after the engine's side ends; in both cases
//! on the condition that the one stderr line can be written (stderr is unbuffered, and a host
//! that never drains a full stderr pipe holds that write). What the host had not read when it
//! stopped reading, and what the relay still held, is lost: `attach` does not wait for a host
//! to drain its output.

use std::io;
use std::path::Path;

use anyhow::{Result, bail};
use ommatix::operator_file::{OpenError, open_regular};
use ommatix_adapter_mcp::{TICKET_MAX_LEN, TICKET_MIN_LEN};

use crate::cli::AttachArgs;
use crate::config::Config;

/// Why `attach` was refused before it connected (exit code 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Every instance of the pipe stayed busy through every try.
    Busy,
    /// The engine said it serves the most clients it allows, and closed the connection.
    Full,
    /// The engine closed the connection before it sent anything: it refused the handshake (a
    /// ticket already spent, or a full set of spent tickets), had no place for this client and
    /// no thread left to say so, or stopped first.
    NoAnswer,
}

/// What failed in the relay (exit code 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Reading this process's standard input.
    Input(io::ErrorKind),
    /// Writing this process's standard output.
    Output(io::ErrorKind),
    /// Opening the pipe, with the operating system's code.
    Connect(Option<i32>),
    /// A relay thread could not be started: the relay starts both before it writes the
    /// handshake, so nothing was sent to the engine and nothing read from the input.
    Thread,
}

/// How `attach` ended. Its exit code tells "ended normally" (0) from "no engine" (3) from
/// "refused" (4) from "the engine ended it" (5) from a failure (6); a refusal to start is the
/// error of [`run`] (1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The agent's host closed this process's input: the normal end.
    InputEnded,
    /// No pipe of that name: no engine listens.
    NoEngine,
    /// The engine refused this client, or closed before answering.
    Refused(Refusal),
    /// The engine ended the connection after it had answered.
    EngineEnded,
    /// The relay failed.
    Failed(Failure),
}

impl Outcome {
    /// The process's exit code.
    pub const fn code(self) -> i32 {
        match self {
            Self::InputEnded => 0,
            Self::NoEngine => 3,
            Self::Refused(_) => 4,
            Self::EngineEnded => 5,
            Self::Failed(_) => 6,
        }
    }

    /// The one stderr line, after `ommatix: attach: `. It names a side and a cause, never a
    /// ticket, a path's content or traffic.
    pub fn message(self) -> String {
        match self {
            Self::InputEnded => "the agent's host closed the input; detached".to_owned(),
            Self::NoEngine => "no engine is listening on the pipe; start `ommatix serve` with \
                               [mcp] transport = \"pipe\" (attach starts no engine)"
                .to_owned(),
            Self::Refused(Refusal::Busy) => {
                "refused: every instance of the pipe stayed busy; try again".to_owned()
            }
            Self::Refused(Refusal::Full) => {
                "refused: the engine serves the most clients it allows".to_owned()
            }
            Self::Refused(Refusal::NoAnswer) => "refused: the engine closed the connection \
                                                  before answering (a ticket already used, no \
                                                  place for another client, or the engine \
                                                  stopped)"
                .to_owned(),
            Self::EngineEnded => "the engine ended the connection".to_owned(),
            Self::Failed(Failure::Input(kind)) => format!("reading the input failed ({kind:?})"),
            Self::Failed(Failure::Output(kind)) => {
                format!("writing the output failed ({kind:?})")
            }
            Self::Failed(Failure::Connect(Some(code))) => {
                format!("the pipe could not be opened (os error {code})")
            }
            Self::Failed(Failure::Connect(None)) => "the pipe could not be opened".to_owned(),
            Self::Failed(Failure::Thread) => {
                "a relay thread could not be started; nothing was sent to the engine or read \
                 from the input"
                    .to_owned()
            }
        }
    }
}

/// A ticket read from its file: the bytes are overwritten before they are freed, and neither
/// `Debug` nor anything else here shows them.
pub struct Ticket(Vec<u8>);

impl Ticket {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ticket(..)")
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

/// Why a ticket file was refused. Nothing of the file's content is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketError {
    /// The file could not be opened or read.
    Unreadable(io::ErrorKind),
    /// The path opened, but not as a regular file on a disk (a directory, a device or a named
    /// pipe), or its kind could not be read: refused before anything is read.
    NotARegularFile,
    /// The file is longer than a ticket.
    TooLarge,
    /// The file is not exactly one ticket.
    NotATicket,
}

impl std::fmt::Display for TicketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(kind) => write!(f, "the ticket file cannot be read ({kind:?})"),
            Self::NotARegularFile => write!(
                f,
                "the ticket file is not a regular file on a disk (a directory, a device or a                  named pipe is refused before anything is read)"
            ),
            Self::TooLarge => write!(
                f,
                "the ticket file is longer than a ticket ({TICKET_MAX_LEN} bytes)"
            ),
            Self::NotATicket => write!(
                f,
                "the ticket file must hold exactly one ticket: {TICKET_MIN_LEN} to \
                 {TICKET_MAX_LEN} characters of A-Z, a-z, 0-9, '-' and '_', with nothing else, \
                 not even a line break"
            ),
        }
    }
}

impl std::error::Error for TicketError {}

/// Whether `bytes` is exactly one ticket (D33): 32 to 128 characters of the base64url alphabet,
/// without padding, and nothing else.
pub fn is_ticket(bytes: &[u8]) -> bool {
    (TICKET_MIN_LEN..=TICKET_MAX_LEN).contains(&bytes.len())
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Reads the ticket file: one open, the handle's kind checked before any read (a regular file on
/// a disk: [`open_regular`]), at most one byte more than the longest ticket, nothing written;
/// refused unless its whole content is one ticket. The path is the operator's, trusted for
/// integrity; the open is an ordinary one. A stated limit: the open, the queries and the read can
/// each block for as long as the operator's filesystem does.
///
/// # Errors
///
/// [`TicketError`], which carries nothing of the content.
pub fn read_ticket(path: &Path) -> Result<Ticket, TicketError> {
    use std::io::Read;

    let file = open_regular(path).map_err(|e| match e {
        OpenError::NotFound => TicketError::Unreadable(io::ErrorKind::NotFound),
        OpenError::Unopenable(kind) => TicketError::Unreadable(kind),
        OpenError::NotARegularFile => TicketError::NotARegularFile,
    })?;
    let mut bytes = Ticket(Vec::with_capacity(TICKET_MAX_LEN + 1));
    file.take(TICKET_MAX_LEN as u64 + 1)
        .read_to_end(&mut bytes.0)
        .map_err(|e| TicketError::Unreadable(e.kind()))?;
    if bytes.0.len() > TICKET_MAX_LEN {
        return Err(TicketError::TooLarge);
    }
    if !is_ticket(&bytes.0) {
        return Err(TicketError::NotATicket);
    }
    Ok(bytes)
}

/// The checks of `attach` that need nothing opened: the configuration says `pipe`, the name
/// derives, and the ticket file holds a ticket.
///
/// The configuration is read with [`Config::load_closed`]: a file that cannot be used is
/// reported by the path the operator gave and a closed cause (unreadable; not valid TOML, with
/// line and column numbers only; the keys that fail their rule), never by a line or value of the
/// file. The path is the operator's, trusted for integrity, like the ticket's.
///
/// # Errors
///
/// Why `attach` refuses to start (exit code 1).
pub fn prepare(args: &AttachArgs) -> Result<(String, Option<Ticket>)> {
    let config = match args.config.as_deref() {
        Some(path) => Config::load_closed(path)
            .map_err(|problem| anyhow::anyhow!("--config {}: {problem}", path.display()))?,
        None => Config::default(),
    };
    if !config.pipe() {
        bail!(
            "attach needs [mcp] transport = \"pipe\" in the configuration, the one `serve` \
             reads: with \"stdio\" there is no pipe to attach to"
        );
    }
    let name = config.pipe_name()?;
    let ticket = match &args.ticket_file {
        Some(path) => Some(read_ticket(path).map_err(|e| anyhow::anyhow!("--ticket-file: {e}"))?),
        None => None,
    };
    Ok((name, ticket))
}

/// Runs `attach`: prepares, connects and relays until either side ends.
///
/// # Errors
///
/// Why `attach` refused to start (exit code 1); nothing was opened or sent.
pub fn run(args: &AttachArgs) -> Result<Outcome> {
    let (name, ticket) = prepare(args)?;
    relay_to(&name, ticket)
}

#[cfg(not(windows))]
fn relay_to(_: &str, _: Option<Ticket>) -> Result<Outcome> {
    bail!("attach is for Windows only (ADR-0043, decision 11)")
}

#[cfg(windows)]
fn relay_to(name: &str, ticket: Option<Ticket>) -> Result<Outcome> {
    use std::sync::Arc;

    use ommatix_adapter_mcp::pipe::{
        CONNECT_ATTEMPTS, CONNECT_PAUSE, ConnectError, PipeName, RelayError, connect, relay,
    };

    let name = PipeName::new(name).map_err(|e| anyhow::anyhow!("[mcp] pipe_name: {e}"))?;
    // Before the pipe is opened: without an output there is nothing to relay to.
    let output = match win::raw_stdout() {
        Ok(output) => output,
        Err(error) => return Ok(Outcome::Failed(Failure::Output(error.kind()))),
    };
    let stream = match connect(&name, CONNECT_ATTEMPTS, CONNECT_PAUSE) {
        Ok(stream) => stream,
        Err(ConnectError::NoEngine) => return Ok(Outcome::NoEngine),
        Err(ConnectError::Busy) => return Ok(Outcome::Refused(Refusal::Busy)),
        Err(ConnectError::Os { code, .. }) => return Ok(Outcome::Failed(Failure::Connect(code))),
    };
    let pipe = Arc::new(win::Unlingering(stream));
    let (output, watch) = watch::Watched::new(output);
    let line: &[u8] = match &ticket {
        Some(ticket) => ticket.bytes(),
        None => &[],
    };
    let relayed = relay(pipe, line, io::stdin(), output);
    drop(ticket);
    match relayed {
        Ok(end) => Ok(watch::classify(end, &watch)),
        Err(RelayError::Thread(_)) => Ok(Outcome::Failed(Failure::Thread)),
        // The ticket was checked by `read_ticket`, and an empty line passes: neither can be
        // refused for its form, so this is a defect of ours, refused without sending anything.
        Err(error @ (RelayError::LineBreak | RelayError::TooLong)) => {
            bail!("the handshake line was refused before it was sent: {error}")
        }
    }
}

#[cfg(windows)]
mod win {
    use std::io::{self, Read, Write};

    use ommatix_adapter_mcp::pipe::PipeStream;

    /// The client stream, marked flushed right before it is dropped by whichever relay thread
    /// drops it last, so that it is closed at once and never handed to the pipe crate's
    /// process-wide flushing thread (see the module's documentation). What was written is in
    /// the pipe already; nothing waits for the engine to read it.
    #[derive(Debug)]
    pub struct Unlingering(pub PipeStream);

    impl Read for &Unlingering {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut stream = &self.0;
            stream.read(buf)
        }
    }

    impl Write for &Unlingering {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut stream = &self.0;
            stream.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            // A flush waits until the engine has read everything: the relay never needs it.
            Ok(())
        }
    }

    impl Drop for Unlingering {
        fn drop(&mut self) {
            self.0.assume_flushed();
        }
    }

    /// The process's standard output as a file of its own: a duplicate of its handle, written
    /// with no buffer of Rust's in between (see the module's documentation). This is the one
    /// place of the `attach` path that touches `std::io::stdout()`, and only for its handle.
    ///
    /// # Errors
    ///
    /// When the process has no standard output, or its handle cannot be duplicated.
    pub fn raw_stdout() -> io::Result<std::fs::File> {
        use std::os::windows::io::AsHandle;

        let stdout = io::stdout();
        let handle = stdout.as_handle().try_clone_to_owned()?;
        Ok(std::fs::File::from(handle))
    }
}

/// What `attach` learns from the bytes it relays to its output: how many there were, and
/// whether they were exactly the engine's "full" line. It compares them with that constant
/// and keeps nothing of them.
#[cfg(windows)]
mod watch {
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::time::Duration;

    use ommatix_adapter_mcp::pipe::{FULL_LINE, RelayEnd};

    use super::{Failure, Outcome, Refusal};

    /// How long `attach` waits, once the engine's side has ended, for the bytes already on
    /// their way to its output to be written.
    pub const DRAIN_WAIT: Duration = Duration::from_secs(5);

    #[derive(Debug, Default)]
    pub struct Watch {
        bytes: AtomicU64,
        /// How many leading bytes matched `FULL_LINE` and its line end, and whether a byte
        /// differed or came after it.
        full: Mutex<(usize, bool)>,
        done: Mutex<bool>,
        ended: Condvar,
    }

    impl Watch {
        pub fn bytes(&self) -> u64 {
            self.bytes.load(Ordering::SeqCst)
        }

        pub fn full_line(&self) -> bool {
            let (matched, differed) = *self.full.lock().unwrap_or_else(PoisonError::into_inner);
            !differed && matched == FULL_LINE.len() + 1
        }

        /// Waits at most `wait` for the output to be dropped: the relay's direction from the
        /// engine has then ended, and everything it read is written.
        pub fn wait_done(&self, wait: Duration) {
            let done = self.done.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = self
                .ended
                .wait_timeout_while(done, wait, |done| !*done)
                .unwrap_or_else(PoisonError::into_inner);
        }

        fn saw(&self, bytes: &[u8]) {
            self.bytes.fetch_add(bytes.len() as u64, Ordering::SeqCst);
            let expected = FULL_LINE.as_bytes();
            let mut full = self.full.lock().unwrap_or_else(PoisonError::into_inner);
            for &byte in bytes {
                let (matched, differed) = &mut *full;
                if *differed {
                    break;
                }
                // The line, then its line end, then nothing more.
                let next = expected
                    .get(*matched)
                    .copied()
                    .or_else(|| (*matched == expected.len()).then_some(b'\n'));
                if next == Some(byte) {
                    *matched += 1;
                } else {
                    *differed = true;
                }
            }
        }
    }

    /// The relay's output, watched.
    #[derive(Debug)]
    pub struct Watched<W: Write> {
        inner: W,
        watch: Arc<Watch>,
    }

    impl<W: Write> Watched<W> {
        pub fn new(inner: W) -> (Self, Arc<Watch>) {
            let watch = Arc::new(Watch::default());
            (
                Self {
                    inner,
                    watch: Arc::clone(&watch),
                },
                watch,
            )
        }
    }

    impl<W: Write> Write for Watched<W> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = self.inner.write(buf)?;
            self.watch.saw(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    impl<W: Write> Drop for Watched<W> {
        fn drop(&mut self) {
            let _ = self.inner.flush();
            *self
                .watch
                .done
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = true;
            self.watch.ended.notify_all();
        }
    }

    /// How the relay ended, as `attach` reports it. When the engine's side ended, it waits for
    /// the bytes already read from the pipe to reach the output (at most [`DRAIN_WAIT`]), then
    /// tells a refusal (nothing came, or only the full line) from an end after answers.
    pub fn classify(end: RelayEnd, watch: &Watch) -> Outcome {
        match end {
            RelayEnd::InputEnded => Outcome::InputEnded,
            RelayEnd::InputFailed(kind) => Outcome::Failed(Failure::Input(kind)),
            RelayEnd::OutputFailed(kind) => Outcome::Failed(Failure::Output(kind)),
            RelayEnd::PipeEnded | RelayEnd::PipeReadFailed(_) | RelayEnd::PipeWriteFailed(_) => {
                watch.wait_done(DRAIN_WAIT);
                if watch.bytes() == 0 {
                    Outcome::Refused(Refusal::NoAnswer)
                } else if watch.full_line() {
                    Outcome::Refused(Refusal::Full)
                } else {
                    Outcome::EngineEnded
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn watched(chunks: &[&[u8]]) -> Arc<Watch> {
            let (mut output, watch) = Watched::new(Vec::new());
            for chunk in chunks {
                output.write_all(chunk).unwrap();
            }
            drop(output);
            watch
        }

        #[test]
        fn ut_79_the_end_of_the_engine_side_is_told_by_what_came_before_it() {
            let full = format!("{FULL_LINE}\n");
            let (head, tail) = full.as_bytes().split_at(20);
            for end in [
                RelayEnd::PipeEnded,
                RelayEnd::PipeReadFailed(io::ErrorKind::BrokenPipe),
                RelayEnd::PipeWriteFailed(io::ErrorKind::BrokenPipe),
            ] {
                assert_eq!(
                    classify(end, &watched(&[])),
                    Outcome::Refused(Refusal::NoAnswer)
                );
                assert_eq!(
                    classify(end, &watched(&[head, tail])),
                    Outcome::Refused(Refusal::Full)
                );
                // An answer, or the full line with anything after or around it, is an end
                // after answering.
                for chunks in [
                    vec![b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n".as_slice()],
                    vec![full.as_bytes(), b"x"],
                    vec![FULL_LINE.as_bytes()],
                    vec![b" ", full.as_bytes()],
                ] {
                    assert_eq!(classify(end, &watched(&chunks)), Outcome::EngineEnded);
                }
            }
            assert_eq!(
                classify(RelayEnd::InputEnded, &watched(&[b"x"])),
                Outcome::InputEnded
            );
            assert_eq!(
                classify(
                    RelayEnd::OutputFailed(io::ErrorKind::BrokenPipe),
                    &watched(&[])
                ),
                Outcome::Failed(Failure::Output(io::ErrorKind::BrokenPipe))
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICKET: &str = "Qv7Tkt_secret-0123456789abcdefABCDEF";

    struct Dir(std::path::PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ommatix-attach-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn ut_79_a_ticket_file_is_exactly_one_ticket_or_it_is_refused_unsent() {
        let dir = Dir::new("ticket");
        let min = "a".repeat(TICKET_MIN_LEN);
        let max = "Z".repeat(TICKET_MAX_LEN);
        for good in [
            TICKET,
            &min,
            &max,
            "-_0123456789-_ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        ] {
            let ticket = read_ticket(&dir.file("good", good.as_bytes())).unwrap();
            assert_eq!(ticket.bytes(), good.as_bytes());
            assert_eq!(format!("{ticket:?}"), "Ticket(..)");
        }
        let cases: Vec<(Vec<u8>, TicketError)> = vec![
            (format!("{TICKET}\n").into_bytes(), TicketError::NotATicket),
            (
                format!("{TICKET}\r\n").into_bytes(),
                TicketError::NotATicket,
            ),
            (format!("{TICKET}\r").into_bytes(), TicketError::NotATicket),
            (format!("\n{TICKET}").into_bytes(), TicketError::NotATicket),
            (format!("{TICKET} ").into_bytes(), TicketError::NotATicket),
            (
                format!("\u{feff}{TICKET}").into_bytes(),
                TicketError::NotATicket,
            ),
            (format!("{TICKET}=").into_bytes(), TicketError::NotATicket),
            (
                TICKET.replace('_', "+").into_bytes(),
                TicketError::NotATicket,
            ),
            (
                TICKET.replace('_', "/").into_bytes(),
                TicketError::NotATicket,
            ),
            (
                "a".repeat(TICKET_MIN_LEN - 1).into_bytes(),
                TicketError::NotATicket,
            ),
            (Vec::new(), TicketError::NotATicket),
            (
                "a".repeat(TICKET_MAX_LEN + 1).into_bytes(),
                TicketError::TooLarge,
            ),
            (vec![b'a'; 1 << 20], TicketError::TooLarge),
            (b"\xff".repeat(40), TicketError::NotATicket),
        ];
        for (bytes, expected) in cases {
            let path = dir.file("bad", &bytes);
            let refused = read_ticket(&path).unwrap_err();
            assert_eq!(refused, expected, "{:?}", String::from_utf8_lossy(&bytes));
            let shown = format!("{refused} {refused:?}");
            assert!(!shown.contains("Qv7"), "{shown}");
        }
        assert!(matches!(
            read_ticket(&dir.0.join("absent")),
            Err(TicketError::Unreadable(io::ErrorKind::NotFound))
        ));
        // A device that opens is not a file: refused before any read.
        #[cfg(windows)]
        assert_eq!(
            read_ticket(Path::new("NUL")).unwrap_err(),
            TicketError::NotARegularFile
        );
    }

    #[test]
    fn ut_79_attach_refuses_a_configuration_without_the_pipe_and_quotes_no_ticket() {
        let dir = Dir::new("prepare");
        let stdio = dir.file("stdio.toml", b"");
        let ticket = dir.file("ticket", TICKET.as_bytes());
        let args = |config: &std::path::Path, ticket: Option<std::path::PathBuf>| AttachArgs {
            config: Some(config.to_path_buf()),
            ticket_file: ticket,
        };
        let refused = format!(
            "{:#}",
            prepare(&args(&stdio, Some(ticket.clone()))).unwrap_err()
        );
        assert!(refused.contains("transport = \"pipe\""), "{refused}");

        let pipe = dir.file(
            "pipe.toml",
            b"[mcp]\ntransport = \"pipe\"\npipe_name = \"ommatix-unit\"\n",
        );
        let prepared = prepare(&args(&pipe, Some(ticket)));
        if cfg!(windows) {
            let (name, ticket) = prepared.unwrap();
            assert_eq!(name, "ommatix-unit");
            assert_eq!(ticket.unwrap().bytes(), TICKET.as_bytes());
            let (_, none) = prepare(&args(&pipe, None)).unwrap();
            assert!(none.is_none());
            let broken = dir.file("broken", format!("{TICKET}\n").as_bytes());
            let refused = format!("{:#}", prepare(&args(&pipe, Some(broken))).unwrap_err());
            assert!(
                refused.contains("--ticket-file") && !refused.contains("Qv7"),
                "{refused}"
            );
        } else {
            let refused = format!("{:#}", prepared.unwrap_err());
            assert!(refused.contains("Windows build"), "{refused}");
        }
    }

    #[test]
    fn ut_79_the_exit_codes_tell_the_ends_apart_and_the_messages_quote_nothing() {
        let all = [
            Outcome::InputEnded,
            Outcome::NoEngine,
            Outcome::Refused(Refusal::Busy),
            Outcome::Refused(Refusal::Full),
            Outcome::Refused(Refusal::NoAnswer),
            Outcome::EngineEnded,
            Outcome::Failed(Failure::Input(io::ErrorKind::BrokenPipe)),
            Outcome::Failed(Failure::Output(io::ErrorKind::BrokenPipe)),
            Outcome::Failed(Failure::Connect(Some(5))),
            Outcome::Failed(Failure::Connect(None)),
            Outcome::Failed(Failure::Thread),
        ];
        let codes: Vec<i32> = all.iter().map(|e| e.code()).collect();
        assert_eq!(codes, [0, 3, 4, 4, 4, 5, 6, 6, 6, 6, 6]);
        assert!(Outcome::NoEngine.message().contains("no engine"));
        for ended in all {
            let message = ended.message();
            assert!(!message.is_empty() && !message.contains('\n'), "{message}");
            assert!(!message.contains("  "), "{message:?}");
        }
    }

    /// B1 (the exit's cleanup): nothing on the `attach` path writes through Rust's global
    /// stdout buffer, so no byte, and no partial line, can be pending in it when
    /// `std::process::exit` runs its cleanup, whether or not the relay's thread is blocked. The
    /// only use of `std::io::stdout()` in this module is to duplicate its handle; the relay
    /// writes to that duplicate (the `output` it is given); `run_attach` in `main.rs` touches
    /// stdout nowhere, and neither file prints to stdout.
    #[test]
    fn ut_79_nothing_on_the_attach_path_writes_through_rusts_stdout_buffer() {
        let stdout_call = ["io::std", "out"].concat();
        let prints = [["print", "!("].concat(), ["println", "!("].concat()];
        let code_lines = |text: &str| -> Vec<String> {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("//"))
                .map(str::to_owned)
                .collect()
        };
        let uses = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .filter(|line| {
                    line.contains(&stdout_call)
                        || prints.iter().any(|p| {
                            line.match_indices(p.as_str())
                                .any(|(at, _)| !line[..at].ends_with('e'))
                        })
                })
                .cloned()
                .collect()
        };
        let module = code_lines(include_str!("attach.rs"));
        assert_eq!(
            uses(&module),
            vec![["let stdout = io::std", "out();"].concat()]
        );
        assert!(
            module
                .iter()
                .any(|line| line == "let handle = stdout.as_handle().try_clone_to_owned()?;")
        );
        let main = include_str!("main.rs");
        let from = main.find("fn run_attach(").expect("run_attach");
        let run_attach = &main[from..from + main[from..].find("\n}\n").expect("its end")];
        assert!(run_attach.contains("std::process::exit(ended.code())"));
        assert_eq!(uses(&code_lines(run_attach)), Vec::<String>::new());
    }
}
