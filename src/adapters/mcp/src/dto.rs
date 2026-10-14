//! Wire representations. Domain types are never serialised directly to agents so the
//! public contract can evolve independently of the domain model.

use ommatix_application::PerceptionReport;
use ommatix_domain::{Element, Point, Rect};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ElementDto {
    pub id: String,
    pub kind: ommatix_domain::ElementKind,
    pub bounds: Rect,
    /// Suggested click target (geometric centre).
    pub center: Point,
    pub first_seen: u64,
    pub last_changed: u64,
}

impl From<&Element> for ElementDto {
    fn from(e: &Element) -> Self {
        Self {
            id: e.id.to_string(),
            kind: e.kind,
            bounds: e.bounds,
            center: e.bounds.center(),
            first_seen: e.first_seen,
            last_changed: e.last_changed,
        }
    }
}

pub fn elements(list: &[Element]) -> Vec<ElementDto> {
    list.iter().map(ElementDto::from).collect()
}

#[derive(Debug, Serialize)]
pub struct ReportDto {
    pub frame_no: u64,
    pub width: u32,
    pub height: u32,
    pub dirty_cells: usize,
    pub total_cells: usize,
    pub dirty_regions: Vec<Rect>,
    pub added: Vec<ElementDto>,
    pub updated: Vec<ElementDto>,
    pub removed: Vec<String>,
    pub element_count: usize,
    pub timings_us: ommatix_application::Timings,
}

impl From<&PerceptionReport> for ReportDto {
    fn from(r: &PerceptionReport) -> Self {
        Self {
            frame_no: r.frame_no,
            width: r.width,
            height: r.height,
            dirty_cells: r.dirty_cells,
            total_cells: r.total_cells,
            dirty_regions: r.dirty_regions.clone(),
            added: elements(&r.delta.added),
            updated: elements(&r.delta.updated),
            removed: r.delta.removed.iter().map(ToString::to_string).collect(),
            element_count: r.element_count,
            timings_us: r.timings,
        }
    }
}
