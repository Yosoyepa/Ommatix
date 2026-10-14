//! Wire representations. Domain and application types are never serialised directly, so
//! the public contract with agents can evolve independently of the model
//! (bootstrap ADR B0004).

use std::time::Duration;

use ommatix_application::{ActionReceipt, Observation, PerceptionReport, Timings};
use ommatix_domain::{
    Action, Element, ElementId, ElementKind, Key, MouseButton, Point, Rect, SceneDelta,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
pub struct RectDto {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl From<Rect> for RectDto {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PointDto {
    pub x: u32,
    pub y: u32,
}

impl From<Point> for PointDto {
    fn from(p: Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

#[derive(Debug, Serialize)]
pub struct ElementDto {
    pub id: String,
    pub kind: &'static str,
    pub bounds: RectDto,
    /// Suggested click target (geometric centre).
    pub center: PointDto,
    pub first_seen: u64,
    pub last_changed: u64,
}

impl From<&Element> for ElementDto {
    fn from(e: &Element) -> Self {
        Self {
            id: format_element_id(e.id),
            kind: kind_name(e.kind),
            bounds: e.bounds.into(),
            center: e.bounds.center().into(),
            first_seen: e.first_seen,
            last_changed: e.last_changed,
        }
    }
}

pub fn elements(list: &[Element]) -> Vec<ElementDto> {
    list.iter().map(ElementDto::from).collect()
}

#[derive(Debug, Serialize)]
pub struct DeltaDto {
    pub added: Vec<ElementDto>,
    pub updated: Vec<ElementDto>,
    pub removed: Vec<String>,
}

impl From<&SceneDelta> for DeltaDto {
    fn from(d: &SceneDelta) -> Self {
        Self {
            added: elements(&d.added),
            updated: elements(&d.updated),
            removed: d.removed.iter().copied().map(format_element_id).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReportDto {
    pub frame_no: u64,
    pub width: u32,
    pub height: u32,
    pub total_cells: usize,
    pub dirty_cells: usize,
    pub geometry_changed: bool,
    pub analysed_regions: Vec<RectDto>,
    pub analysed_px: u64,
    #[serde(flatten)]
    pub delta: DeltaDto,
    pub element_count: usize,
}

impl From<&PerceptionReport> for ReportDto {
    fn from(r: &PerceptionReport) -> Self {
        Self {
            frame_no: r.frame_no,
            width: r.screen.w,
            height: r.screen.h,
            total_cells: r.total_cells,
            dirty_cells: r.dirty_cells,
            geometry_changed: r.geometry_changed,
            analysed_regions: r.analysed_regions.iter().copied().map(Into::into).collect(),
            analysed_px: r.analysed_px,
            delta: (&r.delta).into(),
            element_count: r.element_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ObservationDto {
    pub frames: u32,
    pub settled: bool,
    pub frame_no: u64,
    pub dirty_cells: usize,
    pub analysed_px: u64,
    #[serde(flatten)]
    pub delta: DeltaDto,
    pub element_count: usize,
}

impl From<&Observation> for ObservationDto {
    fn from(o: &Observation) -> Self {
        Self {
            frames: o.frames,
            settled: o.settled,
            frame_no: o.frame_no,
            dirty_cells: o.dirty_cells,
            analysed_px: o.analysed_px,
            delta: (&o.delta).into(),
            element_count: o.element_count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDto {
    Click { at: PointDto, button: &'static str },
    TypeText { text: String },
    PressKey { key: &'static str },
}

impl From<&Action> for ActionDto {
    fn from(a: &Action) -> Self {
        match a {
            Action::Click { at, button } => Self::Click {
                at: (*at).into(),
                button: button_name(*button),
            },
            Action::TypeText { text } => Self::TypeText { text: text.clone() },
            Action::PressKey { key } => Self::PressKey {
                key: key_name(*key),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReceiptDto {
    pub action: ActionDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// `false` when the input controller is a dry-run or a simulator.
    pub live: bool,
}

impl From<&ActionReceipt> for ReceiptDto {
    fn from(r: &ActionReceipt) -> Self {
        Self {
            action: (&r.action).into(),
            element_id: r.element.map(format_element_id),
            live: r.live,
        }
    }
}

/// Wall-clock measurements. They vary between runs, so they travel in `_meta` and keep
/// `structuredContent` reproducible.
pub fn timings_meta(t: &Timings) -> Value {
    json!({ "ommatix/timings_us": {
        "capture": micros(t.capture),
        "diff": micros(t.diff),
        "detect": micros(t.detect),
        "total": micros(t.total),
    }})
}

pub fn elapsed_meta(elapsed: Duration) -> Value {
    json!({ "ommatix/elapsed_us": micros(elapsed) })
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

pub fn format_element_id(id: ElementId) -> String {
    format!("e{}", id.0)
}

/// Parses the wire form of an element id (`"e12"`). Only the form ids are written in is
/// accepted, so one element has one id (`"e012"` is not `"e12"`).
pub fn parse_element_id(s: &str) -> Result<ElementId, String> {
    s.strip_prefix('e')
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .filter(|n| *n == "0" || !n.starts_with('0'))
        .and_then(|n| n.parse::<u64>().ok())
        .map(ElementId)
        .ok_or_else(|| format!("invalid element id {s:?}, expected e.g. \"e4\""))
}

fn kind_name(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Blob => "blob",
        ElementKind::Text => "text",
        ElementKind::Button => "button",
        ElementKind::Icon => "icon",
    }
}

/// Wire names of the keys, in the order the tool schema lists them.
pub const KEYS: [(&str, Key); 14] = [
    ("enter", Key::Enter),
    ("tab", Key::Tab),
    ("escape", Key::Escape),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("space", Key::Space),
    ("arrow_up", Key::ArrowUp),
    ("arrow_down", Key::ArrowDown),
    ("arrow_left", Key::ArrowLeft),
    ("arrow_right", Key::ArrowRight),
    ("home", Key::Home),
    ("end", Key::End),
    ("page_up", Key::PageUp),
    ("page_down", Key::PageDown),
];

pub const BUTTONS: [(&str, MouseButton); 3] = [
    ("left", MouseButton::Left),
    ("right", MouseButton::Right),
    ("middle", MouseButton::Middle),
];

pub fn parse_key(name: &str) -> Result<Key, String> {
    lookup(&KEYS, name).ok_or_else(|| format!("unknown key {name:?}"))
}

pub fn parse_button(name: &str) -> Result<MouseButton, String> {
    lookup(&BUTTONS, name).ok_or_else(|| format!("unknown mouse button {name:?}"))
}

fn key_name(key: Key) -> &'static str {
    name_of(&KEYS, key)
}

fn button_name(button: MouseButton) -> &'static str {
    name_of(&BUTTONS, button)
}

fn lookup<T: Copy>(table: &[(&str, T)], name: &str) -> Option<T> {
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn name_of<T: Copy + PartialEq>(table: &[(&'static str, T)], value: T) -> &'static str {
    table
        .iter()
        .find(|(_, v)| *v == value)
        .map_or("unknown", |(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ids_round_trip_and_reject_junk() {
        assert_eq!(parse_element_id("e42"), Ok(ElementId(42)));
        assert_eq!(format_element_id(ElementId(42)), "e42");
        assert_eq!(parse_element_id("e0"), Ok(ElementId(0)));
        for junk in [
            "42",
            "e",
            "e-1",
            "e+1",
            "E1",
            "e1x",
            "e99999999999999999999",
            // Verification N12: one element, one id.
            "e00001",
            "e042",
            "e00",
        ] {
            assert!(parse_element_id(junk).is_err(), "{junk}");
        }
    }

    #[test]
    fn every_key_and_button_has_one_wire_name() {
        for (name, key) in KEYS {
            assert_eq!(parse_key(name), Ok(key));
            assert_eq!(key_name(key), name);
        }
        for (name, button) in BUTTONS {
            assert_eq!(parse_button(name), Ok(button));
            assert_eq!(button_name(button), name);
        }
        assert!(parse_key("Enter").is_err());
    }
}
