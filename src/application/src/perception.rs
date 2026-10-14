use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use ommatix_domain::{
    ChangeTracker, Detection, DomainError, Element, ElementId, Frame, GridSpec, Point, Rect, Scene,
    SceneDelta,
};

use crate::{AppError, Clock, ElementDetector, FrameSource, ScreenModel};

/// Tunables of the perception pipeline. Validated by [`PerceptionEngine::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerceptionConfig {
    /// Edge of each grid cell ("ommatidium") in pixels. Must be at least 1.
    pub cell_size: u32,
    /// Edge of each spatial-index bucket in pixels. Must be at least 1.
    pub index_bucket_size: u32,
    /// Minimum IoU, in `(0, 1]`, for a re-detected element to keep its id.
    pub identity_iou: f32,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            cell_size: 32,
            index_bucket_size: 64,
            identity_iou: 0.5,
        }
    }
}

impl PerceptionConfig {
    fn grid(&self) -> Result<GridSpec, DomainError> {
        GridSpec::new(self.cell_size)
    }

    fn scene(&self) -> Result<Scene, DomainError> {
        let bucket = NonZeroU32::new(self.index_bucket_size).ok_or(DomainError::ZeroBucketSize)?;
        Scene::new(bucket, self.identity_iou)
    }
}

/// Cost of each pipeline stage, as measured by the injected [`Clock`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timings {
    pub capture: Duration,
    pub diff: Duration,
    pub detect: Duration,
    pub total: Duration,
}

/// Outcome of one perception tick.
#[derive(Debug, Clone, PartialEq)]
pub struct PerceptionReport {
    pub frame_no: u64,
    pub screen: Rect,
    pub total_cells: usize,
    pub dirty_cells: usize,
    /// First frame, or the screen size changed: the scene was rebuilt from scratch and
    /// every previous element is reported as removed.
    pub geometry_changed: bool,
    /// Regions actually re-analysed: dirty cells merged and grown to whole elements.
    pub analysed_regions: Vec<Rect>,
    /// Pixels inside `analysed_regions`, i.e. the detector's workload.
    pub analysed_px: u64,
    pub delta: SceneDelta,
    pub element_count: usize,
    pub timings: Timings,
}

/// The perception use case: capture, diff, detect only where something changed, and
/// keep a spatially indexed scene that agents can query with exact coordinates.
pub struct PerceptionEngine {
    source: Box<dyn FrameSource>,
    detector: Box<dyn ElementDetector>,
    clock: Arc<dyn Clock>,
    config: PerceptionConfig,
    tracker: ChangeTracker,
    scene: Scene,
    frame_no: u64,
    screen: Option<Rect>,
}

impl std::fmt::Debug for PerceptionEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerceptionEngine")
            .field("source", &self.source.name())
            .field("detector", &self.detector.name())
            .field("config", &self.config)
            .field("frame_no", &self.frame_no)
            .field("elements", &self.scene.len())
            .finish_non_exhaustive()
    }
}

impl PerceptionEngine {
    /// All collaborators are injected; `config` is validated here.
    pub fn new(
        source: Box<dyn FrameSource>,
        detector: Box<dyn ElementDetector>,
        clock: Arc<dyn Clock>,
        config: PerceptionConfig,
    ) -> Result<Self, AppError> {
        Ok(Self {
            source,
            detector,
            clock,
            tracker: ChangeTracker::new(config.grid()?),
            scene: config.scene()?,
            config,
            frame_no: 0,
            screen: None,
        })
    }

    pub fn config(&self) -> &PerceptionConfig {
        &self.config
    }

    pub fn source_name(&self) -> &'static str {
        self.source.name()
    }

    pub fn detector_name(&self) -> &'static str {
        self.detector.name()
    }

    /// Number of the last processed frame (0 before the first tick).
    pub fn frame_no(&self) -> u64 {
        self.frame_no
    }

    /// Captures and processes one frame. Returns `None` when the source is exhausted.
    ///
    /// The tick is atomic: if detection fails, neither the scene nor the change tracker
    /// moves, and the same change is processed again on the next tick.
    pub fn tick(&mut self) -> Result<Option<PerceptionReport>, AppError> {
        let start = self.clock.now();
        let Some(frame) = self.source.next_frame().map_err(AppError::Capture)? else {
            return Ok(None);
        };
        let captured = self.clock.now();

        let pending = self.tracker.compare(&frame);
        let screen = frame.bounds();
        let geometry_changed = pending.change().geometry_changed;
        let regions: Vec<Rect> = if geometry_changed {
            vec![screen]
        } else {
            self.scene
                .expand_regions(&pending.change().regions)
                .into_iter()
                .map(|r| r.clip(&screen))
                .filter(|r| !r.is_empty())
                .collect()
        };
        let diffed = self.clock.now();

        let detections = self.detect(&frame, &regions)?;
        let frame_no = self.frame_no + 1;
        let mut delta = SceneDelta::default();
        if geometry_changed {
            delta.removed = self.scene.clear();
        }
        let delta = delta.then(self.scene.apply(frame_no, &regions, &detections));
        let change = self.tracker.commit(pending);
        let detected = self.clock.now();

        self.frame_no = frame_no;
        self.screen = Some(screen);
        Ok(Some(PerceptionReport {
            frame_no,
            screen,
            total_cells: change.total_cells,
            dirty_cells: change.dirty_cells,
            geometry_changed,
            analysed_px: regions.iter().map(Rect::area).sum(),
            analysed_regions: regions,
            delta,
            element_count: self.scene.len(),
            timings: Timings {
                capture: captured.saturating_sub(start),
                diff: diffed.saturating_sub(captured),
                detect: detected.saturating_sub(diffed),
                total: detected.saturating_sub(start),
            },
        }))
    }

    /// Runs the detector on every region and keeps only what lies inside each region.
    fn detect(&self, frame: &Frame, regions: &[Rect]) -> Result<Vec<Detection>, AppError> {
        let mut detections = Vec::new();
        for region in regions {
            let found = self
                .detector
                .detect(frame, *region)
                .map_err(AppError::Detection)?;
            detections.extend(found.into_iter().filter_map(|d| {
                let bounds = d.bounds.clip(region);
                (!bounds.is_empty()).then_some(Detection { bounds, ..d })
            }));
        }
        Ok(detections)
    }

    /// Bounds of the last processed frame, `None` before the first tick.
    pub fn screen_bounds(&self) -> Option<Rect> {
        self.screen
    }

    pub fn element(&self, id: ElementId) -> Option<Element> {
        self.scene.get(id).copied()
    }

    /// All known elements in reading order.
    pub fn elements(&self) -> Vec<Element> {
        self.scene.elements()
    }

    /// Elements under a screen coordinate, most specific first.
    pub fn elements_at(&self, p: Point) -> Vec<Element> {
        self.scene.elements_at(p)
    }

    /// Elements intersecting a rectangle, in reading order.
    pub fn elements_in(&self, rect: &Rect) -> Vec<Element> {
        self.scene.elements_in(rect)
    }
}

impl ScreenModel for PerceptionEngine {
    fn screen_bounds(&self) -> Option<Rect> {
        PerceptionEngine::screen_bounds(self)
    }

    fn element(&self, id: ElementId) -> Option<Element> {
        PerceptionEngine::element(self, id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};

    use ommatix_domain::ElementKind;

    use super::*;
    use crate::{ManualClock, PortError};

    struct Script(VecDeque<Frame>);

    impl FrameSource for Script {
        fn name(&self) -> &'static str {
            "script"
        }
        fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
            Ok(self.0.pop_front())
        }
    }

    /// Test double: one blob per region, bounding every non-white pixel. Can be told to
    /// fail, and can report a box larger than the region, like a sloppy detector would.
    #[derive(Default)]
    struct BoundingBoxDetector {
        fail: Arc<AtomicBool>,
        overshoot: u32,
    }

    impl ElementDetector for BoundingBoxDetector {
        fn name(&self) -> &'static str {
            "bbox"
        }
        fn detect(&self, frame: &Frame, region: Rect) -> Result<Vec<Detection>, PortError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(PortError::new("bbox", "model unavailable"));
            }
            let mut acc: Option<Rect> = None;
            for y in region.y..region.bottom() {
                for x in region.x..region.right() {
                    if frame.pixel(x, y) != WHITE {
                        let px = Rect::new(x, y, 1, 1);
                        acc = Some(acc.map_or(px, |a| a.union(&px)));
                    }
                }
            }
            Ok(acc
                .map(|bounds| Detection {
                    bounds: bounds.inflate(self.overshoot),
                    kind: ElementKind::Blob,
                })
                .into_iter()
                .collect())
        }
    }

    const WHITE: [u8; 4] = [255; 4];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn config() -> PerceptionConfig {
        PerceptionConfig {
            cell_size: 16,
            index_bucket_size: 32,
            identity_iou: 0.5,
        }
    }

    fn engine_with(frames: Vec<Frame>, detector: BoundingBoxDetector) -> PerceptionEngine {
        PerceptionEngine::new(
            Box::new(Script(frames.into())),
            Box::new(detector),
            Arc::new(ManualClock::new()),
            config(),
        )
        .unwrap()
    }

    fn engine(frames: Vec<Frame>) -> PerceptionEngine {
        engine_with(frames, BoundingBoxDetector::default())
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let new = |config| {
            PerceptionEngine::new(
                Box::new(Script(VecDeque::new())),
                Box::new(BoundingBoxDetector::default()),
                Arc::new(ManualClock::new()),
                config,
            )
        };
        for bad in [
            PerceptionConfig {
                cell_size: 0,
                ..config()
            },
            PerceptionConfig {
                index_bucket_size: 0,
                ..config()
            },
            PerceptionConfig {
                identity_iou: 0.0,
                ..config()
            },
        ] {
            assert!(matches!(new(bad), Err(AppError::Config(_))), "{bad:?}");
        }
    }

    #[test]
    fn static_screen_costs_nothing_after_first_frame() {
        let mut f = Frame::solid(64, 64, WHITE).unwrap();
        f.fill_rect(Rect::new(4, 4, 8, 8), BLACK);
        let mut e = engine(vec![f.clone(), f]);

        let first = e.tick().unwrap().unwrap();
        assert!(first.geometry_changed);
        assert_eq!(first.dirty_cells, 16);
        assert_eq!(first.analysed_px, 64 * 64);
        assert_eq!(first.delta.added.len(), 1);

        let second = e.tick().unwrap().unwrap();
        assert_eq!(second.dirty_cells, 0);
        assert_eq!(second.analysed_regions.len(), 0);
        assert_eq!(second.analysed_px, 0);
        assert!(second.delta.is_empty());
        assert_eq!(second.element_count, 1);
        assert_eq!(second.timings, Timings::default(), "virtual clock");
        assert!(e.tick().unwrap().is_none(), "source exhausted");
    }

    #[test]
    fn only_changed_cells_are_reprocessed_and_queries_reflect_them() {
        let base = Frame::solid(64, 64, WHITE).unwrap();
        let mut changed = base.clone();
        changed.fill_rect(Rect::new(36, 36, 8, 8), [255, 0, 0, 255]);
        let mut e = engine(vec![base, changed]);

        e.tick().unwrap();
        let r = e.tick().unwrap().unwrap();
        assert_eq!(r.dirty_cells, 1);
        assert_eq!(r.analysed_regions, vec![Rect::new(32, 32, 16, 16)]);
        assert_eq!(r.analysed_px, 256);
        assert_eq!(r.delta.added[0].bounds, Rect::new(36, 36, 8, 8));
        assert_eq!(e.elements_at(Point::new(40, 40)).len(), 1);
    }

    #[test]
    fn a_resize_removes_every_previous_element_even_with_the_same_grid() {
        let mut a = Frame::solid(64, 32, WHITE).unwrap();
        a.fill_rect(Rect::new(52, 4, 6, 6), BLACK);
        let b = Frame::solid(60, 32, WHITE).unwrap(); // same 4x2 grid, ghost would be off-screen
        let mut e = engine(vec![a, b]);

        let old = e.tick().unwrap().unwrap().delta.added[0].id;
        let r = e.tick().unwrap().unwrap();
        assert!(r.geometry_changed);
        assert_eq!(r.dirty_cells, r.total_cells);
        assert_eq!(r.delta.removed, vec![old]);
        assert_eq!(r.element_count, 0);
        assert_eq!(e.elements_at(Point::new(54, 6)).len(), 0);
        assert_eq!(e.screen_bounds(), Some(Rect::new(0, 0, 60, 32)));
    }

    #[test]
    fn detections_never_leave_their_region() {
        let base = Frame::solid(64, 64, WHITE).unwrap();
        let mut changed = base.clone();
        changed.fill_rect(Rect::new(36, 36, 8, 8), BLACK);
        let sloppy = BoundingBoxDetector {
            overshoot: 20,
            ..BoundingBoxDetector::default()
        };
        let mut e = engine_with(vec![base, changed], sloppy);
        e.tick().unwrap();
        let r = e.tick().unwrap().unwrap();
        assert_eq!(r.delta.added[0].bounds, Rect::new(32, 32, 16, 16));
    }

    #[test]
    fn a_failed_detection_changes_nothing_and_is_retried() {
        let base = Frame::solid(64, 64, WHITE).unwrap();
        let mut changed = base.clone();
        changed.fill_rect(Rect::new(36, 36, 8, 8), BLACK);
        let detector = BoundingBoxDetector::default();
        let fail = Arc::clone(&detector.fail);
        let mut e = engine_with(vec![base, changed.clone(), changed], detector);
        e.tick().unwrap();

        fail.store(true, Ordering::SeqCst);
        let err = e.tick().unwrap_err();
        assert!(matches!(err, AppError::Detection(_)), "{err}");
        assert_eq!(e.frame_no(), 1);
        assert_eq!(e.elements().len(), 0);

        fail.store(false, Ordering::SeqCst);
        let r = e.tick().unwrap().unwrap();
        assert_eq!(r.frame_no, 2);
        assert_eq!(r.dirty_cells, 1, "the lost change is processed now");
        assert_eq!(r.delta.added.len(), 1);
    }
}
