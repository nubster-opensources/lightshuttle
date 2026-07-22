//! Pure resolution of per-target defaults and overrides.
//!
//! These helpers are the single place where the optional `export:` manifest
//! section is turned into concrete values consumed by the emitters. Keeping
//! all defaults here (namespace derived from the project name, replicas of
//! one, resources enabled by default) means they are defined and tested once
//! and shared by every emitter without duplication.

use std::collections::BTreeMap;

use lightshuttle_manifest::{DnsName, ExportConfig, ImagePullPolicy};
use lightshuttle_spec::ContainerSpec;

use crate::error::Result;
use crate::model::Target;

/// Environment key fragments that classify a variable as a secret rather than
/// plain configuration.
///
/// Each marker is matched case-insensitively against the full environment
/// variable key. When a match is found, the emitter routes the variable into
/// a secret store (a Kubernetes `Secret` or a Helm `stringData` block) and
/// replaces its value with a placeholder so real credentials never appear in
/// the exported artifact.
///
/// All emitters reference this single slice so the classification stays in
/// sync across all export targets.
///
/// ```rust
/// use lightshuttle_export::resolve::SECRET_MARKERS;
///
/// assert!(SECRET_MARKERS.contains(&"PASSWORD"));
/// assert!(SECRET_MARKERS.contains(&"TOKEN"));
/// ```
pub const SECRET_MARKERS: &[&str] = &[
    "PASSWORD",
    "PASSWD",
    "PASS",
    "SECRET",
    "TOKEN",
    "KEY",
    "CREDENTIAL",
    "AUTH",
    "CERT",
    "PWD",
];

/// Split a resolved environment into plain configuration and redacted
/// secrets. Explicit manifest classification takes precedence; the legacy
/// key-name heuristic remains as a compatibility safety net.
pub(crate) fn split_env(
    spec: &ContainerSpec,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut config = BTreeMap::new();
    let mut secret = BTreeMap::new();
    for (key, value) in &spec.env {
        if is_secret_key(spec, key) {
            secret.insert(key.clone(), "***".to_owned());
        } else {
            config.insert(key.clone(), value.clone());
        }
    }
    (config, secret)
}

/// Environment emitted to Compose. Sensitive values are read from the
/// caller's environment at `docker compose` time instead of being copied from
/// the manifest into the generated YAML.
pub(crate) fn compose_env(spec: &ContainerSpec) -> BTreeMap<String, String> {
    spec.env
        .iter()
        .map(|(key, value)| {
            let value = if is_secret_key(spec, key) {
                format!("${{{key}}}")
            } else {
                value.clone()
            };
            (key.clone(), value)
        })
        .collect()
}

fn is_secret_key(spec: &ContainerSpec, key: &str) -> bool {
    spec.secret_env_keys.contains(key)
        || SECRET_MARKERS
            .iter()
            .any(|marker| key.to_ascii_uppercase().contains(marker))
}

/// Default replica count when neither a per-resource nor a per-target
/// override is set.
const DEFAULT_REPLICAS: u32 = 1;

/// Default Helm chart version when neither the chart override nor the
/// project version is set.
const DEFAULT_CHART_VERSION: &str = "0.1.0";

/// Returns `true` when `resource` should be emitted for `target`.
///
/// A resource is included by default. It is excluded only when the manifest
/// `export:` section contains an explicit `enabled: false` override for the
/// given resource and target combination.
///
/// ```rust
/// use lightshuttle_export::{Target, resolve::enabled_for};
///
/// // Without any export config, every resource is enabled.
/// assert!(enabled_for(Target::Compose, "db", None));
/// ```
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

/// Returns the replica count for `resource` on `target`.
///
/// Resolution order: per-resource override -> per-target default -> `1`.
/// Compose has no replica concept and always returns `1` regardless of any
/// override.
///
/// ```rust
/// use lightshuttle_export::{Target, resolve::replicas_for};
///
/// // Defaults to 1 when no export config is provided.
/// assert_eq!(replicas_for(Target::Kubernetes, "api", None), 1);
/// assert_eq!(replicas_for(Target::Compose, "api", None), 1);
/// ```
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

/// Returns the Kubernetes namespace to use for the export.
///
/// If the manifest `export.kubernetes.namespace` override is set, it is used
/// as-is. Otherwise the project name is returned as the default namespace.
///
/// ```rust
/// use lightshuttle_export::resolve::namespace_for;
///
/// assert_eq!(namespace_for("my-project", None), "my-project");
/// ```
#[must_use]
pub fn namespace_for(project: &str, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.kubernetes.as_ref())
        .and_then(|k| k.namespace.clone())
        .unwrap_or_else(|| project.to_owned())
}

/// Returns the image pull policy for `resource` on the Kubernetes or Helm target.
///
/// Resolution order: per-resource override -> per-target default ->
/// `IfNotPresent`.
///
/// ```rust
/// use lightshuttle_export::resolve::image_pull_policy_for;
/// use lightshuttle_manifest::ImagePullPolicy;
///
/// assert_eq!(image_pull_policy_for("api", None), ImagePullPolicy::IfNotPresent);
/// ```
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

/// Returns the Helm chart name.
///
/// If `export.helm.chart_name` is set, it is returned. Otherwise the project
/// name is used.
///
/// ```rust
/// use lightshuttle_export::resolve::chart_name_for;
///
/// assert_eq!(chart_name_for("my-project", None), "my-project");
/// ```
#[must_use]
pub fn chart_name_for(project: &str, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.helm.as_ref())
        .and_then(|h| h.chart_name.clone())
        .unwrap_or_else(|| project.to_owned())
}

/// Returns the Helm chart version string.
///
/// Resolution order: `export.helm.chart_version` override -> `project_version`
/// -> `"0.1.0"`.
///
/// ```rust
/// use lightshuttle_export::resolve::chart_version_for;
///
/// assert_eq!(chart_version_for(Some("1.2.3"), None), "1.2.3");
/// assert_eq!(chart_version_for(None, None), "0.1.0");
/// ```
#[must_use]
pub fn chart_version_for(project_version: Option<&str>, export: Option<&ExportConfig>) -> String {
    export
        .and_then(|e| e.helm.as_ref())
        .and_then(|h| h.chart_version.clone())
        .or_else(|| project_version.map(ToOwned::to_owned))
        .unwrap_or_else(|| DEFAULT_CHART_VERSION.to_owned())
}

/// Converts a manifest name into a DNS-1123 label.
///
/// Delegates to [`DnsName::from_manifest_name`], which is injective: a name
/// that is not already a label receives a deterministic suffix, so `foo_bar`
/// and `foo-bar` no longer converge onto one identifier and overwrite each
/// other's artifact.
///
/// # Errors
///
/// Returns [`crate::ExportError::Unsupported`] when the name is empty. That
/// is unreachable for a validated resource name, and reachable for a
/// user-supplied chart name.
pub(crate) fn dns_name(name: &str) -> Result<String> {
    DnsName::from_manifest_name(name)
        .map(|label| label.as_str().to_owned())
        .map_err(|source| crate::ExportError::Unsupported {
            resource: name.to_owned(),
            target: "export",
            reason: source.to_string(),
        })
}

/// Resolves the namespace an export targets, as a validated DNS label.
///
/// An explicit `export.kubernetes.namespace` is a deliberate value, so it is
/// validated and rejected when it is not a label, never rewritten: handing
/// back an identifier the user did not write would be worse than refusing.
/// A namespace derived from the project name is normalised like any other
/// manifest name.
///
/// # Errors
///
/// Returns [`crate::ExportError::Unsupported`] when an explicit override is
/// not a valid Kubernetes namespace.
pub fn namespace_label_for(project: &str, export: Option<&ExportConfig>) -> Result<String> {
    let override_value = export
        .and_then(|config| config.kubernetes.as_ref())
        .and_then(|kubernetes| kubernetes.namespace.clone());

    match override_value {
        Some(value) => DnsName::parse(&value)
            .map(|label| label.as_str().to_owned())
            .map_err(|source| crate::ExportError::Unsupported {
                resource: "export.kubernetes.namespace".to_owned(),
                target: "kubernetes",
                reason: source.to_string(),
            }),
        None => dns_name(project),
    }
}

#[cfg(test)]
mod tests {
    use super::dns_name;

    fn label(name: &str) -> String {
        dns_name(name).unwrap_or_else(|error| panic!("`{name}` should normalise, got {error}"))
    }

    #[test]
    fn a_name_that_is_already_a_label_is_left_alone() {
        assert_eq!(label("my-service"), "my-service");
    }

    /// This test previously asserted that `my_service` became `my-service`,
    /// which is the defect: `my-service` is itself a valid manifest name, so
    /// the two converged and one artifact overwrote the other. The corpus was
    /// defending the bug.
    #[test]
    fn an_underscore_name_does_not_converge_with_its_hyphen_twin() {
        assert_ne!(label("my_service"), label("my-service"));
    }

    #[test]
    fn every_label_stays_within_the_dns_limit() {
        let long = "a".repeat(70);
        assert!(label(&long).len() <= 63);
        assert!(!label(&long).ends_with('-'));
    }

    #[test]
    fn an_empty_name_is_rejected_rather_than_invented() {
        assert!(dns_name("").is_err());
    }
}
