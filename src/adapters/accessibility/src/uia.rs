//! `UiaProvider`: the foreground application's UI Automation tree (Windows, feature `uia`).
//!
//! Safe Rust over the [`uiautomation`] crate; this module has no `unsafe` of its own
//! (ADR-0017). COM objects live on one worker thread, initialised in the multithreaded
//! apartment, which the engine reaches through channels: the provider itself is `Send`
//! whatever the COM interfaces are.
//!
//! One snapshot:
//!
//! 1. **Attest the application.** The element with keyboard focus is walked up to its
//!    top-level window. An `ApplicationFrameHost` frame stands for the process of its
//!    hosted `CoreWindow`; a frame without one attests nothing. This is the resolution the
//!    `SendInput` actuator applies, so evidence and input concern the same application.
//! 2. **Pick its windows.** Every visible top-level window of that process, and the hosted
//!    `CoreWindow` of every frame it lives in: menus and pop-ups are separate windows. Of
//!    other windows only the process id, class, visibility and rectangle are read.
//! 3. **Walk them** in the control view, bounded by the [`Budget`]. Subtrees of another
//!    process, off screen, or whose rectangle lies off the captured monitor or outside
//!    every planned region are not entered. Each step fetches the cached properties in one
//!    call: control type, name, rectangle, enabled, off-screen, password, process id and
//!    the availability of the `Invoke`, `Toggle`, `ExpandCollapse`, `SelectionItem`,
//!    `Value` (and whether it is read-only) and `Scroll` patterns. `Value.Value` is never
//!    requested.
//! 4. **Say when it was cut short.** A walk that ends on its budget (elements, time or
//!    depth), or that cannot read a window, reports `truncated`: what is missing is
//!    unknown, not absent, and the engine reads those regions again on its next
//!    observation.
//!
//! `keyboard_focus` (ADR-0018) reads the same cached properties of the one element that has
//! keyboard focus, when it belongs to the attested foreground application and lies on the
//! captured monitor. It only asks the framework which element has focus: it never moves it.

use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use ommatix_application::{
    AccessibilityProvider, AccessibleElement, AccessiblePatterns, AccessibleRead, PortError,
};
use ommatix_domain::DisplayMapping;
use ommatix_domain::bbox::BBox;
use uiautomation::core::UICacheRequest;
use uiautomation::types::{TreeScope, UIProperty};
use uiautomation::variants::Variant;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use crate::{ADAPTER, Budget, HOSTED_WINDOW_CLASS, Spent, is_frame_host_window, role_of};

/// The live mapping from frame pixels to desktop coordinates (the capturer's).
type MappingSource = Box<dyn Fn() -> Result<DisplayMapping, PortError> + Send + Sync>;

enum Request {
    Snapshot {
        mapping: DisplayMapping,
        regions: Vec<BBox>,
    },
    Focus {
        mapping: DisplayMapping,
    },
}

enum Answer {
    Elements(AccessibleRead),
    Focus(Option<AccessibleElement>),
}

type Reply = Result<Answer, String>;

/// Reads the foreground application's UI Automation tree for the
/// [`AccessibilityProvider`] port. Never sends input and never reads a value.
pub struct UiaProvider {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
    mapping: MappingSource,
}

impl fmt::Debug for UiaProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiaProvider").finish_non_exhaustive()
    }
}

impl UiaProvider {
    /// Starts the UI Automation client on its own thread. `mapping` maps frame pixels of
    /// the captured monitor to desktop coordinates (the capturer's live mapping).
    pub fn new(
        mapping: impl Fn() -> Result<DisplayMapping, PortError> + Send + Sync + 'static,
        budget: Budget,
    ) -> Result<Self, PortError> {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, replies) = mpsc::channel::<Reply>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        thread::Builder::new()
            .name("ommatix-uia".to_owned())
            .spawn(move || {
                let reader = match Reader::new(budget) {
                    Ok(reader) => {
                        let _ = ready_tx.send(Ok(()));
                        reader
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                for request in inbox {
                    let reply = Ok(match request {
                        Request::Snapshot { mapping, regions } => {
                            Answer::Elements(reader.snapshot(&mapping, &regions))
                        }
                        Request::Focus { mapping } => Answer::Focus(reader.focused(&mapping)),
                    });
                    if outbox.send(reply).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| {
                PortError::new(ADAPTER, "cannot start the UI Automation thread").with_source(e)
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                requests,
                replies,
                mapping: Box::new(mapping),
            }),
            Ok(Err(e)) => Err(PortError::new(
                ADAPTER,
                format!("UI Automation is unavailable: {e}"),
            )),
            Err(_) => Err(PortError::new(ADAPTER, "the UI Automation thread ended")),
        }
    }
}

impl AccessibilityProvider for UiaProvider {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn snapshot(&mut self, regions: &[BBox]) -> Result<AccessibleRead, PortError> {
        if regions.is_empty() {
            return Ok(AccessibleRead::complete(Vec::new()));
        }
        let mapping = (self.mapping)()?;
        match self.ask(Request::Snapshot {
            mapping,
            regions: regions.to_vec(),
        })? {
            Answer::Elements(read) => Ok(read),
            Answer::Focus(_) => Err(PortError::new(ADAPTER, "unexpected reply")),
        }
    }

    fn reads_keyboard_focus(&self) -> bool {
        true
    }

    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError> {
        let mapping = (self.mapping)()?;
        match self.ask(Request::Focus { mapping })? {
            Answer::Focus(element) => Ok(element),
            Answer::Elements(_) => Err(PortError::new(ADAPTER, "unexpected reply")),
        }
    }
}

impl UiaProvider {
    /// Sends one request to the worker thread and waits for its answer.
    fn ask(&mut self, request: Request) -> Result<Answer, PortError> {
        self.requests
            .send(request)
            .map_err(|_| PortError::new(ADAPTER, "the UI Automation thread ended"))?;
        self.replies
            .recv()
            .map_err(|_| PortError::new(ADAPTER, "the UI Automation thread ended"))?
            .map_err(|e| PortError::new(ADAPTER, e))
    }
}

/// The COM side, owned by the worker thread.
struct Reader {
    automation: UIAutomation,
    walker: UITreeWalker,
    root: UIElement,
    /// What deciding whether to read a window needs.
    light: UICacheRequest,
    /// What one element of the attested application gives.
    full: UICacheRequest,
    budget: Budget,
}

/// Properties fetched for every element of the attested application.
const FULL: [UIProperty; 14] = [
    UIProperty::ControlType,
    UIProperty::Name,
    UIProperty::BoundingRectangle,
    UIProperty::IsEnabled,
    UIProperty::IsOffscreen,
    UIProperty::IsPassword,
    UIProperty::ProcessId,
    UIProperty::IsInvokePatternAvailable,
    UIProperty::IsTogglePatternAvailable,
    UIProperty::IsExpandCollapsePatternAvailable,
    UIProperty::IsSelectionItemPatternAvailable,
    UIProperty::IsValuePatternAvailable,
    UIProperty::ValueIsReadOnly,
    UIProperty::IsScrollPatternAvailable,
];

/// Properties fetched for top-level windows and their children.
const LIGHT: [UIProperty; 4] = [
    UIProperty::ProcessId,
    UIProperty::ClassName,
    UIProperty::IsOffscreen,
    UIProperty::BoundingRectangle,
];

fn cache_request(
    automation: &UIAutomation,
    properties: &[UIProperty],
) -> uiautomation::Result<UICacheRequest> {
    let request = automation.create_cache_request()?;
    for p in properties {
        request.add_property(*p)?;
    }
    request.set_tree_scope(TreeScope::Element)?;
    Ok(request)
}

/// A cached boolean property; `false` when it is unsupported or unreadable.
fn flag(element: &UIElement, property: UIProperty) -> bool {
    element
        .get_cached_property_value(property)
        .ok()
        .and_then(|v: Variant| TryInto::<bool>::try_into(v).ok())
        .unwrap_or(false)
}

/// The element's rectangle on the captured monitor, in frame pixels; `Err(())` when the
/// element has a rectangle that is off this monitor, `Ok(None)` when it has none.
fn frame_box(element: &UIElement, mapping: &DisplayMapping) -> Result<Option<BBox>, ()> {
    let Ok(r) = element.get_cached_bounding_rectangle() else {
        return Ok(None);
    };
    let (l, t, rt, b) = (r.get_left(), r.get_top(), r.get_right(), r.get_bottom());
    if rt <= l || b <= t {
        return Ok(None);
    }
    mapping.frame_box(l, t, rt, b).map(Some).ok_or(())
}

impl Reader {
    fn new(budget: Budget) -> Result<Self, String> {
        let automation = UIAutomation::new().map_err(|e| e.to_string())?;
        let walker = automation
            .get_control_view_walker()
            .map_err(|e| e.to_string())?;
        let root = automation.get_root_element().map_err(|e| e.to_string())?;
        let light = cache_request(&automation, &LIGHT).map_err(|e| e.to_string())?;
        let full = cache_request(&automation, &FULL).map_err(|e| e.to_string())?;
        Ok(Self {
            automation,
            walker,
            root,
            light,
            full,
            budget,
        })
    }

    /// The children of `element` in the control view, with `cache`'s properties.
    fn children(&self, element: &UIElement, cache: &UICacheRequest) -> Vec<UIElement> {
        self.walker
            .get_children_build_cache(element, cache)
            .unwrap_or_default()
    }

    /// The top-level window (a child of the desktop) that contains `element`.
    fn top_level(&self, element: UIElement) -> Option<UIElement> {
        let mut current = element;
        for _ in 0..=self.budget.max_depth * 2 {
            let parent = self.walker.get_parent(&current).ok()?;
            if self.automation.compare_elements(&parent, &self.root).ok()? {
                return Some(current);
            }
            current = parent;
        }
        None
    }

    /// The process the foreground window belongs to, resolved through an
    /// `ApplicationFrameHost` frame; `None` when nothing has focus or the frame hosts no
    /// application.
    fn foreground_process(&self) -> Option<i32> {
        let focused = self.automation.get_focused_element().ok()?;
        self.process_of_window_holding(focused)
    }

    /// The application whose top-level window contains `focused` (see
    /// [`Self::foreground_process`]).
    fn process_of_window_holding(&self, focused: UIElement) -> Option<i32> {
        let top = self
            .top_level(focused)?
            .build_updated_cache(&self.light)
            .ok()?;
        let pid = top.get_cached_process_id().ok()?;
        let class = top.get_cached_classname().unwrap_or_default();
        if !is_frame_host_window(&class) {
            return Some(pid);
        }
        self.children(&top, &self.light)
            .into_iter()
            .find_map(|child| {
                let child_pid = child.get_cached_process_id().ok()?;
                let hosted = child.get_cached_classname().ok()? == HOSTED_WINDOW_CLASS;
                (hosted && child_pid != pid).then_some(child_pid)
            })
    }

    /// The windows of process `pid` to walk: its visible top-level windows and the hosted
    /// windows of the frames it lives in.
    fn windows_of(&self, pid: i32) -> Vec<UIElement> {
        let mut windows = Vec::new();
        for top in self.children(&self.root, &self.light) {
            if top.is_cached_offscreen().unwrap_or(true) {
                continue;
            }
            let class = top.get_cached_classname().unwrap_or_default();
            if is_frame_host_window(&class) {
                windows.extend(
                    self.children(&top, &self.light)
                        .into_iter()
                        .filter(|c| c.get_cached_process_id().ok() == Some(pid)),
                );
            } else if top.get_cached_process_id().ok() == Some(pid) {
                windows.push(top);
            }
        }
        windows
    }

    fn snapshot(&self, mapping: &DisplayMapping, regions: &[BBox]) -> AccessibleRead {
        self.foreground_process()
            .map(|pid| self.snapshot_of(pid, mapping, regions))
            .unwrap_or_default()
    }

    /// The element that has keyboard focus, when it belongs to the foreground application
    /// (the one a snapshot reads), is on screen and lies on the captured monitor. The
    /// element is read like any other, from cached properties, and its value never is.
    fn focused(&self, mapping: &DisplayMapping) -> Option<AccessibleElement> {
        let focused = self
            .automation
            .get_focused_element()
            .ok()?
            .build_updated_cache(&self.full)
            .ok()?;
        let pid = self.process_of_window_holding(focused.clone())?;
        if focused.get_cached_process_id().ok() != Some(pid)
            || focused.is_cached_offscreen().unwrap_or(true)
        {
            return None;
        }
        let bbox = frame_box(&focused, mapping).ok().flatten()?;
        Self::element(&focused, bbox)
    }

    /// The elements of process `pid`'s visible windows inside `regions`, within the budget.
    /// The read is `truncated` when a bound ended the walk or a window could not be read.
    fn snapshot_of(&self, pid: i32, mapping: &DisplayMapping, regions: &[BBox]) -> AccessibleRead {
        let start = Instant::now();
        let mut found = Vec::new();
        let mut spent = Spent::default();
        let mut truncated = false;
        for window in self.windows_of(pid) {
            let Ok(window) = window.build_updated_cache(&self.full) else {
                truncated = true;
                continue;
            };
            let mut stack = vec![(window, 0usize)];
            while let Some((element, depth)) = stack.pop() {
                spent.elapsed = start.elapsed();
                if !spent.allows_more(&self.budget) {
                    return AccessibleRead {
                        elements: found,
                        truncated: true,
                    };
                }
                spent.visited += 1;
                if element.get_cached_process_id().ok() != Some(pid)
                    || element.is_cached_offscreen().unwrap_or(true)
                {
                    continue;
                }
                let bbox = match frame_box(&element, mapping) {
                    Err(()) => continue,
                    Ok(Some(b)) if !regions.iter().any(|r| r.overlaps(&b)) => continue,
                    Ok(b) => b,
                };
                if let Some(bbox) = bbox
                    && let Some(e) = Self::element(&element, bbox)
                {
                    found.push(e);
                }
                let mut children = self.children(&element, &self.full);
                if depth < self.budget.max_depth {
                    children.reverse();
                    stack.extend(children.into_iter().map(|c| (c, depth + 1)));
                } else {
                    // The depth bound cuts the walk only where something lies below it.
                    truncated |= !children.is_empty();
                }
            }
        }
        AccessibleRead {
            elements: found,
            truncated,
        }
    }

    /// The element as evidence, when its control type is one the contract can use.
    fn element(element: &UIElement, bbox: BBox) -> Option<AccessibleElement> {
        let control_type: i32 = element
            .get_cached_property_value(UIProperty::ControlType)
            .ok()
            .and_then(|v| v.try_into().ok())?;
        let password = element.is_cached_password().unwrap_or(false);
        let patterns = AccessiblePatterns {
            invoke: flag(element, UIProperty::IsInvokePatternAvailable),
            toggle: flag(element, UIProperty::IsTogglePatternAvailable),
            expand_collapse: flag(element, UIProperty::IsExpandCollapsePatternAvailable),
            select: flag(element, UIProperty::IsSelectionItemPatternAvailable),
            editable_value: flag(element, UIProperty::IsValuePatternAvailable)
                && element
                    .get_cached_property_value(UIProperty::ValueIsReadOnly)
                    .ok()
                    .and_then(|v: Variant| TryInto::<bool>::try_into(v).ok())
                    == Some(false),
            scroll: flag(element, UIProperty::IsScrollPatternAvailable),
        };
        let role = role_of(control_type, patterns.editable_value, password)?;
        Some(AccessibleElement {
            bbox,
            role,
            name: element.get_cached_name().unwrap_or_default(),
            enabled: element.is_cached_enabled().unwrap_or(false),
            offscreen: false,
            password,
            patterns,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    use super::*;

    fn everything() -> (DisplayMapping, BBox) {
        (
            DisplayMapping::identity(7680, 4320).expect("a valid size"),
            BBox::new(0.0, 0.0, 7680.0, 4320.0).expect("a valid box"),
        )
    }

    /// Counts per role and per "would be confirmed", never a name.
    fn counts(elements: &[AccessibleElement]) -> BTreeMap<(String, bool), usize> {
        let mut counts = BTreeMap::new();
        for e in elements {
            let confirmed = e.enabled
                && !e.offscreen
                && if e.role == ommatix_domain::fusion::Role::Input {
                    e.patterns.editable_value && !e.password
                } else {
                    e.patterns.clickable()
                };
            *counts
                .entry((format!("{:?}", e.role), confirmed))
                .or_insert(0) += 1;
        }
        counts
    }

    /// Manual: reads the foreground application's tree on the primary monitor and prints
    /// counts per role only (never names). Run it with the window to inspect in front:
    /// `cargo test -p ommatix-adapter-accessibility --features uia -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the live desktop; run by hand"]
    fn reads_the_foreground_tree() {
        let (mapping, all) = everything();
        let source = move || Ok(mapping);
        let mut provider = UiaProvider::new(source, Budget::default()).unwrap();
        let t = Instant::now();
        let read = provider.snapshot(&[all]).unwrap();
        eprintln!(
            "{} elements in {:?} (truncated: {}): {:?}",
            read.elements.len(),
            t.elapsed(),
            read.truncated,
            counts(&read.elements)
        );
    }

    /// Manual: reads which element has keyboard focus (ADR-0018) and prints its role, state
    /// and size, never its name. Moves no focus and sends no input:
    /// `cargo test -p ommatix-adapter-accessibility --features uia -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the live desktop; run by hand"]
    fn reads_the_element_with_keyboard_focus() {
        let (mapping, _) = everything();
        let source = move || Ok(mapping);
        let mut provider = UiaProvider::new(source, Budget::default()).unwrap();
        assert!(provider.reads_keyboard_focus());
        let t = Instant::now();
        let focused = provider.keyboard_focus().unwrap();
        eprintln!(
            "focus read in {:?}: {:?}",
            t.elapsed(),
            focused.map(|e| (
                e.role,
                e.enabled,
                e.password,
                e.patterns.editable_value,
                e.bbox
            ))
        );
    }

    /// Kills (and reaps) the process the test started, and no other.
    struct Started(Child);

    impl Drop for Started {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Manual: starts `tools/s4/testapp` (built with `cargo build --release` there, or at
    /// `OMMATIX_TESTAPP`), reads the tree of that process only, prints counts per role and
    /// closes the process it started. Sends no input and needs no focus.
    #[test]
    #[ignore = "starts the S4 test application and reads its UI Automation tree"]
    fn reads_the_tree_of_the_s4_test_application() {
        let exe = std::env::var_os("OMMATIX_TESTAPP").map_or_else(
            || {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../tools/s4/testapp/target/release/ommatix-testapp.exe")
            },
            PathBuf::from,
        );
        let child = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {}: {e}", exe.display()));
        let pid = i32::try_from(child.id()).expect("a process id");
        let _started = Started(child);

        let (mapping, all) = everything();
        let reader = Reader::new(Budget::default()).unwrap();
        let mut elements = Vec::new();
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(250));
            elements = reader.snapshot_of(pid, &mapping, &[all]).elements;
            if elements.len() > 3 {
                break;
            }
        }
        let counts = counts(&elements);
        eprintln!("test application: {} elements: {counts:?}", elements.len());
        assert!(
            counts
                .keys()
                .any(|(role, confirmed)| role == "Button" && *confirmed),
            "no confirmed button: {counts:?}"
        );
        assert!(elements.iter().all(|e| !e.password));
    }
}
