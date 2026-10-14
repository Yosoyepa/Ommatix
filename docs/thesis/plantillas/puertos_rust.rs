//! Plantilla de interfaces, no un runtime ni una implementación compilada aquí.
//! Rust sigue siendo la decisión de producto; los oráculos Python son independientes.
use std::sync::Arc;

pub struct Rect { pub x0: f32, pub y0: f32, pub x1: f32, pub y1: f32 }
pub struct Frame {
    pub pixels: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub row_stride: usize,
    pub geometry_epoch: u64,
    pub captured_at_ns: u64,
}
pub struct RawEvidence {
    pub bbox: Rect,
    pub text: Option<String>,
    pub role: Option<String>,
    pub source_frame: u64,
    pub provider_version: String,
}
pub struct SnapshotRef {
    pub session_id: String,
    pub snapshot_id: String,
    pub target_id: String,
    pub expected_revision: u64,
    pub geometry_epoch: u64,
}
pub enum EngineError {
    InvalidInput(String), CaptureUnavailable, InferenceFailed,
    StaleSnapshot, StaleTarget, PolicyDenied, Indeterminate,
}
pub trait FrameCapturer {
    fn capture(&mut self) -> Result<Frame, EngineError>;
}
pub trait VisionDetector {
    fn detect(&mut self, frame: &Frame, regions: &[Rect])
        -> Result<Vec<RawEvidence>, EngineError>;
}
pub trait TextRecognizer {
    fn recognize(&mut self, frame: &Frame, regions: &[Rect])
        -> Result<Vec<RawEvidence>, EngineError>;
}
pub trait Clock { fn now_monotonic_ns(&self) -> u64; }
// ValidatedAction debe ser construible sólo por el gate autorizado en el runtime.
pub struct ValidatedAction { _private: () }
pub struct DeliveryReceipt { pub submitted: u32, pub inserted: u32 }
pub trait OsActuator {
    fn deliver(&mut self, action: &ValidatedAction)
        -> Result<DeliveryReceipt, EngineError>;
}
