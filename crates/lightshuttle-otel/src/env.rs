//! Inject the standard OpenTelemetry environment keys into a
//! resource's environment.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Environment variable read by every `OTel` SDK to locate the OTLP
/// gRPC endpoint of the collector.
const OTEL_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Logical service name reported to the collector.
const OTEL_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";

/// Comma-separated `key=value` pairs attached as resource attributes.
const OTEL_RESOURCE_ATTRIBUTES: &str = "OTEL_RESOURCE_ATTRIBUTES";

/// Inject the three standard `OTel` environment keys into `env`.
///
/// `collector_host` must be the in-network hostname of the bundled
/// collector (typically `<project>_lightshuttle_otel`).
/// `collector_grpc_port` is the OTLP gRPC port.
/// `service_name` is the logical name reported as `service.name`.
///
/// The function is idempotent and never overrides any value already
/// present in `env`: user-defined keys win.
pub fn inject_otel_env<S: BuildHasher>(
    env: &mut HashMap<String, String, S>,
    collector_host: &str,
    collector_grpc_port: u16,
    service_name: &str,
) {
    let endpoint = format!("http://{collector_host}:{collector_grpc_port}");
    env.entry(OTEL_ENDPOINT.to_owned()).or_insert(endpoint);
    env.entry(OTEL_SERVICE_NAME.to_owned())
        .or_insert_with(|| service_name.to_owned());
    env.entry(OTEL_RESOURCE_ATTRIBUTES.to_owned())
        .or_insert_with(|| format!("service.name={service_name},deployment.environment=local"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_every_key_when_env_is_empty() {
        let mut env = HashMap::new();
        inject_otel_env(&mut env, "demo_lightshuttle_otel", 4317, "cache");

        assert_eq!(
            env.get(OTEL_ENDPOINT).map(String::as_str),
            Some("http://demo_lightshuttle_otel:4317")
        );
        assert_eq!(
            env.get(OTEL_SERVICE_NAME).map(String::as_str),
            Some("cache")
        );
        assert_eq!(
            env.get(OTEL_RESOURCE_ATTRIBUTES).map(String::as_str),
            Some("service.name=cache,deployment.environment=local")
        );
    }

    #[test]
    fn does_not_override_user_defined_values() {
        let mut env = HashMap::new();
        env.insert(
            OTEL_ENDPOINT.to_owned(),
            "http://my-collector:4317".to_owned(),
        );
        env.insert(OTEL_SERVICE_NAME.to_owned(), "my-service".to_owned());
        env.insert(
            OTEL_RESOURCE_ATTRIBUTES.to_owned(),
            "service.name=my-service,team=infra".to_owned(),
        );

        inject_otel_env(&mut env, "demo_lightshuttle_otel", 4317, "cache");

        assert_eq!(
            env.get(OTEL_ENDPOINT).map(String::as_str),
            Some("http://my-collector:4317")
        );
        assert_eq!(
            env.get(OTEL_SERVICE_NAME).map(String::as_str),
            Some("my-service")
        );
        assert_eq!(
            env.get(OTEL_RESOURCE_ATTRIBUTES).map(String::as_str),
            Some("service.name=my-service,team=infra")
        );
    }

    #[test]
    fn is_idempotent_when_called_twice() {
        let mut env = HashMap::new();
        inject_otel_env(&mut env, "demo_lightshuttle_otel", 4317, "cache");
        let snapshot = env.clone();
        inject_otel_env(&mut env, "demo_lightshuttle_otel", 4317, "cache");
        assert_eq!(env, snapshot);
    }
}
