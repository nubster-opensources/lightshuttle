//! Optional `observability` section of the manifest.
//!
//! [`ObservabilityConfig`] maps to the `observability:` top-level key in
//! `lightshuttle.yml` and is stored in [`Manifest::observability`].
//! When the section is absent the bundled OpenTelemetry collector is
//! enabled by default.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level observability settings, corresponding to the `observability:`
/// section in `lightshuttle.yml`.
///
/// Currently only controls the bundled OpenTelemetry collector via
/// [`OtelConfig`]. More toggles (tracing, profiling) may be added in
/// future specification revisions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// OpenTelemetry collector configuration.
    ///
    /// `None` preserves the default-on behaviour (the bundled collector
    /// starts alongside the project resources).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otel: Option<OtelConfig>,
}

/// Toggle for the bundled OpenTelemetry collector.
///
/// Nested under [`ObservabilityConfig::otel`] in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct OtelConfig {
    /// Whether the bundled OpenTelemetry collector is active.
    ///
    /// - `None` or `Some(true)`: the collector starts and OTLP environment
    ///   variables are injected into every container.
    /// - `Some(false)`: `lightshuttle up` skips the collector and does not
    ///   inject any OTLP variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
