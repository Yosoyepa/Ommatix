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

use ommatix_application::contract::{ActionKind, ScrollDirection};

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "ADR-0016: SendInput and the process/token queries are unsafe Win32 fns"
)]
mod win;

#[cfg(windows)]
pub use win::SendInputActuator;

pub(crate) const ADAPTER: &str = "sendinput";

/// One wheel notch (`WHEEL_DELTA`).
pub(crate) const WHEEL_DELTA: i32 = 120;

/// Largest normalised absolute coordinate.
const NORMALISED_MAX: i64 = 65_535;

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
