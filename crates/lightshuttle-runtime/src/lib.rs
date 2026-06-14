#![deny(missing_docs)]
//! Container runtime backends and lifecycle manager for LightShuttle.
//!
//! # Crate placement in the stack
//!
//! ```text
//! lightshuttle-spec      (domain types, ContainerSpec)
//! lightshuttle-manifest  (YAML parsing, interpolation)
//!         |
//! lightshuttle-runtime   <-- this crate
//!         |
//! lightshuttle-control   (REST/HTTP control plane)
//! lightshuttle-otel      (OpenTelemetry instrumentation)
//! ```
//!
//! This crate depends on `lightshuttle-spec` (for [`ContainerSpec`] and
//! related domain types) and `lightshuttle-manifest` (for parsed manifests
//! fed into [`LifecyclePlan::from_manifest`]). It is consumed by
//! `lightshuttle-control` (the control plane) and `lightshuttle-otel`.
//!
//! # Core abstractions
//!
//! ## [`ContainerRuntime`] trait
//!
//! The narrow abstraction that hides every daemon-specific detail.
//! The lifecycle manager calls only the methods declared by this trait.
//! [`DockerRuntime`] is the first concrete implementation, backed by the
//! `bollard` crate. Tests and downstream crates use [`testkit::MockRuntime`]
//! as a drop-in replacement that requires no Docker daemon.
//!
//! ## [`LifecyclePlan`]
//!
//! Computed from a parsed manifest by [`LifecyclePlan::from_manifest`].
//! Performs a topological sort (Kahn's algorithm) over the declared
//! `depends_on` graph so the manager can start independent branches in
//! parallel and block each resource until its dependencies are ready.
//!
//! ## [`LifecycleManager`]
//!
//! Orchestrates the full `up` and `down` lifecycle:
//!
//! 1. Starts every resource in topological order, independent branches in
//!    parallel, via `tokio::spawn`.
//! 2. Waits for each container to pass its healthcheck (or to reach
//!    [`ContainerStatus::Running`] when no healthcheck is declared).
//! 3. Publishes [`LifecycleEvent`] on a broadcast channel so the CLI,
//!    dashboard, and REST layer can observe progress.
//! 4. On `SIGINT` or `SIGTERM` (see [`LifecycleManager::run_until_signal`]),
//!    stops all resources in reverse topological order, sends `SIGTERM` and
//!    then `SIGKILL` after the configured grace window, and tears down the
//!    per-project bridge network.
//!
//! # Quick start (no Docker daemon)
//!
//! ```rust,no_run
//! use std::collections::HashMap;
//! use std::time::Duration;
//!
//! use lightshuttle_manifest::Manifest;
//! use lightshuttle_runtime::{LifecyclePlan, LifecycleManager, DockerRuntime};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let yaml = r#"
//! project:
//!   name: myapp
//! resources:
//!   db:
//!     postgres:
//!       version: "16"
//! "#;
//!
//! let manifest = Manifest::parse(yaml)?;
//! let plan = LifecyclePlan::from_manifest(&manifest)?;
//! let runtime = DockerRuntime::connect()?;
//! let (manager, _events) = LifecycleManager::new(plan, runtime);
//!
//! // Blocks until SIGINT/SIGTERM, then tears the stack down cleanly.
//! manager.run_until_signal(Duration::from_secs(30)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! See `docs/spec/manifest-v0.md` in the main repository for the full
//! manifest specification.

pub use crate::docker::{DockerRuntime, LABEL_PROJECT, LABEL_RESOURCE, ManagedContainer};
pub use crate::error::{Result, RuntimeError};
pub use crate::lifecycle::{
    EnvReport, EnvSource, EnvVarReport, EnvVarStatus, LifecycleError, LifecycleEvent,
    LifecycleHandle, LifecycleHandleError, LifecycleManager, LifecyclePlan, ManagerHandle,
    NodeStatus, PlanNode, ResourceStatus, ResourceView,
};
pub use crate::runtime::{
    ContainerId, ContainerRuntime, ContainerStatus, LogChunk, LogChunkStream, LogStream,
};
pub use lightshuttle_spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, ResolvedResource, ResourceOutputs,
    SpecError, VolumeBinding, VolumeSource, from_resource,
};

mod docker;
mod error;
mod lifecycle;
mod runtime;

/// In-memory [`ContainerRuntime`] and supporting helpers for tests.
///
/// See [`testkit::MockRuntime`] for the main type.
pub mod testkit;
