//! Axum route assembly.

use axum::Router;
use lightshuttle_runtime::LifecycleHandle;

use crate::state::ControlState;

pub(crate) mod healthz;
pub(crate) mod logs_ws;
pub(crate) mod resources;

/// Build the full router for the control plane.
///
/// REST endpoints sit under `/api` to leave `/` free for the SSR
/// dashboard introduced in a follow-up PR. WebSocket endpoints sit
/// under `/ws` for the same reason.
pub(crate) fn router<H>(state: ControlState<H>) -> Router
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let api = Router::new()
        .route("/resources", axum::routing::get(resources::list_resources))
        .route(
            "/resources/{name}",
            axum::routing::get(resources::get_resource),
        );

    let ws = Router::new().route("/logs/{name}", axum::routing::get(logs_ws::logs_ws));

    Router::new()
        .route("/healthz", axum::routing::get(healthz::healthz))
        .nest("/api", api)
        .nest("/ws", ws)
        .with_state(state)
}
