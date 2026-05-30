//! Orchestrator self-tracing: wire `tracing` spans to an OTLP gRPC
//! exporter pointed at the bundled collector.

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

/// RAII guard returned by [`init_orchestrator_tracer`].
///
/// Dropping it flushes any pending spans by shutting the tracer
/// provider down. Hold it for the whole lifetime of `lightshuttle up`.
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

/// Initialise orchestrator self-tracing.
///
/// Builds an OTLP gRPC span exporter pointed at `endpoint` (typically
/// `http://127.0.0.1:4317`, the loopback port the bundled collector
/// publishes), installs a `tracing` subscriber composed of an
/// `EnvFilter`, a compact `fmt` layer and the OpenTelemetry bridge
/// layer, and returns a [`TracerGuard`] whose drop flushes pending
/// spans.
///
/// `service` is reported as the `service.name` resource attribute.
///
/// # Errors
///
/// Returns an error if the OTLP exporter cannot be constructed.
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
