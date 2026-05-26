//! Lifecycle coordination: build an execution plan from a parsed
//! manifest and orchestrate the start, supervise and stop phases on
//! top of any [`crate::ContainerRuntime`] implementation.

pub use crate::lifecycle::error::LifecycleError;
pub use crate::lifecycle::handle::{LifecycleHandle, LifecycleHandleError, ManagerHandle};
pub use crate::lifecycle::manager::LifecycleManager;
pub use crate::lifecycle::plan::{LifecyclePlan, PlanNode};
pub use crate::lifecycle::status::{LifecycleEvent, NodeStatus};
pub use crate::lifecycle::view::{ResourceStatus, ResourceView};

mod error;
mod handle;
mod manager;
mod plan;
mod status;
mod view;
