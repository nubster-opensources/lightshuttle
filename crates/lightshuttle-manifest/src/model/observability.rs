//! Optional `observability` section of the manifest.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level observability settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// OpenTelemetry collector toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otel: Option<OtelConfig>,
}

/// Per-feature `OTel` collector toggle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct OtelConfig {
    /// When `Some(false)`, `lightshuttle up` skips both the bundled
    /// collector and the env injection. Absent or `Some(true)` keeps
    /// the default-on behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
