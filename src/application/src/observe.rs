use std::sync::Arc;
use std::time::Duration;

use ommatix_domain::SceneDelta;

use crate::{AppError, Clock, PerceptionEngine};

/// When the screen counts as settled after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlePolicy {
    /// Pause between two perception ticks. Must be positive.
    pub poll_interval: Duration,
    /// Consecutive frames without a changed cell that count as settled. At least 1.
    pub stable_frames: u32,
    /// Keep watching at least this long: applications take a moment to react to input.
    pub min_wait: Duration,
    /// Stop watching after this long, settled or not. Not shorter than `min_wait`.
    pub timeout: Duration,
}

impl Default for SettlePolicy {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(16),
            stable_frames: 2,
            min_wait: Duration::from_millis(100),
            timeout: Duration::from_secs(1),
        }
    }
}

impl SettlePolicy {
    fn validate(&self) -> Result<(), AppError> {
        if self.poll_interval.is_zero() {
            return Err(AppError::InvalidSettlePolicy(
                "poll_interval must be positive",
            ));
        }
        if self.stable_frames == 0 {
            return Err(AppError::InvalidSettlePolicy(
                "stable_frames must be at least 1",
            ));
        }
        if self.min_wait > self.timeout {
            return Err(AppError::InvalidSettlePolicy(
                "min_wait must not exceed timeout",
            ));
        }
        Ok(())
    }
}

/// What happened on screen while an [`Observer`] watched it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    /// Perception ticks performed (at least 1).
    pub frames: u32,
    /// `true` when the screen stopped changing; `false` when the timeout elapsed or the
    /// frame source ran out first.
    pub settled: bool,
    pub elapsed: Duration,
    /// Number of the last frame observed.
    pub frame_no: u64,
    /// Changed cells, summed over all frames.
    pub dirty_cells: usize,
    /// Re-analysed pixels, summed over all frames.
    pub analysed_px: u64,
    /// Net scene change over the whole observation.
    pub delta: SceneDelta,
    pub element_count: usize,
}

/// Watches the screen after an action until it settles (see [`SettlePolicy`]), so the
/// agent sees the result of its action instead of a half-drawn frame.
pub struct Observer {
    clock: Arc<dyn Clock>,
    policy: SettlePolicy,
}

impl std::fmt::Debug for Observer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Observer")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl Observer {
    pub fn new(clock: Arc<dyn Clock>, policy: SettlePolicy) -> Result<Self, AppError> {
        policy.validate()?;
        Ok(Self { clock, policy })
    }

    pub fn policy(&self) -> &SettlePolicy {
        &self.policy
    }

    /// Perceives until `stable_frames` consecutive frames show no change and `min_wait`
    /// has elapsed, or until `timeout`, or until the source runs out.
    ///
    /// Fails with [`AppError::SourceExhausted`] if not even one frame could be observed.
    /// If a tick fails, the error is returned; the scene keeps what was perceived before.
    pub fn observe(&self, engine: &mut PerceptionEngine) -> Result<Observation, AppError> {
        let start = self.clock.now();
        let mut seen = Observation::default();
        let mut stable = 0;
        loop {
            let Some(report) = engine.tick()? else {
                if seen.frames == 0 {
                    return Err(AppError::SourceExhausted);
                }
                return Ok(seen);
            };
            stable = if report.dirty_cells == 0 {
                stable + 1
            } else {
                0
            };
            seen.frames += 1;
            seen.frame_no = report.frame_no;
            seen.dirty_cells += report.dirty_cells;
            seen.analysed_px += report.analysed_px;
            seen.element_count = report.element_count;
            seen.delta = std::mem::take(&mut seen.delta).then(report.delta);
            seen.elapsed = self.clock.now().saturating_sub(start);

            if stable >= self.policy.stable_frames && seen.elapsed >= self.policy.min_wait {
                seen.settled = true;
                return Ok(seen);
            }
            if seen.elapsed >= self.policy.timeout {
                return Ok(seen);
            }
            self.clock.sleep(self.policy.poll_interval);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ommatix_domain::{Detection, ElementKind, Frame, Rect};

    use super::*;
    use crate::{ElementDetector, FrameSource, ManualClock, PerceptionConfig, PortError};

    const WHITE: [u8; 4] = [255; 4];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    /// Replays frames, then repeats the last one forever (a screen that stopped moving),
    /// unless `ends` is set.
    struct Screen {
        frames: VecDeque<Frame>,
        last: Option<Frame>,
        ends: bool,
    }

    impl FrameSource for Screen {
        fn name(&self) -> &'static str {
            "screen"
        }
        fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
            if let Some(f) = self.frames.pop_front() {
                self.last = Some(f.clone());
                return Ok(Some(f));
            }
            Ok(if self.ends { None } else { self.last.clone() })
        }
    }

    /// Reports the bounding box of the black pixels inside the region.
    struct InkBounds;

    impl ElementDetector for InkBounds {
        fn name(&self) -> &'static str {
            "ink-bounds"
        }
        fn detect(&self, frame: &Frame, region: Rect) -> Result<Vec<Detection>, PortError> {
            let mut ink: Option<Rect> = None;
            for y in region.y..region.bottom() {
                for x in region.x..region.right() {
                    if frame.pixel(x, y) == BLACK {
                        let px = Rect::new(x, y, 1, 1);
                        ink = Some(ink.map_or(px, |r| r.union(&px)));
                    }
                }
            }
            Ok(ink
                .map(|bounds| Detection {
                    bounds,
                    kind: ElementKind::Blob,
                })
                .into_iter()
                .collect())
        }
    }

    fn inked(cells: &[u32]) -> Frame {
        let mut f = Frame::solid(64, 16, WHITE).unwrap();
        for c in cells {
            f.fill_rect(Rect::new(c * 16, 0, 16, 16), BLACK);
        }
        f
    }

    fn setup(frames: Vec<Frame>, ends: bool) -> (PerceptionEngine, Observer) {
        let clock = Arc::new(ManualClock::new());
        let screen = Screen {
            frames: frames.into(),
            last: None,
            ends,
        };
        let config = PerceptionConfig {
            cell_size: 16,
            ..PerceptionConfig::default()
        };
        let engine =
            PerceptionEngine::new(Box::new(screen), Box::new(InkBounds), clock.clone(), config)
                .unwrap();
        (
            engine,
            Observer::new(clock, SettlePolicy::default()).unwrap(),
        )
    }

    #[test]
    fn waits_for_an_animation_to_finish_and_reports_the_net_change() {
        // Ink appears in cell 0, moves to cell 1, then to cell 2 and stays there.
        let (mut engine, observer) = setup(
            vec![inked(&[]), inked(&[0]), inked(&[1]), inked(&[2])],
            false,
        );
        engine.tick().unwrap();

        let seen = observer.observe(&mut engine).unwrap();
        assert!(seen.settled);
        assert!(seen.elapsed >= observer.policy().min_wait);
        assert_eq!(seen.delta.added.len(), 1, "{:?}", seen.delta);
        assert_eq!(seen.delta.added[0].bounds, Rect::new(32, 0, 16, 16));
        assert_eq!(seen.delta.removed.len(), 0, "transient blobs cancel out");
        assert_eq!(seen.dirty_cells, 1 + 2 + 2);
    }

    #[test]
    fn gives_up_on_a_screen_that_never_settles() {
        let blinking: Vec<Frame> = (0..200).map(|i| inked(&[i % 2])).collect();
        let (mut engine, observer) = setup(blinking, false);
        let seen = observer.observe(&mut engine).unwrap();
        assert!(!seen.settled);
        assert!(seen.elapsed >= observer.policy().timeout);
    }

    #[test]
    fn an_exhausted_source_ends_the_observation() {
        let (mut engine, observer) = setup(vec![inked(&[]), inked(&[0])], true);
        let seen = observer.observe(&mut engine).unwrap();
        assert_eq!(seen.frames, 2);
        assert!(!seen.settled);
        assert!(matches!(
            observer.observe(&mut engine),
            Err(AppError::SourceExhausted)
        ));
    }

    #[test]
    fn rejects_policies_that_could_spin_forever() {
        let clock: Arc<dyn Clock> = Arc::new(ManualClock::new());
        for bad in [
            SettlePolicy {
                poll_interval: Duration::ZERO,
                ..SettlePolicy::default()
            },
            SettlePolicy {
                stable_frames: 0,
                ..SettlePolicy::default()
            },
            SettlePolicy {
                min_wait: Duration::from_secs(2),
                ..SettlePolicy::default()
            },
        ] {
            assert!(Observer::new(clock.clone(), bad).is_err(), "{bad:?}");
        }
    }
}
