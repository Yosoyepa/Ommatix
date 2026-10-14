use crate::{Frame, GridSignature, GridSpec, Rect, merge_dirty_cells};

/// What changed between the last committed frame and this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameChange {
    pub total_cells: usize,
    pub dirty_cells: usize,
    /// Dirty cells merged into pixel rectangles (4-connected groups), sorted.
    pub regions: Vec<Rect>,
    /// First frame, or the frame size differs from the previous one. Everything known
    /// about the previous screen is stale.
    pub geometry_changed: bool,
}

/// A comparison that has not been committed yet (see [`ChangeTracker::compare`]).
#[derive(Debug, Clone)]
pub struct PendingChange {
    change: FrameChange,
    signature: GridSignature,
}

impl PendingChange {
    pub fn change(&self) -> &FrameChange {
        &self.change
    }
}

/// Owns the cell grid and the last committed frame's signature, and is the single place
/// that decides what "changed" means (bootstrap ADR B0005).
#[derive(Debug, Clone)]
pub struct ChangeTracker {
    grid: GridSpec,
    previous: Option<GridSignature>,
}

impl ChangeTracker {
    pub fn new(grid: GridSpec) -> Self {
        Self {
            grid,
            previous: None,
        }
    }

    pub fn grid(&self) -> &GridSpec {
        &self.grid
    }

    /// Fingerprints `frame` and compares it with the last committed frame.
    ///
    /// Nothing is remembered until the result is passed to [`ChangeTracker::commit`]: if
    /// processing the change fails, the same change is reported again on the next frame
    /// instead of being lost.
    pub fn compare(&self, frame: &Frame) -> PendingChange {
        let signature = self.grid.signature(frame);
        let geometry_changed = self
            .previous
            .as_ref()
            .is_none_or(|p| !p.same_geometry(&signature));
        let dirty = signature.diff(self.previous.as_ref());
        let regions = merge_dirty_cells(&dirty, &self.grid, &frame.bounds());
        PendingChange {
            change: FrameChange {
                total_cells: signature.cell_count(),
                dirty_cells: dirty.len(),
                regions,
                geometry_changed,
            },
            signature,
        }
    }

    /// Makes the compared frame the reference for the next comparison.
    pub fn commit(&mut self, pending: PendingChange) -> FrameChange {
        self.previous = Some(pending.signature);
        pending.change
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255; 4];

    fn observe(t: &mut ChangeTracker, frame: &Frame) -> FrameChange {
        let pending = t.compare(frame);
        t.commit(pending)
    }

    #[test]
    fn reports_first_frame_static_frames_and_resizes() {
        let mut t = ChangeTracker::new(GridSpec::new(16).unwrap());
        let a = Frame::solid(64, 32, WHITE).unwrap();

        let first = observe(&mut t, &a);
        assert!(first.geometry_changed);
        assert_eq!(first.dirty_cells, 8);
        assert_eq!(first.regions, vec![Rect::new(0, 0, 64, 32)]);

        let same = observe(&mut t, &a);
        assert!(!same.geometry_changed);
        assert_eq!(same.dirty_cells, 0);
        assert_eq!(same.regions.len(), 0);

        let mut b = a.clone();
        b.fill_rect(Rect::new(20, 4, 2, 2), [0, 0, 0, 255]);
        let changed = observe(&mut t, &b);
        assert_eq!(changed.regions, vec![Rect::new(16, 0, 16, 16)]);

        let resized = observe(&mut t, &Frame::solid(60, 32, WHITE).unwrap());
        assert!(resized.geometry_changed, "same grid, different size");
        assert_eq!(resized.dirty_cells, resized.total_cells);
    }

    #[test]
    fn uncommitted_changes_are_reported_again() {
        let mut t = ChangeTracker::new(GridSpec::new(16).unwrap());
        let a = Frame::solid(32, 32, WHITE).unwrap();
        observe(&mut t, &a);
        let mut b = a.clone();
        b.fill_rect(Rect::new(0, 0, 1, 1), [0, 0, 0, 255]);

        let dropped = t.compare(&b);
        assert_eq!(dropped.change().dirty_cells, 1);
        drop(dropped); // e.g. detection failed

        assert_eq!(observe(&mut t, &b).dirty_cells, 1, "still pending");
        assert_eq!(observe(&mut t, &b).dirty_cells, 0);
    }
}
