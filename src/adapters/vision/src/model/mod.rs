//! Model-independent pieces of the ONNX detector, in safe Rust and testable without any
//! model: the manifest, the integrity check, the letterbox and its inverse, the decoding
//! of the output tensor and NMS, and the supply-chain gate that checks every file before
//! anything is loaded. The `onnx` feature adds the runtime that uses them.

pub mod artifacts;
pub mod decode;
pub mod integrity;
pub mod letterbox;
pub mod manifest;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Affordance, Control};
use thiserror::Error;

use self::decode::Candidate;
use self::letterbox::Letterbox;
use self::manifest::ClassSpec;

/// Why a model artefact or its output was refused.
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("invalid SHA-256 digest {0:?}: expected 64 hexadecimal characters")]
    InvalidDigest(String),
    #[error("{what}: SHA-256 mismatch (expected {expected}, found {actual}); refusing to load")]
    DigestMismatch {
        what: String,
        expected: String,
        actual: String,
    },
    #[error("{what}: cannot read {path}")]
    Io {
        what: String,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{what}: larger than {max_bytes} bytes; refusing to load")]
    TooLarge { what: String, max_bytes: u64 },
    #[error("invalid model manifest: {0}")]
    InvalidManifest(String),
    #[error("unexpected model output: {0}")]
    UnexpectedOutput(String),
}

/// Controls for the candidates of one crop: each box is taken back to frame coordinates
/// (SDD §2.8: `x = (x_t - pad_x) / scale + origin_x`), clipped to `clip` (the region,
/// inside the frame), and given the role and actionability its class declares. Boxes with
/// no area left are dropped.
pub fn controls_from_candidates(
    candidates: &[Candidate],
    letterbox: &Letterbox,
    origin: (f64, f64),
    clip: &BBox,
    classes: &[ClassSpec],
) -> Vec<Control> {
    candidates
        .iter()
        .filter_map(|c| {
            let class = classes.get(c.class)?;
            let (x0, y0) = letterbox.to_crop(c.bbox[0], c.bbox[1]);
            let (x1, y1) = letterbox.to_crop(c.bbox[2], c.bbox[3]);
            let bbox = BBox::new(x0 + origin.0, y0 + origin.1, x1 + origin.0, y1 + origin.1)
                .ok()?
                .clip(clip)?;
            Some(Control {
                bbox,
                role: class.role,
                affordance: Affordance {
                    actionability: class.actionability,
                    enabled: true,
                    visible: true,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ommatix_domain::fusion::{Actionability, Role};

    use super::*;

    #[test]
    fn candidates_return_to_frame_coordinates_clipped_to_the_region() {
        // A 200x100 crop at (50, 20), letterboxed into 64x64: scale 0.32, pad_y 16.
        let lb = Letterbox::fit(200, 100, 64, 64).unwrap();
        let classes = [
            ClassSpec {
                name: "button".into(),
                role: Role::Button,
                actionability: Actionability::Unknown,
            },
            ClassSpec {
                name: "thing".into(),
                role: Role::Unknown,
                actionability: Actionability::Confirmed,
            },
        ];
        let region = BBox::new(50.0, 20.0, 250.0, 120.0).unwrap();
        let candidates = [
            // Crop (25, 25)–(75, 50) → frame (75, 45)–(125, 70).
            Candidate {
                bbox: [8.0, 24.0, 24.0, 32.0],
                class: 0,
                score: 0.9,
            },
            // Sticks out of the crop to the left: clipped at the region's edge.
            Candidate {
                bbox: [-3.2, 24.0, 3.2, 32.0],
                class: 1,
                score: 0.8,
            },
            // Entirely in the padding: nothing left after clipping.
            Candidate {
                bbox: [10.0, 2.0, 20.0, 10.0],
                class: 0,
                score: 0.7,
            },
            // A class the manifest does not list: dropped.
            Candidate {
                bbox: [8.0, 24.0, 24.0, 32.0],
                class: 7,
                score: 0.6,
            },
        ];
        let controls = controls_from_candidates(&candidates, &lb, (50.0, 20.0), &region, &classes);
        assert_eq!(controls.len(), 2);
        let close = |b: &BBox, e: [f64; 4]| {
            [b.x0(), b.y0(), b.x1(), b.y1()]
                .iter()
                .zip(e)
                .all(|(a, e)| (a - e).abs() < 1e-9)
        };
        assert!(
            close(&controls[0].bbox, [75.0, 45.0, 125.0, 70.0]),
            "{:?}",
            controls[0].bbox
        );
        assert_eq!(controls[0].role, Role::Button);
        assert_eq!(controls[0].affordance.actionability, Actionability::Unknown);
        assert!(
            close(&controls[1].bbox, [50.0, 45.0, 60.0, 70.0]),
            "{:?}",
            controls[1].bbox
        );
        assert_eq!(
            controls[1].affordance.actionability,
            Actionability::Confirmed
        );
    }
}
