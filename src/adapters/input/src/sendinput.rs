//! Win32 `SendInput` actuator (Windows only, feature `sendinput`).
//!
//! This module holds the platform-neutral part, safe Rust that is unit-tested on every OS:
//! normalising virtual-desktop points for `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK`,
//! building an action's event batch, extracting the executable name of a process image
//! path and the integrity, modifier and keyboard-focus checks. The Win32 calls live in the
//! `#[cfg(windows)]` submodule `win`, the only place where this crate allows `unsafe`
//! (ADR-0016).
//!
//! The text of a `type` action is never logged.

#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows module calls these helpers")
)]

use std::time::Duration;

use ommatix_application::contract::{ActionKind, ScrollDirection};
use ommatix_application::{KeysStopCheck, KeysStopped, PortError, WindowOpacity};

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "ADR-0016: SendInput and the process/token queries are unsafe Win32 fns"
)]
mod win;

#[cfg(windows)]
pub use win::{SendInputActuator, WindowReader};

pub(crate) const ADAPTER: &str = "sendinput";

/// One wheel notch (`WHEEL_DELTA`).
pub(crate) const WHEEL_DELTA: i32 = 120;

/// Largest normalised absolute coordinate.
const NORMALISED_MAX: i64 = 65_535;

/// Bound an OS z-order walk; a cycle or incomplete enumeration attests nothing.
#[derive(Default)]
pub(crate) struct WindowWalk(std::collections::BTreeSet<u64>);

impl WindowWalk {
    pub fn visit(&mut self, handle: u64) -> bool {
        handle != 0 && self.0.len() < 4096 && self.0.insert(handle)
    }
}

/// ADR-0035: DWM draws the corners of a window transparently, so a target box within this many
/// physical pixels of two adjacent edges of the window (it meets the square of this size at a
/// corner) is not attested opaque. The radius DWM uses is 8 px at 100 % scaling; 32 px covers
/// scaling up to 400 %. It applies whatever corner preference the window reports.
pub(crate) const CORNER_EXCLUSION_PX: u32 = 32;

/// `WS_EX_LAYERED`: the window is composed through an alpha or colour key.
const WS_EX_LAYERED: u32 = 0x0008_0000;
/// `WS_EX_TRANSPARENT`: hit testing passes through the window.
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
/// `WS_EX_NOREDIRECTIONBITMAP`: the window has no redirection surface, so it can be composed
/// with per-pixel alpha by DirectComposition without being layered. Nothing else reliably
/// attests that such a window is opaque, so it never is.
const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

/// The extended style a `GetWindowLongPtrW(GWL_EXSTYLE)` call returned, given the thread's
/// last error read right after it. The call returns 0 both for a failure and for a
/// legitimate empty style: only a zero with a nonzero error is a failed query (`None`).
pub(crate) fn extended_style_of(value: isize, last_error: u32) -> Option<u32> {
    if value == 0 && last_error != 0 {
        return None;
    }
    // The style bits are the low 32 bits of the pointer-sized value.
    Some(u32::try_from(value & 0xFFFF_FFFF).unwrap_or_default())
}

/// What the actuator may attest about a window's opacity from the two facts it reads
/// (ADR-0035): its extended style (`None` when the query failed), and
/// `DWMWA_SYSTEMBACKDROP_TYPE` (`None` when the query failed). Layered, click-through and
/// no-redirection-bitmap windows, and any backdrop material other than `AUTO` (0, the default
/// of a window that never opted in) or `NONE` (1), are not attested. Legacy blur-behind has no
/// documented getter, so its absence is not attested (ADR-0035, implementation notes).
pub(crate) fn window_opacity_of(
    ex_style: Option<u32>,
    backdrop: Option<u32>,
) -> Option<WindowOpacity> {
    let opaque_style =
        ex_style? & (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOREDIRECTIONBITMAP) == 0;
    let no_backdrop = matches!(backdrop, Some(0 | 1));
    (opaque_style && no_backdrop).then_some(WindowOpacity {
        corner_exclusion_px: CORNER_EXCLUSION_PX,
    })
}

/// What asking for the next top-level sibling of a window answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sibling<H> {
    Found(H),
    /// The window is the last one: the call returned no window and no error.
    End,
    /// The call returned no window with an error: the enumeration is not complete.
    Failed,
}

/// Classifies the answer of `GetWindow(GW_HWNDNEXT)` from its result and the thread's last
/// error, which the caller cleared before the call and reads right after it.
pub(crate) fn sibling<H>(found: Option<H>, last_error: u32) -> Sibling<H> {
    match (found, last_error) {
        (Some(next), _) => Sibling::Found(next),
        (None, 0) => Sibling::End,
        (None, _) => Sibling::Failed,
    }
}

/// Walks the top-level windows front to back from `first`: bounded, cycle-checked, and
/// complete or nothing. `visit` collects one window (`None` aborts), `next` asks for its
/// sibling, and `exists` tells whether the cursor is still a window after that query. A failed
/// enumeration, a destroyed cursor, a null or repeated handle and an overlong walk all return
/// `None`; only the normal end returns `Some`.
pub(crate) fn walk_windows<H: Copy>(
    first: H,
    handle_of: impl Fn(H) -> Option<u64>,
    mut visit: impl FnMut(H) -> Option<()>,
    mut next: impl FnMut(H) -> Sibling<H>,
    exists: impl Fn(H) -> bool,
) -> Option<()> {
    let mut walk = WindowWalk::default();
    let mut cursor = first;
    loop {
        if !walk.visit(handle_of(cursor)?) {
            return None;
        }
        visit(cursor)?;
        let following = next(cursor);
        if !exists(cursor) {
            return None;
        }
        match following {
            Sibling::Found(found) => cursor = found,
            Sibling::End => return Some(()),
            Sibling::Failed => return None,
        }
    }
}

/// Visible DWM frame bounds in the capture's physical-pixel mapping. A window entirely
/// outside the selected monitor has no intersection and does not join its window list.
pub(crate) fn frame_bounds(
    mapping: ommatix_domain::DisplayMapping,
    rect: [i32; 4],
) -> Option<ommatix_domain::bbox::BBox> {
    mapping.frame_box(rect[0], rect[1], rect[2], rect[3])
}

/// One input event of a batch, before it becomes a Win32 `INPUT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// Absolute move over the virtual desktop, both axes normalised to 0..=65535.
    MoveTo {
        dx: i32,
        dy: i32,
    },
    LeftDown,
    LeftUp,
    /// One UTF-16 code unit (`KEYEVENTF_UNICODE`), pressed or released.
    Unicode {
        unit: u16,
        up: bool,
    },
    /// One wheel notch: positive away from the user (up), negative towards (down).
    Wheel(i32),
}

/// The bounding box of the virtual desktop (`SM_XVIRTUALSCREEN`, `SM_YVIRTUALSCREEN`,
/// `SM_CXVIRTUALSCREEN`, `SM_CYVIRTUALSCREEN`), in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtualScreen {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The normalised absolute coordinates of a virtual-desktop pixel, aimed at the pixel's
/// centre so that Windows' mapping back (`n * extent / 65536`, rounded down) lands on
/// it. `None` outside the virtual desktop.
pub(crate) fn normalise((x, y): (i32, i32), screen: VirtualScreen) -> Option<(i32, i32)> {
    let axis = |v: i32, origin: i32, extent: i32| -> Option<i32> {
        let extent = i64::from(extent);
        let offset = i64::from(v) - i64::from(origin);
        if extent <= 0 || offset < 0 || offset >= extent {
            return None;
        }
        let n = (2 * offset + 1) * 65_536 / (2 * extent);
        i32::try_from(n.min(NORMALISED_MAX)).ok()
    };
    Some((
        axis(x, screen.x, screen.width)?,
        axis(y, screen.y, screen.height)?,
    ))
}

/// The events of `kind` at normalised point `at`: a move, then a click (press, release), a
/// click that focuses the validated field followed by a press and a release per UTF-16
/// unit, or one wheel notch per step. A click or a scroll is injected in one `SendInput`
/// call; `type` is split by [`split_keys`] so that its keys wait for the focus check.
pub(crate) fn batch(kind: &ActionKind, (dx, dy): (i32, i32)) -> Vec<Event> {
    let mut events = vec![Event::MoveTo { dx, dy }];
    match kind {
        ActionKind::Click => events.extend([Event::LeftDown, Event::LeftUp]),
        ActionKind::Type { text } => {
            events.extend([Event::LeftDown, Event::LeftUp]);
            for unit in text.encode_utf16() {
                events.push(Event::Unicode { unit, up: false });
                events.push(Event::Unicode { unit, up: true });
            }
        }
        ActionKind::Scroll { direction, steps } => {
            let delta = match direction {
                ScrollDirection::Up => WHEEL_DELTA,
                ScrollDirection::Down => -WHEEL_DELTA,
            };
            events.extend(std::iter::repeat_n(
                Event::Wheel(delta),
                usize::from(*steps),
            ));
        }
    }
    events
}

/// Splits a batch before its first key event: the pointer events (the move and, for
/// `type`, the click that focuses the field) and the key events, which `type` sends only
/// once keyboard focus is verified (SDD §2.9). The second half is empty for a click or a
/// scroll.
pub(crate) fn split_keys(mut events: Vec<Event>) -> (Vec<Event>, Vec<Event>) {
    let first_key = events
        .iter()
        .position(|e| matches!(e, Event::Unicode { .. }))
        .unwrap_or(events.len());
    let keys = events.split_off(first_key);
    (events, keys)
}

/// Longest pause between two characters of a paced `type`, in milliseconds. Below one
/// millisecond Windows' timers give no meaning to a pause; above 50 ms a 29-character sentence
/// already takes 1.5 s, slower than any typist the application under test was built for
/// (50 ms is 20 characters a second, 240 words a minute is 20 as well).
pub const MAX_TYPE_KEY_PAUSE_MS: u64 = 50;

/// Longest extra pause after a word boundary, in milliseconds: one key-repeat delay (250 ms),
/// the longest the lab can want to wait for a text box to finish what a space or a comma
/// starts. [`MAX_PACED_TYPE`] still bounds the whole delivery.
pub const MAX_TYPE_BOUNDARY_PAUSE_MS: u64 = 250;

/// Longest time one paced `type` may hold the engine. The engine is single threaded, so the
/// delivery blocks every other call of the session; the operator's stop is read between two
/// characters, so the hold is interruptible within one pause, but a delivery that takes
/// minutes is not a typing aid. 30 s is the 2048 characters of the longest `type` at 14 ms
/// each, and a text of 600 characters at the largest key pause.
pub const MAX_PACED_TYPE: Duration = Duration::from_secs(30);

/// How the key events of a `type` are paced. Both pauses at zero (the default) are today's
/// behaviour: every key event in one `SendInput` call, no pause, no check between keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypePacing {
    /// The pause after every character that is followed by another.
    pub key_pause: Duration,
    /// An extra pause after a word boundary that is followed by more text.
    pub boundary_pause: Duration,
}

impl TypePacing {
    /// Pacing with the given pauses, refusing a pause above its bound.
    pub fn new(key_pause: Duration, boundary_pause: Duration) -> Result<Self, PortError> {
        if key_pause > Duration::from_millis(MAX_TYPE_KEY_PAUSE_MS) {
            return Err(PortError::new(
                ADAPTER,
                format!("the pause between keys is at most {MAX_TYPE_KEY_PAUSE_MS} ms"),
            ));
        }
        if boundary_pause > Duration::from_millis(MAX_TYPE_BOUNDARY_PAUSE_MS) {
            return Err(PortError::new(
                ADAPTER,
                format!(
                    "the pause after a word boundary is at most {MAX_TYPE_BOUNDARY_PAUSE_MS} ms"
                ),
            ));
        }
        Ok(Self {
            key_pause,
            boundary_pause,
        })
    }

    /// Whether both pauses are zero: the keys then go in one call, as before pacing existed.
    pub fn is_off(self) -> bool {
        self.key_pause.is_zero() && self.boundary_pause.is_zero()
    }

    /// The wait after the character `c` when another follows: the key pause and, after a
    /// word boundary, the boundary pause as well, as one sleep. `None` when the two add up
    /// to more than a `Duration` holds.
    fn pause_after(self, c: char) -> Option<Duration> {
        if is_word_boundary(c) {
            self.key_pause.checked_add(self.boundary_pause)
        } else {
            Some(self.key_pause)
        }
    }

    /// The time the pauses of a paced delivery of `text` add up to (the sends themselves
    /// are not counted). Checked arithmetic: `None` when the sum overflows, which is a
    /// refusal and never a saturated value that could pass for small.
    pub fn duration_of(self, text: &str) -> Option<Duration> {
        let mut total = Duration::ZERO;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if chars.peek().is_some() {
                total = total.checked_add(self.pause_after(c)?)?;
            }
        }
        Some(total)
    }

    /// Why a paced delivery of `text` is refused before anything is sent: its pauses add up to
    /// more than [`MAX_PACED_TYPE`], or overflow. `None` when it fits, and always with pacing
    /// off. The engine tells the agent this as a rejection (`LIMIT_EXCEEDED`).
    pub fn refusal(self, text: &str) -> Option<String> {
        if self.is_off() {
            return None;
        }
        match self.duration_of(text) {
            Some(total) if total <= MAX_PACED_TYPE => None,
            _ => Some(format!(
                "a paced type may take at most {} s with the pauses the operator configured; \
                 send less text at a time; nothing was sent",
                MAX_PACED_TYPE.as_secs()
            )),
        }
    }

    /// [`Self::refusal`] as an error, for the actuator's own last check.
    pub fn check_text(self, text: &str) -> Result<(), PortError> {
        self.refusal(text)
            .map_or(Ok(()), |reason| Err(PortError::new(ADAPTER, reason)))
    }
}

/// Whether the pause after a word boundary follows `c`: white space, ASCII punctuation, the
/// Spanish opening marks and guillemets, and the general punctuation block (dashes, quotes,
/// ellipsis). ASCII symbols (`+`, `=`, `$`, `_`, ...) count with the punctuation, because
/// [`char::is_ascii_punctuation`] does; a letter or a digit is not a boundary.
pub(crate) fn is_word_boundary(c: char) -> bool {
    c.is_whitespace()
        || c.is_ascii_punctuation()
        || matches!(
            c,
            '¡' | '¿' | '«' | '»' | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}'
        )
}

/// What a delivery of the keys of one `type` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyRun {
    /// Every send was made, or the OS stopped accepting at some character (then `inserted`
    /// is below `submitted` and no further key was sent).
    Sent { submitted: u32, inserted: u32 },
    /// A check failed between two characters: nothing more was sent.
    Stopped {
        check: KeysStopCheck,
        sent_chars: usize,
        of_chars: usize,
        why: String,
    },
}

/// A check that ends a paced delivery: which one, for the engine's closed text, and why in the
/// adapter's own words, for the error's message (stderr), never for the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stop {
    pub check: KeysStopCheck,
    pub why: String,
}

/// The error of a paced delivery that a check ended. It carries [`KeysStopped`] as its source,
/// which the engine reads to tell the agent the check and the count of characters sent, and
/// its own message (the adapter's reason and the same count; never the text) for stderr.
pub(crate) fn stopped_error(
    check: KeysStopCheck,
    why: &str,
    sent_chars: usize,
    of_chars: usize,
) -> PortError {
    PortError::new(
        ADAPTER,
        format!("{why}; {sent_chars} of {of_chars} characters were sent, the rest were not"),
    )
    .with_source(KeysStopped {
        check,
        sent_chars,
        of_chars,
    })
}

/// Sends the key events `keys` of `text`. With pacing off it is today's single call with every
/// event and nothing else (no sleep, no check). With pacing on, each character's events (its
/// down and up, both halves of a surrogate pair) go in a call of their own, in order; after a
/// character that another follows, `sleep` waits the key pause and, after a word boundary, the
/// boundary pause as well; and `guard` runs before every character after the first (the first
/// was checked by the caller right before). A `guard` answer stops the delivery, and so does
/// the OS taking fewer events than a character's. Nothing is retried.
pub(crate) fn send_text_keys(
    text: &str,
    keys: &[Event],
    pacing: TypePacing,
    mut send: impl FnMut(&[Event]) -> u32,
    mut sleep: impl FnMut(Duration),
    mut guard: impl FnMut() -> Option<Stop>,
) -> KeyRun {
    let submitted = u32::try_from(keys.len()).unwrap_or(u32::MAX);
    if pacing.is_off() {
        return KeyRun::Sent {
            submitted,
            inserted: send(keys),
        };
    }
    let of_chars = text.chars().count();
    let mut inserted = 0u32;
    let mut offset = 0usize;
    let mut chars = text.chars().peekable();
    let mut sent_chars = 0usize;
    while let Some(c) = chars.next() {
        if sent_chars > 0
            && let Some(Stop { check, why }) = guard()
        {
            return KeyRun::Stopped {
                check,
                sent_chars,
                of_chars,
                why,
            };
        }
        // A character's events: a press and a release per UTF-16 unit.
        let end = offset + c.len_utf16() * 2;
        let Some(group) = keys.get(offset..end) else {
            return KeyRun::Stopped {
                check: KeysStopCheck::Pacing,
                sent_chars,
                of_chars,
                why: "the key events do not match the text".to_owned(),
            };
        };
        let accepted = send(group);
        inserted = inserted.saturating_add(accepted);
        if usize::try_from(accepted) != Ok(group.len()) {
            return KeyRun::Sent {
                submitted,
                inserted,
            };
        }
        offset = end;
        sent_chars += 1;
        if chars.peek().is_some() {
            // One sleep per character, however many pauses it carries: every sleep is a
            // timer tick that can overshoot.
            let Some(pause) = pacing.pause_after(c) else {
                return KeyRun::Stopped {
                    check: KeysStopCheck::Pacing,
                    sent_chars,
                    of_chars,
                    why: "the configured pauses overflow".to_owned(),
                };
            };
            if !pause.is_zero() {
                sleep(pause);
            }
        }
    }
    KeyRun::Sent {
        submitted,
        inserted,
    }
}

/// What the adapter reads between two characters of a paced `type`, as plain facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MidText {
    /// The operator's stop is engaged.
    pub stopped: bool,
    /// The top-level foreground window now, and the one the engine's last look cleared.
    pub window_now: Option<u64>,
    pub window_cleared: Option<u64>,
    /// The foreground process and keyboard focus now.
    pub focus: FocusFacts,
    /// The first modifier or mouse button held down.
    pub held: Option<&'static str>,
}

/// Why the next character of a paced `type` must not be sent, or `None`. The operator's stop
/// first; then the window the engine cleared (its scope on windows holds for that window, so
/// the same window still satisfies it); then the application and keyboard focus (`focus_check`
/// is [`keys_problem`] or [`focus_problem`], as for the first character); then held keys.
pub(crate) fn mid_text_problem(
    facts: MidText,
    focus_check: fn(FocusFacts) -> Option<&'static str>,
) -> Option<Stop> {
    let stop = |check, why: &str| {
        Some(Stop {
            check,
            why: why.to_owned(),
        })
    };
    if facts.stopped {
        return stop(
            KeysStopCheck::OperatorStop,
            "the operator stopped live input",
        );
    }
    match (facts.window_cleared, facts.window_now) {
        (Some(cleared), Some(now)) if cleared == now => {}
        _ => {
            return stop(
                KeysStopCheck::ForegroundWindow,
                "the foreground window changed",
            );
        }
    }
    if let Some(problem) = focus_check(facts.focus) {
        return stop(KeysStopCheck::Focus, problem);
    }
    facts
        .held
        .and_then(|key| stop(KeysStopCheck::HeldKey, &format!("{key} is held down")))
}

/// What the adapter reads about keyboard focus after the click that focuses a `type`
/// target, in process ids and window relations (no window text, no handles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusFacts {
    /// The foreground process attested to the engine and checked before the click.
    pub attested: u32,
    /// The foreground process now.
    pub foreground: Option<u32>,
    /// The process owning the window with keyboard focus (`GUITHREADINFO::hwndFocus` of
    /// the foreground thread); `None` when no window has it.
    pub focus_owner: Option<u32>,
    /// Whether the focused window is the window at the target point or one of its
    /// ancestors (it contains the point).
    pub focus_holds_target: bool,
}

/// Why keys must not be sent after the focusing click, or `None` when keyboard focus is
/// on the target field of the attested application.
pub(crate) fn focus_problem(facts: FocusFacts) -> Option<&'static str> {
    if facts.foreground != Some(facts.attested) {
        return Some("the foreground application changed after the click");
    }
    let Some(owner) = facts.focus_owner else {
        return Some("no window has keyboard focus after the click");
    };
    if owner != facts.attested {
        return Some("keyboard focus belongs to another application");
    }
    if !facts.focus_holds_target {
        return Some("keyboard focus is not on the target field");
    }
    None
}

/// Like [`focus_problem`] for the keys of a two-phase `type` (ADR-0018): the engine has
/// already confirmed, through UI Automation, that keyboard focus is on the target element,
/// so the window-at-the-point relation is not asked again. The attested application must
/// still be in the foreground and own the window with keyboard focus.
pub(crate) fn keys_problem(facts: FocusFacts) -> Option<&'static str> {
    focus_problem(FocusFacts {
        focus_holds_target: true,
        ..facts
    })
}

/// The executable file name of a process image path, lowercase (`notepad.exe`), as the
/// operator lists it in `[safety] allowed_applications`.
pub(crate) fn exe_name(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next()?.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

/// [`exe_name`] of a UTF-16 path; `None` if it is not valid UTF-16.
pub(crate) fn exe_name_utf16(path: &[u16]) -> Option<String> {
    exe_name(&String::from_utf16(path).ok()?)
}

/// The window class of the frame that `ApplicationFrameHost.exe` draws around a hosted
/// (UWP) application, such as the Windows 11 Calculator.
pub(crate) const FRAME_HOST_CLASS: &str = "ApplicationFrameWindow";
/// The executable that owns those frames, lowercase.
pub(crate) const FRAME_HOST_EXE: &str = "applicationframehost.exe";
/// The window class of the hosted application's content, a child of the frame owned by
/// the application's own process (`CalculatorApp.exe`).
pub(crate) const HOSTED_CONTENT_CLASS: &str = "Windows.UI.Core.CoreWindow";

/// Whether a top-level window is an application frame host: both its class and its
/// owner's executable name (from [`exe_name`]) must match. Any other top-level window
/// stands for its own process.
pub(crate) fn is_frame_host(class: &str, owner_exe: Option<&str>) -> bool {
    class == FRAME_HOST_CLASS && owner_exe == Some(FRAME_HOST_EXE)
}

/// What the adapter reads about a top-level window to decide which application it
/// shows, in process ids and window relations (no window text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopLevel {
    /// The process owning the top-level window.
    pub owner: u32,
    /// Whether it is an application frame host ([`is_frame_host`]).
    pub frame_host: bool,
    /// For a frame host, the process owning its hosted content child, if it has one.
    pub hosted: Option<u32>,
}

/// The application a top-level window shows: its own process, or, for a frame host, the
/// process of its hosted content. A frame without hosted content (a suspended or
/// minimised application whose content window has left the frame) shows no application:
/// the frame host itself is never attested, because allowing it would allow every hosted
/// application at once.
pub(crate) fn shown_application(top: TopLevel) -> Option<u32> {
    if top.frame_host {
        top.hosted.filter(|&hosted| hosted != top.owner)
    } else {
        Some(top.owner)
    }
}

/// The application that receives a click at a point whose top-level window is `top`.
/// Inside a frame host only the hosted content (or a window inside it) belongs to the
/// application; the frame's own windows (title bar, caption buttons, input sink) belong to
/// no application, so a point on them is refused.
pub(crate) fn point_application(top: TopLevel, point_in_hosted_content: bool) -> Option<u32> {
    if top.frame_host && !point_in_hosted_content {
        return None;
    }
    shown_application(top)
}

/// User Interface Privilege Isolation: input reaches only processes at our integrity
/// level or below. `SendInput` drops the rest silently and still counts them as inserted,
/// so a higher foreground process is refused before anything is sent. Arguments are the
/// last sub-authority (RID) of each mandatory label (0x2000 medium, 0x3000 high, ...).
pub(crate) fn integrity_allows(own_rid: u32, foreground_rid: u32) -> bool {
    foreground_rid <= own_rid
}

/// Virtual keys that must be up before input is injected: a held modifier would turn the
/// click or the text into a shortcut, a held mouse button into a drag.
pub(crate) const MUST_BE_UP: [(u16, &str); 7] = [
    (0x10, "Shift"),
    (0x11, "Ctrl"),
    (0x12, "Alt"),
    (0x5B, "the left Windows key"),
    (0x5C, "the right Windows key"),
    (0x01, "the left mouse button"),
    (0x02, "the right mouse button"),
];

/// The first key reported down (`GetAsyncKeyState` sets the most significant bit, so the
/// state is negative).
pub(crate) fn first_held(
    states: impl IntoIterator<Item = (&'static str, i16)>,
) -> Option<&'static str> {
    states
        .into_iter()
        .find(|&(_, state)| state < 0)
        .map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// The reference desktop: primary 1920x1080 at (0, 0), secondary 1920x1080 at
    /// (1920, -137).
    const DESKTOP: VirtualScreen = VirtualScreen {
        x: 0,
        y: -137,
        width: 3840,
        height: 1217,
    };

    /// How Windows maps a normalised coordinate back to a pixel.
    fn back(n: i32, origin: i32, extent: i32) -> i32 {
        origin + (i64::from(n) * i64::from(extent) / 65_536) as i32
    }

    #[test]
    fn every_pixel_of_the_virtual_desktop_round_trips() {
        for x in DESKTOP.x..DESKTOP.x + DESKTOP.width {
            let (dx, _) = normalise((x, 0), DESKTOP).unwrap();
            assert!((0..=65_535).contains(&dx));
            assert_eq!(back(dx, DESKTOP.x, DESKTOP.width), x, "x = {x}");
        }
        for y in DESKTOP.y..DESKTOP.y + DESKTOP.height {
            let (_, dy) = normalise((0, y), DESKTOP).unwrap();
            assert_eq!(back(dy, DESKTOP.y, DESKTOP.height), y, "y = {y}");
        }
    }

    #[test]
    fn negative_origins_and_the_edges_are_handled() {
        // The secondary monitor's top-left pixel lies above the primary.
        let (dx, dy) = normalise((1920, -137), DESKTOP).unwrap();
        assert_eq!(
            (back(dx, 0, 3840), back(dy, -137, 1217)),
            (1920, -137),
            "secondary origin"
        );
        assert!(normalise((3839, 1079), DESKTOP).is_some());
        for outside in [
            (3840, 0),
            (-1, 0),
            (0, -138),
            (0, 1080),
            (i32::MIN, i32::MAX),
        ] {
            assert_eq!(normalise(outside, DESKTOP), None, "{outside:?}");
        }
        let empty = VirtualScreen {
            width: 0,
            ..DESKTOP
        };
        assert_eq!(normalise((0, 0), empty), None);
    }

    #[test]
    fn a_click_is_a_move_a_press_and_a_release() {
        assert_eq!(
            batch(&ActionKind::Click, (10, 20)),
            vec![
                Event::MoveTo { dx: 10, dy: 20 },
                Event::LeftDown,
                Event::LeftUp
            ]
        );
    }

    #[test]
    fn typing_clicks_first_then_sends_each_utf16_unit() {
        let events = batch(
            &ActionKind::Type {
                text: "é😀".into()
            },
            (0, 0),
        );
        // é is one unit, the emoji a surrogate pair: 3 units, 6 events after the click.
        assert_eq!(events.len(), 3 + 6);
        assert_eq!(&events[1..3], &[Event::LeftDown, Event::LeftUp]);
        assert_eq!(
            &events[3..5],
            &[
                Event::Unicode {
                    unit: 0xE9,
                    up: false
                },
                Event::Unicode {
                    unit: 0xE9,
                    up: true
                }
            ]
        );
        assert_eq!(
            events[5],
            Event::Unicode {
                unit: 0xD83D,
                up: false
            }
        );
    }

    #[test]
    fn scrolling_sends_one_notch_per_step() {
        let up = batch(
            &ActionKind::Scroll {
                direction: ScrollDirection::Up,
                steps: 3,
            },
            (0, 0),
        );
        assert_eq!(&up[1..], &[Event::Wheel(120); 3]);
        let down = batch(
            &ActionKind::Scroll {
                direction: ScrollDirection::Down,
                steps: 1,
            },
            (0, 0),
        );
        assert_eq!(&down[1..], &[Event::Wheel(-120)]);
    }

    #[test]
    fn typing_splits_before_the_first_key_and_other_actions_have_no_keys() {
        let typing = batch(&ActionKind::Type { text: "ab".into() }, (5, 6));
        let (pointer, keys) = split_keys(typing.clone());
        assert_eq!(
            pointer,
            vec![
                Event::MoveTo { dx: 5, dy: 6 },
                Event::LeftDown,
                Event::LeftUp
            ]
        );
        assert_eq!(keys.len(), 4);
        assert!(keys.iter().all(|e| matches!(e, Event::Unicode { .. })));
        assert_eq!([pointer, keys].concat(), typing);
        for other in [
            ActionKind::Click,
            ActionKind::Scroll {
                direction: ScrollDirection::Down,
                steps: 2,
            },
        ] {
            let events = batch(&other, (0, 0));
            assert_eq!(split_keys(events.clone()), (events, Vec::new()));
        }
    }

    #[test]
    fn keys_are_sent_only_with_focus_on_the_target_of_the_attested_application() {
        const APP: u32 = 41;
        let ok = FocusFacts {
            attested: APP,
            foreground: Some(APP),
            focus_owner: Some(APP),
            focus_holds_target: true,
        };
        assert_eq!(focus_problem(ok), None);
        let cases = [
            (
                FocusFacts {
                    foreground: Some(7),
                    ..ok
                },
                "foreground application changed",
            ),
            (
                FocusFacts {
                    foreground: None,
                    ..ok
                },
                "foreground application changed",
            ),
            (
                FocusFacts {
                    focus_owner: None,
                    ..ok
                },
                "no window has keyboard focus",
            ),
            (
                FocusFacts {
                    focus_owner: Some(7),
                    ..ok
                },
                "another application",
            ),
            (
                FocusFacts {
                    focus_holds_target: false,
                    ..ok
                },
                "not on the target field",
            ),
        ];
        for (facts, expected) in cases {
            let problem = focus_problem(facts).unwrap_or_default();
            assert!(problem.contains(expected), "{facts:?}: {problem}");
        }
    }

    #[test]
    fn the_keys_of_a_two_phase_type_need_only_the_attested_application_to_hold_focus() {
        const APP: u32 = 41;
        let facts = FocusFacts {
            attested: APP,
            foreground: Some(APP),
            focus_owner: Some(APP),
            // UI Automation confirmed the element: the window relation is not asked.
            focus_holds_target: false,
        };
        assert_eq!(keys_problem(facts), None);
        assert!(
            focus_problem(facts).is_some(),
            "the one-shot check still asks"
        );
        for (changed, expected) in [
            (
                FocusFacts {
                    foreground: Some(7),
                    ..facts
                },
                "foreground application changed",
            ),
            (
                FocusFacts {
                    focus_owner: None,
                    ..facts
                },
                "no window has keyboard focus",
            ),
            (
                FocusFacts {
                    focus_owner: Some(7),
                    ..facts
                },
                "another application",
            ),
        ] {
            let problem = keys_problem(changed).unwrap_or_default();
            assert!(problem.contains(expected), "{changed:?}: {problem}");
        }
    }

    // ---- paced typing (decision D49) ----

    /// One thing a delivery did, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Step {
        Send(Vec<Event>),
        Sleep(Duration),
        Guard,
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// Runs `send_text_keys` over the keys of `text` with a fake `SendInput` that accepts what
    /// `accept` says, a clock that records pauses and never sleeps, and a guard that answers
    /// `problem(call)` on its `call`-th ask (1-based).
    fn run(
        text: &str,
        pacing: TypePacing,
        accept: impl Fn(usize, &[Event]) -> u32,
        problem: impl Fn(usize) -> Option<Stop>,
    ) -> (KeyRun, Vec<Step>) {
        let keys = split_keys(batch(&ActionKind::Type { text: text.into() }, (0, 0))).1;
        let log = RefCell::new(Vec::new());
        let sends = RefCell::new(0usize);
        let asks = RefCell::new(0usize);
        let result = send_text_keys(
            text,
            &keys,
            pacing,
            |events| {
                *sends.borrow_mut() += 1;
                log.borrow_mut().push(Step::Send(events.to_vec()));
                accept(*sends.borrow(), events)
            },
            |pause| log.borrow_mut().push(Step::Sleep(pause)),
            || {
                *asks.borrow_mut() += 1;
                log.borrow_mut().push(Step::Guard);
                problem(*asks.borrow())
            },
        );
        (result, log.into_inner())
    }

    fn all(_: usize, events: &[Event]) -> u32 {
        u32::try_from(events.len()).unwrap()
    }

    fn never(_: usize) -> Option<Stop> {
        None
    }

    fn stop(check: KeysStopCheck, why: &str) -> Stop {
        Stop {
            check,
            why: why.to_owned(),
        }
    }

    /// The adapter's reason of a stop, for the tests that read the words.
    fn why(stop: Option<Stop>) -> String {
        stop.map(|s| s.why).unwrap_or_default()
    }

    fn pair(unit: u16) -> [Event; 2] {
        [
            Event::Unicode { unit, up: false },
            Event::Unicode { unit, up: true },
        ]
    }

    fn pacing(key_ms: u64, boundary_ms: u64) -> TypePacing {
        TypePacing::new(MS(key_ms), MS(boundary_ms)).unwrap()
    }

    #[test]
    fn zero_pauses_send_every_key_in_one_call_as_before_pacing_existed() {
        // The events of `aé😀`, written out as they were sent before pacing existed: a press
        // and a release per UTF-16 unit, in one call.
        let golden: Vec<Event> = [0x61, 0xE9, 0xD83D, 0xDE00]
            .into_iter()
            .flat_map(pair)
            .collect();
        assert_eq!(golden.len(), 8);
        let keys = split_keys(batch(
            &ActionKind::Type {
                text: "aé😀".into(),
            },
            (0, 0),
        ))
        .1;
        assert_eq!(keys, golden);
        for off in [TypePacing::default(), pacing(0, 0)] {
            assert!(off.is_off());
            let (result, steps) = run("aé😀", off, all, never);
            assert_eq!(
                steps,
                vec![Step::Send(golden.clone())],
                "one call, no pause"
            );
            assert_eq!(
                result,
                KeyRun::Sent {
                    submitted: 8,
                    inserted: 8
                }
            );
        }
    }

    #[test]
    fn a_key_pause_gives_one_call_per_character_with_a_check_before_each_but_the_first() {
        let (result, steps) = run("abc", pacing(5, 0), all, never);
        let (a, b, c) = (
            pair(0x61).to_vec(),
            pair(0x62).to_vec(),
            pair(0x63).to_vec(),
        );
        assert_eq!(
            steps,
            vec![
                Step::Send(a),
                Step::Sleep(MS(5)),
                Step::Guard,
                Step::Send(b),
                Step::Sleep(MS(5)),
                Step::Guard,
                Step::Send(c),
            ],
            "no pause or check after the last character"
        );
        assert_eq!(
            result,
            KeyRun::Sent {
                submitted: 6,
                inserted: 6
            }
        );
    }

    #[test]
    fn the_boundary_pause_follows_a_boundary_only_when_more_text_follows() {
        let (_, steps) = run("a b ", pacing(2, 7), all, never);
        let sleeps: Vec<Duration> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Sleep(d) => Some(*d),
                _ => None,
            })
            .collect();
        // After `a`: the key pause. After the space: the key pause and the boundary pause, as
        // ONE sleep (each sleep is a timer tick that can overshoot). After `b`: the key
        // pause. After the trailing space (nothing follows): nothing.
        assert_eq!(sleeps, vec![MS(2), MS(9), MS(2)]);
        // Only a boundary pause: the characters that are not boundaries wait for nothing.
        let (_, steps) = run("ab,c", pacing(0, 9), all, never);
        let sleeps: Vec<&Step> = steps
            .iter()
            .filter(|s| matches!(s, Step::Sleep(_)))
            .collect();
        assert_eq!(sleeps, vec![&Step::Sleep(MS(9))]);
        // A final comma has nothing after it either.
        let (_, steps) = run("ab,", pacing(0, 9), all, never);
        assert!(!steps.iter().any(|s| matches!(s, Step::Sleep(_))));
    }

    #[test]
    fn what_counts_as_a_word_boundary() {
        for boundary in [
            ' ', '\t', '\n', ',', '.', ';', ':', '!', '?', '¿', '¡', '—', '…', '«',
        ] {
            assert!(is_word_boundary(boundary), "{boundary:?}");
        }
        for letter in ['a', 'Z', 'é', 'ñ', '1', '_', '😀', '€', '+'] {
            // `_` and `+` are ASCII punctuation: they are boundaries; the rest are not.
            assert_eq!(
                is_word_boundary(letter),
                letter.is_ascii_punctuation(),
                "{letter:?}"
            );
        }
    }

    #[test]
    fn a_surrogate_pair_stays_in_one_call() {
        let (_, steps) = run("x😀y", pacing(1, 0), all, never);
        let sends: Vec<usize> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Send(events) => Some(events.len()),
                _ => None,
            })
            .collect();
        assert_eq!(sends, vec![2, 4, 2], "the emoji's two units go together");
        assert!(steps.contains(&Step::Send([pair(0xD83D), pair(0xDE00)].concat())));
    }

    #[test]
    fn the_stop_raised_after_the_third_character_ends_the_delivery_there() {
        let raised = |call: usize| {
            (call == 3).then(|| {
                stop(
                    KeysStopCheck::OperatorStop,
                    "the operator stopped live input",
                )
            })
        };
        let (result, steps) = run("abcdef", pacing(1, 0), all, raised);
        assert_eq!(
            result,
            KeyRun::Stopped {
                check: KeysStopCheck::OperatorStop,
                sent_chars: 3,
                of_chars: 6,
                why: "the operator stopped live input".to_owned()
            }
        );
        let sent: usize = steps.iter().filter(|s| matches!(s, Step::Send(_))).count();
        assert_eq!(sent, 3, "nothing after the check that failed");
        assert_eq!(steps.last(), Some(&Step::Guard));
    }

    #[test]
    fn a_change_of_foreground_window_or_focus_between_characters_stops_the_delivery() {
        const APP: u32 = 41;
        let focus = FocusFacts {
            attested: APP,
            foreground: Some(APP),
            focus_owner: Some(APP),
            focus_holds_target: true,
        };
        let facts = |call: usize| MidText {
            stopped: false,
            window_cleared: Some(100),
            // The foreground window changes on the third look.
            window_now: Some(if call < 3 { 100 } else { 200 }),
            focus,
            held: None,
        };
        let (result, steps) = run("abcdef", pacing(1, 0), all, |call| {
            mid_text_problem(facts(call), keys_problem)
        });
        assert_eq!(
            result,
            KeyRun::Stopped {
                check: KeysStopCheck::ForegroundWindow,
                sent_chars: 3,
                of_chars: 6,
                why: "the foreground window changed".to_owned()
            }
        );
        assert_eq!(
            steps.iter().filter(|s| matches!(s, Step::Send(_))).count(),
            3
        );

        // Focus moving to another application ends it likewise.
        let (result, _) = run("abcdef", pacing(1, 0), all, |call| {
            let mut facts = facts(1);
            if call == 2 {
                facts.focus.focus_owner = Some(7);
            }
            mid_text_problem(facts, keys_problem)
        });
        assert!(matches!(
            result,
            KeyRun::Stopped { check: KeysStopCheck::Focus, sent_chars: 2, ref why, .. }
                if why.contains("another application")
        ));
    }

    #[test]
    fn each_thing_that_can_change_between_characters_is_checked() {
        const APP: u32 = 41;
        let ok = MidText {
            stopped: false,
            window_now: Some(100),
            window_cleared: Some(100),
            focus: FocusFacts {
                attested: APP,
                foreground: Some(APP),
                focus_owner: Some(APP),
                // UI Automation confirmed the element for the two-phase path.
                focus_holds_target: false,
            },
            held: None,
        };
        assert_eq!(mid_text_problem(ok, keys_problem), None);
        // The one-shot path also asks that the focused window holds the target.
        assert!(why(mid_text_problem(ok, focus_problem)).contains("not on the target field"));
        let cases = [
            (
                MidText {
                    stopped: true,
                    ..ok
                },
                "stopped live input",
            ),
            (
                MidText {
                    window_now: Some(5),
                    ..ok
                },
                "foreground window changed",
            ),
            (
                MidText {
                    window_now: None,
                    ..ok
                },
                "foreground window changed",
            ),
            (
                MidText {
                    window_cleared: None,
                    ..ok
                },
                "foreground window changed",
            ),
            (
                MidText {
                    focus: FocusFacts {
                        foreground: Some(7),
                        ..ok.focus
                    },
                    ..ok
                },
                "foreground application changed",
            ),
            (
                MidText {
                    held: Some("Ctrl"),
                    ..ok
                },
                "Ctrl is held down",
            ),
        ];
        for (facts, expected) in cases {
            let problem = why(mid_text_problem(facts, keys_problem));
            assert!(problem.contains(expected), "{facts:?}: {problem}");
        }
        // The operator's stop is named first.
        let both = MidText {
            stopped: true,
            held: Some("Ctrl"),
            window_now: Some(5),
            ..ok
        };
        assert!(why(mid_text_problem(both, keys_problem)).contains("stopped live input"));
    }

    /// The check the engine names to the agent, one per kind of stop.
    #[test]
    fn each_stop_between_characters_names_its_check() {
        const APP: u32 = 41;
        let ok = MidText {
            stopped: false,
            window_now: Some(100),
            window_cleared: Some(100),
            focus: FocusFacts {
                attested: APP,
                foreground: Some(APP),
                focus_owner: Some(APP),
                focus_holds_target: false,
            },
            held: None,
        };
        let check_of = |facts| mid_text_problem(facts, keys_problem).map(|s| s.check);
        assert_eq!(
            check_of(MidText {
                stopped: true,
                ..ok
            }),
            Some(KeysStopCheck::OperatorStop)
        );
        for window_now in [Some(5), None] {
            assert_eq!(
                check_of(MidText { window_now, ..ok }),
                Some(KeysStopCheck::ForegroundWindow)
            );
        }
        let elsewhere = FocusFacts {
            focus_owner: Some(7),
            ..ok.focus
        };
        assert_eq!(
            check_of(MidText {
                focus: elsewhere,
                ..ok
            }),
            Some(KeysStopCheck::Focus)
        );
        assert_eq!(
            check_of(MidText {
                held: Some("Shift"),
                ..ok
            }),
            Some(KeysStopCheck::HeldKey)
        );
    }

    /// The error of a delivery that a check ended: the engine reads the check and the count from
    /// its source; neither it nor the engine's sentence holds a character of the text.
    #[test]
    fn a_stopped_delivery_reports_the_check_and_how_far_it_got_and_never_the_text() {
        for (check, why) in [
            (
                KeysStopCheck::OperatorStop,
                "the operator stopped live input",
            ),
            (
                KeysStopCheck::ForegroundWindow,
                "the foreground window changed",
            ),
        ] {
            stopped_after_three_of_twelve(check, why);
        }
    }

    fn stopped_after_three_of_twelve(check: KeysStopCheck, why: &str) {
        // No lowercase letter and no digit: none of the closed sentences can contain a character
        // of it by accident.
        let text = "QXJZ#@~|QXJZ";
        let raised = |call: usize| (call == 3).then(|| stop(check, why));
        let (result, steps) = run(text, pacing(1, 0), all, raised);
        let KeyRun::Stopped {
            check: ended_by,
            sent_chars,
            of_chars,
            why: reason,
        } = result
        else {
            panic!("the delivery was not stopped: {result:?}");
        };
        let sends = steps.iter().filter(|s| matches!(s, Step::Send(_))).count();
        assert_eq!(sends, 3);
        let error = stopped_error(ended_by, &reason, sent_chars, of_chars);
        let carried = error
            .source
            .as_deref()
            .and_then(|source| source.downcast_ref::<KeysStopped>())
            .copied();
        let expected = KeysStopped {
            check,
            sent_chars: 3,
            of_chars: 12,
        };
        assert_eq!(carried, Some(expected));
        assert!(
            error.message.contains("3 of 12 characters"),
            "{}",
            error.message
        );
        let told = expected.told();
        assert!(
            told.contains(check.describe()) && told.contains("3 of 12"),
            "{told}"
        );
        for message in [error.message.as_str(), told.as_str()] {
            assert!(
                !message.chars().any(|c| text.contains(c)),
                "the text is in {message:?}"
            );
        }
    }

    #[test]
    fn an_os_that_stops_accepting_ends_the_delivery_without_a_retry() {
        // The second character's call inserts one of its two events.
        let short = |call: usize, events: &[Event]| {
            let all = u32::try_from(events.len()).unwrap();
            if call == 2 { 1 } else { all }
        };
        let (result, steps) = run("abcd", pacing(1, 0), short, never);
        assert_eq!(
            result,
            KeyRun::Sent {
                submitted: 8,
                inserted: 3
            },
            "the receipt shows the shortfall, the engine reports it"
        );
        assert_eq!(
            steps.iter().filter(|s| matches!(s, Step::Send(_))).count(),
            2
        );
    }

    #[test]
    fn a_paced_type_that_would_hold_the_engine_too_long_is_refused_before_it_starts() {
        let text = |n: usize| "a".repeat(n);
        // The longest type, at the pause that fits in the bound, and one more millisecond.
        assert!(pacing(14, 0).check_text(&text(2048)).is_ok());
        let refused = pacing(15, 0)
            .check_text(&text(2048))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("at most 30 s"), "{refused}");
        assert!(refused.contains("nothing was sent"), "{refused}");
        // 600 characters at the largest key pause fit, 602 do not.
        assert!(pacing(50, 0).check_text(&text(600)).is_ok());
        assert!(pacing(50, 0).check_text(&text(602)).is_err());
        // Boundary pauses count, and a trailing boundary does not.
        let spaced = "a ".repeat(100);
        assert_eq!(
            pacing(10, 100).duration_of(&spaced),
            Some(MS(10 * 199 + 100 * 99))
        );
        assert!(pacing(0, 250).check_text(&"a ".repeat(100)).is_ok());
        assert!(pacing(0, 250).check_text(&"a ".repeat(150)).is_err());
        // With pacing off nothing is refused, whatever the length.
        assert!(TypePacing::default().check_text(&text(2048)).is_ok());
    }

    #[test]
    fn a_sum_of_pauses_that_overflows_is_a_refusal_and_not_a_saturated_value() {
        let text = "abc";
        // Two gaps of the largest `Duration` do not fit in one: no total, not `Duration::MAX`.
        let huge = TypePacing {
            key_pause: Duration::MAX,
            boundary_pause: Duration::ZERO,
        };
        assert_eq!(huge.duration_of(text), None);
        // The key pause and the boundary pause of one gap overflow likewise.
        let both = TypePacing {
            key_pause: Duration::MAX,
            boundary_pause: MS(1),
        };
        assert_eq!(both.duration_of("a b"), None);
        for p in [huge, both] {
            let reason = p.refusal("a b c").unwrap_or_default();
            assert!(reason.contains("at most 30 s"), "{reason}");
            assert!(p.check_text("a b c").is_err());
        }
        // A delivery whose pauses overflow stops at the pause that does and sends no more.
        let (result, steps) = run("a b", both, all, never);
        assert!(matches!(
            result,
            KeyRun::Stopped { check: KeysStopCheck::Pacing, sent_chars: 2, ref why, .. }
                if why.contains("overflow")
        ));
        assert_eq!(
            steps.iter().filter(|s| matches!(s, Step::Send(_))).count(),
            2
        );
        // Nothing to wait for is a total of zero, which fits.
        assert_eq!(huge.duration_of("a"), Some(Duration::ZERO));
        assert_eq!(huge.refusal("a"), None);
    }

    #[test]
    fn the_refusal_says_what_to_do_and_that_nothing_was_sent() {
        let reason = pacing(50, 0).refusal(&"a".repeat(2048)).unwrap_or_default();
        assert!(reason.contains("at most 30 s"), "{reason}");
        assert!(reason.contains("send less text"), "{reason}");
        assert!(reason.contains("nothing was sent"), "{reason}");
        // The same text with pacing off, or with pauses that fit, is not refused.
        assert_eq!(TypePacing::default().refusal(&"a".repeat(2048)), None);
        assert_eq!(pacing(14, 0).refusal(&"a".repeat(2048)), None);
    }

    #[test]
    fn the_time_the_bound_counts_is_the_time_a_delivery_waits() {
        for text in ["Hola, mundo. ¿Qué tal?", "a b  c", "x", ""] {
            let p = pacing(3, 11);
            let (_, steps) = run(text, p, all, never);
            let waited: Duration = steps
                .iter()
                .filter_map(|s| match s {
                    Step::Sleep(d) => Some(*d),
                    _ => None,
                })
                .sum();
            assert_eq!(Some(waited), p.duration_of(text), "{text:?}");
        }
    }

    #[test]
    fn a_pause_above_its_bound_is_refused() {
        assert!(TypePacing::new(MS(MAX_TYPE_KEY_PAUSE_MS), MS(MAX_TYPE_BOUNDARY_PAUSE_MS)).is_ok());
        let key = TypePacing::new(MS(MAX_TYPE_KEY_PAUSE_MS + 1), MS(0)).unwrap_err();
        assert!(key.to_string().contains("between keys"), "{key}");
        let boundary = TypePacing::new(MS(0), MS(MAX_TYPE_BOUNDARY_PAUSE_MS + 1)).unwrap_err();
        assert!(boundary.to_string().contains("word boundary"), "{boundary}");
        assert!(TypePacing::default().is_off());
        assert!(!pacing(0, 1).is_off() && !pacing(1, 0).is_off());
    }

    #[test]
    fn the_executable_name_is_the_lowercase_file_name() {
        assert_eq!(
            exe_name(r"C:\Windows\System32\Notepad.EXE").as_deref(),
            Some("notepad.exe")
        );
        assert_eq!(exe_name("calc.exe").as_deref(), Some("calc.exe"));
        assert_eq!(exe_name(r"C:\dir\"), None);
        assert_eq!(exe_name(""), None);
        let wide: Vec<u16> = r"C:\Apps\Ünïcode.exe".encode_utf16().collect();
        assert_eq!(exe_name_utf16(&wide).as_deref(), Some("ünïcode.exe"));
        assert_eq!(exe_name_utf16(&[0xD800]), None, "lone surrogate");
    }

    #[test]
    fn only_the_application_frame_host_is_resolved() {
        assert!(is_frame_host(
            "ApplicationFrameWindow",
            Some("applicationframehost.exe")
        ));
        // explorer.exe also owns windows of that class: they are not resolved.
        assert!(!is_frame_host(
            "ApplicationFrameWindow",
            Some("explorer.exe")
        ));
        assert!(!is_frame_host("ApplicationFrameWindow", None));
        assert!(!is_frame_host("Notepad", Some("applicationframehost.exe")));
        assert!(!is_frame_host(
            "applicationframewindow",
            Some("applicationframehost.exe")
        ));
    }

    /// Regression (S4): the Windows 11 Calculator's foreground window belongs to
    /// `ApplicationFrameHost.exe`, so the actuator attested the frame host instead of the
    /// calculator, and only allowlisting the frame host (every hosted app) would let it act.
    #[test]
    fn a_frame_host_window_attests_the_hosted_application() {
        const HOST: u32 = 10;
        const CALC: u32 = 20;
        let frame = TopLevel {
            owner: HOST,
            frame_host: true,
            hosted: Some(CALC),
        };
        assert_eq!(shown_application(frame), Some(CALC));
        // No hosted content (suspended or minimised): no application, never the host.
        assert_eq!(
            shown_application(TopLevel {
                hosted: None,
                ..frame
            }),
            None
        );
        assert_eq!(
            shown_application(TopLevel {
                hosted: Some(HOST),
                ..frame
            }),
            None
        );
        // An ordinary window stands for its own process, whatever its children.
        let plain = TopLevel {
            owner: 7,
            frame_host: false,
            hosted: Some(CALC),
        };
        assert_eq!(shown_application(plain), Some(7));
    }

    #[test]
    fn a_point_on_the_frame_hosts_own_windows_belongs_to_no_application() {
        let frame = TopLevel {
            owner: 10,
            frame_host: true,
            hosted: Some(20),
        };
        assert_eq!(point_application(frame, true), Some(20));
        // Title bar and caption buttons (minimise, maximise, close) are the host's.
        assert_eq!(point_application(frame, false), None);
        let plain = TopLevel {
            owner: 7,
            frame_host: false,
            hosted: None,
        };
        assert_eq!(point_application(plain, false), Some(7));
    }

    #[test]
    fn a_higher_integrity_foreground_is_refused() {
        const MEDIUM: u32 = 0x2000;
        const HIGH: u32 = 0x3000;
        assert!(integrity_allows(MEDIUM, MEDIUM));
        assert!(integrity_allows(HIGH, MEDIUM));
        assert!(!integrity_allows(MEDIUM, HIGH));
    }

    #[test]
    fn a_held_key_is_reported() {
        assert_eq!(first_held([("Shift", 0), ("Ctrl", 1)]), None);
        assert_eq!(
            first_held([("Shift", 0), ("Ctrl", i16::MIN), ("Alt", -1)]),
            Some("Ctrl")
        );
        assert_eq!(MUST_BE_UP.len(), 7);
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use ommatix_domain::{DisplayMapping, bbox::BBox};

    /// UT-75 (review 2, F2): a window without a redirection bitmap can be composed with
    /// per-pixel alpha by DirectComposition with neither layered bit, so it is never attested.
    #[test]
    fn ut_75_a_window_without_a_redirection_bitmap_is_never_attested_opaque() {
        const NO_REDIRECTION_BITMAP: u32 = WS_EX_NOREDIRECTIONBITMAP;
        for backdrop in [Some(0), Some(1)] {
            for extra in [0, 0x0004_0000, WS_EX_LAYERED, WS_EX_TRANSPARENT] {
                assert_eq!(
                    window_opacity_of(Some(NO_REDIRECTION_BITMAP | extra), backdrop),
                    None,
                    "{extra:#x} {backdrop:?}"
                );
            }
        }
    }

    /// UT-75: only a plain window with no backdrop is attested opaque.
    #[test]
    fn ut_75_only_an_unlayered_window_without_a_backdrop_is_attested_opaque() {
        let opaque = Some(WindowOpacity {
            corner_exclusion_px: 32,
        });
        assert_eq!(CORNER_EXCLUSION_PX, 32);
        // AUTO and NONE.
        assert_eq!(window_opacity_of(Some(0), Some(0)), opaque);
        assert_eq!(window_opacity_of(Some(0x0004_0000), Some(1)), opaque);
        // Layered, click-through, or both.
        for style in [
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
            WS_EX_LAYERED | WS_EX_TRANSPARENT,
        ] {
            assert_eq!(window_opacity_of(Some(style), Some(1)), None, "{style:#x}");
        }
        // MAINWINDOW (Mica), TRANSIENTWINDOW (Acrylic), TABBEDWINDOW (Mica Alt), unknown
        // values, and a query that failed.
        for backdrop in [Some(2), Some(3), Some(4), Some(5), Some(u32::MAX), None] {
            assert_eq!(window_opacity_of(Some(0), backdrop), None, "{backdrop:?}");
        }
    }

    /// UT-75 (review 2, F1): a zero style with a nonzero error is a failed query, not an empty
    /// style; a zero with no error is a legitimate style; no style attests nothing.
    #[test]
    fn ut_75_a_failed_style_query_is_not_an_empty_style() {
        assert_eq!(extended_style_of(0, 0), Some(0));
        assert_eq!(extended_style_of(0, 5), None);
        assert_eq!(extended_style_of(0x0004_0100, 5), Some(0x0004_0100));
        assert_eq!(extended_style_of(0x1_0000_0020, 0), Some(0x20));
        let from_a_failed_query = extended_style_of(0, 1400);
        assert_eq!(window_opacity_of(from_a_failed_query, Some(1)), None);
        assert!(window_opacity_of(extended_style_of(0, 0), Some(1)).is_some());
    }

    /// Review 2, inherited gap: a normal end of the enumeration is complete; an error with
    /// the cursor still a window is not.
    #[test]
    fn a_failed_enumeration_is_not_a_complete_walk() {
        // Handles 1 -> 2 -> 3, then the scripted answer after 3.
        let run = |after_last: Sibling<u64>, exists: bool| {
            let mut visited = Vec::new();
            let done = walk_windows(
                1u64,
                Some,
                |h| {
                    visited.push(h);
                    Some(())
                },
                |h| match h {
                    1 => Sibling::Found(2),
                    2 => Sibling::Found(3),
                    _ => after_last,
                },
                |_| exists,
            );
            (done, visited)
        };
        assert_eq!(run(Sibling::End, true), (Some(()), vec![1, 2, 3]));
        // The cursor is still a window, but the enumeration failed: nothing is attested.
        assert_eq!(run(Sibling::Failed, true).0, None);
        // A destroyed cursor does not end the walk either.
        assert_eq!(run(Sibling::End, false).0, None);
        // A repeated handle (a cycle) and a null one abort.
        let looping = walk_windows(1u64, Some, |_| Some(()), |_| Sibling::Found(1), |_| true);
        assert_eq!(looping, None);
        assert_eq!(
            walk_windows(0u64, Some, |_| Some(()), |_| Sibling::End, |_| true),
            None
        );
        // The classification of the call's result and last error.
        assert_eq!(sibling(Some(7u64), 0), Sibling::Found(7));
        assert_eq!(sibling(Some(7u64), 5), Sibling::Found(7));
        assert_eq!(sibling::<u64>(None, 0), Sibling::End);
        assert_eq!(sibling::<u64>(None, 1400), Sibling::Failed);
    }

    #[test]
    fn a_window_walk_rejects_null_cycles_and_an_incomplete_list() {
        let mut walk = WindowWalk::default();
        assert!(!walk.visit(0));
        for handle in 1..=4096 {
            assert!(walk.visit(handle));
        }
        assert!(!walk.visit(4097));
        assert!(!WindowWalk::default().visit(0));
        let mut cycle = WindowWalk::default();
        assert!(cycle.visit(42));
        assert!(!cycle.visit(42));
    }

    #[test]
    fn visible_bounds_clip_on_a_monitor_with_a_negative_origin() {
        let mapping = DisplayMapping::new((-1920, 0), (1920, 1080), (1920, 1080)).unwrap();
        assert_eq!(
            frame_bounds(mapping, [-2000, 10, -1800, 110]),
            Some(BBox::new(0.0, 10.0, 120.0, 110.0).unwrap())
        );
        assert_eq!(frame_bounds(mapping, [0, 0, 100, 100]), None);
        assert_eq!(frame_bounds(mapping, [-1800, 10, -1800, 110]), None);
    }
}
