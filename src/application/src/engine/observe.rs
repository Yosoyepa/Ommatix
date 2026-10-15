//! One transactional observation (SDD §2.7, thesis chapter 04 §4.2).
//!
//! The candidate snapshot is built aside from the published one and committed in one
//! step. Every stage after the capture takes the region plan as input: only the planned
//! regions reach the detector, the recogniser and the accessibility provider, and
//! evidence outside them is kept. A failed stage publishes nothing, keeps the planned
//! tiles pending and blocks actions.
//!
//! The detector, the recogniser and the accessibility provider read the same frame and do
//! not depend on each other, so an observation runs them concurrently on scoped threads
//! (SDD §2.7, ADR-0019) and joins them before anything is fused: the coordinator stays the
//! single writer, and the results are collected by stage, never by arrival order, so the
//! evidence is the same as when they ran one after the other.
//!
//! Accessibility evidence (ADR-0017) is reconciled with the detector's boxes before the
//! text is fused: an element absorbs the detector boxes that are the same control, its
//! role and name win (`label_source: accessibility`), and OCR words still go to the most
//! specific control by IoC.
//!
//! A bounded accessibility walk can end early (`AccessibleRead::truncated`). The regions it
//! was asked about then keep a *gap*: they are read again on the next observation even when
//! no pixel changed there, evidence already held for them that the partial read did not
//! contradict is kept (so a truncated read never takes away a confirmation or a password's
//! `sensitive` mark), and a visual box there is not confirmed on the strength of a tree
//! that may lack the control its click point falls on.
//!
//! Each accessibility element keeps the instant its read started (ADR-0025). A publication
//! that reuses it keeps that instant, and an observation reads again the regions whose
//! evidence has expired, so a semantic change with no repaint is seen once the window ends.

use std::collections::BTreeSet;
use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{
    AccessibleControl, Actionability, Affordance, Control, FusedControl, LabelChoice, Role,
    TextKind, Word, fuse, reconcile_accessibility,
};
use ommatix_domain::identity::{Changes, Observation, Registry, TargetId};
use ommatix_domain::incremental::{BgraFrame, Evidence, RegionPlan, Work, plan_regions};

use super::accessibility::accessible_control;
use super::icons::{self, IconLabel, Scope};
use super::{Engine, Ports, aged_out};
use crate::PortError;
use crate::contract::{ErrorCode, LabelSource, ObservationPlan, StageTimings, ToolError, WorkKind};
use crate::ports::{AccessibilityProvider, Clock, InputShape, TextRecognizer, VisionDetector};

/// Raw evidence of the last committed observation, clipped to its frame and sorted.
#[derive(Debug, Clone, Default)]
pub(super) struct EvidenceSet {
    pub controls: Vec<Control>,
    pub words: Vec<Word>,
    /// Accessibility elements on screen (ADR-0017), each with the instant its read started.
    pub accessible: Vec<ReadAccessible>,
}

/// An accessibility element and the instant its read started on the injected clock
/// (ADR-0025). Its age is measured from that instant: a later publication that keeps the
/// element does not make the element newer.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ReadAccessible {
    pub element: AccessibleControl,
    pub read_at: Duration,
}

impl EvidenceSet {
    /// Boxes a region must not cut in half.
    fn supports(&self) -> Vec<BBox> {
        self.controls
            .iter()
            .map(|c| c.bbox)
            .chain(self.words.iter().map(|w| w.bbox))
            .chain(self.accessible.iter().map(|a| a.element.control.bbox))
            .collect()
    }

    /// Keeps the evidence outside the regions that were looked at again and adds the fresh
    /// evidence found inside them. `detected` are the regions the detector and the
    /// recogniser re-analysed (the plan); `read` are those the accessibility provider read,
    /// the plan plus the regions of an earlier truncated read.
    ///
    /// When the accessibility read was `truncated`, a held element inside `read` is kept
    /// unless the plan re-analysed its area (its pixels changed, so it may be gone) or the
    /// read found an element at the same box (the fresh one replaces it): a partial read
    /// proves nothing about what it did not reach.
    fn merged(&self, detected: &[BBox], read: &[BBox], fresh: Self, truncated: bool) -> Self {
        let outside = |regions: &[BBox], b: &BBox| !regions.iter().any(|r| r.overlaps(b));
        let keeps = |a: &ReadAccessible| {
            let b = &a.element.control.bbox;
            outside(read, b)
                || (truncated
                    && outside(detected, b)
                    && !fresh
                        .accessible
                        .iter()
                        .any(|f| f.element.control.bbox == *b))
        };
        let mut next = Self {
            controls: self
                .controls
                .iter()
                .filter(|c| outside(detected, &c.bbox))
                .cloned()
                .collect(),
            words: self
                .words
                .iter()
                .filter(|w| outside(detected, &w.bbox))
                .cloned()
                .collect(),
            accessible: self
                .accessible
                .iter()
                .filter(|a| keeps(a))
                .cloned()
                .collect(),
        };
        next.controls.extend(fresh.controls);
        next.words.extend(fresh.words);
        next.accessible.extend(fresh.accessible);
        next
    }

    /// Clips every box to `bounds`, drops what falls outside, sorts and removes exact
    /// duplicates (a detector asked about two regions may report one control twice).
    fn normalised(mut self, bounds: &BBox) -> Self {
        self.controls = self
            .controls
            .into_iter()
            .filter_map(|c| {
                Some(Control {
                    bbox: c.bbox.clip(bounds)?,
                    ..c
                })
            })
            .collect();
        self.words = self
            .words
            .into_iter()
            .filter_map(|w| {
                Some(Word {
                    bbox: w.bbox.clip(bounds)?,
                    text: w.text,
                })
            })
            .collect();
        self.controls.sort_by(|a, b| {
            a.bbox
                .cmp_corners(&b.bbox)
                .then(a.role.cmp(&b.role))
                .then(a.affordance.actionability.cmp(&b.affordance.actionability))
                .then(a.affordance.enabled.cmp(&b.affordance.enabled))
                .then(a.affordance.visible.cmp(&b.affordance.visible))
        });
        self.controls.dedup();
        self.words.sort_by(|a, b| {
            a.bbox
                .cmp_corners(&b.bbox)
                .then_with(|| a.text.cmp(&b.text))
        });
        self.words.dedup();
        self.accessible = self
            .accessible
            .into_iter()
            .filter_map(|a| {
                let bbox = a.element.control.bbox.clip(bounds)?;
                Some(ReadAccessible {
                    element: AccessibleControl {
                        control: Control {
                            bbox,
                            ..a.element.control
                        },
                        ..a.element
                    },
                    read_at: a.read_at,
                })
            })
            .collect();
        self.accessible.sort_by(|a, b| {
            let (x, y) = (&a.element.control, &b.element.control);
            x.bbox
                .cmp_corners(&y.bbox)
                .then(x.role.cmp(&y.role))
                .then_with(|| a.element.label.cmp(&b.element.label))
                .then(x.affordance.actionability.cmp(&y.affordance.actionability))
                .then(x.affordance.enabled.cmp(&y.affordance.enabled))
                .then(x.affordance.visible.cmp(&y.affordance.visible))
                .then(a.element.sensitive.cmp(&b.element.sensitive))
        });
        self.accessible.dedup();
        self
    }
}

/// One published element with what the view needs besides its track.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct StoredElement {
    pub target_id: TargetId,
    pub revision: u64,
    pub role: Role,
    /// Full label; the view truncates it.
    pub label: String,
    pub label_source: LabelSource,
    pub affordance: Affordance,
    /// Inner text and its kind, for roles that hold a value. Redacted by the view unless
    /// both the request and the local policy allow values.
    pub value: Option<(String, TextKind)>,
    pub text_node: bool,
    /// When the accessibility read that confirms this element started (ADR-0025); `None`
    /// when no accessibility evidence confirms it.
    pub evidence_at: Option<Duration>,
}

/// The payload of a committed snapshot.
#[derive(Debug, Clone)]
pub(super) struct Published {
    pub registry: Registry,
    pub evidence: EvidenceSet,
    /// Control ↔ external label pairs, re-processed together by the next plan.
    pub relations: Vec<(BBox, BBox)>,
    /// Curated icon labels (Algorithm 3A), kept for the controls the next plan leaves
    /// untouched.
    pub icons: Vec<IconLabel>,
    pub frame_seq: u64,
    pub captured_at: Duration,
    /// Sorted by target ID.
    pub elements: Vec<StoredElement>,
    /// Regions whose accessibility read was cut short by its budget and not completed
    /// since: the next observation reads them again, pixels changed or not. Empty when
    /// the evidence is complete.
    pub accessibility_gaps: Vec<BBox>,
    /// What the last observation did to the tracks: why each lost its ID (ADR-0026), as
    /// codes only.
    pub changes: Changes,
}

impl Published {
    /// `s<n>` on the wire.
    pub fn snapshot_id(&self) -> String {
        format!("s{}", self.registry.snapshot_id())
    }
}

/// What the observation that published the snapshot measured and planned. Kept beside the
/// snapshot, not inside it: its commit can only be timed after the snapshot is committed.
#[derive(Debug, Clone)]
pub(super) struct Measured {
    pub timings: StageTimings,
    pub plan: ObservationPlan,
}

/// An observation before identity: what the registry sees, plus the view's extras.
struct Candidate {
    observation: Observation,
    label: String,
    label_source: LabelSource,
    value: Option<(String, TextKind)>,
    text_node: bool,
    evidence_at: Option<Duration>,
}

/// `origin` is the accessibility element this control comes from, if any, and when its read
/// started: that element's name wins over every other label, and a password field publishes
/// no OCR text at all. `icon` is the curated label Algorithm 3A found for this control, if
/// any; it only replaces the visual label, never the affordance.
fn control_candidate(
    fc: &FusedControl,
    origin: Option<&ReadAccessible>,
    icon: Option<&str>,
) -> Candidate {
    let role = fc.control.role;
    let access = origin.map(|o| &o.element);
    let sensitive = access.is_some_and(|a| a.sensitive);
    let inner = if sensitive {
        String::new()
    } else {
        fc.inner_text()
    };
    let (label, label_source) = if let Some(a) = access.filter(|a| !a.label.is_empty()) {
        (a.label.clone(), LabelSource::Accessibility)
    } else if sensitive {
        (String::new(), LabelSource::Unknown)
    } else if let Some(icon) = icon {
        (icon.to_owned(), LabelSource::CuratedIcon)
    } else if role.takes_external_label() {
        match &fc.label {
            Some(LabelChoice::Label(word)) => (word.text.clone(), LabelSource::Ocr),
            _ => (String::new(), LabelSource::Unknown),
        }
    } else if inner.is_empty() || role.holds_value() {
        (String::new(), LabelSource::Unknown)
    } else {
        (inner.clone(), LabelSource::Ocr)
    };
    Candidate {
        observation: Observation {
            bbox: fc.control.bbox,
            role,
            label: (!label.is_empty()).then(|| label.clone()),
            // Only an accessibility element carries a native identity (ADR-0026): a visual
            // box has no evidence of being the same object across observations.
            discriminant: access.and_then(|a| a.discriminant.clone()),
            affordance: fc.control.affordance,
        },
        label,
        label_source,
        value: (role.holds_value() && !sensitive)
            .then(|| (inner, fc.text_kind.unwrap_or(TextKind::Unknown))),
        text_node: false,
        evidence_at: origin.map(|o| o.read_at),
    }
}

fn text_node_candidate(word: &Word) -> Candidate {
    Candidate {
        observation: Observation {
            bbox: word.bbox,
            role: Role::TextNode,
            label: Some(word.text.clone()),
            discriminant: None,
            affordance: Affordance::TEXT_NODE,
        },
        label: word.text.clone(),
        label_source: LabelSource::Ocr,
        value: None,
        text_node: true,
        evidence_at: None,
    }
}

/// Pairs each track with the candidate it was built from. Equal observations are
/// interchangeable, so the first unused equal candidate is taken, in track order.
fn stored_elements(registry: &Registry, candidates: Vec<Candidate>) -> Vec<StoredElement> {
    let mut pool: Vec<Option<Candidate>> = candidates.into_iter().map(Some).collect();
    registry
        .tracks()
        .filter_map(|track| {
            let slot = pool.iter_mut().find(|c| {
                c.as_ref()
                    .is_some_and(|c| c.observation == track.observation)
            })?;
            let c = slot.take()?;
            Some(StoredElement {
                target_id: track.target_id,
                revision: track.revision,
                role: track.observation.role,
                label: c.label,
                label_source: c.label_source,
                affordance: track.observation.affordance,
                value: c.value,
                text_node: c.text_node,
                evidence_at: c.evidence_at,
            })
        })
        .collect()
}

/// What fusion and tracking made of the evidence.
struct Tracked {
    registry: Registry,
    relations: Vec<(BBox, BBox)>,
    icons: Vec<IconLabel>,
    elements: Vec<StoredElement>,
    /// When fusion ended and tracking began.
    fused_at: Duration,
    changes: Changes,
}

/// Fuses `evidence`, labels the curated icons of the controls the `planned` work re-analyses (from
/// `frame`) and associates everything with the tracks of `previous`. A visual box in an
/// accessibility `gaps` region is not confirmed: the tree there may lack the control its
/// click point falls on, which a complete read would have shown.
fn fuse_and_track(
    evidence: &EvidenceSet,
    gaps: &[BBox],
    frame: &BgraFrame,
    planned: &Work,
    previous: Option<&Published>,
    engine: &Engine,
    clock: &dyn Clock,
) -> Option<Tracked> {
    let config = &engine.config;
    let fresh;
    let (base, kept_icons) = if let Some(p) = previous {
        (&p.registry, &p.icons[..])
    } else {
        // After a purge the counters of the snapshot it deleted carry on: no ID is issued twice.
        fresh = engine
            .purged_identity
            .clone()
            .unwrap_or_else(|| Registry::new(config.session_id.clone(), frame.geometry_epoch()));
        (&fresh, &[][..])
    };
    // Icons are resolved again where the detector looked again, and everywhere when there
    // is no base to keep labels from.
    let scope = match (planned, previous) {
        (Work::Nothing, Some(_)) => Scope::Regions(&[]),
        (Work::Regions(rs), Some(_)) => Scope::Regions(rs),
        _ => Scope::All,
    };
    let visual: Vec<Control> = evidence
        .controls
        .iter()
        .map(|c| {
            let mut c = c.clone();
            if c.affordance.actionability == Actionability::Confirmed
                && gaps.iter().any(|g| g.overlaps(&c.bbox))
            {
                c.affordance.actionability = Actionability::Unknown;
            }
            c
        })
        .collect();
    let accessible: Vec<AccessibleControl> = evidence
        .accessible
        .iter()
        .map(|a| a.element.clone())
        .collect();
    let reconciled = reconcile_accessibility(&visual, &accessible, config.fusion).ok()?;
    let fusion = fuse(&reconciled.controls, &evidence.words, config.fusion).ok()?;
    let origin_of = |i: usize| reconciled.origins[i].and_then(|k| evidence.accessible.get(k));
    let relations = fusion
        .controls
        .iter()
        .filter_map(|fc| match &fc.label {
            Some(LabelChoice::Label(word)) => Some((fc.control.bbox, word.bbox)),
            _ => None,
        })
        .collect();
    let icons = icons::resolve(
        &fusion.controls,
        frame,
        scope,
        kept_icons,
        config.icons.as_ref(),
    );
    let icon_of = |fc: &FusedControl| {
        icons
            .iter()
            .find(|i| i.bbox == fc.control.bbox && i.role == fc.control.role)
            .map(|i| i.label.as_str())
    };
    let candidates: Vec<Candidate> = fusion
        .controls
        .iter()
        .enumerate()
        .map(|(i, fc)| control_candidate(fc, origin_of(i), icon_of(fc)))
        .chain(fusion.text_nodes.iter().map(text_node_candidate))
        .collect();
    let fused_at = clock.now();
    let observations: Vec<Observation> = candidates.iter().map(|c| c.observation.clone()).collect();
    let (registry, changes) = base
        .observe(frame.geometry_epoch(), &observations, config.association)
        .ok()?;
    let elements = stored_elements(&registry, candidates);
    Some(Tracked {
        registry,
        relations,
        icons,
        elements,
        fused_at,
        changes,
    })
}

/// Fresh evidence of the planned regions and how long each perception stage took.
struct Inferred {
    evidence: EvidenceSet,
    /// Each stage's own duration. With concurrent stages they overlap, so they do not add up
    /// to the wall-clock time of the observation (`StageTimings::total`).
    detect: Duration,
    recognize: Duration,
    accessibility: Duration,
    /// The accessibility read ended on its budget.
    truncated: bool,
    /// The detector's inferences, `None` when it does not count them.
    inference_shapes: Option<Vec<InputShape>>,
}

/// What a perception stage returned, when it started and how long it ran on the injected
/// clock.
struct Timed<T> {
    result: Result<T, PortError>,
    started: Duration,
    took: Duration,
}

fn timed<T>(clock: &dyn Clock, stage: impl FnOnce() -> Result<T, PortError>) -> Timed<T> {
    let start = clock.now();
    let result = stage();
    Timed {
        result,
        started: start,
        took: clock.now().saturating_sub(start),
    }
}

/// The result of a stage that did not run: nothing, in no time.
fn idle<T: Default>() -> Timed<T> {
    Timed {
        result: Ok(T::default()),
        started: Duration::ZERO,
        took: Duration::ZERO,
    }
}

/// Waits for a stage thread. A panic of the stage goes on unwinding in the caller, exactly
/// as it would have when the stages ran one after the other.
fn joined<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// The planner's decision in the vocabulary of the research record and `_meta`.
fn plan_record(
    work: &Work,
    regions: &[BBox],
    inference_shapes: Option<Vec<InputShape>>,
    truncated: bool,
    evidence: &EvidenceSet,
    relations_n: usize,
) -> ObservationPlan {
    let (kind, reason) = match work {
        Work::Nothing => (WorkKind::Idle, None),
        Work::Regions(_) => (WorkKind::Regions, None),
        Work::FullRefresh(reason) => (WorkKind::Full, Some(*reason)),
    };
    ObservationPlan {
        work: kind,
        full_refresh_reason: reason,
        region_count: regions.len(),
        region_area_px: regions.iter().map(BBox::area).sum(),
        inference_count: inference_shapes.as_ref().map(Vec::len),
        input_shapes: inference_shapes,
        accessibility_truncated: truncated,
        controls_n: evidence.controls.len(),
        words_n: evidence.words.len(),
        supports_n: evidence.controls.len() + evidence.words.len() + evidence.accessible.len(),
        relations_n,
    }
}

/// `regions` and `extra` as one sorted list without duplicates.
fn union(regions: &[BBox], extra: &[BBox]) -> Vec<BBox> {
    let mut all: Vec<BBox> = regions.iter().chain(extra).copied().collect();
    all.sort_by(BBox::cmp_corners);
    all.dedup();
    all
}

fn inference_failed() -> ToolError {
    ToolError::new(
        ErrorCode::InferenceFailed,
        "detection or text recognition failed; nothing was published",
    )
}

impl Engine {
    /// Runs one observation and commits it, or reports why nothing was published.
    // One observation is one transaction (SDD §2.7): its stages read in order, and splitting
    // them would only move the clock readings that define each span away from their stage.
    #[allow(clippy::too_many_lines)]
    pub(super) fn observe(&mut self) -> Result<(), ToolError> {
        let clock = self.ports.clock.clone();
        let t0 = clock.now();
        // Any failed observation blocks actions on the evidence it could not refresh
        // (SDD §2.7); with no plan yet, no tile is known to be dirty.
        let Ok(captured) = self.ports.capturer.capture() else {
            self.store.record_failure(&BTreeSet::new());
            return Err(ToolError::new(
                ErrorCode::CaptureUnavailable,
                "the screen could not be captured",
            ));
        };
        let frame = captured.frame;
        let t_capture = clock.now();

        // Geometry epochs start at 1 on the wire: an epoch-0 frame cannot be published.
        if frame.geometry_epoch() == 0 {
            self.store.record_failure(&BTreeSet::new());
            return Err(ToolError::new(
                ErrorCode::UnsupportedGeometry,
                "the frame has no valid geometry epoch",
            ));
        }
        let plan = match self.plan(&frame) {
            Ok(plan) => plan,
            Err(error) => {
                self.store.record_failure(&BTreeSet::new());
                return Err(error);
            }
        };
        let t_plan = clock.now();

        let regions: Vec<BBox> = match &plan.work {
            Work::Nothing => Vec::new(),
            Work::Regions(rs) => rs.clone(),
            Work::FullRefresh(_) => vec![frame.bounds()],
        };
        let bounds = frame.bounds();
        let read_regions = self.accessibility_regions(&plan.work, &regions, &bounds, t0);
        let Ok(Inferred {
            evidence: fresh,
            detect,
            recognize,
            accessibility,
            truncated,
            inference_shapes,
        }) = self.infer(&frame, &regions, &read_regions)
        else {
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        };
        let t_access = clock.now();

        let previous = self.store.published();
        let evidence = match (&plan.work, previous) {
            (Work::FullRefresh(_), _) | (_, None) => fresh,
            (_, Some(p)) => p.evidence.merged(&regions, &read_regions, fresh, truncated),
        }
        .normalised(&bounds);
        let accessibility_gaps = if truncated { read_regions } else { Vec::new() };
        let tracked = fuse_and_track(
            &evidence,
            &accessibility_gaps,
            &frame,
            &plan.work,
            previous,
            self,
            clock.as_ref(),
        );
        let t_track = clock.now();
        let Some(tracked) = tracked else {
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        };

        let span = |a: Duration, b: Duration| b.saturating_sub(a);
        // Counted while the evidence is still here: it moves into the candidate snapshot.
        let observed_plan = plan_record(
            &plan.work,
            &regions,
            inference_shapes,
            truncated,
            &evidence,
            tracked.relations.len(),
        );
        let staged = self.store.stage(
            frame,
            Published {
                registry: tracked.registry,
                evidence,
                relations: tracked.relations,
                icons: tracked.icons,
                frame_seq: captured.frame_seq,
                captured_at: captured.captured_at,
                elements: tracked.elements,
                accessibility_gaps,
                changes: tracked.changes,
            },
        );
        if self.store.commit(staged).is_err() {
            // Unreachable with a single writer; fail closed all the same.
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        }
        // The publication is the last stage: `total` ends here (ADR-0024).
        let t_commit = clock.now();
        self.snapshot_spent = false;
        self.measured = Some(Measured {
            timings: StageTimings {
                capture: span(t0, t_capture),
                plan: span(t_capture, t_plan),
                detect,
                recognize,
                accessibility,
                fuse: span(t_access, tracked.fused_at),
                track: span(tracked.fused_at, t_track),
                commit: span(t_track, t_commit),
                total: span(t0, t_commit),
                response_build: None,
            },
            plan: observed_plan,
        });
        Ok(())
    }

    /// The regions the accessibility provider reads: the plan, the gaps where an earlier read
    /// was cut short, and the regions whose evidence has expired by `now` (ADR-0025), whether
    /// or not pixels changed there. A publication never renews an old read, so this is how a
    /// new observation does. The detector and the recogniser see the plan only. A full
    /// refresh reads everything anyway.
    fn accessibility_regions(
        &self,
        work: &Work,
        planned: &[BBox],
        bounds: &BBox,
        now: Duration,
    ) -> Vec<BBox> {
        if matches!(work, Work::FullRefresh(_)) {
            return planned.to_vec();
        }
        let max = self.config.max_snapshot_age;
        let (gaps, expired) = self.store.published().map_or_else(
            || (Vec::new(), Vec::new()),
            |p| {
                let gaps: Vec<BBox> = p
                    .accessibility_gaps
                    .iter()
                    .filter_map(|g| g.clip(bounds))
                    .collect();
                let expired: Vec<BBox> = p
                    .evidence
                    .accessible
                    .iter()
                    .filter(|a| aged_out(now, a.read_at, max))
                    .filter_map(|a| a.element.control.bbox.clip(bounds))
                    .collect();
                (gaps, expired)
            },
        );
        union(&union(planned, &gaps), &expired)
    }

    /// Algorithm 1 against the committed base, the pending tiles and the last evidence.
    fn plan(&self, frame: &BgraFrame) -> Result<RegionPlan, ToolError> {
        let previous = self.store.published();
        let supports = previous.map(|p| p.evidence.supports()).unwrap_or_default();
        let relations = previous.map_or(&[][..], |p| &p.relations[..]);
        plan_regions(
            self.store.base(),
            frame,
            self.store.pending_dirty(),
            Evidence {
                supports: &supports,
                relations,
            },
            &self.config.plan,
        )
        .map_err(|_| {
            ToolError::new(
                ErrorCode::UnsupportedGeometry,
                "the frame cannot be planned with the configured tiles",
            )
        })
    }

    /// Detects and recognises inside `regions` (the plan) and reads the accessibility tree
    /// inside `read_regions` (the plan and the gaps of an earlier truncated read); no
    /// region, no inference. Accessibility elements outside `read_regions` or off screen
    /// are dropped, whatever the provider returned.
    ///
    /// The three stages read the same frame and do not depend on each other, so they run
    /// concurrently (SDD §2.7, ADR-0019) unless `concurrent_stages` is off: each on its own
    /// thread, joined here, where the single writer fuses what they found. Their results
    /// are collected by stage, never by arrival, so the evidence is the same in both modes.
    /// When one stage fails the others are still waited for, and the failure reported is
    /// the first in the order detect, recognise, accessibility.
    fn infer(
        &mut self,
        frame: &BgraFrame,
        regions: &[BBox],
        read_regions: &[BBox],
    ) -> Result<Inferred, PortError> {
        let concurrent = self.config.concurrent_stages;
        let Ports {
            detector,
            recognizer,
            accessibility,
            clock,
            ..
        } = &mut self.ports;
        let clock: &dyn Clock = clock.as_ref();
        // The shapes are read on the thread that made the call, right after it: they are the
        // inferences of that call. With nothing to detect no call is made, so no inference ran.
        let detect = |detector: &mut Box<dyn VisionDetector>| {
            if regions.is_empty() {
                (idle(), Some(Vec::new()))
            } else {
                let stage = timed(clock, || detector.detect(frame, regions));
                (stage, detector.inference_shapes())
            }
        };
        let recognize = |recognizer: &mut Box<dyn TextRecognizer>| {
            if regions.is_empty() {
                idle()
            } else {
                timed(clock, || recognizer.recognize(frame, regions))
            }
        };
        let read = |provider: &mut Box<dyn AccessibilityProvider>| {
            if read_regions.is_empty() {
                idle()
            } else {
                timed(clock, || provider.snapshot(read_regions))
            }
        };

        // With no region to analyse only the accessibility read has work: nothing to overlap.
        let (detected, words, tree) = if concurrent && !regions.is_empty() {
            std::thread::scope(|scope| {
                let controls = scope.spawn(|| detect(detector));
                let words = scope.spawn(|| recognize(recognizer));
                let tree = read(accessibility);
                (joined(controls), joined(words), tree)
            })
        } else {
            // One after the other, and a failure ends the observation at once.
            let detected = detect(detector);
            let words = if detected.0.result.is_ok() {
                recognize(recognizer)
            } else {
                idle()
            };
            let tree = if detected.0.result.is_ok() && words.result.is_ok() {
                read(accessibility)
            } else {
                idle()
            };
            (detected, words, tree)
        };

        let (controls, inference_shapes) = detected;
        let (detect_took, recognize_took, access_took) = (controls.took, words.took, tree.took);
        // Evidence is dated by when its read started: the state it reports is no older
        // than that, so the age it is judged by is never understated (ADR-0025).
        let read_at = tree.started;
        let controls = controls.result?;
        let words = words.result?;
        let read = tree.result?;
        let accessible = read
            .elements
            .iter()
            .filter(|e| read_regions.iter().any(|r| r.overlaps(&e.bbox)))
            .filter_map(accessible_control)
            .map(|element| ReadAccessible { element, read_at })
            .collect();
        Ok(Inferred {
            evidence: EvidenceSet {
                controls,
                words,
                accessible,
            },
            detect: detect_took,
            recognize: recognize_took,
            accessibility: access_took,
            truncated: read.truncated,
            inference_shapes,
        })
    }
}
