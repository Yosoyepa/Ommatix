//! DXGI Desktop Duplication capture (Windows only, feature `dxgi`).
//!
//! This module holds the platform-neutral part, safe Rust that is unit-tested on every OS:
//! packing the mapped rows into a frame, the geometry epoch and the reuse of the last
//! frame when the desktop did not change. The Win32/COM calls live in the
//! `#[cfg(windows)]` submodule `win`, the only place where this crate allows `unsafe`
//! (ADR-0016).
//!
//! Pixels stay in memory: nothing here writes a frame to disk or to a log (ADR-0012).

#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows module drives these helpers")
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ommatix_application::{CapturedFrame, Clock, PortError};
use ommatix_domain::incremental::BgraFrame;
use ommatix_domain::{DisplayMapping, Scaling};

use crate::stamp::Stamper;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "ADR-0016: DXGI and Direct3D 11 are COM interfaces whose methods are unsafe fns"
)]
mod win;

#[cfg(windows)]
pub use win::DxgiCapturer;

pub(crate) const ADAPTER: &str = "dxgi";

/// Bytes per BGRA8 pixel.
const BPP: usize = 4;

/// The mapping the input adapter must use, shared with whoever converts action points.
pub(crate) type SharedMapping = Arc<Mutex<Result<DisplayMapping, String>>>;

/// How to open a [`DxgiCapturer`](crate::DxgiCapturer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DxgiOptions {
    /// Which output to duplicate, counting the outputs attached to the desktop across
    /// adapters in DXGI enumeration order. Output 0 is normally the primary monitor.
    pub output_index: u32,
    /// How long one capture waits for a desktop update. When none arrives in that time,
    /// the desktop has not changed since the last frame and that frame is returned again
    /// (same pixels, new `frame_seq` and capture instant).
    pub acquire_timeout: Duration,
}

impl Default for DxgiOptions {
    fn default() -> Self {
        Self {
            output_index: 0,
            acquire_timeout: Duration::from_millis(10),
        }
    }
}

/// What a capture saw about the desktop update it consumed, for diagnostics only: the
/// engine's own differ stays authoritative over which regions changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DxgiDiagnostics {
    /// Desktop images copied out of the duplication.
    pub frames_acquired: u64,
    /// Captures that found no desktop update and returned the last frame again.
    pub unchanged: u64,
    /// Times the duplication was lost (mode change, secure desktop, ...) and recreated.
    pub access_lost: u64,
    /// Dirty rectangles DXGI reported with the last acquired image.
    pub last_dirty_rects: u32,
    /// Move rectangles DXGI reported with the last acquired image.
    pub last_move_rects: u32,
    /// Whether DXGI masked protected content out of the last acquired image.
    pub protected_content_masked: bool,
}

/// What identifies the duplicated output's geometry. Any change starts a new epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Geometry {
    /// The output's top-left corner on the virtual desktop, in physical pixels.
    pub origin: (i32, i32),
    /// The desktop image size, in physical pixels.
    pub size: (u32, u32),
    /// Effective DPI of the monitor (96 = 100 %).
    pub dpi: u32,
}

impl Geometry {
    fn mapping(self) -> Result<DisplayMapping, String> {
        // Per-monitor-v2 DPI awareness: frames and input share physical pixels, so the
        // mapping only adds the output's origin on the virtual desktop.
        DisplayMapping::new(self.origin, self.size, self.size)
            .map_err(|_| "the output reports an empty size".to_owned())
    }
}

/// `DXGI_MODE_ROTATION_IDENTITY`.
pub(crate) const ROTATION_IDENTITY: i32 = 1;

/// Only unrotated outputs are in the profile (SDD §2.8): a rotated desktop image would
/// need a transform the engine does not apply.
pub(crate) fn check_rotation(rotation: i32) -> Result<(), PortError> {
    if rotation == ROTATION_IDENTITY {
        Ok(())
    } else {
        Err(PortError::new(
            ADAPTER,
            format!(
                "the output is rotated (DXGI rotation {rotation}); only unrotated outputs are \
                 supported"
            ),
        ))
    }
}

/// How long the first capture after (re)creating the duplication, or after the image size
/// changed, waits for a real desktop image.
pub(crate) const FIRST_FRAME_TIMEOUT: Duration = Duration::from_millis(500);

/// What a capture does with a desktop update it acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateUse {
    /// A desktop image was presented: copy it.
    Copy,
    /// Only the pointer or the metadata changed: return the last frame again.
    ReuseLast,
    /// No desktop image has been presented since the duplication was (re)created and
    /// there is no last frame to fall back on: the texture would be blank (S0 §7). Release
    /// the update and wait at most this long for another one.
    AcquireAgain(Duration),
    /// No desktop image arrived within [`FIRST_FRAME_TIMEOUT`]: fail rather than hand the
    /// engine a blank frame.
    Fail,
}

/// Decides what to do with an acquired update whose `LastPresentTime` is
/// `last_present_time` (0 when no desktop image was presented with it). `has_last_frame`
/// says whether a last frame of the current size exists; `waited` is how long this
/// capture has waited for updates so far.
pub(crate) fn classify_update(
    last_present_time: i64,
    has_last_frame: bool,
    waited: Duration,
) -> UpdateUse {
    if last_present_time != 0 {
        return UpdateUse::Copy;
    }
    if has_last_frame {
        return UpdateUse::ReuseLast;
    }
    match FIRST_FRAME_TIMEOUT.checked_sub(waited) {
        Some(left) if !left.is_zero() => UpdateUse::AcquireAgain(left),
        _ => UpdateUse::Fail,
    }
}

/// Copies `height` rows of `width` BGRA pixels out of a mapped surface whose rows are
/// `row_pitch` bytes apart, into a tightly packed buffer. `None` if the surface is too
/// small for those dimensions.
pub(crate) fn pack_rows(src: &[u8], row_pitch: usize, width: u32, height: u32) -> Option<Vec<u8>> {
    let row_bytes = BPP.checked_mul(width as usize)?;
    if width == 0 || height == 0 || row_pitch < row_bytes {
        return None;
    }
    let needed = row_pitch
        .checked_mul(height as usize - 1)?
        .checked_add(row_bytes)?;
    if src.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity(row_bytes.checked_mul(height as usize)?);
    for row in src.chunks(row_pitch).take(height as usize) {
        out.extend_from_slice(row.get(..row_bytes)?);
    }
    Some(out)
}

/// Bytes a mapped surface of `height` rows `row_pitch` apart must at least span.
pub(crate) fn mapped_len(row_pitch: usize, width: u32, height: u32) -> Option<usize> {
    if height == 0 {
        return None;
    }
    row_pitch
        .checked_mul(height as usize - 1)?
        .checked_add(BPP.checked_mul(width as usize)?)
}

/// Sequence numbers, geometry epochs and the last frame of one DXGI capturer.
#[derive(Debug)]
pub(crate) struct FrameState {
    stamper: Stamper,
    epoch: u64,
    geometry: Option<Geometry>,
    /// The duplication was lost since the last frame: the next one starts a new epoch.
    lost: bool,
    last: Option<BgraFrame>,
    mapping: SharedMapping,
}

impl FrameState {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            stamper: Stamper::new(clock),
            epoch: 1,
            geometry: None,
            lost: false,
            last: None,
            mapping: Arc::new(Mutex::new(Err(
                "no frame has been captured yet; observe first".to_owned(),
            ))),
        }
    }

    pub fn now(&self) -> Duration {
        self.stamper.now()
    }

    pub fn mapping(&self) -> SharedMapping {
        self.mapping.clone()
    }

    /// Whether a last frame of `size` exists to fall back on when nothing changed.
    pub fn has_frame_of(&self, size: (u32, u32)) -> bool {
        self.last
            .as_ref()
            .is_some_and(|f| (f.width(), f.height()) == size)
    }

    /// The duplication was lost: whatever comes next is a new epoch, and the last frame
    /// may no longer show the screen.
    pub fn session_lost(&mut self) {
        self.lost = true;
        self.last = None;
        self.set_mapping(Err(
            "the screen capture was lost; observe again before acting".to_owned(),
        ));
    }

    /// The output can no longer be captured as configured (for example, it was rotated):
    /// input must not aim at it.
    pub fn out_of_profile(&mut self, reason: &str) {
        self.session_lost();
        self.set_mapping(Err(reason.to_owned()));
    }

    fn next_epoch(&self, geometry: Geometry) -> u64 {
        match self.geometry {
            Some(last) if last == geometry && !self.lost => self.epoch,
            None if !self.lost => self.epoch,
            _ => self.epoch.saturating_add(1),
        }
    }

    fn set_mapping(&self, mapping: Result<DisplayMapping, String>) {
        *self.mapping.lock().unwrap_or_else(PoisonError::into_inner) = mapping;
    }

    /// Commits `frame` as the latest one and stamps it.
    fn commit(&mut self, frame: BgraFrame, geometry: Geometry, at: Duration) -> CapturedFrame {
        self.epoch = frame.geometry_epoch();
        self.geometry = Some(geometry);
        self.lost = false;
        self.last = Some(frame.clone());
        self.set_mapping(geometry.mapping());
        self.stamper.stamp(frame, at, Scaling::Dpi(geometry.dpi))
    }

    /// A new desktop image, packed BGRA rows of `geometry.size`.
    pub fn fresh(
        &mut self,
        geometry: Geometry,
        pixels: Vec<u8>,
        captured_at: Duration,
    ) -> Result<CapturedFrame, PortError> {
        let (w, h) = geometry.size;
        let stride = BPP.saturating_mul(w as usize);
        let frame =
            BgraFrame::new(w, h, stride, self.next_epoch(geometry), pixels).map_err(|e| {
                PortError::new(ADAPTER, "the captured image is unusable").with_source(e)
            })?;
        Ok(self.commit(frame, geometry, captured_at))
    }

    /// No desktop update arrived: the last frame again, with a new sequence number and
    /// instant. When only the DPI or the origin changed, the same pixels start a new
    /// epoch. `None` when there is no last frame of this size.
    pub fn unchanged(
        &mut self,
        geometry: Geometry,
        captured_at: Duration,
    ) -> Option<CapturedFrame> {
        let last = self.last.clone()?;
        if (last.width(), last.height()) != geometry.size {
            return None;
        }
        let epoch = self.next_epoch(geometry);
        let frame = if epoch == last.geometry_epoch() {
            last
        } else {
            BgraFrame::new(
                last.width(),
                last.height(),
                last.row_stride(),
                epoch,
                last.pixels().to_vec(),
            )
            .ok()?
        };
        Some(self.commit(frame, geometry, captured_at))
    }
}

/// Reads the shared mapping, for the input adapter.
pub(crate) fn read_mapping(mapping: &SharedMapping) -> Result<DisplayMapping, PortError> {
    mapping
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .map_err(|reason| PortError::new(ADAPTER, reason))
}

#[cfg(test)]
mod tests {
    use ommatix_application::ManualClock;
    use ommatix_domain::Point;

    use super::*;

    const G: Geometry = Geometry {
        origin: (0, 0),
        size: (2, 1),
        dpi: 96,
    };

    fn state() -> FrameState {
        FrameState::new(Arc::new(ManualClock::new()))
    }

    #[test]
    fn rows_are_packed_without_the_pitch_padding() {
        // 2x2 pixels, pitch 12 bytes (4 bytes of padding per row).
        let mut src = vec![0u8; 12 + 8];
        src[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        src[12..20].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
        let packed = pack_rows(&src, 12, 2, 2).unwrap();
        assert_eq!(packed, (1..=16).collect::<Vec<u8>>());
        assert_eq!(mapped_len(12, 2, 2), Some(20));
    }

    #[test]
    fn short_or_inconsistent_surfaces_are_refused() {
        assert_eq!(pack_rows(&[0; 19], 12, 2, 2), None);
        assert_eq!(pack_rows(&[0; 64], 4, 2, 2), None, "pitch below a row");
        assert_eq!(pack_rows(&[0; 64], 8, 0, 2), None);
        assert_eq!(pack_rows(&[0; 64], 8, 2, 0), None);
        assert_eq!(pack_rows(&[0; 64], usize::MAX, 2, 2), None);
        assert_eq!(mapped_len(8, 2, 0), None);
    }

    #[test]
    fn a_first_update_without_a_desktop_image_is_never_a_frame() {
        // Found in S0 and the S3 supervised runs: the first update after opening the
        // duplication carried no presented image, and its blank texture became frame 1.
        let ms = Duration::from_millis;
        assert_eq!(
            classify_update(0, false, Duration::ZERO),
            UpdateUse::AcquireAgain(FIRST_FRAME_TIMEOUT)
        );
        assert_eq!(
            classify_update(0, false, ms(120)),
            UpdateUse::AcquireAgain(ms(380))
        );
        assert_eq!(
            classify_update(0, false, FIRST_FRAME_TIMEOUT),
            UpdateUse::Fail
        );
        assert_eq!(classify_update(0, false, ms(900)), UpdateUse::Fail);
        // A presented image is copied, first or not.
        assert_eq!(classify_update(1, false, Duration::ZERO), UpdateUse::Copy);
        assert_eq!(classify_update(-5, false, ms(499)), UpdateUse::Copy);
        assert_eq!(classify_update(42, true, Duration::ZERO), UpdateUse::Copy);
        // Only the pointer moved: the last frame stands.
        assert_eq!(classify_update(0, true, ms(900)), UpdateUse::ReuseLast);
    }

    #[test]
    fn only_unrotated_outputs_are_in_profile() {
        assert!(check_rotation(ROTATION_IDENTITY).is_ok());
        for rotation in [0, 2, 3, 4] {
            assert!(check_rotation(rotation).is_err(), "{rotation}");
        }
    }

    #[test]
    fn frames_count_up_and_the_epoch_follows_the_geometry() {
        let mut s = state();
        let a = s.fresh(G, vec![0; 8], Duration::ZERO).unwrap();
        assert_eq!((a.frame_seq, a.frame.geometry_epoch()), (1, 1));
        let b = s.fresh(G, vec![1; 8], Duration::ZERO).unwrap();
        assert_eq!((b.frame_seq, b.frame.geometry_epoch()), (2, 1));
        let moved = Geometry {
            origin: (1920, -137),
            ..G
        };
        let c = s.fresh(moved, vec![1; 8], Duration::ZERO).unwrap();
        assert_eq!((c.frame_seq, c.frame.geometry_epoch()), (3, 2));
        let resized = Geometry {
            size: (1, 1),
            ..moved
        };
        let d = s.fresh(resized, vec![1; 4], Duration::ZERO).unwrap();
        assert_eq!((d.frame_seq, d.frame.geometry_epoch()), (4, 3));
        // A broken image changes nothing.
        assert!(s.fresh(G, vec![0; 3], Duration::ZERO).is_err());
        let e = s.fresh(resized, vec![2; 4], Duration::ZERO).unwrap();
        assert_eq!((e.frame_seq, e.frame.geometry_epoch()), (5, 3));
    }

    #[test]
    fn a_lost_duplication_starts_a_new_epoch_and_drops_the_last_frame() {
        let mut s = state();
        s.fresh(G, vec![0; 8], Duration::ZERO).unwrap();
        s.session_lost();
        assert!(s.unchanged(G, Duration::ZERO).is_none());
        assert!(read_mapping(&s.mapping()).is_err());
        let f = s.fresh(G, vec![0; 8], Duration::ZERO).unwrap();
        assert_eq!((f.frame_seq, f.frame.geometry_epoch()), (2, 2));
        assert!(read_mapping(&s.mapping()).is_ok());
    }

    #[test]
    fn no_update_returns_the_last_frame_with_a_new_sequence_number() {
        let mut s = state();
        assert!(s.unchanged(G, Duration::ZERO).is_none(), "no frame yet");
        let a = s.fresh(G, vec![7; 8], Duration::ZERO).unwrap();
        let b = s.unchanged(G, Duration::from_millis(5)).unwrap();
        assert_eq!(b.frame, a.frame);
        assert_eq!((b.frame_seq, b.captured_at), (2, Duration::from_millis(5)));
        // A DPI change without a new image: same pixels, new epoch.
        let scaled = Geometry { dpi: 144, ..G };
        let c = s.unchanged(scaled, Duration::ZERO).unwrap();
        assert_eq!(c.frame.pixels(), a.frame.pixels());
        assert_eq!(c.frame.geometry_epoch(), 2);
        // Another size needs a new image.
        let bigger = Geometry { size: (4, 4), ..G };
        assert!(s.unchanged(bigger, Duration::ZERO).is_none());
    }

    #[test]
    fn the_captured_frame_carries_the_monitor_scaling() {
        // RNF-07, ADR-0031: the DPI the output reports reaches the engine with the frame,
        // both for a new image and for the last frame returned again.
        let mut s = state();
        let at_150 = Geometry { dpi: 144, ..G };
        let a = s.fresh(at_150, vec![0; 8], Duration::ZERO).unwrap();
        assert_eq!(a.scaling, Scaling::Dpi(144));
        let at_175 = Geometry { dpi: 168, ..G };
        let b = s.unchanged(at_175, Duration::ZERO).unwrap();
        assert_eq!(b.scaling, Scaling::Dpi(168));
    }

    #[test]
    fn the_mapping_places_the_output_on_the_virtual_desktop() {
        let mut s = state();
        assert!(read_mapping(&s.mapping()).is_err(), "no frame yet");
        let secondary = Geometry {
            origin: (1920, -137),
            size: (2, 1),
            dpi: 96,
        };
        s.fresh(secondary, vec![0; 8], Duration::ZERO).unwrap();
        let m = read_mapping(&s.mapping()).unwrap();
        assert_eq!(m.to_os(Point::new(1, 0)), (1921, -137));
        s.out_of_profile("rotated");
        assert!(read_mapping(&s.mapping()).is_err());
    }
}
