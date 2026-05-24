//! Lifecycle coordination: build an execution plan from a parsed
//! manifest and orchestrate the start, supervise and stop phases on
//! top of any [`crate::ContainerRuntime`] implementation.

pub use crate::lifecycle::error::LifecycleError;
pub use crate::lifecycle::manager::LifecycleManager;
pub use crate::lifecycle::plan::{LifecyclePlan, PlanNode};
pub use crate::lifecycle::status::{LifecycleEvent, NodeStatus};

mod error;
mod manager;
mod plan;
mod status;
