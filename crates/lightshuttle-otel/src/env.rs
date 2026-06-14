//! Inject the standard OpenTelemetry environment keys into a resource's environment.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Environment variable read by every OpenTelemetry SDK to locate the OTLP gRPC endpoint.
const OTEL_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Logical service name reported to the collector as the `service.name` resource attribute.
const OTEL_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";

/// Comma-separated resource attributes (key=value pairs) attached to all spans and metrics.
const OTEL_RESOURCE_ATTRIBUTES: &str = "OTEL_RESOURCE_ATTRIBUTES";

/// Inject the three standard OpenTelemetry environment keys into a resource's environment.
///
/// Injects `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, and `OTEL_RESOURCE_ATTRIBUTES`
/// into the environment map. The function is idempotent: calling it multiple times with the
/// same arguments produces the same result. Crucially, it never overrides any key already
/// present in `env`, so user-defined values always win.
///
/// # Arguments
///
/// - `env`: mutable reference to any HashMap-like container with a custom hasher.
/// - `collector_host`: the in-network hostname of the collector (e.g. `demo_lightshuttle_otel`).
/// - `collector_grpc_port`: the OTLP gRPC port (typically 4317).
/// - `service_name`: the logical name for the service (used as the span service.name attribute).
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use lightshuttle_otel::inject_otel_env;
///
/// let mut env = HashMap::new();
/// inject_otel_env(&mut env, "myapp_lightshuttle_otel", 4317, "api");
///
/// assert_eq!(
///     env.get("OTEL_EXPORTER_OTLP_ENDPOINT"),
///     Some(&"http://myapp_lightshuttle_otel:4317".to_string())
/// );
/// assert_eq!(
///     env.get("OTEL_SERVICE_NAME"),
///     Some(&"api".to_string())
/// );
/// ```
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
