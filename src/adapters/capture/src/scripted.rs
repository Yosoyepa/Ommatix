use std::collections::VecDeque;

use ommatix_application::{FrameSource, PortError};
use ommatix_domain::{Frame, Rect};

/// Replays an in-memory list of frames. Optionally loops forever.
#[derive(Debug, Clone)]
pub struct ScriptedSource {
    frames: VecDeque<Frame>,
    original: Vec<Frame>,
    looping: bool,
}

impl ScriptedSource {
    pub fn new(frames: Vec<Frame>) -> Self {
        Self {
            frames: frames.clone().into(),
            original: frames,
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
        if self.frames.is_empty() && self.looping {
            self.frames = self.original.clone().into();
        }
        Ok(self.frames.pop_front())
    }
}

/// A small, deterministic "desktop session" used by `ommatix demo` and integration tests:
///
/// 1. A window with a title bar, a text field and two buttons appears.
/// 2. Nothing changes (a static screen must cost ~nothing).
/// 3. The user types into the text field.
/// 4. A modal dialog with an OK button opens.
pub fn demo_script() -> Vec<Frame> {
    const DESKTOP: [u8; 4] = [236, 239, 244, 255];
    const WINDOW: [u8; 4] = [255, 255, 255, 255];
    const TITLE: [u8; 4] = [46, 52, 64, 255];
    const PRIMARY: [u8; 4] = [94, 129, 172, 255];
    const SECONDARY: [u8; 4] = [216, 222, 233, 255];
    const INK: [u8; 4] = [20, 20, 20, 255];
    const FIELD: [u8; 4] = [150, 150, 150, 255];

    let mut f1 = Frame::solid(640, 400, DESKTOP).expect("non-empty frame");
    f1.fill_rect(Rect::new(40, 40, 560, 320), WINDOW);
    f1.fill_rect(Rect::new(40, 40, 560, 28), TITLE);
    // Text field: 1px border.
    f1.fill_rect(Rect::new(80, 120, 300, 30), FIELD);
    f1.fill_rect(Rect::new(81, 121, 298, 28), WINDOW);
    f1.fill_rect(Rect::new(400, 300, 90, 32), SECONDARY); // "Cancel"
    f1.fill_rect(Rect::new(500, 300, 80, 32), PRIMARY); // "Save"

    let f2 = f1.clone();

    let mut f3 = f1.clone();
    for i in 0..5 {
        f3.fill_rect(Rect::new(90 + i * 9, 128, 6, 12), INK); // typed glyphs
    }

    let mut f4 = f3.clone();
    f4.fill_rect(Rect::new(200, 170, 240, 110), TITLE); // modal frame
    f4.fill_rect(Rect::new(202, 172, 236, 106), WINDOW);
    f4.fill_rect(Rect::new(360, 240, 64, 28), PRIMARY); // "OK"

    vec![f1, f2, f3, f4]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_source_exhausts_or_loops() {
        let frames = demo_script();
        let mut once = ScriptedSource::new(frames.clone());
        for _ in 0..frames.len() {
            assert!(once.next_frame().unwrap().is_some());
        }
        assert!(once.next_frame().unwrap().is_none());

        let mut forever = ScriptedSource::new(frames.clone()).looping();
        for _ in 0..frames.len() * 2 {
            assert!(forever.next_frame().unwrap().is_some());
        }
    }
}
