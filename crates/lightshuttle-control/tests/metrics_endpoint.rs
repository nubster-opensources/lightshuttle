//! Integration test for `GET /metrics`.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lightshuttle_control::{ControlServer, ControlState, Metrics};
use lightshuttle_runtime::{
    LifecycleEvent, LifecycleHandle, LifecycleHandleError, LogChunkStream, ResourceStatus,
    ResourceView,
};
use tokio::sync::broadcast;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct StubHandle {
    resources: Arc<Mutex<Vec<ResourceView>>>,
}

impl StubHandle {
    fn with_resources(views: Vec<ResourceView>) -> Self {
        Self {
            resources: Arc::new(Mutex::new(views)),
        }
    }
}

impl LifecycleHandle for StubHandle {
    async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> {
        Ok(self.resources.lock().expect("stub mutex").clone())
    }

    async fn get(&self, name: &str) -> Result<ResourceView, LifecycleHandleError> {
        Err(LifecycleHandleError::UnknownResource(name.to_owned()))
    }

    async fn restart(&self, _name: &str) -> Result<(), LifecycleHandleError> {
        Err(LifecycleHandleError::NotSupported("restart"))
    }

    async fn logs(
        &self,
        name: &str,
        _follow: bool,
    ) -> Result<LogChunkStream, LifecycleHandleError> {
        Err(LifecycleHandleError::UnknownResource(name.to_owned()))
    }

    fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
        let (_tx, rx) = broadcast::channel(1);
        rx
    }
}

fn sample(name: &str, kind: &str, status: ResourceStatus) -> ResourceView {
    ResourceView {
        name: name.to_owned(),
        kind: kind.to_owned(),
        status,
        healthy: matches!(status, ResourceStatus::Running),
        image: format!("{kind}:latest"),
        started_at: Some(SystemTime::UNIX_EPOCH),
        last_error: None,
    }
}

#[tokio::test]
async fn metrics_endpoint_serves_prometheus_text() {
    let handle = StubHandle::with_resources(vec![
        sample("cache", "redis", ResourceStatus::Running),
        sample("db", "postgres", ResourceStatus::Failed),
    ]);
    // This test binary has a single test, so installing the global
    // Prometheus recorder exactly once is safe. The macros in
    // `Metrics::render` target the global recorder, so the installed
    // handle is the only one that observes the scrape-time gauges.
    let metrics = Arc::new(Metrics::install());
    let app = ControlServer::new(ControlState::with_metrics("demo", handle, metrics)).into_router();

    let response = app
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        content_type.starts_with("text/plain"),
        "expected Prometheus text content type, got `{content_type}`"
    );

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("utf-8 body");

    // The gauge family is always present; the test recorder renders the
    // describe + the scrape-time samples set during render().
    assert!(
        body.contains("lightshuttle_resources"),
        "metrics body should advertise the resources gauge, got:\n{body}"
    );
    assert!(
        body.contains("lightshuttle_uptime_seconds"),
        "metrics body should advertise the uptime gauge, got:\n{body}"
    );
    assert!(
        body.contains("status=\"running\""),
        "resources gauge should be labelled by status, got:\n{body}"
    );
}
