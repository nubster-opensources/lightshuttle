//! `GET /healthz` liveness probe.

use axum::Json;
use axum::extract::State;
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
pub(crate) async fn healthz(State(state): State<ControlState>) -> Json<HealthzResponse> {
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
    use tower::ServiceExt;

    use crate::routes::router;
    use crate::state::ControlState;

    #[tokio::test]
    async fn returns_200_with_status_and_project() {
        let app = router(ControlState::new("demo"));

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
