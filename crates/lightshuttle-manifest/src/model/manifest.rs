//! Top-level manifest type.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::dashboard::DashboardConfig;
use super::export::ExportConfig;
use super::observability::ObservabilityConfig;
use super::resource::ResourceKind;

/// Top-level `lightshuttle.yml` model.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Optional manifest version discriminator. Absent means `v0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightshuttle: Option<Version>,

    /// Project metadata.
    pub project: Project,

    /// Optional local dashboard settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<DashboardConfig>,

    /// Optional observability settings (collector toggle, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityConfig>,

    /// Optional export settings (per-target overrides for
    /// `lightshuttle export`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportConfig>,

    /// Declared resources, keyed by name.
    pub resources: IndexMap<String, ResourceKind>,
}

/// Manifest specification version.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum Version {
    /// The `v0` specification.
    #[serde(rename = "v0")]
    V0,
}

/// Project section of the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Project name, used as a prefix in runtime resource names.
    pub name: String,

    /// Free-form version label. Informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Free-form description shown in the dashboard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
