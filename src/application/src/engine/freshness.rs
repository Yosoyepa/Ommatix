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

pub(super) struct FreshFrame {
    pub frame: BgraFrame,
    diff: TileDiff,
    expected: Option<WindowGroup>,
    captured_group: Option<WindowGroup>,
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

    fn current_group(&self, bbox: &BBox) -> Option<WindowGroup> {
        let read = WindowRead::read(self.ports.actuator.as_ref())?;
        read.group(&self.ports.actuator.window_at(bbox.center())?)
    }

    /// Compare only intersecting tiles when the window group is attested. D0 counts
    /// the whole frame on refusal; geometry always stays global.
    pub(super) fn fresh_frame(
        &mut self,
        target: TargetId,
        bbox: &BBox,
    ) -> Result<FreshFrame, ToolError> {
        let expected = self.observed_group(target);
        let first = expected
            .as_ref()
            .and_then(|_| self.current_group(bbox))
            .filter(|g| g.contains_target(bbox));
        let captured = self.ports.capturer.capture().map_err(|_| {
            ToolError::new(
                ErrorCode::CaptureUnavailable,
                "the screen could not be captured to confirm the snapshot; nothing was delivered",
            )
        })?;
        let second = expected
            .as_ref()
            .and_then(|_| self.current_group(bbox))
            .filter(|g| g.contains_target(bbox));
        // Different reads around capture or missing attestation require the whole frame.
        let captured_group = first.filter(|g| Some(g) == second.as_ref());
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
            captured_group,
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
        let read = WindowRead::read(self.ports.actuator.as_ref());
        let current = read
            .as_ref()
            .and_then(|r| r.group(&self.ports.actuator.window_at(bbox.center())?));
        // Identity is independent of whether the old box still fits or a point read
        // succeeds. Complete evidence of a moved/disappeared seed must still refuse.
        let known_change = current.is_none()
            && read.as_ref().is_some_and(|r| {
                let observed_now = r
                    .windows
                    .iter()
                    .find(|w| w.handle == expected.target)
                    .and_then(|window| r.group(window));
                observed_now.as_ref() != Some(expected)
            });
        // The new OS walk can take time. Re-read consent, stop, application/window scope
        // and age after it, before comparing identity or handing anything to the actuator.
        self.last_look(foreground, bbox.center(), target)?;
        let identity_changed = current.as_ref().is_some_and(|g| {
            g != expected
                || fresh
                    .captured_group
                    .as_ref()
                    .is_some_and(|captured| captured != g)
        });
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
        if known_change || identity_changed {
            return Err(self.freshness_refusal(&fresh.frame, &fresh.diff, bbox, Some(expected)));
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
