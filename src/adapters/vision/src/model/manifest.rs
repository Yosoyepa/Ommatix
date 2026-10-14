//! The detector entry of a model manifest shaped like the dossier template
//! `docs/thesis/plantillas/model_manifest.json`, parsed and validated fail-closed.
//!
//! ```json
//! {
//!   "status": "SELECCIONADO",
//!   "artifacts": [{
//!     "name": "…", "role": "detector", "path": "detector.onnx",
//!     "source_url": "…", "revision": "…", "sha256": "<64 hex>", "license": "…",
//!     "export_command": "…", "opset": 17,
//!     "input_shape": [1, 3, 640, 640],
//!     "output_layout": "yolo_cxcywh_scores_cn",
//!     "class_mapping": [
//!       { "name": "button", "role": "button" },
//!       { "name": "link", "affordance": "interactive", "affordance_evidence": "S0 report §3" }
//!     ],
//!     "execution_provider": "cpu", "test_report": "…",
//!     "score_threshold": 0.25, "nms_iou": 0.45
//!   }]
//! }
//! ```
//!
//! - `status` must be `"SELECCIONADO"`: the template's `"CANDIDATOS_NO_SELECCIONADOS"`
//!   (or anything else) means no model has been chosen, and nothing loads.
//! - Exactly one artifact has `"role": "detector"`; the others (OCR) are not read here.
//!   Every template field is required and non-empty; unknown fields are rejected.
//! - `path` is relative to the manifest's directory (or absolute).
//! - `input_shape` is `[1, 3, H, W]` (NCHW, RGB, values in `[0, 1]`).
//! - `input_shapes` (optional, with `input_dimensions`) lists every fixed `[1, 3, H, W]`
//!   the engine may feed a model that exports free height and width: the smallest shape
//!   that holds a region at its own resolution is used, because the cost of an inference
//!   follows the input's area, not the region's. `input_dimensions` names the model's two
//!   free axes, `["height", "width"]`; the detector pins each shape to its own session with
//!   them. The list holds `input_shape`, which must be the largest (the shape of a full
//!   refresh and of any region too big for the others); its sides are multiples of 32.
//!   A model without free axes declares neither field.
//! - `output_layout` names one of the [`OutputLayout`]s; any other is rejected.
//! - `class_mapping` lists the model's classes in output order. A class's `role` is one of
//!   the contract's roles and defaults to `unknown`: no role is inferred from a class name.
//!   `affordance` defaults to `unknown`; `"interactive"` confirms actionability only with a
//!   non-empty `affordance_evidence` (where the S0 report shows it), and is refused without.
//! - `execution_provider` lists the ONNX Runtime execution providers the S0 report
//!   validated the model on, comma-separated: `"cpu"`, `"directml"` or `"cpu,directml"`.
//!   The operator's configuration picks one of them; a provider the manifest does not list
//!   is refused when the detector loads.

use std::path::{Path, PathBuf};

use ommatix_domain::fusion::{Actionability, Role};
use serde::Deserialize;

use super::ModelError;
use super::decode::OutputLayout;
use super::integrity::Sha256Digest;

/// The manifest status that marks a model as chosen.
pub const SELECTED_STATUS: &str = "SELECCIONADO";

/// Largest manifest read, in bytes.
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Bounds on the model's input side, in pixels.
pub const INPUT_SIDE_RANGE: std::ops::RangeInclusive<u32> = 32..=4096;

/// Most classes a detector may declare.
pub const MAX_CLASSES: usize = 1024;

/// An ONNX Runtime execution provider the detector can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExecutionProvider {
    /// `CPUExecutionProvider`, the baseline.
    Cpu,
    /// `DmlExecutionProvider` (DirectML on DirectX 12, Windows).
    DirectMl,
}

impl ExecutionProvider {
    /// Every provider, in manifest order.
    pub const ALL: [Self; 2] = [Self::Cpu, Self::DirectMl];

    /// The manifest and configuration name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::DirectMl => "directml",
        }
    }

    /// Parses a manifest or configuration name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// One class of the detector, as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSpec {
    pub name: String,
    pub role: Role,
    pub actionability: Actionability,
}

/// A validated detector entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectorSpec {
    pub name: String,
    pub revision: String,
    pub license: String,
    /// The weights, resolved against the manifest's directory.
    pub path: PathBuf,
    pub sha256: Sha256Digest,
    pub opset: u32,
    /// The reference input, `input_shape`: the largest of [`input_sizes`](Self::input_sizes).
    pub input_width: u32,
    pub input_height: u32,
    /// Every `(width, height)` the detector may feed the model, smallest area first
    /// (`input_shapes`); only the reference input when the model is fixed.
    pub input_sizes: Vec<(u32, u32)>,
    /// The model's free `(height, width)` axis names (`input_dimensions`); `None` for a model
    /// exported with a fixed input.
    pub input_dimensions: Option<(String, String)>,
    pub output_layout: OutputLayout,
    pub classes: Vec<ClassSpec>,
    pub score_threshold: f32,
    pub nms_iou: f32,
    /// The providers the model was validated on (sorted, without duplicates).
    pub execution_providers: Vec<ExecutionProvider>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    status: String,
    artifacts: Vec<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)] // the template's documentation of itself; not used at run time
    required_fields_per_artifact: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDetector {
    name: String,
    role: String,
    path: String,
    source_url: String,
    revision: String,
    sha256: String,
    license: String,
    export_command: String,
    opset: u32,
    input_shape: Vec<u32>,
    #[serde(default)]
    input_shapes: Option<Vec<Vec<u32>>>,
    #[serde(default)]
    input_dimensions: Option<Vec<String>>,
    output_layout: String,
    class_mapping: Vec<RawClass>,
    execution_provider: String,
    test_report: String,
    score_threshold: f32,
    nms_iou: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClass {
    name: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    affordance: Option<String>,
    #[serde(default)]
    affordance_evidence: Option<String>,
}

/// Parses the role names of the contract schema.
pub fn parse_role(name: &str) -> Option<Role> {
    Some(match name {
        "button" => Role::Button,
        "input" => Role::Input,
        "checkbox" => Role::Checkbox,
        "dropdown" => Role::Dropdown,
        "icon_button" => Role::IconButton,
        "text_node" => Role::TextNode,
        "unknown" => Role::Unknown,
        _ => return None,
    })
}

impl DetectorSpec {
    /// Parses and validates the detector entry of a manifest; `manifest_dir` resolves a
    /// relative `path`.
    pub fn from_manifest(text: &str, manifest_dir: &Path) -> Result<Self, ModelError> {
        let invalid = |m: String| ModelError::InvalidManifest(m);
        let raw: RawManifest =
            serde_json::from_str(text).map_err(|e| invalid(format!("not a manifest: {e}")))?;
        if raw.status != SELECTED_STATUS {
            return Err(invalid(format!(
                "status is {:?}: no model has been selected (expected {SELECTED_STATUS:?})",
                raw.status.chars().take(64).collect::<String>()
            )));
        }
        let detectors: Vec<&serde_json::Value> = raw
            .artifacts
            .iter()
            .filter(|a| a.get("role").and_then(serde_json::Value::as_str) == Some("detector"))
            .collect();
        let [detector] = detectors.as_slice() else {
            return Err(invalid(format!(
                "expected exactly one artifact with role \"detector\", found {}",
                detectors.len()
            )));
        };
        let raw = RawDetector::deserialize(*detector)
            .map_err(|e| invalid(format!("detector artifact: {e}")))?;
        Self::validate(raw, manifest_dir)
    }

    fn validate(raw: RawDetector, manifest_dir: &Path) -> Result<Self, ModelError> {
        let invalid = |m: &str| Err(ModelError::InvalidManifest(m.to_owned()));
        for (field, value) in [
            ("name", &raw.name),
            ("role", &raw.role),
            ("path", &raw.path),
            ("source_url", &raw.source_url),
            ("revision", &raw.revision),
            ("license", &raw.license),
            ("export_command", &raw.export_command),
            ("test_report", &raw.test_report),
        ] {
            if value.trim().is_empty() {
                return Err(ModelError::InvalidManifest(format!(
                    "detector field {field} must not be empty"
                )));
            }
        }
        let sha256 = Sha256Digest::parse(&raw.sha256)?;
        if !(7..=30).contains(&raw.opset) {
            return invalid("detector opset must be between 7 and 30");
        }
        let [1, 3, input_height, input_width] = raw.input_shape[..] else {
            return invalid("detector input_shape must be [1, 3, H, W] (NCHW, RGB)");
        };
        if !INPUT_SIDE_RANGE.contains(&input_width) || !INPUT_SIDE_RANGE.contains(&input_height) {
            return invalid("detector input sides must be between 32 and 4096 pixels");
        }
        let (input_sizes, input_dimensions) = input_sizes(
            (input_width, input_height),
            raw.input_shapes.as_deref(),
            raw.input_dimensions.as_deref(),
        )?;
        let Some(output_layout) = OutputLayout::parse(&raw.output_layout) else {
            return Err(ModelError::InvalidManifest(format!(
                "unsupported output_layout {:?}; supported: {}",
                raw.output_layout.chars().take(64).collect::<String>(),
                OutputLayout::ALL.map(OutputLayout::name).join(", ")
            )));
        };
        let execution_providers = parse_providers(&raw.execution_provider).ok_or_else(|| {
            ModelError::InvalidManifest(
                "detector execution_provider must list \"cpu\" and/or \"directml\", \
                 comma-separated, each once"
                    .to_owned(),
            )
        })?;
        if !(raw.score_threshold > 0.0 && raw.score_threshold <= 1.0) {
            return invalid("detector score_threshold must be in (0, 1]");
        }
        if !(raw.nms_iou > 0.0 && raw.nms_iou <= 1.0) {
            return invalid("detector nms_iou must be in (0, 1]");
        }
        if raw.class_mapping.is_empty() || raw.class_mapping.len() > MAX_CLASSES {
            return invalid("detector class_mapping must list between 1 and 1024 classes");
        }
        let classes = raw
            .class_mapping
            .into_iter()
            .enumerate()
            .map(|(i, class)| class_spec(i, class))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: raw.name,
            revision: raw.revision,
            license: raw.license,
            path: manifest_dir.join(raw.path),
            sha256,
            opset: raw.opset,
            input_width,
            input_height,
            input_sizes,
            input_dimensions,
            output_layout,
            classes,
            score_threshold: raw.score_threshold,
            nms_iou: raw.nms_iou,
            execution_providers,
        })
    }
}

/// Most input shapes a detector may declare: each one is a session in memory.
pub const MAX_INPUT_SHAPES: usize = 16;

/// The multiple every side of a declared shape must be: the coarsest stride of a YOLO-style
/// detector.
pub const INPUT_SIDE_MULTIPLE: u32 = 32;

type InputSizes = (Vec<(u32, u32)>, Option<(String, String)>);

/// Validates `input_shapes` and `input_dimensions` against the reference `(width, height)`.
fn input_sizes(
    reference: (u32, u32),
    shapes: Option<&[Vec<u32>]>,
    dimensions: Option<&[String]>,
) -> Result<InputSizes, ModelError> {
    let invalid = |m: &str| Err(ModelError::InvalidManifest(m.to_owned()));
    let (Some(shapes), Some(dimensions)) = (shapes, dimensions) else {
        if shapes.is_some() || dimensions.is_some() {
            return invalid("detector input_shapes and input_dimensions are declared together");
        }
        return Ok((vec![reference], None));
    };
    let [height_axis, width_axis] = dimensions else {
        return invalid("detector input_dimensions must name two axes: [\"height\", \"width\"]");
    };
    if [height_axis, width_axis]
        .iter()
        .any(|a| a.trim().is_empty() || a.len() > 64)
        || height_axis == width_axis
    {
        return invalid("detector input_dimensions must be two different, non-empty names");
    }
    if shapes.is_empty() || shapes.len() > MAX_INPUT_SHAPES {
        return invalid("detector input_shapes must list between 1 and 16 shapes");
    }
    let mut sizes = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let [1, 3, height, width] = shape[..] else {
            return invalid("detector input_shapes must be [1, 3, H, W] each");
        };
        if !INPUT_SIDE_RANGE.contains(&width)
            || !INPUT_SIDE_RANGE.contains(&height)
            || width % INPUT_SIDE_MULTIPLE != 0
            || height % INPUT_SIDE_MULTIPLE != 0
        {
            return invalid(
                "detector input_shapes sides must be multiples of 32 between 32 and 4096",
            );
        }
        sizes.push((width, height));
    }
    // Smallest area first; equal areas by width, so that the order is the same everywhere.
    sizes.sort_unstable_by_key(|&(w, h)| (u64::from(w) * u64::from(h), w, h));
    let count = sizes.len();
    sizes.dedup();
    if sizes.len() != count {
        return invalid("detector input_shapes must not repeat a shape");
    }
    if sizes.last() != Some(&reference) {
        return invalid(
            "detector input_shapes must include input_shape, and it must be the largest",
        );
    }
    Ok((sizes, Some((height_axis.clone(), width_axis.clone()))))
}

/// `"cpu"`, `"directml"` or both, comma-separated; `None` on anything else or a duplicate.
fn parse_providers(list: &str) -> Option<Vec<ExecutionProvider>> {
    let mut out = list
        .split(',')
        .map(|name| ExecutionProvider::parse(name.trim()))
        .collect::<Option<Vec<_>>>()?;
    let count = out.len();
    out.sort_unstable();
    out.dedup();
    (out.len() == count).then_some(out)
}

fn class_spec(index: usize, raw: RawClass) -> Result<ClassSpec, ModelError> {
    let invalid = |m: &str| ModelError::InvalidManifest(format!("class {index}: {m}"));
    if raw.name.trim().is_empty() {
        return Err(invalid("name must not be empty"));
    }
    let role = match raw.role.as_deref() {
        None => Role::Unknown,
        Some(name) => parse_role(name).ok_or_else(|| invalid("unknown role"))?,
    };
    let evidence = raw
        .affordance_evidence
        .as_deref()
        .is_some_and(|e| !e.trim().is_empty());
    let actionability = match raw.affordance.as_deref() {
        None | Some("unknown") => Actionability::Unknown,
        Some("interactive") if evidence => Actionability::Confirmed,
        Some("interactive") => {
            return Err(invalid(
                "affordance \"interactive\" requires a non-empty affordance_evidence",
            ));
        }
        Some(_) => return Err(invalid("affordance must be \"unknown\" or \"interactive\"")),
    };
    Ok(ClassSpec {
        name: raw.name,
        role,
        actionability,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn manifest() -> Value {
        json!({
            "status": "SELECCIONADO",
            "artifacts": [
                {
                    "name": "synthetic-detector", "role": "detector", "path": "det.onnx",
                    "source_url": "https://example.invalid/det", "revision": "r1",
                    "sha256": DIGEST, "license": "Apache-2.0",
                    "export_command": "export --opset 17", "opset": 17,
                    "input_shape": [1, 3, 320, 640],
                    "output_layout": "yolo_cxcywh_scores_cn",
                    "class_mapping": [
                        { "name": "button", "role": "button" },
                        { "name": "thing" },
                        { "name": "link", "affordance": "interactive",
                          "affordance_evidence": "S0 report, table 3" }
                    ],
                    "execution_provider": "cpu", "test_report": "reports/s0.md",
                    "score_threshold": 0.25, "nms_iou": 0.45
                },
                { "role": "ocr", "name": "anything else" }
            ]
        })
    }

    fn parse(value: &Value) -> Result<DetectorSpec, ModelError> {
        DetectorSpec::from_manifest(&value.to_string(), Path::new("models"))
    }

    fn with(edit: impl FnOnce(&mut Value)) -> String {
        let mut m = manifest();
        edit(&mut m);
        parse(&m).unwrap_err().to_string()
    }

    #[test]
    fn a_complete_manifest_yields_the_detector() {
        let spec = parse(&manifest()).unwrap();
        assert_eq!(spec.path, Path::new("models").join("det.onnx"));
        assert_eq!((spec.input_width, spec.input_height), (640, 320));
        assert_eq!(spec.output_layout, OutputLayout::ChannelsFirst);
        assert_eq!(spec.sha256.to_hex(), DIGEST);
        let roles: Vec<_> = spec
            .classes
            .iter()
            .map(|c| (c.role, c.actionability))
            .collect();
        assert_eq!(
            roles,
            [
                (Role::Button, Actionability::Unknown),
                (Role::Unknown, Actionability::Unknown),
                (Role::Unknown, Actionability::Confirmed),
            ]
        );
    }

    /// A manifest whose model exports free height and width.
    fn shaped(shapes: &Value, dimensions: &Value) -> Value {
        let mut m = manifest();
        m["artifacts"][0]["input_shape"] = json!([1, 3, 320, 640]);
        m["artifacts"][0]["input_shapes"] = shapes.clone();
        m["artifacts"][0]["input_dimensions"] = dimensions.clone();
        m
    }

    #[test]
    fn a_fixed_model_has_one_input_size() {
        let spec = parse(&manifest()).unwrap();
        assert_eq!(spec.input_sizes, [(640, 320)]);
        assert_eq!(spec.input_dimensions, None);
    }

    #[test]
    fn a_model_with_free_axes_declares_its_input_sizes_smallest_first() {
        let m = shaped(
            &json!([
                [1, 3, 320, 640],
                [1, 3, 64, 64],
                [1, 3, 160, 320],
                [1, 3, 320, 320]
            ]),
            &json!(["height", "width"]),
        );
        let spec = parse(&m).unwrap();
        assert_eq!(
            spec.input_sizes,
            [(64, 64), (320, 160), (320, 320), (640, 320)]
        );
        assert_eq!((spec.input_width, spec.input_height), (640, 320));
        assert_eq!(
            spec.input_dimensions,
            Some(("height".to_owned(), "width".to_owned()))
        );
    }

    #[test]
    fn input_sizes_are_refused_unless_they_are_whole_and_led_by_the_reference() {
        let axes = json!(["height", "width"]);
        let ok = json!([[1, 3, 160, 320], [1, 3, 320, 640]]);
        assert!(parse(&shaped(&ok, &axes)).is_ok());
        for (message, shapes, dimensions) in [
            ("declared together", ok.clone(), Value::Null),
            ("two axes", ok.clone(), json!(["height"])),
            ("different", ok.clone(), json!(["size", "size"])),
            ("non-empty", ok.clone(), json!(["height", " "])),
            ("between 1 and 16", json!([]), axes.clone()),
            (
                "[1, 3, H, W]",
                json!([[1, 3, 320, 640], [3, 160, 320]]),
                axes.clone(),
            ),
            (
                "multiples of 32",
                json!([[1, 3, 330, 320], [1, 3, 320, 640]]),
                axes.clone(),
            ),
            (
                "multiples of 32",
                json!([[1, 3, 0, 320], [1, 3, 320, 640]]),
                axes.clone(),
            ),
            (
                "repeat",
                json!([[1, 3, 320, 640], [1, 3, 320, 640]]),
                axes.clone(),
            ),
            ("largest", json!([[1, 3, 160, 320]]), axes.clone()),
            (
                "largest",
                json!([[1, 3, 320, 640], [1, 3, 640, 640]]),
                axes.clone(),
            ),
        ] {
            let mut m = shaped(&shapes, &dimensions);
            if dimensions.is_null() {
                m["artifacts"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("input_dimensions");
            }
            let err = parse(&m).unwrap_err().to_string();
            assert!(err.contains(message), "{message}: {err}");
        }
    }

    #[test]
    fn the_dossier_template_selects_nothing() {
        let template = include_str!("../../../../../docs/thesis/plantillas/model_manifest.json");
        let err = DetectorSpec::from_manifest(template, Path::new(".")).unwrap_err();
        assert!(
            err.to_string().contains("no model has been selected"),
            "{err}"
        );
    }

    #[test]
    fn the_s0_manifest_selects_one_class_without_a_role() {
        let s0 = include_str!("../../../../../docs/thesis/plantillas/model_manifest.s0.json");
        let spec = DetectorSpec::from_manifest(s0, Path::new("models")).unwrap();
        assert_eq!(
            spec.path,
            Path::new("models").join("omniparser-icon-detect-v3/icon_detect_v3.onnx")
        );
        assert_eq!((spec.input_width, spec.input_height), (1280, 1280));
        assert_eq!(spec.output_layout, OutputLayout::ChannelsFirst);
        assert_eq!(spec.opset, 17);
        // One interactable-region class: no role is claimed, only the affordance the
        // model card states, with that statement as evidence.
        let classes: Vec<_> = spec
            .classes
            .iter()
            .map(|c| (c.role, c.actionability))
            .collect();
        assert_eq!(classes, [(Role::Unknown, Actionability::Confirmed)]);
    }

    /// RNF-05: the float16 export is a different artefact with its own pin, and only
    /// DirectML was validated on it (ONNX Runtime's CPU provider has no float16 convolution).
    #[test]
    fn the_rnf05_manifest_selects_the_float16_export_for_directml_only() {
        let text = include_str!("../../../../../docs/thesis/plantillas/model_manifest.rnf05.json");
        let spec = DetectorSpec::from_manifest(text, Path::new("models")).unwrap();
        assert_eq!(
            spec.path,
            Path::new("models").join("omniparser-icon-detect-v3/icon_detect_v3_fp16.onnx")
        );
        assert_eq!(
            spec.sha256.to_hex(),
            "d7a8901db0ae650616ee43f53e6ee8f57a829de54ab8daae431be77556cc0805"
        );
        assert_eq!((spec.input_width, spec.input_height), (1280, 1280));
        assert_eq!(spec.input_sizes, [(1280, 1280)]);
        assert_eq!(spec.execution_providers, [ExecutionProvider::DirectMl]);
        // The same single class, role and affordance as the float32 export.
        let s0 = include_str!("../../../../../docs/thesis/plantillas/model_manifest.s0.json");
        let fp32 = DetectorSpec::from_manifest(s0, Path::new("models")).unwrap();
        assert_eq!(spec.classes, fp32.classes);
        assert_eq!(
            (spec.score_threshold, spec.nms_iou, spec.output_layout),
            (fp32.score_threshold, fp32.nms_iou, fp32.output_layout)
        );
    }

    /// The operator's configuration pins the manifest by hash: it must be the hash of the
    /// manifest this repository ships.
    #[test]
    fn the_rnf05_configuration_pins_the_rnf05_manifest() {
        let manifest =
            include_str!("../../../../../docs/thesis/plantillas/model_manifest.rnf05.json");
        let config = include_str!("../../../../../tools/s0/ommatix.rnf05.directml.toml");
        let pin = format!(
            "detector_manifest_sha256 = \"{}\"",
            Sha256Digest::of(manifest.as_bytes()).to_hex()
        );
        assert!(config.contains(&pin), "{pin}");
    }

    #[test]
    fn incomplete_or_unsupported_manifests_are_refused() {
        for (message, pointer, value) in [
            ("selected", "/status", json!("CANDIDATOS_NO_SELECCIONADOS")),
            ("exactly one", "/artifacts/0/role", json!("ocr")),
            ("license", "/artifacts/0/license", json!(" ")),
            ("invalid type", "/artifacts/0/sha256", json!(null)),
            ("digest", "/artifacts/0/sha256", json!("abc")),
            ("opset", "/artifacts/0/opset", json!(3)),
            ("NCHW", "/artifacts/0/input_shape", json!([1, 640, 640, 3])),
            ("4096", "/artifacts/0/input_shape", json!([1, 3, 8192, 640])),
            ("output_layout", "/artifacts/0/output_layout", json!("detr")),
            (
                "execution_provider",
                "/artifacts/0/execution_provider",
                json!("cuda"),
            ),
            (
                "execution_provider",
                "/artifacts/0/execution_provider",
                json!("cpu,cpu"),
            ),
            (
                "execution_provider",
                "/artifacts/0/execution_provider",
                json!(""),
            ),
            (
                "score_threshold",
                "/artifacts/0/score_threshold",
                json!(0.0),
            ),
            ("nms_iou", "/artifacts/0/nms_iou", json!(1.5)),
            ("class_mapping", "/artifacts/0/class_mapping", json!([])),
            (
                "unknown role",
                "/artifacts/0/class_mapping/1/role",
                json!("link"),
            ),
            (
                "affordance_evidence",
                "/artifacts/0/class_mapping/2/affordance_evidence",
                json!(""),
            ),
            (
                "affordance must",
                "/artifacts/0/class_mapping/1/affordance",
                json!("clickable"),
            ),
            (
                "name must not be empty",
                "/artifacts/0/class_mapping/1/name",
                json!(""),
            ),
        ] {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let error = with(|m| m.pointer_mut(parent).unwrap()[key] = value);
            assert!(
                error.contains(message),
                "{pointer}: expected {message:?} in {error:?}"
            );
        }
        let duplicated = with(|m| {
            let detector = m["artifacts"][0].clone();
            m["artifacts"].as_array_mut().unwrap().push(detector);
        });
        assert!(duplicated.contains("exactly one"), "{duplicated}");
        let missing = with(|m| {
            m["artifacts"][0]
                .as_object_mut()
                .unwrap()
                .remove("test_report");
        });
        assert!(missing.contains("missing field"), "{missing}");
        let unknown = with(|m| m["artifacts"][0]["trust_me"] = json!(true));
        assert!(unknown.contains("unknown field"), "{unknown}");
        assert!(DetectorSpec::from_manifest("not json", Path::new(".")).is_err());
    }

    #[test]
    fn the_manifest_lists_its_validated_execution_providers() {
        let providers = |value: &str| {
            let mut m = manifest();
            m["artifacts"][0]["execution_provider"] = json!(value);
            parse(&m).unwrap().execution_providers
        };
        assert_eq!(providers("cpu"), [ExecutionProvider::Cpu]);
        assert_eq!(providers("directml"), [ExecutionProvider::DirectMl]);
        assert_eq!(
            providers("directml, cpu"),
            [ExecutionProvider::Cpu, ExecutionProvider::DirectMl]
        );
    }

    #[test]
    fn contract_role_names_parse() {
        for (name, role) in [
            ("button", Role::Button),
            ("input", Role::Input),
            ("checkbox", Role::Checkbox),
            ("dropdown", Role::Dropdown),
            ("icon_button", Role::IconButton),
            ("text_node", Role::TextNode),
            ("unknown", Role::Unknown),
        ] {
            assert_eq!(parse_role(name), Some(role));
        }
        assert_eq!(parse_role("Button"), None);
    }
}
