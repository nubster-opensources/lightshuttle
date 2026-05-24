//! Coordinated startup and shutdown of every resource declared in a
//! [`crate::LifecyclePlan`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::error::RuntimeError;
use crate::lifecycle::error::LifecycleError;
use crate::lifecycle::plan::LifecyclePlan;
use crate::lifecycle::status::{LifecycleEvent, NodeStatus};
use crate::runtime::{ContainerId, ContainerRuntime};
use crate::spec::ContainerSpec;

/// Default healthcheck timeout, applied when the manifest does not
/// provide one of its own. Kept conservative for v0.1.
const DEFAULT_HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-resource shared state.
#[derive(Clone)]
struct NodeHandle {
    status_tx: Arc<watch::Sender<NodeStatus>>,
    status_rx: watch::Receiver<NodeStatus>,
    container_id: Arc<Mutex<Option<ContainerId>>>,
}

/// Coordinates the startup, supervision and shutdown of every resource
/// declared in a [`LifecyclePlan`].
pub struct LifecycleManager<R: ContainerRuntime + 'static> {
    plan: Arc<LifecyclePlan>,
    runtime: Arc<R>,
    nodes: HashMap<String, NodeHandle>,
    event_tx: mpsc::UnboundedSender<LifecycleEvent>,
}

impl<R: ContainerRuntime + 'static> LifecycleManager<R> {
    /// Build a manager bound to `plan` and `runtime`. Returns the event
    /// stream receiver alongside.
    #[must_use]
    pub fn new(plan: LifecyclePlan, runtime: R) -> (Self, mpsc::UnboundedReceiver<LifecycleEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut nodes: HashMap<String, NodeHandle> = HashMap::new();
        for node in plan.nodes() {
            let (tx, rx) = watch::channel(NodeStatus::Pending);
            nodes.insert(
                node.name.clone(),
                NodeHandle {
                    status_tx: Arc::new(tx),
                    status_rx: rx,
                    container_id: Arc::new(Mutex::new(None)),
                },
            );
        }
        let manager = Self {
            plan: Arc::new(plan),
            runtime: Arc::new(runtime),
            nodes,
            event_tx,
        };
        (manager, event_rx)
    }

    /// Start every resource in topological order. Independent branches
    /// start in parallel. On the first failure, the resources that
    /// already started are stopped automatically before the error is
    /// returned.
    pub async fn start_all(&self) -> Result<(), LifecycleError> {
        let mut handles: Vec<tokio::task::JoinHandle<Result<(), LifecycleError>>> =
            Vec::with_capacity(self.plan.nodes().len());

        for node in self.plan.nodes() {
            let dep_receivers: HashMap<String, watch::Receiver<NodeStatus>> = node
                .depends_on
                .iter()
                .map(|dep| {
                    let handle = self
                        .nodes
                        .get(dep)
                        .ok_or_else(|| LifecycleError::UnknownResource(dep.clone()))?;
                    Ok::<_, LifecycleError>((dep.clone(), handle.status_rx.clone()))
                })
                .collect::<Result<_, _>>()?;

            let node_handle = self.nodes[&node.name].clone();
            let spec = node.spec.clone();
            let name = node.name.clone();
            let runtime = Arc::clone(&self.runtime);
            let event_tx = self.event_tx.clone();

            let task = tokio::spawn(async move {
                start_one(name, spec, runtime, node_handle, dep_receivers, event_tx).await
            });
            handles.push(task);
        }

        let mut first_error: Option<LifecycleError> = None;
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
                Err(join_err) => {
                    if first_error.is_none() {
                        first_error = Some(LifecycleError::Start {
                            resource: "<panicked task>".to_owned(),
                            source: RuntimeError::InvalidSpec(join_err.to_string()),
                        });
                    }
                }
            }
        }

        if let Some(err) = first_error {
            warn!(error = %err, "start_all failed; rolling back");
            let _ = self.stop_all(Duration::from_secs(10)).await;
            return Err(err);
        }

        let _ = self.event_tx.send(LifecycleEvent::StackStarted);
        info!(
            "stack started: {} resource(s) healthy",
            self.plan.nodes().len()
        );
        Ok(())
    }

    /// Stop every resource in reverse topological order with the given
    /// SIGTERM-to-SIGKILL grace window.
    pub async fn stop_all(&self, grace: Duration) -> Result<(), LifecycleError> {
        let _ = self.event_tx.send(LifecycleEvent::StackStopping);

        let mut errors: Vec<(String, RuntimeError)> = Vec::new();
        for node in self.plan.nodes().iter().rev() {
            let Some(handle) = self.nodes.get(&node.name) else {
                continue;
            };
            let id = {
                let guard = handle
                    .container_id
                    .lock()
                    .expect("container_id mutex poisoned");
                guard.clone()
            };
            let Some(id) = id else { continue };
            match self.runtime.stop(&id, grace).await {
                Ok(()) => {
                    let _ = handle.status_tx.send(NodeStatus::Stopped);
                    let _ = self.event_tx.send(LifecycleEvent::ResourceStopped {
                        name: node.name.clone(),
                    });
                }
                Err(e) => errors.push((node.name.clone(), e)),
            }
        }

        let _ = self.event_tx.send(LifecycleEvent::StackStopped);

        if let Some((resource, source)) = errors.into_iter().next() {
            return Err(LifecycleError::Stop { resource, source });
        }
        Ok(())
    }

    /// Opinionated entry point used by `lightshuttle up`: starts the
    /// stack, waits for `SIGINT` or `SIGTERM`, then stops the stack
    /// cleanly with the configured grace window.
    pub async fn run_until_signal(&self, grace: Duration) -> Result<(), LifecycleError> {
        self.start_all().await?;
        wait_for_shutdown_signal().await;
        self.stop_all(grace).await
    }
}

async fn start_one<R: ContainerRuntime + 'static>(
    name: String,
    spec: ContainerSpec,
    runtime: Arc<R>,
    handle: NodeHandle,
    dep_receivers: HashMap<String, watch::Receiver<NodeStatus>>,
    event_tx: mpsc::UnboundedSender<LifecycleEvent>,
) -> Result<(), LifecycleError> {
    // 1. Wait for every dependency to become ready.
    for (dep_name, mut rx) in dep_receivers {
        loop {
            let status = rx.borrow_and_update().clone();
            if status.is_ready() {
                debug!(node = %name, dep = %dep_name, "dependency ready");
                break;
            }
            if let NodeStatus::Failed { reason } = status {
                let _ = handle.status_tx.send(NodeStatus::Failed {
                    reason: format!("dependency `{dep_name}` failed: {reason}"),
                });
                return Err(LifecycleError::DependencyFailed {
                    resource: name,
                    dependency: dep_name,
                    reason,
                });
            }
            if rx.changed().await.is_err() {
                let reason = format!("dependency `{dep_name}` watch channel closed");
                let _ = handle.status_tx.send(NodeStatus::Failed {
                    reason: reason.clone(),
                });
                return Err(LifecycleError::DependencyFailed {
                    resource: name,
                    dependency: dep_name,
                    reason,
                });
            }
        }
    }

    // 2. Start the container.
    let _ = handle.status_tx.send(NodeStatus::Starting);
    let id = match runtime.start(&spec).await {
        Ok(id) => id,
        Err(source) => {
            let _ = handle.status_tx.send(NodeStatus::Failed {
                reason: source.to_string(),
            });
            let _ = event_tx.send(LifecycleEvent::ResourceFailed {
                name: name.clone(),
                error: source.to_string(),
            });
            return Err(LifecycleError::Start {
                resource: name,
                source,
            });
        }
    };

    {
        let mut guard = handle
            .container_id
            .lock()
            .expect("container_id mutex poisoned");
        *guard = Some(id.clone());
    }
    let _ = handle.status_tx.send(NodeStatus::Running);
    let _ = event_tx.send(LifecycleEvent::ResourceStarted {
        name: name.clone(),
        container_id: id.to_string(),
    });

    // 3. Wait for the healthcheck.
    match runtime.wait_healthy(&id, DEFAULT_HEALTHCHECK_TIMEOUT).await {
        Ok(()) => {
            let _ = handle.status_tx.send(NodeStatus::Healthy);
            let _ = event_tx.send(LifecycleEvent::ResourceHealthy { name: name.clone() });
            Ok(())
        }
        Err(RuntimeError::Timeout { .. }) => {
            let reason = format!("healthcheck timed out after {DEFAULT_HEALTHCHECK_TIMEOUT:?}");
            let _ = handle.status_tx.send(NodeStatus::Failed {
                reason: reason.clone(),
            });
            let _ = event_tx.send(LifecycleEvent::ResourceFailed {
                name: name.clone(),
                error: reason,
            });
            Err(LifecycleError::HealthcheckTimeout {
                resource: name,
                timeout: DEFAULT_HEALTHCHECK_TIMEOUT,
            })
        }
        Err(source) => {
            let _ = handle.status_tx.send(NodeStatus::Failed {
                reason: source.to_string(),
            });
            let _ = event_tx.send(LifecycleEvent::ResourceFailed {
                name: name.clone(),
                error: source.to_string(),
            });
            Err(LifecycleError::Start {
                resource: name,
                source,
            })
        }
    }
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            warn!("failed to install SIGTERM handler: {e}");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => info!("received SIGINT"),
        _ = sigterm.recv() => info!("received SIGTERM"),
    }
}

#[cfg(windows)]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("received Ctrl+C");
}
