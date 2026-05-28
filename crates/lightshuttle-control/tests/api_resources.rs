//! Integration tests for `GET /api/resources` and
//! `GET /api/resources/:name` using an in-memory `LifecycleHandle`
//! stub.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lightshuttle_control::{ControlServer, ControlState};
use lightshuttle_runtime::{
    LifecycleHandle, LifecycleHandleError, LogChunkStream, ResourceStatus, ResourceView,
};
use tower::ServiceExt;

/// In-memory lifecycle handle whose state is fully controlled by the
/// test. Cheap to clone (every field is an `Arc`).
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
}

fn sample_view(name: &str, kind: &str) -> ResourceView {
    ResourceView {
        name: name.to_owned(),
        kind: kind.to_owned(),
        status: ResourceStatus::Running,
        healthy: true,
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

#[tokio::test]
async fn list_returns_every_resource_as_json_array() {
    let app = build_app(vec![
        sample_view("cache", "redis"),
        sample_view("db", "postgres"),
    ]);

    let response = app
        .oneshot(
            Request::get("/api/resources")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    let arr = json.as_array().expect("array body");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["name"], "cache");
    assert_eq!(arr[0]["kind"], "redis");
    assert_eq!(arr[0]["status"], "Running");
    assert_eq!(arr[1]["name"], "db");
}

#[tokio::test]
async fn list_returns_empty_array_when_stack_is_empty() {
    let app = build_app(Vec::new());

    let response = app
        .oneshot(
            Request::get("/api/resources")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    assert_eq!(bytes.as_ref(), b"[]");
}

#[tokio::test]
async fn get_returns_single_view_for_known_resource() {
    let app = build_app(vec![sample_view("cache", "redis")]);

    let response = app
        .oneshot(
            Request::get("/api/resources/cache")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(json["name"], "cache");
    assert_eq!(json["kind"], "redis");
    assert_eq!(json["healthy"], true);
}

#[tokio::test]
async fn get_returns_404_with_error_body_for_unknown_resource() {
    let app = build_app(vec![sample_view("cache", "redis")]);

    let response = app
        .oneshot(
            Request::get("/api/resources/nope")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(json["error"], "unknown resource");
    assert_eq!(json["resource"], "nope");
}
