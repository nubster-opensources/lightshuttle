//! Pure resolution of per-target defaults and overrides.
//!
//! These helpers are the single place where the optional `export:` manifest
//! section is turned into concrete values consumed by the emitters. Keeping
//! all defaults here (namespace derived from the project name, replicas of
//! one, resources enabled by default) means they are defined and tested once
//! and shared by every emitter without duplication.

use lightshuttle_manifest::{ExportConfig, ImagePullPolicy};

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

/// Sanitise a manifest name into a DNS-1123 compliant label.
///
/// Lowercases the input, replaces every character outside `[a-z0-9-]`
/// with a hyphen, prepends `x` when the result would start with a digit
/// or a hyphen, and truncates to 63 characters (stripping any trailing
/// hyphens produced by the truncation).
#[must_use]
pub(crate) fn dns_name(name: &str) -> String {
    let normalized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let prefixed = if normalized
        .chars()
        .next()
        .is_none_or(|c| c == '-' || c.is_ascii_digit())
    {
        format!("x{normalized}")
    } else {
        normalized
    };
    let truncated: String = prefixed.chars().take(63).collect();
    truncated.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::dns_name;

    #[test]
    fn dns_name_already_valid() {
        assert_eq!(dns_name("my-service"), "my-service");
    }

    #[test]
    fn dns_name_lowercase() {
        assert_eq!(dns_name("MyService"), "myservice");
    }

    #[test]
    fn dns_name_underscores_become_hyphens() {
        assert_eq!(dns_name("my_service"), "my-service");
    }

    #[test]
    fn dns_name_leading_digit_gets_prefix() {
        assert_eq!(dns_name("1redis"), "x1redis");
    }

    #[test]
    fn dns_name_leading_hyphen_gets_prefix() {
        assert_eq!(dns_name("-leading"), "x-leading");
    }

    #[test]
    fn dns_name_trailing_hyphen_stripped() {
        assert_eq!(dns_name("trailing-"), "trailing");
    }

    #[test]
    fn dns_name_truncated_to_63() {
        let long = "a".repeat(70);
        assert_eq!(dns_name(&long).len(), 63);
    }

    #[test]
    fn dns_name_truncation_strips_trailing_hyphen() {
        let name = format!("{}-b", "a".repeat(62));
        let result = dns_name(&name);
        assert!(
            !result.ends_with('-'),
            "must not end with hyphen after truncation"
        );
        assert!(result.len() <= 63);
    }
}
