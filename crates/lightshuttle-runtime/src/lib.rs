//! Container runtime backends and lifecycle manager for LightShuttle.
//!
//! This crate provides:
//!
//! - A narrow [`ContainerRuntime`] trait that the lifecycle manager
//!   targets and that hides the daemon-specific surface.
//! - A first concrete implementation [`DockerRuntime`] backed by the
//!   `bollard` crate.
//! - A [`ContainerSpec`] value built from a `lightshuttle-manifest`
//!   resource declaration, with the v0 defaults already applied
//!   (image expansion, database name derivation, random password
//!   generation, healthcheck materialisation).
//! - A [`LifecycleManager`] that consumes a [`LifecyclePlan`] and a
//!   runtime, then orchestrates startup (topological, parallel where
//!   possible, gated on healthchecks), supervises the running stack
//!   and rolls everything down on signal.
//!
//! See `docs/spec/manifest-v0.md` in the main repository for the
//! manifest specification this runtime consumes.

pub use crate::docker::{DockerRuntime, LABEL_PROJECT, LABEL_RESOURCE, ManagedContainer};
pub use crate::error::{Result, RuntimeError};
pub use crate::lifecycle::{
    LifecycleError, LifecycleEvent, LifecycleHandle, LifecycleHandleError, LifecycleManager,
    LifecyclePlan, ManagerHandle, NodeStatus, PlanNode, ResourceStatus, ResourceView,
};
pub use crate::runtime::{
    ContainerId, ContainerRuntime, ContainerStatus, LogChunk, LogChunkStream, LogStream,
};
pub use crate::spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, ResolvedResource, ResourceOutputs,
    VolumeBinding, VolumeSource, from_resource,
};

mod docker;
mod error;
mod lifecycle;
mod runtime;
mod spec;

pub mod testkit;
