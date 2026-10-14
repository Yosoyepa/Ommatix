//! Deduplication journal of `execute_action` (SDD §2.9, chapter 07 §7.4).
//!
//! Maps a `request_id` to the FNV-1a hash of the canonical arguments and the stored
//! result. The arguments themselves are never kept, so text typed by an action is not
//! stored in clear. The journal is bounded and forgets its oldest entry first.

use std::collections::{BTreeMap, VecDeque};

use crate::contract::{ActionKind, ActionRequest, ActionResult, ExpectedEffect, ScrollDirection};

/// Request IDs remembered per session.
pub const JOURNAL_CAPACITY: usize = 1024;

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

    pub fn lookup(&self, request_id: &str, hash: u64) -> Lookup {
        match self.entries.get(request_id) {
            None => Lookup::New,
            Some((stored, result)) if *stored == hash => Lookup::Replay(result.clone()),
            Some(_) => Lookup::Conflict,
        }
    }

    pub fn record(&mut self, request_id: &str, hash: u64, result: ActionResult) {
        if self
            .entries
            .insert(request_id.to_owned(), (hash, result))
            .is_none()
        {
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
    use ommatix_domain::identity::ActionStatus;

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
