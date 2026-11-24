//! Several clients on one engine (ADR-0039, ADR-0040), as the broker will drive it: the snapshot
//! spent from outside, the journal per client, the published page served again and the policy
//! store that applies the grant of the client being served. Nothing outside these tests calls
//! them yet.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroUsize;

use super::*;
use crate::grant::{Grant, GrantedPolicy};
use crate::lease::{Acquired, ClientId, LeaseChange, LeaseTable};
use crate::ports::PolicyStore;

/// Fixed-seed generator (splitmix64): the same sequence on every run, with no random crate.
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
        usize::try_from(self.next_u64() % u64::try_from(n).unwrap()).unwrap()
    }
}

/// `n` client identifiers, as the lease table gives them.
fn clients(n: usize) -> Vec<ClientId> {
    let mut table = LeaseTable::new(
        NonZeroUsize::new(JOURNAL_CLIENTS).unwrap(),
        Duration::from_secs(1),
    )
    .unwrap();
    (0..n)
        .map(|_| table.connect(Duration::ZERO).result.unwrap())
        .collect()
}

/// A rig whose engine reads the operator's policy through a [`GrantedPolicy`], and that store.
fn granted_rig(world: World) -> (Rig, Arc<GrantedPolicy>) {
    let mut r = rig(world);
    let granted = Arc::new(GrantedPolicy::new(r.policy.clone()));
    r.engine = Engine::new(
        EngineConfig::new(SESSION),
        Ports {
            capturer: Box::new(Capturer(r.world.clone(), r.clock.clone())),
            detector: Box::new(Detector(r.world.clone())),
            recognizer: Box::new(Recognizer(r.world.clone())),
            accessibility: Box::new(Access(r.world.clone(), r.clock.clone())),
            actuator: Box::new(Actuator(r.world.clone())),
            policy: granted.clone(),
            clock: r.clock.clone(),
        },
    );
    (r, granted)
}

/// A dry-run click on the Save button of `page`.
fn dry_click(page: &ObservePage, request_id: &str) -> ActionRequest {
    Rig::action(page, "Save", request_id, ActionKind::Click)
}

/// The value the page shows for the input field, if any.
fn input_value(page: &ObservePage) -> Option<(String, TextKind)> {
    page.elements
        .iter()
        .find(|e| e.role == Role::Input)
        .and_then(|e| e.text.clone())
}

fn stale(result: &ActionResult) -> bool {
    (result.status, error_code(result)) == (ActionStatus::Rejected, Some(ErrorCode::StaleSnapshot))
}

/// The request IDs the journal of `client` remembers (`None`: the journal of no client).
fn journaled(r: &Rig, client: Option<ClientId>) -> Vec<String> {
    match client {
        None => r.engine.journal.request_ids(),
        Some(client) => r
            .engine
            .client_journals
            .get(&client)
            .map(super::super::journal::Journal::request_ids)
            .unwrap_or_default(),
    }
}

// ---- the snapshot spent from outside ----

#[test]
fn a_snapshot_spent_from_outside_refuses_live_and_dry_actions_until_the_next_observation() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());

    // Nothing published: nothing happens.
    r.engine.spend_snapshot();
    assert!(!r.engine.snapshot_spent);

    let page = r.observe();
    r.engine.spend_snapshot();
    let dry = r.engine.execute_action(&dry_click(&page, "dry"));
    let live = r
        .engine
        .execute_action(&live_click(&page, "live", ExpectedEffect::None));
    assert!(stale(&dry), "{dry:?}");
    assert!(stale(&live), "{live:?}");
    assert!(r.world().delivered.is_empty(), "nothing is delivered");

    let page = r.observe();
    let dry = r.engine.execute_action(&dry_click(&page, "dry-2"));
    assert_eq!(dry.status, ActionStatus::Simulated);
    let live = r
        .engine
        .execute_action(&live_click(&page, "live-2", ExpectedEffect::None));
    assert!(live.input_delivered(), "{live:?}");
    assert_eq!(r.world().delivered.len(), 1);
}

// ---- the journal per client ----

#[test]
fn two_clients_with_one_request_id_get_their_own_results_and_each_retry_replays_its_own() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let ids = clients(2);
    let (a, b) = (ids[0], ids[1]);

    let page = r.observe();
    let dry = dry_click(&page, "r1");
    let live = live_click(&page, "r1", ExpectedEffect::None);
    let a_result = r.engine.execute_action_for(a, &dry);
    assert_eq!(a_result.status, ActionStatus::Simulated);
    // The same request identifier with other arguments: b's own request, not a conflict.
    let b_result = r.engine.execute_action_for(b, &live);
    assert!(b_result.input_delivered(), "{b_result:?}");
    assert_eq!(r.world().delivered.len(), 1);

    // The delivery spent the snapshot, so anything that reaches the gate now is refused: a
    // replay is told apart from a new execution.
    assert_eq!(r.engine.execute_action_for(a, &dry), a_result, "a's retry");
    assert_eq!(r.engine.execute_action_for(b, &live), b_result, "b's retry");
    assert_eq!(r.world().delivered.len(), 1, "a retry delivers nothing new");
    let conflict = r.engine.execute_action_for(a, &live);
    assert_eq!(error_code(&conflict), Some(ErrorCode::RequestConflict));

    // The path of no client never sees a client's entry...
    let none = r.engine.execute_action(&dry);
    assert!(stale(&none), "new on the path of no client: {none:?}");
    // ... and a client never sees an entry of the path of no client.
    let page = r.observe();
    let other = dry_click(&page, "r2");
    let first = r.engine.execute_action(&other);
    assert_eq!(first.status, ActionStatus::Simulated);
    r.engine.spend_snapshot();
    assert!(stale(&r.engine.execute_action_for(a, &other)), "new for a");
    assert_eq!(
        r.engine.execute_action(&other),
        first,
        "replayed for no client"
    );
    assert_eq!(r.world().delivered.len(), 1);
}

#[test]
fn a_client_that_fills_its_share_forgets_only_its_own_results_that_reported_no_input() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let ids = clients(3);
    // The first client is the busy one; the others act less, and the path of no client too.
    let actors = [Some(ids[0]), Some(ids[1]), Some(ids[2]), None];
    let capacity = |actor: Option<ClientId>| {
        if actor.is_some() {
            JOURNAL_CAPACITY_PER_CLIENT
        } else {
            JOURNAL_CAPACITY
        }
    };
    let mut evictable: BTreeMap<Option<ClientId>, VecDeque<String>> = BTreeMap::new();
    let mut kept: BTreeMap<Option<ClientId>, BTreeSet<String>> = BTreeMap::new();
    let mut sent: BTreeMap<Option<ClientId>, usize> = BTreeMap::new();
    let mut evicted: BTreeMap<Option<ClientId>, usize> = BTreeMap::new();
    let mut rng = Rng(0x0039_0011);
    let mut page = r.observe();
    for step in 0..2_000 {
        let actor = actors[match rng.below(8) {
            0..=3 => 0,
            4 => 1,
            5 => 2,
            _ => 3,
        }];
        // Each actor numbers its own requests, so the actors reuse each other's identifiers.
        let count = sent.entry(actor).or_default();
        let id = format!("r{count}");
        *count += 1;
        let request = match rng.below(10) {
            0 => live_click(&page, &id, ExpectedEffect::None),
            1 => live_click(&page, &id, ExpectedEffect::ScrollMoved),
            _ => dry_click(&page, &id),
        };
        let result = match actor {
            Some(client) => r.engine.execute_action_for(client, &request),
            None => r.engine.execute_action(&request),
        };
        if result.delivered || result.status == ActionStatus::Indeterminate {
            kept.entry(actor).or_default().insert(id);
            if rng.below(2) == 0 {
                page = r.observe();
            }
        } else {
            let queue = evictable.entry(actor).or_default();
            queue.push_back(id);
            if queue.len() > capacity(actor) {
                queue.pop_front();
                *evicted.entry(actor).or_default() += 1;
            }
        }
        if step % 100 == 99 {
            page = r.observe();
        }
    }
    for actor in actors {
        let mut expected: Vec<String> = evictable
            .get(&actor)
            .into_iter()
            .flatten()
            .chain(kept.get(&actor).into_iter().flatten())
            .cloned()
            .collect();
        expected.sort();
        assert_eq!(journaled(&r, actor), expected, "{actor:?}");
    }
    for client in &ids {
        let actor = Some(*client);
        assert!(
            evicted.get(&actor).copied().unwrap_or(0) > 0,
            "{actor:?} filled its share"
        );
        assert!(
            !kept.get(&actor).is_none_or(BTreeSet::is_empty),
            "{actor:?} kept input"
        );
    }
    assert_eq!(
        evicted.get(&None),
        None,
        "the path of no client stays within its capacity"
    );
    assert!(!kept.get(&None).is_none_or(BTreeSet::is_empty));
}

#[test]
fn forgetting_a_client_removes_exactly_its_entries() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let ids = clients(2);
    let (a, b) = (ids[0], ids[1]);
    let first = r.observe();
    for i in 0..5 {
        let request = dry_click(&first, &format!("r{i}"));
        r.engine.execute_action_for(a, &request);
        r.engine.execute_action_for(b, &request);
        r.engine.execute_action(&request);
    }
    let delivered = r
        .engine
        .execute_action_for(a, &live_click(&first, "live", ExpectedEffect::None));
    assert!(delivered.input_delivered());
    let page = r.observe();
    let (theirs, nobodys) = (journaled(&r, Some(b)), journaled(&r, None));
    assert_eq!(journaled(&r, Some(a)).len(), 6);

    r.engine.forget_client(a);
    assert_eq!(journaled(&r, Some(a)), Vec::<String>::new());
    assert_eq!(journaled(&r, Some(b)), theirs);
    assert_eq!(journaled(&r, None), nobodys);
    // The snapshot and the registry are untouched.
    assert_eq!(
        r.engine.serve_published(&ObserveRequest::default()),
        Ok(Some(page.clone()))
    );
    let dry = r.engine.execute_action_for(a, &dry_click(&page, "after"));
    assert_eq!(dry.status, ActionStatus::Simulated);
    // What a forgot is new again: refused on the old snapshot, and nothing is sent twice.
    assert!(stale(
        &r.engine.execute_action_for(a, &dry_click(&first, "r0"))
    ));
    let again = r
        .engine
        .execute_action_for(a, &live_click(&first, "live", ExpectedEffect::None));
    assert!(stale(&again), "{again:?}");
    assert_eq!(r.world().delivered.len(), 1);
    // b still replays its own.
    let replay = r.engine.execute_action_for(b, &dry_click(&first, "r0"));
    assert_eq!(replay.status, ActionStatus::Simulated);
}

// ---- the published page served again ----

/// The form and four words in a row: seven elements, so a small limit pages.
fn paged_form() -> World {
    let mut world = form();
    for i in 0..4u8 {
        let x = 40.0 + f64::from(i) * 60.0;
        world
            .words
            .push(Word::new(format!("w{i}"), bx(x, 150.0, x + 40.0, 165.0)));
    }
    world
}

/// Every page of the published view for `request`, served again, compared with what
/// `observe_screen` gives for the same cursor; returns how many pages there were.
fn assert_served_pages_equal_observed(
    r: &mut Rig,
    request: &ObserveRequest,
    first: &ObservePage,
) -> usize {
    assert_eq!(
        r.engine.serve_published(request),
        Ok(Some(first.clone())),
        "first page"
    );
    let mut served = 1;
    let mut cursor = first.next_cursor.clone();
    while let Some(next) = cursor {
        let paged = ObserveRequest {
            cursor: Some(next),
            ..request.clone()
        };
        let observed = r.observe_with(&paged).unwrap();
        assert_eq!(r.engine.serve_published(&paged), Ok(Some(observed.clone())));
        cursor = observed.next_cursor;
        served += 1;
    }
    served
}

#[test]
fn the_published_page_served_again_equals_the_observed_one_and_changes_nothing() {
    let mut r = rig(paged_form());
    r.world().live = true;
    r.set_policy(live_policy());

    let small = ObserveRequest {
        limit: 3,
        ..ObserveRequest::default()
    };
    let first = r.observe_with(&small).unwrap();
    assert_eq!(
        assert_served_pages_equal_observed(&mut r, &small, &first),
        3
    );
    let query = ObserveRequest {
        query: Some("w".into()),
        limit: 2,
        ..ObserveRequest::default()
    };
    let queried = r.observe_with(&query).unwrap();
    assert_eq!(queried.total_elements, 4);
    assert_eq!(
        assert_served_pages_equal_observed(&mut r, &query, &queried),
        2
    );

    let page = r.observe();
    let captures = r.world().seq;
    let metrics = r.engine.engine_metrics();
    for _ in 0..3 {
        assert_eq!(
            r.engine.serve_published(&ObserveRequest::default()),
            Ok(Some(page.clone()))
        );
    }
    assert_eq!(r.world().seq, captures, "serving captures nothing");
    assert_eq!(r.engine.engine_metrics(), metrics, "no counter moves");
    // The holder's next live action is still valid.
    let live = r
        .engine
        .execute_action(&live_click(&page, "live", ExpectedEffect::None));
    assert!(live.input_delivered(), "{live:?}");
}

#[test]
fn each_caller_gets_its_own_values_from_the_same_published_snapshot() {
    let mut r = rig(form());
    let asked = ObserveRequest {
        include_values: true,
        ..ObserveRequest::default()
    };
    r.set_policy(LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    });
    let observed = r.observe_with(&asked).unwrap();
    let hello = Some(("hello".to_owned(), TextKind::Unknown));
    assert_eq!(input_value(&observed), hello);

    // The next caller is not allowed values: the same snapshot, without them.
    r.set_policy(LocalPolicy::default());
    let hidden = r.engine.serve_published(&asked).unwrap().unwrap();
    assert_eq!(hidden.snapshot_id, observed.snapshot_id);
    assert_eq!(input_value(&hidden), None);
    // And the one after is allowed them again.
    r.set_policy(LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    });
    assert_eq!(r.engine.serve_published(&asked), Ok(Some(observed)));
}

#[test]
fn the_age_of_the_published_snapshot_is_read_on_the_injected_clock() {
    let mut r = rig(form());
    assert_eq!(r.engine.published_age(), None);
    let observed = r.observe();
    assert_eq!(r.engine.published_age(), Some(Duration::ZERO));
    r.clock.advance(Duration::from_millis(1_234));
    assert_eq!(r.engine.published_age(), Some(Duration::from_millis(1_234)));
    let served = r
        .engine
        .serve_published(&ObserveRequest::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        served,
        ObservePage {
            age_ms: 1_234,
            ..observed
        }
    );
}

#[test]
fn serving_with_nothing_published_says_so() {
    let mut r = rig(form());
    let any = ObserveRequest::default();
    assert_eq!(r.engine.serve_published(&any), Ok(None));
    let with_cursor = ObserveRequest {
        cursor: Some("1.3.0.0000000000000000".into()),
        ..ObserveRequest::default()
    };
    assert_eq!(r.engine.serve_published(&with_cursor), Ok(None));
    let out_of_bounds = ObserveRequest {
        limit: 0,
        ..ObserveRequest::default()
    };
    assert_eq!(
        r.engine.serve_published(&out_of_bounds).map_err(|e| e.code),
        Err(ErrorCode::LimitExceeded)
    );

    // A failed first observation publishes nothing.
    r.world().fail_capture = true;
    assert!(r.observe_with(&any).is_err());
    assert_eq!(r.engine.serve_published(&any), Ok(None));

    // Nor is anything published after a purge.
    r.world().fail_capture = false;
    r.observe();
    r.engine.purge();
    assert_eq!(r.engine.serve_published(&any), Ok(None));
    assert_eq!(r.engine.published_age(), None);
}

// ---- the policy store that applies a grant ----

fn full_grant() -> Grant {
    Grant {
        applications: [APP.to_owned()].into(),
        deliver_input: true,
        read_values: true,
        ..Grant::default()
    }
}

/// One call on behalf of a client without a grant, which returns early on a refusal.
fn call_refused_early(
    granted: &GrantedPolicy,
    engine: &mut Engine,
    request: &ActionRequest,
) -> Option<ActionResult> {
    let _installed = granted.install(None);
    let result = engine.execute_action(request);
    if result.status == ActionStatus::Rejected {
        return None;
    }
    Some(result)
}

#[test]
fn the_gate_and_the_view_decide_under_the_installed_grant() {
    let (mut r, granted) = granted_rig(form());
    r.world().live = true;
    r.set_policy(LocalPolicy {
        include_values: true,
        allowed_applications: vec![APP.into(), "other.exe".into()],
        ..live_policy()
    });
    let elsewhere = Grant {
        applications: ["other.exe".to_owned()].into(),
        ..full_grant()
    };
    let blind = Grant {
        read_values: false,
        ..full_grant()
    };
    let asked = ObserveRequest {
        include_values: true,
        ..ObserveRequest::default()
    };
    let hello = Some(("hello".to_owned(), TextKind::Unknown));

    // An application the grant leaves out is refused, and nothing is delivered.
    let page = {
        let _installed = granted.install(Some(elsewhere.clone()));
        let page = r.observe_with(&asked).unwrap();
        let result = r
            .engine
            .execute_action(&live_click(&page, "elsewhere", ExpectedEffect::None));
        assert_policy_denied(
            &result,
            "foreground application is not on the local allowlist",
        );
        page
    };
    assert_eq!(input_value(&page), hello, "this grant reads values");
    // A grant without `read_values` hides the values of the same snapshot.
    {
        let _installed = granted.install(Some(blind));
        let served = r.engine.serve_published(&asked).unwrap().unwrap();
        assert_eq!(input_value(&served), None);
    }
    // No grant: the default one, which delivers nothing and shows no value.
    {
        let _installed = granted.install(None);
        let served = r.engine.serve_published(&asked).unwrap().unwrap();
        assert_eq!(input_value(&served), None);
        let result = r
            .engine
            .execute_action(&live_click(&page, "nothing", ExpectedEffect::None));
        assert_policy_denied(
            &result,
            "live input is not allowed by the local policy or no live actuator is available",
        );
    }
    assert!(
        call_refused_early(
            &granted,
            &mut r.engine,
            &live_click(&page, "early", ExpectedEffect::None)
        )
        .is_none()
    );
    assert_eq!(r.world().delivered.len(), 0);

    // The guard is gone: the next call is under the operator's policy again.
    assert_eq!(granted.current(), r.policy.current());
    assert_eq!(r.engine.serve_published(&asked), Ok(Some(page.clone())));
    let result = r
        .engine
        .execute_action(&live_click(&page, "inner", ExpectedEffect::None));
    assert!(result.input_delivered(), "{result:?}");

    // The operator's stop refuses a delivery under every grant.
    let page = r.observe();
    r.engage_operator_stop();
    for (i, grant) in [Some(full_grant()), Some(elsewhere), None]
        .into_iter()
        .enumerate()
    {
        let _installed = granted.install(grant);
        let result = r.engine.execute_action(&live_click(
            &page,
            &format!("stopped-{i}"),
            ExpectedEffect::None,
        ));
        assert_policy_denied(
            &result,
            "the operator stopped live input; nothing was delivered",
        );
    }
    assert_eq!(r.world().delivered.len(), 1);
    r.clear_operator_stop();
    {
        let _installed = granted.install(Some(full_grant()));
        let result = r
            .engine
            .execute_action(&live_click(&page, "granted", ExpectedEffect::None));
        assert!(result.input_delivered(), "{result:?}");
    }

    // The operator's purge request and its acknowledgement reach the operator's store.
    {
        let _installed = granted.install(Some(full_grant()));
        r.policy.2.create();
        r.engine.honour_purge_request();
    }
    assert_eq!(r.policy.2.purges(), 1);
    assert!(!r.policy.purge_requested());
    assert_eq!(r.engine.serve_published(&asked), Ok(None));
}

// ---- everything together, as the broker will use it ----

/// What a client was given when it connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granted {
    /// The application, live input and values.
    Full,
    /// The application and values, no live input.
    NoInput,
    /// No valid grant.
    Nothing,
}

impl Granted {
    fn grant(self) -> Option<Grant> {
        match self {
            Self::Full => Some(full_grant()),
            Self::NoInput => Some(Grant {
                deliver_input: false,
                ..full_grant()
            }),
            Self::Nothing => None,
        }
    }
}

struct Client {
    id: ClientId,
    granted: Granted,
    page: Option<ObservePage>,
    last: Option<(ActionRequest, ActionResult)>,
    sent: usize,
}

/// What the interleaving saw, by kind.
#[derive(Default)]
struct Seen(BTreeMap<&'static str, usize>);

impl Seen {
    fn note(&mut self, kind: &'static str) {
        *self.0.entry(kind).or_default() += 1;
    }
}

/// The broker acts on what a lease operation changed: a new holder observes before it acts.
fn apply(engine: &mut Engine, change: &LeaseChange, seen: &mut Seen) {
    if change.spend_snapshot {
        engine.spend_snapshot();
        seen.note("lease spent the snapshot");
    }
}

const SCREEN_TEXT: [&str; 4] = ["Name", "hello", "Save", "Cancel"];

#[test]
#[allow(clippy::too_many_lines)] // one interleaving, checked after every step
fn several_clients_on_one_engine_deliver_once_per_snapshot_only_as_holder_under_their_grant() {
    let (mut r, granted) = granted_rig(form());
    r.world().live = true;
    r.set_policy(LocalPolicy {
        include_values: true,
        ..live_policy()
    });
    let mut table = LeaseTable::new(
        NonZeroUsize::new(JOURNAL_CLIENTS).unwrap(),
        Duration::from_millis(2_000),
    )
    .unwrap();
    let mut rng = Rng(0x0039_0040);
    let mut seen = Seen::default();
    let mut connected: Vec<Client> = Vec::new();
    let mut delivered_on = BTreeSet::new();
    let mut deliveries = 0_usize;
    let mut current: Option<String> = None;
    let mut stopped = false;
    let asked = ObserveRequest {
        include_values: true,
        ..ObserveRequest::default()
    };

    for step in 0..6_000 {
        r.clock.advance(Duration::from_millis(
            u64::try_from(rng.below(300)).unwrap(),
        ));
        let now = r.clock.now();
        let op = rng.below(40);
        if connected.is_empty() || (op == 0 && connected.len() < 4) {
            let outcome = table.connect(now);
            apply(&mut r.engine, &outcome.change, &mut seen);
            let granted = [Granted::Full, Granted::NoInput, Granted::Nothing][rng.below(3)];
            connected.push(Client {
                id: outcome.result.unwrap(),
                granted,
                page: None,
                last: None,
                sent: 0,
            });
            seen.note("connect");
            continue;
        }
        let index = rng.below(connected.len());
        let (id, kind) = (connected[index].id, connected[index].granted);
        match op {
            0..=2 => {
                let outcome = table.disconnect(id, now);
                apply(&mut r.engine, &outcome.change, &mut seen);
                outcome.result.unwrap();
                r.engine.forget_client(id);
                connected.remove(index);
                seen.note("disconnect");
                if connected.is_empty() {
                    r.engine.purge();
                    current = None;
                    seen.note("last client left: purge");
                }
            }
            3..=6 => {
                let outcome = table.acquire(id, now);
                apply(&mut r.engine, &outcome.change, &mut seen);
                seen.note(match outcome.result {
                    Ok(Acquired::Granted { .. }) => "lease granted",
                    Ok(Acquired::Renewed { .. }) => "lease renewed",
                    Ok(Acquired::Queued { .. }) => "lease queued",
                    Ok(Acquired::Implicit) => "lease implicit",
                    Err(_) => "lease renewal refused",
                });
            }
            7 => {
                let outcome = table.release(id, now);
                apply(&mut r.engine, &outcome.change, &mut seen);
                seen.note(if outcome.result.is_ok() {
                    "lease released"
                } else {
                    "release refused"
                });
            }
            8..=17 => {
                let outcome = table.publishes(id, now);
                apply(&mut r.engine, &outcome.change, &mut seen);
                let publishes = outcome.result.unwrap();
                let captures = r.world().seq;
                let page = {
                    let _installed = granted.install(kind.grant());
                    if publishes {
                        let page = r.engine.observe_screen(&asked).unwrap();
                        current = Some(page.snapshot_id.clone());
                        seen.note("published");
                        Some(page)
                    } else {
                        let page = r.engine.serve_published(&asked).unwrap();
                        assert_eq!(r.world().seq, captures, "step {step}: serving captured");
                        seen.note(if page.is_some() {
                            "served the published page"
                        } else {
                            "nothing published"
                        });
                        page
                    }
                };
                if let Some(page) = &page {
                    assert_eq!(Some(&page.snapshot_id), current.as_ref(), "step {step}");
                    let shows = input_value(page).is_some();
                    assert_eq!(shows, kind != Granted::Nothing, "step {step}: values");
                    seen.note(if shows {
                        "values shown"
                    } else {
                        "values hidden"
                    });
                }
                connected[index].page = page;
            }
            18..=33 => {
                let Some(page) = connected[index].page.clone() else {
                    seen.note("no page to act on");
                    continue;
                };
                let client = &mut connected[index];
                client.sent += 1;
                let request_id = format!("c{}-{}", id.get(), client.sent);
                let live = op <= 27;
                let request = if live {
                    live_click(&page, &request_id, ExpectedEffect::None)
                } else {
                    dry_click(&page, &request_id)
                };
                if live {
                    let outcome = table.may_deliver(id, now);
                    apply(&mut r.engine, &outcome.change, &mut seen);
                    if !outcome.result.unwrap() {
                        // The broker refuses before the gate (ADR-0039, decision 4).
                        seen.note("refused without the lease");
                        continue;
                    }
                }
                let before = r.world().delivered.len();
                let result = {
                    let _installed = granted.install(kind.grant());
                    r.engine.execute_action_for(id, &request)
                };
                if result.input_delivered() {
                    deliveries += 1;
                    assert!(live, "step {step}: a dry run delivered");
                    assert_eq!(
                        kind,
                        Granted::Full,
                        "step {step}: delivered without a grant"
                    );
                    assert_eq!(
                        Some(&request.snapshot_id),
                        current.as_ref(),
                        "step {step}: not the current snapshot"
                    );
                    assert!(
                        delivered_on.insert(request.snapshot_id.clone()),
                        "step {step}: a second delivery on {}",
                        request.snapshot_id
                    );
                    assert_eq!(r.world().delivered.len(), before + 1);
                    seen.note("delivered");
                } else {
                    assert_eq!(r.world().delivered.len(), before, "step {step}");
                    seen.note(match (result.status, error_code(&result)) {
                        (ActionStatus::Simulated, _) => "dry run simulated",
                        (_, Some(ErrorCode::StaleSnapshot)) => "refused: stale snapshot",
                        (_, Some(ErrorCode::PolicyDenied)) if stopped => "refused: operator stop",
                        (_, Some(ErrorCode::PolicyDenied)) => "refused: grant",
                        other => panic!("step {step}: unexpected {other:?}"),
                    });
                }
                if let Some(error) = &result.error {
                    for text in SCREEN_TEXT {
                        assert!(!error.message.contains(text), "step {step}: {error:?}");
                    }
                }
                connected[index].last = Some((request, result));
            }
            34..=37 => {
                let Some((request, stored)) = connected[index].last.clone() else {
                    continue;
                };
                if !request.dry_run {
                    let outcome = table.may_deliver(id, now);
                    apply(&mut r.engine, &outcome.change, &mut seen);
                    if !outcome.result.unwrap() {
                        seen.note("refused without the lease");
                        continue;
                    }
                }
                let before = r.world().delivered.len();
                let replay = {
                    let _installed = granted.install(kind.grant());
                    r.engine.execute_action_for(id, &request)
                };
                assert_eq!(
                    replay, stored,
                    "step {step}: a retry replays its own result"
                );
                assert_eq!(r.world().delivered.len(), before, "step {step}");
                seen.note("retry replayed");
            }
            _ => {
                if stopped {
                    r.clear_operator_stop();
                    seen.note("operator cleared the stop");
                } else {
                    r.engage_operator_stop();
                    let change = table.revoke();
                    apply(&mut r.engine, &change, &mut seen);
                    seen.note("operator stop revoked the lease");
                }
                stopped = !stopped;
            }
        }
        assert_eq!(
            r.world().delivered.len(),
            deliveries,
            "step {step}: the actuator's events are not the deliveries"
        );
    }
    let kinds = [
        "connect",
        "disconnect",
        "last client left: purge",
        "lease granted",
        "lease renewed",
        "lease queued",
        "lease implicit",
        "lease renewal refused",
        "lease released",
        "release refused",
        "lease spent the snapshot",
        "published",
        "served the published page",
        "nothing published",
        "values shown",
        "values hidden",
        "refused without the lease",
        "delivered",
        "dry run simulated",
        "refused: stale snapshot",
        "refused: grant",
        "refused: operator stop",
        "retry replayed",
        "operator stop revoked the lease",
        "operator cleared the stop",
    ];
    for kind in kinds {
        assert!(
            seen.0.contains_key(kind),
            "never seen: {kind}; seen {:?}",
            seen.0
        );
    }
    eprintln!("several clients: {:?}", seen.0);
}
