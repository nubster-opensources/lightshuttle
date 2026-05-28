//! JSON-shaped HTTP errors returned by every REST endpoint.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lightshuttle_runtime::LifecycleHandleError;
use serde::Serialize;

/// Wire representation of an API error.
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    /// Short, machine-friendly slug describing the error category.
    pub error: String,
    /// Resource name when the error is scoped to a single resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// Translates `LifecycleHandleError` variants into HTTP status + JSON
/// body. Kept as a plain struct (not an enum) because every handler
/// reduces failures to one of three buckets: not found, not yet
/// implemented, or runtime fault.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
}

impl ApiError {
    /// 404 with `{"error":"unknown resource","resource":"<name>"}`.
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

    /// 501 with `{"error":"not supported","resource":null}`.
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

    /// 500 with `{"error":"runtime error","resource":null}`.
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
            LifecycleHandleError::Runtime(e) => Self::runtime(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
