use serde::{Deserialize, Serialize};

use crate::Rect;

/// Stable identifier of a perceived element. Ids are never reused within a session and
/// survive across frames while the element keeps (approximately) the same bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub u64);

impl std::fmt::Display for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "e{}", self.0)
    }
}

/// What kind of visual element was detected. The MVP detector only emits `Blob`;
/// richer kinds are reserved for future detectors (OCR, widget classifiers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// A connected group of foreground pixels: no semantic class yet.
    Blob,
    Text,
    Button,
    Icon,
}

/// A raw detector output, before identity assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    pub bounds: Rect,
    pub kind: ElementKind,
}

/// An element living in the [`Scene`](crate::Scene).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    pub bounds: Rect,
    pub kind: ElementKind,
    /// Frame number in which the element was first observed.
    pub first_seen: u64,
    /// Frame number in which the element's pixels last changed.
    pub last_changed: u64,
}
