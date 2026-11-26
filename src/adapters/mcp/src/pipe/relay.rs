//! The relay of the client end (ADR-0043, decisions 8 and 9): one handshake line, then two byte
//! streams copied to and from the pipe at once, without reading what they carry.

use std::io::{self, Read, Write};
use std::sync::{Arc, mpsc};
use std::thread;

use crate::broker::TICKET_MAX_LEN;

/// The buffer of each direction of [`relay`]: the most bytes it holds at once.
pub const RELAY_CHUNK: usize = 8 * 1024;

/// How a [`relay`] ended: the first of its two directions to stop, and why. The other direction
/// is not waited for (see [`relay`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayEnd {
    /// The caller's input ended: the agent's host closed it.
    InputEnded,
    /// The engine closed the pipe.
    PipeEnded,
    /// Reading the caller's input failed.
    InputFailed(io::ErrorKind),
    /// Reading the pipe failed.
    PipeReadFailed(io::ErrorKind),
    /// Writing to the pipe failed, the handshake line included.
    PipeWriteFailed(io::ErrorKind),
    /// Writing the caller's output failed.
    OutputFailed(io::ErrorKind),
}

/// Why [`relay`] wrote nothing: in each case nothing was written to the pipe and nothing was
/// read from the caller's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayError {
    /// The handshake line contains `\n` or `\r`, so it would put a second line on the pipe.
    LineBreak,
    /// The handshake line is longer than [`TICKET_MAX_LEN`], the longest a broker admits.
    TooLong,
    /// A thread of the relay could not be started.
    Thread(io::ErrorKind),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LineBreak => f.write_str("the handshake line contains a line break"),
            Self::TooLong => write!(
                f,
                "the handshake line is longer than {TICKET_MAX_LEN} bytes"
            ),
            Self::Thread(kind) => write!(f, "a relay thread could not be started ({kind:?})"),
        }
    }
}

impl std::error::Error for RelayError {}

/// Relays between the engine's pipe and the caller's two byte streams (ADR-0043, decision 8).
///
/// It refuses, before anything is written, a handshake line that contains `\n` or `\r` or is
/// longer than [`TICKET_MAX_LEN`]; the caller has already validated the line (an empty line, or
/// a ticket). Then it starts its two threads, which wait. Only once both exist does it write
/// `handshake` and a `\n` to the pipe (decision 9) and let them run: one copies `input` to the
/// pipe and the other the pipe to `output`, each with a buffer of [`RELAY_CHUNK`] bytes,
/// flushing `output` after every chunk, until either direction stops; it returns which one and
/// why. When a thread cannot start, or the handshake cannot be written, the threads that
/// started end without reading or writing anything. It never reads, keeps or logs what it
/// copies.
///
/// The direction still running is not waited for: a read cannot be interrupted, so its thread
/// ends when its own source ends or fails (the pipe when the engine closes it; the input when
/// the host closes it), or with the process. The caller is expected to exit once this returns,
/// which closes the pipe, so that the engine sees the client leave.
///
/// # Errors
///
/// [`RelayError`] when the handshake line is refused or a thread cannot be started; nothing is
/// then written to the pipe or read from `input`.
pub fn relay<P, R, W>(
    pipe: Arc<P>,
    handshake: &[u8],
    input: R,
    output: W,
) -> Result<RelayEnd, RelayError>
where
    P: Send + Sync + 'static,
    for<'a> &'a P: Read + Write,
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    relay_with(pipe, handshake, input, output, |name, work| {
        thread::Builder::new()
            .name(name.to_owned())
            .spawn(work)
            .map(drop)
    })
}

/// The work of one direction of the relay, handed to a thread.
type Work = Box<dyn FnOnce() + Send + 'static>;

/// [`relay`] with the way its threads are started given by the caller, so that a test can
/// refuse one.
fn relay_with<P, R, W>(
    pipe: Arc<P>,
    handshake: &[u8],
    input: R,
    output: W,
    mut spawn: impl FnMut(&str, Work) -> io::Result<()>,
) -> Result<RelayEnd, RelayError>
where
    P: Send + Sync + 'static,
    for<'a> &'a P: Read + Write,
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    if handshake.contains(&b'\n') || handshake.contains(&b'\r') {
        return Err(RelayError::LineBreak);
    }
    if handshake.len() > TICKET_MAX_LEN {
        return Err(RelayError::TooLong);
    }
    let (ended, first) = mpsc::channel();
    // Each thread waits for its go; a go sender dropped unsent ends it before it touches its
    // side.
    let (go_up, up_waits) = mpsc::channel::<()>();
    let (go_down, down_waits) = mpsc::channel::<()>();
    let up = Arc::clone(&pipe);
    let handshake_pipe = Arc::clone(&pipe);
    let up_ended = ended.clone();
    spawn(
        "ommatix-relay-to-engine",
        Box::new(move || {
            if up_waits.recv().is_ok() {
                let _ = up_ended.send(to_pipe(input, &up));
            }
        }),
    )
    .map_err(|e| RelayError::Thread(e.kind()))?;
    let down = pipe;
    spawn(
        "ommatix-relay-from-engine",
        Box::new(move || {
            if down_waits.recv().is_ok() {
                let _ = ended.send(from_pipe(&down, output));
            }
        }),
    )
    .map_err(|e| RelayError::Thread(e.kind()))?;
    // Both threads exist: only now is anything written.
    let mut line = Vec::with_capacity(handshake.len() + 1);
    line.extend_from_slice(handshake);
    line.push(b'\n');
    let mut writer: &P = &handshake_pipe;
    if let Err(error) = writer.write_all(&line) {
        return Ok(RelayEnd::PipeWriteFailed(error.kind()));
    }
    let _ = go_up.send(());
    let _ = go_down.send(());
    // Both senders live in the threads, and each sends before it ends: a receive error is not
    // possible, and would mean that both ended without a word, as if the pipe had.
    Ok(first.recv().unwrap_or(RelayEnd::PipeEnded))
}

/// The caller's input to the pipe, until the input ends or a side fails.
fn to_pipe<P, R>(mut input: R, pipe: &P) -> RelayEnd
where
    for<'a> &'a P: Write,
    R: Read,
{
    let mut buffer = vec![0_u8; RELAY_CHUNK];
    let mut writer = pipe;
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => return RelayEnd::InputEnded,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return RelayEnd::InputFailed(error.kind()),
        };
        if let Err(error) = writer.write_all(&buffer[..n]) {
            return RelayEnd::PipeWriteFailed(error.kind());
        }
    }
}

/// The pipe to the caller's output, until the pipe ends or a side fails.
fn from_pipe<P, W>(pipe: &P, mut output: W) -> RelayEnd
where
    for<'a> &'a P: Read,
    W: Write,
{
    let mut buffer = vec![0_u8; RELAY_CHUNK];
    let mut reader = pipe;
    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => return RelayEnd::PipeEnded,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return RelayEnd::PipeReadFailed(error.kind()),
        };
        if let Err(error) = output.write_all(&buffer[..n]).and_then(|()| output.flush()) {
            return RelayEnd::OutputFailed(error.kind());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Condvar, Mutex, PoisonError};

    use super::*;

    /// One direction of an in-memory pipe: bytes written wait until they are read; a read
    /// blocks until there are bytes or the direction is closed.
    #[derive(Default)]
    struct Lane {
        state: Mutex<(VecDeque<u8>, bool)>,
        ready: Condvar,
    }

    impl Lane {
        fn push(&self, bytes: &[u8]) -> io::Result<()> {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.1 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            state.0.extend(bytes);
            self.ready.notify_all();
            Ok(())
        }

        fn pull(&self, buf: &mut [u8]) -> usize {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            while state.0.is_empty() && !state.1 {
                state = self
                    .ready
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            let n = buf.len().min(state.0.len());
            for (slot, byte) in buf.iter_mut().zip(state.0.drain(..n)) {
                *slot = byte;
            }
            n
        }

        fn close(&self) {
            self.state.lock().unwrap_or_else(PoisonError::into_inner).1 = true;
            self.ready.notify_all();
        }
    }

    /// The client end of an in-memory pipe: it writes to the engine's lane and reads from the
    /// client's.
    #[derive(Default)]
    struct Duplex {
        to_engine: Lane,
        to_client: Lane,
    }

    impl Read for &Duplex {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            Ok(self.to_client.pull(buf))
        }
    }

    impl Write for &Duplex {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.to_engine.push(buf)?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A reader of a lane, as the caller's input.
    struct LaneReader(Arc<Lane>);

    impl Read for LaneReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            Ok(self.0.pull(buf))
        }
    }

    /// A writer into a lane, as the caller's output.
    struct LaneWriter(Arc<Lane>);

    impl Write for LaneWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.push(buf)?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Reads a lane until `n` bytes came or it closed.
    fn read_exactly(lane: &Lane, n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0_u8; 4096];
        while out.len() < n {
            let got = lane.pull(&mut buf[..(n - out.len()).min(4096)]);
            if got == 0 {
                break;
            }
            out.extend_from_slice(&buf[..got]);
        }
        out
    }

    #[test]
    fn it_31_a_handshake_with_a_line_break_or_over_the_bound_is_refused_before_anything_is_written()
    {
        for (line, refusal) in [
            (b"abc\n".to_vec(), RelayError::LineBreak),
            (b"\nabc".to_vec(), RelayError::LineBreak),
            (b"ab\rc".to_vec(), RelayError::LineBreak),
            (b"abc\r".to_vec(), RelayError::LineBreak),
            (vec![b'a'; TICKET_MAX_LEN + 1], RelayError::TooLong),
        ] {
            let pipe = Arc::new(Duplex::default());
            let input = Arc::new(Lane::default());
            let output = Arc::new(Lane::default());
            let ended = relay(
                Arc::clone(&pipe),
                &line,
                LaneReader(input),
                LaneWriter(Arc::clone(&output)),
            );
            assert_eq!(ended, Err(refusal));
            pipe.to_engine.close();
            assert!(
                read_exactly(&pipe.to_engine, 1).is_empty(),
                "nothing written"
            );
        }
    }

    #[test]
    fn it_31_the_handshake_line_goes_first_and_alone_and_an_empty_one_is_a_bare_line_end() {
        for line in [&b""[..], &[b'Q'; TICKET_MAX_LEN][..]] {
            let pipe = Arc::new(Duplex::default());
            let input = Arc::new(Lane::default());
            input.push(b"{\"after\":1}\n").unwrap();
            input.close();
            let ended = relay(
                Arc::clone(&pipe),
                line,
                LaneReader(input),
                LaneWriter(Arc::new(Lane::default())),
            );
            assert_eq!(ended, Ok(RelayEnd::InputEnded));
            let mut expected = line.to_vec();
            expected.extend_from_slice(b"\n{\"after\":1}\n");
            assert_eq!(read_exactly(&pipe.to_engine, expected.len()), expected);
        }
    }

    #[test]
    fn it_31_both_directions_flow_at_once_and_either_side_ending_ends_the_relay() {
        // The engine's side answers every line with a long reply while the input is still
        // being written: a relay that copied one direction at a time would stall.
        let pipe = Arc::new(Duplex::default());
        let input = Arc::new(Lane::default());
        let output = Arc::new(Lane::default());
        let engine = {
            let pipe = Arc::clone(&pipe);
            thread::spawn(move || {
                let handshake = read_exactly(&pipe.to_engine, 1);
                assert_eq!(handshake, b"\n");
                let mut line = Vec::new();
                let mut buf = [0_u8; 1];
                let mut replies = 0_u32;
                while pipe.to_engine.pull(&mut buf) == 1 {
                    if buf[0] == b'\n' {
                        let n = 1000 + (line.len() * 37) % 20_000;
                        let mut reply = vec![b'r'; n];
                        reply.push(b'\n');
                        pipe.to_client.push(&reply).unwrap();
                        replies += 1;
                        line.clear();
                    } else {
                        line.push(buf[0]);
                    }
                }
                replies
            })
        };
        let relayed = {
            let (pipe, input, output) =
                (Arc::clone(&pipe), Arc::clone(&input), Arc::clone(&output));
            thread::spawn(move || relay(pipe, b"", LaneReader(input), LaneWriter(output)))
        };
        let mut expected = 0;
        for i in 0..500_usize {
            let request = vec![b'q'; 1 + (i * 131) % 3000];
            input.push(&request).unwrap();
            input.push(b"\n").unwrap();
            expected += 1000 + (request.len() * 37) % 20_000 + 1;
        }
        assert_eq!(read_exactly(&output, expected).len(), expected);
        // The engine closes its side: the relay reports the pipe's end.
        pipe.to_client.close();
        assert_eq!(relayed.join().unwrap(), Ok(RelayEnd::PipeEnded));
        pipe.to_engine.close();
        assert_eq!(engine.join().unwrap(), 500);
        input.close();
    }

    #[test]
    fn a_thread_that_cannot_start_leaves_the_pipe_unwritten_and_the_input_unread() {
        for refused in [1_u32, 2] {
            let pipe = Arc::new(Duplex::default());
            let input = Arc::new(Lane::default());
            input.push(b"request\n").unwrap();
            input.close();
            let mut started = Vec::new();
            let mut calls = 0_u32;
            let ended = relay_with(
                Arc::clone(&pipe),
                b"",
                LaneReader(Arc::clone(&input)),
                LaneWriter(Arc::new(Lane::default())),
                |name, work| {
                    calls += 1;
                    if calls == refused {
                        return Err(io::ErrorKind::OutOfMemory.into());
                    }
                    started.push(thread::Builder::new().name(name.to_owned()).spawn(work)?);
                    Ok(())
                },
            );
            assert_eq!(ended, Err(RelayError::Thread(io::ErrorKind::OutOfMemory)));
            for thread in started {
                thread.join().unwrap();
            }
            pipe.to_engine.close();
            assert!(
                read_exactly(&pipe.to_engine, 1).is_empty(),
                "refusal of thread {refused}: nothing written, not even the handshake"
            );
            assert_eq!(
                read_exactly(&input, 8),
                b"request\n",
                "refusal of thread {refused}: the input unread"
            );
        }
    }

    #[test]
    fn it_31_the_end_of_the_input_and_a_failed_output_are_reported_as_such() {
        let pipe = Arc::new(Duplex::default());
        let input = Arc::new(Lane::default());
        input.close();
        assert_eq!(
            relay(
                Arc::clone(&pipe),
                b"",
                LaneReader(input),
                LaneWriter(Arc::new(Lane::default()))
            ),
            Ok(RelayEnd::InputEnded)
        );
        pipe.to_client.close();

        let pipe = Arc::new(Duplex::default());
        let output = Arc::new(Lane::default());
        output.close();
        pipe.to_client.push(b"reply\n").unwrap();
        let input = Arc::new(Lane::default());
        assert_eq!(
            relay(
                Arc::clone(&pipe),
                b"",
                LaneReader(Arc::clone(&input)),
                LaneWriter(output)
            ),
            Ok(RelayEnd::OutputFailed(io::ErrorKind::BrokenPipe))
        );
        input.close();

        let pipe = Arc::new(Duplex::default());
        pipe.to_engine.close();
        assert_eq!(
            relay(
                Arc::clone(&pipe),
                b"",
                LaneReader(Arc::new(Lane::default())),
                LaneWriter(Arc::new(Lane::default()))
            ),
            Ok(RelayEnd::PipeWriteFailed(io::ErrorKind::BrokenPipe))
        );
    }
}
