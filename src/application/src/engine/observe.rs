//! One transactional observation (SDD §2.7, thesis chapter 04 §4.2).
//!
//! The candidate snapshot is built aside from the published one and committed in one
//! step. Every stage after the capture takes the region plan as input: only the planned
//! regions reach the detector, the recogniser and the accessibility provider, and
//! evidence outside them is kept. A failed stage publishes nothing, keeps the planned
//! tiles pending and blocks actions.
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

use std::collections::BTreeSet;
use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{
    AccessibleControl, Actionability, Affordance, Control, FusedControl, LabelChoice, Role,
    TextKind, Word, fuse, reconcile_accessibility,
};
use ommatix_domain::identity::{Observation, Registry, TargetId};
use ommatix_domain::incremental::{BgraFrame, Evidence, RegionPlan, Work, plan_regions};

use super::accessibility::accessible_control;
use super::icons::{self, IconLabel, Scope};
use super::{Engine, EngineConfig};
use crate::PortError;
use crate::contract::{ErrorCode, LabelSource, StageTimings, ToolError};
use crate::ports::Clock;

/// Raw evidence of the last committed observation, clipped to its frame and sorted.
#[derive(Debug, Clone, Default)]
pub(super) struct EvidenceSet {
    pub controls: Vec<Control>,
    pub words: Vec<Word>,
    /// Accessibility elements on screen (ADR-0017).
    pub accessible: Vec<AccessibleControl>,
}

impl EvidenceSet {
    /// Boxes a region must not cut in half.
    fn supports(&self) -> Vec<BBox> {
        self.controls
            .iter()
            .map(|c| c.bbox)
            .chain(self.words.iter().map(|w| w.bbox))
            .chain(self.accessible.iter().map(|a| a.control.bbox))
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
        let keeps = |a: &AccessibleControl| {
            let b = &a.control.bbox;
            outside(read, b)
                || (truncated
                    && outside(detected, b)
                    && !fresh.accessible.iter().any(|f| f.control.bbox == *b))
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
                let bbox = a.control.bbox.clip(bounds)?;
                Some(AccessibleControl {
                    control: Control { bbox, ..a.control },
                    ..a
                })
            })
            .collect();
        self.accessible.sort_by(|a, b| {
            let (x, y) = (&a.control, &b.control);
            x.bbox
                .cmp_corners(&y.bbox)
                .then(x.role.cmp(&y.role))
                .then_with(|| a.label.cmp(&b.label))
                .then(x.affordance.actionability.cmp(&y.affordance.actionability))
                .then(x.affordance.enabled.cmp(&y.affordance.enabled))
                .then(x.affordance.visible.cmp(&y.affordance.visible))
                .then(a.sensitive.cmp(&b.sensitive))
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
    pub timings: StageTimings,
    /// Regions whose accessibility read was cut short by its budget and not completed
    /// since: the next observation reads them again, pixels changed or not. Empty when
    /// the evidence is complete.
    pub accessibility_gaps: Vec<BBox>,
}

impl Published {
    /// `s<n>` on the wire.
    pub fn snapshot_id(&self) -> String {
        format!("s{}", self.registry.snapshot_id())
    }
}

/// An observation before identity: what the registry sees, plus the view's extras.
struct Candidate {
    observation: Observation,
    label: String,
    label_source: LabelSource,
    value: Option<(String, TextKind)>,
    text_node: bool,
}

/// `access` is the accessibility element this control comes from, if any: its name wins
/// over every other label, and a password field publishes no OCR text at all. `icon` is
/// the curated label Algorithm 3A found for this control, if any; it only replaces the
/// visual label, never the affordance.
fn control_candidate(
    fc: &FusedControl,
    access: Option<&AccessibleControl>,
    icon: Option<&str>,
) -> Candidate {
    let role = fc.control.role;
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
            discriminant: None,
            affordance: fc.control.affordance,
        },
        label,
        label_source,
        value: (role.holds_value() && !sensitive)
            .then(|| (inner, fc.text_kind.unwrap_or(TextKind::Unknown))),
        text_node: false,
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
    config: &EngineConfig,
    clock: &dyn Clock,
) -> Option<Tracked> {
    let fresh;
    let (base, kept_icons) = if let Some(p) = previous {
        (&p.registry, &p.icons[..])
    } else {
        fresh = Registry::new(config.session_id.clone(), frame.geometry_epoch());
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
    let reconciled = reconcile_accessibility(&visual, &evidence.accessible, config.fusion).ok()?;
    let fusion = fuse(&reconciled.controls, &evidence.words, config.fusion).ok()?;
    let access_of = |i: usize| reconciled.origins[i].and_then(|k| evidence.accessible.get(k));
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
        .map(|(i, fc)| control_candidate(fc, access_of(i), icon_of(fc)))
        .chain(fusion.text_nodes.iter().map(text_node_candidate))
        .collect();
    let fused_at = clock.now();
    let observations: Vec<Observation> = candidates.iter().map(|c| c.observation.clone()).collect();
    let (registry, _changes) = base
        .observe(frame.geometry_epoch(), &observations, config.association)
        .ok()?;
    let elements = stored_elements(&registry, candidates);
    Some(Tracked {
        registry,
        relations,
        icons,
        elements,
        fused_at,
    })
}

/// Fresh evidence of the planned regions and when detection and recognition ended.
struct Inferred {
    evidence: EvidenceSet,
    t_detect: Duration,
    t_recognize: Duration,
    /// The accessibility read ended on its budget.
    truncated: bool,
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
        let read_regions = self.accessibility_regions(&plan.work, &regions, &bounds);
        let Ok(Inferred {
            evidence: fresh,
            t_detect,
            t_recognize,
            truncated,
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
            &self.config,
            clock.as_ref(),
        );
        let t_track = clock.now();
        let Some(tracked) = tracked else {
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        };

        let span = |a: Duration, b: Duration| b.saturating_sub(a);
        let timings = StageTimings {
            capture: span(t0, t_capture),
            plan: span(t_capture, t_plan),
            detect: span(t_plan, t_detect),
            recognize: span(t_detect, t_recognize),
            accessibility: span(t_recognize, t_access),
            fuse: span(t_access, tracked.fused_at),
            track: span(tracked.fused_at, t_track),
            total: span(t0, t_track),
        };
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
                timings,
                accessibility_gaps,
            },
        );
        if self.store.commit(staged).is_err() {
            // Unreachable with a single writer; fail closed all the same.
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        }
        self.snapshot_spent = false;
        Ok(())
    }

    /// The regions the accessibility provider reads: the plan, and also where an earlier
    /// read was cut short, whether or not pixels changed there. The detector and the
    /// recogniser see the plan only. A full refresh reads everything anyway.
    fn accessibility_regions(&self, work: &Work, planned: &[BBox], bounds: &BBox) -> Vec<BBox> {
        if matches!(work, Work::FullRefresh(_)) {
            return planned.to_vec();
        }
        let gaps: Vec<BBox> = self
            .store
            .published()
            .map(|p| {
                p.accessibility_gaps
                    .iter()
                    .filter_map(|g| g.clip(bounds))
                    .collect()
            })
            .unwrap_or_default();
        union(planned, &gaps)
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
    fn infer(
        &mut self,
        frame: &BgraFrame,
        regions: &[BBox],
        read_regions: &[BBox],
    ) -> Result<Inferred, PortError> {
        let mut controls = Vec::new();
        let mut words = Vec::new();
        let mut t_detect = self.ports.clock.now();
        let mut t_recognize = t_detect;
        if !regions.is_empty() {
            controls = self.ports.detector.detect(frame, regions)?;
            t_detect = self.ports.clock.now();
            words = self.ports.recognizer.recognize(frame, regions)?;
            t_recognize = self.ports.clock.now();
        }
        let mut accessible = Vec::new();
        let mut truncated = false;
        if !read_regions.is_empty() {
            let read = self.ports.accessibility.snapshot(read_regions)?;
            truncated = read.truncated;
            accessible = read
                .elements
                .iter()
                .filter(|e| read_regions.iter().any(|r| r.overlaps(&e.bbox)))
                .filter_map(accessible_control)
                .collect();
        }
        Ok(Inferred {
            evidence: EvidenceSet {
                controls,
                words,
                accessible,
            },
            t_detect,
            t_recognize,
            truncated,
        })
    }
}
