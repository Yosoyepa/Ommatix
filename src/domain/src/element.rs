use crate::Rect;

/// What kind of visual element the prototype's flat-region detector found. It only emits
/// `Blob`; the thesis pipeline uses `fusion::Role` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementKind {
    /// A region that stands out from its surroundings: no semantic class yet.
    Blob,
    Text,
    Button,
    Icon,
}

/// A raw output of the flat-region detector, in RGBA frame pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub bounds: Rect,
    pub kind: ElementKind,
}
