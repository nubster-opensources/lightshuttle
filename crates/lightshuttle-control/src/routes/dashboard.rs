//! SSR dashboard pages rendered with Askama.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use lightshuttle_runtime::{LifecycleHandle, ResourceStatus, ResourceView};

use crate::state::ControlState;

/// View model passed to every dashboard template.
struct ResourceCard {
    name: String,
    kind: String,
    status: &'static str,
    status_class: &'static str,
    healthy: bool,
    image: String,
    last_error: Option<String>,
}

impl From<ResourceView> for ResourceCard {
    fn from(view: ResourceView) -> Self {
        let (status, status_class) = render_status(view.status);
        Self {
            name: view.name,
            kind: view.kind,
            status,
            status_class,
            healthy: view.healthy,
            image: view.image,
            last_error: view.last_error,
        }
    }
}

fn render_status(status: ResourceStatus) -> (&'static str, &'static str) {
    match status {
        ResourceStatus::Pending => ("pending", "pending"),
        ResourceStatus::Starting => ("starting", "starting"),
        ResourceStatus::Running => ("running", "running"),
        ResourceStatus::Failed => ("failed", "failed"),
        ResourceStatus::Stopped => ("stopped", "stopped"),
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    project: &'a str,
    resources: &'a [ResourceCard],
}

#[derive(Template)]
#[template(path = "_status_table.html")]
struct StatusTableTemplate<'a> {
    resources: &'a [ResourceCard],
}

#[derive(Template)]
#[template(path = "resource.html")]
struct ResourceTemplate<'a> {
    project: &'a str,
    resource: &'a ResourceCard,
}

/// `GET /` — index page listing every managed resource.
pub(crate) async fn index<H>(State(state): State<ControlState<H>>) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let cards = collect_cards(&state).await;
    let template = IndexTemplate {
        project: &state.project,
        resources: &cards,
    };
    render(&template)
}

/// `GET /_partials/resources` — HTMX partial reused by polling.
pub(crate) async fn status_table<H>(State(state): State<ControlState<H>>) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let cards = collect_cards(&state).await;
    let template = StatusTableTemplate { resources: &cards };
    render(&template)
}

/// `GET /resources/{name}` — detail page for a single resource.
pub(crate) async fn resource<H>(
    State(state): State<ControlState<H>>,
    Path(name): Path<String>,
) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    match state.handle.get(&name).await {
        Ok(view) => {
            let card = ResourceCard::from(view);
            let template = ResourceTemplate {
                project: &state.project,
                resource: &card,
            };
            render(&template)
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("resource `{name}` not found"),
        )
            .into_response(),
    }
}

async fn collect_cards<H>(state: &ControlState<H>) -> Vec<ResourceCard>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    match state.handle.list().await {
        Ok(views) => views.into_iter().map(ResourceCard::from).collect(),
        Err(err) => {
            tracing::error!(error = %err, "failed to list resources for dashboard");
            Vec::new()
        }
    }
}

fn render<T: Template>(template: &T) -> Response {
    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "template render failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "dashboard template render failed",
            )
                .into_response()
        }
    }
}
