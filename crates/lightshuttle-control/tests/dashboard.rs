//! Integration tests for the SSR dashboard and the embedded assets.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lightshuttle_control::{ControlServer, ControlState};
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
        self.resources
            .lock()
            .expect("stub mutex")
            .iter()
            .find(|v| v.name == name)
            .cloned()
            .ok_or_else(|| LifecycleHandleError::UnknownResource(name.to_owned()))
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

fn sample(name: &str, kind: &str, status: ResourceStatus, healthy: bool) -> ResourceView {
    ResourceView {
        name: name.to_owned(),
        kind: kind.to_owned(),
        status,
        healthy,
        image: format!("{kind}:latest"),
        started_at: Some(SystemTime::UNIX_EPOCH),
        last_error: None,
    }
}

fn build_app(views: Vec<ResourceView>) -> axum::Router {
    let handle = StubHandle::with_resources(views);
    let state = ControlState::new("demo", handle);
    ControlServer::new(state).into_router()
}

async fn body_text(response: axum::response::Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8 body")
}

#[tokio::test]
async fn index_renders_a_row_per_resource_and_links_to_assets() {
    let app = build_app(vec![
        sample("cache", "redis", ResourceStatus::Running, true),
        sample("db", "postgres", ResourceStatus::Failed, false),
    ]);

    let response = app
        .oneshot(
            Request::get("/")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_text(response).await;

    // Layout boilerplate and asset links.
    assert!(body.contains("<title>Resources"));
    assert!(body.contains("/_assets/style.css"));
    assert!(body.contains("/_assets/htmx.min.js"));

    // HTMX polling on the partial.
    assert!(body.contains("hx-get=\"/_partials/resources\""));
    assert!(body.contains("hx-trigger=\"every 2s\""));

    // Each resource gets a row and a restart button targeting the
    // backend API.
    assert!(body.contains("cache"));
    assert!(body.contains("db"));
    assert!(body.contains("status-running"));
    assert!(body.contains("status-failed"));
    assert!(body.contains("hx-post=\"/api/resources/cache/restart\""));
    assert!(body.contains("hx-post=\"/api/resources/db/restart\""));
}

#[tokio::test]
async fn partial_resources_returns_the_table_only() {
    let app = build_app(vec![sample(
        "cache",
        "redis",
        ResourceStatus::Running,
        true,
    )]);

    let response = app
        .oneshot(
            Request::get("/_partials/resources")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_text(response).await;

    // Partial = no layout, just the table swap target.
    assert!(!body.contains("<html"));
    assert!(body.contains("resource-table"));
    assert!(body.contains("cache"));
}

#[tokio::test]
async fn resource_detail_renders_metadata_and_log_pane() {
    let app = build_app(vec![sample(
        "cache",
        "redis",
        ResourceStatus::Running,
        true,
    )]);

    let response = app
        .oneshot(
            Request::get("/resources/cache")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_text(response).await;

    assert!(body.contains("<title>cache"));
    assert!(body.contains("log-pane"));
    assert!(body.contains("data-resource=\"cache\""));
    assert!(body.contains("redis:latest"));
}

#[tokio::test]
async fn resource_detail_returns_404_for_unknown() {
    let app = build_app(vec![sample(
        "cache",
        "redis",
        ResourceStatus::Running,
        true,
    )]);

    let response = app
        .oneshot(
            Request::get("/resources/nope")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assets_serve_embedded_css_and_htmx_with_cache_headers() {
    let app = build_app(Vec::new());

    let css = app
        .clone()
        .oneshot(
            Request::get("/_assets/style.css")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(css.status(), StatusCode::OK);
    assert_eq!(
        css.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/css; charset=utf-8"
    );
    assert!(css.headers().get(header::CACHE_CONTROL).is_some());

    let js = app
        .oneshot(
            Request::get("/_assets/htmx.min.js")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(js.status(), StatusCode::OK);
    assert_eq!(
        js.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/javascript; charset=utf-8"
    );

    let bytes = js
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    // HTMX 2.x bundles always reference the lib name in their body.
    assert!(bytes.len() > 10_000);
}

#[tokio::test]
async fn responses_carry_baseline_security_headers() {
    let app = build_app(vec![sample(
        "cache",
        "redis",
        ResourceStatus::Running,
        true,
    )]);

    let response = app
        .oneshot(
            Request::get("/")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let headers = response.headers();
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    let csp = headers
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(csp.contains("default-src 'self'"), "got CSP: {csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "got CSP: {csp}");
}
