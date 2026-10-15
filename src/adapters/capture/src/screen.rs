use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::{CapturedFrame, Clock, FrameCapturer, PortError};
use ommatix_domain::{DisplayMapping, Scaling};
use xcap::Monitor;

use crate::stamp::Stamper;

const ADAPTER: &str = "screen";

/// The mapping native input must use, shared with whoever converts action points.
type SharedMapping = Arc<Mutex<Result<DisplayMapping, String>>>;

/// Captures the primary monitor (or the first one found), freshly on every call, as BGRA
/// frames.
///
/// When opened it measures the monitor once, to learn the frame size and how frame
/// pixels map to OS input coordinates ([`ScreenSource::display_mapping`]). A later frame
/// of another size starts a new geometry epoch (so every earlier target is stale) and
/// invalidates that mapping for good: native input fails closed until Ommatix restarts.
///
/// The monitor is looked up by id for every capture: xcap's handle is not `Send` on
/// Windows, and a monitor that disappears must be an error, not another screen.
#[derive(Debug)]
pub struct ScreenSource {
    monitor_id: u32,
    frame_size: (u32, u32),
    mapping: SharedMapping,
    stamper: Stamper,
}

impl ScreenSource {
    /// `clock` stamps each capture; it must be the engine's monotonic clock.
    pub fn primary(clock: Arc<dyn Clock>) -> Result<Self, PortError> {
        let monitors = all_monitors()?;
        let monitor = monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())
            .ok_or_else(|| PortError::new(ADAPTER, "no monitor found"))?;

        let probe = capture_rgba(monitor)?;
        let frame_size = (probe.width(), probe.height());
        let geometry = |e| PortError::new(ADAPTER, "cannot read monitor geometry").with_source(e);
        let origin = (
            monitor.x().map_err(geometry)?,
            monitor.y().map_err(geometry)?,
        );
        let size = (
            monitor.width().map_err(geometry)?,
            monitor.height().map_err(geometry)?,
        );
        let mapping = input_mapping(input_space(monitor), origin, size, frame_size)
            .map_err(|reason| format!("native input is not available: {reason}"));
        Ok(Self {
            monitor_id: monitor.id().map_err(geometry)?,
            frame_size,
            mapping: Arc::new(Mutex::new(mapping)),
            stamper: Stamper::new(clock),
        })
    }

    /// How this monitor's frame pixels map to OS input coordinates (HiDPI scale and the
    /// monitor's position on a multi-monitor desktop), for the native input adapter.
    ///
    /// An error on systems where the mapping has not been verified, and after the frame
    /// size changed: native input must never guess where a click lands.
    pub fn display_mapping(&self) -> Result<DisplayMapping, PortError> {
        read_mapping(&self.mapping)
    }

    /// A live view of [`ScreenSource::display_mapping`] for the native input adapter,
    /// which starts failing as soon as a capture sees another frame size.
    pub fn mapping_source(
        &self,
    ) -> impl Fn() -> Result<DisplayMapping, PortError> + Send + Sync + 'static {
        let mapping = self.mapping.clone();
        move || read_mapping(&mapping)
    }
}

fn read_mapping(mapping: &SharedMapping) -> Result<DisplayMapping, PortError> {
    mapping
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .map_err(|reason| PortError::new(ADAPTER, reason))
}

fn all_monitors() -> Result<Vec<Monitor>, PortError> {
    Monitor::all().map_err(|e| PortError::new(ADAPTER, "cannot list monitors").with_source(e))
}

fn capture_rgba(monitor: &Monitor) -> Result<xcap::image::RgbaImage, PortError> {
    monitor
        .capture_image()
        .map_err(|e| PortError::new(ADAPTER, "capture failed").with_source(e))
}

impl FrameCapturer for ScreenSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let monitor = all_monitors()?
            .into_iter()
            .find(|m| m.id().is_ok_and(|id| id == self.monitor_id))
            .ok_or_else(|| {
                PortError::new(ADAPTER, "the captured monitor is gone; restart Ommatix")
            })?;
        let captured_at = self.stamper.now();
        let img = capture_rgba(&monitor)?;
        let (w, h) = (img.width(), img.height());
        if let Some(reason) = size_change(self.frame_size, (w, h)) {
            *self.mapping.lock().unwrap_or_else(PoisonError::into_inner) = Err(reason);
        }
        // This interim adapter does not read the monitor's scale: it cannot tell it (ADR-0031).
        self.stamper
            .stamp_rgba(ADAPTER, w, h, img.into_raw(), captured_at, Scaling::Unknown)
    }
}

/// Why the start-up mapping no longer holds, if the frame size changed: the resolution
/// or the display scale changed.
fn size_change((w, h): (u32, u32), (fw, fh): (u32, u32)) -> Option<String> {
    ((fw, fh) != (w, h))
        .then(|| format!("the monitor changed from {w}x{h} to {fw}x{fh} pixels; restart Ommatix"))
}

/// The coordinate space the OS input system uses for the captured monitor, given how
/// xcap reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    dead_code,
    reason = "each platform constructs only the variants that apply to it"
)]
enum InputSpace {
    /// Input uses the units xcap reports the monitor in (macOS: points, while frames are
    /// pixels).
    Reported,
    /// Input uses physical pixels, the frame's own units. xcap divided the monitor's
    /// position by `scale` (X11: Xft.dpi / 96), so it is scaled back.
    Physical { scale: f32 },
    /// Not verified on this system.
    Unsupported(&'static str),
}

/// macOS: CoreGraphics events and `CGDisplayBounds` are both in global points.
#[cfg(target_os = "macos")]
fn input_space(_: &Monitor) -> InputSpace {
    InputSpace::Reported
}

/// Windows: xcap reports the physical display mode, but input is virtualised by the
/// display scale unless the process is DPI-aware, which nothing here makes it.
#[cfg(target_os = "windows")]
fn input_space(monitor: &Monitor) -> InputSpace {
    match monitor.scale_factor() {
        Ok(scale) if (scale - 1.0).abs() < f32::EPSILON => InputSpace::Physical { scale: 1.0 },
        _ => InputSpace::Unsupported("display scaling other than 100% is not supported yet"),
    }
}

/// Linux: input goes through XTest in X root-window pixels, the space the frame is
/// captured in. Under Wayland, XTest only reaches XWayland windows and the coordinate
/// space depends on the compositor.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn input_space(monitor: &Monitor) -> InputSpace {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return InputSpace::Unsupported("Wayland sessions are not supported yet");
    }
    InputSpace::Physical {
        scale: monitor.scale_factor().unwrap_or(1.0),
    }
}

fn input_mapping(
    space: InputSpace,
    origin: (i32, i32),
    size: (u32, u32),
    frame: (u32, u32),
) -> Result<DisplayMapping, &'static str> {
    let mapping = match space {
        InputSpace::Reported => DisplayMapping::new(origin, frame, size),
        InputSpace::Physical { scale } => {
            let unscale = |v: i32| (f64::from(v) * f64::from(scale)).round() as i32;
            DisplayMapping::new((unscale(origin.0), unscale(origin.1)), frame, frame)
        }
        InputSpace::Unsupported(reason) => return Err(reason),
    };
    mapping.map_err(|_| "the monitor reports an empty size")
}

#[cfg(test)]
mod tests {
    use ommatix_domain::Point;

    use super::*;

    #[test]
    fn retina_frames_map_to_points() {
        let m = input_mapping(InputSpace::Reported, (0, 0), (1440, 900), (2880, 1800)).unwrap();
        assert_eq!(m.to_os(Point::new(1080, 632)), (540, 316));
    }

    /// Verification N3: with Xft.dpi 192, xcap reports a 3840x2160 monitor at (3840, 0)
    /// as 1920x1080 at (1920, 0); XTest wants the physical pixel.
    #[test]
    fn x11_hidpi_clicks_land_on_the_physical_pixel() {
        let space = InputSpace::Physical { scale: 2.0 };
        let m = input_mapping(space, (1920, 0), (1920, 1080), (3840, 2160)).unwrap();
        assert_eq!(m.to_os(Point::new(3000, 1500)), (6840, 1500));
        let primary = input_mapping(space, (0, 0), (1920, 1080), (3840, 2160)).unwrap();
        assert_eq!(primary.to_os(Point::new(3000, 1500)), (3000, 1500));
    }

    /// Verification N4: a frame of another size invalidates the start-up mapping.
    #[test]
    fn a_resolution_change_invalidates_the_mapping() {
        assert_eq!(size_change((1920, 1080), (1920, 1080)), None);
        let reason = size_change((1920, 1080), (2560, 1440)).unwrap();
        assert!(reason.contains("1920x1080 to 2560x1440"), "{reason}");
        let shared: SharedMapping = Arc::new(Mutex::new(Err(reason)));
        assert!(read_mapping(&shared).is_err());
    }

    #[test]
    fn unverified_systems_refuse_native_input() {
        let space = InputSpace::Unsupported("Wayland sessions are not supported yet");
        assert_eq!(
            input_mapping(space, (0, 0), (1920, 1080), (1920, 1080)),
            Err("Wayland sessions are not supported yet")
        );
    }
}
