//! [`OnnxVisionDetector`]: a generic, fail-closed YOLO-style detector on ONNX Runtime
//! (feature `onnx`, ADR-0009).
//!
//! Nothing is downloaded at build time: `ort` is built with `load-dynamic`, and the
//! operator points the configuration at an ONNX Runtime library pinned by SHA-256. Loading
//! goes in this order, and any failure refuses the detector (there is no fallback model
//! and no fallback provider):
//!
//! 1. the manifest is read (bounded) and its SHA-256 checked against the operator's pin;
//! 2. its detector entry is validated ([`DetectorSpec`]), and the configured execution
//!    provider must be one the manifest lists as validated;
//! 3. for DirectML (feature `directml`), the operator's `DirectML.dll` is checked against
//!    its pin and loaded by absolute path *before* ONNX Runtime, so that ONNX Runtime binds
//!    to it and not to another `DirectML.dll` the system search would find;
//! 4. the runtime library's SHA-256 is checked, then the library is loaded by its absolute
//!    path (the check and the load are two steps: the library must sit in a directory only
//!    the operator can write);
//! 5. the weights are read into memory (bounded), their SHA-256 checked against the
//!    manifest, and the session is built from those same bytes with the configured
//!    provider. DirectML is registered with `error_on_failure`, sequential execution and
//!    the memory pattern disabled, as its documentation requires, and with ONNX Runtime's
//!    CPU fallback disabled (`session.disable_cpu_ep_fallback`): if the provider cannot be
//!    created, or any node would run on the CPU instead, the detector is refused instead
//!    of silently running on the CPU.
//!
//! Per region, the crop is letterboxed into the input ([`Letterbox`]), the output is
//! decoded and suppressed ([`decode`], [`nms`]), and the boxes are taken back to the frame
//! and clipped to the region ([`controls_from_candidates`]). Each planned region costs one
//! inference: packing several crops into one input was measured in S0 and changed the
//! detections, so it is not done (docs/thesis/S0_RESULTADOS.md).
//!
//! An inference costs about as much as its input has pixels, and a crop is never magnified
//! (S0 §6), so a small region padded to the model's full 1280×1280 input spends most of the
//! inference on padding. A model exported with free height and width
//! (`tools/s0/export_omniparser_v3.py --dynamic`, manifest `input_shapes`) therefore gets one
//! session per declared input shape, each pinned to its shape with ONNX Runtime's
//! free-dimension overrides, and every crop goes to the smallest shape that holds it at its
//! own resolution ([`best_fit`]). The sessions are built, and warmed up with one inference
//! each, at load, in parallel: the first observation does not pay for the shapes it did not
//! use. A model with a fixed input has one session and works as before.

use std::path::{Path, PathBuf};

use ommatix_application::{PortError, VisionDetector};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Control;
use ommatix_domain::incremental::BgraFrame;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

use crate::crop::{PixelRect, crop_bgra};
use crate::model::decode::{Candidate, decode, nms};
use crate::model::integrity::{Sha256Digest, read_verified};
use crate::model::letterbox::{Letterbox, PAD_VALUE, best_fit};
use crate::model::manifest::{DetectorSpec, ExecutionProvider, MAX_MANIFEST_BYTES};
use crate::model::{ModelError, controls_from_candidates};

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

/// A YOLO-style detector whose weights passed every check. It exists only after a
/// verified load, so [`VisionDetector::models_loaded`] is always `true`.
pub struct OnnxVisionDetector {
    spec: DetectorSpec,
    /// One session per `spec.input_sizes`, in the same order (smallest input first).
    sessions: Vec<Session>,
    input_name: String,
    provider: ExecutionProvider,
}

impl std::fmt::Debug for OnnxVisionDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxVisionDetector")
            .field("spec", &self.spec)
            .field("input_name", &self.input_name)
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

fn port(message: impl Into<String>) -> PortError {
    PortError::new(NAME, message)
}

fn model_error(e: ModelError) -> PortError {
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

/// Loads the operator's `DirectML.dll` by absolute path, so that ONNX Runtime binds to it.
#[cfg(feature = "directml")]
fn preload_directml(path: &Path) -> Result<(), PortError> {
    ort::util::preload_dylib(path)
        .map_err(|e| port("cannot load the DirectML library").with_source(e))
}

#[cfg(not(feature = "directml"))]
fn preload_directml(_: &Path) -> Result<(), PortError> {
    Err(port(
        "the DirectML execution provider is not in this build; rebuild with `--features directml`",
    ))
}

#[cfg(feature = "directml")]
fn directml_providers() -> Vec<ort::ep::ExecutionProviderDispatch> {
    vec![ort::ep::DirectML::default().build().error_on_failure()]
}

#[cfg(not(feature = "directml"))]
fn directml_providers() -> Vec<ort::ep::ExecutionProviderDispatch> {
    // Unreachable: `preload_directml` refuses DirectML first in this build.
    Vec::new()
}

impl OnnxVisionDetector {
    /// Loads and verifies the detector (see the module documentation).
    pub fn load(source: &OnnxDetectorSource) -> Result<Self, PortError> {
        if source.intra_threads > MAX_INTRA_THREADS {
            return Err(port(format!(
                "intra-op threads must be between 0 (ONNX Runtime's default) and \
                 {MAX_INTRA_THREADS}"
            )));
        }
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
        match (provider, &source.directml) {
            (ExecutionProvider::DirectMl, Some((path, sha256))) => {
                let path = verified_library("DirectML library", path, sha256)?;
                preload_directml(&path)?;
            }
            (ExecutionProvider::DirectMl, None) => {
                return Err(port(
                    "the DirectML execution provider needs a pinned DirectML.dll",
                ));
            }
            (ExecutionProvider::Cpu, Some(_)) => {
                return Err(port("a DirectML library is pinned but the provider is cpu"));
            }
            (ExecutionProvider::Cpu, None) => {}
        }

        let runtime = verified_library(
            "ONNX Runtime library",
            &source.runtime_path,
            &source.runtime_sha256,
        )?;
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
        let sessions = Self::sessions(provider, source.intra_threads, &spec, &weights)?;
        let first = &sessions[0];
        let [input] = first.inputs() else {
            return Err(port(format!(
                "the detector must have exactly one input, it has {}",
                first.inputs().len()
            )));
        };
        let input_name = input.name().to_owned();
        if first.outputs().is_empty() {
            return Err(port("the detector has no output"));
        }
        let mut detector = Self {
            spec,
            sessions,
            input_name,
            provider,
        };
        if provider == ExecutionProvider::DirectMl {
            detector.warm_up()?;
        }
        Ok(detector)
    }

    /// One session per declared input size, built in parallel (a DirectML session takes
    /// about a second to build).
    fn sessions(
        provider: ExecutionProvider,
        intra_threads: usize,
        spec: &DetectorSpec,
        weights: &[u8],
    ) -> Result<Vec<Session>, PortError> {
        let dimensions = spec.input_dimensions.as_ref();
        std::thread::scope(|scope| {
            let builds: Vec<_> = spec
                .input_sizes
                .iter()
                .map(|&size| {
                    scope.spawn(move || {
                        Self::session(provider, intra_threads, weights, dimensions, size)
                    })
                })
                .collect();
            builds
                .into_iter()
                .map(|build| {
                    build
                        .join()
                        .unwrap_or_else(|_| Err(port("a session build panicked")))
                })
                .collect()
        })
    }

    /// One inference per session on an empty input: the first inference of a DirectML
    /// session costs two to three times a warm one (S0 §9.3), and that should be paid at
    /// load, not by the first observation that happens to use the shape. The output is
    /// discarded. The CPU provider has no such first-use cost.
    fn warm_up(&mut self) -> Result<(), PortError> {
        let (name, sizes) = (self.input_name.as_str(), &self.spec.input_sizes);
        std::thread::scope(|scope| {
            let runs: Vec<_> = self
                .sessions
                .iter_mut()
                .zip(sizes)
                .map(|(session, &(width, height))| {
                    scope.spawn(move || {
                        let shape = [1, 3, height as usize, width as usize];
                        let blank = vec![PAD_VALUE; 3 * width as usize * height as usize];
                        let tensor = Tensor::from_array((shape, blank))
                            .map_err(|e| port(format!("cannot build the warm-up tensor: {e}")))?;
                        session
                            .run(ort::inputs![name => tensor])
                            .map(drop)
                            .map_err(|e| port(format!("the warm-up inference failed: {e}")))
                    })
                })
                .collect();
            // Every run is waited for, then the first failure (in shape order) is reported.
            let results: Vec<_> = runs
                .into_iter()
                .map(|run| {
                    run.join()
                        .unwrap_or_else(|_| Err(port("a warm-up inference panicked")))
                })
                .collect();
            results.into_iter().collect::<Result<(), _>>()
        })
    }

    fn session(
        provider: ExecutionProvider,
        intra_threads: usize,
        weights: &[u8],
        dimensions: Option<&(String, String)>,
        (width, height): (u32, u32),
    ) -> Result<Session, PortError> {
        let configure = |what: &'static str| {
            move |e: ort::Error<ort::session::builder::SessionBuilder>| {
                port(format!("cannot configure the session ({what}): {e}"))
            }
        };
        let mut builder = Session::builder()
            .map_err(|e| port(format!("cannot create an ONNX Runtime session: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(configure("optimization level"))?
            .with_intra_threads(intra_threads)
            .map_err(configure("intra-op threads"))?
            .with_deterministic_compute(true)
            .map_err(configure("deterministic compute"))?;
        if let Some((height_axis, width_axis)) = dimensions {
            // Pin the model's free axes: the session then has the static shapes DirectML
            // needs to place every node on the GPU.
            builder = builder
                .with_dimension_override(height_axis, i64::from(height))
                .map_err(configure("input height"))?
                .with_dimension_override(width_axis, i64::from(width))
                .map_err(configure("input width"))?;
        }
        if provider == ExecutionProvider::DirectMl {
            // DirectML does not support parallel execution or memory patterns. With the CPU
            // fallback disabled, ONNX Runtime refuses the session if any node would run
            // on the CPU instead of DirectML.
            builder = builder
                .with_parallel_execution(false)
                .map_err(configure("sequential execution"))?
                .with_memory_pattern(false)
                .map_err(configure("memory pattern"))?
                .with_config_entry("session.disable_cpu_ep_fallback", "1")
                .map_err(configure("no CPU fallback"))?
                .with_execution_providers(directml_providers())
                .map_err(|e| {
                    port(format!(
                        "cannot create the DirectML execution provider: {e}"
                    ))
                })?;
        }
        builder
            .commit_from_memory(weights)
            .map_err(|e| port(format!("cannot load the detector weights: {e}")))
    }

    /// The validated manifest entry.
    pub fn spec(&self) -> &DetectorSpec {
        &self.spec
    }

    /// The execution provider the session runs on.
    pub fn execution_provider(&self) -> ExecutionProvider {
        self.provider
    }

    /// Every input size the detector can feed, `(width, height)`, smallest first. The
    /// last one is the reference (`input_shape`).
    pub fn input_sizes(&self) -> &[(u32, u32)] {
        &self.spec.input_sizes
    }

    /// One inference of session `size` on a full input tensor: the decoded candidates,
    /// before NMS.
    fn infer(&mut self, size: usize, input: Vec<f32>) -> Result<Vec<Candidate>, PortError> {
        let (width, height) = self.spec.input_sizes[size];
        let shape = [1, 3, height as usize, width as usize];
        let tensor = Tensor::from_array((shape, input))
            .map_err(|e| port(format!("cannot build the input tensor: {e}")))?;
        let outputs = self.sessions[size]
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|e| port(format!("inference failed: {e}")))?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| port(format!("the detector output is not an f32 tensor: {e}")))?;
        let dims: Vec<i64> = shape.iter().copied().collect();
        let spec = &self.spec;
        decode(
            &dims,
            data,
            spec.output_layout,
            spec.classes.len(),
            spec.score_threshold,
        )
        .map_err(model_error)
    }

    /// The candidates of the whole frame letterboxed into one reference-size input, after
    /// NMS, in input coordinates with their scores: what [`VisionDetector::detect`] maps
    /// back for a full refresh at the reference size. For measurement tools (the S0
    /// provider equivalence check).
    pub fn frame_candidates(&mut self, frame: &BgraFrame) -> Result<Vec<Candidate>, PortError> {
        let rect = PixelRect::cover(&frame.bounds(), frame.width(), frame.height())
            .ok_or_else(|| port("empty frame"))?;
        let reference = self.spec.input_sizes.len() - 1;
        let (input_width, input_height) = self.spec.input_sizes[reference];
        let letterbox = Letterbox::fit(rect.width, rect.height, input_width, input_height)
            .ok_or_else(|| port("empty frame"))?;
        let candidates = self.infer(reference, letterbox.tensor(&crop_bgra(frame, rect)))?;
        Ok(nms(candidates, self.spec.nms_iou))
    }

    /// The index in [`input_sizes`](Self::input_sizes) a `region` of `frame` is fed through:
    /// the smallest input that holds its pixels at their own resolution, or the one that
    /// reduces it least. `None` when the region has no pixel inside the frame.
    pub fn input_for(&self, frame: &BgraFrame, region: &BBox) -> Option<usize> {
        let rect = PixelRect::cover(region, frame.width(), frame.height())?;
        best_fit(&self.spec.input_sizes, rect.width, rect.height)
    }

    /// The controls of one region through the input `size` of
    /// [`input_sizes`](Self::input_sizes): the one [`input_for`](Self::input_for) chooses,
    /// or any other to compare them. For measurement tools.
    pub fn detect_at(
        &mut self,
        frame: &BgraFrame,
        region: &BBox,
        size: usize,
    ) -> Result<Vec<Control>, PortError> {
        Ok(self
            .scored_at(frame, region, size)?
            .into_iter()
            .map(|(control, _)| control)
            .collect())
    }

    /// [`detect_at`](Self::detect_at) with each control's score, in the order of the
    /// candidates after NMS (highest score first), for measurement tools.
    pub fn scored_at(
        &mut self,
        frame: &BgraFrame,
        region: &BBox,
        size: usize,
    ) -> Result<Vec<(Control, f32)>, PortError> {
        let Some(&(input_width, input_height)) = self.spec.input_sizes.get(size) else {
            return Err(port("no such input size"));
        };
        let Some(rect) = PixelRect::cover(region, frame.width(), frame.height()) else {
            return Ok(Vec::new());
        };
        let Some(clip) = region.clip(&frame.bounds()) else {
            return Ok(Vec::new());
        };
        let letterbox = Letterbox::fit(rect.width, rect.height, input_width, input_height)
            .ok_or_else(|| port("empty crop"))?;
        let candidates = self.infer(size, letterbox.tensor(&crop_bgra(frame, rect)))?;
        let kept = nms(candidates, self.spec.nms_iou);
        let origin = (f64::from(rect.x), f64::from(rect.y));
        Ok(kept
            .iter()
            .filter_map(|candidate| {
                let one = std::slice::from_ref(candidate);
                controls_from_candidates(one, &letterbox, origin, &clip, &self.spec.classes)
                    .pop()
                    .map(|control| (control, candidate.score))
            })
            .collect())
    }

    fn detect_region(
        &mut self,
        frame: &BgraFrame,
        region: &BBox,
    ) -> Result<Vec<Control>, PortError> {
        match self.input_for(frame, region) {
            Some(size) => self.detect_at(frame, region, size),
            None => Ok(Vec::new()),
        }
    }
}

impl VisionDetector for OnnxVisionDetector {
    fn name(&self) -> &'static str {
        match self.provider {
            ExecutionProvider::Cpu => "onnx-cpu",
            ExecutionProvider::DirectMl => "onnx-directml",
        }
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
            execution_provider: ExecutionProvider::Cpu,
            directml: None,
            intra_threads: 1,
        }
    }

    fn error(source: &OnnxDetectorSource) -> String {
        let err = OnnxVisionDetector::load(source).unwrap_err();
        ommatix_application::error_chain(&err)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ommatix-onnx-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn nothing_loads_without_a_verified_manifest() {
        let dir = temp_dir("manifest");
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

    /// The configured provider must be one the manifest validated, DirectML needs its
    /// pinned library, and nothing falls back to the CPU: every case is refused before
    /// ONNX Runtime is loaded.
    #[test]
    fn the_execution_provider_fails_closed() {
        let dir = temp_dir("provider");
        let s0 = include_str!("../../../../docs/thesis/plantillas/model_manifest.s0.json");
        let cpu_only = s0.replace(
            "\"execution_provider\": \"cpu,directml\"",
            "\"execution_provider\": \"cpu\"",
        );
        assert!(cpu_only.contains("\"execution_provider\": \"cpu\""));
        let mut src = source(&dir);
        std::fs::write(&src.manifest_path, &cpu_only).unwrap();
        src.manifest_sha256 = Sha256Digest::of(cpu_only.as_bytes()).to_hex();
        src.execution_provider = ExecutionProvider::DirectMl;
        assert!(error(&src).contains("does not list the directml execution provider"));

        std::fs::write(&src.manifest_path, s0).unwrap();
        src.manifest_sha256 = Sha256Digest::of(s0.as_bytes()).to_hex();
        assert!(error(&src).contains("needs a pinned DirectML.dll"));

        let dll = dir.join("DirectML.dll");
        std::fs::write(&dll, b"not a library").unwrap();
        src.directml = Some((dll.clone(), Sha256Digest::of(b"another").to_hex()));
        assert!(error(&src).contains("SHA-256 mismatch"));

        src.execution_provider = ExecutionProvider::Cpu;
        assert!(error(&src).contains("provider is cpu"));

        src.directml = None;
        src.intra_threads = MAX_INTRA_THREADS + 1;
        assert!(error(&src).contains("intra-op threads"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
