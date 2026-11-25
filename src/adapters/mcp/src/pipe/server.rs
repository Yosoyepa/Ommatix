//! The listener of the pipe (ADR-0043, decisions 2 to 5 and 7): it binds the name, accepts
//! connections on one thread and serves each one on a thread of its own through the broker.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use interprocess::ConnectWaitMode;
use interprocess::os::windows::named_pipe::{PipeListener, PipeListenerOptions, pipe_mode};
use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use widestring::U16CString;

use super::{FULL_LINE, MAX_REFUSING, PipeError, PipeName, PipeStream};
use crate::broker::{Admission, Broker, BrokerError, Reservation};
use ommatix_application::lease::LeaseRefusal;

/// The pipe's security descriptor (ADR-0043, decision 5): protected, one allow entry with every
/// right for the object's owner, and nothing for anyone else.
pub const OWNER_ONLY_SDDL: &str = "D:P(A;;GA;;;OW)";

/// How long [`PipeServer::shutdown`] tries to wake its accept thread, in tries of
/// [`WAKE_WAIT`] each.
const WAKE_TRIES: u32 = 50;

/// How long one wake-up connection waits for a free instance.
const WAKE_WAIT: Duration = Duration::from_millis(100);

type Listener = PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>;

/// What a [`PipeServer`] has done and is doing: counts only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipeStats {
    /// Connections being served now, each on its own thread, whether its handshake was read
    /// yet or not. Each holds a place among the broker's clients from accept on, so this is
    /// never above the broker's bound on clients, but for a thread that is ending after its
    /// client left.
    pub connections: usize,
    /// Connections beyond the bound being told that the engine is full now, each on a
    /// short-lived thread. Never above [`MAX_REFUSING`].
    pub refusing: usize,
    /// Connections the broker gave a place to at accept, so far.
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
}

/// What [`PipeServer::shutdown`] left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Whether the accept thread ended; when it did, the pipe's waiting instance is closed.
    pub accept_stopped: bool,
    /// The counts at the end of the wait.
    pub stats: PipeStats,
    /// The error that stopped the accept loop before the shutdown, if any.
    pub failure: Option<PipeError>,
}

impl ShutdownReport {
    /// True when nothing of the server remains: no accept thread, no connection or refusal
    /// thread. The broker's clients leave with their connections.
    pub fn complete(&self) -> bool {
        self.accept_stopped && self.stats.connections == 0 && self.stats.refusing == 0
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

/// A place counted in [`PipeStats`] for as long as its thread runs; given back when the thread
/// ends, whatever ends it.
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

/// A connection's reader that reads as the end of input once the server is shutting down, so
/// that a connection serves no message after [`PipeServer::shutdown`]: bytes that arrive then
/// are dropped unread and the client leaves.
struct UntilStopped<'a> {
    stream: &'a PipeStream,
    shared: &'a Shared,
}

impl Read for UntilStopped<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shared.stopping() {
            return Ok(0);
        }
        let n = (&mut &*self.stream).read(buf)?;
        Ok(if self.shared.stopping() { 0 } else { n })
    }
}

/// Serves a [`Broker`] over a local Windows named pipe (ADR-0043, proposed).
///
/// One thread accepts connections. For each, before anything is read from it, the broker is
/// asked for a place among its clients ([`Broker::reserve`]); a connection given one is served
/// on a thread of its own with [`Reservation::serve_sync_with_handshake`], from its handshake
/// line to its end, and holds that place from accept on, so peers that send nothing hold at
/// most the broker's bound on clients. A connection beyond the bound is refused at accept,
/// before anything is read from it: it is told the engine is full
/// ([`super::FULL_LINE`]) on a short-lived thread of its own, so that a peer that never reads
/// blocks only that thread, and past [`MAX_REFUSING`] such threads it is closed with no line.
/// The accept thread does nothing else between two accepts.
///
/// **What it cannot do.** No timeout and no cancel is possible on a blocked read, write or flush
/// through the crate's safe API, and this adapter adds no `unsafe`. So a client that connects
/// and then neither sends nor reads what it is sent holds one place, one thread and the pipe's
/// name until it acts, leaves or the process ends; the bound on clients, counted from accept, is
/// what limits it. For the same reason the handshake has a bound on its size and none on its
/// time. The real end of an engine is the end of its process, which closes every handle.
///
/// Dropping the server shuts it down as [`PipeServer::shutdown`] does, without waiting for its
/// connections.
#[allow(missing_debug_implementations)] // holds the broker (trait objects)
pub struct PipeServer {
    name: PipeName,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
    /// Disconnected when the accept thread ends: its sender is dropped with the thread.
    accept_ended: Receiver<()>,
}

impl PipeServer {
    /// Creates the pipe `name` and starts serving `broker` on it, with at most the broker's
    /// bound on clients served at once (the bound given to [`Broker::new`]).
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

    /// The error that stopped the accept loop, if one did: the server then accepts nobody new.
    pub fn failure(&self) -> Option<PipeError> {
        *self
            .shared
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Stops the server.
    ///
    /// It stops accepting at once: the accept thread, blocked until a client connects, is
    /// woken by a connection the server makes to its own pipe, sees that it is stopping and
    /// ends, which closes the pipe's waiting instance. From then on every connection reads its
    /// input as ended, so it serves no further message: it ends at the next byte its client
    /// sends, or when its client leaves or its pipe breaks, and its client leaves the broker
    /// (the last one purges the engine). A connection blocked on a client that stays connected
    /// without sending or reading cannot be interrupted without `unsafe` (the crate has no
    /// safe call that cancels a read or disconnects an instance); its instance, and so the
    /// name, stay until that client leaves. This waits up to `wait` for every thread to end
    /// and reports what is left; calling it again waits again.
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

    fn stop(&mut self, wait: Duration) -> ShutdownReport {
        self.shared.stopping.store(true, Ordering::SeqCst);
        let accept_stopped = match self.accept.take() {
            Some(accept) => {
                // Connecting is what makes a blocking accept return; the accept loop drops this
                // connection unread. Each connection is held open until the accept thread has
                // ended: one that left at once could reach the waiting instance before the accept
                // waits on it, and the crate drops such a dead-on-arrival connection and waits
                // on. The wait for the thread is bounded and never a bare join: another process
                // of the same user may add instances to this pipe (the owner has every right),
                // and a connection may land on one of those instead of this listener's.
                let mut wakes = Vec::new();
                let mut ended = self.accept_ended(Duration::ZERO);
                for _ in 0..WAKE_TRIES {
                    if ended {
                        break;
                    }
                    if let Ok(made) = PipeStream::connect_by_path_with_wait_mode(
                        self.name.path(),
                        ConnectWaitMode::Timeout(WAKE_WAIT),
                    ) {
                        wakes.push(made);
                    }
                    ended = self.accept_ended(WAKE_WAIT);
                }
                if ended {
                    let _ = accept.join();
                }
                for wake in wakes {
                    wake.assume_flushed();
                }
                ended
            }
            None => true,
        };
        let stats = self
            .wait_until(wait, |s| s.connections == 0 && s.refusing == 0)
            .unwrap_or_else(|| self.stats());
        let threads = std::mem::take(
            &mut *self
                .shared
                .threads
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for thread in threads {
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
        ShutdownReport {
            accept_stopped,
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

/// Accepts until the server stops or the listener fails, then drops the listener, which closes
/// the waiting instance.
fn accept_loop(listener: &Listener, broker: &Broker, shared: &Arc<Shared>) {
    loop {
        let accepted = listener.accept();
        if shared.stopping() {
            if let Ok(stream) = accepted {
                stream.assume_flushed();
            }
            return;
        }
        let stream = match accepted {
            Ok(stream) => stream,
            Err(error) => {
                // Fail closed: a listener that cannot accept serves nobody new.
                *shared
                    .failure
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(PipeError::of_io(&error));
                shared.changed.notify_all();
                return;
            }
        };
        let reserved = broker.reserve();
        let place = {
            let mut stats = shared.stats();
            let place = match reserved {
                Ok(reservation) => {
                    stats.connections += 1;
                    stats.accepted += 1;
                    Place::Serve(reservation)
                }
                Err(BrokerError::Lease(LeaseRefusal::TooManyClients))
                    if stats.refusing < MAX_REFUSING =>
                {
                    stats.refusing += 1;
                    stats.told_full += 1;
                    Place::Refuse
                }
                Err(_) => {
                    stats.closed_full += 1;
                    Place::Close
                }
            };
            shared.changed.notify_all();
            place
        };
        match place {
            Place::Serve(reservation) => {
                spawn(shared, false, move |shared| {
                    serve(reservation, shared, &stream);
                });
            }
            Place::Refuse => spawn(shared, true, move |_| refuse(&stream)),
            Place::Close => stream.assume_flushed(),
        }
    }
}

/// Starts the thread of one counted place; when it cannot start, the place is given back, the
/// broker's reservation, owned by `work`, is dropped (which frees it) and the connection is
/// closed.
fn spawn(shared: &Arc<Shared>, refusal: bool, work: impl FnOnce(&Shared) + Send + 'static) {
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
            work(&slot.shared);
        });
    if let Ok(handle) = started {
        shared.keep(handle);
    }
}

/// Serves one connection in its reserved place, from its handshake to its end.
fn serve(reservation: Reservation, shared: &Shared, stream: &PipeStream) {
    let reader = UntilStopped { stream, shared };
    let outcome = reservation.serve_sync_with_handshake(reader, stream);
    // Every reply was flushed when it was written; whatever is left goes nowhere, and the
    // stream never waits in the crate's flushing pool.
    stream.assume_flushed();
    shared.update(|s| match outcome {
        Ok(Admission::Served) => s.admitted += 1,
        Ok(Admission::Refused(_)) => s.refused += 1,
        Err(_) => s.failed += 1,
    });
}

/// Tells a connection beyond the bound that the engine is full, and closes it. The flush waits
/// until the peer has read the line, on this thread only.
fn refuse(stream: &PipeStream) {
    let mut line = String::with_capacity(FULL_LINE.len() + 1);
    line.push_str(FULL_LINE);
    line.push('\n');
    let mut writer = stream;
    let _ = writer
        .write_all(line.as_bytes())
        .and_then(|()| writer.flush());
    stream.assume_flushed();
}
