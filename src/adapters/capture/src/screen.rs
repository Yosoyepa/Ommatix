use ommatix_application::{FrameSource, PortError};
use ommatix_domain::{DisplayMapping, Frame};
use xcap::Monitor;

const ADAPTER: &str = "screen";

/// Captures the primary monitor (or the first one found), freshly on every call.
///
/// When opened it measures the monitor once, to learn the frame size and how frame
/// pixels map to OS input coordinates ([`ScreenSource::display_mapping`]). A later frame
/// of another size is an error: restart after changing the resolution or the scale.
///
/// The monitor is looked up by id for every capture: xcap's handle is not `Send` on
/// Windows, and a monitor that disappears must be an error, not another screen.
#[derive(Debug)]
pub struct ScreenSource {
    monitor_id: u32,
    frame_size: (u32, u32),
    mapping: Result<DisplayMapping, &'static str>,
}

impl ScreenSource {
    pub fn primary() -> Result<Self, PortError> {
        let monitors = all_monitors()?;
        let monitor = monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())
            .ok_or_else(|| PortError::new(ADAPTER, "no monitor found"))?;

        let probe = capture(monitor)?;
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
        let mapping = input_mapping(input_space(monitor), origin, size, frame_size);
        Ok(Self {
            monitor_id: monitor.id().map_err(geometry)?,
            frame_size,
            mapping,
        })
    }

    /// How this monitor's frame pixels map to OS input coordinates (HiDPI scale and the
    /// monitor's position on a multi-monitor desktop), for the native input adapter.
    ///
    /// An error on systems where the mapping has not been verified: native input must
    /// never guess where a click lands.
    pub fn display_mapping(&self) -> Result<DisplayMapping, PortError> {
        self.mapping.map_err(|reason| {
            PortError::new(ADAPTER, format!("native input is not available: {reason}"))
        })
    }
}

fn all_monitors() -> Result<Vec<Monitor>, PortError> {
    Monitor::all().map_err(|e| PortError::new(ADAPTER, "cannot list monitors").with_source(e))
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
        let monitor = all_monitors()?
            .into_iter()
            .find(|m| m.id().is_ok_and(|id| id == self.monitor_id))
            .ok_or_else(|| {
                PortError::new(ADAPTER, "the captured monitor is gone; restart Ommatix")
            })?;
        let frame = capture(&monitor)?;
        same_size(self.frame_size, &frame)?;
        Ok(Some(frame))
    }
}

/// A frame of another size than the first one means the resolution or the display scale
/// changed: the input mapping measured at start-up no longer holds.
fn same_size((w, h): (u32, u32), frame: &Frame) -> Result<(), PortError> {
    if (frame.width(), frame.height()) == (w, h) {
        return Ok(());
    }
    Err(PortError::new(
        ADAPTER,
        format!(
            "the monitor changed from {w}x{h} to {}x{} pixels; restart Ommatix",
            frame.width(),
            frame.height()
        ),
    ))
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
    fn a_resolution_change_is_an_error() {
        let frame = |w, h| Frame::solid(w, h, [0, 0, 0, 255]).unwrap();
        assert!(same_size((1920, 1080), &frame(1920, 1080)).is_ok());
        let err = same_size((1920, 1080), &frame(2560, 1440)).unwrap_err();
        assert!(err.to_string().contains("1920x1080 to 2560x1440"), "{err}");
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
