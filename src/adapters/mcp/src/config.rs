//! The `[mcp]` section of the configuration (`plantillas/configuracion_mvp.toml`).

/// Default and contract limit for one message in either direction (chapter 07 §7.2).
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// The smallest accepted `max_message_bytes`: below it even an `execute_action` result
/// could not be sent. Smaller configured values are raised to it.
pub const MIN_MAX_MESSAGE_BYTES: usize = 1024;

/// MCP adapter settings. The modern profile (2026-07-28) is always served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpConfig {
    /// Also serve the legacy 2025-11-25 profile (`initialize`/`initialized`). Off by
    /// default; when off, `initialize` gets the unsupported-version error and discovery
    /// advertises only 2026-07-28.
    pub legacy_enabled: bool,
    /// Largest message accepted on input and emitted on output, in bytes, excluding the
    /// line terminator.
    pub max_message_bytes: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            legacy_enabled: false,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        }
    }
}

impl McpConfig {
    /// Default configuration with the legacy profile enabled.
    pub fn with_legacy() -> Self {
        Self {
            legacy_enabled: true,
            ..Self::default()
        }
    }

    /// The effective message limit.
    pub fn message_limit(&self) -> usize {
        self.max_message_bytes.max(MIN_MAX_MESSAGE_BYTES)
    }
}
