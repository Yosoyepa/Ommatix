use crate::{DomainError, Frame, Rect};

/// Column/row of a grid cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
            cell.col.saturating_mul(self.cell_size),
            cell.row.saturating_mul(self.cell_size),
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
                let mut h = WordHasher::new();
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

    /// `true` when both signatures describe frames of the same size, so their cells can be
    /// compared one to one. This is the single definition of "the geometry changed".
    pub fn same_geometry(&self, other: &GridSignature) -> bool {
        self.width == other.width && self.height == other.height
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
            Some(prev) if self.same_geometry(prev) => self
                .hashes
                .iter()
                .zip(&prev.hashes)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| CellCoord {
                    col: i as u32 % self.cols,
                    row: i as u32 / self.cols,
                })
                .collect(),
            _ => self.all_cells(),
        }
    }
}

/// Hash of the colour of RGBA pixel rows, two pixels (one 64-bit word) at a time
/// (ADR-0011 §6). Deterministic across runs and platforms (unlike `RandomState`) and
/// dependency-free.
///
/// - Each step `h = (h ^ word) * K; h ^= h >> 32` is a bijection of `h` for a fixed word,
///   so a change confined to two neighbouring pixels always changes the final hash.
/// - The multiply carries low bits up and the shift carries high bits down, so changes
///   spread over several words collide only by chance (none in 2²⁰ random two-word
///   changes; a plain multiply-xor missed one in 2¹⁶).
/// - Alpha is not hashed: a screen is opaque and the detector never reads it.
struct WordHasher(u64);

impl WordHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    /// Odd (so multiplying is a bijection), with bits set throughout: 2⁶⁴ / φ.
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    /// The colour bytes of the two pixels in a little-endian word.
    const COLOUR: u64 = 0x00ff_ffff_00ff_ffff;

    const fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// `bytes` must start at a pixel.
    fn write(&mut self, bytes: &[u8]) {
        let (words, tail) = bytes.as_chunks::<8>();
        for w in words {
            self.mix(u64::from_le_bytes(*w) & Self::COLOUR);
        }
        if !tail.is_empty() {
            let mut last = [0u8; 8];
            last[..tail.len()].copy_from_slice(tail);
            // Tag the tail with its length so a short row differs from a zero-padded one.
            self.mix((u64::from_le_bytes(last) & Self::COLOUR) ^ ((tail.len() as u64) << 59));
        }
    }

    fn mix(&mut self, word: u64) {
        let h = (self.0 ^ word).wrapping_mul(Self::K);
        self.0 = h ^ (h >> 32);
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
    fn every_single_pixel_change_marks_only_its_cell() {
        let g = GridSpec::new(8).unwrap();
        let a = Frame::solid(24, 24, WHITE).unwrap();
        let base = g.signature(&a);
        for y in 0..24 {
            for x in 0..24 {
                let mut b = a.clone();
                b.fill_rect(Rect::new(x, y, 1, 1), [254, 255, 255, 255]);
                assert_eq!(
                    g.signature(&b).diff(Some(&base)),
                    vec![CellCoord {
                        col: x / 8,
                        row: y / 8
                    }],
                    "pixel ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn odd_cell_widths_hash_their_tail() {
        // 3-pixel cells: 12 bytes per row, i.e. one word plus a 4-byte tail.
        let g = GridSpec::new(3).unwrap();
        let a = Frame::solid(3, 3, WHITE).unwrap();
        let mut b = a.clone();
        b.fill_rect(Rect::new(2, 1, 1, 1), [0, 0, 0, 255]);
        assert_eq!(g.signature(&b).diff(Some(&g.signature(&a))).len(), 1);
    }

    fn hash(words: &[u64]) -> u64 {
        let mut h = WordHasher::new();
        for w in words {
            h.write(&w.to_le_bytes());
        }
        h.finish()
    }

    /// Verification N10: flipping the top (alpha) bit of two words used to cancel out.
    /// Alpha is now left out on purpose; every colour bit counts.
    #[test]
    fn alpha_is_not_part_of_the_picture() {
        let g = GridSpec::new(4).unwrap();
        let a = Frame::solid(8, 4, WHITE).unwrap();
        let mut translucent = a.clone();
        translucent.fill_rect(Rect::new(0, 0, 8, 4), [255, 255, 255, 0]);
        assert_eq!(g.signature(&translucent).diff(Some(&g.signature(&a))), []);

        let colour_bits = (0..64).filter(|b| (WordHasher::COLOUR >> b) & 1 == 1);
        for bit in colour_bits {
            let flip = 1u64 << bit;
            assert_ne!(hash(&[7, 9]), hash(&[7 ^ flip, 9]), "bit {bit}");
            assert_ne!(hash(&[7, 9]), hash(&[7 ^ flip, 9 ^ flip]), "bit {bit}");
        }
    }

    /// Verification N10: changes to the high bytes of two words (the blue of every second
    /// pixel) used to collide about once in 2¹⁶.
    #[test]
    fn changes_spread_over_two_words_do_not_collide() {
        let mut rng = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut missed = 0;
        for _ in 0..1 << 20 {
            let (a, b, r) = (next(), next(), next());
            let blue = |shift: u32| ((r >> shift) % 255 + 1) << 48;
            if hash(&[a, b]) == hash(&[a ^ blue(0), b ^ blue(8)]) {
                missed += 1;
            }
        }
        assert_eq!(missed, 0);
    }

    /// Hashing cost per frame: `cargo test -p ommatix-domain --release -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore = "benchmark"]
    fn bench_signature() {
        let g = GridSpec::new(32).unwrap();
        for (w, h) in [(1920, 1080), (3840, 2160)] {
            let mut f = Frame::solid(w, h, WHITE).unwrap();
            for i in 0..w / 8 {
                f.fill_rect(
                    Rect::new(i * 8, (i * 7) % h, 3, 5),
                    [0, 0, (i % 256) as u8, 255],
                );
            }
            let start = std::time::Instant::now();
            let runs = 20;
            for _ in 0..runs {
                std::hint::black_box(g.signature(std::hint::black_box(&f)));
            }
            eprintln!("{w}x{h}: signature in {:?}", start.elapsed() / runs);
        }
    }

    #[test]
    fn first_frame_or_resize_marks_everything() {
        let g = GridSpec::new(8).unwrap();
        let a = g.signature(&Frame::solid(16, 16, WHITE).unwrap());
        let b = g.signature(&Frame::solid(24, 16, WHITE).unwrap());
        assert_eq!(a.diff(None).len(), 4);
        assert_eq!(b.diff(Some(&a)).len(), 6);
    }

    #[test]
    fn same_grid_but_different_size_is_a_geometry_change() {
        // 640 and 630 pixels both need 20 columns of 32 px.
        let g = GridSpec::new(32).unwrap();
        let a = g.signature(&Frame::solid(640, 64, WHITE).unwrap());
        let b = g.signature(&Frame::solid(630, 64, WHITE).unwrap());
        assert_eq!((a.cols(), a.rows()), (b.cols(), b.rows()));
        assert!(!a.same_geometry(&b));
        assert_eq!(b.diff(Some(&a)).len(), b.cell_count());
    }
}
