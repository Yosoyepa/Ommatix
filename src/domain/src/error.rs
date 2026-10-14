use thiserror::Error;

/// Errors raised by domain invariants.
#[derive(Debug, Error, Clone, PartialEq)]
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
    #[error("an RGBA8 frame of {width}x{height} pixels does not fit in memory")]
    FrameTooLarge { width: u32, height: u32 },
    #[error("grid cell size must be non-zero")]
    ZeroCellSize,
    #[error("spatial index bucket size must be non-zero")]
    ZeroBucketSize,
    #[error("identity IoU threshold must be in (0, 1], got {0}")]
    InvalidIdentityIou(f32),
    #[error("display mapping sizes must be non-zero")]
    EmptyDisplay,
}
