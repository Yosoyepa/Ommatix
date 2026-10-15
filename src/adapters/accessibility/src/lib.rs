//! # ommatix-adapter-accessibility
//!
//! The driven adapter for the [`AccessibilityProvider`](ommatix_application::AccessibilityProvider)
//! port (ADR-0017): the UI Automation tree of the foreground window, read as evidence that
//! the engine fuses with the visual path.
//!
//! - `UiaProvider` (feature `uia`, Windows only): a client of UI Automation through the
//!   safe [`uiautomation`] crate, with no `unsafe` of its own. It runs on a thread of its
//!   own (COM multithreaded apartment) and reads only cached properties.
//!
//! The platform-neutral rules are here, safe Rust tested on every OS:
//!
//! - [`role_of`]: the UI Automation control type → contract role table.
//! - [`Budget`]: the bounds of one tree walk (depth, elements, time).
//! - [`is_frame_host_window`] and [`HOSTED_WINDOW_CLASS`]: the `ApplicationFrameHost`
//!   resolution the `SendInput` actuator also applies, so both attest the same
//!   application.
//!
//! What it never does: read a field's value (`ValuePattern.Value` is never requested),
//! read an element of another application than the foreground one, report an element the
//! screen does not show, or send input (the crate's input and pattern calls are forbidden
//! by this crate's `clippy.toml`).

use std::collections::VecDeque;
use std::time::Duration;

use ommatix_domain::fusion::Role;

#[cfg(all(windows, feature = "uia"))]
mod uia;

#[cfg(all(windows, feature = "uia"))]
pub use uia::UiaProvider;

/// The adapter's name in `PortError`s and summaries.
pub const ADAPTER: &str = "uia";

/// UI Automation control type identifiers (`UIA_*ControlTypeId`).
pub mod control_type {
    pub const BUTTON: i32 = 50_000;
    pub const CALENDAR: i32 = 50_001;
    pub const CHECK_BOX: i32 = 50_002;
    pub const COMBO_BOX: i32 = 50_003;
    pub const EDIT: i32 = 50_004;
    pub const HYPERLINK: i32 = 50_005;
    pub const IMAGE: i32 = 50_006;
    pub const LIST_ITEM: i32 = 50_007;
    pub const LIST: i32 = 50_008;
    pub const MENU: i32 = 50_009;
    pub const MENU_BAR: i32 = 50_010;
    pub const MENU_ITEM: i32 = 50_011;
    pub const PROGRESS_BAR: i32 = 50_012;
    pub const RADIO_BUTTON: i32 = 50_013;
    pub const SCROLL_BAR: i32 = 50_014;
    pub const SLIDER: i32 = 50_015;
    pub const SPINNER: i32 = 50_016;
    pub const STATUS_BAR: i32 = 50_017;
    pub const TAB: i32 = 50_018;
    pub const TAB_ITEM: i32 = 50_019;
    pub const TEXT: i32 = 50_020;
    pub const TOOL_BAR: i32 = 50_021;
    pub const TOOL_TIP: i32 = 50_022;
    pub const TREE: i32 = 50_023;
    pub const TREE_ITEM: i32 = 50_024;
    pub const CUSTOM: i32 = 50_025;
    pub const GROUP: i32 = 50_026;
    pub const THUMB: i32 = 50_027;
    pub const DATA_GRID: i32 = 50_028;
    pub const DATA_ITEM: i32 = 50_029;
    pub const DOCUMENT: i32 = 50_030;
    pub const SPLIT_BUTTON: i32 = 50_031;
    pub const WINDOW: i32 = 50_032;
    pub const PANE: i32 = 50_033;
    pub const HEADER: i32 = 50_034;
    pub const HEADER_ITEM: i32 = 50_035;
    pub const TABLE: i32 = 50_036;
    pub const TITLE_BAR: i32 = 50_037;
    pub const SEPARATOR: i32 = 50_038;
    pub const SEMANTIC_ZOOM: i32 = 50_039;
    pub const APP_BAR: i32 = 50_040;
}

/// The contract role of a UI Automation element, or `None` when the element is not a
/// control the agent could act on and is not reported at all (panes, groups, windows,
/// text, images, lists, menus, bars, scroll bars, sliders, ...).
///
/// - `Button`, `SplitButton` → `button`.
/// - `Edit` and `Document` → `input` only when their text can be set (`Value` available
///   and not read-only) and they are not password fields. A read-only, valueless or
///   password one is reported with role `unknown`: never a target for `type`.
/// - `CheckBox` → `checkbox`; `ComboBox` → `dropdown`.
/// - `RadioButton`, `MenuItem`, `TabItem`, `ListItem`, `TreeItem`, `DataItem`,
///   `Hyperlink`, `HeaderItem` → `unknown`: the contract has no role for them and calling
///   them buttons would invent one. Their affordance still comes from their patterns
///   (a tab or a menu item with `SelectionItem`/`ExpandCollapse`/`Invoke` is clickable).
pub const fn role_of(kind: i32, editable_value: bool, password: bool) -> Option<Role> {
    use crate::control_type as ct;
    match kind {
        ct::BUTTON | ct::SPLIT_BUTTON => Some(Role::Button),
        ct::EDIT | ct::DOCUMENT if editable_value && !password => Some(Role::Input),
        ct::CHECK_BOX => Some(Role::Checkbox),
        ct::COMBO_BOX => Some(Role::Dropdown),
        // A read-only, valueless or password edit, and items the contract has no role for.
        ct::EDIT
        | ct::DOCUMENT
        | ct::RADIO_BUTTON
        | ct::MENU_ITEM
        | ct::TAB_ITEM
        | ct::LIST_ITEM
        | ct::TREE_ITEM
        | ct::DATA_ITEM
        | ct::HYPERLINK
        | ct::HEADER_ITEM => Some(Role::Unknown),
        _ => None,
    }
}

/// The class of the top-level window `ApplicationFrameHost.exe` draws around a UWP
/// application.
pub const FRAME_HOST_CLASS: &str = "ApplicationFrameWindow";

/// The class of the hosted application's window inside that frame.
pub const HOSTED_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

/// Whether a top-level window is an `ApplicationFrameHost` frame, whose application is
/// the process of its hosted `CoreWindow` child, never the frame's own (the `SendInput`
/// actuator resolves it the same way).
pub fn is_frame_host_window(class_name: &str) -> bool {
    class_name == FRAME_HOST_CLASS
}

/// The bounds of one walk of the tree. Whatever bound is reached first ends the walk; the
/// elements found so far are still reported (less evidence, never wrong evidence: the
/// visual path covers the rest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Levels below a top-level window.
    pub max_depth: usize,
    /// Elements visited (reported or not), across all windows of the application.
    pub max_elements: usize,
    /// Wall time of the walk.
    pub max_duration: Duration,
}

impl Budget {
    pub const DEFAULT_MAX_DEPTH: usize = 32;
    pub const DEFAULT_MAX_ELEMENTS: usize = 2_000;
    pub const DEFAULT_MAX_DURATION: Duration = Duration::from_millis(250);
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_depth: Self::DEFAULT_MAX_DEPTH,
            max_elements: Self::DEFAULT_MAX_ELEMENTS,
            max_duration: Self::DEFAULT_MAX_DURATION,
        }
    }
}

/// Where a walk stands against its [`Budget`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    pub visited: usize,
    pub elapsed: Duration,
}

impl Spent {
    /// Whether the walk may visit one more element.
    pub fn allows_more(&self, budget: &Budget) -> bool {
        self.visited < budget.max_elements && self.elapsed < budget.max_duration
    }
}

/// How a bounded [`walk`] ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Walked {
    /// A bound (elements or time) ended the walk with nodes still to visit.
    pub out_of_budget: bool,
    /// The depth bound stopped the walk at a node that has children.
    pub depth_cut: bool,
}

/// Visits the tree under `root` **breadth first**, within `budget`.
///
/// Shallowest first, because a walk that ends on its budget loses the end of its order and
/// the controls an agent acts on (toolbars, address bars, tab strips, buttons) sit near the
/// top of an application's tree while its content (file lists, documents, long menus) sits
/// below. Depth first spent the whole budget on the first large subtree before the next
/// sibling's chrome was read (E2E-03, tenth round: File Explorer's navigation pane before
/// its address bar).
///
/// `enter` is called once per visited node, in order, and says whether to descend into it
/// (`false` skips its subtree); `children` is called only for a node that was entered.
/// `elapsed` is the walk's wall time so far; `spent` carries the count across calls, so one
/// budget can span several windows.
pub fn walk<N>(
    root: N,
    budget: &Budget,
    spent: &mut Spent,
    mut elapsed: impl FnMut() -> Duration,
    mut enter: impl FnMut(&N) -> bool,
    mut children: impl FnMut(&N) -> Vec<N>,
) -> Walked {
    let mut walked = Walked::default();
    let mut queue = VecDeque::from([(root, 0usize)]);
    while let Some((node, depth)) = queue.pop_front() {
        spent.elapsed = elapsed();
        if !spent.allows_more(budget) {
            walked.out_of_budget = true;
            break;
        }
        spent.visited += 1;
        if !enter(&node) {
            continue;
        }
        let below = children(&node);
        if depth < budget.max_depth {
            queue.extend(below.into_iter().map(|child| (child, depth + 1)));
        } else {
            // The depth bound cuts the walk only where something lies below it.
            walked.depth_cut |= !below.is_empty();
        }
    }
    walked
}

#[cfg(test)]
mod tests {
    use super::control_type as ct;
    use super::*;

    #[test]
    fn buttons_checkboxes_and_combo_boxes_keep_their_contract_roles() {
        assert_eq!(role_of(ct::BUTTON, false, false), Some(Role::Button));
        assert_eq!(role_of(ct::SPLIT_BUTTON, false, false), Some(Role::Button));
        assert_eq!(role_of(ct::CHECK_BOX, false, false), Some(Role::Checkbox));
        assert_eq!(role_of(ct::COMBO_BOX, true, false), Some(Role::Dropdown));
    }

    #[test]
    fn only_a_writable_non_password_edit_is_an_input() {
        for edit in [ct::EDIT, ct::DOCUMENT] {
            assert_eq!(role_of(edit, true, false), Some(Role::Input));
            assert_eq!(
                role_of(edit, false, false),
                Some(Role::Unknown),
                "read-only"
            );
            assert_eq!(role_of(edit, true, true), Some(Role::Unknown), "password");
        }
    }

    #[test]
    fn items_without_a_contract_role_are_unknown_not_buttons() {
        for item in [
            ct::RADIO_BUTTON,
            ct::MENU_ITEM,
            ct::TAB_ITEM,
            ct::LIST_ITEM,
            ct::TREE_ITEM,
            ct::DATA_ITEM,
            ct::HYPERLINK,
            ct::HEADER_ITEM,
        ] {
            assert_eq!(role_of(item, false, false), Some(Role::Unknown), "{item}");
        }
    }

    #[test]
    fn containers_text_and_decoration_are_not_reported() {
        for other in [
            ct::WINDOW,
            ct::PANE,
            ct::GROUP,
            ct::TEXT,
            ct::IMAGE,
            ct::LIST,
            ct::MENU,
            ct::MENU_BAR,
            ct::TOOL_BAR,
            ct::TAB,
            ct::SCROLL_BAR,
            ct::THUMB,
            ct::SLIDER,
            ct::TITLE_BAR,
            ct::CUSTOM,
            0,
            -1,
            99_999,
        ] {
            assert_eq!(role_of(other, true, false), None, "{other}");
        }
    }

    #[test]
    fn the_frame_host_is_recognised_by_its_class() {
        assert!(is_frame_host_window("ApplicationFrameWindow"));
        assert!(!is_frame_host_window("Notepad"));
        assert!(!is_frame_host_window(HOSTED_WINDOW_CLASS));
    }

    #[test]
    fn a_walk_stops_at_the_first_bound_reached() {
        let budget = Budget::default();
        assert!(Spent::default().allows_more(&budget));
        let full = Spent {
            visited: budget.max_elements,
            elapsed: Duration::ZERO,
        };
        assert!(!full.allows_more(&budget));
        let late = Spent {
            visited: 1,
            elapsed: budget.max_duration,
        };
        assert!(!late.allows_more(&budget));
    }

    /// A tree as child lists: node `i` has the children `tree[i]`.
    fn visit_order(
        tree: &[Vec<usize>],
        budget: &Budget,
        skip: &[usize],
        mut now: impl FnMut() -> Duration,
    ) -> (Vec<usize>, Walked) {
        let mut order = Vec::new();
        let mut spent = Spent::default();
        let walked = walk(
            0,
            budget,
            &mut spent,
            &mut now,
            |&n| {
                order.push(n);
                !skip.contains(&n)
            },
            |&n| tree[n].clone(),
        );
        assert_eq!(spent.visited, order.len());
        (order, walked)
    }

    /// Node 0 is a window with a toolbar (1, with buttons 4 and 5) and a long content list
    /// (2, with items 6..=9) and a status bar (3).
    fn window() -> Vec<Vec<usize>> {
        vec![
            vec![1, 2, 3],
            vec![4, 5],
            vec![6, 7, 8, 9],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ]
    }

    #[test]
    fn a_walk_reads_the_chrome_before_the_content() {
        let (order, walked) = visit_order(&window(), &Budget::default(), &[], || Duration::ZERO);
        assert_eq!(order, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(walked, Walked::default());
    }

    #[test]
    fn a_walk_cut_by_elements_loses_the_deepest_nodes_not_a_later_sibling() {
        // E2E-03: depth first would visit 0, 1, 4, 5, 2 and never reach the status bar (3);
        // breadth first has read every level above the cut.
        let budget = Budget {
            max_elements: 5,
            ..Budget::default()
        };
        let (order, walked) = visit_order(&window(), &budget, &[], || Duration::ZERO);
        assert_eq!(order, [0, 1, 2, 3, 4]);
        assert!(walked.out_of_budget);
    }

    #[test]
    fn a_walk_cut_by_time_stops_where_the_clock_runs_out() {
        let budget = Budget::default();
        let mut ticks = 0;
        let (order, walked) = visit_order(&window(), &budget, &[], || {
            ticks += 1;
            if ticks > 3 {
                budget.max_duration
            } else {
                Duration::ZERO
            }
        });
        assert_eq!(order, [0, 1, 2]);
        assert!(walked.out_of_budget);
    }

    #[test]
    fn a_skipped_subtree_is_not_entered() {
        let (order, walked) = visit_order(&window(), &Budget::default(), &[2], || Duration::ZERO);
        assert_eq!(order, [0, 1, 2, 3, 4, 5]);
        assert_eq!(walked, Walked::default());
    }

    #[test]
    fn the_depth_bound_is_reported_only_where_something_lies_below_it() {
        let budget = Budget {
            max_depth: 1,
            ..Budget::default()
        };
        let (order, walked) = visit_order(&window(), &budget, &[], || Duration::ZERO);
        assert_eq!(order, [0, 1, 2, 3], "nothing below depth 1 is visited");
        assert!(walked.depth_cut);
        assert!(!walked.out_of_budget);
        let flat = vec![vec![1, 2], vec![], vec![]];
        let (_, walked) = visit_order(&flat, &budget, &[], || Duration::ZERO);
        assert!(!walked.depth_cut, "the nodes at the bound have no children");
    }
}
