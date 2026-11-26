//! The listener of the pipe (ADR-0043, decisions 2 to 5 and 7): it binds the name, accepts
//! connections on one thread, serves each one on a worker thread of its own through the broker,
//! and joins every worker on one reaper thread before the worker's place is free again.
//!
//! **What the listener guarantees, and who enforces it (D40).**
//!
//! - *The bound on threads.* At any moment the listener's threads that are not joined yet are at
//!   most `1 + 1 + P`: the accept thread, the reaper, and `P` workers, where `P` is the number
//!   of places held (connections plus refusals: at most [`Broker::max_clients`] plus
//!   [`MAX_REFUSING`]). A place is taken at accept and given back **only after
//!   `JoinHandle::join` has returned** for the worker that held it, by the reaper, never by the
//!   worker. The accept thread joins nobody and waits on nobody: when every place is held,
//!   ending workers included, a new connection is refused. The cost: the reaper joins one worker
//!   at a time, so a worker slow to exit delays the return of the places behind it, and
//!   admission refuses meanwhile (it never waits).
//! - *The stop.* There is one stop check per request, made immediately before the request is
//!   handed to the broker (the handshake's admission, and every message after it). No request
//!   passes that check after the stop is set. A request that passed it earlier runs to its end:
//!   the engine's work, a delivered action and its reply may all happen after the stop, and its
//!   connection counts as remaining until its worker is joined. The stop reaches a connection's
//!   reader as a distinct outcome, not as an end of input: a request it had begun to read is
//!   discarded, not handed on. Nothing more is promised. This stop is the listener's; the
//!   operator's stop of actuation (ADR-0007 and its successors) is another thing, independent of
//!   it, and unchanged.
//! - *The dependency's own retry (a stated limit).* In `interprocess` 2.4.9 a client that
//!   connects and leaves before the accept returns makes the accept call disconnect that
//!   instance and wait again, inside the dependency, without this listener's back-off and
//!   without looking at its stop. Each such iteration costs the peer one connection of its own
//!   (no amplification, no loop without the peer's continued work), and only a process the
//!   pipe's descriptor admits (the same user) can cause it. [`PipeStats::accept_retries`] does
//!   not count these iterations and bounds nothing about them. The shutdown's wake-up ends the
//!   wait when it reaches this listener's instance and the accept returns; when no wake-up does
//!   within its budget (another instance of the name takes them), shutdown says
//!   [`ShutdownState::AcceptAlive`], the accept thread stays alive, and the stop takes effect at
//!   the next connection that does reach it.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use interprocess::os::windows::named_pipe::{PipeListener, PipeListenerOptions, pipe_mode};
use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use widestring::U16CString;

use super::{FULL_LINE, MAX_REFUSING, PipeError, PipeName, PipeStream};
use crate::broker::{Admission, Broker, BrokerError, Reservation};
use crate::server::{self, READ_CHUNK};
use ommatix_application::lease::LeaseRefusal;

/// The pipe's security descriptor (ADR-0043, decision 5): protected, one allow entry with every
/// right for the object's owner, and nothing for anyone else.
pub const OWNER_ONLY_SDDL: &str = "D:P(A;;GA;;;OW)";

/// How many wake-up connections [`PipeServer::shutdown`] makes, at most, waiting up to
/// [`WAKE_WAIT`] after each for its accept thread to end.
const WAKE_TRIES: u32 = 20;

/// How long [`PipeServer::shutdown`] waits for the accept thread after each wake-up connection.
const WAKE_WAIT: Duration = Duration::from_millis(100);

/// The first pause before accepting again after a transient error; each further error in a row
/// doubles it, up to [`ACCEPT_RETRY_MAX`].
const ACCEPT_RETRY_FIRST: Duration = Duration::from_millis(10);

/// The longest pause between two tries of accepting after transient errors.
const ACCEPT_RETRY_MAX: Duration = Duration::from_secs(1);

/// Errors that the accept call returns and the listener retries: the system lacked memory,
/// resources or quota for a moment (`ERROR_NOT_ENOUGH_MEMORY` 8, `ERROR_OUTOFMEMORY` 14,
/// `ERROR_NO_SYSTEM_RESOURCES` 1450, `ERROR_NOT_ENOUGH_QUOTA` 1816), which may pass; or one
/// client left while it was being connected (`ERROR_NO_DATA` 232, `ERROR_PIPE_NOT_CONNECTED`
/// 233), which concerns that client and not the listener. The handling of 232 and 233 is a
/// defence: the dependency handles an immediate 232 itself (see the module's documentation),
/// and whether Windows returns either code through the paths that reach this listener was not
/// established. A failed re-creation of the waiting instance keeps the connected one, so a retry
/// still serves its client. Every other error is fatal.
pub const ACCEPT_TRANSIENT_ERRORS: [i32; 6] = [8, 14, 232, 233, 1450, 1816];

type Listener = PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>;

/// What a [`PipeServer`] has done and is doing: counts only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipeStats {
    /// Places held by connection workers: each is taken at accept, before anything is read from
    /// the connection, and given back only after the reaper has joined the worker. Never above
    /// the broker's bound on clients ([`Broker::max_clients`]). This is the bound.
    pub connections: usize,
    /// Places held by refusal workers (connections beyond the bound being told that the engine
    /// is full), given back the same way. Never above [`MAX_REFUSING`].
    pub refusing: usize,
    /// Connections being served now: counted at accept and taken off by the worker once it has
    /// closed its connection. For diagnostics and tests; not the bound.
    pub serving: usize,
    /// The listener's threads not joined yet (the accept thread, the reaper, and the workers),
    /// counted by whoever joins them. Never above `2 + connections + refusing`.
    pub threads: usize,
    /// The most connection places held at once so far.
    pub peak_connections: usize,
    /// Workers joined so far by the reaper.
    pub joined: u64,
    /// Bytes the connections handed to the broker's readers so far.
    pub bytes_in: u64,
    /// Connections given a place at accept, so far.
    pub accepted: u64,
    /// Of those, the ones whose handshake admitted a client.
    pub admitted: u64,
    /// Of those, the ones whose handshake the broker refused.
    pub refused: u64,
    /// Of those, the ones that ended with an error of the stream.
    pub failed: u64,
    /// Of those, the ones that ended at the listener's stop.
    pub stopped: u64,
    /// Connections beyond the bound that were told the engine is full.
    pub told_full: u64,
    /// Connections closed at accept with no line: beyond the bound when no refusal worker is
    /// free, or refused by a broker that is unavailable after a failed call.
    pub closed_full: u64,
    /// Errors that the accept call returned and the listener retried (see
    /// [`ACCEPT_TRANSIENT_ERRORS`]). It does not count the retries inside the dependency.
    pub accept_retries: u64,
}

/// Where a [`PipeServer::shutdown`] left the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    /// Stopped: the accept thread, every worker and the reaper are joined. The pipe's waiting
    /// instance is closed.
    Complete,
    /// Stopped accepting: the accept thread is joined, but these places are still held by
    /// workers not joined yet (still serving, or ending). The listener keeps them, and the
    /// reaper; calling [`PipeServer::shutdown`] again checks again.
    Remaining {
        /// Places of connection workers not joined yet.
        connections: usize,
        /// Places of refusal workers not joined yet.
        refusing: usize,
    },
    /// The accept thread could not be woken and is still alive, waiting on the pipe's instance:
    /// every wake-up connection reached another instance of the name (one that another process
    /// of the same user added) or none was free. It ends at the next connection that reaches it;
    /// it serves none. Calling [`PipeServer::shutdown`] again tries again.
    AcceptAlive,
}

/// What [`PipeServer::shutdown`] left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Where the server stands.
    pub state: ShutdownState,
    /// The counts at the end of the wait.
    pub stats: PipeStats,
    /// The error that stopped the accept loop before the shutdown, if any.
    pub failure: Option<PipeError>,
}

impl ShutdownReport {
    /// True when nothing of the server remains: the accept thread, every worker and the reaper
    /// are joined. The broker's clients leave with their connections.
    pub fn complete(&self) -> bool {
        self.state == ShutdownState::Complete
    }

    /// True when the accept thread is joined, so nobody new is accepted.
    pub fn accept_stopped(&self) -> bool {
        self.state != ShutdownState::AcceptAlive
    }
}

/// How a pause of the accept loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseEnd {
    /// The stop's notification came with the stop set.
    Stopped,
    /// The pause ran out.
    TimedOut,
}

/// One worker, from the moment its place is taken until the reaper has joined it.
struct Worker {
    refusal: bool,
    /// Set once the thread has started; taken by the reaper, which joins it.
    handle: Option<JoinHandle<()>>,
    /// The worker said it is ending before its handle was set: the accept thread hands its
    /// identifier to the reaper again once it sets the handle.
    ended: bool,
}

/// Everything behind the listener's one lock. No join and no I/O happens under it.
#[derive(Default)]
struct State {
    stats: PipeStats,
    /// Raised by every change of `stats` and by the stop, so that a waiter that looked at a
    /// copy can wait only while nothing changed since.
    version: u64,
    /// Every worker not joined yet, with its place.
    workers: BTreeMap<u64, Worker>,
    next_worker: u64,
    /// Whether the reaper's loop has ended (all its senders are gone and every worker joined).
    reaper_done: bool,
    /// Entries of the accept loop into its pause, and how the last one ended (for tests).
    pauses: u64,
    last_pause: Option<PauseEnd>,
}

/// What the accept thread, the reaper and the workers share.
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    stopping: AtomicBool,
    failure: Mutex<Option<PipeError>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: Mutex::default(),
            changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Changes the counts under the lock, raises the version and wakes the waiters.
    fn update(&self, change: impl FnOnce(&mut PipeStats)) {
        let mut state = self.lock();
        change(&mut state.stats);
        state.version += 1;
        self.changed.notify_all();
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Sets the stop and wakes whatever waits, a paused accept loop included. The flag is set
    /// before the lock is taken, so a waiter that looks at it under the lock sees it.
    fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let mut state = self.lock();
        state.version += 1;
        self.changed.notify_all();
    }

    /// Pauses for `pause`, or until the stop's notification comes with the stop set; a wake-up
    /// that is neither (spurious) waits on. The entry is counted under the lock that the wait
    /// releases, so whoever sees the count knows the loop is waiting.
    fn pause_unless_stopped(&self, pause: Duration) -> PauseEnd {
        let deadline = Instant::now() + pause;
        let mut state = self.lock();
        state.pauses += 1;
        self.changed.notify_all();
        let end = loop {
            if self.stopping() {
                break PauseEnd::Stopped;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break PauseEnd::TimedOut;
            };
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        };
        state.last_pause = Some(end);
        end
    }

    fn fail(&self, error: PipeError) {
        *self.failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
        self.update(|_| ());
    }

    /// Waits under the lock until `until` holds, at most until `deadline`; for the listener's
    /// own conditions only, never a caller's.
    fn wait_for(&self, deadline: Instant, until: impl Fn(&State) -> bool) -> bool {
        let mut state = self.lock();
        loop {
            if until(&state) {
                return true;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// Where an accepted connection goes, decided at accept.
enum Place {
    /// Served in the place the broker reserved for it.
    Serve(Reservation),
    Refuse,
    Close,
}

/// Sends a worker's identifier to the reaper when dropped: the last act of the worker's
/// closure, on every exit, a panic included.
struct Ending {
    id: u64,
    reaper: Sender<u64>,
}

impl Drop for Ending {
    fn drop(&mut self) {
        let _ = self.reaper.send(self.id);
    }
}

/// Takes a connection off [`PipeStats::serving`] when dropped, after its connection is closed.
struct Serving<'a>(&'a Shared);

impl Drop for Serving<'_> {
    fn drop(&mut self) {
        self.0.update(|s| s.serving -= 1);
    }
}

/// A server end of the pipe that is closed without the crate's lingering flush on every exit of
/// its holder, unwinding included: whatever is left unread goes nowhere, and the handle never
/// waits in the crate's process-wide flushing pool behind a peer that does not read.
struct Closing(PipeStream);

impl Deref for Closing {
    type Target = PipeStream;

    fn deref(&self) -> &PipeStream {
        &self.0
    }
}

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.assume_flushed();
    }
}

/// A connection's reader for the broker. It hands its reader at most one line per read, so the
/// broker's own buffer never holds a second request, and it counts what it hands on
/// ([`PipeStats::bytes_in`]). Once the server is stopping, every read ends with the stop (an
/// error the listener tells apart), never with an end of input: whatever the broker's reader
/// had accumulated of a request is then discarded with the connection, not handed on.
struct UntilStopped<'a, R> {
    source: R,
    shared: &'a Shared,
    buffer: Box<[u8]>,
    start: usize,
    end: usize,
}

impl<'a, R: Read> UntilStopped<'a, R> {
    fn new(source: R, shared: &'a Shared) -> Self {
        Self {
            source,
            shared,
            buffer: vec![0; READ_CHUNK].into_boxed_slice(),
            start: 0,
            end: 0,
        }
    }
}

impl<R: Read> Read for UntilStopped<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shared.stopping() {
            return Err(server::stopped());
        }
        if self.start == self.end {
            let n = self.source.read(&mut self.buffer)?;
            if self.shared.stopping() {
                return Err(server::stopped());
            }
            if n == 0 {
                return Ok(0);
            }
            (self.start, self.end) = (0, n);
        }
        let waiting = &self.buffer[self.start..self.end];
        let line = waiting
            .iter()
            .position(|&b| b == b'\n')
            .map_or(waiting.len(), |at| at + 1);
        let n = line.min(buf.len());
        buf[..n].copy_from_slice(&waiting[..n]);
        self.start += n;
        self.shared.update(|s| s.bytes_in += n as u64);
        Ok(n)
    }
}

/// Serves a [`Broker`] over a local Windows named pipe (ADR-0043, proposed).
///
/// One thread accepts connections. For each, before anything is read from it, it takes a place
/// among the server's connections and asks the broker for a place among its clients
/// ([`Broker::reserve`]), neither of which waits on anything; a connection given both is served
/// on a worker thread of its own with [`Reservation::serve_sync_with_handshake_until`], from its
/// handshake line to its end. A connection beyond the bound is refused at accept, before
/// anything is read from it: it is told the engine is full ([`super::FULL_LINE`]) on a
/// refusal worker of its own, so that a peer that never reads blocks only that worker; when no
/// refusal worker is free ([`MAX_REFUSING`]), or when the broker is unavailable after a failed
/// call, it is closed with **no** line. The accept thread does nothing else between two
/// accepts. One reaper thread joins every worker and only then gives its place back; see the
/// module's documentation for the bound on threads, the stop and their limits.
///
/// **What it cannot do.** No timeout and no cancel is possible on a blocked read, write or flush
/// through the crate's safe API, and this adapter adds no `unsafe`. So a client that connects
/// and then neither sends nor reads what it is sent holds one place, one thread and the pipe's
/// name until it acts, leaves or the process ends; the bound on clients, counted from accept, is
/// what limits it. For the same reason the handshake has a bound on its size and none on its
/// time. The real end of an engine is the end of its process, which closes every handle.
///
/// The pipe is opened in this crate by one function only, the identification-level client of
/// [`super::connect`]: the server's own wake-up connection at shutdown goes through it too.
///
/// **Dropping the server** without [`PipeServer::shutdown`] sets the stop and makes the
/// bounded wake-up of the accept thread ([`WAKE_TRIES`] connections, [`WAKE_WAIT`] after each),
/// and joins nothing: it does not wait for the workers. The accept thread (once woken, or at
/// the next connection that reaches it), the workers and the reaper then end on their own.
#[allow(missing_debug_implementations)] // holds the broker (trait objects)
pub struct PipeServer {
    name: PipeName,
    shared: Arc<Shared>,
    /// The accept thread, kept until it is joined.
    accept: Option<JoinHandle<()>>,
    /// Disconnected when the accept thread ends: its sender is dropped with the thread.
    accept_ended: Mutex<Receiver<()>>,
    /// The reaper, kept until it is joined.
    reaper: Option<JoinHandle<()>>,
}

impl PipeServer {
    /// Creates the pipe `name` and starts serving `broker` on it, with at most the broker's
    /// bound on clients served at once ([`Broker::max_clients`]).
    ///
    /// The pipe is created as the first instance of its name, so a second server on the same
    /// name, in this process or another, fails with [`PipeError::NameTaken`] (decision 4);
    /// remote clients are refused by the pipe itself (decision 3); and its descriptor is
    /// [`OWNER_ONLY_SDDL`] (decision 5), which grants the owner of the object, the user of an
    /// engine that is not elevated (decision 6), and no one else.
    ///
    /// # Errors
    ///
    /// [`PipeError::NameTaken`], [`PipeError::Descriptor`], [`PipeError::Thread`], or
    /// [`PipeError::Os`] for any other refusal of the operating system.
    pub fn bind(broker: Broker, name: PipeName) -> Result<Self, PipeError> {
        let sddl = U16CString::from_str(OWNER_ONLY_SDDL).map_err(|_| PipeError::Descriptor)?;
        let descriptor =
            SecurityDescriptor::deserialize(&sddl).map_err(|_| PipeError::Descriptor)?;
        let listener: Listener = PipeListenerOptions::new()
            .path(name.path())
            .accept_remote(false)
            .security_descriptor(Some(descriptor))
            .create_duplex::<pipe_mode::Bytes>()
            .map_err(|e| PipeError::of_bind(&e))?;
        let shared = Arc::new(Shared::new());
        // The accept thread and the reaper, counted until they are joined.
        shared.update(|s| s.threads = 2);
        let (to_reaper, reaper) = start_reaper(&shared).map_err(|e| PipeError::Thread(e.kind()))?;
        let (ending, accept_ended) = mpsc::channel::<()>();
        let accept = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("ommatix-pipe-accept".to_owned())
                .spawn(move || {
                    let _ending = ending;
                    accept_loop(&listener, &broker, &shared, &to_reaper);
                })
                // The reaper's last sender went with the closure: it ends on its own.
                .map_err(|e| PipeError::Thread(e.kind()))?
        };
        Ok(Self {
            name,
            shared,
            accept: Some(accept),
            accept_ended: Mutex::new(accept_ended),
            reaper: Some(reaper),
        })
    }

    /// The pipe's name.
    pub fn name(&self) -> &PipeName {
        &self.name
    }

    /// The counts now.
    pub fn stats(&self) -> PipeStats {
        self.shared.lock().stats
    }

    /// Waits until `until` holds for the counts, at most `timeout`; returns the counts then,
    /// or `None` when the time ran out first.
    ///
    /// `until` is called outside the listener's lock, on a copy of the counts, so a predicate
    /// that blocks holds up nothing of the listener. No change is missed between the copy and
    /// the wait: the copy is taken with the counts' version, and the wait goes on only while the
    /// version is the same.
    pub fn wait_until(
        &self,
        timeout: Duration,
        until: impl Fn(&PipeStats) -> bool,
    ) -> Option<PipeStats> {
        let deadline = Instant::now() + timeout;
        loop {
            let (copy, seen) = {
                let guard = self.shared.lock();
                (guard.stats, guard.version)
            };
            if until(&copy) {
                return Some(copy);
            }
            let mut guard = self.shared.lock();
            while guard.version == seen {
                let left = deadline.checked_duration_since(Instant::now())?;
                guard = self
                    .shared
                    .changed
                    .wait_timeout(guard, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        }
    }

    /// The error that stopped the accept loop, if one did: the server then accepts nobody new
    /// ([`PipeServer::accepting`] is false), and [`PipeServer::shutdown`] reports it.
    pub fn failure(&self) -> Option<PipeError> {
        *self
            .shared
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the server accepts connections now: it is not shutting down and its accept
    /// thread has not ended (after a fatal error of accepting, see [`PipeServer::failure`]).
    /// While it retries after a transient error it is still accepting.
    pub fn accepting(&self) -> bool {
        !self.shared.stopping()
            && !matches!(
                self.ended_signal().try_recv(),
                Err(TryRecvError::Disconnected)
            )
    }

    /// Stops the server, and says truthfully what is left ([`ShutdownState`]).
    ///
    /// It stops accepting: the accept thread, blocked until a client connects, is woken by
    /// connections the server makes to its own pipe through the production client
    /// ([`super::connect`]). A wake-up ends the wait when it reaches this listener's instance and
    /// the accept returns; the thread then sees the stop and ends, which closes the waiting
    /// instance. Each wake-up connection is held until the accept thread has ended, and the wait
    /// for it is bounded ([`WAKE_TRIES`] connections, [`WAKE_WAIT`] after each), never a bare
    /// join: another process of the same user may add instances to this pipe (the owner has
    /// every right on it), and a wake-up connection may reach one of those instead. When none
    /// reached the accept thread, the report says [`ShutdownState::AcceptAlive`], the thread is
    /// kept, it stays alive until the next connection that reaches it, and calling this again
    /// tries again.
    ///
    /// The stop is checked once per request, immediately before the request is handed to the
    /// broker: no request passes that check after the stop. A request that passed it earlier
    /// runs to its end (the engine's work, a delivered action and its reply may happen after the
    /// stop), and its connection is remaining until its worker is joined. A request the
    /// connection had begun to read is discarded. A connection blocked on a client that stays
    /// connected without sending or reading cannot be interrupted without `unsafe` (the crate
    /// has no safe call that cancels a read or disconnects an instance); its instance, and so
    /// the name, stay until that client leaves.
    ///
    /// Then it waits up to `wait` for the workers, on what the reaper signals (it never joins a
    /// worker itself), and for the reaper's end, which it joins. When the time runs out with
    /// workers left, the report says [`ShutdownState::Remaining`]: their handles, their places
    /// and the reaper stay with the server, and calling this again checks again and can reach
    /// [`ShutdownState::Complete`].
    pub fn shutdown(&mut self, wait: Duration) -> ShutdownReport {
        self.shared.stop();
        let deadline = Instant::now() + wait;
        let accept_stopped = match self.accept.take() {
            Some(accept) if self.wake_accept() => {
                let _ = accept.join();
                self.shared.update(|s| s.threads -= 1);
                true
            }
            Some(accept) => {
                self.accept = Some(accept);
                false
            }
            None => true,
        };
        if accept_stopped && self.reaper.is_some() {
            let ended = self.shared.wait_for(deadline, |state| {
                state.workers.is_empty() && state.reaper_done
            });
            if ended && let Some(reaper) = self.reaper.take() {
                // The reaper's loop has ended; it is only leaving its thread.
                let _ = reaper.join();
                self.shared.update(|s| s.threads -= 1);
            }
        }
        let stats = self.stats();
        let left = if !accept_stopped {
            ShutdownState::AcceptAlive
        } else if self.reaper.is_none() {
            ShutdownState::Complete
        } else {
            ShutdownState::Remaining {
                connections: stats.connections,
                refusing: stats.refusing,
            }
        };
        ShutdownReport {
            state: left,
            stats,
            failure: self.failure(),
        }
    }

    /// The receiver that disconnects when the accept thread ends.
    fn ended_signal(&self) -> MutexGuard<'_, Receiver<()>> {
        self.accept_ended
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the accept thread has ended, waiting at most `wait` for it.
    fn accept_ended(&self, wait: Duration) -> bool {
        matches!(
            self.ended_signal().recv_timeout(wait),
            Err(RecvTimeoutError::Disconnected)
        )
    }

    /// Wakes the accept thread until it ends or the tries run out; true when it ended.
    fn wake_accept(&self) -> bool {
        let mut wakes: Vec<Closing> = Vec::new();
        let mut ended = self.accept_ended(Duration::ZERO);
        for _ in 0..WAKE_TRIES {
            if ended {
                break;
            }
            // One try, no retry: a busy pipe is tried again on the next round.
            if let Ok(made) = super::connect(&self.name, 1, Duration::ZERO) {
                wakes.push(Closing(made));
            }
            ended = self.accept_ended(WAKE_WAIT);
        }
        // The wake-up connections close here, without the crate's lingering flush.
        drop(wakes);
        ended
    }
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        // Bounded, and nothing is joined: see the type's documentation.
        self.shared.stop();
        if self.accept.is_some() {
            self.wake_accept();
        }
    }
}

/// Starts the reaper: it receives each ending worker's identifier, takes that worker's handle
/// out of the table under the lock, releases the lock, joins the worker, and only then gives the
/// worker's place back. It ends when every sender is gone (the accept thread's and every
/// worker's), after the last join.
fn start_reaper(shared: &Arc<Shared>) -> io::Result<(Sender<u64>, JoinHandle<()>)> {
    let (to_reaper, ending) = mpsc::channel::<u64>();
    let shared = Arc::clone(shared);
    let reaper = thread::Builder::new()
        .name("ommatix-pipe-reaper".to_owned())
        .spawn(move || reap(&shared, &ending))?;
    Ok((to_reaper, reaper))
}

fn reap(shared: &Shared, ending: &Receiver<u64>) {
    for id in ending {
        let handle = {
            let mut state = shared.lock();
            match state.workers.get_mut(&id) {
                Some(worker) => {
                    let handle = worker.handle.take();
                    if handle.is_none() {
                        worker.ended = true;
                    }
                    handle
                }
                None => None,
            }
        };
        let Some(handle) = handle else { continue };
        // Outside every lock: a worker slow to leave its thread delays only the reaper.
        let _ = handle.join();
        let mut state = shared.lock();
        if let Some(worker) = state.workers.remove(&id) {
            if worker.refusal {
                state.stats.refusing -= 1;
            } else {
                state.stats.connections -= 1;
            }
            state.stats.threads -= 1;
            state.stats.joined += 1;
            state.version += 1;
            shared.changed.notify_all();
        }
    }
    let mut state = shared.lock();
    state.reaper_done = true;
    state.version += 1;
    shared.changed.notify_all();
}

/// Accepts until the server stops or accepting fails for good, then drops the listener, which
/// closes the waiting instance.
fn accept_loop(listener: &Listener, broker: &Broker, shared: &Arc<Shared>, reaper: &Sender<u64>) {
    while let Some(stream) = next_connection(|| listener.accept(), shared, ACCEPT_RETRY_FIRST) {
        let stream = Closing(stream);
        if shared.stopping() {
            return;
        }
        // The server's own place first: only this thread takes one, so the count cannot rise
        // under it. A place held by a worker not joined yet is not free.
        let has_place = {
            let mut state = shared.lock();
            let free = state.stats.connections < broker.max_clients();
            if free {
                state.stats.connections += 1;
                state.stats.peak_connections =
                    state.stats.peak_connections.max(state.stats.connections);
                state.version += 1;
                shared.changed.notify_all();
            }
            free
        };
        // Then the broker's, outside the server's lock. It never waits: the broker counts its
        // places apart from the lock that serves calls.
        let reserved = if has_place {
            broker.reserve()
        } else {
            Err(BrokerError::Lease(LeaseRefusal::TooManyClients))
        };
        let place = {
            let mut guard = shared.lock();
            let stats = &mut guard.stats;
            let place = match reserved {
                Ok(reservation) => {
                    stats.accepted += 1;
                    stats.serving += 1;
                    Place::Serve(reservation)
                }
                Err(refusal) => {
                    if has_place {
                        stats.connections -= 1;
                    }
                    match refusal {
                        BrokerError::Lease(LeaseRefusal::TooManyClients)
                            if stats.refusing < MAX_REFUSING =>
                        {
                            stats.refusing += 1;
                            stats.told_full += 1;
                            Place::Refuse
                        }
                        _ => {
                            stats.closed_full += 1;
                            Place::Close
                        }
                    }
                }
            };
            guard.version += 1;
            shared.changed.notify_all();
            place
        };
        match place {
            Place::Serve(reservation) => {
                start_worker(shared, reaper, false, move |shared| {
                    serve(reservation, shared, stream)
                });
            }
            Place::Refuse => start_worker(shared, reaper, true, move |_| refuse(stream)),
            Place::Close => drop(stream),
        }
    }
}

/// The next connection `accept` gives, retrying it after a transient error (see
/// [`ACCEPT_TRANSIENT_ERRORS`]) with a pause from `first_pause`, doubling up to
/// [`ACCEPT_RETRY_MAX`] (or `first_pause` if it is longer), that the stop interrupts. `None` when
/// the server stops, or after a fatal error, which is recorded as the server's failure.
fn next_connection<S>(
    mut accept: impl FnMut() -> io::Result<S>,
    shared: &Shared,
    first_pause: Duration,
) -> Option<S> {
    let mut pause = first_pause;
    loop {
        if shared.stopping() {
            return None;
        }
        match accept() {
            Ok(stream) => return Some(stream),
            Err(_) if shared.stopping() => return None,
            Err(error)
                if error
                    .raw_os_error()
                    .is_some_and(|code| ACCEPT_TRANSIENT_ERRORS.contains(&code)) =>
            {
                shared.update(|s| s.accept_retries += 1);
                if shared.pause_unless_stopped(pause) == PauseEnd::Stopped {
                    return None;
                }
                pause = (pause * 2).min(ACCEPT_RETRY_MAX.max(first_pause));
            }
            Err(error) => {
                // Fail closed: a listener that cannot accept serves nobody new, and says so.
                shared.fail(PipeError::of_io(&error));
                return None;
            }
        }
    }
}

/// Starts the worker of a place the caller has already counted (a connection's or a
/// refusal's). The worker is entered in the table, with its place, before its thread starts; its
/// work hands back the connection when it is done with it and the worker closes it, then (for a
/// connection) leaves [`PipeStats::serving`], then tells the reaper that it is ending, which is
/// the last act of its closure on every exit. The place stays charged until the reaper has
/// joined the thread. When the thread cannot start, nothing ran: the entry, the place and the
/// count go at once, and the work (with the broker's reservation and the connection) is dropped.
fn start_worker<T>(
    shared: &Arc<Shared>,
    reaper: &Sender<u64>,
    refusal: bool,
    work: impl FnOnce(&Shared) -> T + Send + 'static,
) where
    T: 'static,
{
    let id = {
        let mut state = shared.lock();
        let id = state.next_worker;
        state.next_worker += 1;
        state.workers.insert(
            id,
            Worker {
                refusal,
                handle: None,
                ended: false,
            },
        );
        state.stats.threads += 1;
        state.version += 1;
        shared.changed.notify_all();
        id
    };
    let ending = Ending {
        id,
        reaper: reaper.clone(),
    };
    let worker_shared = Arc::clone(shared);
    let started = thread::Builder::new()
        .name(
            if refusal {
                "ommatix-pipe-refusal"
            } else {
                "ommatix-pipe-connection"
            }
            .to_owned(),
        )
        .spawn(move || {
            // Dropped in the reverse order: the serving count, then the reaper is told.
            let _ending = ending;
            let shared = &*worker_shared;
            let _serving = (!refusal).then(|| Serving(shared));
            let connection = work(shared);
            drop(connection);
        });
    if let Ok(handle) = started {
        let again = {
            let mut state = shared.lock();
            state.workers.get_mut(&id).is_some_and(|worker| {
                worker.handle = Some(handle);
                worker.ended
            })
        };
        if again {
            let _ = reaper.send(id);
        }
    } else {
        let mut state = shared.lock();
        state.workers.remove(&id);
        if refusal {
            state.stats.refusing -= 1;
        } else {
            state.stats.connections -= 1;
            state.stats.serving -= 1;
        }
        state.stats.threads -= 1;
        state.version += 1;
        shared.changed.notify_all();
    }
}

/// Serves one connection in its reserved place, from its handshake to its end, and hands the
/// connection back to be closed.
fn serve(reservation: Reservation, shared: &Shared, stream: Closing) -> Closing {
    let reader = UntilStopped::new(&*stream, shared);
    let outcome =
        reservation.serve_sync_with_handshake_until(reader, &*stream, || shared.stopping());
    shared.update(|s| match outcome {
        Ok(Admission::Served) => s.admitted += 1,
        Ok(Admission::Refused(_)) => s.refused += 1,
        Err(error) if server::is_stopped(&error) => s.stopped += 1,
        Err(_) => s.failed += 1,
    });
    stream
}

/// Tells a connection beyond the bound that the engine is full, and hands the connection back
/// to be closed. The flush waits until the peer has read the line, on this worker only.
fn refuse(stream: Closing) -> Closing {
    let mut line = String::with_capacity(FULL_LINE.len() + 1);
    line.push_str(FULL_LINE);
    line.push('\n');
    let mut writer: &PipeStream = &stream;
    let _ = writer
        .write_all(line.as_bytes())
        .and_then(|()| writer.flush());
    stream
}

#[cfg(test)]
mod tests;
