//! Control-plane facing handle: a stable, backend-agnostic seam over
//! [`crate::LifecycleManager`].
//!
//! The [`LifecycleHandle`] trait exposes only the operations the
//! dashboard, REST API and CLI subcommands need. The concrete
//! [`ManagerHandle`] adapter wraps an `Arc<LifecycleManager<R>>` and
//! erases nothing of substance: the trait stays generic so its callers
//! pay zero allocation per call.

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::broadcast;

use crate::error::RuntimeError;
use crate::lifecycle::manager::LifecycleManager;
use crate::lifecycle::status::LifecycleEvent;
use crate::lifecycle::view::{ResourceStatus, ResourceView, image_label, last_error_from};
use crate::runtime::{ContainerRuntime, LogChunkStream};

/// Errors returned by [`LifecycleHandle`] operations.
#[derive(Debug, Error)]
pub enum LifecycleHandleError {
    /// The requested resource does not exist in the current plan.
    #[error("resource `{0}` does not exist in the current plan")]
    UnknownResource(String),
    /// The handle does not support this operation yet (e.g. `restart`
    /// before the `restart_one` primitive lands in the manager).
    #[error("operation `{0}` is not supported by this handle yet")]
    NotSupported(&'static str),
    /// Underlying runtime error.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

/// Control-plane facing view of a running stack.
///
/// Implementations expose just enough to drive a dashboard, REST API
/// and CLI subcommands without leaking any backend type.
pub trait LifecycleHandle: Send + Sync {
    /// List every resource managed by this stack with its current view.
    fn list(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ResourceView>, LifecycleHandleError>> + Send;

    /// Look up a single resource by name.
    fn get(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<ResourceView, LifecycleHandleError>> + Send;

    /// Restart a single resource by name.
    fn restart(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<(), LifecycleHandleError>> + Send;

    /// Stream logs for a single resource. When `follow` is true the
    /// stream stays open and emits new chunks as they arrive.
    fn logs(
        &self,
        name: &str,
        follow: bool,
    ) -> impl std::future::Future<Output = Result<LogChunkStream, LifecycleHandleError>> + Send;

    /// Open a fresh subscription on the lifecycle event broadcast.
    /// Implementations return a `broadcast::Receiver` so multiple
    /// consumers (REST handlers, WebSocket sessions, CLI followers)
    /// can read independently.
    fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent>;
}

/// Newtype adapter turning an `Arc<LifecycleManager<R>>` into a
/// [`LifecycleHandle`].
pub struct ManagerHandle<R: ContainerRuntime + 'static> {
    inner: Arc<LifecycleManager<R>>,
}

// Manual `Clone` impl: the derived one would require `R: Clone`, but
// the only field is an `Arc`, so cloning a `ManagerHandle` never has
// to clone `R` itself.
impl<R: ContainerRuntime + 'static> Clone for ManagerHandle<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<R: ContainerRuntime + 'static> ManagerHandle<R> {
    /// Wrap a shared manager.
    #[must_use]
    pub fn new(inner: Arc<LifecycleManager<R>>) -> Self {
        Self { inner }
    }

    /// Borrow the underlying manager.
    #[must_use]
    pub fn manager(&self) -> &Arc<LifecycleManager<R>> {
        &self.inner
    }
}

impl<R: ContainerRuntime + 'static> LifecycleHandle for ManagerHandle<R> {
    async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> {
        let plan = self.inner.plan_arc();
        let mut out: Vec<ResourceView> = Vec::with_capacity(plan.nodes().len());
        for node in plan.nodes() {
            let snapshot = self
                .inner
                .snapshot(&node.name)
                .ok_or_else(|| LifecycleHandleError::UnknownResource(node.name.clone()))?;
            out.push(ResourceView {
                name: node.name.clone(),
                kind: node.kind.clone(),
                status: ResourceStatus::from(&snapshot.status),
                healthy: matches!(
                    snapshot.status,
                    crate::lifecycle::status::NodeStatus::Healthy
                ),
                image: image_label(&node.spec.image),
                started_at: snapshot.started_at,
                last_error: last_error_from(&snapshot.status),
            });
        }
        Ok(out)
    }

    async fn get(&self, name: &str) -> Result<ResourceView, LifecycleHandleError> {
        let plan = self.inner.plan_arc();
        let node = plan
            .nodes()
            .iter()
            .find(|n| n.name == name)
            .ok_or_else(|| LifecycleHandleError::UnknownResource(name.to_owned()))?;
        let snapshot = self
            .inner
            .snapshot(name)
            .ok_or_else(|| LifecycleHandleError::UnknownResource(name.to_owned()))?;
        Ok(ResourceView {
            name: node.name.clone(),
            kind: node.kind.clone(),
            status: ResourceStatus::from(&snapshot.status),
            healthy: matches!(
                snapshot.status,
                crate::lifecycle::status::NodeStatus::Healthy
            ),
            image: image_label(&node.spec.image),
            started_at: snapshot.started_at,
            last_error: last_error_from(&snapshot.status),
        })
    }

    async fn restart(&self, name: &str) -> Result<(), LifecycleHandleError> {
        self.inner.restart_one(name).await.map_err(|err| match err {
            crate::LifecycleError::ResourceNotFound(name) => {
                LifecycleHandleError::UnknownResource(name)
            }
            crate::LifecycleError::Start { source, .. }
            | crate::LifecycleError::Stop { source, .. } => {
                LifecycleHandleError::Runtime(source)
            }
            crate::LifecycleError::SpecBuild { source, .. } => {
                LifecycleHandleError::Runtime(RuntimeError::InvalidSpec(source.to_string()))
            }
            other => LifecycleHandleError::Runtime(RuntimeError::InvalidSpec(other.to_string())),
        })
    }

    async fn logs(&self, name: &str, follow: bool) -> Result<LogChunkStream, LifecycleHandleError> {
        let plan = self.inner.plan_arc();
        if !plan.nodes().iter().any(|n| n.name == name) {
            return Err(LifecycleHandleError::UnknownResource(name.to_owned()));
        }
        let snapshot = self
            .inner
            .snapshot(name)
            .ok_or_else(|| LifecycleHandleError::UnknownResource(name.to_owned()))?;
        let container_id = snapshot.container_id.ok_or_else(|| {
            LifecycleHandleError::Runtime(RuntimeError::InvalidSpec(format!(
                "resource `{name}` is not running"
            )))
        })?;
        let stream = self.inner.runtime_arc().logs(&container_id, follow).await?;
        Ok(stream)
    }

    fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
        self.inner.subscribe_events()
    }
}
