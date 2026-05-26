//! Coordinated startup and shutdown of every resource declared in a
//! [`crate::LifecyclePlan`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use lightshuttle_manifest::{InterpolationContext, Interpolator};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::error::RuntimeError;
use crate::lifecycle::error::LifecycleError;
use crate::lifecycle::plan::LifecyclePlan;
use crate::lifecycle::status::{LifecycleEvent, NodeStatus};
use crate::runtime::{ContainerId, ContainerRuntime};
use crate::spec::{ContainerSpec, ResourceOutputs};

/// Default healthcheck timeout, applied when the manifest does not
/// provide one of its own. Kept conservative for v0.1.
const DEFAULT_HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-resource shared state.
#[derive(Clone)]
struct NodeHandle {
    status_tx: Arc<watch::Sender<NodeStatus>>,
    status_rx: watch::Receiver<NodeStatus>,
    outputs_tx: Arc<watch::Sender<Option<ResourceOutputs>>>,
    outputs_rx: watch::Receiver<Option<ResourceOutputs>>,
    container_id: Arc<Mutex<Option<ContainerId>>>,
    started_at: Arc<Mutex<Option<SystemTime>>>,
}

/// Point-in-time snapshot of one managed resource, consumed by the
/// control plane via [`super::handle::ManagerHandle`].
pub(super) struct NodeSnapshot {
    /// Lifecycle status at the moment of the snapshot.
    pub(super) status: NodeStatus,
    /// Wall-clock time at which the runtime accepted the start request.
    pub(super) started_at: Option<SystemTime>,
    /// Container identifier returned by the runtime, when known.
    pub(super) container_id: Option<ContainerId>,
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
            let (status_tx, status_rx) = watch::channel(NodeStatus::Pending);
            let (outputs_tx, outputs_rx) = watch::channel(None);
            nodes.insert(
                node.name.clone(),
                NodeHandle {
                    status_tx: Arc::new(status_tx),
                    status_rx,
                    outputs_tx: Arc::new(outputs_tx),
                    outputs_rx,
                    container_id: Arc::new(Mutex::new(None)),
                    started_at: Arc::new(Mutex::new(None)),
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
            let mut dep_status_rxs: HashMap<String, watch::Receiver<NodeStatus>> = HashMap::new();
            let mut dep_outputs_rxs: HashMap<String, watch::Receiver<Option<ResourceOutputs>>> =
                HashMap::new();
            for dep in &node.depends_on {
                let handle = self
                    .nodes
                    .get(dep)
                    .ok_or_else(|| LifecycleError::UnknownResource(dep.clone()))?;
                dep_status_rxs.insert(dep.clone(), handle.status_rx.clone());
                dep_outputs_rxs.insert(dep.clone(), handle.outputs_rx.clone());
            }

            let node_handle = self.nodes[&node.name].clone();
            let spec = node.spec.clone();
            let own_outputs = node.outputs.clone();
            let name = node.name.clone();
            let runtime = Arc::clone(&self.runtime);
            let event_tx = self.event_tx.clone();

            let task = tokio::spawn(async move {
                start_one(
                    name,
                    spec,
                    own_outputs,
                    runtime,
                    node_handle,
                    dep_status_rxs,
                    dep_outputs_rxs,
                    event_tx,
                )
                .await
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

    /// Shared reference to the underlying execution plan.
    pub(super) fn plan_arc(&self) -> &Arc<LifecyclePlan> {
        &self.plan
    }

    /// Shared reference to the underlying container runtime.
    pub(super) fn runtime_arc(&self) -> &Arc<R> {
        &self.runtime
    }

    /// Point-in-time snapshot of one resource, or `None` when the name
    /// is not part of the plan.
    pub(super) fn snapshot(&self, name: &str) -> Option<NodeSnapshot> {
        let handle = self.nodes.get(name)?;
        let status = handle.status_rx.borrow().clone();
        let started_at = *handle.started_at.lock().expect("started_at mutex poisoned");
        let container_id = handle
            .container_id
            .lock()
            .expect("container_id mutex poisoned")
            .clone();
        Some(NodeSnapshot {
            status,
            started_at,
            container_id,
        })
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn start_one<R: ContainerRuntime + 'static>(
    name: String,
    spec: ContainerSpec,
    own_outputs: ResourceOutputs,
    runtime: Arc<R>,
    handle: NodeHandle,
    dep_status_rxs: HashMap<String, watch::Receiver<NodeStatus>>,
    mut dep_outputs_rxs: HashMap<String, watch::Receiver<Option<ResourceOutputs>>>,
    event_tx: mpsc::UnboundedSender<LifecycleEvent>,
) -> Result<(), LifecycleError> {
    // 1. Wait for every dependency to become ready.
    for (dep_name, mut rx) in dep_status_rxs {
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

    // 2. Collect dependency outputs.
    let mut dep_outputs: HashMap<String, ResourceOutputs> = HashMap::new();
    for (dep_name, rx) in &mut dep_outputs_rxs {
        loop {
            if let Some(out) = rx.borrow_and_update().clone() {
                dep_outputs.insert(dep_name.clone(), out);
                break;
            }
            if rx.changed().await.is_err() {
                let reason = format!("dependency `{dep_name}` outputs channel closed");
                let _ = handle.status_tx.send(NodeStatus::Failed {
                    reason: reason.clone(),
                });
                return Err(LifecycleError::DependencyFailed {
                    resource: name,
                    dependency: dep_name.clone(),
                    reason,
                });
            }
        }
    }

    // 3. Resolve interpolations and inject LSH_<DEP>_<PROP> env vars.
    let resolved_spec = match interpolate_and_inject(spec, &dep_outputs) {
        Ok(s) => s,
        Err(reason) => {
            let _ = handle.status_tx.send(NodeStatus::Failed {
                reason: reason.clone(),
            });
            return Err(LifecycleError::Start {
                resource: name,
                source: RuntimeError::InvalidSpec(reason),
            });
        }
    };

    // 4. Start the container.
    let _ = handle.status_tx.send(NodeStatus::Starting);
    let id = match runtime.start(&resolved_spec).await {
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
    {
        let mut guard = handle.started_at.lock().expect("started_at mutex poisoned");
        *guard = Some(SystemTime::now());
    }
    let _ = handle.status_tx.send(NodeStatus::Running);
    let _ = event_tx.send(LifecycleEvent::ResourceStarted {
        name: name.clone(),
        container_id: id.to_string(),
    });

    // 5. Wait for the healthcheck.
    match runtime.wait_healthy(&id, DEFAULT_HEALTHCHECK_TIMEOUT).await {
        Ok(()) => {
            let _ = handle.outputs_tx.send(Some(own_outputs));
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

/// Apply two-pass interpolation to `spec`: resolve every
/// `${resources.<name>.<property>}` against `dep_outputs`, then inject
/// `LSH_<DEP>_<PROPERTY>` automatic environment variables.
///
/// Returns the resolved spec or a human-readable diagnostic when an
/// interpolation references an unknown resource or property.
fn interpolate_and_inject(
    mut spec: ContainerSpec,
    dep_outputs: &HashMap<String, ResourceOutputs>,
) -> std::result::Result<ContainerSpec, String> {
    let mut ctx = InterpolationContext::from_env();
    for (name, outputs) in dep_outputs {
        ctx = ctx.with_resource(name.clone(), outputs.clone());
    }
    let interpolator = Interpolator::new(&ctx);

    // Resolve env values.
    let mut resolved_env = std::collections::HashMap::with_capacity(spec.env.len());
    for (k, v) in spec.env.drain() {
        let resolved = interpolator.resolve(&v).map_err(|e| e.to_string())?;
        resolved_env.insert(k, resolved);
    }

    // Inject LSH_<DEP>_<PROPERTY> variables.
    for (dep_name, outputs) in dep_outputs {
        let dep_upper = dep_name.to_uppercase().replace('-', "_");
        for (prop, value) in outputs {
            let prop_upper = prop.to_uppercase().replace('-', "_");
            let key = format!("LSH_{dep_upper}_{prop_upper}");
            resolved_env.entry(key).or_insert_with(|| value.clone());
        }
    }
    spec.env = resolved_env;

    // Resolve command arguments.
    if let Some(args) = spec.command.as_mut() {
        for arg in args.iter_mut() {
            *arg = interpolator.resolve(arg).map_err(|e| e.to_string())?;
        }
    }

    Ok(spec)
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
