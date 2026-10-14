//! Decoding a YOLO-style detection tensor and deterministic non-maximum suppression.
//!
//! The only accepted output is the anchor-free YOLO head exported without NMS (the
//! YOLOv8/YOLO11 ONNX export): one `f32` tensor of rank 3 with batch 1, where each of the
//! `N` candidates has `4 + C` values: the box centre `cx, cy` and size `w, h` in input
//! tensor pixels, then one score per class in `[0, 1]` (already activated, no objectness
//! column). `C` must equal the manifest's class count. Two memory orders are accepted:
//!
//! - [`OutputLayout::ChannelsFirst`] (`"yolo_cxcywh_scores_cn"`): shape `[1, 4 + C, N]`.
//! - [`OutputLayout::CandidatesFirst`] (`"yolo_cxcywh_scores_nc"`): shape `[1, N, 4 + C]`.
//!
//! Anything else (another rank, batch, channel count, a non-finite value in a kept box) is
//! rejected, never guessed at.

use std::cmp::Ordering;

use super::ModelError;

/// Most candidates kept for NMS (the highest scores), bounding its quadratic cost.
pub const MAX_CANDIDATES: usize = 4096;

/// Most detections kept per region after NMS.
pub const MAX_DETECTIONS: usize = 512;

/// Most candidates a tensor may declare.
pub const MAX_ANCHORS: usize = 1 << 20;

/// Memory order of the YOLO output tensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputLayout {
    /// `[1, 4 + C, N]`.
    ChannelsFirst,
    /// `[1, N, 4 + C]`.
    CandidatesFirst,
}

impl OutputLayout {
    pub const ALL: [Self; 2] = [Self::ChannelsFirst, Self::CandidatesFirst];

    pub const fn name(self) -> &'static str {
        match self {
            Self::ChannelsFirst => "yolo_cxcywh_scores_cn",
            Self::CandidatesFirst => "yolo_cxcywh_scores_nc",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }
}

/// One candidate box in tensor coordinates, `[x0, y0, x1, y1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub bbox: [f64; 4],
    pub class: usize,
    pub score: f32,
}

impl Candidate {
    fn area(&self) -> f64 {
        (self.bbox[2] - self.bbox[0]) * (self.bbox[3] - self.bbox[1])
    }

    fn iou(&self, other: &Self) -> f64 {
        let w = self.bbox[2].min(other.bbox[2]) - self.bbox[0].max(other.bbox[0]);
        let h = self.bbox[3].min(other.bbox[3]) - self.bbox[1].max(other.bbox[1]);
        if w <= 0.0 || h <= 0.0 {
            return 0.0;
        }
        let inter = w * h;
        inter / (self.area() + other.area() - inter)
    }

    /// Highest score first; ties by class, then by corners, so the order is total.
    fn rank(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then(self.class.cmp(&other.class))
            .then_with(|| {
                self.bbox
                    .iter()
                    .zip(&other.bbox)
                    .map(|(a, b)| a.total_cmp(b))
                    .find(|o| o.is_ne())
                    .unwrap_or(Ordering::Equal)
            })
    }
}

/// The candidates of `data` (with shape `shape`) whose best class score reaches
/// `score_threshold`, best first, at most [`MAX_CANDIDATES`].
pub fn decode(
    shape: &[i64],
    data: &[f32],
    layout: OutputLayout,
    classes: usize,
    score_threshold: f32,
) -> Result<Vec<Candidate>, ModelError> {
    let bad = |m: String| ModelError::UnexpectedOutput(m);
    let channels = 4 + classes;
    let dims: Vec<usize> = shape
        .iter()
        .map(|&d| {
            usize::try_from(d)
                .map_err(|_| bad(format!("dynamic or negative dimension in {shape:?}")))
        })
        .collect::<Result<_, _>>()?;
    let anchors = match (layout, dims.as_slice()) {
        (OutputLayout::ChannelsFirst, &[1, c, n]) | (OutputLayout::CandidatesFirst, &[1, n, c])
            if c == channels =>
        {
            n
        }
        _ => {
            return Err(bad(format!(
                "expected {} with 4 + {classes} channels, got shape {shape:?}",
                layout.name()
            )));
        }
    };
    if anchors > MAX_ANCHORS || data.len() != anchors * channels {
        return Err(bad(format!(
            "{} values for shape {shape:?} (at most {MAX_ANCHORS} candidates)",
            data.len()
        )));
    }
    let at = |n: usize, c: usize| match layout {
        OutputLayout::ChannelsFirst => data[c * anchors + n],
        OutputLayout::CandidatesFirst => data[n * channels + c],
    };
    let mut out = Vec::new();
    for n in 0..anchors {
        let (mut class, mut score) = (0, f32::NEG_INFINITY);
        for k in 0..classes {
            let s = at(n, 4 + k);
            if s > score {
                (class, score) = (k, s);
            }
        }
        if score.is_nan() || score < score_threshold {
            continue;
        }
        let [cx, cy, w, h] = [0, 1, 2, 3].map(|c| f64::from(at(n, c)));
        if ![cx, cy, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
            continue;
        }
        out.push(Candidate {
            bbox: [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0],
            class,
            score,
        });
    }
    out.sort_by(Candidate::rank);
    out.truncate(MAX_CANDIDATES);
    Ok(out)
}

/// Greedy per-class non-maximum suppression: in rank order, a candidate is dropped when its
/// IoU with a kept candidate of the same class exceeds `iou_threshold`. Deterministic: the
/// result depends only on the set of candidates, not on their input order.
pub fn nms(mut candidates: Vec<Candidate>, iou_threshold: f32) -> Vec<Candidate> {
    candidates.sort_by(Candidate::rank);
    let threshold = f64::from(iou_threshold);
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if kept.len() == MAX_DETECTIONS {
            break;
        }
        let suppressed = kept
            .iter()
            .any(|k| k.class == candidate.class && k.iou(&candidate) > threshold);
        if !suppressed {
            kept.push(candidate);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two classes, three candidates, in `[1, N, 4 + C]` order.
    fn candidates_first() -> Vec<f32> {
        vec![
            10.0, 10.0, 4.0, 2.0, 0.9, 0.1, // class 0, kept
            30.0, 30.0, 6.0, 6.0, 0.2, 0.6, // class 1, kept
            50.0, 50.0, 2.0, 2.0, 0.1, 0.2, // below the threshold
        ]
    }

    fn transpose(data: &[f32], n: usize, c: usize) -> Vec<f32> {
        (0..c)
            .flat_map(|ci| (0..n).map(move |ni| data[ni * c + ci]))
            .collect()
    }

    #[test]
    fn both_layouts_decode_to_the_same_candidates() {
        let nc = candidates_first();
        let cn = transpose(&nc, 3, 6);
        let a = decode(&[1, 3, 6], &nc, OutputLayout::CandidatesFirst, 2, 0.5).unwrap();
        let b = decode(&[1, 6, 3], &cn, OutputLayout::ChannelsFirst, 2, 0.5).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a,
            [
                Candidate {
                    bbox: [8.0, 9.0, 12.0, 11.0],
                    class: 0,
                    score: 0.9
                },
                Candidate {
                    bbox: [27.0, 27.0, 33.0, 33.0],
                    class: 1,
                    score: 0.6
                },
            ]
        );
    }

    #[test]
    fn other_shapes_are_rejected() {
        let nc = candidates_first();
        for (shape, layout, classes) in [
            (vec![1, 3, 6], OutputLayout::ChannelsFirst, 2),
            (vec![1, 3, 6], OutputLayout::CandidatesFirst, 3),
            (vec![2, 3, 6], OutputLayout::CandidatesFirst, 2),
            (vec![3, 6], OutputLayout::CandidatesFirst, 2),
            (vec![1, -1, 6], OutputLayout::CandidatesFirst, 2),
            (vec![1, 2, 6], OutputLayout::CandidatesFirst, 2),
        ] {
            assert!(
                matches!(
                    decode(&shape, &nc, layout, classes, 0.5),
                    Err(ModelError::UnexpectedOutput(_))
                ),
                "{shape:?} {layout:?} {classes}"
            );
        }
    }

    #[test]
    fn degenerate_and_non_finite_boxes_are_dropped() {
        let data = vec![
            f32::NAN,
            1.0,
            1.0,
            1.0,
            0.9, //
            5.0,
            5.0,
            0.0,
            1.0,
            0.9, //
            5.0,
            5.0,
            1.0,
            1.0,
            f32::NAN, //
            5.0,
            5.0,
            1.0,
            f32::INFINITY,
            0.9,
        ];
        assert!(
            decode(&[1, 4, 5], &data, OutputLayout::CandidatesFirst, 1, 0.1)
                .unwrap()
                .is_empty()
        );
    }

    fn c(bbox: [f64; 4], class: usize, score: f32) -> Candidate {
        Candidate { bbox, class, score }
    }

    #[test]
    fn nms_keeps_the_best_of_each_overlapping_group_per_class() {
        let input = vec![
            c([0.0, 0.0, 10.0, 10.0], 0, 0.8),
            c([1.0, 0.0, 11.0, 10.0], 0, 0.9), // IoU 0.82 with the first: wins
            c([1.0, 0.0, 11.0, 10.0], 1, 0.7), // other class: kept
            c([20.0, 0.0, 30.0, 10.0], 0, 0.5), // no overlap: kept
        ];
        let kept = nms(input.clone(), 0.5);
        assert_eq!(kept, vec![input[1], input[2], input[3]]);
        // The order of the input does not matter.
        let mut reversed = input;
        reversed.reverse();
        assert_eq!(nms(reversed, 0.5), kept);
    }

    #[test]
    fn equal_scores_are_ranked_by_class_then_corners() {
        let a = c([5.0, 0.0, 15.0, 10.0], 0, 0.5);
        let b = c([0.0, 0.0, 10.0, 10.0], 0, 0.5);
        assert_eq!(nms(vec![a, b], 0.3), vec![b]);
        assert_eq!(nms(vec![b, a], 0.3), vec![b]);
    }

    #[test]
    fn layouts_round_trip_by_name() {
        for layout in OutputLayout::ALL {
            assert_eq!(OutputLayout::parse(layout.name()), Some(layout));
        }
        assert_eq!(OutputLayout::parse("yolo"), None);
    }
}
