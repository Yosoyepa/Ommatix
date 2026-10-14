//! Algorithm 3B of the thesis: stable identity by tracking (chapter 04 §4.5, ADR-0005), and
//! the validity checks an action must pass before anything reaches the OS.
//!
//! A target ID (`e7`) is assigned once per session, is opaque and is never reused, not even
//! after its element disappears. Observations are associated to tracks one to one, only when
//! the association is unambiguous; otherwise the old track retires and the new element gets
//! a new ID. Every change an action depends on bumps the track's revision, and every
//! observation is a new snapshot. An action names session, epoch, snapshot, target and
//! revision; any mismatch rejects it, without looking for a "similar" element.
//!
//! Reference: `Registry` in the Python oracle (OR-37..OR-47).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::bbox::BBox;
use crate::fusion::{Affordance, NotActionable, Role, normalized};

/// Session-scoped element identifier, written `e<n>` with `n >= 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId(u64);

impl TargetId {
    pub const fn number(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "e{}", self.0)
    }
}

impl FromStr for TargetId {
    type Err = ();

    /// Parses `e<n>`; anything else (`e0`, `e07`, `7`, `E7`) is not an ID this engine issued.
    fn from_str(s: &str) -> Result<Self, ()> {
        let digits = s.strip_prefix('e').ok_or(())?;
        if digits.is_empty()
            || digits.starts_with('0')
            || !digits.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(());
        }
        digits.parse().map(Self).map_err(|_| ())
    }
}

/// One element as fusion published it, before identity is assigned.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub bbox: BBox,
    pub role: Role,
    pub label: Option<String>,
    /// Content that tells repeated rows apart (a file name in a list). A change of it is a
    /// different element, even at the same place.
    pub discriminant: Option<String>,
    pub affordance: Affordance,
}

/// An element with its identity.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub target_id: TargetId,
    /// Starts at 1 and grows with every change an action depends on.
    pub revision: u64,
    pub observation: Observation,
}

#[derive(Debug, Error, Clone, Copy, PartialEq)]
pub enum IdentityError {
    #[error("association IoU threshold must be in (0, 1], got {0}")]
    InvalidMinIou(f64),
}

/// How close an observation must stay to its track to keep the ID.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssociationPolicy {
    /// Smallest IoU between the track's last box and the new one. An initial value [P], to
    /// be calibrated with the matcher.
    pub min_iou: f64,
}

impl Default for AssociationPolicy {
    fn default() -> Self {
        Self { min_iou: 0.5 }
    }
}

/// What an observation did to the registry, each list sorted by ID.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub created: Vec<TargetId>,
    pub updated: Vec<TargetId>,
    pub retired: Vec<TargetId>,
}

/// The tracks of one session at one snapshot. Observing returns the next registry, so it can
/// be staged and committed with the rest of the snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Registry {
    session_id: String,
    geometry_epoch: u64,
    snapshot_id: u64,
    next_id: u64,
    tracks: BTreeMap<TargetId, Track>,
    retired: BTreeSet<TargetId>,
}

impl Registry {
    pub fn new(session_id: impl Into<String>, geometry_epoch: u64) -> Self {
        Self {
            session_id: session_id.into(),
            geometry_epoch,
            snapshot_id: 0,
            next_id: 1,
            tracks: BTreeMap::new(),
            retired: BTreeSet::new(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub const fn geometry_epoch(&self) -> u64 {
        self.geometry_epoch
    }

    pub const fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    /// Live tracks, by ID.
    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    pub fn get(&self, id: TargetId) -> Option<&Track> {
        self.tracks.get(&id)
    }

    pub fn is_retired(&self, id: TargetId) -> bool {
        self.retired.contains(&id)
    }

    /// The registry after `observations`, taken at `geometry_epoch`.
    ///
    /// A track keeps its ID only when exactly one observation is compatible with it and it
    /// is the only track compatible with that observation: same role, same normalised label
    /// and discriminant, and an IoU of at least `policy.min_iou`. A new geometry epoch
    /// associates nothing. Unmatched tracks retire; unmatched observations get new IDs in
    /// reading order (top to bottom, left to right).
    pub fn observe(
        &self,
        geometry_epoch: u64,
        observations: &[Observation],
        policy: AssociationPolicy,
    ) -> Result<(Self, Changes), IdentityError> {
        if !(policy.min_iou > 0.0 && policy.min_iou <= 1.0) {
            return Err(IdentityError::InvalidMinIou(policy.min_iou));
        }
        let mut order: Vec<&Observation> = observations.iter().collect();
        order.sort_by(|a, b| {
            a.bbox
                .y0()
                .total_cmp(&b.bbox.y0())
                .then(a.bbox.x0().total_cmp(&b.bbox.x0()))
                .then(a.bbox.y1().total_cmp(&b.bbox.y1()))
                .then(a.bbox.x1().total_cmp(&b.bbox.x1()))
                .then(a.role.cmp(&b.role))
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| a.discriminant.cmp(&b.discriminant))
        });

        let same_geometry = geometry_epoch == self.geometry_epoch;
        let tracks: Vec<&Track> = self.tracks.values().collect();
        let compatible = |t: &Track, o: &Observation| {
            same_geometry
                && t.observation.role == o.role
                && key(t.observation.label.as_deref()) == key(o.label.as_deref())
                && key(t.observation.discriminant.as_deref()) == key(o.discriminant.as_deref())
                && t.observation.bbox.iou(&o.bbox) >= policy.min_iou
        };
        let by_track: Vec<Vec<usize>> = tracks
            .iter()
            .map(|t| {
                (0..order.len())
                    .filter(|&j| compatible(t, order[j]))
                    .collect()
            })
            .collect();
        let mut matched_track = vec![None; order.len()];
        for (i, candidates) in by_track.iter().enumerate() {
            if let [j] = candidates[..] {
                let rivals = by_track.iter().filter(|c| c.contains(&j)).count();
                if rivals == 1 {
                    matched_track[j] = Some(i);
                }
            }
        }

        let mut next = Self {
            session_id: self.session_id.clone(),
            geometry_epoch,
            snapshot_id: self.snapshot_id + 1,
            next_id: self.next_id,
            tracks: BTreeMap::new(),
            retired: self.retired.clone(),
        };
        let mut delta = Changes::default();
        for (j, observation) in order.iter().enumerate() {
            let track = if let Some(i) = matched_track[j] {
                let old = tracks[i];
                let modified = old.observation != **observation;
                if modified {
                    delta.updated.push(old.target_id);
                }
                Track {
                    target_id: old.target_id,
                    revision: old.revision + u64::from(modified),
                    observation: (*observation).clone(),
                }
            } else {
                let id = TargetId(next.next_id);
                next.next_id += 1;
                delta.created.push(id);
                Track {
                    target_id: id,
                    revision: 1,
                    observation: (*observation).clone(),
                }
            };
            next.tracks.insert(track.target_id, track);
        }
        for t in &tracks {
            if !next.tracks.contains_key(&t.target_id) {
                next.retired.insert(t.target_id);
                delta.retired.push(t.target_id);
            }
        }
        delta.updated.sort_unstable();
        Ok((next, delta))
    }

    /// Checks an action against this (committed) registry, in the thesis's order: session,
    /// epoch, snapshot, target, revision, actionability, then the delivery mode.
    pub fn check_action(
        &self,
        action: &ActionRef<'_>,
        delivery: Delivery,
    ) -> Result<Clearance, ActionRejection> {
        if action.session_id != self.session_id {
            return Err(ActionRejection::SessionMismatch);
        }
        if action.geometry_epoch != self.geometry_epoch {
            return Err(ActionRejection::EpochMismatch);
        }
        if action.snapshot_id != self.snapshot_id {
            return Err(ActionRejection::StaleSnapshot);
        }
        let track = action
            .target_id
            .parse::<TargetId>()
            .ok()
            .and_then(|id| self.tracks.get(&id))
            .ok_or(ActionRejection::TargetNotFound)?;
        if action.expected_revision != track.revision {
            return Err(ActionRejection::StaleTarget);
        }
        track
            .observation
            .affordance
            .check()
            .map_err(ActionRejection::NotActionable)?;
        match delivery {
            Delivery::DryRun => Ok(Clearance::Simulate {
                target_id: track.target_id,
            }),
            Delivery::Live {
                actuator_available: false,
            } => Err(ActionRejection::ActuatorUnavailable),
            Delivery::Live {
                actuator_available: true,
            } => Ok(Clearance::Deliver {
                target_id: track.target_id,
                bbox: track.observation.bbox,
            }),
        }
    }
}

/// Label or discriminant as compared for association: normalised, absent equals empty.
fn key(text: Option<&str>) -> String {
    text.map(normalized).unwrap_or_default()
}

/// The references an action carries, as the agent sent them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionRef<'a> {
    pub session_id: &'a str,
    pub geometry_epoch: u64,
    pub snapshot_id: u64,
    pub target_id: &'a str,
    pub expected_revision: u64,
}

/// How an action would be carried out. Dry run is the default of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    DryRun,
    Live { actuator_available: bool },
}

/// An action that passed every check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Clearance {
    /// Validated only; nothing is sent to the OS.
    Simulate { target_id: TargetId },
    /// May be handed to the actuator, after the local policy and focus checks.
    Deliver { target_id: TargetId, bbox: BBox },
}

impl Clearance {
    /// The status to report when the clearance is all that happened.
    pub const fn status(&self) -> ActionStatus {
        match self {
            Self::Simulate { .. } => ActionStatus::Simulated,
            // Cleared is not delivered: the actuator reports that, not the gate.
            Self::Deliver { .. } => ActionStatus::Indeterminate,
        }
    }
}

/// Why an action was rejected; the contract code is in brackets.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ActionRejection {
    #[error("the action belongs to another session [SESSION_MISMATCH]")]
    SessionMismatch,
    #[error("the screen geometry changed since that observation [EPOCH_MISMATCH]")]
    EpochMismatch,
    #[error("a newer snapshot exists; observe again [STALE_SNAPSHOT]")]
    StaleSnapshot,
    #[error("no live element has that ID [TARGET_NOT_FOUND]")]
    TargetNotFound,
    #[error("the element changed since that observation [STALE_TARGET]")]
    StaleTarget,
    #[error("the element cannot be acted on: {0} [NOT_ACTIONABLE]")]
    NotActionable(NotActionable),
    #[error("live input was requested but no actuator is available")]
    ActuatorUnavailable,
}

/// Outcome statuses of the contract. Each one says no more than what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Simulated,
    Rejected,
    InputDelivered,
    EffectObserved,
    Indeterminate,
}

impl ActionStatus {
    /// Whether input reached the OS. Never true for a simulation or a rejection.
    pub const fn input_delivered(self) -> bool {
        matches!(self, Self::InputDelivered | Self::EffectObserved)
    }

    /// Whether the declared effect was checked.
    pub const fn verified(self) -> bool {
        matches!(self, Self::EffectObserved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fusion::Actionability;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn button(bbox: BBox, label: &str) -> Observation {
        Observation {
            bbox,
            role: Role::Button,
            label: Some(label.to_owned()),
            discriminant: None,
            affordance: Affordance {
                actionability: Actionability::Confirmed,
                enabled: true,
                visible: true,
            },
        }
    }

    fn observe(registry: &Registry, observations: &[Observation]) -> (Registry, Changes) {
        registry
            .observe(
                registry.geometry_epoch(),
                observations,
                AssociationPolicy::default(),
            )
            .unwrap()
    }

    fn id(n: u64) -> TargetId {
        TargetId(n)
    }

    fn action<'a>(registry: &'a Registry, target: &'a str, revision: u64) -> ActionRef<'a> {
        ActionRef {
            session_id: registry.session_id(),
            geometry_epoch: registry.geometry_epoch(),
            snapshot_id: registry.snapshot_id(),
            target_id: target,
            expected_revision: revision,
        }
    }

    /// The dossier's original ID: FNV-1a over role, label and the 64 px cell of the
    /// position, reduced to five digits. Kept only to reproduce its counterexamples.
    fn legacy_id(role: u8, label: &str, x: u32, y: u32) -> u32 {
        let mut h: u32 = 2_166_136_261;
        let mut eat = |byte: u8| h = (h ^ u32::from(byte)).wrapping_mul(16_777_619);
        eat(role);
        for b in label.to_lowercase().replace(' ', "").bytes() {
            eat(b);
        }
        for b in ((x >> 6) as u16)
            .to_le_bytes()
            .into_iter()
            .chain(((y >> 6) as u16).to_le_bytes())
        {
            eat(b);
        }
        10_000 + h % 90_000
    }

    /// Birthday collision probability of `n` IDs in a space of `space`.
    fn collision_probability(n: u32, space: u32) -> f64 {
        1.0 - (0..n)
            .map(|k| 1.0 - f64::from(k) / f64::from(space))
            .product::<f64>()
    }

    /// UT-32 (OR-34): the original ID changes when a button moves one pixel across a cell;
    /// the registry keeps it.
    #[test]
    fn ut_32_cell_boundary_changes_legacy_id_but_not_the_track() {
        assert_ne!(
            legacy_id(1, "Guardar", 63, 100),
            legacy_id(1, "Guardar", 64, 100)
        );
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(58.0, 100.0, 63.0, 110.0), "Guardar")],
        );
        let (r, changes) = observe(&r, &[button(bx(59.0, 100.0, 64.0, 110.0), "Guardar")]);
        assert_eq!(changes.created, vec![]);
        assert_eq!(r.get(id(1)).unwrap().revision, 2);
    }

    /// UT-33 (OR-35, OR-36): the original ID collides for repeated controls in a cell, and
    /// five digits make collisions likely; session IDs never collide.
    #[test]
    fn ut_33_legacy_ids_collide_and_session_ids_do_not() {
        assert_eq!(
            legacy_id(1, "Eliminar", 10, 10),
            legacy_id(1, "Eliminar", 20, 20)
        );
        let p = collision_probability(300, 90_000);
        assert!(p > 0.39 && p < 0.40, "{p}");

        let (r, changes) = observe(
            &Registry::new("s", 1),
            &[
                button(bx(10.0, 10.0, 30.0, 20.0), "Eliminar"),
                button(bx(20.0, 30.0, 40.0, 40.0), "Eliminar"),
            ],
        );
        assert_eq!(changes.created, vec![id(1), id(2)]);
        assert_eq!(r.tracks().count(), 2);
    }

    /// UT-34 (OR-37): moving a known element updates its box and revision, not its ID.
    #[test]
    fn ut_34_registry_does_not_depend_on_coordinates() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(60.0, 0.0, 66.0, 10.0), "Guardar")],
        );
        let (r, changes) = observe(&r, &[button(bx(61.0, 0.0, 67.0, 10.0), "Guardar")]);
        assert_eq!(changes.updated, vec![id(1)]);
        let t = r.get(id(1)).unwrap();
        assert_eq!(t.revision, 2);
        assert_eq!(t.observation.bbox, bx(61.0, 0.0, 67.0, 10.0));

        // Nothing changed: same ID, same revision.
        let (r, changes) = observe(&r, &[button(bx(61.0, 0.0, 67.0, 10.0), "Guardar")]);
        assert_eq!(changes, Changes::default());
        assert_eq!(r.get(id(1)).unwrap().revision, 2);
    }

    /// UT-35 (OR-38): a retired ID is never reused, even for an identical element.
    #[test]
    fn ut_35_retired_ids_are_not_reused() {
        let a = button(bx(0.0, 0.0, 10.0, 10.0), "A");
        let (r, _) = observe(&Registry::new("s", 1), std::slice::from_ref(&a));
        let (r, changes) = observe(&r, &[]);
        assert_eq!(changes.retired, vec![id(1)]);
        assert!(r.is_retired(id(1)));
        let (r, changes) = observe(&r, &[a]);
        assert_eq!(changes.created, vec![id(2)]);
        assert!(r.get(id(1)).is_none());
    }

    /// UT-36: one observation never feeds two tracks; indistinguishable cases abstain.
    #[test]
    fn ut_36_matching_is_one_to_one_and_abstains() {
        // Two identical buttons side by side keep their own IDs.
        let left = button(bx(0.0, 0.0, 40.0, 20.0), "OK");
        let right = button(bx(50.0, 0.0, 90.0, 20.0), "OK");
        let (r, _) = observe(&Registry::new("s", 1), &[left.clone(), right.clone()]);
        let (r, changes) = observe(&r, &[right, left]);
        assert_eq!(changes, Changes::default());

        // Two tracks, one observation that fits both: no guess, the tracks retire.
        let a = button(bx(0.0, 0.0, 40.0, 20.0), "OK");
        let b = button(bx(4.0, 0.0, 44.0, 20.0), "OK");
        let (r2, _) = observe(&Registry::new("s", 1), &[a, b]);
        let (_, changes) = observe(&r2, &[button(bx(2.0, 0.0, 42.0, 20.0), "OK")]);
        assert_eq!(changes.retired, vec![id(1), id(2)]);
        assert_eq!(changes.created, vec![id(3)]);

        // One track, two observations that fit it: both are new.
        let (_, changes) = observe(
            &r,
            &[
                button(bx(1.0, 0.0, 41.0, 20.0), "OK"),
                button(bx(0.0, 1.0, 40.0, 21.0), "OK"),
                button(bx(50.0, 0.0, 90.0, 20.0), "OK"),
            ],
        );
        assert_eq!(changes.retired, vec![id(1)]);
        assert_eq!(changes.created, vec![id(3), id(4)]);
    }

    /// UT-37: a virtualised row that now shows other content gets a new identity.
    #[test]
    fn ut_37_virtualised_row_with_new_content_retires_the_identity() {
        let row = |file: &str| Observation {
            discriminant: Some(file.to_owned()),
            ..button(bx(0.0, 100.0, 300.0, 120.0), "Abrir")
        };
        let (r, _) = observe(&Registry::new("s", 1), &[row("informe-A.pdf")]);
        let (r, changes) = observe(&r, &[row("informe-B.pdf")]);
        assert_eq!(changes.retired, vec![id(1)]);
        assert_eq!(changes.created, vec![id(2)]);
        assert_eq!(
            r.check_action(&action(&r, "e1", 1), Delivery::DryRun),
            Err(ActionRejection::TargetNotFound)
        );
    }

    /// UT-38 (OR-39): an action from an older snapshot is rejected even if its ID lives on.
    #[test]
    fn ut_38_stale_snapshot_is_rejected() {
        let a = button(bx(0.0, 0.0, 10.0, 10.0), "A");
        let (r, _) = observe(&Registry::new("s", 1), std::slice::from_ref(&a));
        let old = r.snapshot_id();
        let (r, _) = observe(&r, &[a, button(bx(20.0, 20.0, 30.0, 30.0), "B")]);
        let request = ActionRef {
            snapshot_id: old,
            ..action(&r, "e1", 1)
        };
        assert_eq!(
            r.check_action(&request, Delivery::DryRun),
            Err(ActionRejection::StaleSnapshot)
        );
    }

    /// UT-39 (OR-40): an action that expects an older revision is rejected.
    #[test]
    fn ut_39_stale_revision_is_rejected() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "A")],
        );
        let disabled = Observation {
            affordance: Affordance {
                enabled: false,
                ..Affordance::TEXT_NODE
            },
            ..button(bx(0.0, 0.0, 10.0, 10.0), "A")
        };
        let (r, changes) = observe(&r, &[disabled]);
        assert_eq!(changes.updated, vec![id(1)]);
        assert_eq!(r.get(id(1)).unwrap().revision, 2);
        assert_eq!(
            r.check_action(&action(&r, "e1", 1), Delivery::DryRun),
            Err(ActionRejection::StaleTarget)
        );
    }

    /// UT-40 (OR-42): references from another session are rejected.
    #[test]
    fn ut_40_other_session_is_rejected() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "A")],
        );
        let request = ActionRef {
            session_id: "other",
            ..action(&r, "e1", 1)
        };
        assert_eq!(
            r.check_action(&request, Delivery::DryRun),
            Err(ActionRejection::SessionMismatch)
        );
        let request = ActionRef {
            geometry_epoch: 2,
            ..action(&r, "e1", 1)
        };
        assert_eq!(
            r.check_action(&request, Delivery::DryRun),
            Err(ActionRejection::EpochMismatch)
        );
    }

    /// UT-41 (OR-47): retired, unknown and malformed IDs are not found.
    #[test]
    fn ut_41_missing_target_is_rejected() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "A")],
        );
        let (r, _) = observe(&r, &[]);
        for target in [
            "e1",
            "e9",
            "e01",
            "e0",
            "1",
            "E1",
            "",
            "e18446744073709551616",
        ] {
            assert_eq!(
                r.check_action(&action(&r, target, 1), Delivery::DryRun),
                Err(ActionRejection::TargetNotFound),
                "{target:?}"
            );
        }
    }

    /// UT-42 (OR-43): a dry run is reported as simulated, never as delivered input.
    #[test]
    fn ut_42_simulation_never_reports_delivered_input() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "A")],
        );
        let clearance = r
            .check_action(&action(&r, "e1", 1), Delivery::DryRun)
            .unwrap();
        assert_eq!(clearance, Clearance::Simulate { target_id: id(1) });
        assert_eq!(clearance.status(), ActionStatus::Simulated);
        assert!(!clearance.status().input_delivered());
        assert!(!clearance.status().verified());
        assert!(!ActionStatus::Rejected.input_delivered());
        assert!(!ActionStatus::Indeterminate.verified());
    }

    /// UT-43 (OR-44): live mode without an actuator is rejected.
    #[test]
    fn ut_43_live_mode_without_actuator_is_rejected() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "A")],
        );
        assert_eq!(
            r.check_action(
                &action(&r, "e1", 1),
                Delivery::Live {
                    actuator_available: false
                }
            ),
            Err(ActionRejection::ActuatorUnavailable)
        );
        assert!(matches!(
            r.check_action(
                &action(&r, "e1", 1),
                Delivery::Live {
                    actuator_available: true
                }
            ),
            Ok(Clearance::Deliver { .. })
        ));
    }

    #[test]
    fn text_nodes_and_disabled_elements_are_not_actionable() {
        let title = Observation {
            role: Role::TextNode,
            affordance: Affordance::TEXT_NODE,
            ..button(bx(0.0, 0.0, 100.0, 20.0), "Configuración")
        };
        let (r, _) = observe(&Registry::new("s", 1), &[title]);
        assert_eq!(
            r.check_action(&action(&r, "e1", 1), Delivery::DryRun),
            Err(ActionRejection::NotActionable(NotActionable::Unconfirmed))
        );
    }

    #[test]
    fn a_new_geometry_epoch_associates_nothing() {
        let a = button(bx(0.0, 0.0, 10.0, 10.0), "A");
        let (r, _) = observe(&Registry::new("s", 1), std::slice::from_ref(&a));
        let (r, changes) = r.observe(2, &[a], AssociationPolicy::default()).unwrap();
        assert_eq!(changes.retired, vec![id(1)]);
        assert_eq!(changes.created, vec![id(2)]);
        assert_eq!(r.geometry_epoch(), 2);
    }

    #[test]
    fn labels_are_compared_normalised() {
        let (r, _) = observe(
            &Registry::new("s", 1),
            &[button(bx(0.0, 0.0, 10.0, 10.0), "Guardar  cambios")],
        );
        let (_, changes) = observe(&r, &[button(bx(0.0, 0.0, 10.0, 10.0), "guardar cambios")]);
        assert_eq!(changes.updated, vec![id(1)]);
        assert_eq!(changes.created, vec![]);
    }

    #[test]
    fn target_ids_round_trip() {
        assert_eq!("e7".parse::<TargetId>(), Ok(id(7)));
        assert_eq!(id(42).to_string(), "e42");
    }

    #[test]
    fn association_policy_is_validated() {
        assert_eq!(
            Registry::new("s", 1)
                .observe(1, &[], AssociationPolicy { min_iou: 0.0 })
                .unwrap_err(),
            IdentityError::InvalidMinIou(0.0)
        );
    }
}
