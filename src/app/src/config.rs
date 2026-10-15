//! The operator's configuration file, shaped like the dossier template
//! `docs/thesis/plantillas/configuracion_mvp.toml`.
//!
//! Every field is optional and defaults to the template's value; unknown sections and keys
//! are rejected. [`Config::validate`] rejects values this build cannot honour (another
//! transport or profile, weights outside a model manifest, unpinned or half-configured
//! models, a persistent cache, permissions it cannot enforce) instead of accepting them
//! silently.
//!
//! `[cache]` may add, beyond the template, a curated prototypes file for Algorithm 3A, its
//! SHA-256 and the thresholds to use it with. The thesis calibrates those thresholds (H4),
//! so none has a default: the resolver is on only when the file, its digest, the context,
//! `gradient_tau`, `max_distance` and `min_margin` are all set, and any of them without the
//! others is refused. The file is checked against the digest before it is parsed (SEC-06).
//!
//! The file is the operator's: nothing the agent sends can change it, and no CLI flag
//! loosens `[safety]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ommatix_adapter_mcp::config::MIN_MAX_MESSAGE_BYTES;
use ommatix_adapter_mcp::{DEFAULT_MAX_MESSAGE_BYTES, MODERN_PROTOCOL_VERSION, McpConfig};
use ommatix_adapter_vision::model::integrity::Sha256Digest;
use ommatix_application::{IconResolverConfig, LocalPolicy, VerifyParams};
use ommatix_domain::appearance::CachePolicy;
use ommatix_domain::candidates::CandidateSearch;
use ommatix_domain::incremental::PlanParams;
use serde::Deserialize;

/// Longest effect-verification window accepted, in milliseconds.
const MAX_VERIFY_TIMEOUT_MS: u64 = 5_000;
/// Longest wait between two verification captures accepted, in milliseconds.
const MAX_VERIFY_POLL_MS: u64 = 1_000;

/// Largest configuration file read, in bytes.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// What the engine does with actions when the CLI does not choose an input adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    /// Actions are simulated and recorded; nothing reaches the OS.
    DryRun,
    /// Native input; still needs `[safety] actuation_enabled = true`, the `native-input`
    /// build and the screen source.
    Native,
}

/// How Algorithm 2 finds the candidate controls of an observation (`[engine] candidate_search`,
/// ADR-0027). Both give the same result; `scan` is the reference and the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateSearchKind {
    /// Every control is inspected, in source order.
    #[default]
    Scan,
    /// An R*-tree built once per observation over the reconciled controls. Experimental:
    /// its adoption as the default is not decided (ADR-0027).
    Rstar,
}

impl CandidateSearchKind {
    pub const fn search(self) -> CandidateSearch {
        match self {
            Self::Scan => CandidateSearch::Scan,
            Self::Rstar => CandidateSearch::RStar,
        }
    }
}

/// `[engine]`: Algorithm 1's parameters and the engine's mode.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct EngineSection {
    pub mode: EngineMode,
    pub platform: String,
    pub snapshot_policy: String,
    pub tile_size: u32,
    pub halo_px: u32,
    pub max_regions: usize,
    pub full_refresh_area_ratio: f64,
    /// The adaptive cost rule of Algorithm 1 (INC-07, ADR-0030): a plan with this many regions
    /// or more is processed as a full refresh (reason `adaptive_cost`), because each region is a
    /// whole detector inference and the full frame is one. `0` switches the rule off.
    pub adaptive_min_regions: usize,
    /// How old the snapshot's capture and the accessibility read that confirms a target may
    /// be when an action is authorised, measured on the injected clock (ADR-0025). Checked at
    /// the gate and before every real input.
    pub max_snapshot_age_ms: u64,
    /// How long after delivering an action the engine looks for its visual effect
    /// (SDD §2.9, the deadline of BDD-29). 0: a single check right after delivery.
    pub verify_timeout_ms: u64,
    /// The wait between two verification captures inside that window.
    pub verify_poll_ms: u64,
    /// Detection, text recognition and the accessibility read of one observation run
    /// concurrently (SDD §2.7, ADR-0019). The evidence is the same either way; `false`
    /// runs them one after the other, for diagnosis and for before/after measurements.
    pub concurrent_stages: bool,
    /// The experimental full-frame baseline B0 (dossier chapter 09 §9.2, ADR-0024): every
    /// observation processes the whole frame. An experiment switch, never a production mode;
    /// `notes()` says so on stderr when it is on.
    pub experimental_full_frame: bool,
    /// How Algorithm 2 enumerates the candidate controls (ADR-0027). `scan`, the reference,
    /// by default; `rstar` is the experimental index and gives the same results.
    pub candidate_search: CandidateSearchKind,
}

impl Default for EngineSection {
    fn default() -> Self {
        Self {
            mode: EngineMode::DryRun,
            platform: "windows".to_owned(),
            snapshot_policy: "strict".to_owned(),
            tile_size: 64,
            halo_px: 32,
            max_regions: 4,
            full_refresh_area_ratio: 0.40,
            adaptive_min_regions: 2,
            max_snapshot_age_ms: 10_000,
            verify_timeout_ms: 500,
            verify_poll_ms: 50,
            concurrent_stages: true,
            experimental_full_frame: false,
            candidate_search: CandidateSearchKind::Scan,
        }
    }
}

/// `[mcp]`: the driving adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct McpSection {
    pub profile: String,
    pub transport: String,
    pub legacy_enabled: bool,
    pub max_message_bytes: usize,
}

impl Default for McpSection {
    fn default() -> Self {
        Self {
            profile: MODERN_PROTOCOL_VERSION.to_owned(),
            transport: "stdio".to_owned(),
            legacy_enabled: false,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        }
    }
}

/// `[safety]`: the local policy. Its defaults deny everything.
#[allow(clippy::struct_excessive_bools)] // the dossier template's keys, one flag each
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SafetySection {
    pub actuation_enabled: bool,
    pub allowed_applications: Vec<String>,
    pub allow_destructive: bool,
    pub allow_screenshots_to_client: bool,
    pub include_values: bool,
    /// The operator's stop (chapter 08 §8.2): while a file exists at this path, no new
    /// input is sent. Relative to the configuration file. Empty: no stop file, and the
    /// only local stop is ending the process.
    pub stop_file: String,
    /// Per application, the only window classes live input may go to (ADR-0020), for an
    /// application whose process owns more than the window the operator means: for
    /// `explorer.exe` that is `["CabinetWClass"]`, a File Explorer window, and not the
    /// desktop, the taskbar or a pop-up. Keys are the lowercase executable names of
    /// `allowed_applications`. Empty (the default): no application is restricted by class.
    pub allowed_window_classes: BTreeMap<String, Vec<String>>,
    /// The operator's pin on one window (ADR-0020): the path of a file that holds the
    /// handle (decimal) of the only window live input may go to. Read on every action;
    /// a missing, empty or malformed file allows no live input. Relative to the
    /// configuration file. Empty (the default): no pin.
    pub window_pin_file: String,
    /// The operator's purge file (SEC-07, chapter 08 §8.4): while the operator creates it, the
    /// engine deletes its state and cache at its next request, once per instance of the file,
    /// and never deletes the file. Relative to the configuration file. Empty (the default): no
    /// purge file.
    pub purge_file: String,
}

/// The OCR engine (`[models] ocr`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrKind {
    /// No OCR: elements carry empty labels with `label_source: unknown`.
    #[default]
    None,
    /// The OCR built into Windows (`Windows.Media.Ocr`), for exactly `ocr_language`.
    Winrt,
}

/// The accessibility evidence (`[models] accessibility`, ADR-0017).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityKind {
    /// No accessibility evidence: only the visual path.
    #[default]
    None,
    /// The foreground application's UI Automation tree (Windows), fused with the visual
    /// path. Only with a live source (`dxgi` or `screen`).
    Uia,
}

/// The ONNX Runtime execution provider of the detector (`[models]
/// detector_execution_provider`). The model manifest must list it as validated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// The CPU, the baseline.
    #[default]
    Cpu,
    /// DirectML (DirectX 12, Windows), with a pinned `DirectML.dll`; never falls back to
    /// the CPU.
    Directml,
}

/// Most intra-op threads `[models] detector_threads` accepts.
pub const MAX_DETECTOR_THREADS: u32 = 64;

/// `[models]`: the perception models, chosen in the S0 spike. Empty (no model, no OCR)
/// until the operator pins them; every file is pinned by SHA-256 and refused on mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ModelsSection {
    /// Model manifest shaped like `docs/thesis/plantillas/model_manifest.json`, whose
    /// detector entry names the weights and their SHA-256. Relative to the configuration
    /// file, like `onnxruntime_path`.
    pub detector_manifest: String,
    pub detector_manifest_sha256: String,
    /// The ONNX Runtime library the detector runs on (`onnxruntime.dll`).
    pub onnxruntime_path: String,
    pub onnxruntime_sha256: String,
    /// The detector's execution provider (`cpu` unless the operator chooses `directml`).
    pub detector_execution_provider: ProviderKind,
    /// With `directml`: the `DirectML.dll` ONNX Runtime binds to, pinned like the runtime.
    /// Empty with `cpu`.
    pub directml_path: String,
    pub directml_sha256: String,
    /// Intra-op threads of the detector, `1..=MAX_DETECTOR_THREADS`; `0` (the default)
    /// lets ONNX Runtime choose, which is the number of physical cores.
    pub detector_threads: u32,
    pub ocr: OcrKind,
    /// BCP-47 tag of the OCR language; never replaced by another language.
    pub ocr_language: String,
    /// Accessibility evidence fused with the detector and the OCR (ADR-0017).
    pub accessibility: AccessibilityKind,
    /// Template keys: the weights are declared in the manifest, so these stay empty.
    pub detector_path: String,
    pub detector_sha256: String,
    pub ocr_path: String,
    pub ocr_sha256: String,
}

impl Default for ModelsSection {
    fn default() -> Self {
        Self {
            detector_manifest: String::new(),
            detector_manifest_sha256: String::new(),
            onnxruntime_path: String::new(),
            onnxruntime_sha256: String::new(),
            detector_execution_provider: ProviderKind::Cpu,
            directml_path: String::new(),
            directml_sha256: String::new(),
            detector_threads: 0,
            ocr: OcrKind::None,
            ocr_language: "es-ES".to_owned(),
            accessibility: AccessibilityKind::None,
            detector_path: String::new(),
            detector_sha256: String::new(),
            ocr_path: String::new(),
            ocr_sha256: String::new(),
        }
    }
}

impl ModelsSection {
    /// Whether a detector is configured (validation makes the four keys all-or-nothing).
    pub fn has_detector(&self) -> bool {
        !self.detector_manifest.is_empty()
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |ok: bool, problem: &str| {
            if !ok {
                out.push(problem.to_owned());
            }
        };
        check(
            self.detector_path.is_empty() && self.detector_sha256.is_empty(),
            "[models] detector_path and detector_sha256 must stay empty: the weights and \
             their SHA-256 are declared in the model manifest (set detector_manifest and \
             detector_manifest_sha256)",
        );
        check(
            self.ocr_path.is_empty() && self.ocr_sha256.is_empty(),
            "[models] ocr_path and ocr_sha256 must stay empty: no file-based OCR model is \
             supported (use ocr = \"winrt\")",
        );
        let detector = [
            &self.detector_manifest,
            &self.detector_manifest_sha256,
            &self.onnxruntime_path,
            &self.onnxruntime_sha256,
        ];
        check(
            detector.iter().all(|v| v.is_empty()) || detector.iter().all(|v| !v.is_empty()),
            "[models] detector_manifest, detector_manifest_sha256, onnxruntime_path and \
             onnxruntime_sha256 must be set together (or all left empty for no detector \
             model)",
        );
        check(
            [
                &self.detector_manifest_sha256,
                &self.onnxruntime_sha256,
                &self.directml_sha256,
            ]
            .iter()
            .all(|v| v.is_empty() || is_sha256(v)),
            "[models] detector_manifest_sha256, onnxruntime_sha256 and directml_sha256 must \
             be SHA-256 digests (64 hexadecimal characters)",
        );
        let directml_pinned = !self.directml_path.is_empty() && !self.directml_sha256.is_empty();
        let directml_empty = self.directml_path.is_empty() && self.directml_sha256.is_empty();
        match self.detector_execution_provider {
            ProviderKind::Cpu => check(
                directml_empty,
                "[models] directml_path and directml_sha256 are only used with \
                 detector_execution_provider = \"directml\"",
            ),
            ProviderKind::Directml => check(
                directml_pinned && self.has_detector(),
                "[models] detector_execution_provider = \"directml\" needs a detector and \
                 directml_path with directml_sha256 (the DirectML.dll ONNX Runtime binds to); \
                 it never falls back to the CPU",
            ),
        }
        check(
            self.detector_threads <= MAX_DETECTOR_THREADS,
            "[models] detector_threads must be between 0 (ONNX Runtime's default: the \
             physical cores) and 64",
        );
        check(
            is_language_tag(&self.ocr_language),
            "[models] ocr_language must be a BCP-47 language tag such as \"es-ES\"",
        );
        out
    }
}

fn is_sha256(hex: &str) -> bool {
    Sha256Digest::parse(hex).is_ok()
}

/// A BCP-47-shaped tag: ASCII letters, digits and inner hyphens, 2 to 35 characters.
fn is_language_tag(tag: &str) -> bool {
    (2..=35).contains(&tag.len())
        && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !tag.starts_with('-')
        && !tag.ends_with('-')
        && !tag.contains("--")
}

/// `[cache]`: the icon cache (Algorithm 3A).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CacheSection {
    /// Most curated prototypes the file may hold.
    pub max_entries: u32,
    pub persist: bool,
    /// Curated prototypes, JSON lines (see `prototypes.rs`); relative to the configuration
    /// file. Empty: no prototypes, every icon stays unknown.
    pub prototypes: String,
    /// The SHA-256 (64 hexadecimal characters) of the prototypes file the operator reviewed
    /// (chapter 08 §8.2: hash of the cache). Required with `prototypes`.
    pub prototypes_sha256: String,
    /// The application or screen whose prototypes apply. Until the engine can identify
    /// the foreground application (S3), the operator names it here.
    pub context: String,
    /// Gradient threshold of the descriptor, in grey levels (1..=255).
    pub gradient_tau: Option<u8>,
    /// Largest descriptor distance accepted as a match, in [0, 1].
    pub max_distance: Option<f64>,
    /// Smallest distance gap to the next different label, in [0, 1].
    pub min_margin: Option<f64>,
}

impl Default for CacheSection {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            persist: false,
            prototypes: String::new(),
            prototypes_sha256: String::new(),
            context: String::new(),
            gradient_tau: None,
            max_distance: None,
            min_margin: None,
        }
    }
}

impl CacheSection {
    /// Whether any key of the icon resolver is set.
    fn resolver_requested(&self) -> bool {
        !self.prototypes.is_empty()
            || !self.prototypes_sha256.is_empty()
            || !self.context.is_empty()
            || self.gradient_tau.is_some()
            || self.max_distance.is_some()
            || self.min_margin.is_some()
    }

    /// What is wrong with this section, one problem per entry.
    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |ok: bool, problem: &str| {
            if !ok {
                out.push(problem.to_owned());
            }
        };
        check(
            self.max_entries >= 1,
            "[cache] max_entries must be at least 1",
        );
        check(
            !self.persist,
            "[cache] persist = true cannot be honoured: the icon cache lives in memory only",
        );
        let c = self;
        if c.resolver_requested() {
            check(
                !c.prototypes.trim().is_empty()
                    && !c.prototypes_sha256.is_empty()
                    && !c.context.is_empty()
                    && c.gradient_tau.is_some()
                    && c.max_distance.is_some()
                    && c.min_margin.is_some(),
                "[cache] the icon resolver needs all of prototypes, prototypes_sha256, context, \
                 gradient_tau, max_distance and min_margin (the thesis calibrates them; there \
                 are no defaults)",
            );
            check(
                c.prototypes_sha256.is_empty() || is_sha256(&c.prototypes_sha256),
                "[cache] prototypes_sha256 must be a SHA-256 digest (64 hexadecimal characters)",
            );
            check(
                c.context.is_empty()
                    || (!c.context.trim().is_empty()
                        && c.context.chars().count() <= crate::prototypes::MAX_CONTEXT_CHARS
                        && !c.context.chars().any(char::is_control)),
                "[cache] context must be a visible name of at most 256 characters",
            );
            check(
                c.gradient_tau != Some(0),
                "[cache] gradient_tau must be between 1 and 255",
            );
            check(
                [c.max_distance, c.min_margin]
                    .iter()
                    .flatten()
                    .all(|v| (0.0..=1.0).contains(v)),
                "[cache] max_distance and min_margin must be between 0 and 1",
            );
        }
        out
    }
}

/// The whole file. `Config::default()` is the dossier template.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub engine: EngineSection,
    pub mcp: McpSection,
    pub safety: SafetySection,
    pub models: ModelsSection,
    pub cache: CacheSection,
    /// Directory of the file this configuration was read from: relative paths in it
    /// resolve there.
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
}

impl Config {
    /// Parses and validates a configuration file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).context("invalid configuration")?;
        config.validate()?;
        Ok(config)
    }

    /// Reads `path`, or returns the template's defaults when there is none.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let read = || -> Result<String> {
            let size = std::fs::metadata(path)?.len();
            if size > MAX_CONFIG_BYTES {
                bail!("the file has {size} bytes; the limit is {MAX_CONFIG_BYTES}");
            }
            Ok(std::fs::read_to_string(path)?)
        };
        let text = read().with_context(|| format!("cannot read {}", path.display()))?;
        let mut config = Self::parse(&text).with_context(|| format!("in {}", path.display()))?;
        config.base_dir = path.parent().map(Path::to_path_buf);
        Ok(config)
    }

    /// Rejects values that are out of range or that this build cannot honour. Every
    /// problem is reported, one per line.
    pub fn validate(&self) -> Result<()> {
        let problems = self.problems();
        if problems.is_empty() {
            Ok(())
        } else {
            bail!("invalid configuration:\n  - {}", problems.join("\n  - "))
        }
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |ok: bool, problem: &str| {
            if !ok {
                out.push(problem.to_owned());
            }
        };
        let e = &self.engine;
        check(
            e.platform == "windows",
            "[engine] platform: the only target platform is \"windows\" (ADR-0008); \
             other platforms are future ports",
        );
        check(
            e.snapshot_policy == "strict",
            "[engine] snapshot_policy: only \"strict\" is implemented (every observation \
             publishes a new snapshot)",
        );
        check(
            (8..=1024).contains(&e.tile_size),
            "[engine] tile_size must be between 8 and 1024 pixels",
        );
        check(
            e.halo_px <= 1024,
            "[engine] halo_px must be at most 1024 pixels",
        );
        check(
            (1..=1024).contains(&e.max_regions),
            "[engine] max_regions must be between 1 and 1024",
        );
        check(
            e.full_refresh_area_ratio > 0.0 && e.full_refresh_area_ratio <= 1.0,
            "[engine] full_refresh_area_ratio must be greater than 0 and at most 1",
        );
        check(
            e.adaptive_min_regions <= 1024,
            "[engine] adaptive_min_regions must be at most 1024 (0 switches the rule off)",
        );
        check(
            e.max_snapshot_age_ms >= 1,
            "[engine] max_snapshot_age_ms must be at least 1",
        );
        check(
            e.verify_timeout_ms <= MAX_VERIFY_TIMEOUT_MS,
            "[engine] verify_timeout_ms must be between 0 and 5000",
        );
        check(
            (1..=MAX_VERIFY_POLL_MS).contains(&e.verify_poll_ms),
            "[engine] verify_poll_ms must be between 1 and 1000",
        );

        let m = &self.mcp;
        check(
            m.profile == MODERN_PROTOCOL_VERSION,
            "[mcp] profile: only \"2026-07-28\" is served; enable 2025-11-25 with \
             legacy_enabled = true",
        );
        check(
            m.transport == "stdio",
            "[mcp] transport: only \"stdio\" is implemented",
        );
        check(
            (MIN_MAX_MESSAGE_BYTES..=DEFAULT_MAX_MESSAGE_BYTES).contains(&m.max_message_bytes),
            "[mcp] max_message_bytes must be between 4096 and 65536 (the contract's 64 KiB \
             limit)",
        );

        let s = &self.safety;
        check(
            s.allowed_applications
                .iter()
                .all(|a| !a.trim().is_empty() && a.chars().count() <= 256),
            "[safety] allowed_applications entries must be non-empty names of at most 256 \
             characters",
        );
        check(
            s.stop_file.is_empty() || !s.stop_file.trim().is_empty(),
            "[safety] stop_file must be a path, or empty for none",
        );
        check(
            s.window_pin_file.is_empty() || !s.window_pin_file.trim().is_empty(),
            "[safety] window_pin_file must be a path, or empty for none",
        );
        check(
            s.purge_file.is_empty() || !s.purge_file.trim().is_empty(),
            "[safety] purge_file must be a path, or empty for none",
        );
        check(
            !s.allow_destructive,
            "[safety] allow_destructive = true cannot be honoured: this build does not \
             classify destructive actions",
        );
        check(
            !s.allow_screenshots_to_client,
            "[safety] allow_screenshots_to_client = true cannot be honoured: the contract \
             sends no screenshots",
        );

        out.extend(window_class_problems(s));
        out.extend(self.models.problems());
        out.extend(self.cache.problems());
        out
    }

    /// Notes on settings that change the run and are not production behaviour, reported on
    /// stderr at start: the experimental full-frame baseline (ADR-0024).
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.engine.experimental_full_frame {
            notes.push(
                "[engine] experimental_full_frame = true: EXPERIMENTAL baseline B0, every \
                 observation processes the whole frame; not a production mode"
                    .to_owned(),
            );
        }
        notes
    }

    /// A path from this file: a relative one resolves against the file's directory (the
    /// working directory for the built-in defaults).
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        match &self.base_dir {
            Some(dir) if path.is_relative() => dir.join(path),
            _ => path.to_path_buf(),
        }
    }

    /// The operator's stop file, resolved like every path in this file; `None` when
    /// `[safety] stop_file` is empty.
    pub fn stop_file(&self) -> Option<PathBuf> {
        let path = &self.safety.stop_file;
        (!path.is_empty()).then(|| self.resolve_path(path))
    }

    /// The window pin file (ADR-0020), resolved like every path in this file; `None` when
    /// `[safety] window_pin_file` is empty.
    pub fn window_pin_file(&self) -> Option<PathBuf> {
        let path = &self.safety.window_pin_file;
        (!path.is_empty()).then(|| self.resolve_path(path))
    }

    /// The purge file (SEC-07), resolved like every path in this file; `None` when
    /// `[safety] purge_file` is empty.
    pub fn purge_file(&self) -> Option<PathBuf> {
        let path = &self.safety.purge_file;
        (!path.is_empty()).then(|| self.resolve_path(path))
    }

    /// Algorithm 3A's settings, reading the prototypes file; `None` when the resolver is
    /// not configured. Assumes [`Self::validate`] passed.
    pub fn icon_resolver(&self) -> Result<Option<IconResolverConfig>> {
        let c = &self.cache;
        let (Some(tau), Some(max_distance), Some(min_margin)) =
            (c.gradient_tau, c.max_distance, c.min_margin)
        else {
            return Ok(None);
        };
        if c.prototypes.trim().is_empty() || c.context.is_empty() {
            return Ok(None);
        }
        let policy = CachePolicy::new(max_distance, min_margin)
            .context("[cache] max_distance and min_margin must be between 0 and 1")?;
        let path = self.resolve_path(&c.prototypes);
        let pin = Sha256Digest::parse(&c.prototypes_sha256).context("[cache] prototypes_sha256")?;
        let cache = crate::prototypes::load(&path, c.max_entries, tau, &pin)
            .context("[cache] prototypes")?;
        Ok(Some(IconResolverConfig {
            cache,
            policy,
            tau,
            context: c.context.clone(),
        }))
    }

    /// Algorithm 1's parameters.
    pub fn plan_params(&self) -> PlanParams {
        PlanParams {
            tile_size: self.engine.tile_size,
            halo_px: f64::from(self.engine.halo_px),
            max_regions: self.engine.max_regions,
            max_area_fraction: self.engine.full_refresh_area_ratio,
            adaptive_min_regions: self.engine.adaptive_min_regions,
            experimental_full_frame: self.engine.experimental_full_frame,
            ..PlanParams::default()
        }
    }

    /// The settle window of effect verification.
    pub fn verify_params(&self) -> VerifyParams {
        VerifyParams {
            timeout: Duration::from_millis(self.engine.verify_timeout_ms),
            poll: Duration::from_millis(self.engine.verify_poll_ms),
        }
    }

    /// How old the snapshot's capture and the accessibility evidence may be when an action is
    /// authorised (ADR-0025).
    pub fn max_snapshot_age(&self) -> Duration {
        Duration::from_millis(self.engine.max_snapshot_age_ms)
    }

    /// The MCP adapter's settings. `--legacy` can only add the legacy profile.
    pub fn mcp_config(&self, legacy_flag: bool) -> McpConfig {
        McpConfig {
            legacy_enabled: self.mcp.legacy_enabled || legacy_flag,
            max_message_bytes: self.mcp.max_message_bytes,
        }
    }

    /// The local policy the engine reads through its `PolicyStore` port.
    pub fn local_policy(&self) -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: self.safety.actuation_enabled,
            include_values: self.safety.include_values,
            allowed_applications: self.safety.allowed_applications.clone(),
            window_classes: self.safety.allowed_window_classes.clone(),
            // The pin names a window only while its file does: the policy store reads it.
            window_pin: None,
            stopped: false,
        }
    }
}

/// The process that draws the frames of hosted (UWP) applications: its windows stand for
/// every hosted application, so a scope on it would scope none of them (ADR-0020).
const FRAME_HOST_EXE: &str = "applicationframehost.exe";

/// Classes of the windows a restriction exists to keep input out of: the desktop and the
/// taskbar, whose owner (`explorer.exe`) is also the owner of the windows to act on.
const SHELL_CLASSES: [&str; 5] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "#32769",
];

/// The problems of `[safety] allowed_window_classes`: an entry the engine could never match
/// would silently leave its application unrestricted, so every mismatch is an error.
fn window_class_problems(s: &SafetySection) -> Vec<String> {
    let mut out = Vec::new();
    for (application, classes) in &s.allowed_window_classes {
        let key = format!("[safety] allowed_window_classes \"{application}\"");
        if *application != application.trim().to_lowercase() || application.is_empty() {
            out.push(format!(
                "{key}: the key must be a lowercase executable name, as the actuator attests it"
            ));
        }
        if application == FRAME_HOST_EXE {
            out.push(format!(
                "{key}: applicationframehost.exe draws the frames of every hosted application                  and is never allowed"
            ));
        }
        if !s.allowed_applications.contains(application) {
            out.push(format!(
                "{key}: the application is not in allowed_applications, so the entry would                  never apply"
            ));
        }
        if classes.is_empty() {
            out.push(format!("{key}: list at least one window class"));
        }
        for class in classes {
            if class.is_empty()
                || class.chars().count() > 256
                || class.chars().any(char::is_control)
            {
                out.push(format!(
                    "{key}: window classes are non-empty names of at most 256 characters"
                ));
            } else if SHELL_CLASSES.contains(&class.as_str()) {
                out.push(format!(
                    "{key}: {class} is the desktop or the taskbar and cannot be allowed"
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../../../docs/thesis/plantillas/configuracion_mvp.toml");

    fn error(text: &str) -> String {
        format!("{:#}", Config::parse(text).unwrap_err())
    }

    #[test]
    fn defaults_equal_the_dossier_template() {
        assert_eq!(Config::parse(TEMPLATE).unwrap(), Config::default());
        assert_eq!(Config::parse("").unwrap(), Config::default());
        assert_eq!(Config::load(None).unwrap(), Config::default());
        let partial = Config::parse("[engine]\ntile_size = 32\n").unwrap();
        assert_eq!(partial.engine.tile_size, 32);
        assert_eq!(partial.engine.halo_px, 32);
        assert_eq!(partial.mcp, McpSection::default());
    }

    #[test]
    fn the_defaults_map_to_the_engine_defaults_and_deny_everything() {
        let config = Config::default();
        // `PlanParams::default()` is the reference planner (the adaptive rule off); the
        // product's configuration adds the rule's measured threshold (ADR-0030) and nothing else.
        assert_eq!(
            config.plan_params(),
            PlanParams {
                adaptive_min_regions: config.engine.adaptive_min_regions,
                ..PlanParams::default()
            }
        );
        assert_eq!(config.verify_params(), VerifyParams::default());
        assert_eq!(config.mcp_config(false), McpConfig::default());
        assert_eq!(config.local_policy(), LocalPolicy::default());
    }

    #[test]
    fn unknown_sections_and_keys_are_rejected() {
        for text in [
            "[engine]\ntile_sise = 64\n",
            "[telemetry]\nenabled = true\n",
            "[safety]\nagent_may_enable_actuation = true\n",
            "top_level = 1\n",
        ] {
            assert!(error(text).contains("unknown"), "{text}: {}", error(text));
        }
    }

    #[test]
    fn invalid_values_are_rejected_with_an_explanation() {
        for (text, expected) in [
            ("[engine]\ntile_size = 0\n", "tile_size"),
            ("[engine]\nmax_regions = 0\n", "max_regions"),
            (
                "[engine]\nfull_refresh_area_ratio = 1.5\n",
                "full_refresh_area_ratio",
            ),
            (
                "[engine]\nfull_refresh_area_ratio = 0.0\n",
                "full_refresh_area_ratio",
            ),
            ("[engine]\nmax_snapshot_age_ms = 0\n", "max_snapshot_age_ms"),
            ("[engine]\nverify_timeout_ms = 5001\n", "verify_timeout_ms"),
            ("[engine]\nverify_timeout_ms = -1\n", "verify_timeout_ms"),
            ("[engine]\nverify_poll_ms = 0\n", "verify_poll_ms"),
            ("[engine]\nverify_poll_ms = 1001\n", "verify_poll_ms"),
            ("[engine]\nmode = \"live\"\n", "mode"),
            ("[engine]\nplatform = \"linux\"\n", "platform"),
            (
                "[engine]\nsnapshot_policy = \"lenient\"\n",
                "snapshot_policy",
            ),
            ("[mcp]\nprofile = \"2025-11-25\"\n", "legacy_enabled"),
            ("[mcp]\ntransport = \"http\"\n", "transport"),
            ("[mcp]\nmax_message_bytes = 100000\n", "max_message_bytes"),
            ("[mcp]\nmax_message_bytes = 10\n", "max_message_bytes"),
            ("[safety]\nallow_destructive = true\n", "allow_destructive"),
            ("[safety]\nstop_file = \"  \"\n", "stop_file"),
            ("[safety]\npurge_file = \"  \"\n", "purge_file"),
            (
                "[safety]\nallow_screenshots_to_client = true\n",
                "screenshots",
            ),
            (
                "[safety]\nallowed_applications = [\" \"]\n",
                "allowed_applications",
            ),
            ("[models]\ndetector_path = \"d.onnx\"\n", "[models]"),
            ("[cache]\npersist = true\n", "persist"),
            ("[cache]\nmax_entries = 0\n", "max_entries"),
            ("[cache]\nprototypes = \"icons.jsonl\"\n", "no defaults"),
            (
                "[cache]\nprototypes_sha256 = \"abc\"\n",
                "prototypes_sha256",
            ),
            ("[cache]\nmax_distance = 0.1\n", "no defaults"),
            ("[cache]\ncontext = \"app\"\n", "no defaults"),
            ("[cache]\ngradient_tau = 0\n", "gradient_tau"),
            ("[cache]\ngradient_tau = 256\n", "gradient_tau"),
            ("[cache]\nmin_margin = 1.5\n", "min_margin"),
            ("[cache]\ncontext = \" \"\n", "context"),
            ("[engine]\ntile_size = \"big\"\n", "tile_size"),
        ] {
            let message = error(text);
            assert!(message.contains(expected), "{text}: {message}");
        }
        // Every problem is reported at once.
        let message = error("[engine]\ntile_size = 0\n[cache]\npersist = true\n");
        assert!(message.contains("tile_size") && message.contains("persist"));
    }

    #[test]
    fn the_operator_can_enable_what_the_engine_honours() {
        let config = Config::parse(
            "[engine]\nmode = \"native\"\nhalo_px = 0\n\
             [mcp]\nlegacy_enabled = true\nmax_message_bytes = 4096\n\
             [safety]\nactuation_enabled = true\ninclude_values = true\n\
             allowed_applications = [\"notepad.exe\"]\n",
        )
        .unwrap();
        assert_eq!(config.engine.mode, EngineMode::Native);
        assert!(config.plan_params().halo_px.abs() < f64::EPSILON);
        assert_eq!(
            config.mcp_config(false),
            McpConfig {
                legacy_enabled: true,
                max_message_bytes: 4096
            }
        );
        let policy = config.local_policy();
        assert!(policy.actuation_enabled && policy.include_values && !policy.stopped);
        assert_eq!(policy.allowed_applications, vec!["notepad.exe".to_owned()]);
    }

    #[test]
    fn the_verification_window_is_configurable_within_bounds() {
        let config =
            Config::parse("[engine]\nverify_timeout_ms = 0\nverify_poll_ms = 1\n").unwrap();
        assert_eq!(
            config.verify_params(),
            VerifyParams {
                timeout: Duration::ZERO,
                poll: Duration::from_millis(1),
            }
        );
        let config =
            Config::parse("[engine]\nverify_timeout_ms = 5000\nverify_poll_ms = 1000\n").unwrap();
        assert_eq!(config.verify_params().timeout, Duration::from_secs(5));
    }

    #[test]
    fn the_snapshot_window_is_ten_seconds_unless_the_operator_sets_it() {
        assert_eq!(
            Config::default().max_snapshot_age(),
            Duration::from_secs(10)
        );
        let config = Config::parse("[engine]\nmax_snapshot_age_ms = 2500\n").unwrap();
        assert_eq!(config.max_snapshot_age(), Duration::from_millis(2500));
    }

    #[test]
    fn the_candidate_search_is_the_scan_unless_the_operator_asks_for_the_index() {
        assert_eq!(
            Config::default().engine.candidate_search,
            CandidateSearchKind::Scan
        );
        let config = Config::parse(
            "[engine]
candidate_search = \"rstar\"
",
        )
        .unwrap();
        assert_eq!(
            config.engine.candidate_search.search(),
            CandidateSearch::RStar
        );
        assert!(
            Config::parse(
                "[engine]
candidate_search = \"tree\"
"
            )
            .is_err()
        );
    }

    #[test]
    fn observation_stages_run_concurrently_unless_the_operator_turns_it_off() {
        assert!(Config::default().engine.concurrent_stages);
        let config = Config::parse(
            "[engine]
concurrent_stages = false
",
        )
        .unwrap();
        assert!(!config.engine.concurrent_stages);
    }

    #[test]
    fn inc_07_the_adaptive_rule_reaches_the_planner_and_zero_switches_it_off() {
        // ADR-0030: the rule's threshold travels from `[engine]` to Algorithm 1; 0 is off and
        // an absurd value is refused like the other plan parameters.
        assert_eq!(Config::default().engine.adaptive_min_regions, 2);
        assert_eq!(Config::default().plan_params().adaptive_min_regions, 2);
        let off = Config::parse(
            "[engine]
adaptive_min_regions = 0
",
        )
        .unwrap();
        assert_eq!(off.plan_params().adaptive_min_regions, 0);
        let three = Config::parse(
            "[engine]
adaptive_min_regions = 3
",
        )
        .unwrap();
        assert_eq!(three.plan_params().adaptive_min_regions, 3);
        assert!(
            Config::parse(
                "[engine]
adaptive_min_regions = 5000
"
            )
            .is_err()
        );
    }

    #[test]
    fn the_full_frame_baseline_is_off_by_default_and_is_announced_when_on() {
        // B0 (ADR-0024): an experiment switch. Off, it adds no note and plans as the product
        // does; on, the note says it is not a production mode and the planner is forced.
        let default = Config::default();
        assert!(!default.engine.experimental_full_frame);
        assert!(!default.plan_params().experimental_full_frame);
        assert!(default.notes().iter().all(|n| !n.contains("EXPERIMENTAL")));

        let baseline = Config::parse("[engine]\nexperimental_full_frame = true\n").unwrap();
        assert!(baseline.plan_params().experimental_full_frame);
        assert!(
            baseline
                .notes()
                .iter()
                .any(|n| n.contains("experimental_full_frame = true")
                    && n.contains("not a production mode"))
        );
    }

    #[test]
    fn the_stop_file_is_optional_and_relative_to_the_configuration() {
        assert_eq!(Config::default().stop_file(), None);
        let mut config = Config::parse("[safety]\nstop_file = \"STOP\"\n").unwrap();
        assert_eq!(config.stop_file(), Some(PathBuf::from("STOP")));
        config.base_dir = Some(PathBuf::from("operator"));
        assert_eq!(config.stop_file(), Some(Path::new("operator").join("STOP")));
        // It never shows in the policy itself: the stop is read when the engine acts.
        assert!(!config.local_policy().stopped);
    }

    #[test]
    fn the_purge_file_is_optional_and_relative_to_the_configuration() {
        assert_eq!(Config::default().purge_file(), None);
        let mut config = Config::parse("[safety]\npurge_file = \"PURGE\"\n").unwrap();
        assert_eq!(config.purge_file(), Some(PathBuf::from("PURGE")));
        config.base_dir = Some(PathBuf::from("operator"));
        assert_eq!(
            config.purge_file(),
            Some(Path::new("operator").join("PURGE"))
        );
        // A purge request is not a stop: the policy the engine reads does not change.
        assert_eq!(config.local_policy(), Config::default().local_policy());
    }

    const EXPLORER_SCOPE: &str = "[safety]
allowed_applications = [\"explorer.exe\"]
                                  allowed_window_classes = { \"explorer.exe\" = [\"CabinetWClass\"] }
";

    #[test]
    fn no_window_scope_is_configured_by_default() {
        let config = Config::default();
        assert!(config.safety.allowed_window_classes.is_empty());
        assert_eq!(config.window_pin_file(), None);
        let policy = config.local_policy();
        assert!(policy.window_classes.is_empty() && policy.window_pin.is_none());
    }

    #[test]
    fn the_operator_can_scope_an_application_to_window_classes() {
        let config = Config::parse(EXPLORER_SCOPE).unwrap();
        assert_eq!(
            config.local_policy().window_classes,
            BTreeMap::from([("explorer.exe".to_owned(), vec!["CabinetWClass".to_owned()])])
        );
        // The pin names a window only while its file does, so the policy starts without one.
        assert_eq!(config.local_policy().window_pin, None);
    }

    #[test]
    fn the_window_pin_file_is_optional_and_relative_to_the_configuration() {
        let mut config = Config::parse(
            "[safety]
window_pin_file = \"window.pin\"
",
        )
        .unwrap();
        assert_eq!(config.window_pin_file(), Some(PathBuf::from("window.pin")));
        config.base_dir = Some(PathBuf::from("operator"));
        assert_eq!(
            config.window_pin_file(),
            Some(Path::new("operator").join("window.pin"))
        );
        assert!(
            error(
                "[safety]
window_pin_file = \" \"
"
            )
            .contains("window_pin_file")
        );
    }

    /// An entry that could never match would leave its application unrestricted without
    /// saying so, and some classes are exactly what the restriction keeps input out of.
    #[test]
    fn window_class_entries_that_cannot_work_are_rejected() {
        let scoped = |entry: &str| {
            format!(
                "[safety]
allowed_applications = [\"explorer.exe\", \"applicationframehost.exe\"]
                 allowed_window_classes = {{ {entry} }}
"
            )
        };
        for (entry, expected) in [
            ("\"Explorer.exe\" = [\"CabinetWClass\"]", "lowercase"),
            (
                "\"notepad.exe\" = [\"Notepad\"]",
                "not in allowed_applications",
            ),
            ("\"explorer.exe\" = []", "at least one window class"),
            ("\"explorer.exe\" = [\"\"]", "non-empty"),
            (
                "\"applicationframehost.exe\" = [\"ApplicationFrameWindow\"]",
                "never allowed",
            ),
            ("\"explorer.exe\" = [\"Progman\"]", "desktop or the taskbar"),
            (
                "\"explorer.exe\" = [\"CabinetWClass\", \"Shell_TrayWnd\"]",
                "taskbar",
            ),
            ("\"explorer.exe\" = [\"WorkerW\"]", "desktop"),
        ] {
            let message = error(&scoped(entry));
            assert!(message.contains(expected), "{entry}: {message}");
        }
        assert!(Config::parse(&scoped("\"explorer.exe\" = [\"CabinetWClass\"]")).is_ok());
    }

    const DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn accessibility_is_off_unless_the_operator_chooses_uia() {
        assert_eq!(
            Config::default().models.accessibility,
            AccessibilityKind::None
        );
        let config = Config::parse("[models]\naccessibility = \"uia\"\n").unwrap();
        assert_eq!(config.models.accessibility, AccessibilityKind::Uia);
        assert!(Config::parse("[models]\naccessibility = \"ax\"\n").is_err());
    }

    #[test]
    fn models_default_to_none_and_are_pinned_when_configured() {
        let models = Config::default().models;
        assert!(!models.has_detector());
        assert_eq!(models.ocr, OcrKind::None);
        assert_eq!(models.ocr_language, "es-ES");

        let config = Config::parse(&format!(
            "[models]\ndetector_manifest = \"models/manifest.json\"\n\
             detector_manifest_sha256 = \"{DIGEST}\"\n\
             onnxruntime_path = \"C:/onnxruntime/onnxruntime.dll\"\n\
             onnxruntime_sha256 = \"{}\"\n\
             ocr = \"winrt\"\nocr_language = \"es-MX\"\n",
            DIGEST.to_uppercase()
        ))
        .unwrap();
        assert!(config.models.has_detector());
        assert_eq!(config.models.ocr, OcrKind::Winrt);
        assert_eq!(config.models.ocr_language, "es-MX");
    }

    const DETECTOR: &str = "[models]\ndetector_manifest = \"m.json\"\n\
                            detector_manifest_sha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"\n\
                            onnxruntime_path = \"o.dll\"\n\
                            onnxruntime_sha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"\n";

    #[test]
    fn the_detector_runs_on_the_cpu_with_onnx_runtime_threads_unless_configured() {
        let models = Config::parse(DETECTOR).unwrap().models;
        assert_eq!(models.detector_execution_provider, ProviderKind::Cpu);
        assert_eq!(models.detector_threads, 0);

        let models = Config::parse(&format!(
            "{DETECTOR}detector_execution_provider = \"directml\"\n\
             directml_path = \"runtime/DirectML.dll\"\ndirectml_sha256 = \"{DIGEST}\"\n\
             detector_threads = 8\n"
        ))
        .unwrap()
        .models;
        assert_eq!(models.detector_execution_provider, ProviderKind::Directml);
        assert_eq!(models.directml_path, "runtime/DirectML.dll");
        assert_eq!(models.detector_threads, 8);
    }

    #[test]
    fn half_configured_or_unpinned_models_are_rejected() {
        for (text, expected) in [
            (
                "[models]\ndetector_manifest = \"m.json\"\n".to_owned(),
                "must be set together",
            ),
            (
                format!(
                    "[models]\ndetector_manifest = \"m.json\"\ndetector_manifest_sha256 = \
                     \"{DIGEST}\"\nonnxruntime_path = \"o.dll\"\nonnxruntime_sha256 = \"abc\"\n"
                ),
                "SHA-256",
            ),
            ("[models]\ndetector_sha256 = \"x\"\n".to_owned(), "manifest"),
            ("[models]\nocr_path = \"rec.onnx\"\n".to_owned(), "ocr_path"),
            ("[models]\nocr = \"tesseract\"\n".to_owned(), "ocr"),
            ("[models]\nocr_language = \"\"\n".to_owned(), "ocr_language"),
            (
                "[models]\nocr_language = \"es ES\"\n".to_owned(),
                "ocr_language",
            ),
            (
                "[models]\nocr_language = \"-es\"\n".to_owned(),
                "ocr_language",
            ),
            (
                "[models]\ndetector_execution_provider = \"cuda\"\n".to_owned(),
                "detector_execution_provider",
            ),
            (
                format!("{DETECTOR}detector_execution_provider = \"directml\"\n"),
                "never falls back to the CPU",
            ),
            (
                format!(
                    "{DETECTOR}detector_execution_provider = \"directml\"\n\
                     directml_path = \"DirectML.dll\"\ndirectml_sha256 = \"abc\"\n"
                ),
                "directml_sha256",
            ),
            (
                format!("{DETECTOR}directml_path = \"DirectML.dll\"\n"),
                "only used with",
            ),
            (
                "[models]\ndetector_threads = 65\n".to_owned(),
                "detector_threads",
            ),
            (
                "[models]\ndetector_threads = -1\n".to_owned(),
                "detector_threads",
            ),
        ] {
            let message = error(&text);
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    #[test]
    fn the_legacy_flag_only_adds_the_legacy_profile() {
        let config = Config::default();
        assert_eq!(config.mcp_config(true), McpConfig::with_legacy());
        assert_eq!(config.local_policy(), LocalPolicy::default());
    }

    /// A resolver section pinned by `digest` (64 hexadecimal characters).
    fn resolver(digest: &str) -> String {
        format!(
            "[cache]\nprototypes = \"icons.jsonl\"\nprototypes_sha256 = \"{digest}\"\n\
             context = \"notepad.exe\"\ngradient_tau = 3\nmax_distance = 0.1\nmin_margin = 0.1\n"
        )
    }

    const NO_PIN: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn the_icon_resolver_is_off_unless_fully_configured() {
        assert_eq!(Config::default().icon_resolver().unwrap(), None);
        let config = Config::parse(&resolver(NO_PIN)).unwrap();
        assert_eq!(config.cache.context, "notepad.exe");
        // The file is read relative to the configuration file and refused when missing.
        let config = Config {
            base_dir: Some(PathBuf::from("definitely/not/here")),
            ..config
        };
        let message = format!("{:#}", config.icon_resolver().unwrap_err());
        assert!(
            message.contains("cannot load") && message.contains("here"),
            "{message}"
        );
        // The digest is part of the set: without it, or malformed, the file is refused.
        let unpinned = resolver(NO_PIN).replace(&format!("prototypes_sha256 = \"{NO_PIN}\"\n"), "");
        let message = format!("{:#}", Config::parse(&unpinned).unwrap_err());
        assert!(message.contains("prototypes_sha256"), "{message}");
        let message = format!("{:#}", Config::parse(&resolver("12345")).unwrap_err());
        assert!(message.contains("64 hexadecimal"), "{message}");
    }

    /// The operator's directory with a reviewed prototypes file and a configuration that
    /// pins it. Returns the directory and the file's text.
    fn reviewed_prototypes(tag: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("ommatix-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let grid: Vec<String> = [0, 90, 90, 0, 0, 90, 90, 0, 0]
            .repeat(8)
            .iter()
            .map(ToString::to_string)
            .collect();
        let prototypes = format!(
            "{{\"label\":\"Save\",\"context\":\"notepad.exe\",\"grid\":[{}]}}\n",
            grid.join(",")
        );
        std::fs::write(dir.join("icons.jsonl"), &prototypes).unwrap();
        let digest = Sha256Digest::of(prototypes.as_bytes()).to_hex();
        std::fs::write(dir.join("ommatix.toml"), resolver(&digest)).unwrap();
        (dir, prototypes)
    }

    #[test]
    fn a_curated_prototypes_file_configures_the_resolver() {
        let (dir, _) = reviewed_prototypes("resolver");
        let config = Config::load(Some(&dir.join("ommatix.toml"))).unwrap();
        let resolver = config.icon_resolver().unwrap().expect("configured");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(resolver.context, "notepad.exe");
        assert_eq!(resolver.tau, 3);
        assert_eq!(resolver.cache.prototypes().len(), 1);
        assert_eq!(resolver.policy, CachePolicy::new(0.1, 0.1).unwrap());
    }

    /// SEC-06, end to end through the configuration: a prototype added to the reviewed
    /// file after the operator pinned it stops the engine before any icon is labelled.
    #[test]
    fn sec_06_a_poisoned_prototypes_file_stops_the_resolver() {
        let (dir, prototypes) = reviewed_prototypes("poisoned");
        let config = Config::load(Some(&dir.join("ommatix.toml"))).unwrap();
        assert!(config.icon_resolver().unwrap().is_some());
        let extra = prototypes.replace("Save", "Delete everything");
        std::fs::write(dir.join("icons.jsonl"), format!("{prototypes}{extra}")).unwrap();
        let message = format!("{:#}", config.icon_resolver().unwrap_err());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(message.contains("SHA-256 mismatch"), "{message}");
        assert!(message.contains("[cache] prototypes"), "{message}");
    }

    #[test]
    fn model_paths_resolve_like_the_prototypes_file() {
        let config = Config {
            base_dir: Some(PathBuf::from("ops")),
            ..Config::default()
        };
        assert_eq!(
            config.resolve_path("models/manifest.json"),
            Path::new("ops").join("models/manifest.json")
        );
        let absolute = std::env::current_dir().unwrap().join("onnxruntime.dll");
        assert_eq!(config.resolve_path(absolute.to_str().unwrap()), absolute);
        assert_eq!(
            Config::default().resolve_path("manifest.json"),
            PathBuf::from("manifest.json")
        );
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let path = Path::new("definitely/not/here.toml");
        let message = format!("{:#}", Config::load(Some(path)).unwrap_err());
        assert!(message.contains("cannot read"), "{message}");
    }
}
