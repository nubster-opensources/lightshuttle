//! `GET /api/resources` and `GET /api/resources/:name`.

use axum::Json;
use axum::extract::{Path, State};
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
