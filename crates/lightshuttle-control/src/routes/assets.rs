//! Embedded static assets served from `/_assets/`.
//!
//! The CSS and the HTMX library are baked into the binary via
//! `include_bytes!` so a single `lightshuttle` binary boots the
//! dashboard without any filesystem dependency.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

const STYLE_CSS: &[u8] = include_bytes!("../../assets/style.css");
const HTMX_JS: &[u8] = include_bytes!("../../assets/htmx.min.js");

fn serve(bytes: &'static [u8], content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    )
        .into_response()
}

/// `GET /_assets/style.css`.
pub(crate) async fn style_css() -> Response {
    serve(STYLE_CSS, "text/css; charset=utf-8")
}

/// `GET /_assets/htmx.min.js`.
pub(crate) async fn htmx_js() -> Response {
    serve(HTMX_JS, "application/javascript; charset=utf-8")
}
