//! The supply-chain gate of the ONNX detector (thesis chapter 08 §8.2, SEC-06), without
//! ONNX Runtime: every file the detector would load is read once, bounded, and checked
//! against the digest the operator pinned, **before** any library is mapped into the
//! process or any weight reaches a session. A tampered manifest, runtime library or weights
//! file is refused here, in safe Rust, so the whole gate is tested with temporary files and
//! no model.
//!
//! The order is the trust chain: the manifest against the operator's pin, its detector
//! entry (including the execution providers it validated), the pinned `DirectML.dll` when
//! that provider is chosen, the ONNX Runtime library, and the weights against the digest
//! the manifest declares. There is no fallback model, provider or library.

use std::path::{Path, PathBuf};

use ommatix_application::PortError;

use super::ModelError;
use super::integrity::{Sha256Digest, read_verified};
use super::manifest::{DetectorSpec, ExecutionProvider, MAX_MANIFEST_BYTES};

const NAME: &str = "onnx";

/// Largest weights file loaded, in bytes.
pub const MAX_MODEL_BYTES: u64 = 1024 * 1024 * 1024;

/// Largest ONNX Runtime (or DirectML) library hashed, in bytes.
pub const MAX_RUNTIME_BYTES: u64 = 512 * 1024 * 1024;

/// Most intra-op threads the detector accepts.
pub const MAX_INTRA_THREADS: usize = 64;

/// Where the detector comes from, each file pinned by the operator, and how it runs.
#[derive(Debug, Clone)]
pub struct OnnxDetectorSource {
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
    pub runtime_path: PathBuf,
    pub runtime_sha256: String,
    /// The provider to run on; the manifest must list it.
    pub execution_provider: ExecutionProvider,
    /// `DirectML.dll` and its SHA-256: required with DirectML, refused with the CPU.
    pub directml: Option<(PathBuf, String)>,
    /// Intra-op threads, `1..=MAX_INTRA_THREADS`; `0` lets ONNX Runtime choose (its
    /// default is the number of physical cores).
    pub intra_threads: usize,
}

/// The detector's files after every check passed: the manifest's detector entry, the
/// libraries by absolute path (not yet loaded) and the weights, the very bytes that were
/// hashed.
#[derive(Debug)]
pub struct VerifiedDetector {
    pub spec: DetectorSpec,
    /// The pinned `DirectML.dll`, when the provider is DirectML.
    pub directml: Option<PathBuf>,
    pub runtime: PathBuf,
    pub weights: Vec<u8>,
}

fn port(message: impl Into<String>) -> PortError {
    PortError::new(NAME, message)
}

/// A refused artefact, as the port reports it.
pub fn model_error(e: ModelError) -> PortError {
    port("model refused").with_source(e)
}

/// Checks a library against its pin and returns its absolute path.
fn verified_library(what: &str, path: &Path, sha256: &str) -> Result<PathBuf, PortError> {
    let digest = Sha256Digest::parse(sha256).map_err(model_error)?;
    let path = std::path::absolute(path)
        .map_err(|e| port(format!("cannot resolve the {what} path")).with_source(e))?;
    read_verified(what, &path, &digest, MAX_RUNTIME_BYTES).map_err(model_error)?;
    Ok(path)
}

/// Verifies every file of `source` (see the module documentation). Nothing is loaded.
pub fn verify(source: &OnnxDetectorSource) -> Result<VerifiedDetector, PortError> {
    let manifest_sha = Sha256Digest::parse(&source.manifest_sha256).map_err(model_error)?;
    let manifest = read_verified(
        "detector manifest",
        &source.manifest_path,
        &manifest_sha,
        MAX_MANIFEST_BYTES,
    )
    .map_err(model_error)?;
    let text =
        std::str::from_utf8(&manifest).map_err(|_| port("the detector manifest is not UTF-8"))?;
    let dir = source.manifest_path.parent().unwrap_or(Path::new("."));
    let spec = DetectorSpec::from_manifest(text, dir).map_err(model_error)?;

    let provider = source.execution_provider;
    if !spec.execution_providers.contains(&provider) {
        return Err(port(format!(
            "the model manifest does not list the {} execution provider as validated \
             (it lists {})",
            provider.name(),
            spec.execution_providers
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let directml = match (provider, &source.directml) {
        (ExecutionProvider::DirectMl, Some((path, sha256))) => {
            Some(verified_library("DirectML library", path, sha256)?)
        }
        (ExecutionProvider::DirectMl, None) => {
            return Err(port(
                "the DirectML execution provider needs a pinned DirectML.dll",
            ));
        }
        (ExecutionProvider::Cpu, Some(_)) => {
            return Err(port("a DirectML library is pinned but the provider is cpu"));
        }
        (ExecutionProvider::Cpu, None) => None,
    };
    let runtime = verified_library(
        "ONNX Runtime library",
        &source.runtime_path,
        &source.runtime_sha256,
    )?;
    let weights = read_verified(
        "detector weights",
        &spec.path,
        &spec.sha256,
        MAX_MODEL_BYTES,
    )
    .map_err(model_error)?;
    Ok(VerifiedDetector {
        spec,
        directml,
        runtime,
        weights,
    })
}

#[cfg(test)]
mod tests {
    use ommatix_application::error_chain;
    use serde_json::{Value, json};

    use super::*;

    const S0_MANIFEST: &str =
        include_str!("../../../../../docs/thesis/plantillas/model_manifest.s0.json");

    /// A temporary directory holding a manifest, a runtime library and weights that are
    /// fake files: nothing here is a model or a library.
    struct Kit {
        dir: PathBuf,
        source: OnnxDetectorSource,
    }

    impl Kit {
        const WEIGHTS: &'static [u8] = b"fake weights, not a model";
        const RUNTIME: &'static [u8] = b"fake runtime, not a library";

        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("ommatix-artifacts-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let kit = Self {
                source: OnnxDetectorSource {
                    manifest_path: dir.join("manifest.json"),
                    manifest_sha256: String::new(),
                    runtime_path: dir.join("onnxruntime.dll"),
                    runtime_sha256: Sha256Digest::of(Self::RUNTIME).to_hex(),
                    execution_provider: ExecutionProvider::Cpu,
                    directml: None,
                    intra_threads: 1,
                },
                dir,
            };
            std::fs::write(&kit.source.runtime_path, Self::RUNTIME).unwrap();
            std::fs::write(kit.dir.join("weights.onnx"), Self::WEIGHTS).unwrap();
            kit
        }

        /// A selected manifest for the fake weights, pinned by the source.
        fn with_manifest(mut self, edit: impl FnOnce(&mut Value)) -> Self {
            let mut manifest: Value = serde_json::from_str(S0_MANIFEST).unwrap();
            let detector = &mut manifest["artifacts"][0];
            detector["path"] = json!("weights.onnx");
            detector["sha256"] = json!(Sha256Digest::of(Self::WEIGHTS).to_hex());
            detector["execution_provider"] = json!("cpu");
            edit(&mut manifest);
            let text = manifest.to_string();
            std::fs::write(&self.source.manifest_path, &text).unwrap();
            self.source.manifest_sha256 = Sha256Digest::of(text.as_bytes()).to_hex();
            self
        }

        fn refusal(&self) -> String {
            error_chain(&verify(&self.source).unwrap_err())
        }
    }

    impl Drop for Kit {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// SEC-06, the positive control: with every pin right the gate hands over the very
    /// bytes it hashed and loads nothing.
    #[test]
    fn sec_06_a_complete_kit_verifies_and_returns_the_hashed_bytes() {
        let kit = Kit::new("complete").with_manifest(|_| {});
        let verified = verify(&kit.source).unwrap();
        assert_eq!(verified.weights, Kit::WEIGHTS);
        assert_eq!(verified.spec.sha256, Sha256Digest::of(Kit::WEIGHTS));
        assert!(verified.runtime.is_absolute());
        assert_eq!(verified.directml, None);
        // The licence of the weights is recorded in the manifest entry the engine keeps.
        assert!(verified.spec.license.starts_with("MIT"));
    }

    #[test]
    fn sec_06_a_tampered_manifest_is_refused() {
        let kit = Kit::new("manifest").with_manifest(|_| {});
        // Any edit after the operator pinned it: here, the detector is made more trusting.
        let edited = std::fs::read_to_string(&kit.source.manifest_path)
            .unwrap()
            .replace("0.05", "0.01");
        std::fs::write(&kit.source.manifest_path, edited).unwrap();
        assert!(kit.refusal().contains("SHA-256 mismatch"));

        let mut unpinned = Kit::new("manifest-pin").with_manifest(|_| {});
        unpinned.source.manifest_sha256 = "not a digest".into();
        assert!(unpinned.refusal().contains("invalid SHA-256"));
        unpinned.source.manifest_sha256 = Sha256Digest::of(b"another manifest").to_hex();
        assert!(unpinned.refusal().contains("SHA-256 mismatch"));
    }

    #[test]
    fn sec_06_a_missing_or_unselected_manifest_loads_nothing() {
        let mut kit = Kit::new("missing");
        kit.source.manifest_sha256 = Sha256Digest::of(b"a manifest").to_hex();
        assert!(kit.refusal().contains("cannot read"));
        let template = include_str!("../../../../../docs/thesis/plantillas/model_manifest.json");
        std::fs::write(&kit.source.manifest_path, template).unwrap();
        kit.source.manifest_sha256 = Sha256Digest::of(template.as_bytes()).to_hex();
        assert!(kit.refusal().contains("no model has been selected"));
    }

    /// The weights are checked before any library is loaded: a corrupted, truncated,
    /// extended or substituted file is refused with only temporary files on disk.
    #[test]
    fn sec_06_corrupted_or_substituted_weights_are_refused() {
        let kit = Kit::new("weights").with_manifest(|_| {});
        let weights = kit.dir.join("weights.onnx");
        let mut flipped = Kit::WEIGHTS.to_vec();
        flipped[3] ^= 0x01;
        let truncated = &Kit::WEIGHTS[..Kit::WEIGHTS.len() - 1];
        let extended = [Kit::WEIGHTS, b"!"].concat();
        for (case, bytes) in [
            ("one flipped bit", flipped.as_slice()),
            ("truncated", truncated),
            ("extended", extended.as_slice()),
            ("another file", b"a different model entirely".as_slice()),
            ("empty", b"".as_slice()),
        ] {
            std::fs::write(&weights, bytes).unwrap();
            let message = kit.refusal();
            assert!(message.contains("detector weights"), "{case}: {message}");
            assert!(message.contains("SHA-256 mismatch"), "{case}: {message}");
        }
        std::fs::remove_file(&weights).unwrap();
        assert!(kit.refusal().contains("cannot read"));
        // Restored, the same kit verifies again: the gate keeps no state.
        std::fs::write(&weights, Kit::WEIGHTS).unwrap();
        assert!(verify(&kit.source).is_ok());
    }

    #[test]
    fn sec_06_a_tampered_runtime_library_is_refused() {
        let mut kit = Kit::new("runtime").with_manifest(|_| {});
        std::fs::write(&kit.source.runtime_path, b"fake runtime, patched").unwrap();
        let message = kit.refusal();
        assert!(message.contains("ONNX Runtime library"), "{message}");
        assert!(message.contains("SHA-256 mismatch"), "{message}");

        kit.source.runtime_sha256 = "abc".into();
        assert!(kit.refusal().contains("invalid SHA-256"));
        kit.source.runtime_sha256 = Sha256Digest::of(Kit::RUNTIME).to_hex();
        std::fs::remove_file(&kit.source.runtime_path).unwrap();
        assert!(kit.refusal().contains("cannot read"));
    }

    /// The provider is fixed by the operator and validated by the manifest; DirectML needs
    /// its own pinned library, and nothing falls back to the CPU.
    #[test]
    fn sec_06_the_execution_provider_fails_closed() {
        let mut kit = Kit::new("provider").with_manifest(|_| {});
        kit.source.execution_provider = ExecutionProvider::DirectMl;
        assert!(
            kit.refusal()
                .contains("does not list the directml execution provider")
        );

        let kit = Kit::new("provider-both")
            .with_manifest(|m| m["artifacts"][0]["execution_provider"] = json!("cpu,directml"));
        let mut kit = kit;
        kit.source.execution_provider = ExecutionProvider::DirectMl;
        assert!(kit.refusal().contains("needs a pinned DirectML.dll"));

        let dll = kit.dir.join("DirectML.dll");
        std::fs::write(&dll, b"not a library").unwrap();
        kit.source.directml = Some((dll.clone(), Sha256Digest::of(b"another").to_hex()));
        let message = kit.refusal();
        assert!(message.contains("DirectML library"), "{message}");
        assert!(message.contains("SHA-256 mismatch"), "{message}");

        kit.source.directml = Some((dll, Sha256Digest::of(b"not a library").to_hex()));
        let verified = verify(&kit.source).unwrap();
        assert!(verified.directml.is_some_and(|p| p.is_absolute()));

        kit.source.execution_provider = ExecutionProvider::Cpu;
        assert!(kit.refusal().contains("provider is cpu"));
    }

    /// The manifest is the licence ledger: a detector entry without a licence, source or
    /// revision is refused, so no weights are ever loaded with an unrecorded licence.
    #[test]
    fn sec_06_a_detector_without_a_recorded_licence_is_refused() {
        for field in ["license", "source_url", "revision"] {
            let kit = Kit::new(&format!("licence-{field}"))
                .with_manifest(|m| m["artifacts"][0][field] = json!(" "));
            let message = kit.refusal();
            assert!(message.contains(field), "{field}: {message}");
        }
    }

    /// Every manifest the repository ships and the operator configurations pin records,
    /// per artefact, where it comes from, its revision, its licence and (for a file) its
    /// digest; no AGPL weights are named as the ones used (ADR-0009).
    #[test]
    fn sec_06_the_shipped_manifests_record_origin_digest_and_licence() {
        let manifests = [
            ("model_manifest.s0.json", S0_MANIFEST),
            (
                "model_manifest.rnf05.json",
                include_str!("../../../../../docs/thesis/plantillas/model_manifest.rnf05.json"),
            ),
        ];
        for (name, text) in manifests {
            let manifest: Value = serde_json::from_str(text).unwrap();
            assert_eq!(manifest["status"], "SELECCIONADO", "{name}");
            let artifacts = manifest["artifacts"].as_array().unwrap();
            assert!(!artifacts.is_empty(), "{name}");
            for artifact in artifacts {
                let who = format!("{name}: {}", artifact["name"]);
                for field in ["name", "source_url", "revision", "license", "test_report"] {
                    let value = artifact[field].as_str().unwrap_or_default();
                    assert!(!value.trim().is_empty(), "{who}: {field} is empty");
                }
                if artifact["role"] == "detector" {
                    let digest = artifact["sha256"].as_str().unwrap_or_default();
                    assert!(Sha256Digest::parse(digest).is_ok(), "{who}: {digest}");
                    let licence = artifact["license"].as_str().unwrap();
                    assert!(licence.starts_with("MIT"), "{who}: {licence}");
                    // The AGPL-3.0 weights are named only to say they are not used.
                    if licence.contains("AGPL") {
                        assert!(licence.contains("No se usan"), "{who}: {licence}");
                    }
                } else {
                    // A component of the operating system has no file to pin, and says so.
                    assert!(
                        artifact["sha256"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("no aplica")),
                        "{who}"
                    );
                }
            }
        }
    }
}
