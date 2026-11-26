//! Unit tests of the listener's parts that need no pipe: the reaper's joins, the accept loop's
//! errors and pause, the connection's reader, and the dispatch check. Each waits on an
//! observable (a count taken under the listener's lock, a channel, a gate), never on elapsed
//! time; where a wait has a deadline, it only turns a hang into a failure.

use std::cell::RefCell;
use std::io::BufReader;
use std::sync::atomic::AtomicUsize;

use serde_json::Value;

use super::*;
use crate::config::McpConfig;
use crate::protocol::{Backend, Connection};
use crate::tools::{self, CallError, ToolOutput};
use crate::transport::LineReader;

/// The longest any test here waits for something it expects; never reached when the code is
/// right.
const PATIENCE: Duration = Duration::from_secs(20);

/// A gate a test holds closed and opens; waiting on it ends when it is open.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    fn pass(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
}

/// Held by a thread-local of a worker thread: its destructor runs after the worker's closure has
/// returned and before its thread has exited, so a join of that thread waits on it. It says when
/// it began, then waits for the gate.
struct ExitHold {
    began: mpsc::Sender<()>,
    gate: Arc<Gate>,
}

impl Drop for ExitHold {
    fn drop(&mut self) {
        let _ = self.began.send(());
        self.gate.pass();
    }
}

thread_local! {
    static EXIT_HOLD: RefCell<Option<ExitHold>> = const { RefCell::new(None) };
}

/// The place an accept takes for a connection, counted as `accept_loop` counts it.
fn charge_connection(shared: &Shared) {
    shared.update(|s| {
        s.connections += 1;
        s.serving += 1;
    });
}

/// R-T2-2 / D40 G1: a worker whose closure has returned, whose connection is closed and whose
/// serving count is gone, but whose thread has not exited, still holds its place and its
/// thread's count; both go only once the reaper's join has returned, exactly once.
#[test]
fn it_29_a_worker_s_place_is_free_only_after_the_reaper_has_joined_it() {
    let shared = Arc::new(Shared::new());
    let (to_reaper, reaper) = start_reaper(&shared).unwrap();
    let gate = Arc::new(Gate::default());
    let (began, exiting) = mpsc::channel();
    charge_connection(&shared);
    {
        let gate = Arc::clone(&gate);
        start_worker(&shared, &to_reaper, false, move |_| {
            EXIT_HOLD.with(|hold| *hold.borrow_mut() = Some(ExitHold { began, gate }));
        });
    }
    exiting
        .recv_timeout(PATIENCE)
        .expect("the worker's thread is exiting");
    let held = shared.lock().stats;
    assert_eq!(
        held.serving, 0,
        "the connection is closed and served no more"
    );
    assert_eq!(
        (held.connections, held.threads, held.joined),
        (1, 1, 0),
        "the place and the thread stay until the join returns"
    );
    gate.open();
    assert!(
        shared.wait_for(Instant::now() + PATIENCE, |s| s.stats.connections == 0),
        "the place comes back after the join"
    );
    let after = shared.lock().stats;
    assert_eq!((after.threads, after.joined), (0, 1), "joined once");
    assert!(shared.lock().workers.is_empty());
    // The reaper ends when its last sender goes, and only then.
    assert!(!shared.lock().reaper_done);
    drop(to_reaper);
    reaper.join().unwrap();
    assert!(shared.lock().reaper_done);
}

#[test]
fn it_30_a_transient_error_of_accepting_is_retried_and_a_fatal_one_stops_the_listener_visibly() {
    let shared = Shared::new();
    let mut script = vec![
        Err(io::Error::from_raw_os_error(1450)),
        Err(io::Error::from_raw_os_error(232)),
        Ok(7_u8),
        Err(io::Error::from_raw_os_error(6)),
    ]
    .into_iter();
    let first = Duration::from_millis(1);
    assert_eq!(
        next_connection(|| script.next().unwrap(), &shared, first),
        Some(7)
    );
    assert_eq!(shared.lock().stats.accept_retries, 2);
    assert_eq!(*shared.failure.lock().unwrap(), None);
    assert_eq!(
        next_connection(|| script.next().unwrap(), &shared, first),
        None
    );
    assert_eq!(
        *shared.failure.lock().unwrap(),
        Some(PipeError::Os {
            kind: io::Error::from_raw_os_error(6).kind(),
            code: Some(6)
        }),
        "a fatal error is recorded, not swallowed"
    );
    // An error without a code (a poisoned lock in the crate) is fatal too.
    let shared = Shared::new();
    assert_eq!(
        next_connection(|| Err::<u8, _>(io::Error::other("lock")), &shared, first),
        None
    );
    assert!(shared.failure.lock().unwrap().is_some());
}

/// D40 G5: the stop's notification ends the accept loop's pause. The loop is known to be in its
/// timed wait before the stop (its entry is counted under the lock the wait releases), the
/// pause is an hour, far longer than the test, and the test asserts why the wait ended, not how
/// long it took: without the notification the loop would stay in its wait and the test fails.
#[test]
fn it_30_the_stop_s_notification_ends_the_pause_between_two_tries_of_accepting() {
    let shared = Arc::new(Shared::new());
    let tries = Arc::new(AtomicUsize::new(0));
    let (ended, outcome) = mpsc::channel();
    {
        let shared = Arc::clone(&shared);
        let tries = Arc::clone(&tries);
        // Not joined: without the notification it would wait out its hour.
        thread::spawn(move || {
            let next = next_connection(
                || {
                    tries.fetch_add(1, Ordering::SeqCst);
                    Err::<u8, _>(io::Error::from_raw_os_error(8))
                },
                &shared,
                Duration::from_secs(3600),
            );
            let _ = ended.send(next);
        });
    }
    assert!(
        shared.wait_for(Instant::now() + PATIENCE, |s| s.pauses == 1),
        "the loop entered its pause"
    );
    shared.stop();
    let next = outcome
        .recv_timeout(PATIENCE)
        .expect("the stop's notification ended the pause");
    assert_eq!(next, None);
    assert_eq!(
        shared.lock().last_pause,
        Some(PauseEnd::Stopped),
        "it ended on the notification, with the stop set"
    );
    assert_eq!(tries.load(Ordering::SeqCst), 1, "no try after the stop");
    assert_eq!(
        *shared.failure.lock().unwrap(),
        None,
        "a stop is no failure"
    );
}

#[test]
fn it_30_a_connection_s_reader_hands_one_line_at_a_time_and_ends_with_the_stop() {
    let shared = Shared::new();
    let input: &[u8] = b"one\r\ntwo\nthree";
    let mut reader = UntilStopped::new(input, &shared);
    let mut buf = [0_u8; 64];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"one\r\n");
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"two\n");
    // A smaller buffer gets part of the line, then the rest.
    let n = reader.read(&mut buf[..2]).unwrap();
    assert_eq!(&buf[..n], b"th");
    assert_eq!(
        shared.lock().stats.bytes_in,
        11,
        "what it handed on is counted"
    );
    shared.stop();
    let stop = reader.read(&mut buf).unwrap_err();
    assert!(
        server::is_stopped(&stop),
        "the stop is its own outcome, not an end of input: {stop:?}"
    );

    // A line begun before the stop is not completed by an end of input: once its bytes have
    // entered the framer (the framer asked for more), the stop comes, and the framer gets the
    // stop, so the line is discarded.
    let shared = Shared::new();
    let gate = Arc::new(ReadGate::default());
    let source = Gated {
        bytes: b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}",
        gate: Arc::clone(&gate),
    };
    let mut lines = LineReader::new(
        BufReader::with_capacity(READ_CHUNK, UntilStopped::new(source, &shared)),
        1024,
    );
    let reading = thread::scope(|scope| {
        let handle = scope.spawn(|| lines.next_frame_blocking());
        gate.wait_for_reads(2);
        shared.stop();
        gate.release();
        handle.join().unwrap()
    });
    let error = reading.unwrap_err();
    assert!(server::is_stopped(&error), "{error:?}");
}

/// A source that hands out its bytes, then blocks on its next read until released, counting its
/// reads, so a test can set the stop after a partial line has entered the framer.
struct Gated {
    bytes: &'static [u8],
    gate: Arc<ReadGate>,
}

/// The count of a [`Gated`] source's reads, and its release.
#[derive(Default)]
struct ReadGate {
    state: Mutex<(usize, bool)>,
    changed: Condvar,
}

impl ReadGate {
    fn wait_for_reads(&self, n: usize) {
        let mut state = self.state.lock().unwrap();
        while state.0 < n {
            state = self.changed.wait(state).unwrap();
        }
    }

    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

impl Read for Gated {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut state = self.gate.state.lock().unwrap();
        state.0 += 1;
        self.gate.changed.notify_all();
        if !self.bytes.is_empty() {
            let n = self.bytes.len().min(buf.len());
            buf[..n].copy_from_slice(&self.bytes[..n]);
            self.bytes = &self.bytes[n..];
            return Ok(n);
        }
        while !state.1 {
            state = self.gate.changed.wait(state).unwrap();
        }
        Ok(0)
    }
}

/// A backend that counts what reaches it.
#[derive(Default)]
struct Counting {
    boundaries: Arc<AtomicUsize>,
}

impl Backend for Counting {
    fn request_boundary(&self) {
        self.boundaries.fetch_add(1, Ordering::SeqCst);
    }

    fn call_tool(&self, _: &str, _: &Value) -> Result<ToolOutput, CallError> {
        Err(CallError::Unavailable)
    }
}

/// D40 G2: a frame held between its read and the dispatch check while the stop is set is not
/// handed to the connection, and nothing is written back.
#[test]
fn it_30_a_frame_held_before_the_dispatch_check_when_the_stop_comes_is_not_dispatched() {
    let boundaries = Arc::new(AtomicUsize::new(0));
    let mut connection = Connection::with_backend(
        Box::new(Counting {
            boundaries: Arc::clone(&boundaries),
        }),
        McpConfig::default(),
        tools::descriptors().into(),
    );
    let input: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
    let mut lines = LineReader::new(BufReader::new(input), 1024);
    let mut written = Vec::new();
    let flag = AtomicBool::new(false);
    let (held, is_held) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let (connection, lines, out, flag_ref) = (&mut connection, &mut lines, &mut written, &flag);
    let outcome = thread::scope(|scope| {
        let serving = scope.spawn(move || {
            // The check: the frame is read and waits here, before the flag is looked at.
            let check = || {
                let _ = held.send(());
                let _ = released.recv();
                flag_ref.load(Ordering::SeqCst)
            };
            server::exchange_blocking_until(connection, lines, out, &check)
        });
        let reached = is_held.recv_timeout(PATIENCE);
        flag.store(true, Ordering::SeqCst);
        drop(release);
        let outcome = serving.join().unwrap();
        reached.expect("the frame reached the dispatch check");
        outcome
    });
    let error = outcome.unwrap_err();
    assert!(server::is_stopped(&error), "{error:?}");
    assert_eq!(
        boundaries.load(Ordering::SeqCst),
        0,
        "nothing reached the backend"
    );
    assert!(written.is_empty(), "nothing was written back");
}
