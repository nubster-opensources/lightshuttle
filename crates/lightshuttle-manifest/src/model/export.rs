//! Optional `export` section of the manifest.
//!
//! Carries per-target overrides consumed by `lightshuttle export`. The
//! section is purely structural: it holds raw optional values only.
//! Defaults (chart name from the project, namespace, replica counts) are
//! resolved later, during the lowering step in the `lightshuttle-export`
//! crate, so the manifest layer never owns target semantics.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level `export` settings, one optional sub-table per target.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExportConfig {
    /// Overrides for the `docker-compose` target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<ComposeExport>,

    /// Overrides for the Kubernetes manifests target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesExport>,

    /// Overrides for the Helm chart target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm: Option<HelmExport>,
}

/// Overrides applied when exporting to `docker-compose`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComposeExport {
    /// Per-resource overrides, keyed by manifest resource name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, ComposeResourceExport>,
}

/// Per-resource overrides for the `docker-compose` target.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComposeResourceExport {
    /// When `Some(false)`, the resource is omitted from the export.
    /// Absent or `Some(true)` keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Overrides applied when exporting to Kubernetes manifests.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesExport {
    /// Target namespace. Defaults to the project name during lowering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Default image pull policy for every resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<ImagePullPolicy>,

    /// Default replica count for every resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Per-resource overrides, keyed by manifest resource name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, KubernetesResourceExport>,
}

/// Per-resource overrides for the Kubernetes target.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesResourceExport {
    /// When `Some(false)`, the resource is omitted from the export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Replica count override for this resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Image pull policy override for this resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<ImagePullPolicy>,
}

/// Overrides applied when exporting to a Helm chart.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct HelmExport {
    /// Chart name. Defaults to the project name during lowering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_name: Option<String>,

    /// Chart version. Defaults to the project version, else `0.1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_version: Option<String>,

    /// Default replica count exposed through chart values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Per-resource overrides, keyed by manifest resource name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, HelmResourceExport>,
}

/// Per-resource overrides for the Helm target.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct HelmResourceExport {
    /// When `Some(false)`, the resource is omitted from the chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Replica count override for this resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
}

/// Kubernetes image pull policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub enum ImagePullPolicy {
    /// Always pull the image.
    Always,
    /// Pull only when the image is not present locally.
    #[default]
    IfNotPresent,
    /// Never pull; the image must already be present.
    Never,
}
