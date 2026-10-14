//! Staging and atomic commit of observations (thesis chapter 04 §4.2, ADR-0006).
//!
//! The next state is built aside from the published one: [`SnapshotStore::stage`] never
//! touches the store. [`SnapshotStore::commit`] publishes the payload and moves the committed
//! base in one step, and only if nothing was committed since the stage was built. A failed
//! observation publishes nothing, keeps its dirty tiles pending and blocks actions until a
//! consistent observation is committed.
//!
//! Reference: `SnapshotStore` and `Pending` in the Python oracle (OR-18..OR-20).

use std::collections::BTreeSet;

use thiserror::Error;

use crate::incremental::{BgraFrame, TileCoord};

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum CommitError {
    /// Another observation was committed after this one was staged.
    #[error("stale commit: staged on revision {staged_on}, store is at revision {current}")]
    Stale { staged_on: u64, current: u64 },
}

/// Whether the published snapshot may be acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreState {
    /// Nothing has been committed yet.
    Empty,
    /// The last observation was committed.
    Ready,
    /// The last observation failed: the published snapshot is informational only.
    Degraded,
}

/// A candidate observation built on top of revision `base_revision`.
#[derive(Debug, Clone, PartialEq)]
pub struct Staged<P> {
    base_revision: u64,
    frame: BgraFrame,
    payload: P,
}

impl<P> Staged<P> {
    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }

    pub const fn frame(&self) -> &BgraFrame {
        &self.frame
    }

    pub const fn payload(&self) -> &P {
        &self.payload
    }
}

/// The published snapshot, its committed base frame and the work still pending.
#[derive(Debug, Clone)]
pub struct SnapshotStore<P> {
    revision: u64,
    base: Option<BgraFrame>,
    published: Option<P>,
    pending_dirty: BTreeSet<TileCoord>,
    degraded: bool,
}

impl<P> Default for SnapshotStore<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> SnapshotStore<P> {
    pub const fn new() -> Self {
        Self {
            revision: 0,
            base: None,
            published: None,
            pending_dirty: BTreeSet::new(),
            degraded: false,
        }
    }

    /// Number of commits so far.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// The committed base that the next frame is compared against.
    pub const fn base(&self) -> Option<&BgraFrame> {
        self.base.as_ref()
    }

    pub const fn published(&self) -> Option<&P> {
        self.published.as_ref()
    }

    /// Tiles that changed but were not processed successfully yet.
    pub const fn pending_dirty(&self) -> &BTreeSet<TileCoord> {
        &self.pending_dirty
    }

    pub const fn state(&self) -> StoreState {
        match (&self.published, self.degraded) {
            (None, _) => StoreState::Empty,
            (Some(_), false) => StoreState::Ready,
            (Some(_), true) => StoreState::Degraded,
        }
    }

    /// Actions need a snapshot that reflects the last observation.
    pub const fn actions_allowed(&self) -> bool {
        matches!(self.state(), StoreState::Ready)
    }

    /// Builds a candidate on the current revision without changing the store.
    pub const fn stage(&self, frame: BgraFrame, payload: P) -> Staged<P> {
        Staged {
            base_revision: self.revision,
            frame,
            payload,
        }
    }

    /// Publishes `staged` and makes its frame the committed base, or rejects it unchanged if
    /// another commit happened since it was staged. The staged work must have covered the
    /// pending tiles, which a commit clears.
    pub fn commit(&mut self, staged: Staged<P>) -> Result<u64, CommitError> {
        if staged.base_revision != self.revision {
            return Err(CommitError::Stale {
                staged_on: staged.base_revision,
                current: self.revision,
            });
        }
        self.base = Some(staged.frame);
        self.published = Some(staged.payload);
        self.pending_dirty.clear();
        self.degraded = false;
        self.revision += 1;
        Ok(self.revision)
    }

    /// Records a failed observation: nothing is published, the base stays, the planned dirty
    /// tiles stay pending and actions are blocked.
    pub fn record_failure(&mut self, dirty: &BTreeSet<TileCoord>) {
        self.pending_dirty.extend(dirty.iter().copied());
        self.degraded = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental::{DEFAULT_TILE_SIZE, dirty_tiles};

    fn blank() -> BgraFrame {
        BgraFrame::new(65, 65, 260, 1, vec![0u8; 260 * 65]).unwrap()
    }

    fn changed(frame: &BgraFrame, x: usize, y: usize) -> BgraFrame {
        let mut buf = frame.pixels().to_vec();
        buf[y * frame.row_stride() + 4 * x] = 255;
        BgraFrame::new(65, 65, 260, 1, buf).unwrap()
    }

    /// UT-13 (OR-18): staging builds a candidate without touching the published state.
    #[test]
    fn ut_13_staging_does_not_mutate_the_published_state() {
        let mut store = SnapshotStore::new();
        let staged = store.stage(blank(), vec!["e1"]);
        assert!(store.base().is_none());
        assert_eq!(store.revision(), 0);
        assert_eq!(store.state(), StoreState::Empty);

        assert_eq!(store.commit(staged), Ok(1));
        assert_eq!(store.published(), Some(&vec!["e1"]));
        assert_eq!(store.base(), Some(&blank()));
    }

    /// UT-14 (OR-19): a failed observation keeps the base and its change pending, and blocks
    /// actions.
    #[test]
    fn ut_14_failed_observation_keeps_base_and_pending_change() {
        let mut store = SnapshotStore::new();
        let f = blank();
        store.commit(store.stage(f.clone(), vec!["e1"])).unwrap();
        assert!(store.actions_allowed());

        let g = changed(&f, 0, 1);
        let dirty = dirty_tiles(store.base(), &g, DEFAULT_TILE_SIZE)
            .unwrap()
            .tiles;
        let _aborted = store.stage(g.clone(), vec!["e2"]); // inference failed: never committed
        store.record_failure(&dirty);

        assert_eq!(store.published(), Some(&vec!["e1"]));
        assert_eq!(store.base(), Some(&f));
        assert_eq!(store.pending_dirty(), &dirty);
        assert_eq!(
            dirty_tiles(store.base(), &g, DEFAULT_TILE_SIZE)
                .unwrap()
                .tiles,
            BTreeSet::from([TileCoord::new(0, 0)])
        );
        assert_eq!(store.state(), StoreState::Degraded);
        assert!(!store.actions_allowed());

        store.commit(store.stage(g, vec!["e2"])).unwrap();
        assert!(store.pending_dirty().is_empty());
        assert!(store.actions_allowed());
    }

    /// UT-15 (OR-20): a candidate staged on an older revision is rejected.
    #[test]
    fn ut_15_stale_commit_is_rejected() {
        let mut store = SnapshotStore::new();
        let a = store.stage(blank(), vec!["a"]);
        let b = store.stage(blank(), vec!["b"]);
        store.commit(a).unwrap();
        assert_eq!(
            store.commit(b),
            Err(CommitError::Stale {
                staged_on: 0,
                current: 1
            })
        );
        assert_eq!(store.published(), Some(&vec!["a"]));
        assert_eq!(store.revision(), 1);
    }
}
