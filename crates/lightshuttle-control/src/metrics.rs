//! Prometheus metrics for the control plane.
//!
//! Metrics are exposed in the Prometheus text exposition format on
//! `GET /metrics`. The recorder is installed once per process by
//! [`Metrics::install`]; tests build a non-installing handle via
//! [`Metrics::for_test`] so multiple control servers can coexist
//! without panicking on a double global install.

use std::time::Instant;

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Counter incremented on every accepted restart request.
pub(crate) const RESTART_TOTAL: &str = "lightshuttle_restart_total";

/// Histogram of the seconds a resource takes to go from started to
/// healthy.
pub(crate) const EVENT_DURATION: &str = "lightshuttle_lifecycle_event_duration_seconds";

/// Gauge of resource count, labelled by status.
const RESOURCES: &str = "lightshuttle_resources";

/// Gauge of orchestrator uptime in seconds.
const UPTIME: &str = "lightshuttle_uptime_seconds";

/// Prometheus metrics handle for the control plane.
///
/// Wraps a [`PrometheusHandle`] and the process start time used to
/// compute `lightshuttle_uptime_seconds` at scrape time.
///
/// # Lifecycle
///
/// Call [`Metrics::install`] **once** per process to register the global
/// Prometheus recorder. Pass the resulting value (wrapped in an
/// [`std::sync::Arc`]) to [`crate::ControlState::with_metrics`] so
/// `GET /metrics` can render a live snapshot.
///
/// For tests and embedders that do not need metrics, build with
/// [`Metrics::for_test`], which does not touch the global recorder.
///
/// # Tracked metrics
///
/// | Metric name | Kind | Description |
/// |---|---|---|
/// | `lightshuttle_restart_total` | counter | Accepted restart requests |
/// | `lightshuttle_lifecycle_event_duration_seconds` | histogram | Seconds from start to healthy |
/// | `lightshuttle_resources` | gauge (per status label) | Managed resource count |
/// | `lightshuttle_uptime_seconds` | gauge | Process uptime |
pub struct Metrics {
    handle: PrometheusHandle,
    started: Instant,
}

impl Metrics {
    /// Install the global Prometheus recorder and describe every
    /// metric. Call exactly once per process, before any metric is
    /// recorded.
    ///
    /// # Panics
    ///
    /// Panics if a global recorder is already installed.
    #[must_use]
    pub fn install() -> Self {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .expect("failed to install the Prometheus recorder");
        describe_metrics();
        Self {
            handle,
            started: Instant::now(),
        }
    }

    /// Build a non-installing handle for tests.
    ///
    /// The `metrics!` macros always target the globally installed
    /// recorder, which this constructor never sets. The returned handle
    /// therefore renders an empty snapshot regardless of any metric
    /// recorded elsewhere. Use [`Self::install`] plus
    /// [`super::ControlState::with_metrics`] to serve live metrics.
    #[must_use]
    pub fn for_test() -> Self {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        Self {
            handle,
            started: Instant::now(),
        }
    }

    /// Render the current metrics snapshot in Prometheus text format.
    ///
    /// Before serialising, this method refreshes the two scrape-time gauges:
    /// - `lightshuttle_resources{status="<s>"}` for each `(status, count)` pair
    ///   in `status_counts`.
    /// - `lightshuttle_uptime_seconds` derived from the process start time.
    ///
    /// The returned string is suitable for serving directly as the body of
    /// `GET /metrics` with content type `text/plain; version=0.0.4`.
    #[must_use]
    pub fn render(&self, status_counts: &[(&str, u64)]) -> String {
        for (status, count) in status_counts {
            #[allow(clippy::cast_precision_loss)]
            gauge!(RESOURCES, "status" => (*status).to_owned()).set(*count as f64);
        }
        #[allow(clippy::cast_precision_loss)]
        gauge!(UPTIME).set(self.started.elapsed().as_secs_f64());
        self.handle.render()
    }
}

/// Increment the restart counter. Safe to call from anywhere once the
/// recorder is installed; a no-op when no recorder is present.
pub(crate) fn record_restart() {
    counter!(RESTART_TOTAL).increment(1);
}

/// Record a sample in the `lightshuttle_lifecycle_event_duration_seconds`
/// histogram.
///
/// `seconds` is the elapsed wall time from when the resource was started
/// until it reached a healthy state. Safe to call from any thread once
/// the global recorder is installed via [`Metrics::install`]. A no-op
/// when no recorder is present (e.g. in tests built with
/// [`Metrics::for_test`]).
pub fn observe_event_duration(seconds: f64) {
    histogram!(EVENT_DURATION).record(seconds);
}

fn describe_metrics() {
    describe_counter!(RESTART_TOTAL, "Total number of accepted restart requests");
    describe_histogram!(
        EVENT_DURATION,
        "Seconds a resource takes to go from started to healthy"
    );
    describe_gauge!(RESOURCES, "Number of managed resources, labelled by status");
    describe_gauge!(UPTIME, "Orchestrator uptime in seconds");
}
