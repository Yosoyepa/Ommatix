//! A grant: limits a client holds over the policy-shaped part of the local policy, its delivery
//! budget and its term, and the meet that applies them (ADR-0040, decisions 1 and 2).
//!
//! A grant never widens the operator's policy. The policy the engine acts under for a client
//! is [`Grant::narrow`] of the local policy, and a meet is never above either operand: an
//! application, a window, live input or the reading of values is allowed under it only when
//! the policy and the grant both allow it. The operator's stop is not part of a grant.
//!
//! [`GrantedPolicy`] applies a grant to every read of the policy (ADR-0040, decision 6): a
//! store that wraps the operator's store, into which the caller serving one client installs
//! that client's grant for the length of the call.
//!
//! A grant's delivery budget and term are not part of the policy: [`Grant::narrow`] ignores them.
//! [`GrantLedger`] keeps, for one connected client, the count of its results that reported input
//! and the ceilings of its budget and term, and says what the client may still do at a call
//! (ADR-0040, decisions 2, 6 and 11; proposed). The broker of the MCP adapter keeps one ledger per
//! client and checks both limits under its one lock at each call's entry.
//!
//! No clock, no I/O, no serialisation: the caller passes the instant in. The broker of the MCP
//! adapter (ADR-0040, proposed) installs each client's grant through [`GrantedPolicy`] for the
//! length of its call.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::contract::ActionResult;
use crate::ports::{LocalPolicy, PolicyStore, WindowPin};

/// What a client may do within the local policy (ADR-0040, decision 2): the policy-shaped
/// limits (applications, window classes, a window pin, live input and the reading of values),
/// and two limits the broker keeps outside the policy, a delivery budget and a term.
///
/// The default grant allows nothing: no application, no live input, no values; its budget and
/// term are `None`, no limit, as an absent key of the grants file is. Collections are ordered, so
/// two grants that allow the same things compare equal whatever the order they were built in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grant {
    /// The applications the grant allows, compared as exact strings, as the action gate does.
    pub applications: BTreeSet<String>,
    /// Per application, the only window classes the grant lets live input go to. An
    /// application without an entry is not restricted by class; an entry with no classes
    /// allows no window of that application.
    pub window_classes: BTreeMap<String, BTreeSet<String>>,
    /// The grant's pin on one window, `None` when it sets none.
    pub window_pin: Option<WindowPin>,
    /// Whether the client may deliver live input at all.
    pub deliver_input: bool,
    /// Whether the client may read field values.
    pub read_values: bool,
    /// The most results that report input the client may have, counted from its admission:
    /// `None` is no limit, `Some(0)` observe only. Enforced by the broker, not by
    /// [`Grant::narrow`].
    pub delivery_budget: Option<u32>,
    /// How long after its admission the client may deliver: `None` for the whole connection.
    /// Enforced by the broker, not by [`Grant::narrow`].
    pub term: Option<Duration>,
}

impl Grant {
    /// The policy the engine acts under for a client holding this grant (ADR-0040,
    /// decision 1): what `policy` allows and this grant allows, and nothing else.
    ///
    /// - `actuation_enabled` and `include_values` hold only when both sides say so.
    /// - `allowed_applications` lists, sorted and each once, the applications both allow.
    /// - `window_classes` has, for each application that stays allowed, the classes both
    ///   sides list when both restrict it, the list of the side that restricts it when only
    ///   one does, and no entry when neither does. An empty intersection stays an entry with
    ///   an empty list: the window-scope gate treats such an entry as "no class is allowed",
    ///   so every window of the application is refused, never read as "not restricted".
    /// - `window_pin` is the pin of the side that sets one; two equal pins give that pin;
    ///   two different pins, or an unset one, give [`WindowPin::Unset`], which allows no
    ///   live input.
    /// - `stopped` is copied from the policy: the operator's stop is not part of a grant, so
    ///   a grant can neither set nor clear it.
    ///
    /// The delivery budget and the term play no part here: two grants that differ only in them
    /// narrow every policy to the same policy. The broker enforces them ([`GrantLedger`]).
    #[must_use]
    pub fn narrow(&self, policy: &LocalPolicy) -> LocalPolicy {
        let allowed_applications: BTreeSet<&String> = policy
            .allowed_applications
            .iter()
            .filter(|application| self.applications.contains(application.as_str()))
            .collect();
        let mut window_classes = BTreeMap::new();
        for application in &allowed_applications {
            let from_policy = policy
                .window_classes
                .get(application.as_str())
                .map(|classes| classes.iter().cloned().collect::<BTreeSet<String>>());
            let from_grant = self.window_classes.get(application.as_str()).cloned();
            if let Some(classes) = meet_classes(from_policy, from_grant) {
                window_classes.insert((*application).clone(), classes.into_iter().collect());
            }
        }
        LocalPolicy {
            actuation_enabled: policy.actuation_enabled && self.deliver_input,
            include_values: policy.include_values && self.read_values,
            allowed_applications: allowed_applications.into_iter().cloned().collect(),
            window_classes,
            window_pin: meet_pins(policy.window_pin, self.window_pin),
            stopped: policy.stopped,
        }
    }

    /// The grant that allows exactly what this grant and `other` both allow (ADR-0040,
    /// decision 1), under the same rules as [`Grant::narrow`]. It is commutative,
    /// associative and idempotent, and narrowing a policy by one grant and then by the other
    /// gives the same policy as narrowing it by their meet. The delivery budget and the term are
    /// each the smaller of the two sides, `None` (no limit) being the identity.
    #[must_use]
    pub fn meet(&self, other: &Grant) -> Grant {
        let mut window_classes = BTreeMap::new();
        for application in self
            .window_classes
            .keys()
            .chain(other.window_classes.keys())
        {
            let met = meet_classes(
                self.window_classes.get(application).cloned(),
                other.window_classes.get(application).cloned(),
            );
            if let Some(classes) = met {
                window_classes.insert(application.clone(), classes);
            }
        }
        Grant {
            applications: self
                .applications
                .intersection(&other.applications)
                .cloned()
                .collect(),
            window_classes,
            window_pin: meet_pins(self.window_pin, other.window_pin),
            deliver_input: self.deliver_input && other.deliver_input,
            read_values: self.read_values && other.read_values,
            delivery_budget: meet_limits(self.delivery_budget, other.delivery_budget),
            term: meet_limits(self.term, other.term),
        }
    }
}

/// One connected client's account of its grant's delivery budget and term (ADR-0040, decisions
/// 2, 6 and 11; proposed): what it has spent, and the ceilings of both limits. Pure: the caller
/// passes the instant in, as to [`LeaseTable`](crate::LeaseTable).
///
/// - The term runs from `admitted_at`, the client's admission, whether or not a grant was read
///   then, and it has ended once the instant reaches `admitted_at + term`. A sum that cannot be
///   represented is an ended term (fail closed).
/// - Each ceiling is the meet of every value read for the client so far ([`GrantLedger::read`]),
///   `None` (no limit) being the identity: a later read can lower it, never raise it. Before the
///   first read there is no ceiling.
/// - The count grows by one for every result passed to [`GrantLedger::record`] that reported
///   input ([`ActionResult::reported_input`]), from admission and whether or not a ceiling exists
///   yet, and it never decreases. It saturates, and a saturated count is at least every budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantLedger {
    admitted_at: Duration,
    spent: u64,
    budget: Option<u32>,
    term: Option<Duration>,
}

impl GrantLedger {
    /// The ledger of a client admitted at `admitted_at`: nothing spent, no ceiling.
    pub fn new(admitted_at: Duration) -> Self {
        Self {
            admitted_at,
            spent: 0,
            budget: None,
            term: None,
        }
    }

    /// Reads the budget and term of the grant listed for the client now: each ceiling becomes
    /// the meet of the stored ceiling and the value read.
    pub fn read(&mut self, grant: &Grant) {
        self.budget = meet_limits(self.budget, grant.delivery_budget);
        self.term = meet_limits(self.term, grant.term);
    }

    /// Whether the term has ended at `now`: a term ceiling exists and `now` has reached the
    /// admission plus the term, or that sum cannot be represented.
    pub fn term_ended(&self, now: Duration) -> bool {
        self.term.is_some_and(|term| {
            self.admitted_at
                .checked_add(term)
                .is_none_or(|end| end <= now)
        })
    }

    /// Whether the budget is spent: a budget ceiling exists and the count has reached it.
    pub fn budget_spent(&self) -> bool {
        self.budget
            .is_some_and(|budget| self.spent >= u64::from(budget))
    }

    /// Counts `result` when it reported input, or may have.
    pub fn record(&mut self, result: &ActionResult) {
        if result.reported_input() {
            self.spent = self.spent.saturating_add(1);
        }
    }

    /// The grant the client is served under at `now`: `None`, which installs the default grant
    /// (ADR-0040, decision 7), once its term has ended; `grant` without live input once its
    /// budget is spent; `grant` unchanged otherwise.
    pub fn effective(&self, grant: Option<Grant>, now: Duration) -> Option<Grant> {
        if self.term_ended(now) {
            return None;
        }
        let mut grant = grant?;
        if self.budget_spent() {
            grant.deliver_input = false;
        }
        Some(grant)
    }

    /// How many results that reported input the client has had since its admission.
    pub fn spent(&self) -> u64 {
        self.spent
    }
}

/// A [`PolicyStore`] that applies the grant of the client being served to the operator's store
/// it wraps (ADR-0040, decision 6). Give the engine a clone of the `Arc` as its policy port and
/// keep one to install grants.
///
/// While a grant is installed, [`PolicyStore::current`] is [`Grant::narrow`] of the inner
/// store's policy, read afresh on every call: every read the engine makes (the gate's, the last
/// look's and the view's) is narrowed, and the operator's stop, read from the inner store each
/// time, holds under every grant. With no grant installed it is the inner policy unchanged.
/// The purge request is passed to the inner store untouched.
///
/// A client without a valid grant (none, an ended term, a withdrawn grant) is served under
/// [`Grant::default()`], which permits nothing: no live input and no values (ADR-0040,
/// decision 7). [`GrantedPolicy::install`] takes the client's grant as an `Option` for that
/// reason: `None` installs the default grant, never the inner policy.
pub struct GrantedPolicy {
    inner: Arc<dyn PolicyStore>,
    installed: Mutex<Option<Grant>>,
}

impl std::fmt::Debug for GrantedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrantedPolicy")
            .field("installed", &*self.slot())
            .finish_non_exhaustive()
    }
}

impl GrantedPolicy {
    /// Wraps the operator's store, with no grant installed.
    pub fn new(inner: Arc<dyn PolicyStore>) -> Self {
        Self {
            inner,
            installed: Mutex::new(None),
        }
    }

    /// Installs the grant of the client about to be served, for as long as the returned guard
    /// lives. Call it while holding the engine's lock, around exactly one call into the engine
    /// on behalf of that client. `None` (the client holds no valid grant) installs
    /// [`Grant::default()`], which permits nothing.
    ///
    /// Dropping the guard puts back what was installed before, so an early return or a panic in
    /// the call cannot leave one client's grant in force for the next caller.
    #[must_use = "the grant is removed when the guard is dropped"]
    pub fn install(&self, grant: Option<Grant>) -> InstalledGrant<'_> {
        let previous = self.slot().replace(grant.unwrap_or_default());
        InstalledGrant {
            store: self,
            previous,
        }
    }

    fn slot(&self) -> MutexGuard<'_, Option<Grant>> {
        // The slot is only replaced whole, so a panic elsewhere cannot leave it half written.
        self.installed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl PolicyStore for GrantedPolicy {
    fn current(&self) -> LocalPolicy {
        let inner = self.inner.current();
        match &*self.slot() {
            Some(grant) => grant.narrow(&inner),
            None => inner,
        }
    }

    fn purge_requested(&self) -> bool {
        self.inner.purge_requested()
    }

    fn purge_done(&self) {
        self.inner.purge_done();
    }
}

/// A grant installed in a [`GrantedPolicy`]; dropping it puts back what was installed before.
#[derive(Debug)]
pub struct InstalledGrant<'a> {
    store: &'a GrantedPolicy,
    previous: Option<Grant>,
}

impl Drop for InstalledGrant<'_> {
    fn drop(&mut self) {
        *self.store.slot() = self.previous.take();
    }
}

/// The class limit of two sides for one application: the classes both list, the list of the
/// side that restricts it, or `None` when neither does. Two lists with nothing in common
/// give `Some` of an empty set, which denies every window.
fn meet_classes(
    first: Option<BTreeSet<String>>,
    second: Option<BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.intersection(&second).cloned().collect()),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The smaller of two limits, `None` (no limit) being the identity.
fn meet_limits<T: Ord>(first: Option<T>, second: Option<T>) -> Option<T> {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.min(second)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The pin of two sides: one pin gives that pin, two equal pins give it, and two different
/// pins give `Unset`, so that no live input goes anywhere (ADR-0020, ADR-0040).
fn meet_pins(first: Option<WindowPin>, second: Option<WindowPin>) -> Option<WindowPin> {
    match (first, second) {
        (None, pin) | (pin, None) => pin,
        (Some(first), Some(second)) if first == second => Some(first),
        (Some(_), Some(_)) => Some(WindowPin::Unset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::TopLevelWindow;
    use crate::window_scope::window_scope_problem;
    use ommatix_domain::identity::ActionStatus;

    const APPLICATIONS: [&str; 4] = ["a.exe", "b.exe", "c.exe", "d.exe"];
    const CLASSES: [&str; 3] = ["K1", "K2", "K3"];
    const HANDLES: [u64; 3] = [1, 2, 3];
    const CASES: usize = 1_500;

    /// Fixed-seed generator (splitmix64): the same cases on every run, with no random crate.
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
        fn below(&mut self, n: usize) -> usize {
            let n = u64::try_from(n).unwrap();
            usize::try_from(self.next_u64() % n).unwrap()
        }

        /// True with probability `numerator / denominator`.
        fn chance(&mut self, numerator: usize, denominator: usize) -> bool {
            self.below(denominator) < numerator
        }

        /// Up to `max` picks with replacement, so a list may repeat and is in no order.
        fn picks(&mut self, items: &[&str], max: usize) -> Vec<String> {
            (0..self.below(max + 1))
                .map(|_| items[self.below(items.len())].to_owned())
                .collect()
        }

        fn pin(&mut self) -> Option<WindowPin> {
            match self.below(8) {
                0..=3 => None,
                4 => Some(WindowPin::Unset),
                _ => Some(WindowPin::Window(HANDLES[self.below(HANDLES.len())])),
            }
        }

        /// No limit, zero, or a small positive limit.
        fn limit(&mut self) -> Option<u32> {
            match self.below(5) {
                0 | 1 => None,
                2 => Some(0),
                _ => Some(u32::try_from(1 + self.below(3)).unwrap()),
            }
        }

        fn term(&mut self) -> Option<Duration> {
            self.limit()
                .map(|seconds| Duration::from_millis(u64::from(seconds) * 1_000))
        }
    }

    fn policy(rng: &mut Rng) -> LocalPolicy {
        let mut window_classes = BTreeMap::new();
        for application in APPLICATIONS {
            if rng.chance(1, 3) {
                window_classes.insert(application.to_owned(), rng.picks(&CLASSES, 3));
            }
        }
        LocalPolicy {
            actuation_enabled: rng.chance(2, 3),
            include_values: rng.chance(2, 3),
            allowed_applications: rng.picks(&APPLICATIONS, 4),
            window_classes,
            window_pin: rng.pin(),
            stopped: rng.chance(1, 4),
        }
    }

    fn grant(rng: &mut Rng) -> Grant {
        let mut window_classes = BTreeMap::new();
        for application in APPLICATIONS {
            if rng.chance(1, 3) {
                window_classes.insert(
                    application.to_owned(),
                    rng.picks(&CLASSES, 3).into_iter().collect(),
                );
            }
        }
        Grant {
            applications: rng.picks(&APPLICATIONS, 4).into_iter().collect(),
            window_classes,
            window_pin: rng.pin(),
            deliver_input: rng.chance(2, 3),
            read_values: rng.chance(2, 3),
            delivery_budget: rng.limit(),
            term: rng.term(),
        }
    }

    /// The grant read as a policy that is not stopped: what the grant alone permits, asked
    /// through the same predicates as the policy.
    fn as_policy(grant: &Grant) -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: grant.deliver_input,
            include_values: grant.read_values,
            allowed_applications: grant.applications.iter().cloned().collect(),
            window_classes: grant
                .window_classes
                .iter()
                .map(|(application, classes)| {
                    (application.clone(), classes.iter().cloned().collect())
                })
                .collect(),
            window_pin: grant.window_pin,
            stopped: false,
        }
    }

    fn window(class: &str, handle: u64) -> TopLevelWindow {
        TopLevelWindow {
            rectangle: None,
            z_order: 0,
            class: class.to_owned(),
            handle,
        }
    }

    /// Every window the actuator could attest, and none.
    fn windows() -> Vec<Option<TopLevelWindow>> {
        let mut all = vec![None];
        for class in CLASSES {
            for handle in HANDLES {
                all.push(Some(window(class, handle)));
            }
        }
        all
    }

    /// The action gate's question for live input: not stopped, actuation on, the foreground
    /// application on the allowlist (`contains`, as `execute_action` asks it) and the window
    /// scope satisfied by the foreground window and the window under the point.
    fn may_deliver(
        policy: &LocalPolicy,
        application: &str,
        foreground: Option<&TopLevelWindow>,
        at_point: Option<&TopLevelWindow>,
    ) -> bool {
        !policy.stopped
            && policy.actuation_enabled
            && policy
                .allowed_applications
                .contains(&application.to_owned())
            && window_scope_problem(policy, application, foreground, at_point).is_none()
    }

    /// Every delivery question the small alphabets can ask.
    fn deliveries(policy: &LocalPolicy) -> Vec<bool> {
        let all = windows();
        let mut answers = Vec::new();
        for application in APPLICATIONS {
            for foreground in &all {
                for at_point in &all {
                    answers.push(may_deliver(
                        policy,
                        application,
                        foreground.as_ref(),
                        at_point.as_ref(),
                    ));
                }
            }
        }
        answers
    }

    /// How often each outcome of a property was seen, so a property that holds only because
    /// one side never varies is caught.
    #[derive(Default)]
    struct Seen {
        both: usize,
        policy_only: usize,
        grant_only: usize,
        neither: usize,
    }

    impl Seen {
        fn record(&mut self, policy: bool, grant: bool) {
            match (policy, grant) {
                (true, true) => self.both += 1,
                (true, false) => self.policy_only += 1,
                (false, true) => self.grant_only += 1,
                (false, false) => self.neither += 1,
            }
        }

        fn assert_all_outcomes(&self, what: &str) {
            assert!(self.both > 0, "{what}: never both permit");
            assert!(
                self.policy_only > 0,
                "{what}: never only the policy permits"
            );
            assert!(self.grant_only > 0, "{what}: never only the grant permits");
            assert!(self.neither > 0, "{what}: never neither permits");
        }
    }

    #[test]
    fn a_narrowed_policy_permits_a_delivery_only_when_the_policy_and_the_grant_both_do() {
        let mut rng = Rng(0x0040_0001);
        let mut seen = Seen::default();
        for case in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            let narrowed = grant.narrow(&policy);
            let grant_policy = as_policy(&grant);
            let (mine, theirs, theirs_grant) = (
                deliveries(&narrowed),
                deliveries(&policy),
                deliveries(&grant_policy),
            );
            for ((narrow, policy_says), grant_says) in mine.iter().zip(&theirs).zip(&theirs_grant) {
                assert_eq!(
                    *narrow,
                    *policy_says && *grant_says,
                    "case {case}: {policy:?} narrowed by {grant:?}"
                );
                seen.record(*policy_says, *grant_says);
            }
        }
        seen.assert_all_outcomes("delivery");
    }

    #[test]
    fn a_narrowed_policy_returns_values_only_when_the_policy_and_the_grant_both_allow_it() {
        let mut rng = Rng(0x0040_0002);
        let mut seen = Seen::default();
        for _ in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            let narrowed = grant.narrow(&policy);
            assert_eq!(
                narrowed.include_values,
                policy.include_values && grant.read_values
            );
            seen.record(policy.include_values, grant.read_values);
        }
        seen.assert_all_outcomes("values");
    }

    #[test]
    fn a_narrowed_policy_lists_each_allowed_application_once_in_order() {
        let mut rng = Rng(0x0040_0003);
        for _ in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            let narrowed = grant.narrow(&policy);
            let mut sorted = narrowed.allowed_applications.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(narrowed.allowed_applications, sorted);
            for application in APPLICATIONS {
                let both = policy.allowed_applications.iter().any(|a| a == application)
                    && grant.applications.contains(application);
                assert_eq!(
                    narrowed
                        .allowed_applications
                        .iter()
                        .any(|a| a == application),
                    both,
                    "{application}"
                );
            }
        }
    }

    #[test]
    fn meet_allows_exactly_what_both_grants_allow() {
        let mut rng = Rng(0x0040_0004);
        let mut seen = Seen::default();
        for case in 0..CASES {
            let (first, second) = (grant(&mut rng), grant(&mut rng));
            let met = first.meet(&second);
            let (mine, theirs_first, theirs_second) = (
                deliveries(&as_policy(&met)),
                deliveries(&as_policy(&first)),
                deliveries(&as_policy(&second)),
            );
            for ((meet, first_says), second_says) in
                mine.iter().zip(&theirs_first).zip(&theirs_second)
            {
                assert_eq!(*meet, *first_says && *second_says, "case {case}");
                seen.record(*first_says, *second_says);
            }
            assert_eq!(met.read_values, first.read_values && second.read_values);
        }
        seen.assert_all_outcomes("meet");
    }

    #[test]
    fn meet_is_commutative_associative_and_idempotent() {
        let mut rng = Rng(0x0040_0005);
        for case in 0..CASES {
            let (a, b, c) = (grant(&mut rng), grant(&mut rng), grant(&mut rng));
            assert_eq!(a.meet(&b), b.meet(&a), "commutative, case {case}");
            assert_eq!(
                a.meet(&b).meet(&c),
                a.meet(&b.meet(&c)),
                "associative, case {case}"
            );
            assert_eq!(a.meet(&a), a, "idempotent, case {case}");
        }
    }

    #[test]
    fn narrowing_by_one_grant_and_then_the_other_equals_narrowing_by_their_meet() {
        let mut rng = Rng(0x0040_0006);
        let (mut changed, mut unchanged) = (0, 0);
        for case in 0..CASES {
            let policy = policy(&mut rng);
            let (a, b) = (grant(&mut rng), grant(&mut rng));
            let in_steps = b.narrow(&a.narrow(&policy));
            assert_eq!(in_steps, a.meet(&b).narrow(&policy), "case {case}");
            assert_eq!(in_steps, a.narrow(&b.narrow(&policy)), "order, case {case}");
            if in_steps == a.narrow(&policy) {
                unchanged += 1;
            } else {
                changed += 1;
            }
        }
        assert!(
            changed > 0 && unchanged > 0,
            "{changed} changed, {unchanged} not"
        );
    }

    #[test]
    fn a_stopped_policy_stays_stopped_under_every_grant_and_no_grant_sets_the_stop() {
        let mut rng = Rng(0x0040_0007);
        let (mut stopped, mut running) = (0, 0);
        for _ in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            let narrowed = grant.narrow(&policy);
            assert_eq!(narrowed.stopped, policy.stopped);
            if policy.stopped {
                stopped += 1;
                assert!(deliveries(&narrowed).iter().all(|permitted| !permitted));
            } else {
                running += 1;
            }
        }
        assert!(
            stopped > 0 && running > 0,
            "{stopped} stopped, {running} running"
        );
    }

    #[test]
    fn disjoint_class_lists_for_one_application_leave_no_window_of_it_allowed() {
        let policy = LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec!["a.exe".to_owned(), "b.exe".to_owned()],
            window_classes: [("a.exe".to_owned(), vec!["K1".to_owned()])].into(),
            ..LocalPolicy::default()
        };
        let grant = Grant {
            applications: ["a.exe", "b.exe"].map(str::to_owned).into(),
            window_classes: [("a.exe".to_owned(), ["K2".to_owned()].into())].into(),
            deliver_input: true,
            ..Grant::default()
        };
        let narrowed = grant.narrow(&policy);
        assert!(narrowed.allowed_applications.contains(&"a.exe".to_owned()));
        assert_eq!(narrowed.window_classes.get("a.exe"), Some(&Vec::new()));
        for foreground in windows() {
            for at_point in windows() {
                assert!(
                    !may_deliver(&narrowed, "a.exe", foreground.as_ref(), at_point.as_ref()),
                    "{foreground:?} under {at_point:?}"
                );
            }
        }
        // The other application is not restricted by class and still delivers.
        let any = window("K3", 9);
        assert!(may_deliver(&narrowed, "b.exe", Some(&any), Some(&any)));
    }

    #[test]
    fn one_side_that_restricts_an_application_by_class_restricts_it_in_the_result() {
        let policy = LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec!["a.exe".to_owned()],
            window_classes: [("a.exe".to_owned(), vec!["K2".to_owned(), "K1".to_owned()])].into(),
            ..LocalPolicy::default()
        };
        let grant = Grant {
            applications: ["a.exe".to_owned()].into(),
            deliver_input: true,
            ..Grant::default()
        };
        assert_eq!(
            grant.narrow(&policy).window_classes,
            [("a.exe".to_owned(), vec!["K1".to_owned(), "K2".to_owned()])].into()
        );
        let restricted = Grant {
            window_classes: [("a.exe".to_owned(), ["K3".to_owned()].into())].into(),
            ..grant
        };
        let open = LocalPolicy {
            window_classes: BTreeMap::new(),
            ..policy
        };
        assert_eq!(
            restricted.narrow(&open).window_classes,
            [("a.exe".to_owned(), vec!["K3".to_owned()])].into()
        );
        assert!(Grant::default().narrow(&open).window_classes.is_empty());
    }

    #[test]
    fn two_different_pins_or_an_unset_one_leave_no_live_input() {
        let pinned = |handle| Some(WindowPin::Window(handle));
        assert_eq!(meet_pins(None, None), None);
        assert_eq!(meet_pins(pinned(1), None), pinned(1));
        assert_eq!(meet_pins(None, pinned(1)), pinned(1));
        assert_eq!(meet_pins(pinned(1), pinned(1)), pinned(1));
        assert_eq!(meet_pins(pinned(1), pinned(2)), Some(WindowPin::Unset));
        assert_eq!(meet_pins(pinned(2), pinned(1)), Some(WindowPin::Unset));
        for other in [None, pinned(1), Some(WindowPin::Unset)] {
            assert_eq!(
                meet_pins(Some(WindowPin::Unset), other),
                Some(WindowPin::Unset)
            );
            assert_eq!(
                meet_pins(other, Some(WindowPin::Unset)),
                Some(WindowPin::Unset)
            );
        }

        let policy = LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec!["a.exe".to_owned()],
            window_pin: pinned(1),
            ..LocalPolicy::default()
        };
        let grant = Grant {
            applications: ["a.exe".to_owned()].into(),
            window_pin: pinned(2),
            deliver_input: true,
            ..Grant::default()
        };
        let narrowed = grant.narrow(&policy);
        assert_eq!(narrowed.window_pin, Some(WindowPin::Unset));
        for handle in HANDLES {
            let target = window("K1", handle);
            assert!(!may_deliver(
                &narrowed,
                "a.exe",
                Some(&target),
                Some(&target)
            ));
        }
    }

    #[test]
    fn the_default_grant_permits_nothing_as_the_default_policy_does() {
        let nothing = |policy: &LocalPolicy| {
            !policy.actuation_enabled
                && !policy.include_values
                && policy.allowed_applications.is_empty()
                && deliveries(policy).iter().all(|permitted| !permitted)
        };
        assert!(nothing(&LocalPolicy::default()));
        assert!(nothing(&as_policy(&Grant::default())));
        let mut rng = Rng(0x0040_0008);
        for _ in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            assert!(nothing(&Grant::default().narrow(&policy)), "{policy:?}");
            assert!(
                nothing(&as_policy(&Grant::default().meet(&grant))),
                "{grant:?}"
            );
        }
    }

    // ---- UT-81: the meet of a delivery budget and a term ----

    /// A limit read as a number, `None` (no limit) above every bound.
    fn at_most<T: Ord + Copy>(limit: Option<T>, other: Option<T>) -> bool {
        match (limit, other) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(limit), Some(other)) => limit <= other,
        }
    }

    #[test]
    fn ut_81_the_meet_of_budgets_and_terms_is_the_smaller_side_with_no_limit_as_identity() {
        let mut rng = Rng(0x0040_0081);
        let (mut budgets, mut terms) = (Seen::default(), Seen::default());
        for case in 0..CASES {
            let (a, b) = (grant(&mut rng), grant(&mut rng));
            let met = a.meet(&b);
            for side in [&a, &b] {
                assert!(
                    at_most(met.delivery_budget, side.delivery_budget),
                    "budget above a side, case {case}: {a:?} {b:?}"
                );
                assert!(
                    at_most(met.term, side.term),
                    "term above a side, case {case}"
                );
            }
            let expected_budget = match (a.delivery_budget, b.delivery_budget) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, None) => x,
                (None, y) => y,
            };
            assert_eq!(met.delivery_budget, expected_budget, "case {case}");
            let expected_term = match (a.term, b.term) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, None) => x,
                (None, y) => y,
            };
            assert_eq!(met.term, expected_term, "case {case}");
            budgets.record(a.delivery_budget.is_some(), b.delivery_budget.is_some());
            terms.record(a.term.is_some(), b.term.is_some());
            // The default grant has no limit: its meet keeps the other side's budget and term.
            let with_default = Grant::default().meet(&a);
            assert_eq!(with_default.delivery_budget, a.delivery_budget);
            assert_eq!(with_default.term, a.term);
        }
        budgets.assert_all_outcomes("budget limits");
        terms.assert_all_outcomes("term limits");
    }

    #[test]
    fn ut_81_the_generator_reaches_no_limit_zero_and_a_positive_limit_for_both_fields() {
        let mut rng = Rng(0x0040_0082);
        let mut budgets = BTreeSet::new();
        let mut terms = BTreeSet::new();
        for _ in 0..CASES {
            let g = grant(&mut rng);
            budgets.insert(g.delivery_budget.map(|b| b.min(1)));
            terms.insert(g.term.map(|t| t.min(Duration::from_millis(1))));
        }
        assert_eq!(budgets, BTreeSet::from([None, Some(0), Some(1)]));
        assert_eq!(
            terms,
            BTreeSet::from([None, Some(Duration::ZERO), Some(Duration::from_millis(1))])
        );
    }

    #[test]
    fn ut_81_two_grants_that_differ_only_in_budget_and_term_narrow_every_policy_alike() {
        let mut rng = Rng(0x0040_0083);
        let mut differed = 0;
        for case in 0..CASES {
            let policy = policy(&mut rng);
            let first = grant(&mut rng);
            let second = Grant {
                delivery_budget: rng.limit(),
                term: rng.term(),
                ..first.clone()
            };
            differed += usize::from(first != second);
            assert_eq!(first.narrow(&policy), second.narrow(&policy), "case {case}");
        }
        assert!(differed > CASES / 2, "{differed} pairs differed");
    }

    // ---- UT-93: one client's ledger of its budget and term ----

    fn result(status: ActionStatus, delivered: bool) -> ActionResult {
        ActionResult {
            request_id: "r".to_owned(),
            status,
            target_id: "t".to_owned(),
            snapshot_id: "s".to_owned(),
            error: None,
            observed_effect: None,
            delivered,
        }
    }

    fn limits(budget: Option<u32>, term_ms: Option<u64>) -> Grant {
        Grant {
            delivery_budget: budget,
            term: term_ms.map(Duration::from_millis),
            deliver_input: true,
            read_values: true,
            ..Grant::default()
        }
    }

    #[test]
    fn ut_93_reported_input_is_delivered_or_indeterminate_and_nothing_else() {
        use ActionStatus::{EffectObserved, Indeterminate, InputDelivered, Rejected, Simulated};
        assert!(result(InputDelivered, true).reported_input());
        assert!(result(EffectObserved, true).reported_input());
        assert!(result(Indeterminate, true).reported_input());
        assert!(result(Indeterminate, false).reported_input());
        assert!(!result(Simulated, false).reported_input());
        assert!(!result(Rejected, false).reported_input());
        // The ledger counts by that predicate alone.
        let mut ledger = GrantLedger::new(Duration::ZERO);
        for (status, delivered) in [
            (InputDelivered, true),
            (EffectObserved, true),
            (Indeterminate, true),
            (Indeterminate, false),
            (Simulated, false),
            (Rejected, false),
        ] {
            ledger.record(&result(status, delivered));
        }
        assert_eq!(ledger.spent(), 4);
    }

    #[test]
    fn ut_93_a_ceiling_is_the_meet_of_every_value_read_and_a_later_read_never_raises_it() {
        let mut ledger = GrantLedger::new(Duration::ZERO);
        // Before any read there is no ceiling, whatever was spent.
        for _ in 0..3 {
            ledger.record(&result(ActionStatus::InputDelivered, true));
        }
        assert!(!ledger.budget_spent());
        assert!(!ledger.term_ended(Duration::MAX));
        // No limit read: still no ceiling.
        ledger.read(&limits(None, None));
        assert!(!ledger.budget_spent());
        // 10 → 5 → 8 keeps 5; the three already spent count.
        ledger.read(&limits(Some(10), Some(10_000)));
        ledger.read(&limits(Some(5), Some(5_000)));
        ledger.read(&limits(Some(8), Some(8_000)));
        ledger.read(&limits(None, None));
        assert!(!ledger.budget_spent());
        ledger.record(&result(ActionStatus::InputDelivered, true));
        assert!(!ledger.budget_spent(), "4 of 5");
        ledger.record(&result(ActionStatus::Indeterminate, false));
        assert!(ledger.budget_spent(), "5 of 5");
        assert!(!ledger.term_ended(Duration::from_millis(4_999)));
        assert!(ledger.term_ended(Duration::from_millis(5_000)));
        // A lowering takes effect at once, counting what was spent.
        let mut lowered = GrantLedger::new(Duration::ZERO);
        lowered.read(&limits(Some(4), None));
        lowered.record(&result(ActionStatus::InputDelivered, true));
        lowered.record(&result(ActionStatus::InputDelivered, true));
        assert!(!lowered.budget_spent());
        lowered.read(&limits(Some(2), None));
        assert!(lowered.budget_spent());
        lowered.read(&limits(Some(100), None));
        assert!(lowered.budget_spent(), "a raise is ignored");
    }

    #[test]
    fn ut_93_the_term_runs_from_admission_ends_inclusively_and_an_overflow_is_an_ended_term() {
        let admitted = Duration::from_secs(7);
        let mut ledger = GrantLedger::new(admitted);
        ledger.read(&limits(None, Some(2_000)));
        let end = admitted + Duration::from_secs(2);
        assert!(!ledger.term_ended(end.checked_sub(Duration::from_millis(1)).unwrap()));
        assert!(ledger.term_ended(end));
        assert!(ledger.term_ended(end + Duration::from_secs(1)));
        // A zero term ended at admission.
        let mut zero = GrantLedger::new(admitted);
        zero.read(&limits(None, Some(0)));
        assert!(zero.term_ended(admitted));
        // A term that cannot be added to the admission: ended, at every instant.
        let mut overflow = GrantLedger::new(Duration::MAX);
        overflow.read(&limits(None, Some(1)));
        assert!(overflow.term_ended(Duration::ZERO));
        assert!(overflow.term_ended(Duration::MAX));
        // No term ceiling: never ended, even at the largest instant.
        let mut open = GrantLedger::new(Duration::MAX);
        open.read(&limits(Some(3), None));
        assert!(!open.term_ended(Duration::MAX));
    }

    #[test]
    fn ut_93_a_saturated_count_reads_as_spent_for_every_budget() {
        let mut ledger = GrantLedger {
            admitted_at: Duration::ZERO,
            spent: u64::MAX - 1,
            budget: None,
            term: None,
        };
        ledger.record(&result(ActionStatus::InputDelivered, true));
        ledger.record(&result(ActionStatus::InputDelivered, true));
        assert_eq!(ledger.spent(), u64::MAX);
        ledger.read(&limits(Some(u32::MAX), None));
        assert!(ledger.budget_spent());
    }

    #[test]
    fn ut_93_the_effective_grant_is_none_after_the_term_and_observe_only_after_the_budget() {
        let grant = limits(Some(1), Some(1_000));
        let mut ledger = GrantLedger::new(Duration::ZERO);
        ledger.read(&grant);
        let now = Duration::from_millis(500);
        assert_eq!(
            ledger.effective(Some(grant.clone()), now),
            Some(grant.clone())
        );
        assert_eq!(ledger.effective(None, now), None, "no grant stays none");
        ledger.record(&result(ActionStatus::EffectObserved, true));
        let spent = ledger.effective(Some(grant.clone()), now).unwrap();
        assert!(!spent.deliver_input, "a spent budget delivers nothing");
        assert_eq!(
            Grant {
                deliver_input: true,
                ..spent
            },
            grant,
            "everything else is kept, values included"
        );
        assert_eq!(
            ledger.effective(Some(grant.clone()), Duration::from_millis(1_000)),
            None,
            "an ended term is the default grant"
        );
    }

    /// The operator's store as the wrapper sees it: a policy the operator may change between
    /// reads, the stop and a count of purge requests and of purges done.
    #[derive(Debug, Default)]
    struct Operator {
        policy: Mutex<LocalPolicy>,
        stop: std::sync::atomic::AtomicBool,
        purge_pending: std::sync::atomic::AtomicBool,
        purges_done: std::sync::atomic::AtomicUsize,
    }

    impl Operator {
        fn set(&self, policy: LocalPolicy) {
            *self.policy.lock().unwrap() = policy;
        }
    }

    impl PolicyStore for Operator {
        fn current(&self) -> LocalPolicy {
            let mut policy = self.policy.lock().unwrap().clone();
            policy.stopped |= self.stop.load(std::sync::atomic::Ordering::SeqCst);
            policy
        }

        fn purge_requested(&self) -> bool {
            self.purge_pending.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn purge_done(&self) {
            self.purge_pending
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.purges_done
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn wrapped() -> (Arc<Operator>, GrantedPolicy) {
        let operator = Arc::new(Operator::default());
        let store = GrantedPolicy::new(operator.clone());
        (operator, store)
    }

    #[test]
    fn an_installed_grant_narrows_every_read_and_dropping_it_restores_the_inner_policy() {
        let (operator, store) = wrapped();
        let mut rng = Rng(0x0040_0009);
        let (mut narrowed, mut unchanged) = (0, 0);
        for case in 0..CASES {
            let (first, second, grant) = (policy(&mut rng), policy(&mut rng), grant(&mut rng));
            operator.set(first.clone());
            assert_eq!(store.current(), first, "case {case}: nothing installed");
            {
                let _installed = store.install(Some(grant.clone()));
                assert_eq!(store.current(), grant.narrow(&first), "case {case}");
                // The operator changes the policy during the call: the next read sees it.
                operator.set(second.clone());
                assert_eq!(store.current(), grant.narrow(&second), "case {case}");
                if grant.narrow(&second) == second {
                    unchanged += 1;
                } else {
                    narrowed += 1;
                }
            }
            assert_eq!(store.current(), second, "case {case}: removed");
        }
        assert!(
            narrowed > 0 && unchanged > 0,
            "{narrowed} narrowed, {unchanged} not"
        );
    }

    #[test]
    fn a_client_without_a_grant_is_served_under_the_default_grant_which_permits_nothing() {
        let (operator, store) = wrapped();
        let mut rng = Rng(0x0040_000A);
        for case in 0..CASES {
            let policy = policy(&mut rng);
            operator.set(policy.clone());
            let _installed = store.install(None);
            let served = store.current();
            assert_eq!(served, Grant::default().narrow(&policy), "case {case}");
            assert!(
                !served.actuation_enabled && !served.include_values,
                "case {case}"
            );
            assert!(deliveries(&served).iter().all(|permitted| !permitted));
        }
    }

    #[test]
    fn the_wrapper_shows_values_only_when_the_policy_and_the_grant_both_allow_them() {
        let (operator, store) = wrapped();
        for (policy_values, grant_values) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            operator.set(LocalPolicy {
                include_values: policy_values,
                ..LocalPolicy::default()
            });
            let _installed = store.install(Some(Grant {
                read_values: grant_values,
                ..Grant::default()
            }));
            assert_eq!(
                store.current().include_values,
                policy_values && grant_values,
                "policy {policy_values}, grant {grant_values}"
            );
        }
    }

    /// One call on behalf of a client, which returns early when the engine refuses.
    fn call_that_returns_early(
        store: &GrantedPolicy,
        grant: &Grant,
        refused: bool,
    ) -> Result<(), ()> {
        let _installed = store.install(Some(grant.clone()));
        if refused {
            return Err(());
        }
        Ok(())
    }

    #[test]
    fn neither_an_early_return_nor_a_panic_leaves_a_grant_installed_and_guards_nest() {
        let (operator, store) = wrapped();
        let open = LocalPolicy {
            actuation_enabled: true,
            include_values: true,
            allowed_applications: APPLICATIONS.map(str::to_owned).to_vec(),
            ..LocalPolicy::default()
        };
        operator.set(open.clone());
        let only_a = Grant {
            applications: ["a.exe".to_owned()].into(),
            deliver_input: true,
            ..Grant::default()
        };
        assert!(call_that_returns_early(&store, &only_a, true).is_err());
        assert_eq!(store.current(), open, "after an early return");

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _installed = store.install(Some(only_a.clone()));
            panic!("the call panicked");
        }));
        assert!(panicked.is_err());
        assert_eq!(store.current(), open, "after a panic");

        let outer = store.install(Some(only_a.clone()));
        {
            let _inner = store.install(None);
            assert_eq!(store.current(), Grant::default().narrow(&open));
        }
        assert_eq!(
            store.current(),
            only_a.narrow(&open),
            "the outer grant is back"
        );
        drop(outer);
        assert_eq!(store.current(), open);
    }

    #[test]
    fn the_operator_stop_and_the_purge_request_reach_the_inner_store_under_every_grant() {
        let (operator, store) = wrapped();
        let mut rng = Rng(0x0040_000B);
        for case in 0..CASES {
            let (policy, grant) = (policy(&mut rng), grant(&mut rng));
            operator.set(LocalPolicy {
                stopped: false,
                ..policy
            });
            let stop = rng.chance(1, 2);
            operator
                .stop
                .store(stop, std::sync::atomic::Ordering::SeqCst);
            let installed = rng.chance(2, 3).then(|| store.install(Some(grant)));
            assert_eq!(store.current().stopped, stop, "case {case}");
            if stop {
                assert!(
                    deliveries(&store.current())
                        .iter()
                        .all(|permitted| !permitted)
                );
            }
            drop(installed);
        }

        let _installed = store.install(None);
        assert!(!store.purge_requested());
        operator
            .purge_pending
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(store.purge_requested(), "the request is the operator's");
        store.purge_done();
        assert!(!operator.purge_requested());
        assert_eq!(
            operator
                .purges_done
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
}
