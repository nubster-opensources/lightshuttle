//! OpenTelemetry collector configuration and container spec generation.

use std::collections::HashMap;

use lightshuttle_runtime::{ContainerSpec, ImageSource, PortBinding};

/// Reserved resource name for the bundled collector inside the lifecycle plan.
///
/// This name is stable and follows the LightShuttle naming convention
/// (`project_lightshuttle_otel`), so dependents can refer to the collector
/// via standard manifest interpolation: `${resources.lightshuttle_otel.host}`.
/// Manifest augmentation skips silently if a user resource already owns this name.
pub const SYNTHETIC_RESOURCE_NAME: &str = "lightshuttle_otel";

/// Default OTLP gRPC port (collector receiver).
const DEFAULT_OTLP_GRPC_PORT: u16 = 4317;

/// Default OTLP HTTP port (collector receiver).
const DEFAULT_OTLP_HTTP_PORT: u16 = 4318;

/// Default collector image, pinned to a known-good tag.
const DEFAULT_IMAGE: &str = "otel/opentelemetry-collector:0.108.0";

/// Strongly-typed configuration of the bundled OpenTelemetry collector.
///
/// Encapsulates the container image, OTLP gRPC port, and OTLP HTTP port
/// published by the collector. All fields are public so callers can override
/// individual knobs (e.g. custom image tag, different port mapping) without
/// recreating the whole value.
///
/// # Example
///
/// Use defaults, then customize:
///
/// ```
/// use lightshuttle_otel::CollectorConfig;
///
/// let mut config = CollectorConfig::defaults();
/// config.otlp_grpc_port = 5317;  // Custom gRPC port
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorConfig {
    /// Container image of the collector (e.g. `otel/opentelemetry-collector:0.108.0`).
    pub image: String,
    /// Host-side OTLP gRPC port published by the collector.
    pub otlp_grpc_port: u16,
    /// Host-side OTLP HTTP port published by the collector.
    pub otlp_http_port: u16,
}

impl CollectorConfig {
    /// Sensible defaults: official upstream image, OTLP gRPC on port 4317, OTLP HTTP on port 4318.
    ///
    /// # Example
    ///
    /// ```
    /// use lightshuttle_otel::CollectorConfig;
    ///
    /// let config = CollectorConfig::defaults();
    /// assert_eq!(config.otlp_grpc_port, 4317);
    /// assert_eq!(config.otlp_http_port, 4318);
    /// ```
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            image: DEFAULT_IMAGE.to_owned(),
            otlp_grpc_port: DEFAULT_OTLP_GRPC_PORT,
            otlp_http_port: DEFAULT_OTLP_HTTP_PORT,
        }
    }

    /// Compute the in-network hostname the collector will advertise.
    ///
    /// Follows the LightShuttle `<project>_<resource>` container naming convention.
    /// This hostname is used when injecting `OTEL_EXPORTER_OTLP_ENDPOINT` into dependents.
    ///
    /// # Example
    ///
    /// ```
    /// use lightshuttle_otel::CollectorConfig;
    ///
    /// let config = CollectorConfig::defaults();
    /// assert_eq!(config.hostname("demo"), "demo_lightshuttle_otel");
    /// ```
    #[must_use]
    pub fn hostname(&self, project: &str) -> String {
        format!("{project}_{SYNTHETIC_RESOURCE_NAME}")
    }

    /// Materialize into a [`ContainerSpec`] runnable by [`lightshuttle_runtime`].
    ///
    /// Builds a container spec with:
    /// - OTLP gRPC receiver listening on the configured `otlp_grpc_port`.
    /// - OTLP HTTP receiver listening on the configured `otlp_http_port`.
    /// - Both ports bound to localhost (127.0.0.1) only.
    /// - Built-in default configuration mode (`--config=builtin:default-config`).
    /// - No healthcheck probe (the built-in config does not enable the `health_check` extension,
    ///   and masking a crash would be worse than surfacing it via container exit status).
    ///
    /// # Example
    ///
    /// ```
    /// use lightshuttle_otel::CollectorConfig;
    ///
    /// let config = CollectorConfig::defaults();
    /// let spec = config.to_container_spec("myapp");
    /// assert_eq!(spec.name, "myapp_lightshuttle_otel");
    /// assert_eq!(spec.ports.len(), 2);
    /// ```
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
            entrypoint: None,
            command: None,
            // No Docker healthcheck. The previous `... || exit 0` probe
            // always reported healthy and masked a crashed collector. The
            // collector image runs `--config=builtin:default-config`,
            // which does not enable the health_check extension, so there
            // is no reliable HTTP probe to target. A crash is instead
            // surfaced through the container exit status: `wait_healthy`
            // observes a stopped container and fails rather than passing
            // a dead collector off as healthy.
            healthcheck: None,
            working_dir: None,
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
