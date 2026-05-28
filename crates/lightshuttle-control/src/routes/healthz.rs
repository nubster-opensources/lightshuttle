//! `GET /healthz` liveness probe.

use axum::Json;
use axum::extract::State;
use lightshuttle_runtime::LifecycleHandle;
use serde::Serialize;

use crate::state::ControlState;

/// JSON body returned by `/healthz`.
#[derive(Debug, Serialize)]
pub(crate) struct HealthzResponse {
    /// Always `"ok"` when the server is up.
    pub(crate) status: &'static str,
    /// Project name as declared in the manifest.
    pub(crate) project: String,
}

/// Handler for `GET /healthz`.
pub(crate) async fn healthz<H>(State(state): State<ControlState<H>>) -> Json<HealthzResponse>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    Json(HealthzResponse {
        status: "ok",
        project: state.project,
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use lightshuttle_runtime::{
        LifecycleEvent, LifecycleHandle, LifecycleHandleError, LogChunkStream, ResourceView,
    };
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use crate::routes::router;
    use crate::state::ControlState;

    /// Minimal handle that returns empty data; satisfies the router's
    /// type bounds for routes that do not touch it.
    #[derive(Clone, Default)]
    struct NopHandle;

    impl LifecycleHandle for NopHandle {
        async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> {
            Ok(Vec::new())
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

    #[tokio::test]
    async fn returns_200_with_status_and_project() {
        let app = router(ControlState::new("demo", NopHandle));

        let response = app
            .oneshot(
                Request::get("/healthz")
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
        assert_eq!(json["status"], "ok");
        assert_eq!(json["project"], "demo");
    }
}
