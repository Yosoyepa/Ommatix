//! The operator's configuration file, shaped like the dossier template
//! `docs/thesis/plantillas/configuracion_mvp.toml`.
//!
//! Every field is optional and defaults to the template's value; unknown sections and keys
//! are rejected. [`Config::validate`] rejects values this build cannot honour (another
//! transport or profile, weights outside a model manifest, unpinned or half-configured
//! models, a persistent cache, permissions it cannot enforce)
//! instead of accepting them silently, and [`Config::notes`] lists the accepted values that
//! the engine does not enforce yet, for stderr.
//!
//! `[cache]` may add, beyond the template, a curated prototypes file for Algorithm 3A and
//! the thresholds to use it with. The thesis calibrates those thresholds (H4), so none has
//! a default: the resolver is on only when the file, the context, `gradient_tau`,
//! `max_distance` and `min_margin` are all set, and any of them without the others is
//! refused.
//!
//! The file is the operator's: nothing the agent sends can change it, and no CLI flag
//! loosens `[safety]`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ommatix_adapter_mcp::config::MIN_MAX_MESSAGE_BYTES;
use ommatix_adapter_mcp::{DEFAULT_MAX_MESSAGE_BYTES, MODERN_PROTOCOL_VERSION, McpConfig};
use ommatix_adapter_vision::model::integrity::Sha256Digest;
use ommatix_application::{IconResolverConfig, LocalPolicy, VerifyParams};
use ommatix_domain::appearance::CachePolicy;
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
    pub max_snapshot_age_ms: u64,
    /// How long after delivering an action the engine looks for its visual effect
    /// (SDD §2.9, the deadline of BDD-29). 0: a single check right after delivery.
    pub verify_timeout_ms: u64,
    /// The wait between two verification captures inside that window.
    pub verify_poll_ms: u64,
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
            max_snapshot_age_ms: 250,
            verify_timeout_ms: 500,
            verify_poll_ms: 50,
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
                    && !c.context.is_empty()
                    && c.gradient_tau.is_some()
                    && c.max_distance.is_some()
                    && c.min_margin.is_some(),
                "[cache] the icon resolver needs all of prototypes, context, gradient_tau, \
                 max_distance and min_margin (the thesis calibrates them; there are no \
                 defaults)",
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
            !s.allow_destructive,
            "[safety] allow_destructive = true cannot be honoured: this build does not \
             classify destructive actions",
        );
        check(
            !s.allow_screenshots_to_client,
            "[safety] allow_screenshots_to_client = true cannot be honoured: the contract \
             sends no screenshots",
        );

        out.extend(self.models.problems());
        out.extend(self.cache.problems());
        out
    }

    /// Accepted values that the engine does not enforce yet, to report on stderr.
    pub fn notes(&self) -> Vec<String> {
        vec![format!(
            "[engine] max_snapshot_age_ms = {} is not enforced yet; every page reports \
                 its age_ms",
            self.engine.max_snapshot_age_ms
        )]
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
        let cache =
            crate::prototypes::load(&path, c.max_entries, tau).context("[cache] prototypes")?;
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
            stopped: false,
        }
    }
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
        assert_eq!(config.plan_params(), PlanParams::default());
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
    fn the_stop_file_is_optional_and_relative_to_the_configuration() {
        assert_eq!(Config::default().stop_file(), None);
        let mut config = Config::parse("[safety]\nstop_file = \"STOP\"\n").unwrap();
        assert_eq!(config.stop_file(), Some(PathBuf::from("STOP")));
        config.base_dir = Some(PathBuf::from("operator"));
        assert_eq!(config.stop_file(), Some(Path::new("operator").join("STOP")));
        // It never shows in the policy itself: the stop is read when the engine acts.
        assert!(!config.local_policy().stopped);
    }

    const DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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

    const RESOLVER: &str = "[cache]\nprototypes = \"icons.jsonl\"\ncontext = \"notepad.exe\"\n\
                            gradient_tau = 3\nmax_distance = 0.1\nmin_margin = 0.1\n";

    #[test]
    fn the_icon_resolver_is_off_unless_fully_configured() {
        assert_eq!(Config::default().icon_resolver().unwrap(), None);
        let config = Config::parse(RESOLVER).unwrap();
        assert_eq!(config.cache.context, "notepad.exe");
        // The file is read relative to the configuration file and refused when missing.
        let config = Config {
            base_dir: Some(PathBuf::from("definitely/not/here")),
            ..config
        };
        let message = format!("{:#}", config.icon_resolver().unwrap_err());
        assert!(
            message.contains("cannot read") && message.contains("here"),
            "{message}"
        );
    }

    #[test]
    fn a_curated_prototypes_file_configures_the_resolver() {
        let dir = std::env::temp_dir().join(format!("ommatix-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let grid: Vec<String> = [0, 90, 90, 0, 0, 90, 90, 0, 0]
            .repeat(8)
            .iter()
            .map(ToString::to_string)
            .collect();
        std::fs::write(
            dir.join("icons.jsonl"),
            format!(
                "{{\"label\":\"Save\",\"context\":\"notepad.exe\",\"grid\":[{}]}}\n",
                grid.join(",")
            ),
        )
        .unwrap();
        std::fs::write(dir.join("ommatix.toml"), RESOLVER).unwrap();
        let config = Config::load(Some(&dir.join("ommatix.toml"))).unwrap();
        let resolver = config.icon_resolver().unwrap().expect("configured");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(resolver.context, "notepad.exe");
        assert_eq!(resolver.tau, 3);
        assert_eq!(resolver.cache.prototypes().len(), 1);
        assert_eq!(resolver.policy, CachePolicy::new(0.1, 0.1).unwrap());
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
