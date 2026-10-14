use thiserror::Error;

/// Errors raised by domain invariants.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("frame dimensions must be non-zero (got {width}x{height})")]
    EmptyFrame { width: u32, height: u32 },
    #[error("pixel buffer has {actual} bytes, expected {expected} (RGBA8 {width}x{height})")]
    BufferSize {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("grid cell size must be non-zero")]
    ZeroCellSize,
}
