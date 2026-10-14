//! What every capturer adds to raw pixels: BGRA conversion, a geometry epoch that grows
//! when the frame size changes, an increasing `frame_seq` and the capture instant.

use std::sync::Arc;
use std::time::Duration;

use ommatix_application::{CapturedFrame, Clock, PortError};
use ommatix_domain::incremental::BgraFrame;

/// Stamps the frames of one capturer.
#[derive(Clone)]
pub(crate) struct Stamper {
    clock: Arc<dyn Clock>,
    frame_seq: u64,
    geometry_epoch: u64,
    last_size: Option<(u32, u32)>,
}

impl std::fmt::Debug for Stamper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stamper")
            .field("frame_seq", &self.frame_seq)
            .field("geometry_epoch", &self.geometry_epoch)
            .field("last_size", &self.last_size)
            .finish_non_exhaustive()
    }
}

impl Stamper {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            frame_seq: 0,
            geometry_epoch: 1,
            last_size: None,
        }
    }

    pub fn now(&self) -> Duration {
        self.clock.now()
    }

    /// Converts tightly packed RGBA8 pixels to BGRA and stamps them.
    pub fn stamp_rgba(
        &mut self,
        adapter: &'static str,
        width: u32,
        height: u32,
        mut rgba: Vec<u8>,
        captured_at: Duration,
    ) -> Result<CapturedFrame, PortError> {
        for p in rgba.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let stride = 4usize.saturating_mul(width as usize);
        // A broken image must not start a new epoch: commit the epoch only on success.
        let size = (width, height);
        let epoch = if self.last_size.is_some_and(|last| last != size) {
            self.geometry_epoch + 1
        } else {
            self.geometry_epoch
        };
        let frame = BgraFrame::new(width, height, stride, epoch, rgba).map_err(|e| {
            PortError::new(adapter, "the captured image is unusable").with_source(e)
        })?;
        self.geometry_epoch = epoch;
        self.last_size = Some(size);
        Ok(self.stamp(frame, captured_at))
    }

    /// Stamps a frame that already carries its epoch.
    pub fn stamp(&mut self, frame: BgraFrame, captured_at: Duration) -> CapturedFrame {
        self.frame_seq += 1;
        CapturedFrame {
            frame,
            frame_seq: self.frame_seq,
            captured_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use ommatix_application::ManualClock;

    use super::*;

    #[test]
    fn rgba_becomes_bgra_and_a_new_size_is_a_new_epoch() {
        let clock = Arc::new(ManualClock::new());
        let mut s = Stamper::new(clock.clone());
        let a = s
            .stamp_rgba("t", 1, 1, vec![1, 2, 3, 4], clock.now())
            .unwrap();
        assert_eq!(a.frame.pixels(), &[3, 2, 1, 4]);
        assert_eq!((a.frame_seq, a.frame.geometry_epoch()), (1, 1));
        let b = s.stamp_rgba("t", 1, 1, vec![0; 4], clock.now()).unwrap();
        assert_eq!((b.frame_seq, b.frame.geometry_epoch()), (2, 1));
        assert!(s.stamp_rgba("t", 2, 2, vec![0; 3], clock.now()).is_err());
        let c = s.stamp_rgba("t", 2, 1, vec![0; 8], clock.now()).unwrap();
        assert_eq!((c.frame_seq, c.frame.geometry_epoch()), (3, 2));
    }
}
