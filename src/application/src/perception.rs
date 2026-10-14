use std::time::Instant;

use ommatix_domain::Element;
use ommatix_domain::{GridSignature, GridSpec, Point, Rect, Scene, SceneDelta, merge_dirty_cells};
use serde::Serialize;

use crate::{AppError, ElementDetector, FrameSource};

/// Tunables of the perception pipeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerceptionConfig {
    /// Edge of each grid cell ("ommatidium") in pixels.
    pub cell_size: u32,
    /// Edge of each spatial-index bucket in pixels.
    pub index_bucket_size: u32,
    /// Minimum IoU for a re-detected element to keep its id.
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

/// Wall-clock cost of each pipeline stage, in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Timings {
    pub capture_us: u64,
    pub diff_us: u64,
    pub detect_us: u64,
    pub total_us: u64,
}

/// Outcome of one perception tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PerceptionReport {
    pub frame_no: u64,
    pub width: u32,
    pub height: u32,
    pub total_cells: usize,
    pub dirty_cells: usize,
    /// Regions actually re-examined (dirty cells merged and expanded to whole elements).
    pub dirty_regions: Vec<Rect>,
    pub delta: SceneDelta,
    pub element_count: usize,
    pub timings: Timings,
}

/// The perception use case: capture, diff, detect only where something changed, and
/// keep a spatially indexed scene that agents can query with exact coordinates.
pub struct PerceptionEngine {
    source: Box<dyn FrameSource>,
    detector: Box<dyn ElementDetector>,
    config: PerceptionConfig,
    grid: GridSpec,
    previous: Option<GridSignature>,
    scene: Scene,
    frame_no: u64,
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
    pub fn new(
        source: Box<dyn FrameSource>,
        detector: Box<dyn ElementDetector>,
        config: PerceptionConfig,
    ) -> Result<Self, AppError> {
        Ok(Self {
            source,
            detector,
            grid: GridSpec::new(config.cell_size)?,
            previous: None,
            scene: Scene::new(config.index_bucket_size, config.identity_iou),
            frame_no: 0,
            config,
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

    pub fn frame_no(&self) -> u64 {
        self.frame_no
    }

    /// Captures and processes one frame. Returns `None` when the source is exhausted.
    pub fn tick(&mut self) -> Result<Option<PerceptionReport>, AppError> {
        let t0 = Instant::now();
        let Some(frame) = self.source.next_frame().map_err(AppError::Capture)? else {
            return Ok(None);
        };
        let t_capture = Instant::now();

        self.frame_no += 1;
        let signature = self.grid.signature(&frame);
        let resized = self
            .previous
            .as_ref()
            .is_some_and(|p| p.cols() != signature.cols() || p.rows() != signature.rows());
        if resized {
            self.scene = Scene::new(self.config.index_bucket_size, self.config.identity_iou);
        }
        let dirty = signature.diff(self.previous.as_ref());
        let raw_regions = merge_dirty_cells(&dirty, &self.grid, &frame.bounds());
        let regions: Vec<Rect> = self
            .scene
            .expand_regions(&raw_regions)
            .into_iter()
            .map(|r| r.clip(&frame.bounds()))
            .collect();
        let t_diff = Instant::now();

        let detections: Vec<_> = regions
            .iter()
            .flat_map(|r| self.detector.detect(&frame, *r))
            .collect();
        let delta = self.scene.apply(self.frame_no, &regions, &detections);
        let t_detect = Instant::now();

        let report = PerceptionReport {
            frame_no: self.frame_no,
            width: frame.width(),
            height: frame.height(),
            total_cells: signature.cell_count(),
            dirty_cells: dirty.len(),
            dirty_regions: regions,
            delta,
            element_count: self.scene.len(),
            timings: Timings {
                capture_us: micros(t0, t_capture),
                diff_us: micros(t_capture, t_diff),
                detect_us: micros(t_diff, t_detect),
                total_us: micros(t0, t_detect),
            },
        };
        self.previous = Some(signature);
        Ok(Some(report))
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

fn micros(a: Instant, b: Instant) -> u64 {
    u64::try_from(b.duration_since(a).as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ommatix_domain::{Detection, ElementKind, Frame};

    use super::*;
    use crate::PortError;

    struct Script(VecDeque<Frame>);

    impl FrameSource for Script {
        fn name(&self) -> &'static str {
            "script"
        }
        fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
            Ok(self.0.pop_front())
        }
    }

    /// Test double: reports one blob per non-white pixel bounding box in the region.
    struct BoundingBoxDetector;

    impl ElementDetector for BoundingBoxDetector {
        fn name(&self) -> &'static str {
            "bbox"
        }
        fn detect(&self, frame: &Frame, region: Rect) -> Vec<Detection> {
            let mut acc: Option<Rect> = None;
            for y in region.y..region.bottom() {
                for x in region.x..region.right() {
                    if frame.pixel(x, y) != WHITE {
                        let px = Rect::new(x, y, 1, 1);
                        acc = Some(acc.map_or(px, |a| a.union(&px)));
                    }
                }
            }
            acc.map(|bounds| Detection {
                bounds,
                kind: ElementKind::Blob,
            })
            .into_iter()
            .collect()
        }
    }

    const WHITE: [u8; 4] = [255; 4];

    fn engine(frames: Vec<Frame>) -> PerceptionEngine {
        PerceptionEngine::new(
            Box::new(Script(frames.into())),
            Box::new(BoundingBoxDetector),
            PerceptionConfig {
                cell_size: 16,
                index_bucket_size: 32,
                identity_iou: 0.5,
            },
        )
        .unwrap()
    }

    #[test]
    fn static_screen_costs_nothing_after_first_frame() {
        let mut f = Frame::solid(64, 64, WHITE).unwrap();
        f.fill_rect(Rect::new(4, 4, 8, 8), [0, 0, 0, 255]);
        let mut e = engine(vec![f.clone(), f]);

        let first = e.tick().unwrap().unwrap();
        assert_eq!(first.dirty_cells, 16);
        assert_eq!(first.delta.added.len(), 1);

        let second = e.tick().unwrap().unwrap();
        assert_eq!(second.dirty_cells, 0);
        assert_eq!(second.dirty_regions.len(), 0);
        assert!(second.delta.is_empty());
        assert_eq!(second.element_count, 1);
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
        assert_eq!(r.dirty_regions, vec![Rect::new(32, 32, 16, 16)]);
        assert_eq!(r.delta.added[0].bounds, Rect::new(36, 36, 8, 8));
        assert_eq!(e.elements_at(Point::new(40, 40)).len(), 1);
    }

    #[test]
    fn resolution_change_resets_the_scene() {
        let mut a = Frame::solid(32, 32, WHITE).unwrap();
        a.fill_rect(Rect::new(0, 0, 4, 4), [0, 0, 0, 255]);
        let b = Frame::solid(48, 32, WHITE).unwrap();
        let mut e = engine(vec![a, b]);
        e.tick().unwrap();
        let r = e.tick().unwrap().unwrap();
        assert_eq!(r.dirty_cells, r.total_cells);
        assert_eq!(r.element_count, 0);
    }
}
