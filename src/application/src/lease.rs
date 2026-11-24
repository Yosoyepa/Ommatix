//! The lease over the observe-and-act cycle: a table of the clients connected to one engine and
//! of the single lease among them (ADR-0039, decisions 3 to 6).
//!
//! At most one client holds the lease. Only the holder's observation publishes a new snapshot
//! and only the holder may deliver input; with several clients and no holder every observation
//! publishes and nobody delivers. A lease has a number that grows with every grant and a term
//! measured on the engine's monotonic clock; waiting clients are served in arrival order, and a
//! holder cannot renew while another client waits. A waiting client that releases leaves the
//! queue (decision D29 of the 2026-11-24 plan): the others keep their order, and it may ask again
//! later, at the back. With exactly one client connected and no explicit lease, that client holds
//! the lease implicitly, as the engine behaves today.
//!
//! This module is pure: no clock, no I/O, no threads, no serialisation. Time comes in as the
//! argument `now`, the value of [`Clock::now`](crate::Clock::now) when the call entered the
//! engine's critical section. Nothing in the engine calls it yet.

use std::collections::{BTreeSet, VecDeque};
use std::num::NonZeroUsize;
use std::time::Duration;

/// A client connected to the engine (ADR-0039, decision 3): one per connection, given by
/// [`LeaseTable::connect`] and never given again, so a stale identifier cannot reach the lease
/// of a later client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(u64);

impl ClientId {
    /// The identifier as a number, for the research record.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Why the table refused an operation. A refusal changes nothing beyond what the passing of
/// time already changed (reported in [`Outcome::change`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LeaseRefusal {
    /// The client is not connected: it never was, or it disconnected.
    #[error("the client is not connected")]
    UnknownClient,
    /// The bound on connected clients is reached (ADR-0039, decision 8).
    #[error("the bound on connected clients is reached")]
    TooManyClients,
    /// The client asked to release, and it neither holds the lease nor waits for it: nothing
    /// changed. (A waiter that releases leaves the queue: [`Released::LeftQueue`].)
    #[error("the client neither holds the lease nor waits for it")]
    NotHolder,
    /// The holder asked to renew while another client waits (ADR-0039, decision 5): the lease
    /// runs to its end.
    #[error("the lease cannot be renewed while another client waits")]
    RenewalRefused,
    /// No lease can be granted or renewed: its end, `now + term`, does not fit in a
    /// [`Duration`]. The table's time never runs backwards, so none ever will be again.
    #[error("no lease can be granted: its end would not fit in a duration")]
    TimeExhausted,
}

/// Why a lease ended (ADR-0039, decision 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndCause {
    /// Its holder released it.
    Released,
    /// Its term ended.
    Term,
    /// Its holder disconnected.
    Disconnect,
    /// The operator's stop or a purge revoked it ([`LeaseTable::revoke`]).
    Revoked,
}

/// A lease granted to a client, with its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseGrant {
    pub client: ClientId,
    /// Grows with every grant and is never given again.
    pub number: u64,
}

/// A lease that ended, with its number and the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseEnd {
    pub client: ClientId,
    pub number: u64,
    pub cause: EndCause,
}

/// What one operation changed, so that the caller acts on it without asking again.
///
/// An implicit lease has no number, so a client that comes to hold it is not listed in
/// `granted`; `spend_snapshot` still reports it when that client did not hold the lease before.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseChange {
    /// The leases granted, in the order they were granted.
    pub granted: Vec<LeaseGrant>,
    /// The leases that ended, in the order they ended. One call can end a lease by its term
    /// and then the lease that passed to the first waiter; numbers grow, so ordering the
    /// grants and ends together by number, a grant before the end of the same number, gives
    /// the order in which they happened.
    pub ended: Vec<LeaseEnd>,
    /// True exactly when the lease passed to a client that did not hold it before the call: the
    /// caller must spend the current snapshot, so that the new holder observes before it acts
    /// (ADR-0039, decision 5). An implicit lease turning ordinary does not spend.
    pub spend_snapshot: bool,
}

impl LeaseChange {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.granted.is_empty() && self.ended.is_empty() && !self.spend_snapshot
    }
}

/// The answer to one operation and what it changed. The change is reported on a refusal too,
/// because every operation first settles time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Outcome<T> {
    pub result: Result<T, LeaseRefusal>,
    pub change: LeaseChange,
}

/// What [`LeaseTable::acquire`] did for the asker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acquired {
    /// The asker holds the lease now, under this number: nobody held it, or the asker was the
    /// first waiter and the lease that ended as this call settled time passed to it.
    Granted { number: u64 },
    /// The asker held it and nobody waits: its term now runs from this call, under the same
    /// number, and nothing is spent.
    Renewed { number: u64 },
    /// Another client holds it: the asker waits at this position, counted from 1 for the next
    /// client to be served. Asking again keeps the asker's place.
    Queued { position: usize },
    /// The asker is the only client and holds the lease implicitly; nothing changed.
    Implicit,
}

/// What [`LeaseTable::release`] did for the client that asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Released {
    /// The client held the lease and released it; the lease passed at once to the first
    /// waiter, if any. This includes a waiter granted the lease while the same call settled time:
    /// by then it is the holder.
    Lease,
    /// The client is the only one and holds the lease implicitly; it keeps it and nothing changed.
    Implicit,
    /// The client was waiting and left the queue. Nothing else changed: the holder, the lease's
    /// number and the order of the other waiters stay as they were, and nothing is spent, since
    /// the lease passed to nobody. Asking again puts the client at the back of the queue.
    LeftQueue,
}

/// Where a client stands with respect to the lease ([`LeaseTable::standing`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The client holds the lease: an explicit one under its number, or, as the only client,
    /// the implicit one, which has no number.
    Holder { number: Option<u64> },
    /// The client waits at this position, counted from 1 for the next client to be served.
    Waiting { position: usize },
    /// The client neither holds the lease nor waits for it.
    None,
}

/// The explicit lease: its holder, its number and the instant it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lease {
    client: ClientId,
    number: u64,
    ends_at: Duration,
}

/// The clients connected to one engine and the single lease among them (ADR-0039, decisions 3
/// to 6).
///
/// Every operation takes `now`, the value of the engine's monotonic clock when the call entered
/// the critical section, and first settles time: a lease whose end is at or before `now` ends,
/// and the first waiter is granted at `now`. A `now` earlier than one already seen is read as
/// the latest one, so time never runs backwards in the table. Collections are ordered, so equal
/// histories give equal tables.
///
/// Durations. The only sum on durations is a lease's end, `now + term`, and it is checked: a
/// lease is granted only when its end fits in a [`Duration`], so every lease lasts exactly its
/// term and ends strictly after the instant it was granted at. When the sum does not fit, no
/// lease is granted, and since the table's time never runs backwards none ever will be again:
/// [`LeaseTable::acquire`] is refused with [`LeaseRefusal::TimeExhausted`] (to grant, to queue
/// or to renew; a renewal refused this way leaves the lease to run to its end); a lease that
/// ends grants nobody and empties the queue; and a second connection leaves the first client's
/// implicit lease without making it an ordinary one, so nobody holds the lease and nobody
/// delivers. A sole client's implicit lease has no term and is not affected. Because every
/// operation settles time and every lease ends after the instant it was granted at, a lease
/// always ends later than the last `now` the table has seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseTable {
    max_clients: NonZeroUsize,
    term: Duration,
    latest: Duration,
    next_client: u64,
    next_number: u64,
    clients: BTreeSet<ClientId>,
    lease: Option<Lease>,
    queue: VecDeque<ClientId>,
}

/// The bookkeeping of one operation: the settled instant, the holder before the last step, and
/// what changed so far.
struct Step {
    now: Duration,
    holder: Option<ClientId>,
    change: LeaseChange,
}

impl LeaseTable {
    /// An empty table that admits at most `max_clients` clients and grants leases of `term`
    /// (ADR-0039, open point O3). `None` when `term` is zero: a lease that ends as it is granted
    /// would grant nothing.
    pub fn new(max_clients: NonZeroUsize, term: Duration) -> Option<Self> {
        if term.is_zero() {
            return None;
        }
        Some(Self {
            max_clients,
            term,
            latest: Duration::ZERO,
            next_client: 1,
            next_number: 1,
            clients: BTreeSet::new(),
            lease: None,
            queue: VecDeque::new(),
        })
    }

    /// Connects a new client (ADR-0039, decision 3). A first client holds the lease
    /// implicitly. When a second client connects while the first holds it implicitly, that
    /// implicit lease becomes an ordinary lease of the first client: a new number, a term that
    /// starts at `now`, and nothing spent; when that term's end cannot be represented, the first
    /// client keeps no lease and nobody holds it. Refused with [`LeaseRefusal::TooManyClients`]
    /// at the bound, or when no identifier is left to give.
    pub fn connect(&mut self, now: Duration) -> Outcome<ClientId> {
        let mut step = self.begin(now);
        let result = if self.clients.len() >= self.max_clients.get() || self.next_client == u64::MAX
        {
            Err(LeaseRefusal::TooManyClients)
        } else {
            if let Some(only) = self.implicit_holder() {
                self.grant(only, &mut step);
            }
            let client = ClientId(self.next_client);
            self.next_client += 1;
            self.clients.insert(client);
            self.track(&mut step);
            Ok(client)
        };
        Self::finish(step, result)
    }

    /// Disconnects `client`: removes it, its place in the queue and its lease if it holds one,
    /// in which case the first waiter is granted at once (ADR-0039, decision 5), unless no
    /// lease can be granted any more (see [`LeaseTable`], durations).
    pub fn disconnect(&mut self, client: ClientId, now: Duration) -> Outcome<()> {
        let mut step = self.begin(now);
        let result = if self.clients.remove(&client) {
            self.queue.retain(|waiter| *waiter != client);
            if self.lease.is_some_and(|lease| lease.client == client) {
                self.end_lease(EndCause::Disconnect, &mut step);
                self.grant_first_waiter(&mut step);
            }
            self.track(&mut step);
            Ok(())
        } else {
            Err(LeaseRefusal::UnknownClient)
        };
        Self::finish(step, result)
    }

    /// Asks for the lease (ADR-0039, decisions 4 and 5). Nobody holds it: the client is
    /// granted. Another client holds it: the client joins the queue once, in arrival order.
    /// The client holds it: a renewal to `now + term` under the same number, allowed only when
    /// nobody waits. A sole client holds it implicitly and nothing changes. When `now + term`
    /// does not fit in a [`Duration`], every other case is refused with
    /// [`LeaseRefusal::TimeExhausted`] and nothing changes.
    pub fn acquire(&mut self, client: ClientId, now: Duration) -> Outcome<Acquired> {
        let mut step = self.begin(now);
        let ends_at = step.now.checked_add(self.term);
        let granted_now = self.lease.filter(|lease| {
            lease.client == client
                && step
                    .change
                    .granted
                    .last()
                    .is_some_and(|grant| grant.number == lease.number)
        });
        let result = if !self.clients.contains(&client) {
            Err(LeaseRefusal::UnknownClient)
        } else if self.implicit_holder() == Some(client) {
            Ok(Acquired::Implicit)
        } else if let Some(lease) = granted_now {
            Ok(Acquired::Granted {
                number: lease.number,
            })
        } else if let Some(ends_at) = ends_at {
            match self.lease {
                Some(lease) if lease.client != client => {
                    let position = if let Some(index) =
                        self.queue.iter().position(|waiter| *waiter == client)
                    {
                        index + 1
                    } else {
                        self.queue.push_back(client);
                        self.queue.len()
                    };
                    Ok(Acquired::Queued { position })
                }
                Some(lease) if self.queue.is_empty() => {
                    self.lease = Some(Lease { ends_at, ..lease });
                    Ok(Acquired::Renewed {
                        number: lease.number,
                    })
                }
                Some(_) => Err(LeaseRefusal::RenewalRefused),
                None => self
                    .grant(client, &mut step)
                    .map(|number| Acquired::Granted { number })
                    .ok_or(LeaseRefusal::TimeExhausted),
            }
        } else {
            Err(LeaseRefusal::TimeExhausted)
        };
        Self::finish(step, result)
    }

    /// Releases what `client` has of the lease, after settling time at `now`.
    ///
    /// - The holder releases the lease, which passes at once to the first waiter (ADR-0039,
    ///   decision 5): [`Released::Lease`]. A waiter that the settling of this same call granted
    ///   the lease is the holder by then, and releases it the same way.
    /// - A sole client keeps its implicit lease, which cannot be released: [`Released::Implicit`].
    /// - A waiter leaves the queue (decision D29): [`Released::LeftQueue`]. The others keep their
    ///   order and nothing is spent.
    /// - Any other client neither holds nor waits: [`LeaseRefusal::NotHolder`], nothing changes.
    pub fn release(&mut self, client: ClientId, now: Duration) -> Outcome<Released> {
        let mut step = self.begin(now);
        let result = if !self.clients.contains(&client) {
            Err(LeaseRefusal::UnknownClient)
        } else if self.lease.is_some_and(|lease| lease.client == client) {
            self.end_lease(EndCause::Released, &mut step);
            self.grant_first_waiter(&mut step);
            Ok(Released::Lease)
        } else if self.implicit_holder() == Some(client) {
            Ok(Released::Implicit)
        } else if let Some(index) = self.queue.iter().position(|waiter| *waiter == client) {
            self.queue.remove(index);
            Ok(Released::LeftQueue)
        } else {
            Err(LeaseRefusal::NotHolder)
        };
        Self::finish(step, result)
    }

    /// The operator's stop or a purge (ADR-0039, decision 5): the lease ends and the queue is
    /// emptied, and nobody is granted; clients ask again. A sole client still holds the lease
    /// implicitly afterwards; the operator's stop itself is enforced by the engine's gate. It
    /// takes no `now` and settles nothing: a lease always ends later than the last `now` the
    /// table has seen (see [`LeaseTable`], durations), so there is nothing to settle.
    pub fn revoke(&mut self) -> LeaseChange {
        let mut step = Step {
            now: self.latest,
            holder: self.holder(),
            change: LeaseChange::default(),
        };
        self.queue.clear();
        self.end_lease(EndCause::Revoked, &mut step);
        step.change
    }

    /// Whether `client` may deliver live input (ADR-0039, decision 4): true only for the holder
    /// of a lease that has not ended, or for a sole client holding it implicitly. Asked once per
    /// call, with the `now` at which the call entered the critical section.
    pub fn may_deliver(&mut self, client: ClientId, now: Duration) -> Outcome<bool> {
        let step = self.begin(now);
        let result = if self.clients.contains(&client) {
            Ok(self.holder() == Some(client))
        } else {
            Err(LeaseRefusal::UnknownClient)
        };
        Self::finish(step, result)
    }

    /// Whether the observation of `client` publishes a new snapshot (ADR-0039, decision 4):
    /// true for the holder and, when nobody holds the lease, for any client; false for a client
    /// while another holds it, which receives the page already published.
    pub fn publishes(&mut self, client: ClientId, now: Duration) -> Outcome<bool> {
        let step = self.begin(now);
        let result = if self.clients.contains(&client) {
            Ok(self.holder().is_none_or(|holder| holder == client))
        } else {
            Err(LeaseRefusal::UnknownClient)
        };
        Self::finish(step, result)
    }

    /// Where `client` stands as of the last settled instant (ADR-0039, decision 5: what the
    /// broker reports with each response). It reads and changes nothing, settles no time and
    /// takes no `now`: the caller asks it right after the operation that settled the call.
    /// A client the table does not know stands nowhere.
    pub fn standing(&self, client: ClientId) -> Standing {
        if !self.clients.contains(&client) {
            return Standing::None;
        }
        match self.lease {
            Some(lease) if lease.client == client => Standing::Holder {
                number: Some(lease.number),
            },
            None if self.implicit_holder() == Some(client) => Standing::Holder { number: None },
            _ => self
                .queue
                .iter()
                .position(|waiter| *waiter == client)
                .map_or(Standing::None, |index| Standing::Waiting {
                    position: index + 1,
                }),
        }
    }

    /// The client that holds the lease, explicitly or implicitly, as of the last settled
    /// instant.
    fn holder(&self) -> Option<ClientId> {
        match self.lease {
            Some(lease) => Some(lease.client),
            None => self.implicit_holder(),
        }
    }

    /// The sole client when no explicit lease exists (ADR-0039, decision 6).
    fn implicit_holder(&self) -> Option<ClientId> {
        if self.lease.is_none() && self.clients.len() == 1 {
            self.clients.first().copied()
        } else {
            None
        }
    }

    /// Starts an operation: reads `now` (never earlier than an instant already seen) and
    /// settles time.
    fn begin(&mut self, now: Duration) -> Step {
        let now = now.max(self.latest);
        self.latest = now;
        let mut step = Step {
            now,
            holder: self.holder(),
            change: LeaseChange::default(),
        };
        if self.lease.is_some_and(|lease| lease.ends_at <= now) {
            self.end_lease(EndCause::Term, &mut step);
            self.grant_first_waiter(&mut step);
        }
        step
    }

    fn finish<T>(step: Step, result: Result<T, LeaseRefusal>) -> Outcome<T> {
        Outcome {
            result,
            change: step.change,
        }
    }

    /// Records a passing of the lease: a holder that differs from the one before the last step
    /// spends the snapshot.
    fn track(&self, step: &mut Step) {
        let holder = self.holder();
        if holder.is_some() && holder != step.holder {
            step.change.spend_snapshot = true;
        }
        step.holder = holder;
    }

    fn end_lease(&mut self, cause: EndCause, step: &mut Step) {
        if let Some(lease) = self.lease.take() {
            step.change.ended.push(LeaseEnd {
                client: lease.client,
                number: lease.number,
                cause,
            });
            self.track(step);
        }
    }

    /// Grants the lease to the first waiter still connected, if any. When no lease can be
    /// granted any more, nobody is granted and the queue is emptied: no waiter could be served.
    fn grant_first_waiter(&mut self, step: &mut Step) {
        if step.now.checked_add(self.term).is_none() {
            self.queue.clear();
            return;
        }
        while let Some(waiter) = self.queue.pop_front() {
            if self.clients.contains(&waiter) {
                self.grant(waiter, step);
                return;
            }
        }
    }

    /// Grants a lease of one term from the settled instant and returns its number, or grants
    /// nothing and returns `None` when the end, `now + term`, does not fit in a [`Duration`].
    /// The number grows by one per grant; it would stop growing only after `u64::MAX` grants,
    /// which no process reaches.
    fn grant(&mut self, client: ClientId, step: &mut Step) -> Option<u64> {
        let ends_at = step.now.checked_add(self.term)?;
        let number = self.next_number;
        self.next_number = self.next_number.saturating_add(1);
        self.lease = Some(Lease {
            client,
            number,
            ends_at,
        });
        step.change.granted.push(LeaseGrant { client, number });
        self.track(step);
        Some(number)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const TERM: Duration = Duration::from_secs(10);
    const BOUND: usize = 4;

    fn table() -> LeaseTable {
        LeaseTable::new(NonZeroUsize::new(BOUND).unwrap(), TERM).unwrap()
    }

    fn at(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    fn connect(table: &mut LeaseTable, now: Duration) -> ClientId {
        table.connect(now).result.unwrap()
    }

    fn delivers(table: &mut LeaseTable, client: ClientId, now: Duration) -> bool {
        table.may_deliver(client, now).result.unwrap()
    }

    fn publishes(table: &mut LeaseTable, client: ClientId, now: Duration) -> bool {
        table.publishes(client, now).result.unwrap()
    }

    // ----- direct tests, one behaviour each -----

    #[test]
    fn a_zero_term_cannot_build_a_table() {
        assert!(LeaseTable::new(NonZeroUsize::new(1).unwrap(), Duration::ZERO).is_none());
    }

    #[test]
    fn standing_tells_the_holder_its_number_the_waiters_their_place_and_changes_nothing() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        assert_eq!(
            table.standing(a),
            Standing::Holder { number: None },
            "the implicit lease has no number"
        );
        let b = connect(&mut table, at(1));
        let c = connect(&mut table, at(1));
        assert_eq!(table.standing(a), Standing::Holder { number: Some(1) });
        assert_eq!(table.standing(b), Standing::None);
        assert_eq!(
            table.acquire(c, at(2)).result,
            Ok(Acquired::Queued { position: 1 })
        );
        assert_eq!(
            table.acquire(b, at(2)).result,
            Ok(Acquired::Queued { position: 2 })
        );
        let before = table.clone();
        assert_eq!(table.standing(c), Standing::Waiting { position: 1 });
        assert_eq!(table.standing(b), Standing::Waiting { position: 2 });
        assert_eq!(table, before, "asking changes nothing");

        // The lease passes to c at the end of a's term; b moves up.
        assert!(!delivers(&mut table, a, at(11)));
        assert_eq!(table.standing(a), Standing::None);
        assert_eq!(table.standing(c), Standing::Holder { number: Some(2) });
        assert_eq!(table.standing(b), Standing::Waiting { position: 1 });

        // A client that left, or never came, stands nowhere.
        assert!(table.disconnect(b, at(12)).result.is_ok());
        assert_eq!(table.standing(b), Standing::None);
        assert_eq!(table.standing(ClientId(99)), Standing::None);

        // The stop revokes: nobody holds, nobody waits.
        assert_eq!(
            table.acquire(a, at(12)).result,
            Ok(Acquired::Queued { position: 1 })
        );
        let _ = table.revoke();
        assert_eq!(table.standing(a), Standing::None);
        assert_eq!(table.standing(c), Standing::None);
    }

    #[test]
    fn standing_agrees_with_may_deliver_and_with_every_answer_of_acquire() {
        let mut table = table();
        let mut clients = Vec::new();
        let mut seed: u64 = 0x5EED_0042;
        let mut next = |n: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % n
        };
        for step in 0..4_000_u64 {
            let now = Duration::from_millis(step * 700);
            match next(5) {
                0 if clients.len() < BOUND => clients.push(connect(&mut table, now)),
                1 if !clients.is_empty() => {
                    let index =
                        usize::try_from(next(u64::try_from(clients.len()).unwrap())).unwrap();
                    let gone = clients.remove(index);
                    assert!(table.disconnect(gone, now).result.is_ok());
                }
                2 if !clients.is_empty() => {
                    let index =
                        usize::try_from(next(u64::try_from(clients.len()).unwrap())).unwrap();
                    let asker = clients[index];
                    let standing = match table.acquire(asker, now).result {
                        Ok(Acquired::Granted { number } | Acquired::Renewed { number }) => {
                            Standing::Holder {
                                number: Some(number),
                            }
                        }
                        Ok(Acquired::Queued { position }) => Standing::Waiting { position },
                        Ok(Acquired::Implicit) => Standing::Holder { number: None },
                        Err(_) => table.standing(asker),
                    };
                    assert_eq!(table.standing(asker), standing, "step {step}");
                }
                3 if !clients.is_empty() => {
                    let index =
                        usize::try_from(next(u64::try_from(clients.len()).unwrap())).unwrap();
                    let _ = table.release(clients[index], now);
                }
                _ => {}
            }
            // Settle at `now`, then compare: exactly the holder may deliver, and the waiters'
            // places are 1, 2, … without a gap.
            let mut places = Vec::new();
            for client in clients.clone() {
                let holds = delivers(&mut table, client, now);
                match table.standing(client) {
                    Standing::Holder { .. } => assert!(holds, "step {step}"),
                    Standing::Waiting { position } => {
                        assert!(!holds, "step {step}");
                        places.push(position);
                    }
                    Standing::None => assert!(!holds, "step {step}"),
                }
            }
            places.sort_unstable();
            assert!(
                places.iter().enumerate().all(|(i, p)| *p == i + 1),
                "step {step}: {places:?}"
            );
        }
    }

    #[test]
    fn a_sole_client_holds_the_lease_implicitly_and_acts_as_today() {
        let mut table = table();
        let connected = table.connect(at(0));
        let a = connected.result.unwrap();
        assert_eq!(connected.change.granted, []);
        assert!(connected.change.spend_snapshot, "the lease passed to a");
        for seconds in [0, 5, 100, 10_000] {
            assert!(delivers(&mut table, a, at(seconds)));
            assert!(publishes(&mut table, a, at(seconds)));
        }
        let asked = table.acquire(a, at(10_001));
        assert_eq!(asked.result, Ok(Acquired::Implicit));
        assert!(asked.change.is_empty());
    }

    #[test]
    fn a_second_connection_turns_the_implicit_lease_into_an_ordinary_one_with_a_term() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let second = table.connect(at(5));
        let b = second.result.unwrap();
        assert_eq!(
            second.change.granted,
            vec![LeaseGrant {
                client: a,
                number: 1
            }]
        );
        assert_eq!(second.change.ended, []);
        assert!(!second.change.spend_snapshot, "a keeps the lease it held");
        assert!(delivers(&mut table, a, at(14)));
        assert!(!delivers(&mut table, b, at(14)));
        assert!(!publishes(&mut table, b, at(14)));
        let ended = table.may_deliver(a, at(15));
        assert_eq!(
            ended.result,
            Ok(false),
            "the term started at the second connection"
        );
        assert_eq!(
            ended.change.ended,
            vec![LeaseEnd {
                client: a,
                number: 1,
                cause: EndCause::Term
            }]
        );
    }

    #[test]
    fn with_nobody_holding_every_client_publishes_and_nobody_delivers() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.release(a, at(1));
        for client in [a, b, c] {
            assert!(publishes(&mut table, client, at(2)));
            assert!(!delivers(&mut table, client, at(2)));
        }
    }

    #[test]
    fn a_grant_to_a_new_holder_spends_the_snapshot() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.release(a, at(1));
        let granted = table.acquire(b, at(2));
        assert_eq!(granted.result, Ok(Acquired::Granted { number: 2 }));
        assert_eq!(
            granted.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 2
            }]
        );
        assert!(granted.change.spend_snapshot);
        assert!(delivers(&mut table, b, at(2)));
    }

    #[test]
    fn a_client_without_the_lease_receives_the_published_page_and_cannot_deliver() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        assert!(publishes(&mut table, a, at(1)));
        assert!(!publishes(&mut table, b, at(1)));
        assert!(!delivers(&mut table, b, at(1)));
    }

    #[test]
    fn waiting_clients_are_served_in_arrival_order_and_each_waits_once() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        assert_eq!(
            table.acquire(c, at(1)).result,
            Ok(Acquired::Queued { position: 1 })
        );
        assert_eq!(
            table.acquire(b, at(1)).result,
            Ok(Acquired::Queued { position: 2 })
        );
        assert_eq!(
            table.acquire(c, at(2)).result,
            Ok(Acquired::Queued { position: 1 }),
            "asking again keeps the place"
        );
        let released = table.release(a, at(3));
        assert_eq!(
            released.change.granted,
            vec![LeaseGrant {
                client: c,
                number: 2
            }]
        );
        assert!(released.change.spend_snapshot);
        let released = table.release(c, at(4));
        assert_eq!(
            released.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 3
            }]
        );
    }

    #[test]
    fn a_renewal_extends_the_term_keeps_the_number_and_spends_nothing() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let _b = connect(&mut table, at(0));
        let renewed = table.acquire(a, at(8));
        assert_eq!(renewed.result, Ok(Acquired::Renewed { number: 1 }));
        assert!(renewed.change.is_empty());
        assert!(delivers(&mut table, a, at(17)));
        assert!(!delivers(&mut table, a, at(18)));
    }

    #[test]
    fn a_renewal_is_refused_while_another_client_waits_and_the_lease_runs_to_its_end() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let refused = table.acquire(a, at(8));
        assert_eq!(refused.result, Err(LeaseRefusal::RenewalRefused));
        assert!(refused.change.is_empty());
        assert!(delivers(&mut table, a, at(9)));
        let ended = table.may_deliver(a, at(10));
        assert_eq!(ended.result, Ok(false));
        assert_eq!(
            ended.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 2
            }]
        );
        assert!(ended.change.spend_snapshot);
    }

    #[test]
    fn a_waiter_served_as_its_own_call_settles_time_is_told_it_was_granted() {
        let mut table = table();
        let _a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let asked = table.acquire(b, at(10));
        assert_eq!(asked.result, Ok(Acquired::Granted { number: 2 }));
        assert_eq!(
            asked.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 2
            }]
        );
        assert!(asked.change.spend_snapshot);
    }

    #[test]
    fn a_lease_ends_at_its_term_whatever_its_holder_does_next() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let _b = connect(&mut table, at(0));
        assert!(delivers(&mut table, a, at(9)));
        let late = table.release(a, at(10));
        assert_eq!(
            late.result,
            Err(LeaseRefusal::NotHolder),
            "ended at its term"
        );
        assert_eq!(late.change.ended[0].cause, EndCause::Term);
        let asked = table.acquire(a, at(10));
        assert_eq!(asked.result, Ok(Acquired::Granted { number: 2 }));
        assert!(asked.change.spend_snapshot, "a new lease is a new holding");
    }

    #[test]
    fn only_the_holder_can_release_the_lease_and_a_waiter_that_releases_leaves_the_queue() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        assert_eq!(table.release(b, at(2)).result, Ok(Released::LeftQueue));
        assert_eq!(
            table.acquire(b, at(2)).result,
            Ok(Acquired::Queued { position: 1 })
        );
        assert!(delivers(&mut table, a, at(2)));
    }

    #[test]
    fn a_waiter_that_releases_leaves_the_queue_and_the_others_keep_their_order() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let d = connect(&mut table, at(0));
        for (waiter, position) in [(b, 1), (c, 2), (d, 3)] {
            assert_eq!(
                table.acquire(waiter, at(1)).result,
                Ok(Acquired::Queued { position })
            );
        }
        let left = table.release(c, at(2));
        assert_eq!(left.result, Ok(Released::LeftQueue));
        assert!(
            left.change.is_empty(),
            "the lease passed to nobody: {left:?}"
        );
        assert_eq!(table.standing(a), Standing::Holder { number: Some(1) });
        assert_eq!(table.standing(b), Standing::Waiting { position: 1 });
        assert_eq!(table.standing(c), Standing::None);
        assert_eq!(table.standing(d), Standing::Waiting { position: 2 });
        // The lease passes to b, then to d: c, which left, is never granted.
        let passed = table.release(a, at(3));
        assert_eq!(passed.result, Ok(Released::Lease));
        assert_eq!(
            passed.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 2
            }]
        );
        let passed = table.release(b, at(4));
        assert_eq!(
            passed.change.granted,
            vec![LeaseGrant {
                client: d,
                number: 3
            }]
        );
        assert_eq!(table.standing(c), Standing::None);
    }

    #[test]
    fn a_waiter_that_left_the_queue_asks_again_at_the_back() {
        let mut table = table();
        let _a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        assert_eq!(table.release(b, at(2)).result, Ok(Released::LeftQueue));
        assert_eq!(table.standing(c), Standing::Waiting { position: 1 });
        assert_eq!(
            table.acquire(b, at(3)).result,
            Ok(Acquired::Queued { position: 2 }),
            "at the back, behind c"
        );
        // When a's term ends, c is served first.
        assert!(!delivers(&mut table, b, at(10)));
        assert_eq!(table.standing(c), Standing::Holder { number: Some(2) });
        assert_eq!(table.standing(b), Standing::Waiting { position: 1 });
    }

    #[test]
    fn a_waiter_granted_while_its_release_settles_time_releases_the_lease() {
        let mut table = table();
        let _a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        // a's lease ends at 10: settling time grants b, which then releases the lease to c.
        let released = table.release(b, at(10));
        assert_eq!(released.result, Ok(Released::Lease));
        assert_eq!(
            released.change.granted,
            vec![
                LeaseGrant {
                    client: b,
                    number: 2
                },
                LeaseGrant {
                    client: c,
                    number: 3
                }
            ]
        );
        assert_eq!(
            released
                .change
                .ended
                .iter()
                .map(|end| end.cause)
                .collect::<Vec<_>>(),
            vec![EndCause::Term, EndCause::Released]
        );
        assert!(released.change.spend_snapshot);
        assert_eq!(table.standing(b), Standing::None);
        assert_eq!(table.standing(c), Standing::Holder { number: Some(3) });
    }

    #[test]
    fn a_release_by_a_client_that_neither_holds_nor_waits_changes_nothing() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let before = table.clone();
        let refused = table.release(c, at(1));
        assert_eq!(refused.result, Err(LeaseRefusal::NotHolder));
        assert!(refused.change.is_empty());
        assert_eq!(table, before);
        assert_eq!(table.standing(a), Standing::Holder { number: Some(1) });
        assert_eq!(table.standing(b), Standing::Waiting { position: 1 });
        // A sole client's release keeps its implicit lease.
        let mut alone = self::table();
        let only = connect(&mut alone, at(0));
        assert_eq!(alone.release(only, at(1)).result, Ok(Released::Implicit));
        assert_eq!(alone.standing(only), Standing::Holder { number: None });
    }

    #[test]
    fn a_disconnect_of_the_holder_grants_the_first_waiter_and_spends_the_snapshot() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        let gone = table.disconnect(a, at(2));
        assert_eq!(
            gone.change.ended,
            vec![LeaseEnd {
                client: a,
                number: 1,
                cause: EndCause::Disconnect
            }]
        );
        assert_eq!(
            gone.change.granted,
            vec![LeaseGrant {
                client: b,
                number: 2
            }]
        );
        assert!(gone.change.spend_snapshot);
        assert!(delivers(&mut table, b, at(2)));
        assert!(!delivers(&mut table, c, at(2)));
    }

    #[test]
    fn a_client_left_alone_holds_the_lease_implicitly_and_spends_the_snapshot() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.release(a, at(1));
        let gone = table.disconnect(a, at(2));
        assert!(
            gone.change.spend_snapshot,
            "b did not hold the lease before"
        );
        assert!(
            gone.change.granted.is_empty(),
            "an implicit lease has no number"
        );
        assert!(delivers(&mut table, b, at(2)));
    }

    #[test]
    fn a_waiter_that_disconnects_leaves_the_queue() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        let _ = table.disconnect(b, at(2));
        let released = table.release(a, at(3));
        assert_eq!(
            released.change.granted,
            vec![LeaseGrant {
                client: c,
                number: 2
            }]
        );
    }

    #[test]
    fn revoke_leaves_no_holder_and_no_queue() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        let revoked = table.revoke();
        assert_eq!(
            revoked.ended,
            vec![LeaseEnd {
                client: a,
                number: 1,
                cause: EndCause::Revoked
            }]
        );
        assert_eq!(revoked.granted, []);
        assert!(!revoked.spend_snapshot);
        for client in [a, b, c] {
            assert!(!delivers(&mut table, client, at(1)));
            assert!(publishes(&mut table, client, at(1)));
        }
        assert_eq!(
            table.acquire(c, at(2)).result,
            Ok(Acquired::Granted { number: 2 }),
            "clients ask again"
        );
    }

    #[test]
    fn with_one_client_a_revoke_leaves_its_implicit_lease() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        assert!(table.revoke().is_empty());
        assert!(delivers(&mut table, a, at(1)));
    }

    #[test]
    fn an_unknown_or_disconnected_client_is_refused() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.disconnect(b, at(1));
        for client in [b, ClientId(1_000)] {
            let refused = Some(LeaseRefusal::UnknownClient);
            assert_eq!(table.acquire(client, at(2)).result.err(), refused);
            assert_eq!(table.release(client, at(2)).result.err(), refused);
            assert_eq!(table.disconnect(client, at(2)).result.err(), refused);
            assert_eq!(table.may_deliver(client, at(2)).result.err(), refused);
            assert_eq!(table.publishes(client, at(2)).result.err(), refused);
        }
        assert!(delivers(&mut table, a, at(2)));
    }

    #[test]
    fn a_connection_beyond_the_bound_is_refused_and_identifiers_are_never_reused() {
        let mut table = table();
        let first: Vec<ClientId> = (0..BOUND).map(|_| connect(&mut table, at(0))).collect();
        assert_eq!(
            table.connect(at(0)).result,
            Err(LeaseRefusal::TooManyClients)
        );
        let _ = table.disconnect(first[0], at(1));
        let again = connect(&mut table, at(1));
        assert!(first.iter().all(|old| *old != again));
        assert!(again > first[BOUND - 1]);
    }

    #[test]
    fn a_settle_and_an_operation_in_one_call_report_every_end_and_grant_in_order() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        let gone = table.disconnect(b, at(10));
        assert_eq!(
            gone.change.ended,
            vec![
                LeaseEnd {
                    client: a,
                    number: 1,
                    cause: EndCause::Term
                },
                LeaseEnd {
                    client: b,
                    number: 2,
                    cause: EndCause::Disconnect
                },
            ]
        );
        assert_eq!(
            gone.change.granted,
            vec![
                LeaseGrant {
                    client: b,
                    number: 2
                },
                LeaseGrant {
                    client: c,
                    number: 3
                },
            ]
        );
        assert!(gone.change.spend_snapshot);
    }

    #[test]
    fn an_explicit_lease_of_a_sole_client_that_ends_leaves_it_the_implicit_one_without_spending() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let _ = table.disconnect(b, at(1));
        let ended = table.may_deliver(a, at(10));
        assert_eq!(ended.result, Ok(true));
        assert_eq!(ended.change.ended[0].cause, EndCause::Term);
        assert!(!ended.change.spend_snapshot);
    }

    #[test]
    fn time_that_runs_backwards_or_to_the_end_of_durations_neither_panics_nor_revives_a_lease() {
        let almost_max = Duration::MAX.checked_sub(Duration::from_secs(1)).unwrap();
        let mut table = LeaseTable::new(NonZeroUsize::new(2).unwrap(), Duration::MAX).unwrap();
        let a = connect(&mut table, almost_max);
        let b = connect(&mut table, almost_max);
        assert!(
            !delivers(&mut table, a, almost_max),
            "no lease whose end cannot be represented"
        );
        let ended = table.may_deliver(a, Duration::MAX);
        assert_eq!(ended.result, Ok(false), "nobody holds the lease");
        assert!(
            publishes(&mut table, b, Duration::ZERO),
            "now never runs backwards"
        );
        assert!(!delivers(&mut table, a, Duration::ZERO));
    }

    // ----- the end of representable time (review R-U2-1) -----

    /// The last instant at which a lease of `TERM` can be granted: it ends at `Duration::MAX`.
    fn last_fit() -> Duration {
        Duration::MAX.checked_sub(TERM).unwrap()
    }

    fn after_last_fit() -> Duration {
        last_fit().checked_add(Duration::from_nanos(1)).unwrap()
    }

    /// The clients that may deliver at `now`, asked one after the other at the same instant.
    fn deliverers(table: &mut LeaseTable, clients: &[ClientId], now: Duration) -> Vec<ClientId> {
        clients
            .iter()
            .copied()
            .filter(|client| delivers(table, *client, now))
            .collect()
    }

    #[test]
    fn a_lease_granted_at_the_last_instant_that_fits_lasts_its_whole_term() {
        let mut table = table();
        let a = connect(&mut table, last_fit());
        let b = connect(&mut table, last_fit());
        let before_end = Duration::MAX.checked_sub(Duration::from_nanos(1)).unwrap();
        assert_eq!(deliverers(&mut table, &[a, b], last_fit()), [a]);
        assert_eq!(deliverers(&mut table, &[a, b], before_end), [a]);
        let ended = table.may_deliver(a, Duration::MAX);
        assert_eq!(ended.result, Ok(false), "it ends at Duration::MAX");
        assert_eq!(ended.change.ended[0].cause, EndCause::Term);
        assert_eq!(deliverers(&mut table, &[a, b], Duration::MAX), []);
    }

    #[test]
    fn no_lease_is_granted_one_nanosecond_after_the_last_instant_that_fits() {
        let mut table = table();
        let a = connect(&mut table, after_last_fit());
        let second = table.connect(after_last_fit());
        let b = second.result.unwrap();
        assert_eq!(
            second.change.granted,
            [],
            "the implicit lease does not turn ordinary"
        );
        assert!(!second.change.spend_snapshot);
        for client in [a, b] {
            let asked = table.acquire(client, after_last_fit());
            assert_eq!(asked.result, Err(LeaseRefusal::TimeExhausted));
            assert!(asked.change.is_empty());
            assert!(publishes(&mut table, client, after_last_fit()));
        }
        assert_eq!(deliverers(&mut table, &[a, b], after_last_fit()), []);
    }

    #[test]
    fn at_the_end_of_time_two_clients_never_deliver_at_the_same_instant() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let asked = table.acquire(b, Duration::MAX);
        assert_eq!(asked.result, Err(LeaseRefusal::TimeExhausted));
        assert_eq!(
            asked.change.granted,
            [],
            "a's lease ended and nobody was granted"
        );
        assert_eq!(deliverers(&mut table, &[a, b], Duration::MAX), []);
        assert_eq!(deliverers(&mut table, &[b, a], Duration::MAX), []);
    }

    #[test]
    fn at_the_end_of_time_waiters_are_not_granted_and_the_queue_is_emptied() {
        let mut table = table();
        let a = connect(&mut table, at(0));
        let b = connect(&mut table, at(0));
        let c = connect(&mut table, at(0));
        let _ = table.acquire(b, at(1));
        let _ = table.acquire(c, at(1));
        let settled = table.may_deliver(b, Duration::MAX);
        assert_eq!(settled.result, Ok(false), "b could not be granted");
        assert_eq!(settled.change.granted, []);
        assert!(!settled.change.spend_snapshot);
        assert_eq!(deliverers(&mut table, &[a, b, c], Duration::MAX), []);
        assert_eq!(deliverers(&mut table, &[c, b, a], Duration::MAX), []);
        assert_eq!(
            table.acquire(c, Duration::MAX).result,
            Err(LeaseRefusal::TimeExhausted),
            "c no longer waits and cannot ask again"
        );
    }

    #[test]
    fn a_renewal_whose_end_cannot_be_represented_is_refused_and_the_lease_runs_to_its_end() {
        let mut table = table();
        let five = Duration::from_secs(5);
        let granted_at = last_fit().checked_sub(five).unwrap();
        let a = connect(&mut table, granted_at);
        let b = connect(&mut table, granted_at);
        let end = Duration::MAX.checked_sub(five).unwrap();
        let refused = table.acquire(a, after_last_fit());
        assert_eq!(refused.result, Err(LeaseRefusal::TimeExhausted));
        assert!(refused.change.is_empty());
        let before_end = end.checked_sub(Duration::from_nanos(1)).unwrap();
        assert_eq!(deliverers(&mut table, &[a, b], before_end), [a]);
        assert_eq!(deliverers(&mut table, &[a, b], end), []);
    }

    #[test]
    fn a_waiter_left_alone_at_the_end_of_time_holds_the_implicit_lease() {
        let mut table = table();
        let a = connect(&mut table, last_fit());
        let b = connect(&mut table, last_fit());
        let _ = table.acquire(b, last_fit());
        let gone = table.disconnect(a, after_last_fit());
        assert_eq!(gone.change.granted, [], "no ordinary lease fits");
        assert!(
            gone.change.spend_snapshot,
            "b did not hold the lease before"
        );
        assert_eq!(
            table.acquire(b, Duration::MAX).result,
            Ok(Acquired::Implicit)
        );
        assert!(delivers(&mut table, b, Duration::MAX));
    }

    #[test]
    fn a_sole_clients_implicit_lease_has_no_term_and_outlasts_the_end_of_time() {
        let mut table = table();
        let a = connect(&mut table, Duration::MAX);
        assert!(delivers(&mut table, a, Duration::MAX));
        assert!(publishes(&mut table, a, Duration::MAX));
        assert_eq!(
            table.acquire(a, Duration::MAX).result,
            Ok(Acquired::Implicit)
        );
    }

    // ----- generated interleavings against a reference model -----

    /// Fixed-seed generator (splitmix64): the same runs every time, with no random crate.
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// A number in `0..n`, for `n > 0`.
        fn below(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Op {
        Connect,
        Disconnect(ClientId),
        Acquire(ClientId),
        Release(ClientId),
        Revoke,
        Observe(ClientId),
        Deliver(ClientId),
        Wait(Duration),
    }

    /// How often each kind of outcome occurred in a run.
    #[derive(Debug, Default)]
    struct Seen {
        grants: u64,
        queued: u64,
        renewals: u64,
        refused_renewals: u64,
        ends: BTreeMap<EndCause, u64>,
        allowed_deliveries: u64,
        refused_deliveries: u64,
        deliveries: u64,
        stale_citations: u64,
        spends: u64,
        not_holder: u64,
        left_queue: u64,
        unknown: u64,
        too_many: u64,
        implicit: u64,
        time_exhausted: u64,
    }

    /// The reference model: the engine's snapshot counter as the broker would drive it, and
    /// shadows of the queue and of the lease's term computed from the reported changes.
    struct Model {
        honour_spend: bool,
        current: u64,
        spent: BTreeSet<u64>,
        used: BTreeSet<u64>,
        /// Per client, the snapshot it last received and the moment (a sequence) it did.
        received: BTreeMap<ClientId, (u64, u64)>,
        /// Per holder, the moment its current holding began.
        holding_since: BTreeMap<ClientId, u64>,
        holder: Option<ClientId>,
        sequence: u64,
        /// The clients waiting, in the order they began to wait.
        waiting: Vec<ClientId>,
        /// Per waiter, the position it was last told and the grants counted at that moment.
        told: BTreeMap<ClientId, (u64, u64)>,
        /// The explicit lease as the reported changes describe it, with the end of its term.
        lease: Option<(ClientId, u64, Duration)>,
        last_number: u64,
        seen: Seen,
    }

    impl Model {
        fn new(honour_spend: bool) -> Self {
            Self {
                honour_spend,
                current: 0,
                spent: BTreeSet::new(),
                used: BTreeSet::new(),
                received: BTreeMap::new(),
                holding_since: BTreeMap::new(),
                holder: None,
                sequence: 0,
                waiting: Vec::new(),
                told: BTreeMap::new(),
                lease: None,
                last_number: 0,
                seen: Seen::default(),
            }
        }

        fn tick(&mut self) -> u64 {
            self.sequence += 1;
            self.sequence
        }

        /// Applies what one operation reported, and checks invariants 2, 3, 10 on it.
        fn absorb(
            &mut self,
            table: &LeaseTable,
            change: &LeaseChange,
            now: Duration,
        ) -> Result<(), String> {
            // A lease is granted before it ends and numbers grow, so ordering by number, with
            // a grant before the end of the same number, gives the order the events occurred.
            let mut events: Vec<(u64, Option<LeaseGrant>, Option<LeaseEnd>)> = change
                .granted
                .iter()
                .map(|grant| (grant.number, Some(*grant), None))
                .chain(
                    change
                        .ended
                        .iter()
                        .map(|end| (end.number, None, Some(*end))),
                )
                .collect();
            events.sort_by_key(|(number, grant, _)| (*number, grant.is_none()));
            for (_, grant, end) in events {
                if let Some(end) = end {
                    self.end(end)?;
                }
                if let Some(grant) = grant {
                    self.grant(grant, now)?;
                }
            }
            if self.lease.is_none() && now.checked_add(TERM).is_none() {
                // No lease can be granted any more: nobody waits for one.
                self.waiting.clear();
                self.told.clear();
            }
            if change.spend_snapshot {
                self.seen.spends += 1;
                if self.honour_spend {
                    self.spent.insert(self.current);
                }
            }
            let holder = table.holder();
            if holder != self.holder {
                if let Some(holder) = holder {
                    let since = self.tick();
                    self.holding_since.insert(holder, since);
                }
                self.holder = holder;
            }
            Ok(())
        }

        fn end(&mut self, end: LeaseEnd) -> Result<(), String> {
            *self.seen.ends.entry(end.cause).or_default() += 1;
            match self.lease {
                Some((client, number, _)) if client == end.client && number == end.number => {
                    self.lease = None;
                    Ok(())
                }
                _ => Err(format!("an end of a lease the model did not hold: {end:?}")),
            }
        }

        /// A grant: numbers grow (invariant 2), the first waiter is served first (3) within
        /// the position it was told (10), and the term runs from `now` (4).
        fn grant(&mut self, grant: LeaseGrant, now: Duration) -> Result<(), String> {
            if grant.number <= self.last_number {
                return Err(format!(
                    "invariant 2: lease number {} after {}",
                    grant.number, self.last_number
                ));
            }
            self.last_number = grant.number;
            self.seen.grants += 1;
            if let Some(first) = self.waiting.first().copied() {
                if first != grant.client {
                    return Err(format!(
                        "invariant 3: granted {:?} while {first:?} waited first",
                        grant.client
                    ));
                }
                self.waiting.remove(0);
                let (position, grants_then) = self.told.remove(&first).unwrap_or_default();
                if self.seen.grants - grants_then > position {
                    return Err(format!(
                        "invariant 10: {first:?} told position {position} waited {} grants",
                        self.seen.grants - grants_then
                    ));
                }
            }
            // Invariant 12: a lease ends strictly after the instant it was granted at, by
            // exactly its term (compared with the table in `check`).
            let Some(end) = now.checked_add(TERM).filter(|end| *end > now) else {
                return Err(format!(
                    "invariant 12: {grant:?} granted at {now:?}, where its end cannot be represented"
                ));
            };
            self.lease = Some((grant.client, grant.number, end));
            Ok(())
        }

        fn forget(&mut self, client: ClientId) {
            self.received.remove(&client);
            self.holding_since.remove(&client);
            self.waiting.retain(|waiter| *waiter != client);
            self.told.remove(&client);
        }

        /// The holder the model expects from its own shadows.
        fn expected_holder(&self, table: &LeaseTable) -> Option<ClientId> {
            match self.lease {
                Some((client, _, _)) => Some(client),
                None if table.clients.len() == 1 => table.clients.first().copied(),
                None => None,
            }
        }

        /// A delivery the table allowed: the engine delivers when the client cites the current,
        /// unspent snapshot, and every such delivery must cite a snapshot the client received
        /// after its holding began (invariant 6).
        fn deliver(&mut self, client: ClientId) -> Result<(), String> {
            let Some(&(snapshot, received_at)) = self.received.get(&client) else {
                self.seen.stale_citations += 1;
                return Ok(());
            };
            if snapshot != self.current || self.spent.contains(&snapshot) {
                self.seen.stale_citations += 1;
                return Ok(());
            }
            let since = self.holding_since.get(&client).copied().unwrap_or(u64::MAX);
            if received_at <= since {
                return Err(format!(
                    "invariant 6: {client:?} delivered on snapshot {snapshot}, received before its grant"
                ));
            }
            if !self.used.insert(snapshot) {
                return Err(format!("invariant 6: snapshot {snapshot} authorised twice"));
            }
            self.spent.insert(snapshot);
            self.seen.deliveries += 1;
            Ok(())
        }
    }

    fn pick(rng: &mut Rng, table: &LeaseTable, issued: u64) -> ClientId {
        let connected: Vec<ClientId> = table.clients.iter().copied().collect();
        if connected.is_empty() || rng.below(10) == 0 {
            ClientId(1 + rng.below(issued + 2))
        } else {
            connected[usize::try_from(rng.below(connected.len() as u64)).unwrap()]
        }
    }

    fn generate(rng: &mut Rng, table: &LeaseTable, issued: u64) -> Op {
        let term_ms = u64::try_from(TERM.as_millis()).unwrap();
        match rng.below(100) {
            0..=9 => Op::Connect,
            10..=15 => Op::Disconnect(pick(rng, table, issued)),
            16..=35 => Op::Acquire(pick(rng, table, issued)),
            36..=45 => Op::Release(pick(rng, table, issued)),
            46..=47 => Op::Revoke,
            48..=67 => Op::Observe(pick(rng, table, issued)),
            68..=87 => Op::Deliver(pick(rng, table, issued)),
            _ => {
                let millis = if rng.below(3) == 0 {
                    term_ms + rng.below(2 * term_ms)
                } else {
                    rng.below(term_ms / 2)
                };
                Op::Wait(Duration::from_millis(millis))
            }
        }
    }

    /// Applies one operation to the table and the model.
    fn apply(
        table: &mut LeaseTable,
        model: &mut Model,
        op: Op,
        now: Duration,
    ) -> Result<(), String> {
        match op {
            Op::Connect => {
                let outcome = table.connect(now);
                model.absorb(table, &outcome.change, now)?;
                match outcome.result {
                    Ok(_) => {}
                    Err(LeaseRefusal::TooManyClients) if table.clients.len() == BOUND => {
                        model.seen.too_many += 1;
                    }
                    Err(refusal) => return Err(format!("connect refused: {refusal:?}")),
                }
            }
            Op::Disconnect(client) => {
                let outcome = table.disconnect(client, now);
                model.absorb(table, &outcome.change, now)?;
                if outcome.result.is_ok() {
                    model.forget(client);
                }
                count_unknown(model, outcome.result.err())?;
            }
            Op::Acquire(client) => apply_acquire(table, model, client, now)?,
            Op::Release(client) => {
                let outcome = table.release(client, now);
                model.absorb(table, &outcome.change, now)?;
                match outcome.result {
                    Ok(Released::Lease | Released::Implicit) => {}
                    Ok(Released::LeftQueue) => {
                        // Only a waiter leaves, the lease passes to nobody, and the others keep
                        // their order (the queue is compared with the model in `check`).
                        if !model.waiting.contains(&client) {
                            return Err(format!("{client:?} left a queue it was not in"));
                        }
                        if outcome
                            .change
                            .ended
                            .iter()
                            .any(|end| end.cause == EndCause::Released)
                        {
                            return Err("a waiter leaving released a lease".into());
                        }
                        model.waiting.retain(|waiter| *waiter != client);
                        model.told.remove(&client);
                        model.seen.left_queue += 1;
                    }
                    Err(LeaseRefusal::NotHolder) => {
                        if model.waiting.contains(&client) || model.holder == Some(client) {
                            return Err(format!("{client:?} refused while it held or waited"));
                        }
                        model.seen.not_holder += 1;
                    }
                    Err(refusal) => count_unknown(model, Some(refusal))?,
                }
            }
            Op::Revoke => {
                let change = table.revoke();
                model.waiting.clear();
                model.told.clear();
                model.absorb(table, &change, now)?;
            }
            Op::Observe(client) => {
                let outcome = table.publishes(client, now);
                model.absorb(table, &outcome.change, now)?;
                match outcome.result {
                    Ok(publishes) => {
                        if publishes {
                            model.current += 1;
                        }
                        let at = model.tick();
                        model.received.insert(client, (model.current, at));
                    }
                    Err(refusal) => count_unknown(model, Some(refusal))?,
                }
            }
            Op::Deliver(client) => {
                let outcome = table.may_deliver(client, now);
                model.absorb(table, &outcome.change, now)?;
                match outcome.result {
                    Ok(true) => {
                        model.seen.allowed_deliveries += 1;
                        model.deliver(client)?;
                    }
                    Ok(false) => model.seen.refused_deliveries += 1,
                    Err(refusal) => count_unknown(model, Some(refusal))?,
                }
            }
            Op::Wait(_) => {}
        }
        Ok(())
    }

    /// An acquire, with invariant 5 on renewals and the position told against arrival order.
    fn apply_acquire(
        table: &mut LeaseTable,
        model: &mut Model,
        client: ClientId,
        now: Duration,
    ) -> Result<(), String> {
        let outcome = table.acquire(client, now);
        model.absorb(table, &outcome.change, now)?;
        match outcome.result {
            Ok(Acquired::Granted { .. }) => {}
            Ok(Acquired::Renewed { number }) => {
                if !model.waiting.is_empty() || !outcome.change.is_empty() {
                    return Err("invariant 5: a renewal with a client waiting".into());
                }
                match (model.lease, now.checked_add(TERM)) {
                    (Some((holder, held, _)), Some(end)) if holder == client && held == number => {
                        model.lease = Some((client, number, end));
                    }
                    _ => return Err("a renewal of a lease the model did not hold".into()),
                }
                model.seen.renewals += 1;
            }
            Ok(Acquired::Queued { position }) => {
                if !model.waiting.contains(&client) {
                    model.waiting.push(client);
                }
                let index = model.waiting.iter().position(|w| *w == client).unwrap();
                if index + 1 != position {
                    return Err(format!("told position {position}, model {}", index + 1));
                }
                let position = u64::try_from(position).unwrap();
                model.told.insert(client, (position, model.seen.grants));
                model.seen.queued += 1;
            }
            Ok(Acquired::Implicit) => model.seen.implicit += 1,
            Err(LeaseRefusal::TimeExhausted) => {
                if now.checked_add(TERM).is_some() {
                    return Err("refused for exhausted time while a lease still fits".into());
                }
                model.seen.time_exhausted += 1;
            }
            Err(LeaseRefusal::RenewalRefused) => {
                if model.waiting.is_empty() {
                    return Err("a renewal refused with nobody waiting".into());
                }
                model.seen.refused_renewals += 1;
            }
            Err(refusal) => count_unknown(model, Some(refusal))?,
        }
        Ok(())
    }

    fn count_unknown(model: &mut Model, refusal: Option<LeaseRefusal>) -> Result<(), String> {
        match refusal {
            None => Ok(()),
            Some(LeaseRefusal::UnknownClient) => {
                model.seen.unknown += 1;
                Ok(())
            }
            Some(other) => Err(format!("unexpected refusal {other:?}")),
        }
    }

    /// Invariants that hold after every step, at the last instant the table settled (a wait
    /// alone settles nothing): the structure (1, 9), the queue against the model's arrival
    /// order (3), the term (4), and the answers of both queries for every connected client
    /// (7, 8), which must change nothing at the instant already settled.
    fn check(table: &mut LeaseTable, model: &Model) -> Result<(), String> {
        let now = table.latest;
        if table.clients.len() > BOUND {
            return Err("invariant 9: more clients than the bound".into());
        }
        if table.queue.len() >= table.clients.len().max(1) {
            return Err("invariant 9: a queue as long as the clients".into());
        }
        let mut queued = BTreeSet::new();
        for waiter in &table.queue {
            if !table.clients.contains(waiter) || !queued.insert(*waiter) {
                return Err(format!("invariant 1: waiter {waiter:?} gone or twice"));
            }
            if table.lease.is_none_or(|lease| lease.client == *waiter) {
                return Err(format!(
                    "invariant 1: waiter {waiter:?} with no other holder"
                ));
            }
        }
        if let Some(lease) = table.lease
            && !table.clients.contains(&lease.client)
        {
            return Err("invariant 1: the holder is not connected".into());
        }
        if !table.queue.iter().eq(model.waiting.iter()) {
            return Err(format!(
                "invariant 3: queue {:?}, arrival order {:?}",
                table.queue, model.waiting
            ));
        }
        let shadow = model.lease;
        let actual = table
            .lease
            .map(|lease| (lease.client, lease.number, lease.ends_at));
        if shadow != actual {
            return Err(format!("invariant 4: lease {actual:?}, model {shadow:?}"));
        }
        if let Some((_, _, end)) = shadow
            && end <= now
        {
            return Err(format!(
                "invariant 4: a lease alive at {now:?} past its end"
            ));
        }
        let expected = model.expected_holder(table);
        let connected: Vec<ClientId> = table.clients.iter().copied().collect();
        for client in connected {
            let delivers = table.may_deliver(client, now);
            let publishes = table.publishes(client, now);
            if !delivers.change.is_empty() || !publishes.change.is_empty() {
                return Err("a query at a settled instant changed the table".into());
            }
            let (delivers, publishes) = (delivers.result.unwrap(), publishes.result.unwrap());
            if table.clients.len() == 1 && !(delivers && publishes) {
                return Err("invariant 7: a sole client that cannot act".into());
            }
            if delivers != (expected == Some(client)) {
                return Err(format!("invariant 8: {client:?} delivers={delivers}"));
            }
            if publishes != expected.is_none_or(|holder| holder == client) {
                return Err(format!("invariant 8: {client:?} publishes={publishes}"));
            }
        }
        Ok(())
    }

    /// One generated run; returns what it saw, or the first invariant broken.
    fn run(seed: u64, start: Duration, steps: usize, honour_spend: bool) -> Result<Seen, String> {
        let mut rng = Rng(seed);
        let mut table = table();
        let mut model = Model::new(honour_spend);
        let mut now = start;
        let mut history = Vec::with_capacity(steps);
        for step in 0..steps {
            let op = generate(&mut rng, &table, table.next_client);
            if let Op::Wait(by) = op {
                now = now.saturating_add(by);
            }
            history.push((op, now));
            apply(&mut table, &mut model, op, now)
                .and_then(|()| check(&mut table, &model))
                .map_err(|broken| format!("seed {seed}, step {step}, {op:?}: {broken}"))?;
        }

        // Invariant 11: the same operations give an equal table.
        let mut replayed = self::table();
        let mut replay_model = Model::new(honour_spend);
        for (op, at) in &history {
            apply(&mut replayed, &mut replay_model, *op, *at)?;
        }
        if replayed != table {
            return Err(format!(
                "invariant 11: seed {seed} replayed to another table"
            ));
        }

        // Invariant 10, to the end: with every holder letting its term end, the queue drains
        // within as many terms as it has waiters.
        let waiters = table.queue.len();
        for _ in 0..waiters {
            now = now.saturating_add(TERM);
            if let Some(&client) = table.clients.first() {
                let settled = table.publishes(client, now);
                model.absorb(&table, &settled.change, now)?;
            }
            check(&mut table, &model)?;
        }
        if !table.queue.is_empty() {
            return Err(format!(
                "invariant 10: seed {seed} left {:?} waiting",
                table.queue
            ));
        }
        Ok(model.seen)
    }

    const SEEDS: [u64; 4] = [1, 0x5EED, 0xC0FF_EE11, 0x0DD5_EED5];
    const STEPS: usize = 4_000;
    /// Seeds of runs that start 150 terms before the end of representable time: time moves by
    /// about 400 terms in a run, so each run crosses the last instant at which a lease fits.
    const SEEDS_NEAR_THE_END: [u64; 2] = [0xE1D, 0x0F_714E];

    fn near_the_end() -> Duration {
        Duration::MAX.checked_sub(TERM * 150).unwrap()
    }

    #[test]
    fn generated_interleavings_keep_the_lease_invariants() {
        let mut total = Seen::default();
        let runs = SEEDS
            .map(|seed| (seed, Duration::ZERO))
            .into_iter()
            .chain(SEEDS_NEAR_THE_END.map(|seed| (seed, near_the_end())));
        for (seed, start) in runs {
            let seen = run(seed, start, STEPS, true).unwrap_or_else(|broken| panic!("{broken}"));
            total.grants += seen.grants;
            total.queued += seen.queued;
            total.renewals += seen.renewals;
            total.refused_renewals += seen.refused_renewals;
            for (cause, count) in seen.ends {
                *total.ends.entry(cause).or_default() += count;
            }
            total.allowed_deliveries += seen.allowed_deliveries;
            total.refused_deliveries += seen.refused_deliveries;
            total.deliveries += seen.deliveries;
            total.stale_citations += seen.stale_citations;
            total.spends += seen.spends;
            total.not_holder += seen.not_holder;
            total.left_queue += seen.left_queue;
            total.unknown += seen.unknown;
            total.too_many += seen.too_many;
            total.implicit += seen.implicit;
            total.time_exhausted += seen.time_exhausted;
        }
        for (what, count) in [
            ("grants", total.grants),
            ("queued acquires", total.queued),
            ("renewals", total.renewals),
            ("refused renewals", total.refused_renewals),
            ("allowed deliveries", total.allowed_deliveries),
            ("refused deliveries", total.refused_deliveries),
            ("deliveries", total.deliveries),
            ("stale citations", total.stale_citations),
            ("spent snapshots", total.spends),
            ("refused releases", total.not_holder),
            ("waiters that left the queue", total.left_queue),
            ("unknown clients", total.unknown),
            ("clients beyond the bound", total.too_many),
            ("implicit acquires", total.implicit),
            ("acquires refused for exhausted time", total.time_exhausted),
        ] {
            assert!(count > 0, "no {what} in {total:?}");
        }
        for cause in [
            EndCause::Released,
            EndCause::Term,
            EndCause::Disconnect,
            EndCause::Revoked,
        ] {
            assert!(
                total.ends.get(&cause).copied().unwrap_or(0) > 0,
                "no end by {cause:?} in {total:?}"
            );
        }
    }

    #[test]
    fn the_reference_model_catches_a_caller_that_does_not_spend_on_a_new_holder() {
        let caught: Vec<String> = SEEDS
            .iter()
            .filter_map(|seed| run(*seed, Duration::ZERO, STEPS, false).err())
            .collect();
        assert!(!caught.is_empty(), "no seed caught the missing spend");
        assert!(
            caught.iter().all(|broken| broken.contains("invariant 6")),
            "{caught:?}"
        );
    }
}
