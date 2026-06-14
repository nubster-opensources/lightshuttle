//! Lifecycle coordination: plan building, orchestration, events, and views.
//!
//! This module re-exports the public surface of several submodules:
//!
//! - [`LifecyclePlan`] / [`PlanNode`]: topologically sorted execution plan
//!   derived from a parsed manifest.
//! - [`LifecycleManager`]: orchestrates startup, supervision, and shutdown
//!   on top of any [`crate::ContainerRuntime`].
//! - [`LifecycleHandle`] / [`ManagerHandle`]: backend-agnostic handle used by
//!   the control plane to query and control a running stack.
//! - [`LifecycleEvent`] / [`NodeStatus`]: status types broadcast to subscribers.
//! - [`ResourceView`] / [`ResourceStatus`]: coarse-grained UI-friendly view.
//! - [`EnvReport`] / [`EnvVarReport`] / [`EnvSource`] / [`EnvVarStatus`]:
//!   environment variable resolution report used by the preflight check and
//!   the `lightshuttle secrets check` subcommand.
//! - [`LifecycleError`] / [`LifecycleHandleError`]: error types for the
//!   lifecycle layer.

pub use crate::lifecycle::env_report::{EnvReport, EnvSource, EnvVarReport, EnvVarStatus};
pub use crate::lifecycle::error::LifecycleError;
pub use crate::lifecycle::handle::{LifecycleHandle, LifecycleHandleError, ManagerHandle};
pub use crate::lifecycle::manager::LifecycleManager;
pub use crate::lifecycle::plan::{LifecyclePlan, PlanNode};
pub use crate::lifecycle::status::{LifecycleEvent, NodeStatus};
pub use crate::lifecycle::view::{ResourceStatus, ResourceView};

mod env_report;
mod error;
mod handle;
mod manager;
mod plan;
mod status;
mod view;
