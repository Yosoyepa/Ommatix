use serde::{Deserialize, Serialize};

use crate::{DomainError, Frame, Rect};

/// Column/row of a grid cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CellCoord {
    pub col: u32,
    pub row: u32,
}

/// How the framebuffer is partitioned into independent cells ("ommatidia").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec {
    cell_size: u32,
}

impl GridSpec {
    pub fn new(cell_size: u32) -> Result<Self, DomainError> {
        if cell_size == 0 {
            return Err(DomainError::ZeroCellSize);
        }
        Ok(Self { cell_size })
    }

    pub const fn cell_size(&self) -> u32 {
        self.cell_size
    }

    /// Number of columns and rows needed to cover a `width x height` frame.
    pub const fn dimensions(&self, width: u32, height: u32) -> (u32, u32) {
        (
            width.div_ceil(self.cell_size),
            height.div_ceil(self.cell_size),
        )
    }

    /// Pixel rectangle covered by `cell`, clipped to the frame.
    pub fn cell_rect(&self, cell: CellCoord, frame_bounds: &Rect) -> Rect {
        Rect::new(
            cell.col * self.cell_size,
            cell.row * self.cell_size,
            self.cell_size,
            self.cell_size,
        )
        .clip(frame_bounds)
    }

    /// Fingerprints every cell of `frame`.
    pub fn signature(&self, frame: &Frame) -> GridSignature {
        let bounds = frame.bounds();
        let (cols, rows) = self.dimensions(frame.width(), frame.height());
        let mut hashes = Vec::with_capacity(cols as usize * rows as usize);
        for row in 0..rows {
            for col in 0..cols {
                let r = self.cell_rect(CellCoord { col, row }, &bounds);
                let mut h = Fnv1a::new();
                for y in r.y..r.bottom() {
                    h.write(frame.row_slice(r.x, y, r.w));
                }
                hashes.push(h.finish());
            }
        }
        GridSignature {
            width: frame.width(),
            height: frame.height(),
            cols,
            rows,
            hashes,
        }
    }
}

/// Per-cell fingerprints of one frame. Two signatures from frames of the same size can be
/// diffed to find exactly the cells whose pixels changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridSignature {
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    hashes: Vec<u64>,
}

impl GridSignature {
    pub const fn cols(&self) -> u32 {
        self.cols
    }

    pub const fn rows(&self) -> u32 {
        self.rows
    }

    pub fn cell_count(&self) -> usize {
        self.hashes.len()
    }

    /// Every cell in row-major order.
    pub fn all_cells(&self) -> Vec<CellCoord> {
        (0..self.rows)
            .flat_map(|row| (0..self.cols).map(move |col| CellCoord { col, row }))
            .collect()
    }

    /// Cells that differ from `previous`, in row-major order. If there is no previous
    /// signature or the geometry changed (resolution switch), every cell is dirty.
    pub fn diff(&self, previous: Option<&GridSignature>) -> Vec<CellCoord> {
        match previous {
            Some(prev)
                if prev.width == self.width
                    && prev.height == self.height
                    && prev.cols == self.cols =>
            {
                self.hashes
                    .iter()
                    .zip(&prev.hashes)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, _)| CellCoord {
                        col: i as u32 % self.cols,
                        row: i as u32 / self.cols,
                    })
                    .collect()
            }
            _ => self.all_cells(),
        }
    }
}

/// 64-bit FNV-1a. Chosen for determinism across platforms and runs (unlike `RandomState`)
/// and zero dependencies. Collisions only cause a missed update for one tick in a cell
/// whose content changed to a colliding value, an accepted MVP risk (see ADR-0005).
struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    const fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn dimensions_round_up() {
        let g = GridSpec::new(16).unwrap();
        assert_eq!(g.dimensions(100, 32), (7, 2));
        assert_eq!(
            g.cell_rect(CellCoord { col: 6, row: 0 }, &Rect::new(0, 0, 100, 32)),
            Rect::new(96, 0, 4, 16)
        );
    }

    #[test]
    fn identical_frames_have_no_dirty_cells() {
        let g = GridSpec::new(8).unwrap();
        let f = Frame::solid(32, 32, WHITE).unwrap();
        assert_eq!(g.signature(&f).diff(Some(&g.signature(&f))).len(), 0);
    }

    #[test]
    fn single_pixel_change_marks_only_its_cell() {
        let g = GridSpec::new(8).unwrap();
        let a = Frame::solid(32, 32, WHITE).unwrap();
        let mut b = a.clone();
        b.fill_rect(Rect::new(17, 9, 1, 1), [0, 0, 0, 255]);
        assert_eq!(
            g.signature(&b).diff(Some(&g.signature(&a))),
            vec![CellCoord { col: 2, row: 1 }]
        );
    }

    #[test]
    fn first_frame_or_resize_marks_everything() {
        let g = GridSpec::new(8).unwrap();
        let a = g.signature(&Frame::solid(16, 16, WHITE).unwrap());
        let b = g.signature(&Frame::solid(24, 16, WHITE).unwrap());
        assert_eq!(a.diff(None).len(), 4);
        assert_eq!(b.diff(Some(&a)).len(), 6);
    }
}
