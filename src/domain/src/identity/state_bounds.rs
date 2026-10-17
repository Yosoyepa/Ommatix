//! Test-only pre-change set-based registry; never linked into production.
use super::*;

#[derive(Debug, Clone, PartialEq)]
struct SetRegistry {
    session_id: String,
    geometry_epoch: u64,
    snapshot_id: u64,
    next_id: u64,
    tracks: BTreeMap<TargetId, Track>,
    retired: BTreeSet<TargetId>,
}

#[allow(dead_code)] // Frozen pre-change reference includes the old public accessors.
impl SetRegistry {
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

    /// Retired identifiers retained for this session; this set has no fixed capacity.
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    pub fn is_retired(&self, id: TargetId) -> bool {
        self.retired.contains(&id)
    }

    /// The registry a purge leaves behind (chapter 08 §8.4): no track, hence no label, box or
    /// revision, but the same session, epoch and counters. The live IDs join the retired set,
    /// so the next observation issues new ones and an action naming an old one is refused.
    pub fn purged(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            geometry_epoch: self.geometry_epoch,
            snapshot_id: self.snapshot_id,
            next_id: self.next_id,
            tracks: BTreeMap::new(),
            retired: self
                .retired
                .iter()
                .chain(self.tracks.keys())
                .copied()
                .collect(),
        }
    }

    /// The registry after `observations`, taken at `geometry_epoch`.
    ///
    /// A track keeps its ID only when exactly one observation is compatible with it and it
    /// is the only track compatible with that observation: same role, same normalised label
    /// and discriminant, and an IoU of at least `policy.min_iou`. A new geometry epoch
    /// associates nothing. Unmatched tracks retire, each with the reason the registry
    /// recorded ([`Retirement`]); unmatched observations get new IDs in reading order (top to
    /// bottom, left to right). Uncertain continuity therefore means a new ID, never the ID
    /// of the most similar neighbour (ADR-0026).
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
        let (matched_track, retirements) =
            associate(&tracks, &order, same_geometry, policy.min_iou);

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
        for (t, retirement) in tracks.iter().zip(retirements) {
            if let Some(reason) = retirement {
                next.retired.insert(t.target_id);
                delta.retired.push(t.target_id);
                delta.reasons.push((t.target_id, reason));
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

fn observation(x: u32, label: &str) -> Observation {
    Observation {
        bbox: BBox::new(f64::from(x), 0.0, f64::from(x + 10), 10.0).unwrap(),
        role: Role::Button,
        label: Some(label.into()),
        discriminant: None,
        affordance: Affordance {
            actionability: crate::fusion::Actionability::Confirmed,
            enabled: true,
            visible: true,
        },
    }
}

/// UT-35/OR-38 and UT-41/OR-47 semantics under generated transition histories.
#[test]
fn rnf_06_derived_retirement_matches_set_reference_over_1000_histories() {
    for history in 0..1000_u32 {
        let mut registry = Registry::new("s", 1);
        let mut reference = SetRegistry::new("s", 1);
        let mut state = history + 1;
        let mut issued = BTreeSet::new();
        for step in 0..24 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let choice = if step < 8 { step } else { state % 8 };
            let x = state % 40;
            let evidence = match choice {
                0 => vec![observation(0, "a")],  // creation/return after occlusion
                1 => vec![observation(1, "a")],  // small move
                2 => vec![observation(30, "a")], // large move
                3 => vec![],                     // disappearance/occlusion
                4 => vec![observation(x, "a"), observation(x, "a")], // ambiguity
                _ => vec![observation(x, "b")],
            };
            if choice == 5 {
                registry = registry.purged();
                reference = reference.purged();
            }
            let epoch = registry.geometry_epoch + u64::from(choice == 6);
            let (next, changes) = registry
                .observe(epoch, &evidence, AssociationPolicy::default())
                .unwrap();
            let (old_next, old_changes) = reference
                .observe(epoch, &evidence, AssociationPolicy::default())
                .unwrap();
            assert_eq!(changes, old_changes, "history={history}, step={step}");
            for id in &changes.created {
                assert!(issued.insert(*id), "no ID reuse");
            }
            assert_eq!(next.tracks, old_next.tracks);
            assert_eq!(next.next_id, old_next.next_id);
            registry = next;
            reference = old_next;
            // Includes the invalid zero identifier and future IDs.
            for number in 0..registry.next_id + 5 {
                let id = TargetId(number);
                assert_eq!(
                    registry.is_retired(id),
                    reference.is_retired(id),
                    "history={history}, step={step}, id={number}"
                );
                let text = format!("e{number}");
                let action = ActionRef {
                    session_id: "s",
                    geometry_epoch: epoch,
                    snapshot_id: registry.snapshot_id,
                    target_id: &text,
                    expected_revision: registry.get(id).map_or(1, |t| t.revision),
                };
                let actual = registry.check_action(&action, Delivery::DryRun);
                assert_eq!(actual, reference.check_action(&action, Delivery::DryRun));
                if !registry.tracks.contains_key(&id) {
                    assert_eq!(actual, Err(ActionRejection::TargetNotFound));
                }
            }
        }
        let purged = registry.purged();
        let old_purged = reference.purged();
        for number in 0..purged.next_id + 5 {
            assert_eq!(
                purged.is_retired(TargetId(number)),
                old_purged.is_retired(TargetId(number))
            );
        }
    }
}
