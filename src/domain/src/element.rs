use crate::Rect;

/// Stable identifier of a perceived element. Ids are never reused within a session and
/// survive across frames while the element keeps (approximately) the same bounds. How an
/// id is written on the wire is the driving adapter's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(pub u64);

/// What kind of visual element was detected. The MVP detector only emits `Blob`;
/// richer kinds are reserved for future detectors (OCR, widget classifiers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementKind {
    /// A region that stands out from its surroundings: no semantic class yet.
    Blob,
    Text,
    Button,
    Icon,
}

/// A raw detector output, before identity assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub bounds: Rect,
    pub kind: ElementKind,
}

/// An element living in the [`Scene`](crate::Scene).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    pub id: ElementId,
    pub bounds: Rect,
    pub kind: ElementKind,
    /// Frame number in which the element was first observed.
    pub first_seen: u64,
    /// Frame number in which the element's area was last re-analysed.
    pub last_changed: u64,
}
