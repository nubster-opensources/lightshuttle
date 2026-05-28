//! OpenTelemetry collector configuration.

use std::collections::HashMap;
use std::time::Duration;

use lightshuttle_runtime::{ContainerSpec, HealthcheckSpec, ImageSource, PortBinding};

/// Resource name used for the bundled collector inside the lifecycle
/// plan. Stable so dependents can refer to it via the standard
/// `${resources.lightshuttle_otel.host}` interpolation if needed.
pub const SYNTHETIC_RESOURCE_NAME: &str = "lightshuttle_otel";

/// Default OTLP gRPC port (collector receiver).
const DEFAULT_OTLP_GRPC_PORT: u16 = 4317;

/// Default OTLP HTTP port (collector receiver).
const DEFAULT_OTLP_HTTP_PORT: u16 = 4318;

/// Default collector image, pinned to a known-good tag.
const DEFAULT_IMAGE: &str = "otel/opentelemetry-collector:0.108.0";

/// Strongly-typed configuration of the bundled OpenTelemetry collector.
///
/// All fields are public so callers can override individual knobs
/// (image tag, port mapping) without recreating the whole value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorConfig {
    /// Container image of the collector.
    pub image: String,
    /// Host-side OTLP gRPC port published by the collector.
    pub otlp_grpc_port: u16,
    /// Host-side OTLP HTTP port published by the collector.
    pub otlp_http_port: u16,
}

impl CollectorConfig {
    /// Sane defaults: official upstream image, OTLP gRPC on `:4317`,
    /// OTLP HTTP on `:4318`.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            image: DEFAULT_IMAGE.to_owned(),
            otlp_grpc_port: DEFAULT_OTLP_GRPC_PORT,
            otlp_http_port: DEFAULT_OTLP_HTTP_PORT,
        }
    }

    /// Hostname that dependents must use to reach the collector from
    /// inside the project network. Mirrors the
    /// `<project>_<resource>` container name convention used by
    /// `lightshuttle-runtime`.
    #[must_use]
    pub fn hostname(&self, project: &str) -> String {
        format!("{project}_{SYNTHETIC_RESOURCE_NAME}")
    }

    /// Build a [`ContainerSpec`] runnable by `lightshuttle-runtime`.
    ///
    /// The collector is started in `--config=builtin:default-config`
    /// mode and listens on the OTLP gRPC and HTTP ports defined by
    /// this configuration.
    #[must_use]
    pub fn to_container_spec(&self, project: &str) -> ContainerSpec {
        ContainerSpec {
            name: format!("{project}_{SYNTHETIC_RESOURCE_NAME}"),
            project: project.to_owned(),
            resource: SYNTHETIC_RESOURCE_NAME.to_owned(),
            image: ImageSource::Pull(self.image.clone()),
            env: HashMap::new(),
            ports: vec![
                PortBinding {
                    container_port: DEFAULT_OTLP_GRPC_PORT,
                    host_address: Some("127.0.0.1".to_owned()),
                    host_port: self.otlp_grpc_port,
                },
                PortBinding {
                    container_port: DEFAULT_OTLP_HTTP_PORT,
                    host_address: Some("127.0.0.1".to_owned()),
                    host_port: self.otlp_http_port,
                },
            ],
            volumes: Vec::new(),
            command: None,
            healthcheck: Some(HealthcheckSpec {
                test: vec![
                    "CMD-SHELL".to_owned(),
                    format!(
                        "wget -qO- http://127.0.0.1:{DEFAULT_OTLP_HTTP_PORT}/ > /dev/null 2>&1 || exit 0"
                    ),
                ],
                interval: Duration::from_secs(5),
                timeout: Duration::from_secs(3),
                retries: 3,
                start_period: Duration::from_secs(2),
            }),
        }
    }
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_otlp_standard_ports() {
        let cfg = CollectorConfig::defaults();
        assert_eq!(cfg.otlp_grpc_port, 4317);
        assert_eq!(cfg.otlp_http_port, 4318);
        assert!(cfg.image.starts_with("otel/opentelemetry-collector"));
    }

    #[test]
    fn hostname_is_project_prefixed() {
        let cfg = CollectorConfig::defaults();
        assert_eq!(cfg.hostname("demo"), "demo_lightshuttle_otel");
    }

    #[test]
    fn to_container_spec_publishes_both_otlp_ports() {
        let cfg = CollectorConfig::defaults();
        let spec = cfg.to_container_spec("demo");

        assert_eq!(spec.name, "demo_lightshuttle_otel");
        assert_eq!(spec.project, "demo");
        assert_eq!(spec.resource, "lightshuttle_otel");
        assert_eq!(spec.ports.len(), 2);
        let host_ports: Vec<u16> = spec.ports.iter().map(|p| p.host_port).collect();
        assert!(host_ports.contains(&4317));
        assert!(host_ports.contains(&4318));
    }
}
