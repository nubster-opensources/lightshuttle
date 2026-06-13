//! `GET /metrics`: Prometheus text exposition format.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use lightshuttle_runtime::{LifecycleHandle, ResourceStatus};

use crate::state::ControlState;

/// Prometheus exposition content type per the text format spec.
const PROM_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// `GET /metrics`: refresh scrape-time gauges and render.
pub(crate) async fn metrics<H>(State(state): State<ControlState<H>>) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let counts = match state.handle.list().await {
        Ok(views) => count_by_status(&views),
        Err(err) => {
            tracing::error!(error = %err, "failed to list resources for /metrics");
            StatusCounts::default()
        }
    };

    let body = state.metrics.render(&counts.as_label_pairs());
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, PROM_CONTENT_TYPE)],
        body,
    )
        .into_response()
}

/// Per-status resource tallies for the `lightshuttle_resources` gauge.
#[derive(Default)]
struct StatusCounts {
    pending: u64,
    starting: u64,
    running: u64,
    failed: u64,
    stopped: u64,
}

impl StatusCounts {
    fn as_label_pairs(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("pending", self.pending),
            ("starting", self.starting),
            ("running", self.running),
            ("failed", self.failed),
            ("stopped", self.stopped),
        ]
    }
}

fn count_by_status(views: &[lightshuttle_runtime::ResourceView]) -> StatusCounts {
    let mut counts = StatusCounts::default();
    for view in views {
        match view.status {
            ResourceStatus::Pending => counts.pending += 1,
            ResourceStatus::Starting => counts.starting += 1,
            ResourceStatus::Running => counts.running += 1,
            ResourceStatus::Failed => counts.failed += 1,
            ResourceStatus::Stopped => counts.stopped += 1,
        }
    }
    counts
}
