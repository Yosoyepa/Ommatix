use ommatix_application::{FrameSource, PortError};
use ommatix_domain::{DisplayMapping, Frame};
use xcap::Monitor;

const ADAPTER: &str = "screen";

/// Captures the primary monitor (or the first one found) on every call.
///
/// The first image is taken when the source is opened, to learn how frame pixels map to
/// OS input coordinates ([`ScreenSource::display_mapping`]); it is also the first frame
/// served. Restart after changing the display resolution or scale.
#[derive(Debug)]
pub struct ScreenSource {
    monitor: Monitor,
    mapping: DisplayMapping,
    first: Option<Frame>,
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

        let first = capture(&monitor)?;
        let geometry = |e| PortError::new(ADAPTER, "cannot read monitor geometry").with_source(e);
        let origin = (
            monitor.x().map_err(geometry)?,
            monitor.y().map_err(geometry)?,
        );
        let os_size = (
            monitor.width().map_err(geometry)?,
            monitor.height().map_err(geometry)?,
        );
        let mapping = DisplayMapping::new(origin, (first.width(), first.height()), os_size)
            .map_err(|e| PortError::new(ADAPTER, "monitor reports an empty size").with_source(e))?;
        Ok(Self {
            monitor,
            mapping,
            first: Some(first),
        })
    }

    /// How this monitor's frame pixels map to OS input coordinates (HiDPI scale and the
    /// monitor's position on a multi-monitor desktop). Hand it to the native input
    /// adapter so clicks land where the agent saw the element.
    pub fn display_mapping(&self) -> DisplayMapping {
        self.mapping
    }
}

fn capture(monitor: &Monitor) -> Result<Frame, PortError> {
    let img = monitor
        .capture_image()
        .map_err(|e| PortError::new(ADAPTER, "capture failed").with_source(e))?;
    let (w, h) = (img.width(), img.height());
    Frame::from_rgba(w, h, img.into_raw())
        .map_err(|e| PortError::new(ADAPTER, "captured image is unusable").with_source(e))
}

impl FrameSource for ScreenSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        match self.first.take() {
            Some(frame) => Ok(Some(frame)),
            None => capture(&self.monitor).map(Some),
        }
    }
}
