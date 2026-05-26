//! Axum route assembly.

use axum::Router;

use crate::state::ControlState;

pub(crate) mod healthz;

/// Build the full router for the control plane.
pub(crate) fn router(state: ControlState) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(healthz::healthz))
        .with_state(state)
}
