//! Test helpers for downstream crates and integration tests.
//!
//! Provides an in-memory [`MockRuntime`] that satisfies
//! [`crate::ContainerRuntime`] without any Docker daemon. Containers
//! become healthy a short, deterministic delay after start, unless the
//! caller flags a specific resource as a failure target.
//!
//! `MockRuntime` is intentionally cheap to clone: every internal field
//! is an [`Arc<Mutex<_>>`], so a test can hold an observer clone for
//! introspection after the manager has consumed the original instance.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::stream::{Stream, StreamExt};

use crate::error::RuntimeError;
use crate::runtime::{ContainerId, ContainerRuntime, ContainerStatus, LogChunk, LogChunkStream};
use lightshuttle_spec::ContainerSpec;

/// In-memory [`ContainerRuntime`] for tests.
///
/// Every container becomes [`ContainerStatus::Healthy`] 30 ms after
/// `start`, unless its name is configured as a failure target via
/// [`MockRuntime::fail_on`].
#[derive(Clone)]
pub struct MockRuntime {
    state: Arc<Mutex<HashMap<String, MockContainer>>>,
    fail_on: Arc<Mutex<Option<String>>>,
    start_order: Arc<Mutex<Vec<String>>>,
    stop_order: Arc<Mutex<Vec<String>>>,
    started_specs: Arc<Mutex<Vec<ContainerSpec>>>,
}

struct MockContainer {
    name: String,
    status: ContainerStatus,
    started_at: Instant,
    healthy_after: Duration,
}

impl MockRuntime {
    /// Build a fresh runtime with empty state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            fail_on: Arc::new(Mutex::new(None)),
            start_order: Arc::new(Mutex::new(Vec::new())),
            stop_order: Arc::new(Mutex::new(Vec::new())),
            started_specs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configure the runtime to reject `start` for the resource whose
    /// `ContainerSpec.name` equals `name`.
    pub fn fail_on(&self, name: &str) {
        *self.fail_on.lock().expect("fail_on mutex poisoned") = Some(name.to_owned());
    }

    /// Snapshot of the resource names in start order.
    #[must_use]
    pub fn started_resources(&self) -> Vec<String> {
        self.start_order
            .lock()
            .expect("start_order mutex poisoned")
            .clone()
    }

    /// Snapshot of the resource names in stop order.
    #[must_use]
    pub fn stopped_resources(&self) -> Vec<String> {
        self.stop_order
            .lock()
            .expect("stop_order mutex poisoned")
            .clone()
    }

    /// Snapshot of every container spec the runtime has accepted.
    #[must_use]
    pub fn started_specs(&self) -> Vec<ContainerSpec> {
        self.started_specs
            .lock()
            .expect("started_specs mutex poisoned")
            .clone()
    }
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRuntime for MockRuntime {
    async fn start(&self, spec: &ContainerSpec) -> Result<ContainerId, RuntimeError> {
        if self
            .fail_on
            .lock()
            .expect("fail_on mutex poisoned")
            .as_deref()
            == Some(spec.name.as_str())
        {
            return Err(RuntimeError::InvalidSpec(format!(
                "mock failure for `{}`",
                spec.name
            )));
        }
        let id = ContainerId::new(format!("mock-{}", spec.name));
        if self
            .state
            .lock()
            .expect("state mutex poisoned")
            .contains_key(id.as_str())
        {
            return Err(RuntimeError::InvalidSpec(format!(
                "container name `{}` already in use",
                spec.name
            )));
        }
        self.start_order
            .lock()
            .expect("start_order mutex poisoned")
            .push(spec.name.clone());
        self.started_specs
            .lock()
            .expect("started_specs mutex poisoned")
            .push(spec.clone());
        self.state.lock().expect("state mutex poisoned").insert(
            id.as_str().to_owned(),
            MockContainer {
                name: spec.name.clone(),
                status: ContainerStatus::Starting,
                started_at: Instant::now(),
                healthy_after: Duration::from_millis(30),
            },
        );
        Ok(id)
    }

    async fn stop(&self, id: &ContainerId, _grace: Duration) -> Result<(), RuntimeError> {
        let mut state = self.state.lock().expect("state mutex poisoned");
        if let Some(c) = state.get_mut(id.as_str()) {
            c.status = ContainerStatus::Stopped { exit_code: Some(0) };
            self.stop_order
                .lock()
                .expect("stop_order mutex poisoned")
                .push(c.name.clone());
        }
        Ok(())
    }

    async fn remove(&self, name: &str) -> Result<(), RuntimeError> {
        self.state
            .lock()
            .expect("state mutex poisoned")
            .remove(&format!("mock-{name}"));
        Ok(())
    }

    async fn inspect(&self, id: &ContainerId) -> Result<ContainerStatus, RuntimeError> {
        let state = self.state.lock().expect("state mutex poisoned");
        let c = state
            .get(id.as_str())
            .ok_or_else(|| RuntimeError::NotFound(id.as_str().to_owned()))?;
        Ok(c.status.clone())
    }

    async fn wait_healthy(&self, id: &ContainerId, timeout: Duration) -> Result<(), RuntimeError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            {
                let mut state = self.state.lock().expect("state mutex poisoned");
                if let Some(c) = state.get_mut(id.as_str())
                    && c.started_at.elapsed() >= c.healthy_after
                {
                    c.status = ContainerStatus::Healthy;
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(RuntimeError::Timeout {
            operation: "wait_healthy",
            after: timeout,
        })
    }

    async fn logs(&self, _id: &ContainerId, _follow: bool) -> Result<LogChunkStream, RuntimeError> {
        let empty: Pin<Box<dyn Stream<Item = Result<LogChunk, RuntimeError>> + Send>> =
            Box::pin(futures::stream::empty::<Result<LogChunk, RuntimeError>>().map(|x| x));
        Ok(empty)
    }
}
