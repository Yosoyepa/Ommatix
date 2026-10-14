//! The operator's configuration file, shaped like the dossier template
//! `docs/thesis/plantillas/configuracion_mvp.toml`.
//!
//! Every field is optional and defaults to the template's value; unknown sections and keys
//! are rejected. [`Config::validate`] rejects values this build cannot honour (another
//! transport or profile, model paths, a persistent cache, permissions it cannot enforce)
//! instead of accepting them silently, and [`Config::notes`] lists the accepted values that
//! the engine does not enforce yet, for stderr.
//!
//! The file is the operator's: nothing the agent sends can change it, and no CLI flag
//! loosens `[safety]`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use ommatix_adapter_mcp::config::MIN_MAX_MESSAGE_BYTES;
use ommatix_adapter_mcp::{DEFAULT_MAX_MESSAGE_BYTES, MODERN_PROTOCOL_VERSION, McpConfig};
use ommatix_application::LocalPolicy;
use ommatix_domain::incremental::PlanParams;
use serde::Deserialize;

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
}

/// `[models]`: weights chosen in the S0 spike. Empty until then.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ModelsSection {
    pub detector_path: String,
    pub detector_sha256: String,
    pub ocr_path: String,
    pub ocr_sha256: String,
}

/// `[cache]`: the icon cache (Algorithm 3A).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CacheSection {
    pub max_entries: u32,
    pub persist: bool,
}

impl Default for CacheSection {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            persist: false,
        }
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
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
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
            "[mcp] max_message_bytes must be between 1024 and 65536 (the contract's 64 KiB \
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
            !s.allow_destructive,
            "[safety] allow_destructive = true cannot be honoured: this build does not \
             classify destructive actions",
        );
        check(
            !s.allow_screenshots_to_client,
            "[safety] allow_screenshots_to_client = true cannot be honoured: the contract \
             sends no screenshots",
        );

        let models = &self.models;
        check(
            [
                &models.detector_path,
                &models.detector_sha256,
                &models.ocr_path,
                &models.ocr_sha256,
            ]
            .iter()
            .all(|v| v.is_empty()),
            "[models]: this build loads no model weights (they are chosen in the S0 spike); \
             leave detector_path, detector_sha256, ocr_path and ocr_sha256 empty",
        );

        check(
            self.cache.max_entries >= 1,
            "[cache] max_entries must be at least 1",
        );
        check(
            !self.cache.persist,
            "[cache] persist = true cannot be honoured: the icon cache lives in memory only",
        );
        out
    }

    /// Accepted values that the engine does not enforce yet, to report on stderr.
    pub fn notes(&self) -> Vec<String> {
        vec![
            format!(
                "[engine] max_snapshot_age_ms = {} is not enforced yet; every page reports \
                 its age_ms",
                self.engine.max_snapshot_age_ms
            ),
            format!(
                "[cache] max_entries = {} is not used yet: the engine has no icon cache wired",
                self.cache.max_entries
            ),
        ]
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
    fn the_legacy_flag_only_adds_the_legacy_profile() {
        let config = Config::default();
        assert_eq!(config.mcp_config(true), McpConfig::with_legacy());
        assert_eq!(config.local_policy(), LocalPolicy::default());
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let path = Path::new("definitely/not/here.toml");
        let message = format!("{:#}", Config::load(Some(path)).unwrap_err());
        assert!(message.contains("cannot read"), "{message}");
    }
}
