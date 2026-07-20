//! Axum route assembly.

use axum::Router;
use axum::extract::Request;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, HOST, ORIGIN, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
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
        .layer(axum::middleware::from_fn(enforce_local_browser_boundary))
        .layer(axum::middleware::from_fn(security_headers))
}

/// Reject browser requests that did not originate from the local control
/// plane itself.
///
/// Binding to loopback prevents direct remote connections, but it does not
/// stop a hostile web page from opening a WebSocket or submitting a POST to
/// localhost. Browsers attach `Origin` and Fetch Metadata headers to those
/// requests, so enforce a same-origin boundary whenever they are present.
/// Non-browser clients such as the LightShuttle CLI do not send those headers
/// and remain supported, provided their `Host` header targets loopback.
async fn enforce_local_browser_boundary(request: Request, next: Next) -> Response {
    if !is_allowed_local_request(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden cross-origin request").into_response();
    }
    next.run(request).await
}

fn is_allowed_local_request(headers: &HeaderMap) -> bool {
    let host = match headers.get(HOST) {
        Some(value) => match value.to_str() {
            Ok(value) if is_loopback_authority(value) => Some(value),
            _ => return false,
        },
        // In-process router users and HTTP/2 test harnesses may omit Host.
        // Real HTTP/1.1 browser requests always carry it.
        None => None,
    };

    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
    {
        return false;
    }

    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    if !is_loopback_authority(authority.as_str()) {
        return false;
    }

    host.is_none_or(|host| authority.as_str().eq_ignore_ascii_case(host))
}

fn is_loopback_authority(raw: &str) -> bool {
    let Ok(authority) = raw.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    matches!(
        authority.host(),
        "127.0.0.1" | "[::1]" | "::1" | "localhost"
    )
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
