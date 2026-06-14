#![deny(missing_docs)]

//! OpenTelemetry collector bundling and environment injection for LightShuttle.
//!
//! This crate sits in the observer tier of the LightShuttle architecture, depending on
//! [`lightshuttle_runtime`] and [`lightshuttle_manifest`]. It bundles a standard OpenTelemetry
//! collector image and injects OpenTelemetry environment variables into the running
//! lifecycle plan.
//!
//! # Core building blocks
//!
//! - [`CollectorConfig`]: strongly-typed configuration of the bundled
//!   `otel/opentelemetry-collector` container. Offers sensible defaults
//!   (official upstream image, OTLP gRPC on port 4317, OTLP HTTP on 4318).
//!   Materialised to a [`lightshuttle_runtime::ContainerSpec`] via
//!   [`CollectorConfig.to_container_spec`].
//!
//! - [`augment_manifest`]: injects the collector into a manifest as a new
//!   `container` resource, then instruments all `container` and `dockerfile`
//!   resources with standard OpenTelemetry environment keys, respecting user-defined
//!   values. Must be called before the manifest is rendered to a runtime plan.
//!
//! - [`inject_otel_env`]: helper to inject `OTEL_EXPORTER_OTLP_ENDPOINT`,
//!   `OTEL_SERVICE_NAME`, and `OTEL_RESOURCE_ATTRIBUTES` into a resource
//!   environment. Idempotent: never overrides existing keys.
//!
//! - [`init_orchestrator_tracer`]: wires the orchestrator's own spans to the
//!   collector via an OTLP gRPC exporter, returning a [`TracerGuard`] that
//!   flushes on drop.
//!
//! # Example
//!
//! Wire a manifest and initialise the orchestrator's own tracing:
//!
//! ```rust,no_run
//! use lightshuttle_otel::{CollectorConfig, augment_manifest, init_orchestrator_tracer};
//! use lightshuttle_manifest::Manifest;
//!
//! # fn run() -> anyhow::Result<()> {
//! let manifest_yaml = "project:\n  name: demo\nresources:\n  db:\n    postgres:\n      version: \"16\"\n";
//! let mut manifest = Manifest::parse(manifest_yaml)?;
//! let collector = CollectorConfig::defaults();
//! augment_manifest(&mut manifest, &collector);
//!
//! let container_spec = collector.to_container_spec(manifest.project.name.as_str());
//! // Start container_spec with lightshuttle_runtime...
//!
//! let _guard = init_orchestrator_tracer(
//!     "http://127.0.0.1:4317",
//!     manifest.project.name.as_str()
//! )?;
//! // Spans are now exported to the collector.
//! # Ok(())
//! # }
//! ```
//!
//! # Skipping OpenTelemetry
//!
//! OpenTelemetry is enabled by default. To opt out, set `observability.otel.enabled: false`
//! in the manifest.

pub use crate::augment::{augment_manifest, is_enabled};
pub use crate::config::{CollectorConfig, SYNTHETIC_RESOURCE_NAME};
pub use crate::env::inject_otel_env;
pub use crate::tracer::{TracerGuard, init_orchestrator_tracer};

mod augment;
mod config;
mod env;
mod tracer;
