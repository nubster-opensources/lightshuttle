//! Pure resolution of per-target defaults and overrides.
//!
//! These helpers are the single place where the `export:` section is
//! turned into concrete values, so the defaults (`namespace` from the
//! project, `replicas` of one, `enabled` by default) are defined and
//! tested once and shared by every emitter.

use lightshuttle_manifest::{ExportConfig, ImagePullPolicy};

use crate::model::Target;

/// Default replica count when neither a per-resource nor a per-target
/// override is set.
const DEFAULT_REPLICAS: u32 = 1;

/// Default Helm chart version when neither the chart override nor the
/// project version is set.
const DEFAULT_CHART_VERSION: &str = "0.1.0";

/// Whether `resource` is emitted for `target`. A resource is included
/// unless its per-target override sets `enabled: false`.
#[must_use]
pub fn enabled_for(target: Target, resource: &str, export: Option<&ExportConfig>) -> bool {
    let Some(export) = export else { return true };
    let enabled = match target {
        Target::Compose => export
            .compose
            .as_ref()
            .and_then(|t| t.resources.get(resource))
            .and_then(|r| r.enabled),
        Target::Kubernetes => export
            .kubernetes
            .as_ref()
            .and_then(|t| t.resources.get(resource))
            .and_then(|r| r.enabled),
        Target::Helm => export
            .helm
            .as_ref()
            .and_then(|t| t.resources.get(resource))
            .and_then(|r| r.enabled),
    };
    enabled.unwrap_or(true)
}

/// Replica count for `resource` on `target`: a per-resource override
/// wins over the per-target default, which falls back to one. Compose
/// has no replica concept and always resolves to one.
#[must_use]
pub fn replicas_for(target: Target, resource: &str, export: Option<&ExportConfig>) -> u32 {
    let Some(export) = export else {
        return DEFAULT_REPLICAS;
    };
    match target {
        Target::Compose => DEFAULT_REPLICAS,
        Target::Kubernetes => export.kubernetes.as_ref().map_or(DEFAULT_REPLICAS, |t| {
            t.resources
                .get(resource)
                .and_then(|r| r.replicas)
                .or(t.replicas)
                .unwrap_or(DEFAULT_REPLICAS)
        }),
        Target::Helm => export.helm.as_ref().map_or(DEFAULT_REPLICAS, |t| {
            t.resources
                .get(resource)
                .and_then(|r| r.replicas)
                .or(t.replicas)
                .unwrap_or(DEFAULT_REPLICAS)
        }),
    }
}

/// Kubernetes namespace: the override if set, otherwise the project
/// name.
#[must_use]
pub fn namespace_for(project: &str, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.kubernetes.as_ref())
        .and_then(|k| k.namespace.clone())
        .unwrap_or_else(|| project.to_owned())
}

/// Image pull policy for `resource`: a per-resource override wins over
/// the per-target default, which falls back to `IfNotPresent`.
#[must_use]
pub fn image_pull_policy_for(resource: &str, export: Option<&ExportConfig>) -> ImagePullPolicy {
    export
        .and_then(|e| e.kubernetes.as_ref())
        .map(|k| {
            k.resources
                .get(resource)
                .and_then(|r| r.image_pull_policy)
                .or(k.image_pull_policy)
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Helm chart name: the override if set, otherwise the project name.
#[must_use]
pub fn chart_name_for(project: &str, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.helm.as_ref())
        .and_then(|h| h.chart_name.clone())
        .unwrap_or_else(|| project.to_owned())
}

/// Helm chart version: the override if set, otherwise the project
/// version, otherwise `0.1.0`.
#[must_use]
pub fn chart_version_for(project_version: Option<&str>, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.helm.as_ref())
        .and_then(|h| h.chart_version.clone())
        .or_else(|| project_version.map(ToOwned::to_owned))
        .unwrap_or_else(|| DEFAULT_CHART_VERSION.to_owned())
}
