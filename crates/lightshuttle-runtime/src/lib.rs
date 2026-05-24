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
//!
//! See `docs/spec/manifest-v0.md` in the main repository for the
//! manifest specification this runtime consumes.

pub use crate::docker::DockerRuntime;
pub use crate::error::{Result, RuntimeError};
pub use crate::runtime::{
    ContainerId, ContainerRuntime, ContainerStatus, LogChunk, LogChunkStream, LogStream,
};
pub use crate::spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, VolumeBinding, VolumeSource,
    from_resource,
};

mod docker;
mod error;
mod runtime;
mod spec;
