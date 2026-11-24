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
//!    decide under the meet of the operator's policy and that grant. A client without a valid
//!    grant is served under [`Grant::default`], which permits no live input and no values
//!    (ADR-0040, decision 7).
//! 5. `observe_screen` publishes a new snapshot only for the holder, or for any client while
//!    nobody holds the lease; any other client gets the published page served again for it,
//!    or `STALE_SNAPSHOT` when nothing is published. `execute_action` runs a dry run for any
//!    client (decision 4), and a client's own retry, a request identifier its journal holds,
//!    goes to the engine whoever holds the lease: the engine answers it from the journal (a
//!    replay, or `REQUEST_CONFLICT` for other arguments) and never delivers it again, so a
//!    client whose input was delivered learns so even after it lost the lease. Any other live
//!    input reaches the gate only for the holder, and any other client's is refused before the
//!    gate with `POLICY_DENIED`, delivering and recording nothing. `get_engine_metrics` is
//!    answered as by the stdio server.
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
//! A client that leaves takes its lease, its place in the queue, its journal and its grant
//! with it; the last one to leave purges the engine, as the end of the stdio server's
//! connection does (ADR-0039, decision 10).
//!
//! Nothing in the executable constructs a broker; only tests do (this crate's and the acceptance
//! suite's scenarios): the executable serves stdio only.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ommatix_application::contract::{ActionRequest, ActionResult, ErrorCode, ToolError};
use ommatix_application::lease::{Acquired, ClientId, LeaseChange, LeaseRefusal, Released};
use ommatix_application::{
    Clock, Engine, EngineConfig, Grant, GrantedPolicy, JOURNAL_CLIENTS, LeaseTable, PolicyStore,
    Ports,
};
use ommatix_domain::identity::ActionStatus;
use rmcp::model::Tool;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::config::McpConfig;
use crate::dto::LeaseArgument;
use crate::protocol::{Backend, Connection};
use crate::server::exchange;
use crate::tools::{self, CallError, LeaseReport, ToolCall, ToolOutput};

/// The refusal of a live action from a client that does not hold the lease and whose journal
/// does not know its request identifier (ADR-0039, decisions 4 and 6). It names the limit and
/// quotes nothing from the screen or the agent (ADR-0021, ADR-0040 decision 9).
const NO_LEASE: &str = "live input needs the input lease and this client does not hold it; \
                        nothing was delivered";

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
        }
    }
}

impl std::error::Error for BrokerError {}

/// What a broker retains, for research and tests: counts only, never screen content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerState {
    /// The clients connected, each with the grant it was admitted with.
    pub clients: BTreeSet<ClientId>,
    /// Whether the policy the engine reads outside a call differs from the operator's: true
    /// only if a client's grant stayed installed after its call.
    pub grant_installed: bool,
    /// [`Engine::retained_state_sizes`].
    pub engine: BTreeMap<&'static str, usize>,
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
    config: McpConfig,
    tools: Arc<[Tool]>,
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
    /// The grant of each connected client; `None` for a client without a valid grant.
    grants: BTreeMap<ClientId, Option<Grant>>,
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
        mut ports: Ports,
        config: McpConfig,
        term: Duration,
        max_clients: usize,
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
            grants: BTreeMap::new(),
        };
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                config,
                tools: tools::descriptors().into(),
            }),
        })
    }

    pub fn config(&self) -> McpConfig {
        self.shared.config
    }

    /// Admits a client with its grant (ADR-0039, decision 3; ADR-0040, decision 7). `None` is
    /// a client without a valid grant: it observes without values and delivers nothing. The
    /// client's identifier comes from the lease table; the agent never chooses it.
    ///
    /// # Errors
    ///
    /// [`BrokerError::Lease`] with [`LeaseRefusal::TooManyClients`] at the bound,
    /// [`BrokerError::Unavailable`] after a failed call.
    pub fn connect(&self, grant: Option<Grant>) -> Result<BrokerConnection, BrokerError> {
        let client = {
            let mut state = self.shared.lock()?;
            let now = state.clock.now();
            state.enter();
            let outcome = state.leases.connect(now);
            state.apply(&outcome.change);
            let client = outcome.result.map_err(BrokerError::Lease)?;
            state.grants.insert(client, grant);
            client
        };
        let member = Member {
            shared: Arc::clone(&self.shared),
            client,
        };
        Ok(BrokerConnection {
            connection: Connection::with_backend(
                Box::new(member.clone()),
                self.shared.config,
                Arc::clone(&self.shared.tools),
            ),
            member,
            open: true,
        })
    }

    /// What the broker retains now, `None` after a failed call.
    pub fn retained(&self) -> Option<BrokerState> {
        let state = self.shared.lock().ok()?;
        Some(BrokerState {
            clients: state.grants.keys().copied().collect(),
            grant_installed: state.granted.current() != state.operator.current(),
            engine: state.engine.retained_state_sizes(),
        })
    }
}

impl Shared {
    /// The lock for anything that may observe or deliver: refused after a failed call.
    fn lock(&self) -> Result<MutexGuard<'_, State>, BrokerError> {
        self.state.lock().map_err(|_| BrokerError::Unavailable)
    }

    /// The lock for what only deletes state (the purge request, a client that leaves), which
    /// is safe even after a failed call.
    fn lock_to_delete(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl State {
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
        let reports = asked || state.grants.len() > 1;
        let granted = Arc::clone(&state.granted);
        let _installed = granted.install(state.grants.get(&self.client).cloned().flatten());
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
            // A client's own retry is answered from its journal before the lease is asked for:
            // a known identifier never reaches the gate (ADR-0039, decision 11).
            ToolCall::Act(request)
                if request.dry_run
                    || holds
                    || engine.knows_request(self.client, &request.request_id) =>
            {
                let result = engine.execute_action_for(self.client, &request);
                tools::action(&result, engine.last_action_timings())
            }
            ToolCall::Act(request) => tools::action(&refused_without_lease(&request), None),
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

/// The refusal of a live action from a client without the lease, shaped as the gate's own
/// refusals: the request's identifiers, `rejected`, nothing delivered.
fn refused_without_lease(request: &ActionRequest) -> ActionResult {
    ActionResult {
        request_id: request.request_id.clone(),
        status: ActionStatus::Rejected,
        target_id: request.target_id.clone(),
        snapshot_id: request.snapshot_id.clone(),
        error: Some(ToolError::new(ErrorCode::PolicyDenied, NO_LEASE)),
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
        let limit = self.member.shared.config.message_limit();
        let outcome = exchange(&mut self.connection, limit, reader, &mut writer).await;
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
        let mut state = self.member.shared.lock_to_delete();
        let now = state.clock.now();
        let outcome = state.leases.disconnect(client, now);
        state.apply(&outcome.change);
        state.engine.forget_client(client);
        state.grants.remove(&client);
        if state.grants.is_empty() {
            state.engine.purge();
        }
    }
}

impl Drop for BrokerConnection {
    fn drop(&mut self) {
        self.leave();
    }
}
