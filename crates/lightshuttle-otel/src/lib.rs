//! OpenTelemetry collector bundling and environment injection for
//! LightShuttle.
//!
//! This crate provides two building blocks:
//!
//! - [`CollectorConfig`] — a strongly-typed value describing the
//!   bundled `otel/opentelemetry-collector` container. It can be
//!   materialised into a [`lightshuttle_runtime::ContainerSpec`] via
//!   [`CollectorConfig::to_container_spec`].
//! - [`inject_otel_env`] — an idempotent helper that adds the standard
//!   `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME` and
//!   `OTEL_RESOURCE_ATTRIBUTES` keys to a resource environment without
//!   overriding any value already set by the user.

pub use crate::augment::{augment_manifest, is_enabled};
pub use crate::config::{CollectorConfig, SYNTHETIC_RESOURCE_NAME};
pub use crate::env::inject_otel_env;

mod augment;
mod config;
mod env;
