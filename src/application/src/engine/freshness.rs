//! Optional freshness scope (ADR-0032, amended by ADR-0035); all OS evidence comes through
//! the actuator port.

/// Which exact tile comparison may authorise an action. No tolerance in either scope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActionFreshness {
    #[default]
    Screen,
    TargetWindow,
}

use std::collections::{BTreeMap, BTreeSet};

use ommatix_domain::bbox::BBox;
use ommatix_domain::identity::{Registry, TargetId};
use ommatix_domain::incremental::{
    BgraFrame, TileDiff, TileGrid, dirty_tiles, dirty_tiles_in_regions,
};

use super::Engine;
use crate::contract::{ErrorCode, FreshnessDiff, ToolError};
use crate::ports::{OsActuator, TopLevelWindow};

/// A complete, explicitly sorted read. Rectangle failures never mean an empty group.
#[derive(Debug, Clone, PartialEq)]
struct WindowRead {
    foreground: u64,
    windows: Vec<TopLevelWindow>,
}

fn same_window(a: &TopLevelWindow, b: &TopLevelWindow) -> bool {
    a.handle == b.handle
        && a.class == b.class
        && match (a.rectangle, b.rectangle) {
            (Some(a), Some(b)) => a == b,
            _ => true, // the complete window list attests the rectangle, even if the point query cannot
        }
}

impl WindowRead {
    fn read(actuator: &dyn OsActuator) -> Option<Self> {
        let foreground = actuator.foreground_window()?;
        let mut windows = actuator.top_level_windows()?;
        let again = actuator.foreground_window()?;
        if !same_window(&foreground, &again) {
            return None;
        }
        let mut seen = BTreeSet::new();
        if windows.is_empty()
            || windows
                .iter()
                .any(|w| w.handle == 0 || w.rectangle.is_none() || !seen.insert(w.handle))
        {
            return None;
        }
        windows.sort_by_key(|w| (w.z_order, w.handle));
        windows.iter().find(|w| same_window(w, &foreground))?;
        Some(Self {
            foreground: foreground.handle,
            windows,
        })
    }

    /// The ADR-0032 group: the target, the foreground window and every window overlapping
    /// either. The engine no longer uses it; the M1 probe keeps it as its baseline.
    fn group(&self, target: &TopLevelWindow) -> Option<WindowGroup> {
        let target = self.windows.iter().find(|w| same_window(w, target))?;
        let target_rect = target.rectangle?;
        let foreground = self.windows.iter().find(|w| w.handle == self.foreground)?;
        let foreground_rect = foreground.rectangle?;
        let windows = self
            .windows
            .iter()
            .filter(|w| {
                w.handle == target.handle
                    || w.handle == self.foreground
                    || w.rectangle
                        .is_some_and(|r| r.overlaps(&target_rect) || r.overlaps(&foreground_rect))
            })
            .cloned()
            .collect();
        Some(WindowGroup {
            target: target.handle,
            foreground: self.foreground,
            windows,
        })
    }
}

impl WindowRead {
    /// The ADR-0035 group of `target`: the target window plus every visible window above it
    /// whose rectangle overlaps the target's rectangle grown by `margin`. Windows below it are
    /// not members, and neither is the foreground window unless it meets the same rule; the
    /// foreground handle stays in the identity. Members are ranked by their order in the
    /// group, so a window that closes elsewhere in the list does not change the identity.
    fn occluders(&self, target: &TopLevelWindow, margin: u32) -> Option<WindowGroup> {
        let target = self.windows.iter().find(|w| same_window(w, target))?;
        let reach = target.rectangle?.pad(f64::from(margin));
        let key = (target.z_order, target.handle);
        let windows = self
            .windows
            .iter()
            .filter(|w| {
                w.handle == target.handle
                    || ((w.z_order, w.handle) < key
                        && w.rectangle.is_some_and(|r| r.overlaps(&reach)))
            })
            .zip(0..)
            .map(|(w, rank)| TopLevelWindow {
                z_order: rank,
                ..w.clone()
            })
            .collect();
        Some(WindowGroup {
            target: target.handle,
            foreground: self.foreground,
            windows,
        })
    }
}

/// The immutable OS evidence belonging to this observation, never emitted on the wire.
#[derive(Debug, Clone)]
pub(super) struct WindowSnapshot {
    read: WindowRead,
    targets: BTreeMap<TargetId, u64>,
}

impl WindowSnapshot {
    pub(super) fn sizes(&self) -> (usize, usize) {
        (self.read.windows.len(), self.targets.len())
    }
}

/// Identity includes both seed handles and the ordered (z-order, handle, rectangle) list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct WindowGroup {
    target: u64,
    foreground: u64,
    windows: Vec<TopLevelWindow>,
}

impl WindowGroup {
    fn contains_target(&self, bbox: &BBox) -> bool {
        self.windows
            .iter()
            .find(|w| w.handle == self.target)
            .and_then(|w| w.rectangle)
            .is_some_and(|rect| rect.contains(bbox))
    }

    /// Whether the target window is attested opaque over `bbox` (ADR-0035): the box lies
    /// inside the window, the actuator attests the window opaque, and the box does not meet
    /// one of the window's corner squares, where the window manager may draw transparently.
    fn opaque_over(&self, bbox: &BBox, actuator: &dyn OsActuator) -> bool {
        self.attest(bbox, actuator).0.is_ok()
    }

    /// The same decision with its reason, and whether the actuator attested the opacity at
    /// all. The actuator is always asked once, as the engine's decision needs it.
    fn attest(
        &self,
        bbox: &BBox,
        actuator: &dyn OsActuator,
    ) -> (Result<(), OccluderFallback>, bool) {
        let Some(target) = self.windows.iter().find(|w| w.handle == self.target) else {
            return (Err(OccluderFallback::UnattestedRect), false);
        };
        let opacity = actuator.window_opacity(target);
        let attested = opacity.is_some();
        let (Some(rect), Some(opacity)) = (target.rectangle, opacity) else {
            let reason = if target.rectangle.is_none() {
                OccluderFallback::UnattestedRect
            } else {
                OccluderFallback::NoOpacity
            };
            return (Err(reason), attested);
        };
        let corner = f64::from(opacity.corner_exclusion_px);
        let near_x = bbox.x0() < rect.x0() + corner || bbox.x1() > rect.x1() - corner;
        let near_y = bbox.y0() < rect.y0() + corner || bbox.y1() > rect.y1() - corner;
        let reason = if !rect.contains(bbox) {
            Err(OccluderFallback::BoxOutsideTarget)
        } else if near_x && near_y {
            Err(OccluderFallback::Corner)
        } else {
            Ok(())
        };
        (reason, true)
    }

    fn touches(&self, tile: &BBox, margin: u32, bounds: &BBox) -> bool {
        self.windows
            .iter()
            .filter_map(|w| w.rectangle)
            .filter_map(|r| r.pad(f64::from(margin)).clip(bounds))
            .any(|r| r.overlaps(tile))
    }
}

/// Why the ADR-0035 scope would compare the whole frame instead of the group. Only these
/// names leave the probe (ADR-0012): never a window title, class, handle or coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccluderFallback {
    /// The window walk failed. Reserved: the probe aborts a pair whose walk fails (the
    /// ADR-0032 path) before this could be recorded.
    WalkFailed,
    /// The window under the target, or its rectangle, is not attested. Reserved, like
    /// `WalkFailed`: after a successful read the complete window read and the ADR-0032
    /// guards already require those rectangles, so the probe aborts such a pair first.
    UnattestedRect,
    /// The reads around the captures disagree on the amended group or its attestation.
    RepeatedReadsDiffer,
    /// The target box does not lie inside the target window. Reserved, like `WalkFailed`:
    /// the probe aborts such a pair first.
    BoxOutsideTarget,
    /// The actuator does not attest the target window opaque.
    NoOpacity,
    /// The target box meets a corner square of the target window.
    Corner,
}

impl OccluderFallback {
    /// The enum string the report carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WalkFailed => "walk_failed",
            Self::UnattestedRect => "unattested_rect",
            Self::RepeatedReadsDiffer => "reads_differ",
            Self::BoxOutsideTarget => "box_outside_target",
            Self::NoOpacity => "no_opacity",
            Self::Corner => "corner",
        }
    }
}

/// What one read says about the ADR-0035 group of the probed target: the group, and whether
/// the scope would apply (the engine's own `attest`).
#[derive(Debug, Clone, PartialEq)]
struct OccluderRead {
    /// The margin the group was selected with; the counts use the same one (no second
    /// margin exists), and two reads at different margins are different reads.
    margin: u32,
    group: Option<WindowGroup>,
    attested: Result<(), OccluderFallback>,
    opacity_attested: bool,
}

/// Opaque, read-only attestation for the operator's M1 measurement (ADR-0032), carrying the
/// ADR-0035 group of the same read for the paired repeat. Both groups come from the engine's
/// own complete window read and group selection; the equality of two attestations is that of
/// the ADR-0032 group and target, as before.
#[derive(Debug, Clone)]
pub struct FreshnessProbeGroup {
    group: WindowGroup,
    target: BBox,
    occluder: OccluderRead,
}

impl PartialEq for FreshnessProbeGroup {
    fn eq(&self, other: &Self) -> bool {
        self.group == other.group && self.target == other.target
    }
}

/// Counts only; no window identities, rectangles or screen content leave the probe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreshnessProbeCounts {
    pub total_tiles: usize,
    pub group_tiles: usize,
    pub touches_target: bool,
    pub group_window_count: usize,
    pub group_frame_fraction: f64,
    pub group_covers_whole_frame: bool,
}

/// The ADR-0035 group's counts for one pair; numbers, flags and one enum string only. The
/// group counts are absent when the group itself is not attested.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OccluderProbeCounts {
    pub group_tiles: Option<usize>,
    pub group_window_count: Option<usize>,
    pub group_frame_fraction: Option<f64>,
    pub group_covers_whole_frame: Option<bool>,
    pub opacity_attested: bool,
    /// `None`: the scoped comparison would apply. `Some`: the whole frame would be compared.
    pub fallback: Option<OccluderFallback>,
}

impl OccluderProbeCounts {
    pub const fn scoped(&self) -> bool {
        self.fallback.is_none()
    }
}

impl FreshnessProbeGroup {
    /// Require a non-live reader and a complete, stable read containing the target box.
    /// The caller brackets each capture with equal attestations; no delivery is called.
    pub fn read(reader: &dyn OsActuator, target: BBox) -> Option<Self> {
        Self::read_with_margin(reader, target, 0)
    }

    /// [`read`](Self::read), also selecting the ADR-0035 group with the engine's margin.
    pub fn read_with_margin(reader: &dyn OsActuator, target: BBox, margin: u32) -> Option<Self> {
        if reader.is_live() {
            return None;
        }
        let first = WindowRead::read(reader)?;
        let window = reader.window_at(target.center())?;
        let group = first.group(&window)?;
        if !group.contains_target(&target) || WindowRead::read(reader)? != first {
            return None;
        }
        let occluders = first.occluders(&window, margin);
        let (attested, opacity_attested) = occluders
            .as_ref()
            .map_or((Err(OccluderFallback::UnattestedRect), false), |g| {
                g.attest(&target, reader)
            });
        Some(Self {
            group,
            target,
            occluder: OccluderRead {
                margin,
                group: occluders,
                attested,
                opacity_attested,
            },
        })
    }

    /// Whether two attestations agree on the ADR-0035 group and on whether its scope applies.
    pub fn same_occluders(&self, other: &Self) -> bool {
        self.occluder == other.occluder
    }

    /// The ADR-0035 counts of one pair, next to [`measure`](Self::measure): what the amended
    /// group would compare, and why the scope would fall back to the whole frame, if it
    /// would. The margin is the one the group was selected with in
    /// [`read_with_margin`](Self::read_with_margin) (zero after [`read`](Self::read)): it
    /// cannot differ between selection and measurement. `reads_stable` is false when the
    /// attestations around the captures disagreed. Changed geometry/epoch or a target outside
    /// the frame has no valid measurement.
    pub fn measure_occluder(
        &self,
        base: &BgraFrame,
        current: &BgraFrame,
        reads_stable: bool,
    ) -> Option<OccluderProbeCounts> {
        let margin = self.occluder.margin;
        let bounds = current.bounds();
        if !bounds.contains(&self.target) {
            return None;
        }
        let total = dirty_tiles(Some(base), current, 64).ok()?;
        if total.full_refresh.is_some() {
            return None;
        }
        let fallback = if self.occluder.group.is_none() {
            Some(OccluderFallback::UnattestedRect)
        } else if !reads_stable {
            Some(OccluderFallback::RepeatedReadsDiffer)
        } else {
            self.occluder.attested.err()
        };
        let mut counts = OccluderProbeCounts {
            group_tiles: None,
            group_window_count: None,
            group_frame_fraction: None,
            group_covers_whole_frame: None,
            opacity_attested: self.occluder.opacity_attested,
            fallback,
        };
        if let Some(group) = &self.occluder.group {
            let regions: Vec<_> = group
                .windows
                .iter()
                .filter_map(|w| w.rectangle)
                .filter_map(|r| r.pad(f64::from(margin)).clip(&bounds))
                .collect();
            let scoped = dirty_tiles_in_regions(Some(base), current, 64, &regions).ok()?;
            if scoped.full_refresh.is_some() {
                return None;
            }
            let area = rectangle_union_area(&regions);
            counts.group_tiles = Some(scoped.tiles.len());
            counts.group_window_count = Some(group.windows.len());
            counts.group_frame_fraction = Some(area / bounds.area());
            counts.group_covers_whole_frame = Some(area >= bounds.area());
        }
        Some(counts)
    }

    /// Compare one pair at the thesis's 64-pixel tile size. Changed geometry/epoch or
    /// a target outside the frame has no valid measurement, never an invented zero.
    pub fn measure(
        &self,
        base: &BgraFrame,
        current: &BgraFrame,
        margin: u32,
    ) -> Option<FreshnessProbeCounts> {
        let bounds = current.bounds();
        if !bounds.contains(&self.target) {
            return None;
        }
        let total = dirty_tiles(Some(base), current, 64).ok()?;
        if total.full_refresh.is_some() {
            return None;
        }
        let regions: Vec<_> = self
            .group
            .windows
            .iter()
            .filter_map(|w| w.rectangle)
            .filter_map(|r| r.pad(f64::from(margin)).clip(&bounds))
            .collect();
        let scoped = dirty_tiles_in_regions(Some(base), current, 64, &regions).ok()?;
        if scoped.full_refresh.is_some() {
            return None;
        }
        let grid = TileGrid::for_frame(current, 64).ok()?;
        let area = rectangle_union_area(&regions);
        Some(FreshnessProbeCounts {
            total_tiles: total.tiles.len(),
            group_tiles: scoped.tiles.len(),
            touches_target: total
                .tiles
                .iter()
                .any(|t| grid.tile_box(*t).overlaps(&self.target)),
            group_window_count: self.group.windows.len(),
            group_frame_fraction: area / bounds.area(),
            group_covers_whole_frame: area >= bounds.area(),
        })
    }
}

/// Exact union area of clipped rectangles: overlapping/background windows count once.
fn rectangle_union_area(rectangles: &[BBox]) -> f64 {
    let mut edges: Vec<_> = rectangles.iter().flat_map(|r| [r.x0(), r.x1()]).collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    edges
        .windows(2)
        .map(|span| {
            let mut intervals: Vec<_> = rectangles
                .iter()
                .filter(|r| r.x0() < span[1] && r.x1() > span[0])
                .map(|r| (r.y0(), r.y1()))
                .collect();
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let mut end = f64::NEG_INFINITY;
            let mut height = 0.0;
            for (start, next) in intervals {
                height += (next - start.max(end)).max(0.0);
                end = end.max(next);
            }
            (span[1] - span[0]) * height
        })
        .sum()
}

pub(super) struct FreshFrame {
    pub frame: BgraFrame,
    diff: TileDiff,
    expected: Option<WindowGroup>,
    /// The scope was attested around the capture (both reads equal and usable, opacity
    /// included), so the comparison was regional. Separate from `expected`, which is only
    /// the observed identity.
    scoped: bool,
    known_group_change: bool,
}

fn changed_screen() -> ToolError {
    ToolError::new(
        ErrorCode::StaleSnapshot,
        "the screen changed since the observation; nothing was delivered; observe again",
    )
}

impl Engine {
    /// Reads around a capture; disabled scope does not query the OS at all.
    pub(super) fn window_read(&self) -> Option<WindowSnapshot> {
        if self.config.action_freshness != ActionFreshness::TargetWindow {
            return None;
        }
        Some(WindowSnapshot {
            read: WindowRead::read(self.ports.actuator.as_ref())?,
            targets: BTreeMap::new(),
        })
    }

    pub(super) fn windows_unchanged(
        &self,
        first: Option<WindowSnapshot>,
    ) -> Option<WindowSnapshot> {
        let first = first?;
        (WindowRead::read(self.ports.actuator.as_ref()).as_ref() == Some(&first.read))
            .then_some(first)
    }

    /// Bind each track to the OS window under its point in this observation. A read
    /// after inference must still agree with the two reads around its frame capture.
    pub(super) fn observation_windows(
        &self,
        windows: Option<WindowSnapshot>,
        registry: &Registry,
    ) -> Option<WindowSnapshot> {
        let mut windows = windows?;
        for track in registry.tracks() {
            let bbox = track.observation.bbox;
            if let Some(target) = self.ports.actuator.window_at(bbox.center())
                && windows.read.windows.iter().any(|w| {
                    same_window(w, &target) && w.rectangle.is_some_and(|rect| rect.contains(&bbox))
                })
            {
                windows.targets.insert(track.target_id, target.handle);
            }
        }
        self.windows_unchanged(Some(windows))
    }

    fn observed_group(&self, target: TargetId) -> Option<WindowGroup> {
        if self.config.action_freshness != ActionFreshness::TargetWindow {
            return None;
        }
        let windows = self.store.published()?.windows.as_ref()?;
        let handle = windows.targets.get(&target)?;
        let window = windows.read.windows.iter().find(|w| w.handle == *handle)?;
        windows
            .read
            .occluders(window, self.config.action_freshness_margin_px)
    }

    /// Preserve positive identity evidence separately from scope availability. A later
    /// failed read, or a target box that no longer fits, cannot erase a known change.
    fn current_group(&self, bbox: &BBox, expected: &WindowGroup) -> (Option<WindowGroup>, bool) {
        let margin = self.config.action_freshness_margin_px;
        let read = WindowRead::read(self.ports.actuator.as_ref());
        let identity = read
            .as_ref()
            .and_then(|r| r.occluders(&self.ports.actuator.window_at(bbox.center())?, margin));
        let known_change = identity.as_ref().is_some_and(|group| group != expected)
            || (identity.is_none()
                && read.as_ref().is_some_and(|r| {
                    let observed_now = r
                        .windows
                        .iter()
                        .find(|w| w.handle == expected.target)
                        .and_then(|window| r.occluders(window, margin));
                    observed_now.as_ref() != Some(expected)
                }));
        // An opacity the actuator cannot attest is not a change: it only costs the scope.
        let usable = identity.filter(|group| group.opaque_over(bbox, self.ports.actuator.as_ref()));
        (usable, known_change)
    }

    /// Compare only intersecting tiles when the window group is attested. D0 counts
    /// the whole frame on refusal; geometry always stays global.
    pub(super) fn fresh_frame(
        &mut self,
        target: TargetId,
        bbox: &BBox,
    ) -> Result<FreshFrame, ToolError> {
        let expected = self.observed_group(target);
        let (first, first_changed) = expected
            .as_ref()
            .map_or((None, false), |group| self.current_group(bbox, group));
        // Spend immediately: a later capture/policy error cannot revive known stale evidence.
        self.snapshot_spent |= first_changed;
        let captured = self.ports.capturer.capture().map_err(|_| {
            if first_changed {
                return changed_screen();
            }
            ToolError::new(
                ErrorCode::CaptureUnavailable,
                "the screen could not be captured to confirm the snapshot; nothing was delivered",
            )
        })?;
        let (second, second_changed) = expected
            .as_ref()
            .map_or((None, false), |group| self.current_group(bbox, group));
        let known_group_change = first_changed || second_changed;
        self.snapshot_spent |= known_group_change;
        // Different reads around capture or missing attestation require the whole frame.
        // Keep their known identity changes even when the scope cannot be used.
        let captured_group =
            first.filter(|g| Some(g) == second.as_ref() && g.contains_target(bbox));
        let scoped = captured_group.is_some();
        let scope = expected.as_ref().filter(|_| scoped);
        let regions: Vec<BBox> = scope
            .into_iter()
            .flat_map(|g| &g.windows)
            .filter_map(|w| w.rectangle)
            .filter_map(|r| {
                r.pad(f64::from(self.config.action_freshness_margin_px))
                    .clip(&captured.frame.bounds())
            })
            .collect();
        let diff = if scope.is_some() {
            dirty_tiles_in_regions(
                self.store.base(),
                &captured.frame,
                self.config.plan.tile_size,
                &regions,
            )
        } else {
            dirty_tiles(
                self.store.base(),
                &captured.frame,
                self.config.plan.tile_size,
            )
        }
        .map_err(|_| {
            self.snapshot_spent = true;
            changed_screen()
        })?;
        let changed = diff.full_refresh.is_some() || !diff.tiles.is_empty();
        if changed {
            let scope = scope.filter(|_| diff.full_refresh.is_none());
            return Err(self.freshness_refusal(&captured.frame, &diff, bbox, scope, false));
        }
        Ok(FreshFrame {
            frame: captured.frame,
            diff,
            expected,
            scoped,
            known_group_change,
        })
    }

    /// Runs after the existing last look: an application/policy change keeps its code.
    pub(super) fn check_freshness_group(
        &mut self,
        fresh: &FreshFrame,
        bbox: &BBox,
        foreground: Option<&str>,
        target: TargetId,
    ) -> Result<(), ToolError> {
        let Some(expected) = fresh.expected.as_ref() else {
            return Ok(());
        };
        let (current, known_change) = self.current_group(bbox, expected);
        self.snapshot_spent |= known_change;
        // The new OS walk can take time. Re-read consent, stop, application/window scope
        // and age after it, before comparing identity or handing anything to the actuator.
        self.last_look(foreground, bbox.center(), target)?;
        if fresh.known_group_change || known_change {
            // The identity refusal keeps its code and spends the snapshot; the group is
            // reported only while the scope was usable at every look, opacity included.
            let attested = (fresh.scoped && current.is_some()).then_some(expected);
            return Err(self.freshness_refusal(
                &fresh.frame,
                &fresh.diff,
                bbox,
                attested,
                fresh.scoped,
            ));
        }
        // Losing final attestation requires an actual whole-frame comparison: ignored
        // pixels cannot be authorised by a scoped diff, and no second capture is needed.
        if current.is_none() {
            let full = dirty_tiles(self.store.base(), &fresh.frame, self.config.plan.tile_size)
                .map_err(|_| {
                    self.snapshot_spent = true;
                    changed_screen()
                })?;
            if full.full_refresh.is_some() || !full.tiles.is_empty() {
                return Err(self.freshness_refusal(&fresh.frame, &full, bbox, None, false));
            }
        }
        Ok(())
    }

    fn freshness_refusal(
        &mut self,
        frame: &BgraFrame,
        diff: &TileDiff,
        target: &BBox,
        group: Option<&WindowGroup>,
        regional: bool,
    ) -> ToolError {
        self.snapshot_spent = true;
        // Only a refusal pays for the global diagnostic scan in target-window mode. It is
        // needed whenever the supplied diff is regional, whether or not the group may be
        // reported: the global count must not depend on the scope's availability.
        let full = (group.is_some() || regional)
            .then(|| dirty_tiles(self.store.base(), frame, self.config.plan.tile_size).ok())
            .flatten();
        let diff = full.as_ref().unwrap_or(diff);
        if let (Ok(grid), Some(timings)) = (
            TileGrid::for_frame(frame, self.config.plan.tile_size),
            self.action_timings.as_mut(),
        ) {
            timings.freshness_diff = Some(FreshnessDiff {
                total_tiles: diff.tiles.len(),
                group_tiles: group.map(|g| {
                    diff.tiles
                        .iter()
                        .filter(|&&tile| {
                            g.touches(
                                &grid.tile_box(tile),
                                self.config.action_freshness_margin_px,
                                &frame.bounds(),
                            )
                        })
                        .count()
                }),
                touches_target: diff
                    .tiles
                    .iter()
                    .any(|&tile| grid.tile_box(tile).overlaps(target)),
            });
        }
        changed_screen()
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    use crate::contract::ValidatedAction;
    use crate::ports::WindowOpacity;
    use crate::{DeliveryReceipt, PortError};

    struct Reader {
        windows: Vec<TopLevelWindow>,
        live: bool,
        /// What the reader attests about the opacity of any window (ADR-0035).
        opacity: Option<WindowOpacity>,
        /// The foreground window and the window under the target: by default the first.
        target: u64,
    }
    impl Reader {
        fn new(windows: Vec<TopLevelWindow>) -> Self {
            let target = windows.first().map_or(0, |w| w.handle);
            Self {
                windows,
                live: false,
                opacity: None,
                target,
            }
        }
        fn opaque(mut self, corner_exclusion_px: u32) -> Self {
            self.opacity = Some(WindowOpacity {
                corner_exclusion_px,
            });
            self
        }
        fn at(mut self, handle: u64) -> Self {
            self.target = handle;
            self
        }
    }
    impl OsActuator for Reader {
        fn name(&self) -> &'static str {
            "synthetic-reader"
        }
        fn is_live(&self) -> bool {
            self.live
        }
        fn foreground_window(&self) -> Option<TopLevelWindow> {
            self.windows
                .iter()
                .find(|w| w.handle == self.target)
                .cloned()
        }
        fn window_opacity(&self, _: &TopLevelWindow) -> Option<WindowOpacity> {
            self.opacity
        }
        fn window_at(&self, _: (f64, f64)) -> Option<TopLevelWindow> {
            self.foreground_window()
        }
        fn top_level_windows(&self) -> Option<Vec<TopLevelWindow>> {
            Some(self.windows.clone())
        }
        fn deliver(&mut self, _: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
            panic!("a measurement must never request delivery")
        }
    }
    fn box_at(x: f64, y: f64, right: f64, bottom: f64) -> BBox {
        BBox::new(x, y, right, bottom).unwrap()
    }
    fn window(handle: u64, rectangle: BBox) -> TopLevelWindow {
        TopLevelWindow {
            handle,
            rectangle: Some(rectangle),
            z_order: 0,
            class: "Test".into(),
        }
    }
    fn frame(epoch: u64, pixel: Option<(usize, usize)>) -> BgraFrame {
        let mut pixels = vec![0; 256 * 128 * 4];
        if let Some((x, y)) = pixel {
            pixels[(y * 256 + x) * 4] = 1;
        }
        BgraFrame::new(256, 128, 256 * 4, epoch, pixels).unwrap()
    }
    fn group(windows: Vec<TopLevelWindow>) -> FreshnessProbeGroup {
        FreshnessProbeGroup::read(&Reader::new(windows), box_at(8.0, 8.0, 16.0, 16.0)).unwrap()
    }
    fn at_z(handle: u64, z_order: u32, rectangle: BBox) -> TopLevelWindow {
        TopLevelWindow {
            z_order,
            ..window(handle, rectangle)
        }
    }
    /// T (handle 1, z 1) with a window above it that overlaps it (2), a window below it that
    /// overlaps it (3) and one above that overlaps nothing (4).
    fn stack() -> Vec<TopLevelWindow> {
        vec![
            at_z(2, 0, box_at(40.0, 0.0, 120.0, 64.0)),
            at_z(4, 0, box_at(200.0, 0.0, 256.0, 40.0)),
            at_z(1, 1, box_at(0.0, 0.0, 64.0, 64.0)),
            at_z(3, 2, box_at(32.0, 40.0, 96.0, 100.0)),
        ]
    }
    fn read_stack(reader: &Reader, margin: u32) -> FreshnessProbeGroup {
        FreshnessProbeGroup::read_with_margin(reader, box_at(8.0, 8.0, 16.0, 16.0), margin).unwrap()
    }

    #[test]
    fn probe_counts_exact_tiles_inside_outside_and_at_the_group_boundary() {
        let group = group(vec![window(1, box_at(0.0, 0.0, 64.0, 64.0))]);
        let base = frame(1, None);
        for (pixel, group_tiles, touches) in [
            ((8, 8), 1, true),
            ((63, 63), 1, true),
            ((64, 0), 0, false),
            ((255, 127), 0, false),
        ] {
            let counts = group.measure(&base, &frame(1, Some(pixel)), 0).unwrap();
            assert_eq!(
                (
                    counts.total_tiles,
                    counts.group_tiles,
                    counts.touches_target
                ),
                (1, group_tiles, touches)
            );
            assert_eq!(counts.group_window_count, 1);
            assert!((counts.group_frame_fraction - 0.125).abs() < f64::EPSILON);
            assert!(!counts.group_covers_whole_frame);
        }
        let counts = group.measure(&base, &frame(1, Some((64, 0))), 1).unwrap();
        assert_eq!(
            counts.group_tiles, 1,
            "the same margin as the engine includes this tile"
        );
    }

    #[test]
    fn probe_uses_the_engines_one_hop_group_and_counts_overlapping_areas_once() {
        let group = group(vec![
            window(1, box_at(0.0, 0.0, 64.0, 64.0)),
            window(2, box_at(32.0, 0.0, 96.0, 64.0)),
            window(3, box_at(80.0, 0.0, 160.0, 64.0)),
        ]);
        let counts = group
            .measure(&frame(1, None), &frame(1, Some((128, 0))), 0)
            .unwrap();
        assert_eq!(
            counts.group_window_count, 2,
            "the third window overlaps only the second"
        );
        assert_eq!(counts.group_tiles, 0);
        assert!((counts.group_frame_fraction - 0.1875).abs() < f64::EPSILON);
    }

    #[test]
    fn probe_reports_whole_frame_coverage_from_background_windows() {
        let group = group(vec![
            window(1, box_at(0.0, 0.0, 64.0, 64.0)),
            window(2, box_at(-20.0, -10.0, 300.0, 150.0)),
        ]);
        let counts = group
            .measure(&frame(1, None), &frame(1, Some((255, 127))), 0)
            .unwrap();
        assert!(counts.group_covers_whole_frame);
        assert!((counts.group_frame_fraction - 1.0).abs() < f64::EPSILON);
        assert_eq!(counts.group_tiles, counts.total_tiles);
    }

    #[test]
    fn probe_refuses_live_readers_missing_rectangles_and_geometry_changes() {
        let mut reader = Reader::new(vec![window(1, box_at(0.0, 0.0, 64.0, 64.0))]);
        reader.live = true;
        let target = box_at(8.0, 8.0, 16.0, 16.0);
        assert!(FreshnessProbeGroup::read(&reader, target).is_none());
        reader.live = false;
        let group = FreshnessProbeGroup::read(&reader, target).unwrap();
        assert!(group.measure(&frame(1, None), &frame(2, None), 0).is_none());
        reader.windows[0].rectangle = None;
        assert!(FreshnessProbeGroup::read(&reader, target).is_none());
        reader.windows.clear();
        assert!(FreshnessProbeGroup::read(&reader, target).is_none());
    }

    /// ADR-0035 (paired M1): the ADR-0032 fields stay as they were and the amended group is
    /// reported beside them, from the same read.
    #[test]
    fn probe_reports_the_adr_0035_group_beside_the_adr_0032_one() {
        let reader = Reader::new(stack()).at(1).opaque(0);
        let group = read_stack(&reader, 0);
        // A change in the window below T only: ADR-0032 counts it, ADR-0035 does not.
        let counts = group
            .measure(&frame(1, None), &frame(1, Some((80, 90))), 0)
            .unwrap();
        let occluder = group
            .measure_occluder(&frame(1, None), &frame(1, Some((80, 90))), true)
            .unwrap();
        assert_eq!(
            (
                counts.total_tiles,
                counts.group_tiles,
                counts.group_window_count
            ),
            (1, 1, 3)
        );
        assert_eq!(
            (occluder.group_tiles, occluder.group_window_count),
            (Some(0), Some(2))
        );
        assert!(occluder.scoped() && occluder.opacity_attested);
        assert_eq!(occluder.fallback, None);
        // A change in the window above T: both count it.
        let occluder = group
            .measure_occluder(&frame(1, None), &frame(1, Some((100, 10))), true)
            .unwrap();
        assert_eq!(occluder.group_tiles, Some(1));
        assert!(occluder.group_frame_fraction.unwrap() < 1.0);
        assert_eq!(occluder.group_covers_whole_frame, Some(false));
    }

    #[test]
    fn probe_names_why_the_amended_scope_would_compare_the_whole_frame() {
        let pair = |reader: &Reader, stable: bool| {
            read_stack(reader, 0)
                .measure_occluder(&frame(1, None), &frame(1, None), stable)
                .unwrap()
        };
        let none = pair(&Reader::new(stack()).at(1), true);
        assert_eq!(none.fallback, Some(OccluderFallback::NoOpacity));
        assert!(!none.opacity_attested && !none.scoped());
        assert_eq!(
            none.group_window_count,
            Some(2),
            "the group is still counted"
        );
        let corner = pair(&Reader::new(stack()).at(1).opaque(100), true);
        assert_eq!(corner.fallback, Some(OccluderFallback::Corner));
        assert!(corner.opacity_attested);
        let differ = pair(&Reader::new(stack()).at(1).opaque(0), false);
        assert_eq!(differ.fallback, Some(OccluderFallback::RepeatedReadsDiffer));
        let names: Vec<_> = [
            OccluderFallback::WalkFailed,
            OccluderFallback::UnattestedRect,
            OccluderFallback::RepeatedReadsDiffer,
            OccluderFallback::BoxOutsideTarget,
            OccluderFallback::NoOpacity,
            OccluderFallback::Corner,
        ]
        .map(OccluderFallback::as_str)
        .into();
        assert_eq!(
            names,
            [
                "walk_failed",
                "unattested_rect",
                "reads_differ",
                "box_outside_target",
                "no_opacity",
                "corner"
            ]
        );
    }

    /// The reasons the probe cannot reach through a read (it aborts those pairs first) still
    /// come from the engine's own decision.
    #[test]
    fn the_engines_attestation_names_the_remaining_reasons() {
        let reader = Reader::new(stack()).at(1).opaque(0);
        let read = WindowRead::read(&reader).unwrap();
        let window = reader.window_at((0.0, 0.0)).unwrap();
        let group = read.occluders(&window, 0).unwrap();
        assert_eq!(
            group.attest(&box_at(8.0, 8.0, 16.0, 16.0), &reader).0,
            Ok(())
        );
        assert_eq!(
            group.attest(&box_at(60.0, 60.0, 90.0, 70.0), &reader).0,
            Err(OccluderFallback::BoxOutsideTarget)
        );
        let missing = WindowGroup {
            target: 99,
            ..group.clone()
        };
        assert_eq!(
            missing.attest(&box_at(8.0, 8.0, 16.0, 16.0), &reader),
            (Err(OccluderFallback::UnattestedRect), false)
        );
    }

    /// A window that enters the amended group only through the margin changes it between two
    /// reads while the ADR-0032 groups, and so the old equality, stay equal.
    #[test]
    fn probe_sees_an_occluder_change_that_adr_0032_does_not() {
        let with = |x: f64| {
            vec![
                at_z(2, 0, box_at(x, 10.0, 120.0, 40.0)),
                at_z(1, 1, box_at(0.0, 0.0, 64.0, 64.0)),
            ]
        };
        let (a, b) = (
            read_stack(&Reader::new(with(66.0)).at(1).opaque(0), 5),
            read_stack(&Reader::new(with(70.0)).at(1).opaque(0), 5),
        );
        assert_eq!(a, b, "the ADR-0032 group and target are unchanged");
        assert!(!a.same_occluders(&b));
        assert!(a.same_occluders(&a.clone()));
    }

    /// PB2 (review): the selection margin is part of the measurement object, so counts cannot
    /// be taken at another margin than the group was selected with.
    #[test]
    fn probe_measures_at_the_margin_it_selected_with() {
        let windows = || {
            vec![
                at_z(2, 0, box_at(68.0, 0.0, 256.0, 32.0)),
                at_z(1, 1, box_at(0.0, 0.0, 64.0, 64.0)),
            ]
        };
        let changed = frame(1, Some((200, 0)));
        let read = |margin| read_stack(&Reader::new(windows()).at(1).opaque(0), margin);
        // Margin 5: the window 4 px away is in the group and its tile is compared.
        let five = read(5)
            .measure_occluder(&frame(1, None), &changed, true)
            .unwrap();
        assert_eq!(
            (five.group_window_count, five.group_tiles),
            (Some(2), Some(1))
        );
        assert!(five.scoped());
        // Margin 0 (also what `read` selects): the window is not in the group, consistently.
        let zero = read(0)
            .measure_occluder(&frame(1, None), &changed, true)
            .unwrap();
        assert_eq!(
            (zero.group_window_count, zero.group_tiles),
            (Some(1), Some(0))
        );
        let compat = FreshnessProbeGroup::read(
            &Reader::new(windows()).at(1).opaque(0),
            box_at(8.0, 8.0, 16.0, 16.0),
        )
        .unwrap();
        assert_eq!(
            compat.measure_occluder(&frame(1, None), &changed, true),
            Some(zero)
        );
        // Reads at different margins are different reads of the amended group; the legacy
        // ADR-0032 equality is untouched.
        assert!(!read(5).same_occluders(&read(0)));
        assert_eq!(read(5), read(0));
    }
}
