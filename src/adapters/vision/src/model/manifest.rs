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
//! - `output_layout` names one of the [`OutputLayout`]s; any other is rejected.
//! - `class_mapping` lists the model's classes in output order. A class's `role` is one of
//!   the contract's roles and defaults to `unknown`: no role is inferred from a class name.
//!   `affordance` defaults to `unknown`; `"interactive"` confirms actionability only with a
//!   non-empty `affordance_evidence` (where the S0 report shows it), and is refused without.
//! - `execution_provider` must be `"cpu"`, the only provider this build runs.

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
    pub input_width: u32,
    pub input_height: u32,
    pub output_layout: OutputLayout,
    pub classes: Vec<ClassSpec>,
    pub score_threshold: f32,
    pub nms_iou: f32,
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
        let Some(output_layout) = OutputLayout::parse(&raw.output_layout) else {
            return Err(ModelError::InvalidManifest(format!(
                "unsupported output_layout {:?}; supported: {}",
                raw.output_layout.chars().take(64).collect::<String>(),
                OutputLayout::ALL.map(OutputLayout::name).join(", ")
            )));
        };
        if raw.execution_provider != "cpu" {
            return invalid("detector execution_provider must be \"cpu\" in this build");
        }
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
            output_layout,
            classes,
            score_threshold: raw.score_threshold,
            nms_iou: raw.nms_iou,
        })
    }
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
                json!("directml"),
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
