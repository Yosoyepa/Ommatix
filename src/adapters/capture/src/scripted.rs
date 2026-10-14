use std::sync::Arc;

use ommatix_application::{CapturedFrame, Clock, FrameCapturer, PortError};
use ommatix_domain::Frame;
use ommatix_domain::incremental::BgraFrame;

use crate::stamp::Stamper;

const ADAPTER: &str = "scripted";

#[derive(Debug, Clone)]
enum Script {
    /// RGBA frames; the epoch follows their sizes.
    Rgba(Vec<Frame>),
    /// BGRA frames that carry their own epochs.
    Bgra(Vec<BgraFrame>),
}

impl Script {
    fn len(&self) -> usize {
        match self {
            Self::Rgba(f) => f.len(),
            Self::Bgra(f) => f.len(),
        }
    }
}

/// Replays an in-memory list of frames, optionally looping forever. Capturing past the
/// end of a script that does not loop is an error (`CAPTURE_UNAVAILABLE` on the wire).
#[derive(Debug, Clone)]
pub struct ScriptedSource {
    script: Script,
    cursor: usize,
    looping: bool,
    stamper: Stamper,
}

impl ScriptedSource {
    /// RGBA frames, converted to BGRA; the geometry epoch starts at 1 and grows whenever
    /// the frame size changes. `clock` stamps each capture.
    pub fn new(frames: Vec<Frame>, clock: Arc<dyn Clock>) -> Self {
        Self::with_script(Script::Rgba(frames), clock)
    }

    /// BGRA frames replayed as they are, geometry epochs included.
    pub fn from_bgra(frames: Vec<BgraFrame>, clock: Arc<dyn Clock>) -> Self {
        Self::with_script(Script::Bgra(frames), clock)
    }

    fn with_script(script: Script, clock: Arc<dyn Clock>) -> Self {
        Self {
            script,
            cursor: 0,
            looping: false,
            stamper: Stamper::new(clock),
        }
    }

    /// Restart from the first frame when exhausted (useful for a long-running server).
    #[must_use]
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }
}

impl FrameCapturer for ScriptedSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        if self.cursor == self.script.len() && self.looping {
            self.cursor = 0;
        }
        let at = self.cursor;
        if at >= self.script.len() {
            return Err(PortError::new(ADAPTER, "the script has no more frames"));
        }
        self.cursor += 1;
        let captured_at = self.stamper.now();
        match &self.script {
            Script::Rgba(frames) => {
                let f = &frames[at];
                let rgba = f.as_rgba().to_vec();
                self.stamper
                    .stamp_rgba(ADAPTER, f.width(), f.height(), rgba, captured_at)
            }
            Script::Bgra(frames) => Ok(self.stamper.stamp(frames[at].clone(), captured_at)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ommatix_application::ManualClock;

    use super::*;

    fn frames() -> Vec<Frame> {
        (1..=3)
            .map(|w| Frame::solid(w, 1, [0, 0, 0, 255]).unwrap())
            .collect()
    }

    fn widths(src: &mut ScriptedSource, n: usize) -> Vec<Option<u32>> {
        (0..n)
            .map(|_| src.capture().ok().map(|f| f.frame.width()))
            .collect()
    }

    #[test]
    fn replays_once_then_reports_exhaustion() {
        let mut once = ScriptedSource::new(frames(), Arc::new(ManualClock::new()));
        assert_eq!(widths(&mut once, 4), [Some(1), Some(2), Some(3), None]);
        assert_eq!(widths(&mut once, 1), [None]);
    }

    #[test]
    fn loops_when_asked() {
        let clock = Arc::new(ManualClock::new());
        let mut forever = ScriptedSource::new(frames(), clock.clone()).looping();
        assert_eq!(
            widths(&mut forever, 5),
            [Some(1), Some(2), Some(3), Some(1), Some(2)]
        );
        let mut empty = ScriptedSource::new(Vec::new(), clock).looping();
        assert_eq!(widths(&mut empty, 2), [None, None]);
    }

    #[test]
    fn stamps_sequence_epoch_and_time() {
        let clock = Arc::new(ManualClock::new());
        let mut src = ScriptedSource::new(frames(), clock.clone());
        clock.advance(Duration::from_millis(16));
        let a = src.capture().unwrap();
        assert_eq!((a.frame_seq, a.captured_at), (1, Duration::from_millis(16)));
        assert_eq!(a.frame.geometry_epoch(), 1);
        let b = src.capture().unwrap();
        assert_eq!((b.frame_seq, b.frame.geometry_epoch()), (2, 2), "new size");

        let bgra = BgraFrame::new(1, 1, 4, 7, vec![0; 4]).unwrap();
        let mut kept = ScriptedSource::from_bgra(vec![bgra], clock);
        assert_eq!(kept.capture().unwrap().frame.geometry_epoch(), 7);
    }
}
