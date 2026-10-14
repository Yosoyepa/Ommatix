//! [`OnnxVisionDetector`]: a generic, fail-closed YOLO-style detector on ONNX Runtime
//! (feature `onnx`, ADR-0009).
//!
//! Nothing is downloaded at build time: `ort` is built with `load-dynamic`, and the
//! operator points the configuration at an ONNX Runtime library pinned by SHA-256. Loading
//! goes in this order, and any failure refuses the detector (there is no fallback model):
//!
//! 1. the manifest is read (bounded) and its SHA-256 checked against the operator's pin;
//! 2. its detector entry is validated ([`DetectorSpec`]);
//! 3. the runtime library's SHA-256 is checked, then the library is loaded by its absolute
//!    path (the check and the load are two steps: the library must sit in a directory only
//!    the operator can write);
//! 4. the weights are read into memory (bounded), their SHA-256 checked against the
//!    manifest, and the session is built from those same bytes.
//!
//! Per region, the crop is letterboxed into the input ([`Letterbox`]), the output is
//! decoded and suppressed ([`decode`], [`nms`]), and the boxes are taken back to the frame
//! and clipped to the region ([`controls_from_candidates`]).

use std::path::{Path, PathBuf};

use ommatix_application::{PortError, VisionDetector};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Control;
use ommatix_domain::incremental::BgraFrame;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

use crate::crop::{PixelRect, crop_bgra};
use crate::model::decode::{decode, nms};
use crate::model::integrity::{Sha256Digest, read_verified};
use crate::model::letterbox::Letterbox;
use crate::model::manifest::{DetectorSpec, MAX_MANIFEST_BYTES};
use crate::model::{ModelError, controls_from_candidates};

const NAME: &str = "onnx";

/// Largest weights file loaded, in bytes.
pub const MAX_MODEL_BYTES: u64 = 1024 * 1024 * 1024;

/// Largest ONNX Runtime library hashed, in bytes.
pub const MAX_RUNTIME_BYTES: u64 = 512 * 1024 * 1024;

/// Where the detector comes from, each file pinned by the operator.
#[derive(Debug, Clone)]
pub struct OnnxDetectorSource {
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
    pub runtime_path: PathBuf,
    pub runtime_sha256: String,
}

/// A YOLO-style detector whose weights passed every check. It exists only after a
/// verified load, so [`VisionDetector::models_loaded`] is always `true`.
pub struct OnnxVisionDetector {
    spec: DetectorSpec,
    session: Session,
    input_name: String,
}

impl std::fmt::Debug for OnnxVisionDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxVisionDetector")
            .field("spec", &self.spec)
            .field("input_name", &self.input_name)
            .finish_non_exhaustive()
    }
}

fn port(message: impl Into<String>) -> PortError {
    PortError::new(NAME, message)
}

fn model_error(e: ModelError) -> PortError {
    port("model refused").with_source(e)
}

impl OnnxVisionDetector {
    /// Loads and verifies the detector (see the module documentation).
    pub fn load(source: &OnnxDetectorSource) -> Result<Self, PortError> {
        let manifest_sha = Sha256Digest::parse(&source.manifest_sha256).map_err(model_error)?;
        let manifest = read_verified(
            "detector manifest",
            &source.manifest_path,
            &manifest_sha,
            MAX_MANIFEST_BYTES,
        )
        .map_err(model_error)?;
        let text = std::str::from_utf8(&manifest)
            .map_err(|_| port("the detector manifest is not UTF-8"))?;
        let dir = source.manifest_path.parent().unwrap_or(Path::new("."));
        let spec = DetectorSpec::from_manifest(text, dir).map_err(model_error)?;

        let runtime_sha = Sha256Digest::parse(&source.runtime_sha256).map_err(model_error)?;
        let runtime = std::path::absolute(&source.runtime_path)
            .map_err(|e| port("cannot resolve the ONNX Runtime path").with_source(e))?;
        read_verified(
            "ONNX Runtime library",
            &runtime,
            &runtime_sha,
            MAX_RUNTIME_BYTES,
        )
        .map_err(model_error)?;
        // A second call in the same process keeps the library already loaded.
        let _ = ort::init_from(&runtime)
            .map_err(|e| port("cannot load the ONNX Runtime library").with_source(e))?
            .commit();

        let weights = read_verified(
            "detector weights",
            &spec.path,
            &spec.sha256,
            MAX_MODEL_BYTES,
        )
        .map_err(model_error)?;
        let ort_error = |what: &'static str| move |e: ort::Error<_>| port(format!("{what}: {e}"));
        let session = Session::builder()
            .map_err(|e| port(format!("cannot create an ONNX Runtime session: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_error("cannot configure the session"))?
            .with_intra_threads(1)
            .map_err(ort_error("cannot configure the session"))?
            .with_deterministic_compute(true)
            .map_err(ort_error("cannot configure the session"))?
            .commit_from_memory(&weights)
            .map_err(|e| port(format!("cannot load the detector weights: {e}")))?;
        let [input] = session.inputs() else {
            return Err(port(format!(
                "the detector must have exactly one input, it has {}",
                session.inputs().len()
            )));
        };
        let input_name = input.name().to_owned();
        if session.outputs().is_empty() {
            return Err(port("the detector has no output"));
        }
        Ok(Self {
            spec,
            session,
            input_name,
        })
    }

    /// The validated manifest entry.
    pub fn spec(&self) -> &DetectorSpec {
        &self.spec
    }

    fn detect_region(
        &mut self,
        frame: &BgraFrame,
        region: &BBox,
    ) -> Result<Vec<Control>, PortError> {
        let Some(rect) = PixelRect::cover(region, frame.width(), frame.height()) else {
            return Ok(Vec::new());
        };
        let Some(clip) = region.clip(&frame.bounds()) else {
            return Ok(Vec::new());
        };
        let spec = &self.spec;
        let letterbox =
            Letterbox::fit(rect.width, rect.height, spec.input_width, spec.input_height)
                .ok_or_else(|| port("empty crop"))?;
        let input = letterbox.tensor(&crop_bgra(frame, rect));
        let shape = [1, 3, spec.input_height as usize, spec.input_width as usize];
        let tensor = Tensor::from_array((shape, input))
            .map_err(|e| port(format!("cannot build the input tensor: {e}")))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|e| port(format!("inference failed: {e}")))?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| port(format!("the detector output is not an f32 tensor: {e}")))?;
        let dims: Vec<i64> = shape.iter().copied().collect();
        let candidates = decode(
            &dims,
            data,
            spec.output_layout,
            spec.classes.len(),
            spec.score_threshold,
        )
        .map_err(model_error)?;
        let kept = nms(candidates, spec.nms_iou);
        Ok(controls_from_candidates(
            &kept,
            &letterbox,
            (f64::from(rect.x), f64::from(rect.y)),
            &clip,
            &spec.classes,
        ))
    }
}

impl VisionDetector for OnnxVisionDetector {
    fn name(&self) -> &'static str {
        NAME
    }

    fn models_loaded(&self) -> bool {
        true
    }

    /// Detections in each region, sorted by box then role, without exact duplicates.
    fn detect(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let mut controls = Vec::new();
        for region in regions {
            controls.extend(self.detect_region(frame, region)?);
        }
        controls.sort_by(|a, b| {
            a.bbox
                .cmp_corners(&b.bbox)
                .then(a.role.cmp(&b.role))
                .then(a.affordance.actionability.cmp(&b.affordance.actionability))
        });
        controls.dedup();
        Ok(controls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(dir: &Path) -> OnnxDetectorSource {
        OnnxDetectorSource {
            manifest_path: dir.join("manifest.json"),
            manifest_sha256: Sha256Digest::of(b"{}").to_hex(),
            runtime_path: dir.join("onnxruntime.dll"),
            runtime_sha256: Sha256Digest::of(b"").to_hex(),
        }
    }

    fn error(source: &OnnxDetectorSource) -> String {
        let err = OnnxVisionDetector::load(source).unwrap_err();
        ommatix_application::error_chain(&err)
    }

    #[test]
    fn nothing_loads_without_a_verified_manifest() {
        let dir = std::env::temp_dir().join(format!("ommatix-onnx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut src = source(&dir);
        assert!(error(&src).contains("cannot read"));

        std::fs::write(&src.manifest_path, b"{\"tampered\": true}").unwrap();
        assert!(error(&src).contains("SHA-256 mismatch"));

        src.manifest_sha256 = "not a digest".into();
        assert!(error(&src).contains("invalid SHA-256"));

        let template = include_str!("../../../../docs/thesis/plantillas/model_manifest.json");
        std::fs::write(&src.manifest_path, template).unwrap();
        src.manifest_sha256 = Sha256Digest::of(template.as_bytes()).to_hex();
        assert!(error(&src).contains("no model has been selected"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
