//! JSON-shaped HTTP errors returned by every REST endpoint.
//!
//! Every REST handler maps [`lightshuttle_runtime::LifecycleHandleError`]
//! to one of three HTTP status codes via the [`From`] impl on [`ApiError`].
//! Axum serialises the response through the [`axum::response::IntoResponse`]
//! impl, which pairs the status with a JSON [`ApiErrorBody`].

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lightshuttle_runtime::LifecycleHandleError;
use serde::Serialize;

/// Wire representation of an API error body, serialised as JSON.
///
/// All REST endpoints that can fail return this structure as their error
/// payload. The `resource` field is omitted from the JSON output when it
/// is not applicable (`null` would be misleading for non-resource errors).
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    /// Short, machine-friendly slug describing the error category.
    pub error: String,
    /// Resource name when the error is scoped to a single resource.
    ///
    /// Absent from the serialised output when `None`
    /// (controlled by `#[serde(skip_serializing_if)]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// HTTP error type returned by every control-plane REST handler.
///
/// Wraps an HTTP status code and a JSON body ([`ApiErrorBody`]). Handlers
/// return `Result<_, ApiError>` and rely on the [`axum::response::IntoResponse`]
/// impl to render the response automatically.
///
/// `ApiError` also implements [`From`]`<`[`lightshuttle_runtime::LifecycleHandleError`]`>`,
/// so the `?` operator converts runtime errors into the right HTTP status without
/// boilerplate in each handler.
///
/// Three constructors cover all current failure modes:
///
/// | Constructor | HTTP status | When to use |
/// |---|---|---|
/// | [`ApiError::unknown_resource`] | 404 | Named resource does not exist |
/// | [`ApiError::not_supported`] | 501 | Operation not implemented yet |
/// | [`ApiError::runtime`] | 500 | Unexpected runtime failure |
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
}

impl ApiError {
    /// Build a 404 response for a resource that does not exist.
    ///
    /// The serialised body is `{"error":"unknown resource","resource":"<name>"}`.
    ///
    /// Prefer this over constructing [`ApiError`] directly so the HTTP
    /// status and the error slug remain consistent across all handlers.
    #[must_use]
    pub fn unknown_resource(name: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: ApiErrorBody {
                error: "unknown resource".to_owned(),
                resource: Some(name.into()),
            },
        }
    }

    /// Build a 501 response for an operation that is not yet implemented.
    ///
    /// The serialised body is ``{"error":"operation `op` is not supported yet"}``.
    #[must_use]
    pub fn not_supported(op: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            body: ApiErrorBody {
                error: format!("operation `{op}` is not supported yet"),
                resource: None,
            },
        }
    }

    /// Build a 409 response for a restart that conflicts with one already
    /// in flight for the same resource.
    ///
    /// The serialised body is
    /// `{"error":"restart already in progress","resource":"<name>"}`.
    #[must_use]
    pub fn restart_in_progress(name: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: ApiErrorBody {
                error: "restart already in progress".to_owned(),
                resource: Some(name.into()),
            },
        }
    }

    /// Build a 500 response for an unexpected runtime failure.
    ///
    /// The serialised body is `{"error":"<message>"}`.
    #[must_use]
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ApiErrorBody {
                error: message.into(),
                resource: None,
            },
        }
    }
}

impl From<LifecycleHandleError> for ApiError {
    fn from(err: LifecycleHandleError) -> Self {
        match err {
            LifecycleHandleError::UnknownResource(name) => Self::unknown_resource(name),
            LifecycleHandleError::NotSupported(op) => Self::not_supported(op),
            LifecycleHandleError::RestartInProgress(name) => Self::restart_in_progress(name),
            LifecycleHandleError::Runtime(e) => Self::runtime(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
