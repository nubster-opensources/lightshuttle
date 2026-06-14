//! Orchestrator self-tracing: wire `tracing` spans to an OTLP gRPC exporter.
//!
//! This module provides a single-shot initializer that connects the orchestrator's
//! own observability to the bundled OpenTelemetry collector, returning a guard
//! that must be held for the entire lifetime of the orchestrator process.

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::runtime;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// RAII guard for the orchestrator's tracing infrastructure.
///
/// Returned by [`init_orchestrator_tracer`]. Dropping this guard flushes any
/// pending spans by shutting down the OpenTelemetry tracer provider. Hold the
/// guard for the entire lifetime of the `lightshuttle up` command.
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_otel::init_orchestrator_tracer;
///
/// # fn main() -> anyhow::Result<()> {
/// let _guard = init_orchestrator_tracer("http://127.0.0.1:4317", "lightshuttle")?;
/// // Tracing is now active. The guard keeps the tracer provider alive.
/// # Ok(())
/// # }
/// ```
pub struct TracerGuard {
    provider: TracerProvider,
}

impl Drop for TracerGuard {
    fn drop(&mut self) {
        if let Err(err) = self.provider.shutdown() {
            // The subscriber may already be torn down; fall back to
            // stderr so the flush failure is never silent.
            eprintln!("lightshuttle-otel: tracer flush on shutdown failed: {err}");
        }
    }
}

/// Initialize the orchestrator's self-tracing pipeline.
///
/// Sets up a complete tracing stack:
/// 1. An OTLP gRPC exporter pointed at `endpoint` (typically the bundled collector's loopback
///    port, e.g. `http://127.0.0.1:4317`).
/// 2. A `tracing` subscriber with an `EnvFilter`, a compact `fmt` layer for stderr, and
///    an OpenTelemetry bridge layer that routes spans to the exporter.
/// 3. Sets the global OpenTelemetry tracer provider so downstream crates can access it.
///
/// The function installs the subscriber globally and returns a [`TracerGuard`] that must
/// be held for the orchestrator's entire lifetime. Dropping the guard flushes pending
/// spans and shuts down the tracer provider.
///
/// # Arguments
///
/// - `endpoint`: OTLP gRPC endpoint of the collector (e.g. `http://127.0.0.1:4317`).
/// - `service`: logical service name (reported as the `service.name` resource attribute).
///
/// # Logging
///
/// The subscriber respects the `LIGHTSHUTTLE_LOG` environment variable for filtering
/// (defaults to `info` level if unset). Logs are written to stderr.
///
/// # Errors
///
/// Returns an error if:
/// - The OTLP exporter cannot be constructed (e.g. invalid endpoint).
/// - A global tracing subscriber is already installed.
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_otel::init_orchestrator_tracer;
///
/// # fn main() -> anyhow::Result<()> {
/// let _guard = init_orchestrator_tracer(
///     "http://127.0.0.1:4317",
///     "lightshuttle"
/// )?;
///
/// tracing::info!("Orchestrator started");
/// // The span is now exported to the collector.
/// # Ok(())
/// # }
/// ```
pub fn init_orchestrator_tracer(endpoint: &str, service: &str) -> Result<TracerGuard> {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.to_owned())
        .build()
        .context("failed to build the OTLP span exporter")?;

    let resource = Resource::new(vec![KeyValue::new(SERVICE_NAME, service.to_owned())]);

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(resource)
        .build();

    global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer("lightshuttle");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let filter =
        EnvFilter::try_from_env("LIGHTSHUTTLE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_level(true)
                .compact(),
        )
        .with(otel_layer)
        .try_init()
        .context("a global tracing subscriber is already installed")?;

    Ok(TracerGuard { provider })
}
