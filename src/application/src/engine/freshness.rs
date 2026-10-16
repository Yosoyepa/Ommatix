//! Optional freshness scope (ADR-0032); all OS evidence comes through the actuator port.

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

/// The immutable OS evidence belonging to this observation, never emitted on the wire.
#[derive(Debug, Clone)]
pub(super) struct WindowSnapshot {
    read: WindowRead,
    targets: BTreeMap<TargetId, u64>,
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

    fn touches(&self, tile: &BBox, margin: u32, bounds: &BBox) -> bool {
        self.windows
            .iter()
            .filter_map(|w| w.rectangle)
            .filter_map(|r| r.pad(f64::from(margin)).clip(bounds))
            .any(|r| r.overlaps(tile))
    }
}

/// Opaque, read-only attestation for the operator's M1 measurement (ADR-0032).
/// It shares the engine's complete window read and one-hop group selection.
#[derive(Debug, Clone, PartialEq)]
pub struct FreshnessProbeGroup {
    group: WindowGroup,
    target: BBox,
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

impl FreshnessProbeGroup {
    /// Require a non-live reader and a complete, stable read containing the target box.
    /// The caller brackets each capture with equal attestations; no delivery is called.
    pub fn read(reader: &dyn OsActuator, target: BBox) -> Option<Self> {
        if reader.is_live() {
            return None;
        }
        let first = WindowRead::read(reader)?;
        let group = first.group(&reader.window_at(target.center())?)?;
        if !group.contains_target(&target) || WindowRead::read(reader)? != first {
            return None;
        }
        Some(Self { group, target })
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
        windows.read.group(window)
    }

    /// Preserve positive identity evidence separately from scope availability. A later
    /// failed read, or a target box that no longer fits, cannot erase a known change.
    fn current_group(&self, bbox: &BBox, expected: &WindowGroup) -> (Option<WindowGroup>, bool) {
        let read = WindowRead::read(self.ports.actuator.as_ref());
        let current = read
            .as_ref()
            .and_then(|r| r.group(&self.ports.actuator.window_at(bbox.center())?));
        let known_change = current.as_ref().is_some_and(|group| group != expected)
            || (current.is_none()
                && read.as_ref().is_some_and(|r| {
                    let observed_now = r
                        .windows
                        .iter()
                        .find(|w| w.handle == expected.target)
                        .and_then(|window| r.group(window));
                    observed_now.as_ref() != Some(expected)
                }));
        (current, known_change)
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
        let scope = expected.as_ref().filter(|_| captured_group.is_some());
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
            return Err(self.freshness_refusal(&captured.frame, &diff, bbox, scope));
        }
        Ok(FreshFrame {
            frame: captured.frame,
            diff,
            expected,
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
            return Err(self.freshness_refusal(&fresh.frame, &fresh.diff, bbox, Some(expected)));
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
                return Err(self.freshness_refusal(&fresh.frame, &full, bbox, None));
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
    ) -> ToolError {
        self.snapshot_spent = true;
        // Only a refusal pays for the global diagnostic scan in target-window mode.
        let full = group
            .and_then(|_| dirty_tiles(self.store.base(), frame, self.config.plan.tile_size).ok());
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
    use crate::{DeliveryReceipt, PortError};

    struct Reader {
        windows: Vec<TopLevelWindow>,
        live: bool,
    }
    impl OsActuator for Reader {
        fn name(&self) -> &'static str {
            "synthetic-reader"
        }
        fn is_live(&self) -> bool {
            self.live
        }
        fn foreground_window(&self) -> Option<TopLevelWindow> {
            self.windows.first().cloned()
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
        FreshnessProbeGroup::read(
            &Reader {
                windows,
                live: false,
            },
            box_at(8.0, 8.0, 16.0, 16.0),
        )
        .unwrap()
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
        let mut reader = Reader {
            windows: vec![window(1, box_at(0.0, 0.0, 64.0, 64.0))],
            live: true,
        };
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
}
