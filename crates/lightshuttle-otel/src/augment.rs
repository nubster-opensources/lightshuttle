//! Manifest-level wiring of the bundled `OTel` collector.
//!
//! Adds the collector as a `container` resource at the top of the
//! plan and injects the standard `OTel` env keys into every existing
//! `container` and `dockerfile` resource.

use indexmap::IndexMap;
use lightshuttle_manifest::model::{ContainerConfig, ResourceKind};
use lightshuttle_manifest::{Manifest, ObservabilityConfig};

use crate::config::{CollectorConfig, SYNTHETIC_RESOURCE_NAME};

const OTEL_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const OTEL_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";
const OTEL_RESOURCE_ATTRIBUTES: &str = "OTEL_RESOURCE_ATTRIBUTES";

/// Returns whether `OTel` is enabled for `manifest`.
///
/// The default is `true`; it is only `false` when
/// `observability.otel.enabled` is explicitly set to `false`.
#[must_use]
pub fn is_enabled(manifest: &Manifest) -> bool {
    let observability = manifest
        .observability
        .as_ref()
        .unwrap_or(&ObservabilityConfig { otel: None });
    let otel = observability.otel.as_ref();
    otel.and_then(|o| o.enabled).unwrap_or(true)
}

/// Augment `manifest` in place with the bundled `OTel` collector.
///
/// - Adds an `OTel` collector container at the top of the resources
///   `IndexMap` (so it appears first in topological order).
/// - For every existing `container` and `dockerfile` resource,
///   injects the `OTel` env keys without overriding user-defined
///   values, and adds an implicit `depends_on` on the collector so
///   the runtime starts it before the dependents.
pub fn augment_manifest(manifest: &mut Manifest, config: &CollectorConfig) {
    inject_into_resources(manifest, config);
    prepend_collector_resource(manifest, config);
}

fn inject_into_resources(manifest: &mut Manifest, config: &CollectorConfig) {
    let project = manifest.project.name.as_str();
    let endpoint = format!(
        "http://{host}:{port}",
        host = config.hostname(project),
        port = config.otlp_grpc_port,
    );

    for (resource_name, kind) in &mut manifest.resources {
        match kind {
            ResourceKind::Container(cfg) => {
                inject_env(&mut cfg.env, &endpoint, resource_name);
                push_dep(&mut cfg.depends_on, SYNTHETIC_RESOURCE_NAME.to_owned());
            }
            ResourceKind::Dockerfile(cfg) => {
                inject_env(&mut cfg.env, &endpoint, resource_name);
                push_dep(&mut cfg.depends_on, SYNTHETIC_RESOURCE_NAME.to_owned());
            }
            // postgres/redis use canned commands and ignore `OTel` env.
            ResourceKind::Postgres(_) | ResourceKind::Redis(_) => {}
        }
    }
}

fn inject_env(env: &mut IndexMap<String, String>, endpoint: &str, service: &str) {
    env.entry(OTEL_ENDPOINT.to_owned())
        .or_insert_with(|| endpoint.to_owned());
    env.entry(OTEL_SERVICE_NAME.to_owned())
        .or_insert_with(|| service.to_owned());
    env.entry(OTEL_RESOURCE_ATTRIBUTES.to_owned())
        .or_insert_with(|| format!("service.name={service},deployment.environment=local"));
}

fn push_dep(deps: &mut Vec<String>, name: String) {
    if !deps.iter().any(|d| d == &name) {
        deps.push(name);
    }
}

fn prepend_collector_resource(manifest: &mut Manifest, config: &CollectorConfig) {
    let collector = ContainerConfig {
        image: config.image.clone(),
        ports: Vec::new(),
        env: IndexMap::new(),
        volumes: Vec::new(),
        command: None,
        working_dir: None,
        healthcheck: None,
        depends_on: Vec::new(),
    };

    // Reinsert every existing resource so the collector lands first.
    let existing: Vec<(String, ResourceKind)> = manifest
        .resources
        .drain(..)
        .collect::<Vec<_>>()
        .into_iter()
        .collect();

    manifest.resources.insert(
        SYNTHETIC_RESOURCE_NAME.to_owned(),
        ResourceKind::Container(collector),
    );
    for (name, kind) in existing {
        manifest.resources.insert(name, kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightshuttle_manifest::Manifest;

    fn parse(yaml: &str) -> Manifest {
        Manifest::parse(yaml).expect("manifest parses")
    }

    #[test]
    fn is_enabled_defaults_to_true_when_section_absent() {
        let manifest = parse(
            r"
project:
  name: app
resources:
  api:
    container:
      image: alpine
",
        );
        assert!(is_enabled(&manifest));
    }

    #[test]
    fn is_enabled_is_false_only_when_explicit() {
        let manifest = parse(
            r"
project:
  name: app
observability:
  otel:
    enabled: false
resources:
  api:
    container:
      image: alpine
",
        );
        assert!(!is_enabled(&manifest));
    }

    #[test]
    fn augment_prepends_collector_and_injects_env() {
        let mut manifest = parse(
            r"
project:
  name: demo
resources:
  api:
    container:
      image: alpine
",
        );
        let cfg = CollectorConfig::defaults();
        augment_manifest(&mut manifest, &cfg);

        let names: Vec<&str> = manifest.resources.keys().map(String::as_str).collect();
        assert_eq!(names.first().copied(), Some(SYNTHETIC_RESOURCE_NAME));
        assert!(names.contains(&"api"));

        let api = manifest.resources.get("api").expect("api resource");
        let ResourceKind::Container(api) = api else {
            panic!("expected container resource");
        };
        assert_eq!(
            api.env.get(OTEL_ENDPOINT).map(String::as_str),
            Some("http://demo_lightshuttle_otel:4317")
        );
        assert_eq!(
            api.env.get(OTEL_SERVICE_NAME).map(String::as_str),
            Some("api")
        );
        assert!(api.depends_on.iter().any(|d| d == SYNTHETIC_RESOURCE_NAME));
    }

    #[test]
    fn augment_does_not_override_user_env() {
        let mut manifest = parse(
            r"
project:
  name: demo
resources:
  api:
    container:
      image: alpine
      env:
        OTEL_SERVICE_NAME: custom-service
",
        );
        let cfg = CollectorConfig::defaults();
        augment_manifest(&mut manifest, &cfg);

        let api = manifest.resources.get("api").expect("api resource");
        let ResourceKind::Container(api) = api else {
            panic!("expected container resource");
        };
        assert_eq!(
            api.env.get(OTEL_SERVICE_NAME).map(String::as_str),
            Some("custom-service")
        );
    }
}
