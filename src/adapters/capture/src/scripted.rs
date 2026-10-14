use ommatix_application::{FrameSource, PortError};
use ommatix_domain::Frame;

/// Replays an in-memory list of frames, optionally looping forever.
#[derive(Debug, Clone)]
pub struct ScriptedSource {
    frames: Vec<Frame>,
    cursor: usize,
    looping: bool,
}

impl ScriptedSource {
    pub fn new(frames: Vec<Frame>) -> Self {
        Self {
            frames,
            cursor: 0,
            looping: false,
        }
    }

    /// Restart from the first frame when exhausted (useful for a long-running server).
    #[must_use]
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }
}

impl FrameSource for ScriptedSource {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        if self.cursor == self.frames.len() && self.looping {
            self.cursor = 0;
        }
        let frame = self.frames.get(self.cursor).cloned();
        self.cursor += usize::from(frame.is_some());
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> Vec<Frame> {
        (1..=3)
            .map(|w| Frame::solid(w, 1, [0, 0, 0, 255]).unwrap())
            .collect()
    }

    fn widths(src: &mut ScriptedSource, n: usize) -> Vec<Option<u32>> {
        (0..n)
            .map(|_| src.next_frame().unwrap().map(|f| f.width()))
            .collect()
    }

    #[test]
    fn replays_once_then_reports_exhaustion() {
        let mut once = ScriptedSource::new(frames());
        assert_eq!(widths(&mut once, 4), [Some(1), Some(2), Some(3), None]);
        assert_eq!(widths(&mut once, 1), [None]);
    }

    #[test]
    fn loops_when_asked() {
        let mut forever = ScriptedSource::new(frames()).looping();
        assert_eq!(
            widths(&mut forever, 5),
            [Some(1), Some(2), Some(3), Some(1), Some(2)]
        );
        let mut empty = ScriptedSource::new(Vec::new()).looping();
        assert_eq!(widths(&mut empty, 2), [None, None]);
    }
}
