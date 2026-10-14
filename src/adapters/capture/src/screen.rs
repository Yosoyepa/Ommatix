use ommatix_application::{FrameSource, PortError};
use ommatix_domain::Frame;
use xcap::Monitor;

const ADAPTER: &str = "screen";

/// Captures the primary monitor (or the first one found) on every call.
#[derive(Debug)]
pub struct ScreenSource {
    monitor: Monitor,
}

impl ScreenSource {
    pub fn primary() -> Result<Self, PortError> {
        let monitors = Monitor::all()
            .map_err(|e| PortError::new(ADAPTER, "cannot list monitors").with_source(e))?;
        let monitor = monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())
            .cloned()
            .ok_or_else(|| PortError::new(ADAPTER, "no monitor found"))?;
        Ok(Self { monitor })
    }
}

impl FrameSource for ScreenSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        let img = self
            .monitor
            .capture_image()
            .map_err(|e| PortError::new(ADAPTER, "capture failed").with_source(e))?;
        let (w, h) = (img.width(), img.height());
        Frame::from_rgba(w, h, img.into_raw())
            .map(Some)
            .map_err(|e| PortError::new(ADAPTER, e.to_string()))
    }
}
