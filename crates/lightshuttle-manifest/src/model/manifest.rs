//! Top-level manifest type.
//!
//! [`Manifest`] is the root of the in-memory representation of a parsed
//! `lightshuttle.yml` file. Construct it via [`Manifest::parse`] for
//! the full parse-and-validate path, or deserialise it directly and call
//! [`Manifest::validate`] manually when building one programmatically.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::dashboard::DashboardConfig;
use super::export::ExportConfig;
use super::observability::ObservabilityConfig;
use super::resource::ResourceKind;

/// Top-level `lightshuttle.yml` model.
///
/// Parse a YAML string with [`Manifest::parse`], which runs both structural
/// decoding and semantic validation in one step.
///
/// ```rust,no_run
/// use lightshuttle_manifest::Manifest;
///
/// let yaml = r#"
/// project:
///   name: my-app
/// resources:
///   cache:
///     redis:
///       version: "7"
/// "#;
///
/// let manifest = Manifest::parse(yaml).unwrap();
/// assert_eq!(manifest.project.name, "my-app");
/// assert_eq!(manifest.resources.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Optional manifest version discriminator. Absent means [`Version::V0`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightshuttle: Option<Version>,

    /// Project metadata (name, optional version label, optional description).
    pub project: Project,

    /// Optional settings for the local control-plane HTTP server.
    ///
    /// When absent the dashboard uses a random free port at startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<DashboardConfig>,

    /// Optional observability settings (OpenTelemetry collector toggle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityConfig>,

    /// Optional per-target overrides consumed by `lightshuttle export`.
    ///
    /// See [`ExportConfig`] for the supported targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportConfig>,

    /// Declared resources, keyed by their manifest name.
    ///
    /// Each value is a [`ResourceKind`] variant that carries the
    /// kind-specific configuration. The map preserves insertion order.
    pub resources: IndexMap<String, ResourceKind>,
}

/// Manifest specification version discriminator.
///
/// Carried by the optional top-level `lightshuttle` key in the YAML file.
/// Absent means `v0`. Future specification revisions will introduce new
/// variants here so tooling can reject manifests it does not understand.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum Version {
    /// The `v0` specification (current).
    #[serde(rename = "v0")]
    V0,
}

/// Project metadata, corresponding to the `project:` section of the manifest.
///
/// The `name` field must match the pattern `^[a-z][a-z0-9_-]{0,31}$` and is
/// validated by [`Manifest::validate`]. It is used by the runtime as a prefix
/// for container and network names, and by `lightshuttle-export` as the
/// default Helm chart name and Kubernetes namespace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Project name. Must match `^[a-z][a-z0-9_-]{0,31}$`.
    ///
    /// Used as a prefix for all runtime resource names (containers, networks,
    /// volumes) so it must be stable across machines.
    pub name: String,

    /// Free-form version label. Informational only; not validated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Free-form description displayed in the local dashboard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
