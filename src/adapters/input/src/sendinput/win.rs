//! The Win32 half of the `SendInput` actuator: the only module of this crate that may use
//! `unsafe` (ADR-0016). Every unsafe block calls one `windows` binding (or reads what it
//! just returned) and states why that is sound; raw pointers never leave this module.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use ommatix_application::contract::{ActionKind, ValidatedAction};
use ommatix_application::{DeliveryReceipt, OsActuator, PortError, TopLevelWindow};
use ommatix_domain::DisplayMapping;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, POINT};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GA_ROOT, GUITHREADINFO, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetSystemMetrics, GetWindowThreadProcessId, IsChild, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WindowFromPoint,
};
use windows::core::{PCWSTR, PWSTR};

use super::{
    ADAPTER, Event, FocusFacts, HOSTED_CONTENT_CLASS, MUST_BE_UP, TopLevel, VirtualScreen, batch,
    exe_name_utf16, first_held, focus_problem, integrity_allows, is_frame_host, keys_problem,
    normalise, point_application, shown_application, split_keys,
};
use crate::aim::frame_pixel;

type MappingSource = Box<dyn Fn() -> Result<DisplayMapping, PortError> + Send + Sync>;

/// Longest process image path read, in UTF-16 units (the extended-length path limit).
const MAX_PATH_UNITS: usize = 32_768;

/// How often, and how many times, `type` reads keyboard focus while the application
/// handles the focusing click (input is processed asynchronously): at most 200 ms.
const FOCUS_POLL: Duration = Duration::from_millis(10);
const FOCUS_POLLS: u32 = 20;

fn refuse(message: impl Into<String>) -> PortError {
    PortError::new(ADAPTER, message)
}

/// Makes the process per-monitor-v2 DPI aware (the DXGI capturer does the same), so that
/// virtual-desktop metrics and absolute input are physical pixels. Accepts an awareness
/// already set to that value and refuses any other.
fn ensure_dpi_awareness() -> Result<(), PortError> {
    // SAFETY: takes a predefined DPI_AWARENESS_CONTEXT constant by value; no pointers.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
    {
        return Ok(());
    }
    // SAFETY: no arguments; returns a context value owned by the system.
    let current = unsafe { GetThreadDpiAwarenessContext() };
    // SAFETY: compares two DPI_AWARENESS_CONTEXT values by value; no pointers are read.
    let same = unsafe {
        AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    };
    if same.as_bool() {
        Ok(())
    } else {
        Err(refuse(
            "the process is not per-monitor-v2 DPI aware; input coordinates would not be \
             physical pixels",
        ))
    }
}

/// A process or token handle, closed when dropped.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned open by OpenProcess or OpenProcessToken, is owned
        // by this value alone and is closed exactly once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// The process owning `hwnd`; `None` for a null or stale window.
fn window_process(hwnd: HWND) -> Option<u32> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `hwnd` is passed by value (a stale handle makes the call return 0, not fault);
    // the out-pointer points to a local that outlives the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    (pid != 0).then_some(pid)
}

/// Longest window class name read, in UTF-16 units (class names are at most 256).
const MAX_CLASS_UNITS: usize = 257;

/// The window class name of `hwnd`; `None` for a stale window.
fn class_name(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; MAX_CLASS_UNITS];
    // SAFETY: `hwnd` is passed by value (a stale handle makes the call return 0); the slice
    // is a local valid for its length of writes, which the binding passes as the maximum.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let len = usize::try_from(len).ok().filter(|&n| n > 0)?;
    String::from_utf16(buffer.get(..len)?).ok()
}

/// The executable name of the process owning `hwnd`, lowercase.
fn window_exe_name(hwnd: HWND) -> Option<String> {
    process_exe_name(&open_process(window_process(hwnd)?)?)
}

/// The hosted content window (`Windows.UI.Core.CoreWindow`) inside a frame host's frame.
fn hosted_content(frame: HWND) -> Option<HWND> {
    let class: Vec<u16> = HOSTED_CONTENT_CLASS
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `frame` is passed by value (a stale handle makes the call fail); `class` is a
    // NUL-terminated UTF-16 string alive for the whole call and not kept; the window name
    // is null (any name). Only direct children of `frame` are searched.
    let found = unsafe { FindWindowExW(Some(frame), None, PCWSTR(class.as_ptr()), PCWSTR::null()) };
    found.ok().filter(|hwnd| !hwnd.is_invalid())
}

/// What decides which application the top-level window `top` shows (see
/// [`shown_application`]), and its hosted content window when it is a frame host.
fn top_level(top: HWND) -> Option<(TopLevel, Option<HWND>)> {
    let owner = window_process(top)?;
    let frame_host = is_frame_host(
        &class_name(top).unwrap_or_default(),
        window_exe_name(top).as_deref(),
    );
    let content = if frame_host {
        hosted_content(top)
    } else {
        None
    };
    let hosted = content.and_then(window_process);
    Some((
        TopLevel {
            owner,
            frame_host,
            hosted,
        },
        content,
    ))
}

/// The application of the foreground window and the window that stands for it (the
/// hosted content inside a frame host, otherwise the foreground window itself).
fn foreground_application_window() -> Option<(u32, HWND)> {
    // SAFETY: no arguments; returns a window handle value (null when none is active).
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() {
        return None;
    }
    let (top, content) = top_level(foreground)?;
    let pid = shown_application(top)?;
    Some((pid, content.unwrap_or(foreground)))
}

/// The process of the application in the foreground, if there is one. For a hosted
/// application (the Windows 11 Calculator) it is the hosted process, never
/// `ApplicationFrameHost.exe`.
fn foreground_process() -> Option<u32> {
    foreground_application_window().map(|(pid, _)| pid)
}

/// The window at virtual-desktop pixel `(x, y)` (the deepest visible child there).
fn window_at((x, y): (i32, i32)) -> Option<HWND> {
    // SAFETY: takes a POINT by value; returns a window handle value or null.
    let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
    (!hwnd.is_invalid()).then_some(hwnd)
}

/// What [`focus_problem`] needs, read now: the foreground process, and the window with
/// keyboard focus in the foreground thread, its process and whether it is the window at
/// `desktop` or one of its ancestors.
fn focus_facts(attested: u32, desktop: (i32, i32)) -> FocusFacts {
    let app = foreground_application_window();
    let mut facts = FocusFacts {
        attested,
        foreground: app.map(|(pid, _)| pid),
        focus_owner: None,
        focus_holds_target: false,
    };
    let Some((_, foreground)) = app else {
        return facts;
    };
    // SAFETY: `foreground` is passed by value (a stale handle returns 0); no out-pointer.
    let thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    if thread == 0 {
        return facts;
    }
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(size_of::<GUITHREADINFO>()).unwrap_or(u32::MAX),
        ..GUITHREADINFO::default()
    };
    // SAFETY: `info` is a local GUITHREADINFO whose `cbSize` is its size, as the call
    // requires; the pointer is valid for the call and not kept.
    if unsafe { GetGUIThreadInfo(thread, &raw mut info) }.is_err() {
        return facts;
    }
    let focus = info.hwndFocus;
    facts.focus_owner = window_process(focus);
    if let (Some(target), Some(_)) = (window_at(desktop), facts.focus_owner) {
        // SAFETY: two window handle values by value; a stale one makes the call return
        // false, which fails the check.
        let contains = unsafe { IsChild(focus, target) }.as_bool();
        facts.focus_holds_target = focus == target || contains;
    }
    facts
}

/// The top-level window that contains `hwnd` (the window itself when it is one).
fn root_window(hwnd: HWND) -> HWND {
    // SAFETY: `hwnd` is passed by value; a stale handle yields null, handled below.
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.is_invalid() { hwnd } else { root }
}

/// What the operator's window scope needs to know about a top-level window: its class and
/// its handle (ADR-0020). Never its title.
fn attest_window(hwnd: HWND) -> Option<TopLevelWindow> {
    Some(TopLevelWindow {
        class: class_name(hwnd)?,
        handle: top_level_handle(hwnd)?,
    })
}

/// The handle as the number the operator writes in the pin file.
fn top_level_handle(hwnd: HWND) -> Option<u64> {
    let handle = u64::try_from(hwnd.0.addr()).ok()?;
    (handle != 0).then_some(handle)
}

/// The application whose window is at virtual-desktop pixel `(x, y)`: the process of the
/// top-level window there or, inside a frame host, of its hosted content, and none on the
/// frame host's own windows (see [`point_application`]).
fn process_at(desktop: (i32, i32)) -> Option<u32> {
    let hwnd = window_at(desktop)?;
    let root = root_window(hwnd);
    let (top, content) = top_level(root)?;
    let in_content = content.is_some_and(|content| {
        // SAFETY: two window handle values by value; a stale one makes the call return
        // false, which refuses the point.
        content == hwnd || unsafe { IsChild(content, hwnd) }.as_bool()
    });
    point_application(top, in_content)
}

fn open_process(pid: u32) -> Option<OwnedHandle> {
    // SAFETY: plain values; on success the returned handle is owned by OwnedHandle.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .ok()
        .map(OwnedHandle)
}

/// The executable file name of `process`, lowercase.
fn process_exe_name(process: &OwnedHandle) -> Option<String> {
    let mut buffer = vec![0u16; MAX_PATH_UNITS];
    let mut len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is valid for `len` UTF-16 units of writes and `len` is a local the
    // call updates to the units written (without the terminator).
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &raw mut len,
        )
    }
    .ok()?;
    exe_name_utf16(buffer.get(..len as usize)?)
}

/// The mandatory integrity level (the label's last RID) of `process`'s token.
fn process_integrity(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: `process` is an open process handle (or the current-process pseudo-handle)
    // with query rights; the out-pointer points to a local. The token is owned below.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.ok()?;
    let token = OwnedHandle(token);
    let mut needed = 0u32;
    // SAFETY: a size query: no buffer and length 0; the out-pointer points to a local.
    let _ = unsafe { GetTokenInformation(token.0, TokenIntegrityLevel, None, 0, &raw mut needed) };
    if needed == 0 || needed > 1024 {
        return None;
    }
    // u64 elements: 8-byte alignment, enough for TOKEN_MANDATORY_LABEL and the SID after it.
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    let len = needed;
    // SAFETY: `buffer` is valid for `len` bytes of writes; the out-pointer is a local.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            len,
            &raw mut needed,
        )
    }
    .ok()?;
    // SAFETY: on success the buffer, aligned for it, starts with a TOKEN_MANDATORY_LABEL.
    let label = unsafe { buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>().read() };
    let sid = label.Label.Sid;
    if sid.0.is_null() {
        return None;
    }
    // SAFETY: `sid` points to a valid SID inside `buffer`, which is alive; the returned
    // pointer to its sub-authority count is read at once.
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    let last = u32::from(count.checked_sub(1)?);
    // SAFETY: `last` is below the SID's sub-authority count, so the returned pointer is
    // inside the SID; it is read at once, while `buffer` is alive.
    Some(unsafe { *GetSidSubAuthority(sid, last) })
}

fn virtual_screen() -> Option<VirtualScreen> {
    // SAFETY: plain metric indices; no pointers.
    let metric = |index| unsafe { GetSystemMetrics(index) };
    let screen = VirtualScreen {
        x: metric(SM_XVIRTUALSCREEN),
        y: metric(SM_YVIRTUALSCREEN),
        width: metric(SM_CXVIRTUALSCREEN),
        height: metric(SM_CYVIRTUALSCREEN),
    };
    (screen.width > 0 && screen.height > 0).then_some(screen)
}

/// The first of [`MUST_BE_UP`] that is held down now.
fn held_key() -> Option<&'static str> {
    first_held(MUST_BE_UP.iter().map(|&(vk, name)| {
        // SAFETY: a virtual-key code by value; no pointers.
        (name, unsafe { GetAsyncKeyState(i32::from(vk)) })
    }))
}

fn mouse(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn to_input(event: Event) -> INPUT {
    match event {
        Event::MoveTo { dx, dy } => mouse(
            dx,
            dy,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ),
        Event::LeftDown => mouse(0, 0, 0, MOUSEEVENTF_LEFTDOWN),
        Event::LeftUp => mouse(0, 0, 0, MOUSEEVENTF_LEFTUP),
        Event::Wheel(delta) => mouse(0, 0, delta.cast_unsigned(), MOUSEEVENTF_WHEEL),
        Event::Unicode { unit, up } => INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: if up {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    } else {
                        KEYEVENTF_UNICODE | KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    }
}

/// Sends real mouse and keyboard events through Win32 `SendInput`, aimed through the DXGI
/// capturer's mapping (physical pixels of the selected monitor on the virtual desktop).
///
/// Before anything is sent, and failing closed with an error (the engine then reports the
/// action as indeterminate, never retried):
///
/// - the foreground process must be the one [`OsActuator::foreground_application`] just
///   attested to the engine's allowlist check, and the window at the target point must
///   belong to it;
/// - its integrity level must not be above ours (UIPI would drop the input silently while
///   `SendInput` still counted it as inserted);
/// - no modifier key or mouse button may be held down.
///
/// A hosted application (the Windows 11 Calculator) is drawn inside a frame owned by
/// `ApplicationFrameHost.exe`. The actuator attests the hosted content's process
/// (`CalculatorApp.exe`), never the frame host (allowlisting it would allow every hosted
/// application), and a point on the frame's own windows (title bar, caption buttons)
/// belongs to no application and is refused.
///
/// `type` verifies keyboard focus between its focusing click and its keys (SDD §2.9):
/// after the click, the foreground process must still be the attested one and the window
/// with keyboard focus must belong to it and be the window at the target point (or contain
/// it). Otherwise no key is sent and `deliver` fails; the click was sent, so the engine
/// reports the action as `indeterminate`, never as not delivered.
///
/// For the operator's scope on windows (ADR-0020) the actuator also attests, and only
/// attests, the top-level foreground window and the top-level window at a frame point,
/// each as its class name and handle; the engine decides, at the gate and again right
/// before delivery. Neither title nor text is read.
///
/// A click or a scroll is one `SendInput` call; `type` is two, the click and the keys. The
/// receipt's `submitted` is the batch length and `inserted` what `SendInput` returned (it
/// stops at the first event it cannot insert). Nothing is retried.
///
/// With accessibility evidence the engine owns the pause between the two calls instead
/// (ADR-0018): `deliver_focus_click` sends the pointer events and `deliver_keys` the keys,
/// and the engine sends the second only once UI Automation reports keyboard focus on the
/// target element. The keys phase still checks that the attested application is in the
/// foreground and owns the focused window and that no modifier is held, and it refuses
/// without the matching click.
pub struct SendInputActuator {
    mapping: MappingSource,
    /// The integrity RID of this process.
    own_integrity: u32,
    /// The foreground process last attested to the engine; consumed by the next delivery.
    attested: Mutex<Option<u32>>,
    /// The focusing click of a `type` whose keys are still to come.
    pending: Mutex<Option<PendingKeys>>,
}

/// Where an action is aimed.
struct Aim {
    /// The virtual-desktop pixel.
    desktop: (i32, i32),
    /// Its normalised absolute coordinates.
    at: (i32, i32),
}

/// What the keys phase needs from the focusing click that preceded it.
struct PendingKeys {
    /// The action whose click was delivered; the keys must belong to the same one.
    action: ValidatedAction,
    /// The foreground process the click was checked against.
    foreground: u32,
    /// The virtual-desktop pixel that was clicked.
    desktop: (i32, i32),
}

impl std::fmt::Debug for SendInputActuator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendInputActuator")
            .field("own_integrity", &self.own_integrity)
            .finish_non_exhaustive()
    }
}

impl SendInputActuator {
    /// Reads the mapping before every action (`DxgiCapturer::mapping_source` in the
    /// capture adapter), so input fails closed while the capture is lost or out of
    /// profile.
    pub fn with_mapping_source(
        mapping: impl Fn() -> Result<DisplayMapping, PortError> + Send + Sync + 'static,
    ) -> Result<Self, PortError> {
        ensure_dpi_awareness()?;
        // SAFETY: no arguments; returns the current-process pseudo-handle, which needs no
        // closing.
        let own = process_integrity(unsafe { GetCurrentProcess() })
            .ok_or_else(|| refuse("cannot read this process's integrity level"))?;
        Ok(Self {
            mapping: Box::new(mapping),
            own_integrity: own,
            attested: Mutex::new(None),
            pending: Mutex::new(None),
        })
    }

    /// The virtual-desktop pixel of the action point and its normalised absolute
    /// coordinates.
    fn aim(&self, action: &ValidatedAction) -> Result<Aim, PortError> {
        let desktop = self.desktop_point(action.point())?;
        let screen =
            virtual_screen().ok_or_else(|| refuse("cannot read the virtual desktop size"))?;
        let at = normalise(desktop, screen)
            .ok_or_else(|| refuse("the action point is outside the virtual desktop"))?;
        Ok(Aim { desktop, at })
    }

    /// The virtual-desktop pixel of a frame point, through the capture's current mapping.
    fn desktop_point(&self, point: (f64, f64)) -> Result<(i32, i32), PortError> {
        let mapping = (self.mapping)()?;
        let pixel =
            frame_pixel(point).ok_or_else(|| refuse("the action point is not a frame pixel"))?;
        Ok(mapping.to_os(pixel))
    }

    /// Every check that must pass before the batch is sent; returns the foreground
    /// process it checked.
    fn preflight(&self, desktop: (i32, i32)) -> Result<u32, PortError> {
        let attested = self
            .attested
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let foreground = foreground_process().ok_or_else(|| refuse("no foreground window"))?;
        if attested != Some(foreground) {
            return Err(refuse(
                "the foreground application changed after it was checked; nothing was sent",
            ));
        }
        if process_at(desktop) != Some(foreground) {
            return Err(refuse(
                "the target point is not on a window of the foreground application; nothing \
                 was sent",
            ));
        }
        let integrity = open_process(foreground)
            .and_then(|p| process_integrity(p.0))
            .ok_or_else(|| {
                refuse(
                    "cannot read the foreground application's integrity level (it may be \
                     elevated); nothing was sent",
                )
            })?;
        if !integrity_allows(self.own_integrity, integrity) {
            return Err(refuse(
                "the foreground application runs at a higher integrity level (elevated); \
                 Windows would drop the input; nothing was sent",
            ));
        }
        if let Some(key) = held_key() {
            return Err(refuse(format!("{key} is held down; nothing was sent")));
        }
        Ok(foreground)
    }

    /// Waits, briefly, for the focusing click to give keyboard focus to the target field
    /// of `attested`; fails with the last problem found.
    fn await_focus(attested: u32, desktop: (i32, i32)) -> Result<(), PortError> {
        let mut problem = None;
        for _ in 0..FOCUS_POLLS {
            problem = focus_problem(focus_facts(attested, desktop));
            if problem.is_none() {
                break;
            }
            std::thread::sleep(FOCUS_POLL);
        }
        if let Some(problem) = problem {
            return Err(refuse(format!("{problem}; the click was sent, no key was")));
        }
        if let Some(key) = held_key() {
            return Err(refuse(format!(
                "{key} is held down; the click was sent, no key was"
            )));
        }
        Ok(())
    }
}

/// Sends `events` in one `SendInput` call; returns how many the OS inserted.
fn send(events: Vec<Event>) -> u32 {
    if events.is_empty() {
        return 0;
    }
    let inputs: Vec<INPUT> = events.into_iter().map(to_input).collect();
    let size = i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX);
    // SAFETY: `inputs` is a slice of fully initialised INPUT values whose union member
    // matches their `type`, and `size` is the size of one INPUT, as SendInput requires.
    unsafe { SendInput(&inputs, size) }
}

impl OsActuator for SendInputActuator {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn is_live(&self) -> bool {
        true
    }

    fn foreground_application(&self) -> Option<String> {
        let pid = foreground_process();
        *self.attested.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let name = process_exe_name(&open_process(pid?)?)?;
        *self.attested.lock().unwrap_or_else(PoisonError::into_inner) = pid;
        Some(name)
    }

    fn foreground_window(&self) -> Option<TopLevelWindow> {
        // SAFETY: no arguments; returns a window handle value (null when none is active).
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_invalid() {
            return None;
        }
        attest_window(root_window(foreground))
    }

    fn window_at(&self, point: (f64, f64)) -> Option<TopLevelWindow> {
        let desktop = self.desktop_point(point).ok()?;
        attest_window(root_window(window_at(desktop)?))
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        let Aim { desktop, at } = self.aim(action)?;
        let foreground = self.preflight(desktop)?;
        let events = batch(action.kind(), at);
        let submitted = u32::try_from(events.len()).unwrap_or(u32::MAX);
        let (pointer, keys) = split_keys(events);
        let pointer_len = u32::try_from(pointer.len()).unwrap_or(u32::MAX);
        let mut inserted = send(pointer);
        if inserted == pointer_len && !keys.is_empty() {
            Self::await_focus(foreground, desktop)?;
            inserted = inserted.saturating_add(send(keys));
        }
        Ok(DeliveryReceipt {
            submitted,
            inserted,
        })
    }

    fn delivers_in_phases(&self) -> bool {
        true
    }

    fn deliver_focus_click(
        &mut self,
        action: &ValidatedAction,
    ) -> Result<DeliveryReceipt, PortError> {
        if !matches!(action.kind(), ActionKind::Type { .. }) {
            return Err(refuse("only type is delivered in phases; nothing was sent"));
        }
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let Aim { desktop, at } = self.aim(action)?;
        let foreground = self.preflight(desktop)?;
        let (pointer, keys) = split_keys(batch(action.kind(), at));
        let submitted = u32::try_from(pointer.len()).unwrap_or(u32::MAX);
        let inserted = send(pointer);
        if inserted == submitted && !keys.is_empty() {
            *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(PendingKeys {
                action: action.clone(),
                foreground,
                desktop,
            });
        }
        Ok(DeliveryReceipt {
            submitted,
            inserted,
        })
    }

    fn deliver_keys(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .filter(|pending| pending.action == *action)
            .ok_or_else(|| {
                refuse("no focusing click was delivered for this action; no key was sent")
            })?;
        if let Some(problem) = keys_problem(focus_facts(pending.foreground, pending.desktop)) {
            return Err(refuse(format!("{problem}; the click was sent, no key was")));
        }
        if let Some(key) = held_key() {
            return Err(refuse(format!(
                "{key} is held down; the click was sent, no key was"
            )));
        }
        let (_, keys) = split_keys(batch(action.kind(), (0, 0)));
        let submitted = u32::try_from(keys.len()).unwrap_or(u32::MAX);
        let inserted = send(keys);
        Ok(DeliveryReceipt {
            submitted,
            inserted,
        })
    }
}
