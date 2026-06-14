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

/// Top-level `export` settings, one optional sub-table per export target.
///
/// Stored in [`crate::Manifest::export`]. The section is purely structural: it
/// carries raw optional values only. Defaults such as the chart name, the
/// Kubernetes namespace, and replica counts are resolved during the lowering
/// step in `lightshuttle-export`, so this crate never owns export semantics.
///
/// All resource keys in sub-tables are validated against the manifest's
/// declared resources by [`crate::Manifest::validate`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExportConfig {
    /// Per-resource overrides for the `docker-compose` export target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<ComposeExport>,

    /// Per-resource overrides for the raw Kubernetes manifests target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesExport>,

    /// Per-resource overrides for the Helm chart target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm: Option<HelmExport>,
}

/// Global overrides for the `docker-compose` export target.
///
/// Nested under [`ExportConfig::compose`]. All resource keys in `resources`
/// must reference a name declared in [`crate::Manifest::resources`], enforced by
/// [`crate::Manifest::validate`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComposeExport {
    /// Per-resource overrides, keyed by manifest resource name.
    ///
    /// Resources not listed here receive the default behaviour (included,
    /// standard image naming).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, ComposeResourceExport>,
}

/// Per-resource overrides for the `docker-compose` export target.
///
/// Used as the value type in [`ComposeExport::resources`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComposeResourceExport {
    /// Whether this resource is included in the export.
    ///
    /// `None` or `Some(true)` includes the resource. `Some(false)` omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Global overrides for the raw Kubernetes manifests export target.
///
/// Nested under [`ExportConfig::kubernetes`]. Defaults (namespace, replica
/// count, pull policy) are resolved by `lightshuttle-export` during
/// lowering.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesExport {
    /// Target Kubernetes namespace.
    ///
    /// Defaults to the project name when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Default image pull policy applied to every resource.
    ///
    /// See [`ImagePullPolicy`] for accepted values. Defaults to
    /// [`ImagePullPolicy::IfNotPresent`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<ImagePullPolicy>,

    /// Default replica count for every resource that supports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Per-resource overrides, keyed by manifest resource name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, KubernetesResourceExport>,
}

/// Per-resource overrides for the Kubernetes manifests export target.
///
/// Used as the value type in [`KubernetesExport::resources`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesResourceExport {
    /// Whether this resource is included in the export.
    ///
    /// `None` or `Some(true)` includes the resource. `Some(false)` omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Replica count override for this specific resource, taking precedence
    /// over [`KubernetesExport::replicas`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Image pull policy override for this specific resource, taking
    /// precedence over [`KubernetesExport::image_pull_policy`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<ImagePullPolicy>,
}

/// Global overrides for the Helm chart export target.
///
/// Nested under [`ExportConfig::helm`]. The chart name and version default
/// to the project name and version respectively, resolved by
/// `lightshuttle-export` during lowering.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct HelmExport {
    /// Chart name. Defaults to the project name during lowering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_name: Option<String>,

    /// Chart version (`SemVer` string). Defaults to the project version when
    /// set, otherwise `"0.1.0"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_version: Option<String>,

    /// Default replica count exposed via the generated chart's `values.yaml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,

    /// Per-resource overrides, keyed by manifest resource name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub resources: IndexMap<String, HelmResourceExport>,
}

/// Per-resource overrides for the Helm chart export target.
///
/// Used as the value type in [`HelmExport::resources`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct HelmResourceExport {
    /// Whether this resource is included in the chart.
    ///
    /// `None` or `Some(true)` includes the resource. `Some(false)` omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Replica count override for this resource, taking precedence over
    /// [`HelmExport::replicas`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
}

/// Kubernetes image pull policy, used in [`KubernetesExport`] and
/// [`KubernetesResourceExport`].
///
/// Maps directly to the `imagePullPolicy` field in a Kubernetes
/// `Container` spec.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub enum ImagePullPolicy {
    /// Always pull the image, regardless of whether a local copy exists.
    Always,
    /// Pull the image only when it is not already present on the node.
    /// This is the default.
    #[default]
    IfNotPresent,
    /// Never pull the image; it must already be present on the node.
    Never,
}
