//! A grant: limits a client holds over the policy-shaped part of the local policy, and the
//! meet that applies them (ADR-0040, decisions 1 and 2).
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
//! No clock, no I/O, no serialisation. The broker of the MCP adapter (ADR-0040, proposed)
//! installs each client's grant through [`GrantedPolicy`] for the length of its call; nothing in
//! the executable constructs a broker yet.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::ports::{LocalPolicy, PolicyStore, WindowPin};

/// What a client may do within the local policy (ADR-0040, decision 2), limited to the
/// policy-shaped limits: applications, window classes, a window pin, live input and the
/// reading of values.
///
/// The default grant allows nothing: no application, no live input, no values. Collections
/// are ordered, so two grants that allow the same things compare equal whatever the order
/// they were built in.
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
    /// gives the same policy as narrowing it by their meet.
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
        }
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
