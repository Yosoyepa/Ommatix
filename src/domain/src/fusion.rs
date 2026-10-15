//! Algorithm 2 of the thesis: spatial reconciliation and association (chapter 04 §4.3,
//! ADR-0003).
//!
//! Detector boxes (controls) and OCR words are fused conservatively: text inside a control
//! goes to the most specific owner by IoC, words are read line by line, external labels are
//! chosen by an anisotropic cost inside a corridor, and every doubt is an abstention rather
//! than a guess. Geometry never makes anything actionable on its own.
//!
//! Reference: `normalized`, `reading_order`, `internal_owners`, `corridor` and
//! `choose_external_label` in the Python oracle (OR-21..OR-28).

use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::bbox::BBox;
use crate::candidates::{CandidateSearch, ControlSearch};

#[derive(Debug, Error, Clone, Copy, PartialEq)]
pub enum FusionError {
    #[error("IoC threshold must be in [0, 1], got {0}")]
    InvalidThreshold(f64),
    #[error("label margin must be finite and non-negative, got {0}")]
    InvalidMargin(f64),
}

/// One OCR word with its box.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub bbox: BBox,
}

impl Word {
    pub fn new(text: impl Into<String>, bbox: BBox) -> Self {
        Self {
            text: text.into(),
            bbox,
        }
    }
}

/// Text key for comparing labels: Unicode NFC, lower case, runs of whitespace collapsed to
/// one space. Word boundaries survive: `"a b"` and `"ab"` stay different.
///
/// The oracle uses Python's `casefold`; `to_lowercase` differs from it only on a few
/// characters such as `ß`, which casefold maps to `ss`.
pub fn normalized(text: &str) -> String {
    let nfc: String = text.nfc().collect();
    nfc.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words in reading order: grouped into lines first, then left to right.
///
/// A word joins the line whose anchor (first word) has the closest centre, if that distance
/// is at most half the smaller height; otherwise it opens a new line. Lines keep the order in
/// which they were opened (top to bottom), and the result does not depend on the input order.
pub fn reading_order(words: &[Word]) -> Vec<Word> {
    let mut ordered: Vec<&Word> = words.iter().collect();
    ordered.sort_by(|a, b| {
        a.bbox
            .center()
            .1
            .total_cmp(&b.bbox.center().1)
            .then(a.bbox.x0().total_cmp(&b.bbox.x0()))
            .then_with(|| a.text.cmp(&b.text))
    });
    let mut lines: Vec<Vec<&Word>> = Vec::new();
    for word in ordered {
        let cy = word.bbox.center().1;
        let best = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let anchor = line[0];
                let delta = (cy - anchor.bbox.center().1).abs();
                (delta <= 0.5 * word.bbox.height().min(anchor.bbox.height())).then_some((delta, i))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        match best {
            Some((_, i)) => lines[i].push(word),
            None => lines.push(vec![word]),
        }
    }
    lines
        .into_iter()
        .flat_map(|mut line| {
            line.sort_by(|a, b| {
                a.bbox
                    .x0()
                    .total_cmp(&b.bbox.x0())
                    .then(a.bbox.y0().total_cmp(&b.bbox.y0()))
                    .then_with(|| a.text.cmp(&b.text))
            });
            line
        })
        .cloned()
        .collect()
}

/// Python's `math.isclose` with its default relative tolerance.
fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// Index of the control that owns `word`, if any: among the controls whose IoC over the word
/// reaches `threshold`, the smallest; two of (nearly) equal area abstain. The candidates come
/// from `search`; the IoC filter and the order are the same whichever enumerator found them.
fn owner_of(search: &ControlSearch<'_>, word: &Word, threshold: f64) -> Option<usize> {
    let boxes = search.boxes();
    let mut claims: Vec<(f64, usize)> = Vec::new();
    search.owner_candidates(&word.bbox, threshold, |i| {
        if boxes[i].ioc(&word.bbox) >= threshold {
            claims.push((boxes[i].area(), i));
        }
    });
    claims.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    match claims.as_slice() {
        [] => None,
        [first, second, ..] if is_close(first.0, second.0) => None,
        [first, ..] => Some(first.1),
    }
}

/// The words each control owns, in reading order, indexed like `controls`. A panel does not
/// take text from a control inside it; equal claims abstain. Scans every control (the
/// reference).
pub fn internal_owners(
    controls: &[BBox],
    words: &[Word],
    threshold: f64,
) -> Result<Vec<Vec<Word>>, FusionError> {
    internal_owners_with(controls, words, threshold, CandidateSearch::Scan)
}

/// [`internal_owners`] with the candidates enumerated by `candidates` (ADR-0027). The result
/// is the same for every enumerator.
pub fn internal_owners_with(
    controls: &[BBox],
    words: &[Word],
    threshold: f64,
    candidates: CandidateSearch,
) -> Result<Vec<Vec<Word>>, FusionError> {
    check_threshold(threshold)?;
    let search = ControlSearch::new(controls, candidates);
    let mut owned = vec![Vec::new(); controls.len()];
    for word in words {
        if let Some(i) = owner_of(&search, word, threshold) {
            owned[i].push(word.clone());
        }
    }
    Ok(owned.iter().map(|w| reading_order(w)).collect())
}

fn check_threshold(threshold: f64) -> Result<(), FusionError> {
    if (0.0..=1.0).contains(&threshold) {
        Ok(())
    } else {
        Err(FusionError::InvalidThreshold(threshold))
    }
}

/// The empty space between a text to the left of or above a control and the control's
/// facing edge, excluding both boxes. LTR geometry only.
pub fn corridor(text: &BBox, control: &BBox) -> Option<BBox> {
    if text.x1() < control.x0() {
        let (y0, y1) = (text.y0().max(control.y0()), text.y1().min(control.y1()));
        if y0 < y1 {
            return BBox::new(text.x1(), y0, control.x0(), y1).ok();
        }
    }
    if text.y1() < control.y0() {
        let (x0, x1) = (text.x0().max(control.x0()), text.x1().min(control.x1()));
        if x0 < x1 {
            return BBox::new(x0, text.y1(), x1, control.y0()).ok();
        }
    }
    None
}

/// Outcome of the external label search for one control.
#[derive(Debug, Clone, PartialEq)]
pub enum LabelChoice {
    /// One candidate is clearly better than the rest.
    Label(Word),
    /// The two best candidates are too close to call.
    Ambiguous,
    /// No word to the left or above qualifies.
    NoCandidate,
}

/// Cost of `word` as the external label of `control`, in units of the local scale, or `None`
/// when it is not to the left (aligned) or above (left edges aligned), or too far.
fn label_cost(control: &BBox, word: &BBox) -> Option<f64> {
    let scale = control.height().max(word.height()).max(1.0);
    let dy = (word.center().1 - control.center().1).abs();
    let dx0 = (word.x0() - control.x0()).abs();
    let cost = if word.x1() <= control.x0() && dy <= 0.8 * scale {
        (control.x0() - word.x1() + 2.5 * dy) / scale
    } else if word.y1() <= control.y0() && dx0 <= scale {
        (control.y0() - word.y1() + 1.5 * dx0) / scale
    } else {
        return None;
    };
    (cost <= 6.0).then_some(cost)
}

/// Chooses the external label of `control` among `words`. A word is skipped when another
/// control (not one containing `control`, like a background panel) lies in the corridor
/// between them. Abstains when the best two costs are less than `margin` apart.
pub fn choose_external_label(
    control: &BBox,
    words: &[Word],
    other_controls: &[BBox],
    margin: f64,
) -> Result<LabelChoice, FusionError> {
    let search = ControlSearch::new(other_controls, CandidateSearch::Scan);
    choose_label(control, words, &search, margin)
}

/// [`choose_external_label`] over the controls of one fusion, whose obstacles are enumerated by
/// `search` (ADR-0027). Words are read in source order, and the same predicate decides each
/// obstacle whichever enumerator finds it: another control, not one containing `control`, that
/// overlaps the corridor with positive area.
fn choose_label(
    control: &BBox,
    words: &[Word],
    search: &ControlSearch<'_>,
    margin: f64,
) -> Result<LabelChoice, FusionError> {
    if !(margin.is_finite() && margin >= 0.0) {
        return Err(FusionError::InvalidMargin(margin));
    }
    let boxes = search.boxes();
    let mut candidates: Vec<(f64, &Word)> = words
        .iter()
        .filter_map(|word| {
            let cost = label_cost(control, &word.bbox)?;
            let blocked = corridor(&word.bbox, control).is_some_and(|gap| {
                search.any_overlapping(&gap, |i| {
                    let other = &boxes[i];
                    other != control && !other.contains(control) && other.overlaps(&gap)
                })
            });
            (!blocked).then_some((cost, word))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.text.cmp(&b.1.text)));
    Ok(match candidates.as_slice() {
        [] => LabelChoice::NoCandidate,
        [best, second, ..] if second.0 - best.0 < margin => LabelChoice::Ambiguous,
        [best, ..] => LabelChoice::Label(best.1.clone()),
    })
}

/// Roles of the contract schema. A provider that only knows "interactive region" gives
/// `Unknown`; the engine does not guess `Input`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Button,
    Input,
    Checkbox,
    Dropdown,
    IconButton,
    TextNode,
    Unknown,
}

impl Role {
    /// Roles that look for an external label (above or to the left, in LTR).
    pub const fn takes_external_label(self) -> bool {
        matches!(self, Self::Input | Self::Dropdown | Self::Checkbox)
    }

    /// Roles whose inner text may be a value or a placeholder.
    pub const fn holds_value(self) -> bool {
        matches!(self, Self::Input | Self::Dropdown)
    }
}

/// Whether an element may be acted on, decided from evidence and policy, not from its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Actionability {
    Confirmed,
    Unknown,
    NotActionable,
}

/// What the text inside a field is. Geometry alone only justifies `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextKind {
    Value,
    Placeholder,
    Unknown,
}

/// Why an element may not be acted on.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum NotActionable {
    #[error("no confirmed affordance")]
    Unconfirmed,
    #[error("element is disabled")]
    Disabled,
    #[error("element is not visible")]
    Hidden,
}

/// The evidence an action needs: a confirmed affordance on an enabled, visible element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Affordance {
    pub actionability: Actionability,
    pub enabled: bool,
    pub visible: bool,
}

impl Affordance {
    /// What a word nobody owns gets: informative, never clickable by itself.
    pub const TEXT_NODE: Self = Self {
        actionability: Actionability::Unknown,
        enabled: true,
        visible: true,
    };

    pub const fn check(&self) -> Result<(), NotActionable> {
        if !matches!(self.actionability, Actionability::Confirmed) {
            Err(NotActionable::Unconfirmed)
        } else if !self.enabled {
            Err(NotActionable::Disabled)
        } else if !self.visible {
            Err(NotActionable::Hidden)
        } else {
            Ok(())
        }
    }
}

/// A detected control before fusion.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub bbox: BBox,
    pub role: Role,
    pub affordance: Affordance,
}

/// A control with the text fusion gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedControl {
    pub control: Control,
    /// Words inside the control, in reading order.
    pub inner_words: Vec<Word>,
    /// Kind of the inner text, for roles that hold a value.
    pub text_kind: Option<TextKind>,
    /// External label, for roles that take one.
    pub label: Option<LabelChoice>,
}

impl FusedControl {
    /// The inner words joined by single spaces, as they are read.
    pub fn inner_text(&self) -> String {
        self.inner_words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Fusion parameters; the defaults are the thesis's initial values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FusionParams {
    pub ioc_threshold: f64,
    pub label_margin: f64,
    /// IoU at which a detector box and an accessibility element are the same control
    /// (ADR-0017, [P]; containment uses `ioc_threshold`).
    pub accessibility_iou: f64,
    /// How owners and corridor obstacles are enumerated (ADR-0027). It changes how the
    /// candidates are found, never the result: [`CandidateSearch::Scan`] by default.
    pub candidate_search: CandidateSearch,
}

impl Default for FusionParams {
    fn default() -> Self {
        Self {
            ioc_threshold: 0.75,
            label_margin: 0.15,
            accessibility_iou: 0.5,
            candidate_search: CandidateSearch::Scan,
        }
    }
}

/// Controls with their text, plus the words nobody claimed.
#[derive(Debug, Clone, PartialEq)]
pub struct Fusion {
    /// Indexed like the input controls.
    pub controls: Vec<FusedControl>,
    /// Orphan words, in reading order: informative `text_node`s with [`Affordance::TEXT_NODE`].
    pub text_nodes: Vec<Word>,
}

/// Fuses controls and OCR words. A word picked as the label of several controls labels none
/// of them (only clearly separated, unshared choices are kept).
pub fn fuse(
    controls: &[Control],
    words: &[Word],
    params: FusionParams,
) -> Result<Fusion, FusionError> {
    let boxes: Vec<BBox> = controls.iter().map(|c| c.bbox).collect();
    check_threshold(params.ioc_threshold)?;
    // Built once for this fusion, over the reconciled controls: owners and obstacles share it.
    let search = ControlSearch::new(&boxes, params.candidate_search);
    let owners: Vec<Option<usize>> = words
        .iter()
        .map(|w| owner_of(&search, w, params.ioc_threshold))
        .collect();
    let free: Vec<Word> = words
        .iter()
        .zip(&owners)
        .filter(|(_, o)| o.is_none())
        .map(|(w, _)| w.clone())
        .collect();

    let mut labels = Vec::with_capacity(controls.len());
    for control in controls {
        labels.push(if control.role.takes_external_label() {
            Some(choose_label(
                &control.bbox,
                &free,
                &search,
                params.label_margin,
            )?)
        } else {
            None
        });
    }
    let times_chosen = |labels: &[Option<LabelChoice>], word: &Word| {
        labels
            .iter()
            .filter(|l| matches!(l, Some(LabelChoice::Label(w)) if w == word))
            .count()
    };
    let shared: Vec<bool> = labels
        .iter()
        .map(|l| matches!(l, Some(LabelChoice::Label(w)) if times_chosen(&labels, w) > 1))
        .collect();
    for (label, shared) in labels.iter_mut().zip(shared) {
        if shared {
            *label = Some(LabelChoice::Ambiguous);
        }
    }

    let fused = controls
        .iter()
        .enumerate()
        .zip(labels.iter())
        .map(|((i, control), label)| {
            let inner: Vec<Word> = words
                .iter()
                .zip(&owners)
                .filter(|(_, o)| **o == Some(i))
                .map(|(w, _)| w.clone())
                .collect();
            FusedControl {
                control: control.clone(),
                text_kind: control.role.holds_value().then_some(TextKind::Unknown),
                inner_words: reading_order(&inner),
                label: label.clone(),
            }
        })
        .collect();
    let is_label = |w: &Word| times_chosen(&labels, w) == 1;
    let orphans: Vec<Word> = free.into_iter().filter(|w| !is_label(w)).collect();
    Ok(Fusion {
        controls: fused,
        text_nodes: reading_order(&orphans),
    })
}

/// A control the accessibility tree reported (ADR-0017): its box, role and affordance as
/// the application derived them, its accessible name and whether it is a password field.
/// The name is untrusted screen data, like OCR text.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleControl {
    pub control: Control,
    /// The accessible name, whitespace-normalised; empty when the element has none.
    pub label: String,
    /// A password field: no OCR text inside it is ever published, as label or value.
    pub sensitive: bool,
    /// The element's native identity, already scoped by the application (ADR-0026): the
    /// tracking compares it, and it never reaches the agent. `None` when the platform gives
    /// none.
    pub discriminant: Option<String>,
}

/// Visual detections reconciled with accessibility elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconciled {
    /// The accessibility controls first (in input order), then the visual detections no
    /// accessibility element absorbed (in input order).
    pub controls: Vec<Control>,
    /// Indexed like `controls`: the accessibility element each control comes from.
    pub origins: Vec<Option<usize>>,
}

/// The accessibility element a visual box is the same control as, if any: the best IoU at
/// or above `iou`, else the smallest element containing the box by IoC at or above
/// `ioc`. Two equally good candidates abstain.
fn accessible_match(
    visual: &BBox,
    accessible: &[AccessibleControl],
    params: FusionParams,
) -> Option<usize> {
    let mut by_iou: Vec<(f64, usize)> = accessible
        .iter()
        .enumerate()
        .map(|(i, a)| (a.control.bbox.iou(visual), i))
        .filter(|(v, _)| *v >= params.accessibility_iou)
        .collect();
    by_iou.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    match by_iou.as_slice() {
        [first, second, ..] if is_close(first.0, second.0) => return None,
        [first, ..] => return Some(first.1),
        [] => {}
    }
    // Reconciliation stays on the reference scan (ADR-0027 covers owners and obstacles only).
    let containers: Vec<BBox> = accessible.iter().map(|a| a.control.bbox).collect();
    let search = ControlSearch::new(&containers, CandidateSearch::Scan);
    owner_of(&search, &Word::new("", *visual), params.ioc_threshold)
}

/// Reconciles detector boxes with accessibility elements (ADR-0017, an extension of
/// Algorithm 2). Every accessibility element becomes a control. A detector box that is
/// the same control as one of them ([`FusionParams::accessibility_iou`], or contained in
/// it by IoC) is absorbed: the element's box, role and affordance win. A detector box no
/// element absorbs stays as it was, except that it loses a confirmed affordance when its
/// click point (the box centre) falls on an accessibility element: a click there would
/// act on that element, not on what the box shows. Evidence only ever removes
/// actionability from a visual box, never adds it.
pub fn reconcile_accessibility(
    visual: &[Control],
    accessible: &[AccessibleControl],
    params: FusionParams,
) -> Result<Reconciled, FusionError> {
    check_threshold(params.ioc_threshold)?;
    check_threshold(params.accessibility_iou)?;
    let mut controls: Vec<Control> = accessible.iter().map(|a| a.control.clone()).collect();
    let mut origins: Vec<Option<usize>> = (0..accessible.len()).map(Some).collect();
    for v in visual {
        if accessible_match(&v.bbox, accessible, params).is_some() {
            continue;
        }
        let (cx, cy) = v.bbox.center();
        let lands_elsewhere = accessible.iter().any(|a| {
            let b = a.control.bbox;
            b.x0() <= cx && cx < b.x1() && b.y0() <= cy && cy < b.y1()
        });
        let mut control = v.clone();
        if lands_elsewhere && control.affordance.actionability == Actionability::Confirmed {
            control.affordance.actionability = Actionability::Unknown;
        }
        controls.push(control);
        origins.push(None);
    }
    Ok(Reconciled { controls, origins })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn texts(words: &[Word]) -> Vec<&str> {
        words.iter().map(|w| w.text.as_str()).collect()
    }

    fn control(bbox: BBox, role: Role, actionability: Actionability) -> Control {
        Control {
            bbox,
            role,
            affordance: Affordance {
                actionability,
                enabled: true,
                visible: true,
            },
        }
    }

    /// UT-18 (OR-23): the panel does not take the text of the button inside it.
    #[test]
    fn ut_18_most_specific_control_owns_the_text() {
        let panel = bx(0.0, 0.0, 200.0, 200.0);
        let button = bx(10.0, 10.0, 100.0, 40.0);
        let word = Word::new("OK", bx(20.0, 15.0, 30.0, 25.0));
        let owned = internal_owners(&[panel, button], std::slice::from_ref(&word), 0.75).unwrap();
        assert_eq!(owned, vec![vec![], vec![word.clone()]]);
        // Independent of the input order.
        let owned = internal_owners(&[button, panel], std::slice::from_ref(&word), 0.75).unwrap();
        assert_eq!(owned, vec![vec![word], vec![]]);
    }

    /// UT-19 (OR-24): two equal claims abstain, whatever their order.
    #[test]
    fn ut_19_equal_containers_abstain() {
        let b = bx(0.0, 0.0, 100.0, 100.0);
        let word = Word::new("OK", bx(10.0, 10.0, 30.0, 20.0));
        assert_eq!(
            internal_owners(&[b, b], &[word], 0.75).unwrap(),
            vec![vec![], vec![]]
        );
        assert_eq!(
            internal_owners(&[b], &[], 1.5),
            Err(FusionError::InvalidThreshold(1.5))
        );
    }

    /// UT-20 (OR-21, OR-22): lines first, then left to right, for every input permutation.
    #[test]
    fn ut_20_multiline_reading_order_is_deterministic() {
        let words = [
            Word::new("Sesión", bx(30.0, 10.0, 60.0, 20.0)),
            Word::new("Iniciar", bx(0.0, 10.0, 28.0, 20.0)),
        ];
        assert_eq!(texts(&reading_order(&words)), ["Iniciar", "Sesión"]);

        let words = [
            Word::new("A", bx(20.0, 0.0, 25.0, 10.0)),
            Word::new("B", bx(10.0, 4.0, 15.0, 14.0)),
            Word::new("C", bx(0.0, 8.0, 5.0, 18.0)),
        ];
        let expected = reading_order(&words);
        for p in permutations(&words) {
            assert_eq!(reading_order(&p), expected);
        }

        // Two real lines: the second line's first word is left of the first line's last.
        let words = [
            Word::new("world", bx(60.0, 0.0, 100.0, 10.0)),
            Word::new("again", bx(0.0, 20.0, 40.0, 30.0)),
            Word::new("hello", bx(10.0, 0.0, 50.0, 10.0)),
        ];
        assert_eq!(texts(&reading_order(&words)), ["hello", "world", "again"]);
    }

    fn permutations(words: &[Word]) -> Vec<Vec<Word>> {
        if words.len() <= 1 {
            return vec![words.to_vec()];
        }
        let mut out = Vec::new();
        for i in 0..words.len() {
            let mut rest = words.to_vec();
            let first = rest.remove(i);
            for mut tail in permutations(&rest) {
                tail.insert(0, first.clone());
                out.push(tail);
            }
        }
        out
    }

    /// UT-21 (OR-25): NFC and whitespace are normalised, word boundaries are kept.
    #[test]
    fn ut_21_unicode_is_normalised_and_word_boundaries_kept() {
        assert_eq!(normalized(" SEÑOR  "), normalized("sen\u{303}or"));
        assert_ne!(normalized("a b"), normalized("ab"));
        assert_eq!(normalized("Iniciar\t \n Sesión"), "iniciar sesión");
    }

    /// UT-22 (OR-26, OR-27): the corridor excludes both boxes, and a two-column form gets the
    /// right label for each field.
    #[test]
    fn ut_22_external_labels_use_corridor_and_cost() {
        assert_eq!(
            corridor(&bx(0.0, 0.0, 10.0, 10.0), &bx(20.0, 0.0, 50.0, 10.0)),
            Some(bx(10.0, 0.0, 20.0, 10.0))
        );

        let a = bx(100.0, 100.0, 300.0, 135.0);
        let b = bx(350.0, 100.0, 550.0, 135.0);
        let words = [
            Word::new("Nombre", bx(30.0, 105.0, 90.0, 128.0)),
            Word::new("Apellido", bx(350.0, 72.0, 430.0, 92.0)),
        ];
        let label = |c: &BBox| match choose_external_label(c, &words, &[a, b], 0.15).unwrap() {
            LabelChoice::Label(w) => w.text,
            other => panic!("expected a label, got {other:?}"),
        };
        assert_eq!(label(&a), "Nombre");
        assert_eq!(label(&b), "Apellido");
    }

    /// UT-23 (OR-28): two equally plausible labels give ambiguity, not the first one.
    #[test]
    fn ut_23_ambiguous_external_label_abstains() {
        let field = bx(100.0, 100.0, 200.0, 140.0);
        let t = bx(100.0, 70.0, 150.0, 90.0);
        let words = [Word::new("A", t), Word::new("B", t)];
        assert_eq!(
            choose_external_label(&field, &words, &[], 0.15),
            Ok(LabelChoice::Ambiguous)
        );
        let reversed = [words[1].clone(), words[0].clone()];
        assert_eq!(
            choose_external_label(&field, &reversed, &[], 0.15),
            Ok(LabelChoice::Ambiguous)
        );
    }

    /// UT-24 (OR-41): an orphan word stays an informative text node and cannot be acted on.
    #[test]
    fn ut_24_orphan_text_is_not_actionable() {
        let button = control(
            bx(0.0, 100.0, 80.0, 130.0),
            Role::Button,
            Actionability::Confirmed,
        );
        let title = Word::new("Configuración", bx(0.0, 0.0, 120.0, 20.0));
        let fusion = fuse(
            &[button],
            std::slice::from_ref(&title),
            FusionParams::default(),
        )
        .unwrap();
        assert_eq!(fusion.text_nodes, vec![title]);
        assert_eq!(
            Affordance::TEXT_NODE.check(),
            Err(NotActionable::Unconfirmed)
        );
    }

    /// UT-25: text inside a field keeps `text_kind = unknown`; no value is inferred.
    #[test]
    fn ut_25_inner_field_text_kind_stays_unknown() {
        let field = control(
            bx(0.0, 0.0, 200.0, 30.0),
            Role::Input,
            Actionability::Unknown,
        );
        let button = control(
            bx(0.0, 50.0, 80.0, 80.0),
            Role::Button,
            Actionability::Confirmed,
        );
        let words = [
            Word::new("usuario@correo", bx(5.0, 5.0, 120.0, 25.0)),
            Word::new("Entrar", bx(10.0, 55.0, 60.0, 75.0)),
        ];
        let fusion = fuse(&[field, button], &words, FusionParams::default()).unwrap();
        assert_eq!(fusion.controls[0].inner_text(), "usuario@correo");
        assert_eq!(fusion.controls[0].text_kind, Some(TextKind::Unknown));
        assert_eq!(fusion.controls[1].text_kind, None);
    }

    /// UT-26 (OR-45, OR-46): disabled or hidden elements get no permission, even when their
    /// affordance is confirmed.
    #[test]
    fn ut_26_hidden_or_disabled_elements_are_not_actionable() {
        let confirmed = Affordance {
            actionability: Actionability::Confirmed,
            enabled: true,
            visible: true,
        };
        assert_eq!(confirmed.check(), Ok(()));
        let disabled = Affordance {
            enabled: false,
            ..confirmed
        };
        assert_eq!(disabled.check(), Err(NotActionable::Disabled));
        let hidden = Affordance {
            visible: false,
            ..confirmed
        };
        assert_eq!(hidden.check(), Err(NotActionable::Hidden));
        let unknown = Affordance {
            actionability: Actionability::Unknown,
            ..confirmed
        };
        assert_eq!(unknown.check(), Err(NotActionable::Unconfirmed));
    }

    #[test]
    fn background_panel_does_not_block_a_label() {
        let panel = bx(0.0, 0.0, 600.0, 400.0);
        let field = bx(100.0, 100.0, 300.0, 135.0);
        let words = [Word::new("Nombre", bx(30.0, 105.0, 90.0, 128.0))];
        assert!(matches!(
            choose_external_label(&field, &words, &[panel, field], 0.15),
            Ok(LabelChoice::Label(_))
        ));
        // A control between the word and the field does block it.
        let between = bx(92.0, 100.0, 98.0, 135.0);
        assert_eq!(
            choose_external_label(&field, &words, &[field, between], 0.15),
            Ok(LabelChoice::NoCandidate)
        );
    }

    #[test]
    fn fuse_assigns_labels_and_keeps_label_words_out_of_text_nodes() {
        let field = control(
            bx(100.0, 100.0, 300.0, 135.0),
            Role::Input,
            Actionability::Unknown,
        );
        let words = [
            Word::new("Nombre", bx(30.0, 105.0, 90.0, 128.0)),
            Word::new("Formulario", bx(0.0, 0.0, 120.0, 20.0)),
        ];
        let fusion = fuse(&[field], &words, FusionParams::default()).unwrap();
        assert_eq!(
            fusion.controls[0].label,
            Some(LabelChoice::Label(words[0].clone()))
        );
        assert_eq!(texts(&fusion.text_nodes), ["Formulario"]);
    }

    #[test]
    fn a_label_claimed_by_two_controls_labels_neither() {
        let left = control(
            bx(100.0, 100.0, 200.0, 130.0),
            Role::Input,
            Actionability::Unknown,
        );
        let below = control(
            bx(40.0, 140.0, 200.0, 170.0),
            Role::Input,
            Actionability::Unknown,
        );
        // Left of `left` and above `below`: each picks it as its only candidate.
        let word = Word::new("Ciudad", bx(40.0, 105.0, 90.0, 125.0));
        let fusion = fuse(
            &[left, below],
            std::slice::from_ref(&word),
            FusionParams::default(),
        )
        .unwrap();
        let labels: Vec<_> = fusion.controls.iter().map(|c| c.label.clone()).collect();
        assert!(
            labels
                .iter()
                .all(|l| !matches!(l, Some(LabelChoice::Label(_)))),
            "{labels:?}"
        );
        assert_eq!(fusion.text_nodes, vec![word]);
    }

    fn accessible(bbox: BBox, role: Role, label: &str) -> AccessibleControl {
        AccessibleControl {
            control: control(bbox, role, Actionability::Confirmed),
            label: label.to_owned(),
            sensitive: false,
            discriminant: None,
        }
    }

    /// ADR-0017: a detector box that is the same control as an accessibility element (by
    /// IoU, or contained in it) is absorbed into it, whatever the input order.
    #[test]
    fn ut_44_accessibility_absorbs_the_same_control() {
        let save = accessible(bx(100.0, 100.0, 200.0, 140.0), Role::Button, "Save");
        let shifted = control(
            bx(104.0, 102.0, 204.0, 142.0),
            Role::Unknown,
            Actionability::Unknown,
        );
        let inner_icon = control(
            bx(110.0, 110.0, 130.0, 130.0),
            Role::Unknown,
            Actionability::Confirmed,
        );
        let elsewhere = control(
            bx(300.0, 100.0, 400.0, 140.0),
            Role::Unknown,
            Actionability::Confirmed,
        );
        let visual = [shifted, inner_icon, elsewhere.clone()];
        let r = reconcile_accessibility(
            &visual,
            std::slice::from_ref(&save),
            FusionParams::default(),
        )
        .unwrap();
        assert_eq!(r.controls, vec![save.control.clone(), elsewhere.clone()]);
        assert_eq!(r.origins, vec![Some(0), None]);
        let reversed: Vec<Control> = visual.iter().rev().cloned().collect();
        let r = reconcile_accessibility(&reversed, &[save], FusionParams::default()).unwrap();
        assert_eq!(r.origins, vec![Some(0), None]);
        assert_eq!(r.controls[1], elsewhere);
    }

    /// ADR-0017 (S4 finding 2): a detector box no element absorbs, whose centre lies on an
    /// accessibility element, would click that element: it keeps its role and box but loses
    /// its confirmed affordance. Elsewhere, visual boxes stay as they were.
    #[test]
    fn a_visual_box_centred_on_another_control_is_not_confirmed() {
        let square = accessible(bx(100.0, 100.0, 140.0, 130.0), Role::Button, "Square");
        // Taller and wider than the button, centred on it (IoU 0.2, not contained).
        let wide = control(
            bx(60.0, 80.0, 180.0, 150.0),
            Role::Unknown,
            Actionability::Confirmed,
        );
        let r = reconcile_accessibility(
            std::slice::from_ref(&wide),
            &[square],
            FusionParams::default(),
        )
        .unwrap();
        assert_eq!(r.origins, vec![Some(0), None]);
        assert_eq!(r.controls[1].bbox, wide.bbox);
        assert_eq!(
            r.controls[1].affordance.actionability,
            Actionability::Unknown
        );
    }

    /// Two equally good accessibility matches abstain: the box stays visual (and is not
    /// confirmed, since its centre lies on them).
    #[test]
    fn equal_accessibility_matches_abstain() {
        let b = bx(0.0, 0.0, 100.0, 40.0);
        let twins = [
            accessible(b, Role::Button, "A"),
            accessible(b, Role::Button, "B"),
        ];
        let v = control(b, Role::Unknown, Actionability::Confirmed);
        let r = reconcile_accessibility(&[v], &twins, FusionParams::default()).unwrap();
        assert_eq!(r.origins, vec![Some(0), Some(1), None]);
        assert_eq!(
            r.controls[2].affordance.actionability,
            Actionability::Unknown
        );
        let bad = FusionParams {
            accessibility_iou: 1.5,
            ..FusionParams::default()
        };
        assert_eq!(
            reconcile_accessibility(&[], &[], bad),
            Err(FusionError::InvalidThreshold(1.5))
        );
    }

    // ---- ADR-0027: the candidate index. Every IDX test runs both enumerators on one fixture
    // and must get the reference's answer. Every mutant drops one check of the reference and
    // must then answer differently on the same fixture: that shows the check is needed.

    const BOTH: [CandidateSearch; 2] = [CandidateSearch::Scan, CandidateSearch::RStar];

    /// The index of the single owner among `owned` (every fixture here has one word).
    fn owner_index(owned: &[Vec<Word>]) -> Option<usize> {
        owned.iter().position(|w| !w.is_empty())
    }

    /// The checks of the owner decision that a mutant may drop or change.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OwnerMutant {
        /// No `ioc >= threshold` filter: every control the window meets claims the word.
        WithoutIoc,
        /// Ties go to the first claim instead of abstaining.
        FirstOnTie,
        /// Identical boxes count once (deduplication by geometry).
        GeometryDedupe,
        /// The window is the word's centre point instead of its box.
        CentreWindow,
        /// At a zero threshold the window is used (no fallback to every control).
        ZeroWindow,
        /// The largest claim wins instead of the smallest.
        LargestFirst,
    }

    fn owner_mutant(
        boxes: &[BBox],
        word: &BBox,
        threshold: f64,
        mutant: OwnerMutant,
    ) -> Option<usize> {
        let (cx, cy) = word.center();
        let meets_window = |c: &BBox| {
            c.x0() <= word.x1() && word.x0() <= c.x1() && c.y0() <= word.y1() && word.y0() <= c.y1()
        };
        let mut positions: Vec<usize> = (0..boxes.len())
            .filter(|&i| match mutant {
                OwnerMutant::CentreWindow => {
                    let c = &boxes[i];
                    c.x0() <= cx && cx < c.x1() && c.y0() <= cy && cy < c.y1()
                }
                OwnerMutant::ZeroWindow if threshold == 0.0 => meets_window(&boxes[i]),
                _ => true,
            })
            .collect();
        if mutant == OwnerMutant::GeometryDedupe {
            let mut seen: Vec<BBox> = Vec::new();
            positions.retain(|&i| {
                let first = !seen.contains(&boxes[i]);
                seen.push(boxes[i]);
                first
            });
        }
        let mut claims: Vec<(f64, usize)> = positions
            .into_iter()
            .filter(|&i| mutant == OwnerMutant::WithoutIoc || boxes[i].ioc(word) >= threshold)
            .map(|i| (boxes[i].area(), i))
            .collect();
        claims.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if mutant == OwnerMutant::LargestFirst {
            return claims.last().map(|c| c.1);
        }
        match claims.as_slice() {
            [] => None,
            [first, ..] if mutant == OwnerMutant::FirstOnTie => Some(first.1),
            [first, second, ..] if is_close(first.0, second.0) => None,
            [first, ..] => Some(first.1),
        }
    }

    /// The checks of the obstacle predicate and of the label order that a mutant may change.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum LabelMutant {
        /// Touching the corridor's edge counts as blocking.
        AnyContact,
        /// A control containing the labelled control blocks the corridor (a background panel).
        ContainerBlocks,
        /// The corridor is replaced by the envelope of the word and the control.
        EnvelopeCorridor,
        /// Only the best candidate is kept: no runner-up, so no ambiguity.
        TopOne,
    }

    fn touches(a: &BBox, b: &BBox) -> bool {
        a.x0() <= b.x1() && b.x0() <= a.x1() && a.y0() <= b.y1() && b.y0() <= a.y1()
    }

    fn label_mutant(
        control: &BBox,
        words: &[Word],
        others: &[BBox],
        margin: f64,
        mutant: LabelMutant,
    ) -> LabelChoice {
        let mut candidates: Vec<(f64, &Word)> = words
            .iter()
            .filter_map(|word| {
                let cost = label_cost(control, &word.bbox)?;
                let blocked = match corridor(&word.bbox, control) {
                    None => false,
                    Some(gap) => others.iter().any(|b| {
                        if b == control {
                            return false;
                        }
                        match mutant {
                            LabelMutant::AnyContact => !b.contains(control) && touches(b, &gap),
                            LabelMutant::ContainerBlocks => b.overlaps(&gap),
                            LabelMutant::EnvelopeCorridor => {
                                !b.contains(control) && b.overlaps(&word.bbox.union(control))
                            }
                            LabelMutant::TopOne => !b.contains(control) && b.overlaps(&gap),
                        }
                    }),
                };
                (!blocked).then_some((cost, word))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.text.cmp(&b.1.text)));
        match candidates.as_slice() {
            [] => LabelChoice::NoCandidate,
            [best, second, ..] if mutant != LabelMutant::TopOne && second.0 - best.0 < margin => {
                LabelChoice::Ambiguous
            }
            [best, ..] => LabelChoice::Label(best.1.clone()),
        }
    }

    fn label_of(control: &BBox, words: &[Word], others: &[BBox], margin: f64) -> LabelChoice {
        choose_external_label(control, words, others, margin).unwrap()
    }

    /// IDX-03 (RF-06): a panel, a button and a text that nest; the most specific control owns
    /// the text, whatever the enumerator and the input order. Without the order by area the
    /// panel would own it.
    #[test]
    fn idx_03_the_most_specific_owner_is_the_same_for_both_enumerators() {
        let panel = bx(0.0, 0.0, 200.0, 200.0);
        let button = bx(10.0, 10.0, 100.0, 40.0);
        let word = Word::new("OK", bx(20.0, 15.0, 30.0, 25.0));
        for strategy in BOTH {
            let owned = internal_owners_with(
                &[panel, button],
                std::slice::from_ref(&word),
                0.75,
                strategy,
            )
            .unwrap();
            assert_eq!(owner_index(&owned), Some(1), "{strategy:?}");
            let owned = internal_owners_with(
                &[button, panel],
                std::slice::from_ref(&word),
                0.75,
                strategy,
            )
            .unwrap();
            assert_eq!(owner_index(&owned), Some(0), "{strategy:?}");
        }
        assert_eq!(
            owner_mutant(
                &[panel, button],
                &word.bbox,
                0.75,
                OwnerMutant::LargestFirst
            ),
            Some(0)
        );
    }

    /// IDX-03 (mutation): without the IoC filter, a small control that only grazes the word
    /// (IoC 0.01) would own it, because the window meets it.
    #[test]
    fn idx_03_without_the_ioc_filter_a_grazing_control_owns_the_word() {
        let word = bx(0.0, 0.0, 100.0, 100.0);
        let grazing = bx(90.0, 90.0, 110.0, 110.0);
        let whole = bx(0.0, 0.0, 100.0, 100.0);
        let boxes = [grazing, whole];
        assert_eq!(
            owner_mutant(&boxes, &word, 0.75, OwnerMutant::WithoutIoc),
            Some(0)
        );
        for strategy in BOTH {
            let owned =
                internal_owners_with(&boxes, &[Word::new("x", word)], 0.75, strategy).unwrap();
            assert_eq!(owner_index(&owned), Some(1), "{strategy:?}");
        }
    }

    /// IDX-04 (RF-06): two equal claims abstain, for both enumerators.
    #[test]
    fn idx_04_equal_area_owners_abstain_for_both_enumerators() {
        let b = bx(0.0, 0.0, 100.0, 100.0);
        let word = Word::new("OK", bx(10.0, 10.0, 30.0, 20.0));
        for strategy in BOTH {
            let owned =
                internal_owners_with(&[b, b], std::slice::from_ref(&word), 0.75, strategy).unwrap();
            assert_eq!(owner_index(&owned), None, "{strategy:?}");
        }
        assert_eq!(
            owner_mutant(&[b, b], &word.bbox, 0.75, OwnerMutant::FirstOnTie),
            Some(0)
        );
    }

    /// IDX-07 (RF-06): identical boxes are two controls, so they are two claims and abstain.
    /// Deduplicating by geometry would keep one and give it the word.
    #[test]
    fn idx_07_identical_boxes_are_two_claims_not_one() {
        let b = bx(0.0, 0.0, 100.0, 100.0);
        let word = Word::new("OK", bx(10.0, 10.0, 30.0, 20.0));
        assert_eq!(
            owner_mutant(&[b, b], &word.bbox, 0.75, OwnerMutant::GeometryDedupe),
            Some(0)
        );
        for strategy in BOTH {
            let owned =
                internal_owners_with(&[b, b], std::slice::from_ref(&word), 0.75, strategy).unwrap();
            assert_eq!(owner_index(&owned), None, "{strategy:?}");
        }
    }

    /// IDX-02 (ADR-0027): at an IoC threshold of zero a disjoint control still qualifies, so
    /// the owner is the smallest control of the whole fusion, found by a full pass.
    #[test]
    fn idx_02_zero_threshold_keeps_the_reference_owner() {
        let large = bx(100.0, 100.0, 110.0, 110.0);
        let small = bx(0.0, 0.0, 5.0, 5.0);
        let boxes = [large, small];
        let word = bx(10.0, 10.0, 20.0, 20.0);
        for strategy in BOTH {
            let owned =
                internal_owners_with(&boxes, &[Word::new("x", word)], 0.0, strategy).unwrap();
            assert_eq!(owner_index(&owned), Some(1), "{strategy:?}");
        }
        assert_eq!(
            owner_mutant(&boxes, &word, 0.0, OwnerMutant::ZeroWindow),
            None
        );
    }

    /// IDX-08 (RF-06): the window of a word is the word's whole box. A centre-point window
    /// loses a control that covers 40 % of the word, which is still an owner at 0.25.
    #[test]
    fn idx_08_a_centre_point_window_loses_partial_owners() {
        let word = bx(0.0, 0.0, 10.0, 10.0);
        let partial = bx(0.0, 0.0, 10.0, 4.0);
        assert_eq!(partial.ioc(&word), 0.4);
        assert_eq!(
            owner_mutant(&[partial], &word, 0.25, OwnerMutant::CentreWindow),
            None
        );
        for strategy in BOTH {
            let owned =
                internal_owners_with(&[partial], &[Word::new("x", word)], 0.25, strategy).unwrap();
            assert_eq!(owner_index(&owned), Some(0), "{strategy:?}");
        }
    }

    /// IDX-05 (RF-06): two labels at the same cost stay ambiguous. The first one is not taken,
    /// and no top-k of the enumeration may hide the runner-up.
    #[test]
    fn idx_05_close_label_costs_stay_ambiguous_without_truncation() {
        let field = bx(100.0, 100.0, 300.0, 135.0);
        let words = [
            Word::new("Correo", bx(100.0, 72.0, 170.0, 92.0)),
            Word::new("Teléfono", bx(100.0, 72.0, 190.0, 92.0)),
        ];
        assert_eq!(
            label_of(&field, &words, &[field], 0.15),
            LabelChoice::Ambiguous
        );
        assert_eq!(
            label_mutant(&field, &words, &[field], 0.15, LabelMutant::TopOne),
            LabelChoice::Label(words[0].clone())
        );
        for strategy in BOTH {
            let fusion = fuse(
                &[control(field, Role::Input, Actionability::Unknown)],
                &words,
                FusionParams {
                    candidate_search: strategy,
                    ..FusionParams::default()
                },
            )
            .unwrap();
            assert_eq!(
                fusion.controls[0].label,
                Some(LabelChoice::Ambiguous),
                "{strategy:?}"
            );
        }
    }

    /// IDX-01 (RF-06): a control that only touches the corridor's top edge is not an obstacle.
    /// The corridor between the word and the field is x in [50, 100], y in [105, 115]; the
    /// blocker touches it along y = 105 and overlaps it nowhere.
    #[test]
    fn idx_01_corridor_edge_contact_does_not_block() {
        let field = bx(100.0, 100.0, 200.0, 120.0);
        let word = Word::new("Nombre", bx(0.0, 105.0, 50.0, 115.0));
        let touching = bx(40.0, 100.0, 60.0, 105.0);
        assert_eq!(
            corridor(&word.bbox, &field),
            Some(bx(50.0, 105.0, 100.0, 115.0))
        );
        assert_eq!(
            label_mutant(
                &field,
                std::slice::from_ref(&word),
                &[field, touching],
                0.15,
                LabelMutant::AnyContact
            ),
            LabelChoice::NoCandidate
        );
        for strategy in BOTH {
            let scene = [field, touching];
            let search = ControlSearch::new(&scene, strategy);
            let label = choose_label(&field, std::slice::from_ref(&word), &search, 0.15).unwrap();
            assert_eq!(label, LabelChoice::Label(word.clone()), "{strategy:?}");
        }
    }

    /// IDX-06 (RF-06): a panel that contains the labelled control and crosses its corridor is
    /// not an obstacle; a control that really crosses the corridor is one, for both enumerators.
    #[test]
    fn idx_06_a_panel_around_the_control_is_not_a_blocker() {
        let field = bx(100.0, 100.0, 200.0, 120.0);
        let panel = bx(-10.0, 0.0, 300.0, 200.0);
        let word = Word::new("Nombre", bx(0.0, 105.0, 50.0, 115.0));
        let words = std::slice::from_ref(&word);
        assert_eq!(
            label_mutant(
                &field,
                words,
                &[field, panel],
                0.15,
                LabelMutant::ContainerBlocks
            ),
            LabelChoice::NoCandidate
        );
        let crossing = bx(70.0, 104.0, 80.0, 116.0);
        for strategy in BOTH {
            for (scene, expected) in [
                (vec![field, panel], LabelChoice::Label(word.clone())),
                (vec![field, crossing], LabelChoice::NoCandidate),
            ] {
                let search = ControlSearch::new(&scene, strategy);
                assert_eq!(
                    choose_label(&field, words, &search, 0.15).unwrap(),
                    expected,
                    "{strategy:?}"
                );
            }
        }
    }

    /// IDX-06 (RF-06): the corridor is the gap between the facing edges, never the rectangle
    /// that contains the word and the control. A control inside that rectangle, outside the gap,
    /// does not block the label.
    #[test]
    fn idx_06_the_corridor_is_not_the_envelope_of_its_ends() {
        let field = bx(100.0, 100.0, 200.0, 120.0);
        let word = Word::new("Nombre", bx(0.0, 105.0, 50.0, 115.0));
        let inside_envelope = bx(10.0, 106.0, 20.0, 114.0);
        assert!(word.bbox.union(&field).overlaps(&inside_envelope));
        assert_eq!(
            label_mutant(
                &field,
                std::slice::from_ref(&word),
                &[field, inside_envelope],
                0.15,
                LabelMutant::EnvelopeCorridor
            ),
            LabelChoice::NoCandidate
        );
        for strategy in BOTH {
            let scene = [field, inside_envelope];
            let search = ControlSearch::new(&scene, strategy);
            let label = choose_label(&field, std::slice::from_ref(&word), &search, 0.15).unwrap();
            assert_eq!(label, LabelChoice::Label(word.clone()), "{strategy:?}");
        }
    }

    /// IDX-13 (RF-06): every control overlaps the word. The index keeps all of them, the owner
    /// abstains as the reference does, and the fusion is the same.
    #[test]
    fn idx_13_total_overlap_abstains_for_both_enumerators() {
        let same = bx(0.0, 0.0, 100.0, 100.0);
        let controls = vec![control(same, Role::Button, Actionability::Confirmed); 500];
        let boxes: Vec<BBox> = controls.iter().map(|c| c.bbox).collect();
        let words = [Word::new("OK", bx(1.0, 1.0, 2.0, 2.0))];
        let reference = fuse(&controls, &words, FusionParams::default()).unwrap();
        assert!(reference.controls.iter().all(|c| c.inner_words.is_empty()));
        assert_eq!(
            owner_mutant(&boxes, &words[0].bbox, 0.75, OwnerMutant::FirstOnTie),
            Some(0)
        );
        let indexed = fuse(
            &controls,
            &words,
            FusionParams {
                candidate_search: CandidateSearch::RStar,
                ..FusionParams::default()
            },
        )
        .unwrap();
        assert_eq!(indexed, reference);
    }

    /// IDX-14 (RF-06): a small fusion gives the same whole result for both enumerators, not just
    /// the same owners: labels, orphans and the order of every list.
    #[test]
    fn idx_14_a_small_fusion_is_the_same_for_both_enumerators() {
        let panel = control(
            bx(0.0, 0.0, 400.0, 90.0),
            Role::Unknown,
            Actionability::Unknown,
        );
        let button = control(
            bx(20.0, 20.0, 140.0, 50.0),
            Role::Button,
            Actionability::Confirmed,
        );
        let field = control(
            bx(100.0, 100.0, 300.0, 135.0),
            Role::Input,
            Actionability::Unknown,
        );
        let controls = [panel, button, field];
        let words = [
            Word::new("Aceptar", bx(40.0, 25.0, 110.0, 45.0)),
            Word::new("Nombre", bx(30.0, 105.0, 90.0, 128.0)),
            Word::new("Formulario", bx(0.0, 0.0, 120.0, 20.0)),
        ];
        let scan = fuse(&controls, &words, FusionParams::default()).unwrap();
        let tree = fuse(
            &controls,
            &words,
            FusionParams {
                candidate_search: CandidateSearch::RStar,
                ..FusionParams::default()
            },
        )
        .unwrap();
        assert_eq!(tree, scan);
        assert_eq!(scan.controls[1].inner_words, vec![words[0].clone()]);
        assert_eq!(
            scan.controls[2].label,
            Some(LabelChoice::Label(words[1].clone()))
        );
    }
}
