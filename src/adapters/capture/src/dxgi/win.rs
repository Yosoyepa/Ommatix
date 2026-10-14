//! The Win32/COM half of the DXGI capturer: the only module of this crate that may use
//! `unsafe` (ADR-0016). Every unsafe block calls one `windows` binding and states why the
//! call is sound; raw pointers never leave this module.

use std::sync::Arc;
use std::time::Duration;

use ommatix_application::{CapturedFrame, Clock, FrameCapturer, PortError};
use ommatix_domain::DisplayMapping;
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
    DXGI_ERROR_NOT_FOUND, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_MOVE_RECT,
    IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    GetThreadDpiAwarenessContext, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::core::{HRESULT, Interface};

use super::{
    ADAPTER, DxgiDiagnostics, DxgiOptions, FrameState, Geometry, check_rotation, mapped_len,
    pack_rows, read_mapping,
};

/// How long the first capture after (re)creating the duplication waits for the desktop
/// image; DXGI normally hands the current image over at once.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_millis(500);

fn port(message: impl Into<String>) -> impl FnOnce(windows::core::Error) -> PortError {
    let message = message.into();
    move |e| PortError::new(ADAPTER, message).with_source(e)
}

/// Makes the process per-monitor-v2 DPI aware, so that DXGI, the desktop coordinates and
/// `SendInput` all speak physical pixels. Accepts an awareness already set to the same
/// value (by a manifest or an earlier call) and refuses any other.
fn ensure_dpi_awareness() -> Result<(), PortError> {
    // SAFETY: takes a predefined DPI_AWARENESS_CONTEXT constant by value; no pointers.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
    {
        return Ok(());
    }
    // SAFETY: no arguments; returns a pseudo-handle value owned by the system.
    let current = unsafe { GetThreadDpiAwarenessContext() };
    // SAFETY: compares two DPI_AWARENESS_CONTEXT values by value; no pointers are read.
    let same = unsafe {
        AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    };
    if same.as_bool() {
        Ok(())
    } else {
        Err(PortError::new(
            ADAPTER,
            "the process is not per-monitor-v2 DPI aware and cannot become so; frame pixels \
             would not match input coordinates",
        ))
    }
}

/// The `index`-th output attached to the desktop, across adapters, with its adapter.
fn find_output(index: u32) -> Result<(IDXGIAdapter1, IDXGIOutput), PortError> {
    // SAFETY: no arguments; returns an owned COM interface or an error.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(port("cannot create a DXGI factory"))?;
    let mut seen = 0u32;
    for a in 0u32.. {
        // SAFETY: an index argument; returns an owned interface or DXGI_ERROR_NOT_FOUND
        // past the last adapter.
        let adapter = match unsafe { factory.EnumAdapters1(a) } {
            Ok(adapter) => adapter,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(port("cannot enumerate display adapters")(e)),
        };
        for o in 0u32.. {
            // SAFETY: as above, for the outputs of a live adapter.
            let output = match unsafe { adapter.EnumOutputs(o) } {
                Ok(output) => output,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(port("cannot enumerate display outputs")(e)),
            };
            // SAFETY: returns the output's description by value.
            let desc = unsafe { output.GetDesc() }.map_err(port("cannot describe an output"))?;
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            if seen == index {
                return Ok((adapter, output));
            }
            seen += 1;
        }
    }
    Err(PortError::new(
        ADAPTER,
        format!("there is no output {index}: {seen} output(s) are attached to the desktop"),
    ))
}

/// Releases the acquired desktop image when dropped.
struct HeldFrame(IDXGIOutputDuplication);

impl Drop for HeldFrame {
    fn drop(&mut self) {
        // SAFETY: a HeldFrame exists only after AcquireNextFrame succeeded on this
        // duplication, so ReleaseFrame is its matching call; it takes no pointers. A
        // failure means the duplication is lost, which the next acquire reports.
        let _ = unsafe { self.0.ReleaseFrame() };
    }
}

/// Unmaps a mapped staging texture when dropped.
struct MappedTexture<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}

impl Drop for MappedTexture<'_> {
    fn drop(&mut self) {
        // SAFETY: a MappedTexture exists only after Map succeeded on subresource 0 of this
        // texture through this context; Unmap is its matching call.
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

/// One Desktop Duplication session: lives until the duplication is lost.
struct Session {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    duplication: IDXGIOutputDuplication,
    /// CPU-readable copy target, recreated when the image size changes.
    staging: Option<(ID3D11Texture2D, (u32, u32))>,
}

impl Session {
    fn open(index: u32) -> Result<Self, PortError> {
        let (adapter, output) = find_output(index)?;
        let mut device = None;
        let mut context = None;
        // SAFETY: `adapter` is a live COM interface; the out-pointers point to locals that
        // outlive the call; with an explicit adapter the driver type must be UNKNOWN and
        // the software module null.
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&raw mut device),
                None,
                Some(&raw mut context),
            )
        }
        .map_err(port(
            "cannot create a Direct3D 11 device on the output's adapter",
        ))?;
        let (Some(device), Some(context)) = (device, context) else {
            return Err(PortError::new(ADAPTER, "Direct3D 11 returned no device"));
        };
        let output: IDXGIOutput1 = output
            .cast()
            .map_err(port("the output does not support Desktop Duplication"))?;
        // SAFETY: `device` is a live Direct3D 11 device created on the output's adapter.
        let duplication = unsafe { output.DuplicateOutput(&device) }.map_err(port(
            "cannot duplicate the output (another duplication, a secure desktop or a \
             protected session may hold it)",
        ))?;
        Ok(Self {
            device,
            context,
            output,
            duplication,
            staging: None,
        })
    }

    /// The output's current geometry; an error when it is out of profile.
    fn geometry(&self) -> Result<Geometry, PortError> {
        // SAFETY: returns the output's description by value.
        let out = unsafe { self.output.GetDesc() }.map_err(port("cannot describe the output"))?;
        // SAFETY: returns the duplication's description by value.
        let dup = unsafe { self.duplication.GetDesc() };
        check_rotation(dup.Rotation.0)?;
        if dup.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(PortError::new(
                ADAPTER,
                format!(
                    "the desktop image is not BGRA8 (DXGI format {}); HDR and wide-colour \
                     desktops are not supported",
                    dup.ModeDesc.Format.0
                ),
            ));
        }
        let size = (dup.ModeDesc.Width, dup.ModeDesc.Height);
        let RECT {
            left,
            top,
            right,
            bottom,
        } = out.DesktopCoordinates;
        let desktop = (
            u32::try_from(i64::from(right) - i64::from(left)).unwrap_or(0),
            u32::try_from(i64::from(bottom) - i64::from(top)).unwrap_or(0),
        );
        if desktop != size {
            return Err(PortError::new(
                ADAPTER,
                format!(
                    "the output covers {}x{} desktop pixels but its image is {}x{}; desktop \
                     coordinates are not physical pixels",
                    desktop.0, desktop.1, size.0, size.1
                ),
            ));
        }
        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        // SAFETY: the HMONITOR comes from the output's description; the out-pointers point
        // to locals that outlive the call.
        unsafe {
            GetDpiForMonitor(
                out.Monitor,
                MDT_EFFECTIVE_DPI,
                &raw mut dpi_x,
                &raw mut dpi_y,
            )
        }
        .map_err(port("cannot read the monitor's DPI"))?;
        Ok(Geometry {
            origin: (left, top),
            size,
            dpi: dpi_x,
        })
    }

    /// The next desktop image, or `None` when nothing changed within `timeout`.
    fn acquire(
        &self,
        timeout: Duration,
    ) -> Result<Option<(DXGI_OUTDUPL_FRAME_INFO, IDXGIResource, HeldFrame)>, windows::core::Error>
    {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the out-pointers point to locals that outlive the call; on success a
        // frame is held and HeldFrame releases it.
        match unsafe {
            self.duplication
                .AcquireNextFrame(ms, &raw mut info, &raw mut resource)
        } {
            Ok(()) => {
                let held = HeldFrame(self.duplication.clone());
                match resource {
                    Some(resource) => Ok(Some((info, resource, held))),
                    None => Err(windows::core::Error::new(
                        DXGI_ERROR_ACCESS_LOST,
                        "DXGI returned no desktop resource",
                    )),
                }
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Counts of the move and dirty rectangles of the held frame, for diagnostics.
    fn metadata_counts(&self, info: &DXGI_OUTDUPL_FRAME_INFO) -> (u32, u32) {
        let total = info.TotalMetadataBufferSize as usize;
        if total == 0 {
            return (0, 0);
        }
        let move_size = size_of::<DXGI_OUTDUPL_MOVE_RECT>();
        let mut moves = vec![DXGI_OUTDUPL_MOVE_RECT::default(); total / move_size + 1];
        let mut required = 0u32;
        let bytes = u32::try_from(moves.len() * move_size).unwrap_or(0);
        // SAFETY: `moves` is valid for `bytes` bytes of writes and `required` is a local;
        // a frame is held, as GetFrameMoveRects requires.
        let move_count = unsafe {
            self.duplication
                .GetFrameMoveRects(bytes, moves.as_mut_ptr(), &raw mut required)
        }
        .map_or(0, |()| required as usize / move_size);
        let rect_size = size_of::<RECT>();
        let mut dirty = vec![RECT::default(); total / rect_size + 1];
        let bytes = u32::try_from(dirty.len() * rect_size).unwrap_or(0);
        // SAFETY: as above, for `dirty`.
        let dirty_count = unsafe {
            self.duplication
                .GetFrameDirtyRects(bytes, dirty.as_mut_ptr(), &raw mut required)
        }
        .map_or(0, |()| required as usize / rect_size);
        (
            u32::try_from(dirty_count).unwrap_or(u32::MAX),
            u32::try_from(move_count).unwrap_or(u32::MAX),
        )
    }

    /// A CPU-readable texture of `size`, created once per image size.
    fn staging(&mut self, source: &D3D11_TEXTURE2D_DESC) -> Result<ID3D11Texture2D, PortError> {
        let size = (source.Width, source.Height);
        if let Some((texture, s)) = &self.staging
            && *s == size
        {
            return Ok(texture.clone());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: source.Width,
            Height: source.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: source.Format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: `desc` is a valid local description; no initial data; the out-pointer
        // points to a local that outlives the call.
        unsafe {
            self.device
                .CreateTexture2D(&raw const desc, None, Some(&raw mut texture))
        }
        .map_err(port("cannot create the staging texture"))?;
        let texture =
            texture.ok_or_else(|| PortError::new(ADAPTER, "Direct3D returned no texture"))?;
        self.staging = Some((texture.clone(), size));
        Ok(texture)
    }

    /// Copies the held desktop image into packed BGRA rows, releasing the frame as soon
    /// as the GPU copy is complete.
    fn copy(
        &mut self,
        resource: &IDXGIResource,
        size: (u32, u32),
        held: HeldFrame,
    ) -> Result<Vec<u8>, PortError> {
        let texture: ID3D11Texture2D = resource
            .cast()
            .map_err(port("the desktop image is not a texture"))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: the out-pointer points to a local that outlives the call.
        unsafe { texture.GetDesc(&raw mut desc) };
        if (desc.Width, desc.Height) != size || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(PortError::new(
                ADAPTER,
                "the desktop image does not match the output's mode",
            ));
        }
        let staging = self.staging(&desc)?;
        // SAFETY: both textures belong to this device and share size and format; the
        // destination is a staging texture, as CopyResource requires.
        unsafe { self.context.CopyResource(&staging, &texture) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: `staging` is a CPU-readable staging texture of this context's device
        // with one subresource; the out-pointer points to a local. Map waits for the copy.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped))
        }
        .map_err(port("cannot map the staging texture"))?;
        let unmap = MappedTexture {
            context: &self.context,
            texture: &staging,
        };
        // The copy is complete: give the desktop image back to DXGI now.
        drop(held);
        let pitch = mapped.RowPitch as usize;
        let len = mapped_len(pitch, size.0, size.1)
            .ok_or_else(|| PortError::new(ADAPTER, "the mapped image has an invalid pitch"))?;
        if mapped.pData.is_null() {
            return Err(PortError::new(ADAPTER, "the mapped image has no data"));
        }
        // SAFETY: Map succeeded, so `pData` points to subresource 0 of a `size` BGRA8
        // texture: `size.1` rows `RowPitch` bytes apart, readable until Unmap. `len` covers
        // exactly the last row's pixels and no more. The slice is dropped before `unmap`.
        let src = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), len) };
        let pixels = pack_rows(src, pitch, size.0, size.1);
        drop(unmap);
        pixels.ok_or_else(|| PortError::new(ADAPTER, "the mapped image is too small"))
    }
}

/// Errors after which the duplication must be recreated.
fn is_lost(code: HRESULT) -> bool {
    [
        DXGI_ERROR_ACCESS_LOST,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
    ]
    .contains(&code)
}

enum Failure {
    /// The duplication was lost: recreate it once.
    Lost(windows::core::Error),
    Port(PortError),
}

/// Captures one monitor through DXGI Desktop Duplication, as packed BGRA frames in
/// physical pixels.
///
/// - The process becomes per-monitor-v2 DPI aware, so frames, desktop coordinates and
///   `SendInput` share physical pixels.
/// - Each capture waits at most [`DxgiOptions::acquire_timeout`] for a desktop update.
///   With none, the screen has not changed since the last frame, which is returned again
///   with a new `frame_seq` (the first capture after opening waits up to 500 ms for the
///   desktop image instead, and fails if none arrives).
/// - A lost duplication (mode change, secure desktop, ...) is recreated once within the
///   capture, and the next frame starts a new geometry epoch, as does any change of size,
///   origin or DPI. A rotated output or a non-BGRA8 desktop is out of profile: an error.
/// - Dirty and move rectangles are only counted ([`DxgiCapturer::diagnostics`]); the
///   engine's own differ decides what changed.
///
/// Pixels stay in memory; nothing is written to disk or logs.
pub struct DxgiCapturer {
    options: DxgiOptions,
    session: Option<Session>,
    state: FrameState,
    diagnostics: DxgiDiagnostics,
}

impl std::fmt::Debug for DxgiCapturer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DxgiCapturer")
            .field("options", &self.options)
            .field("open", &self.session.is_some())
            .field("state", &self.state)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl DxgiCapturer {
    /// Opens the duplication of `options.output_index`. `clock` stamps each capture; it
    /// must be the engine's monotonic clock.
    pub fn open(options: DxgiOptions, clock: Arc<dyn Clock>) -> Result<Self, PortError> {
        ensure_dpi_awareness()?;
        let session = Session::open(options.output_index)?;
        session.geometry()?;
        Ok(Self {
            options,
            session: Some(session),
            state: FrameState::new(clock),
            diagnostics: DxgiDiagnostics::default(),
        })
    }

    /// Counters about the duplication, for diagnostics.
    pub fn diagnostics(&self) -> DxgiDiagnostics {
        self.diagnostics
    }

    /// How the last frame's pixels map to virtual-desktop coordinates (the output's
    /// origin). An error before the first frame, after the duplication was lost and while
    /// the output is out of profile: input must never guess where a click lands.
    pub fn display_mapping(&self) -> Result<DisplayMapping, PortError> {
        read_mapping(&self.state.mapping())
    }

    /// A live view of [`DxgiCapturer::display_mapping`] for the input adapter.
    pub fn mapping_source(
        &self,
    ) -> impl Fn() -> Result<DisplayMapping, PortError> + Send + Sync + 'static {
        let mapping = self.state.mapping();
        move || read_mapping(&mapping)
    }

    fn lose_session(&mut self) {
        self.session = None;
        self.state.session_lost();
    }

    fn capture_once(&mut self) -> Result<CapturedFrame, Failure> {
        if self.session.is_none() {
            self.session = Some(Session::open(self.options.output_index).map_err(Failure::Port)?);
        }
        let Self {
            options,
            session: Some(session),
            state,
            diagnostics,
        } = self
        else {
            return Err(Failure::Port(PortError::new(ADAPTER, "no duplication")));
        };
        let geometry = match session.geometry() {
            Ok(geometry) => geometry,
            Err(e) => {
                state.out_of_profile(&e.message);
                return Err(Failure::Port(e));
            }
        };
        let timeout = if state.has_frame_of(geometry.size) {
            options.acquire_timeout
        } else {
            FIRST_FRAME_TIMEOUT
        };
        let acquired = session.acquire(timeout).map_err(|e| {
            if is_lost(e.code()) {
                Failure::Lost(e)
            } else {
                Failure::Port(port("cannot acquire the desktop image")(e))
            }
        })?;
        let captured_at = state.now();
        let Some((info, resource, held)) = acquired else {
            diagnostics.unchanged += 1;
            return state.unchanged(geometry, captured_at).ok_or_else(|| {
                Failure::Port(PortError::new(
                    ADAPTER,
                    "no desktop image arrived from the duplication",
                ))
            });
        };
        let (dirty, moves) = session.metadata_counts(&info);
        diagnostics.last_dirty_rects = dirty;
        diagnostics.last_move_rects = moves;
        diagnostics.protected_content_masked = info.ProtectedContentMaskedOut.as_bool();
        // Only the pointer moved: the desktop image is the last one.
        if info.LastPresentTime == 0
            && let Some(frame) = state.unchanged(geometry, captured_at)
        {
            drop(held);
            diagnostics.unchanged += 1;
            return Ok(frame);
        }
        let pixels = session
            .copy(&resource, geometry.size, held)
            .map_err(Failure::Port)?;
        diagnostics.frames_acquired += 1;
        state
            .fresh(geometry, pixels, captured_at)
            .map_err(Failure::Port)
    }
}

impl FrameCapturer for DxgiCapturer {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        match self.capture_once() {
            Ok(frame) => Ok(frame),
            Err(Failure::Port(e)) => Err(e),
            Err(Failure::Lost(_)) => {
                // Recreate the duplication once; the next frame starts a new epoch.
                self.diagnostics.access_lost += 1;
                self.lose_session();
                match self.capture_once() {
                    Ok(frame) => Ok(frame),
                    Err(Failure::Port(e)) => Err(e),
                    Err(Failure::Lost(e)) => {
                        self.diagnostics.access_lost += 1;
                        self.lose_session();
                        Err(port("the desktop duplication was lost twice; try again")(e))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ommatix_application::ManualClock;

    use super::*;

    /// Manual check on a real desktop: reads one frame of output 0 into memory and checks
    /// its shape. The pixels are never saved. Run with
    /// `cargo test -p ommatix-adapter-capture --features dxgi -- --ignored`.
    #[test]
    #[ignore = "needs an interactive Windows desktop"]
    fn dxgi_captures_one_frame_of_output_0() {
        let mut dxgi = DxgiCapturer::open(DxgiOptions::default(), Arc::new(ManualClock::new()))
            .expect("output 0 can be duplicated");
        let captured = dxgi.capture().expect("one frame");
        let frame = &captured.frame;
        assert!(frame.width() > 0 && frame.height() > 0);
        assert_eq!(frame.row_stride(), 4 * frame.width() as usize);
        assert_eq!(
            frame.pixels().len(),
            frame.row_stride() * frame.height() as usize
        );
        assert_eq!((captured.frame_seq, frame.geometry_epoch()), (1, 1));
        let mapping = dxgi.display_mapping().expect("a mapping after a frame");
        eprintln!(
            "dxgi output 0: {}x{} stride {} epoch {} mapping {mapping:?} {:?}",
            frame.width(),
            frame.height(),
            frame.row_stride(),
            frame.geometry_epoch(),
            dxgi.diagnostics()
        );
        let again = dxgi.capture().expect("a second frame");
        assert_eq!(again.frame_seq, 2);
        assert_eq!(
            (again.frame.width(), again.frame.height()),
            (frame.width(), frame.height())
        );
    }
}
