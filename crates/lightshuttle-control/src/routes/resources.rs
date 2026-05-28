//! `GET /api/resources`, `GET /api/resources/:name` and
//! `POST /api/resources/:name/restart`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use lightshuttle_runtime::{LifecycleHandle, ResourceView};

use crate::error::ApiError;
use crate::state::ControlState;

/// `GET /api/resources` — list every resource managed by the stack.
pub(crate) async fn list_resources<H>(
    State(state): State<ControlState<H>>,
) -> Result<Json<Vec<ResourceView>>, ApiError>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let views = state.handle.list().await?;
    Ok(Json(views))
}

/// `GET /api/resources/:name` — fetch a single resource view.
pub(crate) async fn get_resource<H>(
    State(state): State<ControlState<H>>,
    Path(name): Path<String>,
) -> Result<Json<ResourceView>, ApiError>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let view = state.handle.get(&name).await?;
    Ok(Json(view))
}

/// `POST /api/resources/:name/restart` — schedule a restart and return
/// immediately. The actual outcome is observable on `/ws/events`.
///
/// Existence of the resource is verified synchronously so the response
/// can be `404` when the name is unknown, even though the restart
/// itself runs in a detached task.
pub(crate) async fn restart_resource<H>(
    State(state): State<ControlState<H>>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    // Surface 404 immediately when the resource is unknown.
    let _ = state.handle.get(&name).await?;

    let handle = state.handle.clone();
    let resource = name.clone();
    tokio::spawn(async move {
        if let Err(err) = handle.restart(&resource).await {
            tracing::error!(error = %err, resource = %resource, "restart failed");
        }
    });

    Ok(StatusCode::ACCEPTED)
}
