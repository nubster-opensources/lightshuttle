//! Axum route assembly.

use axum::Router;
use lightshuttle_runtime::LifecycleHandle;

use crate::state::ControlState;

pub(crate) mod assets;
pub(crate) mod dashboard;
pub(crate) mod events_ws;
pub(crate) mod healthz;
pub(crate) mod logs_ws;
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
        .nest("/api", api)
        .nest("/ws", ws)
        .nest("/_assets", assets)
        .nest("/_partials", partials)
        .with_state(state)
}
