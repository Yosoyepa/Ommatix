//! The broker: several MCP connections on one engine, under a single input lease and each
//! client's grant (ADR-0039, ADR-0040).
//!
//! A [`Broker`] owns the one [`Engine`], the [`LeaseTable`], the injected [`Clock`], the
//! [`GrantedPolicy`] the engine reads its policy through and every connected client's grant,
//! all behind one lock: a check and the action it guards run in one critical section
//! (ADR-0039, decision 2). [`Broker::connect`] admits a client and returns a
//! [`BrokerConnection`], which speaks the stdio server's protocol through the same
//! [`Connection`] and can be served over any pair of streams.
//!
//! Each tool call runs inside the lock, in this order:
//!
//! 1. The clock is read once. The operator's purge request is honoured as the stdio server
//!    honours it, and a purge or the operator's stop revokes the lease.
//! 2. The `lease` argument of `observe_screen`, when sent, is applied at that instant:
//!    `acquire` asks for the lease and `release` releases it, exactly as
//!    [`BrokerConnection::acquire`] and [`BrokerConnection::release`] do: a waiter's `release`
//!    takes it out of the queue. What the lease table refuses (a renewal while another client
//!    waits, a release by a client that neither holds the lease nor waits for it, time
//!    exhausted) is no error for the caller: the observation is answered and `_meta` says where
//!    the client stands.
//! 3. The lease is settled for the caller at that instant; when it passed to another client,
//!    the published snapshot is spent, so that the new holder observes before it acts.
//! 4. The caller's grant is installed for the length of the call: the gate and the view
//!    decide under the meet of the operator's policy and that grant. A client admitted with a
//!    grant handed in by its caller ([`Broker::connect`]) keeps that grant for its whole
//!    connection; a client admitted by a ticket ([`Broker::admit`]) has its grant asked of the
//!    [`GrantStore`] at this step, on every call, so that a grant the operator changes or
//!    withdraws takes effect at that client's next call (ADR-0040, decision 5). A client without
//!    a valid grant (none handed in, an empty handshake, a ticket the store does not list, a
//!    store that cannot answer) is served under [`Grant::default`], which permits no live input
//!    and no values (ADR-0040, decision 7).
//!
//!    A grant may also carry a delivery budget and a term (ADR-0040, proposed). Each client has a
//!    [`GrantLedger`] from its admission, the term's start: every grant read at a call lowers
//!    its ceilings to the meet of what was read so far, never raises them, and it counts the
//!    client's results that reported input. At this step, with the call's one instant, a client
//!    whose term has ended is served under [`Grant::default`], and one whose budget is spent
//!    under its grant without live input.
//! 5. `observe_screen` publishes a new snapshot only for the holder, or for any client while
//!    nobody holds the lease; any other client gets the published page served again for it,
//!    or `STALE_SNAPSHOT` when nothing is published. `execute_action`, in this order: a dry run
//!    goes to the engine for any client (decision 4); a client's own retry, a request
//!    identifier its journal holds, goes to the engine whatever the term, the budget or the
//!    lease: the engine answers it from the journal (a replay, or `REQUEST_CONFLICT` for other
//!    arguments) and never delivers it again, so a client whose input was delivered learns so
//!    even after it lost the lease. Any other live input is refused before the gate with
//!    `POLICY_DENIED`, delivering and recording nothing, when the client's term has ended,
//!    then when its budget is spent, then when it does not hold the lease; each refusal's
//!    message names its cause. Only then does it reach the gate, and the result is counted in
//!    the client's ledger when it reported input. Neither a dry run, a retry nor a refusal
//!    spends anything. `get_engine_metrics` is answered as by the stdio server.
//!
//! The broker uses the call's entry instant for both limits. Its one lock covers the grant read,
//! the checks, the engine call and the debit, so two new actions cannot both spend a budget of one.
//! An action admitted before the term ends runs without a term recheck, including between the two
//! phases of a type. Responses carry no numeric budget or term. A refusal names the limit.
//!
//! Responses have the stdio server's shape, with one addition in `_meta`: on `observe_screen`
//! and `execute_action`, when the request carried `lease` or more than one client is connected,
//! the key [`LEASE_META_KEY`](crate::LEASE_META_KEY) says where the client stands (`holder`,
//! `waiting` or `none`, the lease's number, the place in the queue) and whether the page is the
//! one already published, with its age (ADR-0039, proposed, decision 5). A response that carries
//! no page (a tool error, `LIMIT_EXCEEDED` included) reports no shared page. A sole client that never sends `lease` holds
//! the lease implicitly and gets the stdio server's responses byte for byte. The lease can also
//! be asked for through [`BrokerConnection::acquire`] and [`BrokerConnection::release`].
//!
//! A client that leaves takes its lease, its place in the queue, its journal, its grant and its
//! grant's ledger with it; the last one to leave purges the engine, as the end of the stdio
//! server's connection does (ADR-0039, decision 10). A purge deletes perception state, not a
//! connected client's ledger.
//!
//! ## Tickets (ADR-0040 decision 3, ADR-0043 decision 9; proposed)
//!
//! A transport that cannot be handed a grant by its caller admits a client from the first line
//! of its connection, before any MCP message ([`Broker::admit`], or the serving functions
//! [`Broker::serve_with_handshake`] and [`Broker::serve_sync_with_handshake`]):
//!
//! - an empty line admits a client without a grant;
//! - a ticket, [`TICKET_MIN_LEN`] to [`TICKET_MAX_LEN`] characters of the base64url alphabet
//!   (`A`–`Z`, `a`–`z`, `0`–`9`, `-`, `_`) and nothing else, is hashed with SHA-256 at once;
//!   the line is wiped, and the hash is all the broker keeps and all it asks the store with;
//! - a ticket is spent by its first use, for the life of the broker (ADR-0040, decision 4): a
//!   later connection that presents it is refused, whether or not the first is still
//!   connected and whether or not the store lists it;
//! - anything else (a line over the message bound, a line of another form, an input that ends
//!   before the line's `\n`, a spent ticket, a ticket when the spent set is full) is refused
//!   with [`BrokerError::Refused`]: the connection ends, nothing is written back, no client
//!   entry stays and no MCP message is read.
//!
//! A ticket the store does not list is admitted, without a grant, and spent, exactly as a
//! listed one is: the handshake never asks the store, so its outcome and its timing are the
//! same for a listed and an unlisted ticket, and a peer cannot use it to learn which tickets the
//! operator wrote. The store is asked at each tool call instead, so the client holds whatever
//! the operator lists for its ticket at that call, and nothing while it lists nothing.
//!
//! A handshake does bounded work. Its line is read by the bounded reader of every other line,
//! but a line longer than the message limit is refused as soon as it passes the limit, without
//! reading on to its end: a handshake reads at most [`Broker::handshake_read_bound`] bytes (the
//! limit, one byte, and one 8 KiB buffer), whatever the peer sends. After admission the reader
//! reads an oversized message on to its end again, so an established connection survives one.
//! A connection whose handshake is not settled counts against the bound on clients when its
//! listener holds a [`Reservation`] for it ([`Broker::reserve`]), taken before the first read:
//! peers that connect and send nothing cannot hold more places, or threads, than the bound.
//!
//! The places among the clients (connected clients and reservations together) are counted apart
//! from the lock that serves calls, by atomic operations that never wait: a listener reserving a
//! place for a connection it just accepted never waits on a tool call another client is making
//! (ADR-0043, decision 7; D39).
//!
//! The spent set is bounded by [`MAX_SPENT_TICKETS`] hashes (32 bytes each), so a peer that
//! presents ticket after ticket cannot exhaust memory. When it is full the broker fails closed:
//! it admits nobody new by ticket, with the same refusal as a spent ticket, until the engine
//! process restarts; an empty handshake is still admitted, without a grant.
//!
//! The executable constructs a broker only when built with the `broker-pipe` feature and
//! configured with `[mcp] transport = "pipe"` (ADR-0043, proposed); otherwise it serves stdio.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ommatix_application::contract::{ActionRequest, ActionResult, ErrorCode, ToolError};
use ommatix_application::lease::{Acquired, ClientId, LeaseChange, LeaseRefusal, Released};
use ommatix_application::{
    Clock, Engine, EngineConfig, Grant, GrantLedger, GrantStore, GrantedPolicy, JOURNAL_CLIENTS,
    LeaseTable, PolicyStore, Ports,
};
use ommatix_domain::identity::ActionStatus;
use rmcp::model::Tool;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, BufReader};

use crate::config::McpConfig;
use crate::dto::LeaseArgument;
use crate::protocol::{Backend, Connection};
use crate::server::{self, READ_CHUNK, exchange, exchange_blocking, exchange_blocking_until};
use crate::tools::{self, CallError, LeaseReport, ToolCall, ToolOutput};
use crate::transport::{Frame, LineReader};

/// The fewest characters of a ticket (ADR-0043 decision 9; D33): 32 base64url characters carry
/// 192 bits.
pub const TICKET_MIN_LEN: usize = 32;

/// The most characters of a ticket.
pub const TICKET_MAX_LEN: usize = 128;

/// The most tickets a broker remembers as spent: 4096 hashes of 32 bytes, 128 KiB. A full set
/// admits nobody new by ticket until the engine process restarts (fail closed).
pub const MAX_SPENT_TICKETS: usize = 4096;

/// The refusal of a live action from a client that does not hold the lease and whose journal
/// does not know its request identifier (ADR-0039, decisions 4 and 6). It names the limit and
/// quotes nothing from the screen or the agent (ADR-0021, ADR-0040 decision 9).
const NO_LEASE: &str = "live input needs the input lease and this client does not hold it; \
                        nothing was delivered";

/// The refusal of a live action new to the client's journal once its grant's term has ended
/// (ADR-0040, proposed). It names the limit and quotes nothing (ADR-0021, ADR-0040 decision 9).
const TERM_ENDED: &str = "live input is refused: this client's grant has reached the end of its \
                          term; nothing was delivered";

/// The refusal of a live action new to the client's journal once its grant's delivery budget is
/// spent (ADR-0040, proposed). It names the limit and quotes nothing.
const BUDGET_SPENT: &str = "live input is refused: this client's grant has no delivery left in \
                            its budget; nothing was delivered";

/// The answer to an observation by a client that may not publish when nothing is published.
const NOTHING_PUBLISHED: &str = "no snapshot is published and another client holds the input \
                                 lease; observe again";

/// Why the broker refused a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// The bound on clients given to [`Broker::new`] is zero or above
    /// [`JOURNAL_CLIENTS`], the number of clients the engine keeps a journal share for.
    ClientBound,
    /// The lease term given to [`Broker::new`] is zero.
    ZeroTerm,
    /// The lease table refused: the bound on connected clients is reached
    /// ([`LeaseRefusal::TooManyClients`]), or a lease request was refused.
    Lease(LeaseRefusal),
    /// A call panicked while it held the broker's lock. From then on the broker admits no
    /// client and serves no tool call, so nothing is delivered (fail closed); a connection
    /// that leaves still deletes its state, and the last one purges the engine.
    Unavailable,
    /// A handshake admitted nobody: its line was over the bound, of another form than an empty
    /// line or a ticket, or ended with the input; or it named a ticket already spent, or the
    /// set of spent tickets is full. One refusal for all of these, which carries nothing of the
    /// line (ADR-0043, decision 9).
    Refused,
}

/// How a connection served with a handshake ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The handshake admitted a client, whose connection was served until its input ended;
    /// the client then left.
    Served,
    /// The handshake admitted nobody: nothing was written back and no MCP message was read.
    Refused(BrokerError),
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClientBound => write!(
                f,
                "the bound on clients must be between 1 and {JOURNAL_CLIENTS}"
            ),
            Self::ZeroTerm => f.write_str("the lease term must not be zero"),
            Self::Lease(refusal) => refusal.fmt(f),
            Self::Unavailable => f.write_str("the broker is unavailable after a failed call"),
            Self::Refused => f.write_str("the handshake admitted nobody"),
        }
    }
}

impl std::error::Error for BrokerError {}

/// What a broker retains, for research and tests: counts only, never screen content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerState {
    /// The clients connected.
    pub clients: BTreeSet<ClientId>,
    /// Whether the policy the engine reads outside a call differs from the operator's: true
    /// only if a client's grant stayed installed after its call.
    pub grant_installed: bool,
    /// [`Engine::retained_state_sizes`].
    pub engine: BTreeMap<&'static str, usize>,
    /// Per connected client, the results that reported input since its admission, as its
    /// grant's ledger counts them ([`GrantLedger::spent`]). Never on the wire.
    pub spent: BTreeMap<ClientId, u64>,
}

/// Serves several MCP connections on one engine under the input lease and each client's
/// grant (ADR-0039, ADR-0040).
///
/// Cheap to clone: clones share the engine and its lock.
#[derive(Clone)]
#[allow(missing_debug_implementations)] // holds the engine (trait objects)
pub struct Broker {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    /// The places taken among the clients: one for each connected client and each
    /// [`Reservation`], never above `max_clients`. Counted outside `state`'s lock, by atomic
    /// operations only, so taking or giving back a place never waits on a call being served.
    places: AtomicUsize,
    /// The bound on clients given to [`Broker::new`].
    max_clients: usize,
    config: McpConfig,
    tools: Arc<[Tool]>,
    /// The operator's grants by ticket hash, asked on every call of a client admitted by
    /// ticket; `None` when the broker was built without one, and then no ticket grants anything.
    grant_store: Option<Arc<dyn GrantStore>>,
}

/// Where a connected client's grant comes from.
enum Authority {
    /// Handed in by the caller at connect (or none, for an empty handshake), and kept for the
    /// whole connection.
    Handed(Option<Grant>),
    /// The SHA-256 of the ticket the client attached with: its grant is asked of the
    /// [`GrantStore`] on each call. The ticket itself is not kept.
    Ticket([u8; 32]),
}

/// A connected client: where its grant comes from, and its grant's ledger. One entry, so that
/// leaving deletes both at once.
struct Client {
    authority: Authority,
    /// The count and ceilings of its grant's delivery budget and term, kept from its admission.
    ledger: GrantLedger,
}

/// What a well-formed handshake line presents.
enum Presented {
    /// An empty line: no ticket.
    Nothing,
    /// A ticket, by its SHA-256.
    Ticket([u8; 32]),
}

impl Presented {
    /// Reads a handshake line without its terminator: empty, or a ticket of the exact form;
    /// `None` for anything else.
    fn parse(line: &[u8]) -> Option<Self> {
        if line.is_empty() {
            return Some(Self::Nothing);
        }
        let form = (TICKET_MIN_LEN..=TICKET_MAX_LEN).contains(&line.len())
            && line
                .iter()
                .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        form.then(|| Self::Ticket(Sha256::digest(line).into()))
    }
}

/// Overwrites a handshake line before its buffer is freed, so that the broker keeps no copy of
/// a ticket once it is hashed. `black_box` keeps the writes from being dropped as dead stores.
fn wipe(mut line: Vec<u8>) {
    line.fill(0);
    std::hint::black_box(&line);
}

/// Everything a check and the action it guards read or change, behind the broker's one lock.
struct State {
    engine: Engine,
    leases: LeaseTable,
    clock: Arc<dyn Clock>,
    /// The policy store the engine reads: the operator's store narrowed by the grant of the
    /// client being served.
    granted: Arc<GrantedPolicy>,
    /// The operator's store itself, for the stop and the purge request.
    operator: Arc<dyn PolicyStore>,
    /// Each connected client, with where its grant comes from and its grant's ledger.
    clients: BTreeMap<ClientId, Client>,
    /// The hashes of the tickets spent so far, at most [`MAX_SPENT_TICKETS`]. Kept for the
    /// life of the broker: a purge deletes perception state, not this.
    spent: BTreeSet<[u8; 32]>,
}

/// One place among a broker's clients, counted in [`Shared::places`] while it lives: held by a
/// [`Reservation`] and then by the client admitted in it, or by a client admitted without one.
/// Dropping it gives the place back.
struct Seat {
    shared: Arc<Shared>,
}

impl Drop for Seat {
    fn drop(&mut self) {
        self.shared.places.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A place among a broker's clients, held for a connection whose handshake has not been read yet
/// (ADR-0043, decision 7; D36). A listener takes one with [`Broker::reserve`] when it accepts a
/// connection, before it gives the connection a thread or reads a byte from it, and closes the
/// connection unread when the broker refuses. The reservation counts against the bound on
/// clients from that moment, so peers that connect and send nothing cannot hold more threads
/// than the bound allows.
///
/// Serving the connection consumes it ([`Reservation::serve_sync_with_handshake`],
/// [`Reservation::serve_with_handshake`], [`Reservation::admit`]): an admitted client takes the
/// place over, and leaving frees it as for any client; a refused handshake frees it at once.
/// Dropping an unused reservation frees its place.
pub struct Reservation {
    seat: Seat,
}

impl std::fmt::Debug for Reservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reservation").finish_non_exhaustive()
    }
}

impl Reservation {
    /// Admits a client by its handshake line in this reserved place, as [`Broker::admit`] does.
    /// The place goes to the client, or is freed when the handshake is refused.
    ///
    /// # Errors
    ///
    /// As [`Broker::admit`].
    pub fn admit(self, handshake: Vec<u8>) -> Result<BrokerConnection, BrokerError> {
        self.broker().admit_with(Some(self), handshake)
    }

    /// [`Broker::serve_with_handshake`] in this reserved place.
    ///
    /// # Errors
    ///
    /// An I/O error of the stream, while reading the handshake or serving the connection.
    pub async fn serve_with_handshake<R, W>(self, reader: R, writer: W) -> io::Result<Admission>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        self.broker()
            .handshake_then_serve(Some(self), reader, writer)
            .await
    }

    /// [`Broker::serve_sync_with_handshake`] in this reserved place: what a listener that gives
    /// each connection a thread runs on that thread.
    ///
    /// # Errors
    ///
    /// An I/O error of the stream, while reading the handshake or serving the connection.
    pub fn serve_sync_with_handshake<R, W>(self, reader: R, writer: W) -> io::Result<Admission>
    where
        R: io::Read,
        W: io::Write,
    {
        self.serve_sync_with_handshake_until(reader, writer, || false)
    }

    /// [`Reservation::serve_sync_with_handshake`] under a listener's stop (ADR-0043; D40):
    /// `stopped` is asked once per message, as the last act before the message is handed on,
    /// the handshake's admission included. When it answers true the message is not handed on,
    /// nothing is written back, and the connection ends with an error for which the listener
    /// can tell the stop apart; the reservation is freed, or the admitted client leaves. A
    /// message that passed the check before the stop runs to its end, reply included.
    ///
    /// # Errors
    ///
    /// An I/O error of the stream, or the stop, while reading the handshake or serving the
    /// connection.
    pub fn serve_sync_with_handshake_until<R, W, S>(
        self,
        reader: R,
        writer: W,
        stopped: S,
    ) -> io::Result<Admission>
    where
        R: io::Read,
        W: io::Write,
        S: Fn() -> bool,
    {
        self.broker()
            .handshake_then_serve_sync(Some(self), reader, writer, &stopped)
    }

    fn broker(&self) -> Broker {
        Broker {
            shared: Arc::clone(&self.seat.shared),
        }
    }
}

impl Broker {
    /// Builds the one engine over `ports`, with the operator's policy store of `ports` wrapped
    /// in a [`GrantedPolicy`], so that every read of the policy is narrowed by the grant of the
    /// client being served. Leases last `term` (ADR-0039, decision 7: the validity window of
    /// ADR-0025) and at most `max_clients` clients are connected at once.
    ///
    /// # Errors
    ///
    /// [`BrokerError::ClientBound`] when `max_clients` is zero or above [`JOURNAL_CLIENTS`],
    /// [`BrokerError::ZeroTerm`] when `term` is zero.
    pub fn new(
        engine: EngineConfig,
        ports: Ports,
        config: McpConfig,
        term: Duration,
        max_clients: usize,
    ) -> Result<Self, BrokerError> {
        Self::build(engine, ports, config, term, max_clients, None)
    }

    /// [`Broker::new`], with the operator's grants by ticket (ADR-0040, decisions 3 and 5): a
    /// client admitted by a ticket ([`Broker::admit`]) has its grant asked of `grants` on every
    /// tool call, inside the broker's lock. A store that panics answers no grant for that call
    /// and leaves the broker serving (fail closed). Clients connected with a grant handed in
    /// ([`Broker::connect`]) are never looked up.
    ///
    /// # Errors
    ///
    /// As [`Broker::new`].
    pub fn with_grant_store(
        engine: EngineConfig,
        ports: Ports,
        config: McpConfig,
        term: Duration,
        max_clients: usize,
        grants: Arc<dyn GrantStore>,
    ) -> Result<Self, BrokerError> {
        Self::build(engine, ports, config, term, max_clients, Some(grants))
    }

    fn build(
        engine: EngineConfig,
        mut ports: Ports,
        config: McpConfig,
        term: Duration,
        max_clients: usize,
        grant_store: Option<Arc<dyn GrantStore>>,
    ) -> Result<Self, BrokerError> {
        let bound = NonZeroUsize::new(max_clients)
            .filter(|bound| bound.get() <= JOURNAL_CLIENTS)
            .ok_or(BrokerError::ClientBound)?;
        let leases = LeaseTable::new(bound, term).ok_or(BrokerError::ZeroTerm)?;
        let operator = Arc::clone(&ports.policy);
        let granted = Arc::new(GrantedPolicy::new(Arc::clone(&operator)));
        ports.policy = granted.clone();
        let clock = Arc::clone(&ports.clock);
        let state = State {
            engine: Engine::new(engine, ports),
            leases,
            clock,
            granted,
            operator,
            clients: BTreeMap::new(),
            spent: BTreeSet::new(),
        };
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                places: AtomicUsize::new(0),
                max_clients,
                config,
                tools: tools::descriptors().into(),
                grant_store,
            }),
        })
    }

    pub fn config(&self) -> McpConfig {
        self.shared.config
    }

    /// The bound on clients given to [`Broker::new`]: connected clients and [`Reservation`]s
    /// together never exceed it.
    pub fn max_clients(&self) -> usize {
        self.shared.max_clients
    }

    /// Admits a client with its grant (ADR-0039, decision 3; ADR-0040, decision 7). `None` is
    /// a client without a valid grant: it observes without values and delivers nothing. The
    /// client's identifier comes from the lease table; the agent never chooses it.
    ///
    /// # Errors
    ///
    /// [`BrokerError::Lease`] with [`LeaseRefusal::TooManyClients`] at the bound (connected
    /// clients and [`Reservation`]s together), [`BrokerError::Unavailable`] after a failed call.
    pub fn connect(&self, grant: Option<Grant>) -> Result<BrokerConnection, BrokerError> {
        let (client, seat) = {
            let mut state = self.shared.lock()?;
            let now = state.clock.now();
            state.enter();
            let seat = self.shared.take_seat()?;
            let outcome = state.leases.connect(now);
            state.apply(&outcome.change);
            let client = outcome.result.map_err(BrokerError::Lease)?;
            state.clients.insert(
                client,
                Client {
                    authority: Authority::Handed(grant),
                    ledger: GrantLedger::new(now),
                },
            );
            (client, seat)
        };
        Ok(self.connection_for(client, seat))
    }

    /// Admits a client from the first line of its connection, before any MCP message (ADR-0040
    /// decision 3, ADR-0043 decision 9; proposed). `handshake` is that line without its
    /// terminator; the broker takes it, hashes a ticket at once and wipes the line.
    ///
    /// An empty line admits a client without a grant. A ticket admits a client bound to the
    /// ticket's hash, whose grant is asked of the [`GrantStore`] on each of its calls, and is
    /// spent: no later connection can present it while this broker lives. A ticket the store
    /// does not list is admitted and spent alike (see the module's documentation for why).
    ///
    /// # Errors
    ///
    /// [`BrokerError::Refused`] for a line of any other form, a spent ticket or a full spent
    /// set; [`BrokerError::Lease`] with [`LeaseRefusal::TooManyClients`] at the bound on
    /// clients (connected clients and [`Reservation`]s together), which spends no ticket;
    /// [`BrokerError::Unavailable`] after a failed call. A refused handshake leaves no client
    /// entry and consumes nothing of the bound.
    pub fn admit(&self, handshake: Vec<u8>) -> Result<BrokerConnection, BrokerError> {
        self.admit_with(None, handshake)
    }

    /// Holds a place among the clients for a connection just accepted, before its handshake is
    /// read (ADR-0043, decision 7; D36). A listener calls it before it reads a byte from the
    /// connection or gives it a thread, and closes the connection unread when it is refused; see
    /// [`Reservation`].
    ///
    /// It never waits: the places are counted apart from the lock that serves calls, so a
    /// listener's accepting never waits on a tool call of any client (D39).
    ///
    /// # Errors
    ///
    /// [`BrokerError::Lease`] with [`LeaseRefusal::TooManyClients`] when connected clients and
    /// reservations already fill the bound; [`BrokerError::Unavailable`] after a failed call.
    pub fn reserve(&self) -> Result<Reservation, BrokerError> {
        if self.shared.state.is_poisoned() {
            return Err(BrokerError::Unavailable);
        }
        Ok(Reservation {
            seat: self.shared.take_seat()?,
        })
    }

    /// The most bytes a handshake reads from its stream before it admits or refuses: the
    /// message limit, one byte for a `\r`, and one buffer of the reader (8 KiB). A handshake line
    /// longer than the limit is refused as soon as it passes the limit, whether or not a line end
    /// follows, so a peer that sends bytes without end costs a bounded read (ADR-0043, decision
    /// 9; D36).
    pub fn handshake_read_bound(&self) -> usize {
        self.shared
            .config
            .message_limit()
            .saturating_add(1)
            .saturating_add(READ_CHUNK)
    }

    fn admit_with(
        &self,
        reservation: Option<Reservation>,
        handshake: Vec<u8>,
    ) -> Result<BrokerConnection, BrokerError> {
        let presented = Presented::parse(&handshake);
        wipe(handshake);
        // A refusal here drops the reservation, which frees its place.
        let presented = presented.ok_or(BrokerError::Refused)?;
        let (client, seat) = {
            let mut state = self.shared.lock()?;
            // A refusal from here on drops the reservation, which frees its place; an admitted
            // client takes the place over.
            if let Presented::Ticket(hash) = &presented
                && (state.spent.contains(hash) || state.spent.len() >= MAX_SPENT_TICKETS)
            {
                return Err(BrokerError::Refused);
            }
            let now = state.clock.now();
            state.enter();
            let seat = match reservation {
                Some(reservation) => reservation.seat,
                None => self.shared.take_seat()?,
            };
            let outcome = state.leases.connect(now);
            state.apply(&outcome.change);
            let client = outcome.result.map_err(BrokerError::Lease)?;
            let authority = match presented {
                Presented::Nothing => Authority::Handed(None),
                Presented::Ticket(hash) => {
                    state.spent.insert(hash);
                    Authority::Ticket(hash)
                }
            };
            state.clients.insert(
                client,
                Client {
                    authority,
                    ledger: GrantLedger::new(now),
                },
            );
            (client, seat)
        };
        Ok(self.connection_for(client, seat))
    }

    /// Reads the handshake line from `reader` with the bounded reader of every other line,
    /// admits a client by it ([`Broker::admit`]) and serves that client's connection with
    /// [`BrokerConnection::serve`]'s loop, on the same reader, until its input ends; the client
    /// then leaves. A refused handshake writes nothing back and reads no MCP message. The
    /// handshake reads at most [`Broker::handshake_read_bound`] bytes: a line longer than the
    /// message limit is refused as soon as it passes it. A listener that must count the
    /// connection against the bound before the handshake uses [`Reservation`] instead.
    ///
    /// # Errors
    ///
    /// An I/O error of the stream, while reading the handshake or serving the connection.
    pub async fn serve_with_handshake<R, W>(&self, reader: R, writer: W) -> io::Result<Admission>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        self.handshake_then_serve(None, reader, writer).await
    }

    /// [`Broker::serve_with_handshake`] over a blocking reader and writer, for a transport
    /// that gives each connection a thread (ADR-0043): the same handshake, then
    /// [`BrokerConnection::serve_sync`]'s loop on the same reader.
    ///
    /// # Errors
    ///
    /// An I/O error of the stream, while reading the handshake or serving the connection.
    pub fn serve_sync_with_handshake<R, W>(&self, reader: R, writer: W) -> io::Result<Admission>
    where
        R: io::Read,
        W: io::Write,
    {
        self.handshake_then_serve_sync(None, reader, writer, &|| false)
    }

    async fn handshake_then_serve<R, W>(
        &self,
        reservation: Option<Reservation>,
        reader: R,
        mut writer: W,
    ) -> io::Result<Admission>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = self.handshake_reader(BufReader::with_capacity(READ_CHUNK, reader));
        let frame = lines.next_frame().await?;
        let mut conn = match self.admit_frame(reservation, &mut lines, frame) {
            Ok(conn) => conn,
            Err(refusal) => return Ok(Admission::Refused(refusal)),
        };
        let outcome = exchange(&mut conn.connection, &mut lines, &mut writer).await;
        conn.close();
        outcome.map(|()| Admission::Served)
    }

    fn handshake_then_serve_sync<R, W>(
        &self,
        reservation: Option<Reservation>,
        reader: R,
        mut writer: W,
        stopped: &dyn Fn() -> bool,
    ) -> io::Result<Admission>
    where
        R: io::Read,
        W: io::Write,
    {
        let mut lines = self.handshake_reader(io::BufReader::with_capacity(READ_CHUNK, reader));
        let frame = lines.next_frame_blocking()?;
        // The listener's stop, checked as the last act before the admission (D40). A reservation
        // dropped here frees its place.
        if stopped() {
            return Err(server::stopped());
        }
        let mut conn = match self.admit_frame(reservation, &mut lines, frame) {
            Ok(conn) => conn,
            Err(refusal) => return Ok(Admission::Refused(refusal)),
        };
        let outcome =
            exchange_blocking_until(&mut conn.connection, &mut lines, &mut writer, stopped);
        conn.close();
        outcome.map(|()| Admission::Served)
    }

    /// The reader of a connection, set to read its handshake: a line over the message limit
    /// ends the read as soon as it passes the limit.
    fn handshake_reader<R>(&self, inner: R) -> LineReader<R> {
        let mut lines = LineReader::new(inner, self.shared.config.message_limit());
        lines.cut_oversized(true);
        lines
    }

    /// The handshake's frame: a line that ended at its `\n` is admitted or refused by
    /// [`Broker::admit`]; an oversized line, the end of input, or a line the input cut short is
    /// refused. Once a client is admitted, the reader goes back to reading an oversized message
    /// on to its end, so that an established connection survives one.
    fn admit_frame<R>(
        &self,
        reservation: Option<Reservation>,
        lines: &mut LineReader<R>,
        frame: Frame,
    ) -> Result<BrokerConnection, BrokerError> {
        let admitted = match frame {
            Frame::Message(line) if lines.terminated() => self.admit_with(reservation, line),
            Frame::Message(line) => {
                wipe(line);
                Err(BrokerError::Refused)
            }
            Frame::Oversized | Frame::Eof => Err(BrokerError::Refused),
        };
        lines.cut_oversized(false);
        admitted
    }

    /// The connection of a client just admitted, holding its place.
    fn connection_for(&self, client: ClientId, seat: Seat) -> BrokerConnection {
        let member = Member {
            shared: Arc::clone(&self.shared),
            client,
        };
        BrokerConnection {
            connection: Connection::with_backend(
                Box::new(member.clone()),
                self.shared.config,
                Arc::clone(&self.shared.tools),
            ),
            member,
            open: true,
            seat: Some(seat),
        }
    }

    /// What the broker retains now, `None` after a failed call.
    pub fn retained(&self) -> Option<BrokerState> {
        Some(self.shared.lock().ok()?.retained())
    }

    /// [`Broker::retained`] without waiting: `None` while the broker's lock is held, by a call
    /// being served on any thread (this one included), or after a failed call. It never blocks,
    /// so a status report, or a [`GrantStore`] asked inside a call, can use it.
    pub fn try_retained(&self) -> Option<BrokerState> {
        Some(self.shared.state.try_lock().ok()?.retained())
    }
}

impl Shared {
    /// Takes one place among the clients, without waiting: refused when connected clients and
    /// reservations already fill the bound.
    fn take_seat(self: &Arc<Self>) -> Result<Seat, BrokerError> {
        let mut taken = self.places.load(Ordering::SeqCst);
        loop {
            if taken >= self.max_clients {
                return Err(BrokerError::Lease(LeaseRefusal::TooManyClients));
            }
            match self.places.compare_exchange_weak(
                taken,
                taken + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Ok(Seat {
                        shared: Arc::clone(self),
                    });
                }
                Err(now) => taken = now,
            }
        }
    }

    /// The lock for anything that may observe or deliver: refused after a failed call.
    fn lock(&self) -> Result<MutexGuard<'_, State>, BrokerError> {
        self.state.lock().map_err(|_| BrokerError::Unavailable)
    }

    /// The lock for what only deletes state (the purge request, a client that leaves), which
    /// is safe even after a failed call.
    fn lock_to_delete(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The grant a client holds for the call being served, read under the broker's lock: the
    /// one handed in at connect, or what the store lists for the client's ticket at this
    /// instant (ADR-0040, decision 5). `None`, which installs the default grant, when the store
    /// lists nothing, when there is no store, or when the store panics: its panic is contained
    /// here, so that it neither grants anything nor poisons the broker's lock.
    fn grant_of(&self, client: Option<&Authority>) -> Option<Grant> {
        match client? {
            Authority::Handed(grant) => grant.clone(),
            Authority::Ticket(hash) => {
                let store = self.grant_store.as_ref()?;
                catch_unwind(AssertUnwindSafe(|| store.grant(hash)))
                    .ok()
                    .flatten()
            }
        }
    }
}

impl State {
    /// [`BrokerState`] now.
    fn retained(&self) -> BrokerState {
        BrokerState {
            clients: self.clients.keys().copied().collect(),
            grant_installed: self.granted.current() != self.operator.current(),
            engine: self.engine.retained_state_sizes(),
            spent: self
                .clients
                .iter()
                .map(|(client, entry)| (*client, entry.ledger.spent()))
                .collect(),
        }
    }

    /// The operator's side, at every entry into the broker: the purge request is honoured as
    /// the stdio server honours it at a request boundary, and a purge or the operator's stop
    /// revokes the lease (ADR-0039, decision 7). The stop itself is enforced by the gate.
    fn enter(&mut self) {
        let purged = self.operator.purge_requested();
        if purged {
            self.engine.purge();
            self.operator.purge_done();
        }
        if purged || self.operator.current().stopped {
            let change = self.leases.revoke();
            self.apply(&change);
        }
    }

    /// Acts on what a lease operation changed: a lease that passed to another client spends
    /// the published snapshot (ADR-0039, decision 7).
    fn apply(&mut self, change: &LeaseChange) {
        if change.spend_snapshot {
            self.engine.spend_snapshot();
        }
    }
}

/// One client of a broker: what its connection calls the engine through.
#[derive(Clone)]
struct Member {
    shared: Arc<Shared>,
    client: ClientId,
}

impl Backend for Member {
    fn request_boundary(&self) {
        self.shared.lock_to_delete().enter();
    }

    fn call_tool(&self, name: &str, arguments: &Value) -> Result<ToolOutput, CallError> {
        let call = tools::parse(name, arguments)?;
        let mut guard = self
            .shared
            .lock()
            .map_err(|_: BrokerError| CallError::Unavailable)?;
        let state = &mut *guard;
        let now = state.clock.now();
        state.enter();
        // The `lease` argument first, at the call's instant (ADR-0039, decision 5). The table's
        // answer is not an error for the caller: `_meta` reports where the client stands.
        let asked = match &call {
            ToolCall::Observe(_, Some(lease)) => {
                let change = match lease {
                    LeaseArgument::Acquire => state.leases.acquire(self.client, now).change,
                    LeaseArgument::Release => state.leases.release(self.client, now).change,
                };
                state.apply(&change);
                true
            }
            _ => false,
        };
        let settled = match call {
            ToolCall::Observe(..) => state.leases.publishes(self.client, now),
            ToolCall::Act(_) | ToolCall::Metrics => state.leases.may_deliver(self.client, now),
        };
        state.apply(&settled.change);
        // A client the table does not know neither publishes nor delivers.
        let holds = settled.result.unwrap_or(false);
        let reports = asked || state.clients.len() > 1;
        // The grant read for this call, then its budget and term at the call's one instant
        // (ADR-0040, proposed): the ledger takes the meet of what was read, and the grant
        // installed is the one the client may still use.
        let entry = state.clients.get_mut(&self.client);
        let read = self.shared.grant_of(entry.as_ref().map(|e| &e.authority));
        let (grant, term_ended, budget_spent) = match entry {
            Some(entry) => {
                if let Some(read) = &read {
                    entry.ledger.read(read);
                }
                (
                    entry.ledger.effective(read, now),
                    entry.ledger.term_ended(now),
                    entry.ledger.budget_spent(),
                )
            }
            None => (None, false, false),
        };
        let granted = Arc::clone(&state.granted);
        let _installed = granted.install(grant);
        let engine = &mut state.engine;
        // The age of the published page when this response serves it again.
        let mut shared_age = None;
        let mut output = match call {
            ToolCall::Observe(request, _) if holds => {
                let result = engine.observe_screen(&request);
                tools::observation(engine, result)
            }
            ToolCall::Observe(request, _) => {
                let result = engine
                    .serve_published(&request)
                    .and_then(|page| page.ok_or_else(nothing_published));
                if result.is_ok() {
                    shared_age = engine.published_age();
                }
                tools::observation(engine, result)
            }
            // A dry run or a request already in this client's journal goes to the engine
            // without a term, budget or lease refusal from this dispatch. A dry run spends
            // nothing. The engine replays a known request or reports REQUEST_CONFLICT without
            // delivering again (ADR-0039, decision 11; D24).
            ToolCall::Act(request)
                if request.dry_run || engine.knows_request(self.client, &request.request_id) =>
            {
                let result = engine.execute_action_for(self.client, &request);
                tools::action(&result, engine.last_action_timings())
            }
            // A live action new to the journal: the term, then the budget, then the lease, each
            // refused before the gate, delivering and recording nothing.
            ToolCall::Act(request) if term_ended => {
                tools::action(&refused_before_the_gate(&request, TERM_ENDED), None)
            }
            ToolCall::Act(request) if budget_spent => {
                tools::action(&refused_before_the_gate(&request, BUDGET_SPENT), None)
            }
            ToolCall::Act(request) if holds => {
                let result = engine.execute_action_for(self.client, &request);
                if let Some(entry) = state.clients.get_mut(&self.client) {
                    entry.ledger.record(&result);
                }
                tools::action(&result, engine.last_action_timings())
            }
            ToolCall::Act(request) => {
                tools::action(&refused_before_the_gate(&request, NO_LEASE), None)
            }
            ToolCall::Metrics => return Ok(tools::metrics(engine)),
        };
        if reports {
            output.lease = Some(LeaseReport {
                standing: state.leases.standing(self.client),
                shared_age,
            });
        }
        Ok(output)
    }
}

/// `STALE_SNAPSHOT`: the code the engine gives a page request when no snapshot is published.
fn nothing_published() -> ToolError {
    ToolError::new(ErrorCode::StaleSnapshot, NOTHING_PUBLISHED)
}

/// The broker's refusal of a live action before the gate (the term, the budget or the lease),
/// shaped as the gate's own refusals: the request's identifiers, `rejected` with
/// `POLICY_DENIED` and `message`, nothing delivered. It is not journaled.
fn refused_before_the_gate(request: &ActionRequest, message: &'static str) -> ActionResult {
    ActionResult {
        request_id: request.request_id.clone(),
        status: ActionStatus::Rejected,
        target_id: request.target_id.clone(),
        snapshot_id: request.snapshot_id.clone(),
        error: Some(ToolError::new(ErrorCode::PolicyDenied, message)),
        observed_effect: None,
        delivered: false,
    }
}

/// One client's connection to a [`Broker`]: the stdio server's protocol, with the lease and
/// the client's grant applied to every tool call. Leaving ([`BrokerConnection::close`], or
/// dropping it) deletes the client's state.
#[allow(missing_debug_implementations)] // holds the engine (trait objects)
pub struct BrokerConnection {
    connection: Connection,
    member: Member,
    open: bool,
    /// The client's place among the clients, given back when it leaves.
    seat: Option<Seat>,
}

impl BrokerConnection {
    /// The client's identifier, given by the lease table, for the research record. It is never
    /// sent on the wire.
    pub fn client(&self) -> ClientId {
        self.member.client
    }

    /// The protocol core: feed it each incoming message, as with the stdio server's
    /// connection.
    pub fn connection(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// Asks for the lease (ADR-0039, decisions 4 and 5), at the clock's present instant and
    /// under the broker's lock. A lease that passes spends the published snapshot.
    ///
    /// # Errors
    ///
    /// The lease table's refusal, or [`BrokerError::Unavailable`] after a failed call.
    pub fn acquire(&self) -> Result<Acquired, BrokerError> {
        let mut state = self.member.shared.lock()?;
        let now = state.clock.now();
        state.enter();
        let outcome = state.leases.acquire(self.member.client, now);
        state.apply(&outcome.change);
        outcome.result.map_err(BrokerError::Lease)
    }

    /// Releases what this client has of the lease: the lease it holds, which passes at once to
    /// the first client waiting, or its place in the queue ([`Released`]).
    ///
    /// # Errors
    ///
    /// The lease table's refusal, or [`BrokerError::Unavailable`] after a failed call.
    pub fn release(&self) -> Result<Released, BrokerError> {
        let mut state = self.member.shared.lock()?;
        let now = state.clock.now();
        state.enter();
        let outcome = state.leases.release(self.member.client, now);
        state.apply(&outcome.change);
        outcome.result.map_err(BrokerError::Lease)
    }

    /// Serves this connection over any byte stream with the stdio server's message loop and
    /// bounds (ADR-0015), then leaves. Returns at end of input or on an I/O error.
    pub async fn serve<R, W>(mut self, reader: R, mut writer: W) -> io::Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = LineReader::new(
            BufReader::with_capacity(READ_CHUNK, reader),
            self.member.shared.config.message_limit(),
        );
        let outcome = exchange(&mut self.connection, &mut lines, &mut writer).await;
        self.close();
        outcome
    }

    /// [`BrokerConnection::serve`] over a blocking reader and writer, for a transport that
    /// gives each connection a thread (ADR-0043). The loop is the same: the same framing and
    /// bounds, the same answer to every frame, one reply written and flushed at a time; at the
    /// end of input, or when the reader or the writer fails, the client leaves, and the last
    /// one purges the engine. Returns at end of input or on an I/O error.
    pub fn serve_sync<R, W>(mut self, reader: R, mut writer: W) -> io::Result<()>
    where
        R: io::Read,
        W: io::Write,
    {
        let mut lines = LineReader::new(
            io::BufReader::with_capacity(READ_CHUNK, reader),
            self.member.shared.config.message_limit(),
        );
        let outcome = exchange_blocking(&mut self.connection, &mut lines, &mut writer);
        self.close();
        outcome
    }

    /// Leaves (ADR-0039, decision 10): the client's lease ends and passes to the first waiter,
    /// its place in the queue, its journal and its grant are deleted, and when it was the last
    /// client the engine is purged. What it observed stays while other clients remain.
    pub fn close(mut self) {
        self.leave();
    }

    fn leave(&mut self) {
        if !std::mem::replace(&mut self.open, false) {
            return;
        }
        let client = self.member.client;
        {
            let mut state = self.member.shared.lock_to_delete();
            let now = state.clock.now();
            let outcome = state.leases.disconnect(client, now);
            state.apply(&outcome.change);
            state.engine.forget_client(client);
            state.clients.remove(&client);
            if state.clients.is_empty() {
                state.engine.purge();
            }
        }
        // The place is given back once the client is gone from the state.
        self.seat = None;
    }
}

impl Drop for BrokerConnection {
    fn drop(&mut self) {
        self.leave();
    }
}
