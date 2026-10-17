//! Deduplication journal of `execute_action` (SDD §2.9, chapter 07 §7.4).
//!
//! Maps a `request_id` to the FNV-1a hash of the canonical arguments and the stored
//! result. The arguments themselves are never kept, so text typed by an action is not
//! stored in clear.
//!
//! The results that reported no input are bounded: the journal forgets its oldest one first.
//! A result that reported input, or may have (`indeterminate`), is kept until the session
//! ends: a repeated `request_id` of an operation with effects returns the stored result
//! (SDD §2.9, BDD-30), and a replay under the operator's stop must report what was delivered
//! (SEC-02), so no bound may forget it.

use std::collections::{BTreeMap, VecDeque};

use ommatix_domain::identity::ActionStatus;

use crate::contract::{ActionKind, ActionRequest, ActionResult, ExpectedEffect, ScrollDirection};

/// Request IDs remembered per session whose results reported no input (see the module docs).
pub const JOURNAL_CAPACITY: usize = 1024;

/// Whether `result` reported input, or may have: such a result is kept for the session.
fn reported_input(result: &ActionResult) -> bool {
    result.delivered || result.status == ActionStatus::Indeterminate
}

/// 64-bit FNV-1a. Not cryptographic: it tells a replay from a conflict, nothing more.
pub(super) fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(PRIME))
}

/// Length-prefixed fields, so that no two different requests share an encoding.
struct Canonical(Vec<u8>);

impl Canonical {
    fn field(&mut self, bytes: &[u8]) -> &mut Self {
        self.0
            .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.0.extend_from_slice(bytes);
        self
    }

    fn number(&mut self, n: u64) -> &mut Self {
        self.field(&n.to_le_bytes())
    }
}

/// Hash of every argument except `request_id`.
pub(super) fn arguments_hash(request: &ActionRequest) -> u64 {
    let mut c = Canonical(Vec::new());
    c.field(request.session_id.as_bytes())
        .field(request.snapshot_id.as_bytes())
        .number(request.geometry_epoch)
        .field(request.target_id.as_bytes())
        .number(request.expected_revision);
    match &request.action {
        ActionKind::Click => c.field(b"click"),
        ActionKind::Type { text } => c.field(b"type").field(text.as_bytes()),
        ActionKind::Scroll { direction, steps } => c
            .field(b"scroll")
            .field(match direction {
                ScrollDirection::Up => b"up",
                ScrollDirection::Down => b"down",
            })
            .number(u64::from(*steps)),
    };
    c.number(u64::from(request.dry_run));
    c.field(match request.expected_effect {
        ExpectedEffect::None => b"none",
        ExpectedEffect::FocusChanged => b"focus_changed",
        ExpectedEffect::ValueUpdated => b"value_updated",
        ExpectedEffect::ScrollMoved => b"scroll_moved",
        ExpectedEffect::VisualChange => b"visual_change",
    });
    fnv1a64(&c.0)
}

/// What the journal knows about a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Lookup {
    New,
    /// Same ID, same arguments: the stored result, with no new events.
    Replay(ActionResult),
    /// Same ID, other arguments.
    Conflict,
}

#[derive(Debug, Clone)]
pub(super) struct Journal {
    capacity: usize,
    entries: BTreeMap<String, (u64, ActionResult)>,
    /// The entries the capacity may forget, oldest first: those whose results reported no
    /// input. The results that reported input are not listed here, so they are never forgotten.
    order: VecDeque<String>,
}

impl Journal {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn sizes(&self) -> (usize, usize) {
        (self.entries.len(), self.order.len())
    }

    /// Typed payload and string capacities; excludes B-tree/VecDeque allocation overhead.
    pub fn retained_payload_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|(key, (_, result))| {
                std::mem::size_of::<(String, (u64, ActionResult))>()
                    + key.capacity()
                    + result.request_id.capacity()
                    + result.target_id.capacity()
                    + result.snapshot_id.capacity()
                    + result.error.as_ref().map_or(0, |e| e.message.capacity())
            })
            .sum::<usize>()
            + self.order.capacity() * std::mem::size_of::<String>()
            + self.order.iter().map(String::capacity).sum::<usize>()
    }

    pub fn lookup(&self, request_id: &str, hash: u64) -> Lookup {
        match self.entries.get(request_id) {
            None => Lookup::New,
            Some((stored, result)) if *stored == hash => Lookup::Replay(result.clone()),
            Some(_) => Lookup::Conflict,
        }
    }

    pub fn record(&mut self, request_id: &str, hash: u64, result: ActionResult) {
        let keeps_result = reported_input(&result);
        let replaced = self
            .entries
            .insert(request_id.to_owned(), (hash, result))
            .is_some();
        // The engine records each request once. Should one be recorded again, it takes its
        // place in the order afresh, so that a result that reported input is never listed.
        if replaced {
            self.order.retain(|id| id.as_str() != request_id);
        }
        if !keeps_result {
            self.order.push_back(request_id.to_owned());
        }
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> ActionRequest {
        ActionRequest {
            request_id: "r1".into(),
            session_id: "s".into(),
            snapshot_id: "s1".into(),
            geometry_epoch: 1,
            target_id: "e1".into(),
            expected_revision: 1,
            action: ActionKind::Type { text: text.into() },
            dry_run: true,
            expected_effect: ExpectedEffect::None,
        }
    }

    fn result(id: &str) -> ActionResult {
        ActionResult {
            request_id: id.into(),
            status: ActionStatus::Simulated,
            target_id: "e1".into(),
            snapshot_id: "s1".into(),
            error: None,
            observed_effect: None,
            delivered: false,
        }
    }

    #[test]
    fn rnf_06_journal_is_capacity_plus_effect_bearing_results_after_10000_requests() {
        let mut journal = Journal::new(JOURNAL_CAPACITY);
        let mut effects = 0;
        let mut no_input = 0;
        for i in 0..10_000 {
            let id = format!("r{i}");
            let mut receipt = result(&id);
            // Fabricated receipts exercise retention only; no actuator exists in this test.
            if i % 4 == 0 {
                receipt.delivered = true;
                receipt.status = ActionStatus::InputDelivered;
                effects += 1;
            } else if i % 4 == 1 {
                receipt.status = ActionStatus::Indeterminate;
                effects += 1;
            } else {
                no_input += 1;
            }
            journal.record(&id, 0, receipt);
            assert_eq!(
                journal.sizes(),
                (
                    effects + no_input.min(JOURNAL_CAPACITY),
                    no_input.min(JOURNAL_CAPACITY)
                )
            );
        }
        eprintln!(
            "RNF-06 fabricated journal: entries={}, payload_bytes={}, per_entry={:.3}",
            journal.sizes().0,
            journal.retained_payload_bytes(),
            journal.retained_payload_bytes() as f64 / journal.sizes().0 as f64
        );
        for i in (0..10_000).filter(|i| i % 4 < 2) {
            assert!(matches!(
                journal.lookup(&format!("r{i}"), 0),
                Lookup::Replay(_)
            ));
        }
    }

    #[test]
    #[ignore = "unbounded: see docs/reviews/2026-10-17-engine-state-bounds.md"]
    fn rnf_06_effect_bearing_journal_grows_beyond_capacity_without_delivering_input() {
        for indeterminate in [false, true] {
            let mut journal = Journal::new(JOURNAL_CAPACITY);
            for i in 0..10_000 {
                let id = format!("r{i}");
                let mut receipt = result(&id);
                receipt.delivered = !indeterminate;
                receipt.status = if indeterminate {
                    ActionStatus::Indeterminate
                } else {
                    ActionStatus::InputDelivered
                };
                journal.record(&id, 0, receipt);
            }
            assert_eq!(journal.sizes(), (10_000, 0));
            assert!(journal.sizes().0 <= JOURNAL_CAPACITY);
        }
    }

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_hash_covers_every_argument_but_the_request_id() {
        let a = request("hello");
        let mut b = a.clone();
        b.request_id = "r2".into();
        assert_eq!(arguments_hash(&a), arguments_hash(&b));
        assert_ne!(arguments_hash(&a), arguments_hash(&request("hellp")));
        let mut live = a.clone();
        live.dry_run = false;
        assert_ne!(arguments_hash(&a), arguments_hash(&live));
        // Field boundaries are unambiguous.
        let mut c = a.clone();
        c.session_id = "ss".into();
        c.snapshot_id = "1".into();
        assert_ne!(arguments_hash(&a), arguments_hash(&c));
    }

    /// SEC-02 with chapter 02 (SDD): a repeated `request_id` of an operation with effects
    /// returns the stored result. A result that reported input, or may have, outlives the
    /// bound that forgets the others.
    #[test]
    fn sec_02_a_result_that_reported_input_is_never_forgotten() {
        let mut j = Journal::new(2);
        let delivered = ActionResult {
            status: ActionStatus::InputDelivered,
            delivered: true,
            ..result("sent")
        };
        let uncertain = ActionResult {
            status: ActionStatus::Indeterminate,
            delivered: false,
            ..result("unsure")
        };
        j.record("sent", 1, delivered.clone());
        j.record("unsure", 1, uncertain.clone());
        for id in ["a", "b", "c", "d"] {
            j.record(id, 1, result(id));
        }
        assert_eq!(j.lookup("sent", 1), Lookup::Replay(delivered));
        assert_eq!(j.lookup("unsure", 1), Lookup::Replay(uncertain));
        assert_eq!(
            j.lookup("a", 1),
            Lookup::New,
            "the bound still forgets the oldest result that reported no input"
        );
        assert_eq!(j.lookup("b", 1), Lookup::New);
        assert_eq!(j.lookup("d", 1), Lookup::Replay(result("d")));
    }

    #[test]
    fn replays_conflicts_and_forgets_the_oldest_first() {
        let mut j = Journal::new(2);
        j.record("a", 1, result("a"));
        assert_eq!(j.lookup("a", 1), Lookup::Replay(result("a")));
        assert_eq!(j.lookup("a", 2), Lookup::Conflict);
        assert_eq!(j.lookup("b", 1), Lookup::New);
        j.record("b", 1, result("b"));
        j.record("c", 1, result("c"));
        assert_eq!(j.lookup("a", 1), Lookup::New);
        assert_eq!(j.lookup("c", 1), Lookup::Replay(result("c")));
    }
}
