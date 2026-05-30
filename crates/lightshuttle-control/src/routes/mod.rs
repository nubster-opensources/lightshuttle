//! Axum route assembly.

use axum::Router;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::header::{CONTENT_SECURITY_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS};
use axum::middleware::Next;
use axum::response::Response;
use lightshuttle_runtime::LifecycleHandle;

use crate::state::ControlState;

/// Content Security Policy for the dashboard.
///
/// Everything loads from the same origin. `script-src` allows the inline
/// bootstrap script in the resource detail page; `connect-src 'self'`
/// covers the same-origin log and event web sockets.
const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline'; \
     style-src 'self'; \
     connect-src 'self'; \
     img-src 'self' data:; \
     base-uri 'none'; \
     frame-ancestors 'none'";

pub(crate) mod assets;
pub(crate) mod dashboard;
pub(crate) mod events_ws;
pub(crate) mod healthz;
pub(crate) mod logs_ws;
pub(crate) mod metrics;
pub(crate) mod resources;

/// Build the full router for the control plane.
///
/// REST endpoints sit under `/api`, WebSocket endpoints under `/ws`,
/// static assets under `/_assets`, HTMX partials under `/_partials`,
/// and the SSR dashboard pages live at the root.
pub(crate) fn router<H>(state: ControlState<H>) -> Router
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let api = Router::new()
        .route("/resources", axum::routing::get(resources::list_resources))
        .route(
            "/resources/{name}",
            axum::routing::get(resources::get_resource),
        )
        .route(
            "/resources/{name}/restart",
            axum::routing::post(resources::restart_resource),
        );

    let ws = Router::new()
        .route("/logs/{name}", axum::routing::get(logs_ws::logs_ws))
        .route("/events", axum::routing::get(events_ws::events_ws));

    let assets = Router::new()
        .route("/style.css", axum::routing::get(assets::style_css))
        .route("/htmx.min.js", axum::routing::get(assets::htmx_js));

    let partials = Router::new().route("/resources", axum::routing::get(dashboard::status_table));

    Router::new()
        .route("/", axum::routing::get(dashboard::index))
        .route("/resources/{name}", axum::routing::get(dashboard::resource))
        .route("/healthz", axum::routing::get(healthz::healthz))
        .route("/metrics", axum::routing::get(metrics::metrics))
        .nest("/api", api)
        .nest("/ws", ws)
        .nest("/_assets", assets)
        .nest("/_partials", partials)
        .with_state(state)
        .layer(axum::middleware::from_fn(security_headers))
}

/// Attach baseline security headers to every response.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY_VALUE),
    );
    response
}
