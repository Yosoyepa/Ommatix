//! The listener of the pipe (ADR-0043, decisions 2 to 5 and 7): it binds the name, accepts
//! connections on one thread and serves each one on a thread of its own through the broker.

use std::io::{self, Read, Write};
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use interprocess::os::windows::named_pipe::{PipeListener, PipeListenerOptions, pipe_mode};
use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use widestring::U16CString;

use super::{FULL_LINE, MAX_REFUSING, PipeError, PipeName, PipeStream};
use crate::broker::{Admission, Broker, BrokerError, Reservation};
use crate::server::READ_CHUNK;
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

/// Errors of accepting that the listener retries: the system lacked memory, resources or quota
/// for a moment (`ERROR_NOT_ENOUGH_MEMORY` 8, `ERROR_OUTOFMEMORY` 14,
/// `ERROR_NO_SYSTEM_RESOURCES` 1450, `ERROR_NOT_ENOUGH_QUOTA` 1816), which may pass; or one
/// client left while it was being connected (`ERROR_NO_DATA` 232, `ERROR_PIPE_NOT_CONNECTED`
/// 233), which concerns that client and not the listener: the next accept disconnects the
/// instance and waits on. A failed re-creation of the waiting instance keeps the connected one,
/// so a retry still serves its client. Every other error is fatal.
pub const ACCEPT_TRANSIENT_ERRORS: [i32; 6] = [8, 14, 232, 233, 1450, 1816];

type Listener = PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>;

/// What a [`PipeServer`] has done and is doing: counts only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipeStats {
    /// Places of connections being served: each is taken at accept, before anything is read
    /// from the connection, and given back as the last act of its thread, after the connection
    /// is closed. Never above the broker's bound on clients ([`Broker::max_clients`]).
    pub connections: usize,
    /// Connection threads running now, counted by the threads themselves: each adds itself as
    /// its first act and takes itself off just before it gives its place back.
    pub threads: usize,
    /// The most connection threads that ran at once so far. Never above the broker's bound on
    /// clients.
    pub peak_threads: usize,
    /// Places of connections beyond the bound being told that the engine is full now, each on a
    /// short-lived thread that gives its place back as its last act. Never above
    /// [`MAX_REFUSING`].
    pub refusing: usize,
    /// Connections given a place at accept, so far.
    pub accepted: u64,
    /// Of those, the ones whose handshake admitted a client.
    pub admitted: u64,
    /// Of those, the ones whose handshake the broker refused.
    pub refused: u64,
    /// Of those, the ones that ended with an error of the stream.
    pub failed: u64,
    /// Connections beyond the bound that were told the engine is full.
    pub told_full: u64,
    /// Connections closed at accept with no line: beyond the bound past [`MAX_REFUSING`], or
    /// refused by a broker that is unavailable after a failed call.
    pub closed_full: u64,
    /// Transient errors of accepting (see [`ACCEPT_TRANSIENT_ERRORS`]) after which the listener
    /// accepted again.
    pub accept_retries: u64,
}

/// Where a [`PipeServer::shutdown`] left the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    /// Stopped: the accept thread ended, which closed the pipe's waiting instance, and no
    /// connection or refusal thread is left.
    Complete,
    /// Stopped accepting: the accept thread ended, but these connections are still on their
    /// threads, each until its client sends a byte or leaves (see [`PipeServer::shutdown`]).
    Remaining {
        /// Connections still being served.
        connections: usize,
        /// Connections beyond the bound still being told that the engine is full.
        refusing: usize,
    },
    /// The accept thread could not be woken and is still alive, waiting on the pipe's instance:
    /// every wake-up connection reached another instance of the name (one that another process
    /// of the same user added) or none was free. It ends at the next connection it receives;
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
    /// True when nothing of the server remains: no accept thread, no connection or refusal
    /// thread. The broker's clients leave with their connections.
    pub fn complete(&self) -> bool {
        self.state == ShutdownState::Complete
    }

    /// True when the accept thread has ended, so nobody new is accepted.
    pub fn accept_stopped(&self) -> bool {
        self.state != ShutdownState::AcceptAlive
    }
}

/// What the accept thread and the connection threads share.
struct Shared {
    stats: Mutex<PipeStats>,
    changed: Condvar,
    stopping: AtomicBool,
    failure: Mutex<Option<PipeError>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Shared {
    fn stats(&self) -> MutexGuard<'_, PipeStats> {
        self.stats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, change: impl FnOnce(&mut PipeStats)) {
        change(&mut self.stats());
        self.changed.notify_all();
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Sets the stop and wakes whatever waits on the counts, a paused accept loop included.
    fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        // Taking the lock orders the store before any waiter's next look at the flag.
        self.update(|_| ());
    }

    /// Pauses for `pause`, or less when the server stops; true when it stopped.
    fn pause_unless_stopped(&self, pause: Duration) -> bool {
        let deadline = Instant::now() + pause;
        let mut stats = self.stats();
        loop {
            if self.stopping() {
                return true;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            stats = self
                .changed
                .wait_timeout(stats, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn fail(&self, error: PipeError) {
        *self.failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
        self.changed.notify_all();
    }

    fn keep(&self, handle: JoinHandle<()>) {
        let mut threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        threads.retain(|t| !t.is_finished());
        threads.push(handle);
    }
}

/// Where an accepted connection goes, decided at accept and counted under one lock.
enum Place {
    /// Served in the place the broker reserved for it.
    Serve(Reservation),
    Refuse,
    Close,
}

/// A place counted in [`PipeStats`] (`connections` or `refusing`) for as long as its thread
/// runs. It is given back when it is dropped: the last act of its thread, whatever ends it, a
/// panic included.
struct Slot {
    shared: Arc<Shared>,
    refusal: bool,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let refusal = self.refusal;
        self.shared.update(|s| {
            if refusal {
                s.refusing -= 1;
            } else {
                s.connections -= 1;
            }
        });
    }
}

/// A connection thread's count in [`PipeStats::threads`]: added as the thread's first act and
/// taken off when dropped, just before the thread's [`Slot`].
struct Alive<'a>(&'a Shared);

impl<'a> Alive<'a> {
    fn start(shared: &'a Shared) -> Self {
        shared.update(|s| {
            s.threads += 1;
            s.peak_threads = s.peak_threads.max(s.threads);
        });
        Self(shared)
    }
}

impl Drop for Alive<'_> {
    fn drop(&mut self) {
        self.0.update(|s| s.threads -= 1);
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
/// broker's own buffer never holds a second request, and it reads as the end of input once the
/// server is stopping: after [`PipeServer::shutdown`] no further request reaches the broker,
/// whether it arrives then or arrived before and waits here unread, and the client leaves.
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
            return Ok(0);
        }
        if self.start == self.end {
            let n = self.source.read(&mut self.buffer)?;
            if n == 0 || self.shared.stopping() {
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
        Ok(n)
    }
}

/// Serves a [`Broker`] over a local Windows named pipe (ADR-0043, proposed).
///
/// One thread accepts connections. For each, before anything is read from it, it takes a place
/// among the server's connections and asks the broker for a place among its clients
/// ([`Broker::reserve`]), neither of which waits on anything a client controls; a connection
/// given both is served on a thread of its own with
/// [`Reservation::serve_sync_with_handshake`], from its handshake line to its end. A
/// connection beyond the bound is refused at accept, before anything is read from it: it is
/// told the engine is full ([`super::FULL_LINE`]) on a short-lived thread of its own, so that a
/// peer that never reads blocks only that thread, and past [`MAX_REFUSING`] such threads it is
/// closed with no line. The accept thread does nothing else between two accepts.
///
/// **The bound on threads.** With the broker's bound on clients `n` ([`Broker::max_clients`]),
/// the server runs at most `n + MAX_REFUSING + 1` threads: `n` connection threads, four refusal
/// threads and the accept thread. Each connection or refusal thread holds its place from accept
/// to its very end, after its connection is closed, so threads that are ending count too.
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
/// Dropping the server shuts it down as [`PipeServer::shutdown`] does, without waiting for its
/// connections.
#[allow(missing_debug_implementations)] // holds the broker (trait objects)
pub struct PipeServer {
    name: PipeName,
    shared: Arc<Shared>,
    /// The accept thread, kept until it is seen to end.
    accept: Option<JoinHandle<()>>,
    /// Disconnected when the accept thread ends: its sender is dropped with the thread.
    accept_ended: Receiver<()>,
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
        let shared = Arc::new(Shared {
            stats: Mutex::default(),
            changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            failure: Mutex::new(None),
            threads: Mutex::default(),
        });
        let (ending, accept_ended) = mpsc::channel::<()>();
        let accept = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("ommatix-pipe-accept".to_owned())
                .spawn(move || {
                    let _ending = ending;
                    accept_loop(&listener, &broker, &shared);
                })
                .map_err(|e| PipeError::Thread(e.kind()))?
        };
        Ok(Self {
            name,
            shared,
            accept: Some(accept),
            accept_ended,
        })
    }

    /// The pipe's name.
    pub fn name(&self) -> &PipeName {
        &self.name
    }

    /// The counts now.
    pub fn stats(&self) -> PipeStats {
        *self.shared.stats()
    }

    /// Waits until `until` holds for the counts, at most `timeout`; returns the counts then,
    /// or `None` when the time ran out first.
    pub fn wait_until(
        &self,
        timeout: Duration,
        until: impl Fn(&PipeStats) -> bool,
    ) -> Option<PipeStats> {
        let deadline = Instant::now() + timeout;
        let mut stats = self.shared.stats();
        loop {
            if until(&stats) {
                return Some(*stats);
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            stats = self
                .shared
                .changed
                .wait_timeout(stats, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
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
                self.accept_ended.try_recv(),
                Err(TryRecvError::Disconnected)
            )
    }

    /// Stops the server, and says truthfully what is left ([`ShutdownState`]).
    ///
    /// It stops accepting at once: the accept thread, blocked until a client connects, is
    /// woken by connections the server makes to its own pipe through the production client
    /// ([`super::connect`]), sees that it is stopping and ends, which closes the pipe's waiting
    /// instance. Each wake-up connection is held until the accept thread has ended, and the wait
    /// for it is bounded ([`WAKE_TRIES`] connections, [`WAKE_WAIT`] after each), never a bare
    /// join: another process of the same user may add instances to this pipe (the owner has
    /// every right on it), and a wake-up connection may reach one of those instead. When none
    /// reached the accept thread, the report says [`ShutdownState::AcceptAlive`], the thread is
    /// kept, and calling this again tries again.
    ///
    /// From the stop on, every connection reads its input as ended before each request, so it
    /// serves no further request, not even one it already received: it ends at the next byte
    /// its client sends, when it next looks for a request after the reply it is writing, or
    /// when its client leaves or its pipe breaks, and its client leaves the broker (the last one
    /// purges the engine). A connection blocked on a client that stays connected without
    /// sending or reading cannot be interrupted without `unsafe` (the crate has no safe call
    /// that cancels a read or disconnects an instance); its instance, and so the name, stay until
    /// that client leaves. This waits up to `wait` for every thread to end and reports what is
    /// left; calling it again waits again.
    pub fn shutdown(&mut self, wait: Duration) -> ShutdownReport {
        self.stop(wait)
    }

    /// Whether the accept thread has ended, waiting at most `wait` for it.
    fn accept_ended(&self, wait: Duration) -> bool {
        matches!(
            self.accept_ended.recv_timeout(wait),
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

    fn stop(&mut self, wait: Duration) -> ShutdownReport {
        self.shared.stop();
        let accept_stopped = match self.accept.take() {
            Some(accept) if self.wake_accept() => {
                let _ = accept.join();
                true
            }
            Some(accept) => {
                self.accept = Some(accept);
                false
            }
            None => true,
        };
        let stats = self
            .wait_until(wait, |s| s.connections == 0 && s.refusing == 0)
            .unwrap_or_else(|| self.stats());
        let finished: Vec<JoinHandle<()>> = {
            let mut threads = self
                .shared
                .threads
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (finished, running) = std::mem::take(&mut *threads)
                .into_iter()
                .partition(JoinHandle::is_finished);
            *threads = running;
            finished
        };
        for thread in finished {
            let _ = thread.join();
        }
        let left = if !accept_stopped {
            ShutdownState::AcceptAlive
        } else if stats.connections == 0 && stats.refusing == 0 {
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
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        if self.accept.is_some() {
            self.stop(Duration::ZERO);
        }
    }
}

/// Accepts until the server stops or accepting fails for good, then drops the listener, which
/// closes the waiting instance.
fn accept_loop(listener: &Listener, broker: &Broker, shared: &Arc<Shared>) {
    while let Some(stream) = next_connection(|| listener.accept(), shared) {
        let stream = Closing(stream);
        if shared.stopping() {
            return;
        }
        // The server's own place first: only this thread takes one, so the count cannot change
        // under it. When every place is held by a thread, ending or not, the connection is
        // beyond the bound whatever the broker counts.
        let has_place = {
            let mut stats = shared.stats();
            let free = stats.connections < broker.max_clients();
            if free {
                stats.connections += 1;
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
            let mut stats = shared.stats();
            let place = match reserved {
                Ok(reservation) => {
                    stats.accepted += 1;
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
            shared.changed.notify_all();
            place
        };
        match place {
            Place::Serve(reservation) => {
                spawn(shared, false, move |shared| {
                    serve(reservation, shared, stream)
                });
            }
            Place::Refuse => spawn(shared, true, move |_| refuse(stream)),
            Place::Close => drop(stream),
        }
    }
}

/// The next connection `accept` gives, retrying it after a transient error (see
/// [`ACCEPT_TRANSIENT_ERRORS`]) with a pause that doubles up to [`ACCEPT_RETRY_MAX`] and that
/// the stop interrupts. `None` when the server stops, or after a fatal error, which is recorded
/// as the server's failure.
fn next_connection<S>(mut accept: impl FnMut() -> io::Result<S>, shared: &Shared) -> Option<S> {
    let mut pause = ACCEPT_RETRY_FIRST;
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
                if shared.pause_unless_stopped(pause) {
                    return None;
                }
                pause = (pause * 2).min(ACCEPT_RETRY_MAX);
            }
            Err(error) => {
                // Fail closed: a listener that cannot accept serves nobody new, and says so.
                shared.fail(PipeError::of_io(&error));
                return None;
            }
        }
    }
}

/// Starts the thread of one counted place. The thread holds the place to its very end: its
/// work hands back the connection when it is done with it, and the thread closes it; then the
/// thread takes itself off the live count; then, as its last act, it gives the place back. A
/// work that unwinds drops the connection as it unwinds, and the place goes back after it. When
/// the thread cannot start, the place is given back, the broker's reservation, owned by `work`,
/// is dropped (which frees it) and the connection is closed.
fn spawn<T>(shared: &Arc<Shared>, refusal: bool, work: impl FnOnce(&Shared) -> T + Send + 'static)
where
    T: 'static,
{
    let slot = Slot {
        shared: Arc::clone(shared),
        refusal,
    };
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
            let slot = slot;
            let alive = (!refusal).then(|| Alive::start(&slot.shared));
            let connection = work(&slot.shared);
            // The teardown, in this order: the connection closes, the thread leaves the live
            // count, the place goes back. On an unwind the same order holds (reverse order of
            // declaration).
            drop(connection);
            drop(alive);
            drop(slot);
        });
    if let Ok(handle) = started {
        shared.keep(handle);
    }
}

/// Serves one connection in its reserved place, from its handshake to its end, and hands the
/// connection back to be closed.
fn serve(reservation: Reservation, shared: &Shared, stream: Closing) -> Closing {
    let reader = UntilStopped::new(&*stream, shared);
    let outcome = reservation.serve_sync_with_handshake(reader, &*stream);
    shared.update(|s| match outcome {
        Ok(Admission::Served) => s.admitted += 1,
        Ok(Admission::Refused(_)) => s.refused += 1,
        Err(_) => s.failed += 1,
    });
    stream
}

/// Tells a connection beyond the bound that the engine is full, and hands the connection back
/// to be closed. The flush waits until the peer has read the line, on this thread only.
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
mod tests {
    use super::*;

    fn shared() -> Shared {
        Shared {
            stats: Mutex::default(),
            changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            failure: Mutex::new(None),
            threads: Mutex::default(),
        }
    }

    #[test]
    fn a_transient_error_of_accepting_is_retried_and_a_fatal_one_stops_the_listener_visibly() {
        let shared = shared();
        let mut script = vec![
            Err(io::Error::from_raw_os_error(1450)),
            Err(io::Error::from_raw_os_error(232)),
            Ok(7_u8),
            Err(io::Error::from_raw_os_error(6)),
        ]
        .into_iter();
        assert_eq!(next_connection(|| script.next().unwrap(), &shared), Some(7));
        assert_eq!(shared.stats().accept_retries, 2);
        assert_eq!(*shared.failure.lock().unwrap(), None);
        assert_eq!(next_connection(|| script.next().unwrap(), &shared), None);
        assert_eq!(
            *shared.failure.lock().unwrap(),
            Some(PipeError::Os {
                kind: io::Error::from_raw_os_error(6).kind(),
                code: Some(6)
            }),
            "a fatal error is recorded, not swallowed"
        );
        // An error without a code (a poisoned lock in the crate) is fatal too.
        let shared = self::shared();
        assert_eq!(
            next_connection(|| Err::<u8, _>(io::Error::other("lock")), &shared),
            None
        );
        assert!(shared.failure.lock().unwrap().is_some());
    }

    #[test]
    fn the_stop_interrupts_the_pause_between_two_tries_of_accepting() {
        let shared = Arc::new(shared());
        let tries = Arc::new(Mutex::new(0_u32));
        let looping = {
            let shared = Arc::clone(&shared);
            let tries = Arc::clone(&tries);
            thread::spawn(move || {
                next_connection(
                    || {
                        *tries.lock().unwrap() += 1;
                        Err::<u8, _>(io::Error::from_raw_os_error(8))
                    },
                    &shared,
                )
            })
        };
        // The pauses double from 10 ms: the eighth error is followed by a pause of the full
        // second. The count is raised just before the pause begins.
        let mut stats = shared.stats();
        while stats.accept_retries < 8 {
            stats = shared.changed.wait(stats).unwrap();
        }
        drop(stats);
        let stopped = Instant::now();
        shared.stop();
        assert_eq!(looping.join().unwrap(), None);
        // Without the interruption the loop would sleep out the rest of its second.
        assert!(
            stopped.elapsed() < Duration::from_millis(500),
            "{:?}",
            stopped.elapsed()
        );
        assert_eq!(*tries.lock().unwrap(), 8, "no try after the stop");
        assert_eq!(
            *shared.failure.lock().unwrap(),
            None,
            "a stop is no failure"
        );
    }

    /// A connection whose close waits until the test lets it, and says when it began.
    struct SlowClose {
        closing: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl Drop for SlowClose {
        fn drop(&mut self) {
            let _ = self.closing.send(());
            let _ = self.release.recv();
        }
    }

    /// R-T2-2: a connection thread whose broker work is done still holds its place, and still
    /// counts as alive, while its connection is being closed; both go only when it has closed.
    #[test]
    fn a_connection_thread_holds_its_place_until_its_connection_is_closed() {
        let shared = Arc::new(shared());
        // The place, taken at accept.
        shared.update(|s| s.connections += 1);
        let (closing, began) = mpsc::channel();
        let (release, released) = mpsc::channel();
        spawn(&shared, false, move |_| SlowClose {
            closing,
            release: released,
        });
        began.recv().unwrap();
        let stats = *shared.stats();
        assert_eq!(
            (stats.connections, stats.threads),
            (1, 1),
            "the place and the thread stay while the connection closes"
        );
        release.send(()).unwrap();
        let mut stats = shared.stats();
        while stats.connections != 0 || stats.threads != 0 {
            stats = shared.changed.wait(stats).unwrap();
        }
        assert_eq!(stats.peak_threads, 1);
    }

    #[test]
    fn a_connection_s_reader_hands_one_line_at_a_time_and_nothing_once_stopped() {
        let shared = shared();
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
        shared.stop();
        assert_eq!(
            reader.read(&mut buf).unwrap(),
            0,
            "buffered bytes are dropped"
        );

        // Requests already received before the stop are not handed on after it.
        let shared = self::shared();
        let input: &[u8] = b"a\nb\nc\n";
        let mut reader = UntilStopped::new(input, &shared);
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"a\n");
        shared.stop();
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }
}
