use crate::{DomainError, Rect};

/// One captured framebuffer, stored as tightly packed RGBA8 rows.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Frame {
    pub const BYTES_PER_PIXEL: usize = 4;

    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, DomainError> {
        let expected = Self::buffer_len(width, height)?;
        if pixels.len() != expected {
            return Err(DomainError::BufferSize {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// A frame filled with a single colour.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Result<Self, DomainError> {
        let len = Self::buffer_len(width, height)?;
        Self::from_rgba(width, height, rgba.repeat(len / Self::BYTES_PER_PIXEL))
    }

    /// Bytes of a `width x height` RGBA8 buffer.
    fn buffer_len(width: u32, height: u32) -> Result<usize, DomainError> {
        if width == 0 || height == 0 {
            return Err(DomainError::EmptyFrame { width, height });
        }
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(Self::BYTES_PER_PIXEL))
            .ok_or(DomainError::FrameTooLarge { width, height })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// RGBA of the pixel at `(x, y)`. Panics if out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.offset(x, y);
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// The bytes of row `y` restricted to columns `[x, x + w)`. Panics if the span leaves
    /// the row (it would otherwise silently read into the next one).
    pub fn row_slice(&self, x: u32, y: u32, w: u32) -> &[u8] {
        assert!(
            u64::from(x) + u64::from(w) <= u64::from(self.width),
            "row span [{x}, {x}+{w}) exceeds frame width {}",
            self.width
        );
        let start = self.offset(x.min(self.width.saturating_sub(1)), y);
        &self.pixels[start..start + w as usize * Self::BYTES_PER_PIXEL]
    }

    /// Paints `rect` (clipped to the frame) with a solid colour.
    pub fn fill_rect(&mut self, rect: Rect, rgba: [u8; 4]) {
        let r = rect.clip(&self.bounds());
        if r.is_empty() {
            return;
        }
        for y in r.y..r.bottom() {
            let start = self.offset(r.x, y);
            let row = &mut self.pixels[start..start + r.w as usize * Self::BYTES_PER_PIXEL];
            for px in row.as_chunks_mut::<4>().0 {
                *px = rgba;
            }
        }
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x},{y}) out of bounds"
        );
        (y as usize * self.width as usize + x as usize) * Self::BYTES_PER_PIXEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_buffers() {
        assert!(matches!(
            Frame::from_rgba(0, 4, vec![]),
            Err(DomainError::EmptyFrame { .. })
        ));
        assert!(matches!(
            Frame::from_rgba(2, 2, vec![0; 15]),
            Err(DomainError::BufferSize { expected: 16, .. })
        ));
    }

    /// Verification #18a: sizes whose buffer length overflows are errors, not panics.
    #[test]
    fn rejects_sizes_that_overflow() {
        let huge = 1 << 31;
        let too_large = Err(DomainError::FrameTooLarge {
            width: huge,
            height: huge,
        });
        assert_eq!(Frame::from_rgba(huge, huge, vec![]), too_large);
        assert_eq!(Frame::solid(huge, huge, [0; 4]), too_large);
    }

    #[test]
    fn fill_rect_is_clipped() {
        let mut f = Frame::solid(4, 4, [0, 0, 0, 255]).unwrap();
        f.fill_rect(Rect::new(2, 2, 10, 10), [255, 0, 0, 255]);
        assert_eq!(f.pixel(3, 3), [255, 0, 0, 255]);
        assert_eq!(f.pixel(1, 1), [0, 0, 0, 255]);
        f.fill_rect(Rect::new(10, 10, 5, 5), [1, 1, 1, 1]);
    }

    #[test]
    #[should_panic(expected = "exceeds frame width")]
    fn row_slice_rejects_spans_past_the_row() {
        let f = Frame::solid(4, 4, [0; 4]).unwrap();
        let _ = f.row_slice(2, 0, 3);
    }
}
