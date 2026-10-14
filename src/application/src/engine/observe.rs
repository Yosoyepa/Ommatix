//! One transactional observation (SDD §2.7, thesis chapter 04 §4.2).
//!
//! The candidate snapshot is built aside from the published one and committed in one
//! step. Every stage after the capture takes the region plan as input: only the planned
//! regions reach the detector and the recogniser, and evidence outside them is kept. A
//! failed stage publishes nothing, keeps the planned tiles pending and blocks actions.

use std::collections::BTreeSet;
use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{
    Affordance, Control, FusedControl, LabelChoice, Role, TextKind, Word, fuse,
};
use ommatix_domain::identity::{Observation, Registry, TargetId};
use ommatix_domain::incremental::{BgraFrame, Evidence, RegionPlan, Work, plan_regions};

use super::{Engine, EngineConfig};
use crate::PortError;
use crate::contract::{ErrorCode, LabelSource, StageTimings, ToolError};
use crate::ports::Clock;

/// Raw evidence of the last committed observation, clipped to its frame and sorted.
#[derive(Debug, Clone, Default)]
pub(super) struct EvidenceSet {
    pub controls: Vec<Control>,
    pub words: Vec<Word>,
}

impl EvidenceSet {
    /// Boxes a region must not cut in half.
    fn supports(&self) -> Vec<BBox> {
        self.controls
            .iter()
            .map(|c| c.bbox)
            .chain(self.words.iter().map(|w| w.bbox))
            .collect()
    }

    /// Keeps the evidence outside `regions` and adds the fresh evidence found inside them.
    fn replace_in(&self, regions: &[BBox], fresh: Self) -> Self {
        let outside = |b: &BBox| !regions.iter().any(|r| r.overlaps(b));
        let mut next = Self {
            controls: self
                .controls
                .iter()
                .filter(|c| outside(&c.bbox))
                .cloned()
                .collect(),
            words: self
                .words
                .iter()
                .filter(|w| outside(&w.bbox))
                .cloned()
                .collect(),
        };
        next.controls.extend(fresh.controls);
        next.words.extend(fresh.words);
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
    pub frame_seq: u64,
    pub captured_at: Duration,
    /// Sorted by target ID.
    pub elements: Vec<StoredElement>,
    pub timings: StageTimings,
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

fn control_candidate(fc: &FusedControl) -> Candidate {
    let role = fc.control.role;
    let inner = fc.inner_text();
    let (label, label_source) = if role.takes_external_label() {
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
        value: role
            .holds_value()
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
    elements: Vec<StoredElement>,
    /// When fusion ended and tracking began.
    fused_at: Duration,
}

/// Fuses `evidence` and associates it with the tracks of `previous`.
fn fuse_and_track(
    evidence: &EvidenceSet,
    previous: &Registry,
    geometry_epoch: u64,
    config: &EngineConfig,
    clock: &dyn Clock,
) -> Option<Tracked> {
    let fusion = fuse(&evidence.controls, &evidence.words, config.fusion).ok()?;
    let relations = fusion
        .controls
        .iter()
        .filter_map(|fc| match &fc.label {
            Some(LabelChoice::Label(word)) => Some((fc.control.bbox, word.bbox)),
            _ => None,
        })
        .collect();
    let candidates: Vec<Candidate> = fusion
        .controls
        .iter()
        .map(control_candidate)
        .chain(fusion.text_nodes.iter().map(text_node_candidate))
        .collect();
    let fused_at = clock.now();
    let observations: Vec<Observation> = candidates.iter().map(|c| c.observation.clone()).collect();
    let (registry, _changes) = previous
        .observe(geometry_epoch, &observations, config.association)
        .ok()?;
    let elements = stored_elements(&registry, candidates);
    Some(Tracked {
        registry,
        relations,
        elements,
        fused_at,
    })
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
        let Ok((fresh, t_detect)) = self.infer(&frame, &regions) else {
            self.store.record_failure(&plan.dirty);
            return Err(inference_failed());
        };
        let t_recognize = clock.now();

        let previous = self.store.published();
        let evidence = match (&plan.work, previous) {
            (Work::Nothing, Some(p)) => p.evidence.clone(),
            (Work::Regions(rs), Some(p)) => p.evidence.replace_in(rs, fresh),
            _ => fresh,
        }
        .normalised(&frame.bounds());
        let fresh_registry;
        let registry = if let Some(p) = previous {
            &p.registry
        } else {
            fresh_registry = Registry::new(self.config.session_id.clone(), frame.geometry_epoch());
            &fresh_registry
        };
        let tracked = fuse_and_track(
            &evidence,
            registry,
            frame.geometry_epoch(),
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
            fuse: span(t_recognize, tracked.fused_at),
            track: span(tracked.fused_at, t_track),
            total: span(t0, t_track),
        };
        let staged = self.store.stage(
            frame,
            Published {
                registry: tracked.registry,
                evidence,
                relations: tracked.relations,
                frame_seq: captured.frame_seq,
                captured_at: captured.captured_at,
                elements: tracked.elements,
                timings,
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

    /// Detects and recognises inside `regions` only; no region, no inference. Returns the
    /// evidence and the instant detection ended.
    fn infer(
        &mut self,
        frame: &BgraFrame,
        regions: &[BBox],
    ) -> Result<(EvidenceSet, Duration), PortError> {
        if regions.is_empty() {
            return Ok((EvidenceSet::default(), self.ports.clock.now()));
        }
        let controls = self.ports.detector.detect(frame, regions)?;
        let t_detect = self.ports.clock.now();
        let words = self.ports.recognizer.recognize(frame, regions)?;
        Ok((EvidenceSet { controls, words }, t_detect))
    }
}
