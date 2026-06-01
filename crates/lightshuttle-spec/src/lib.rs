//! Manifest to container specification resolution for LightShuttle.
//!
//! This crate is the single source of truth that turns a typed
//! `lightshuttle-manifest` resource declaration into a self-contained
//! [`ContainerSpec`], applying the v0 defaults (image expansion,
//! database name derivation, random password generation, healthcheck
//! materialisation) and computing the [`ResourceOutputs`] a resource
//! exposes to its dependents.
//!
//! The resolution is intentionally free of any container daemon
//! dependency, so both the runtime and the export pipeline can consume
//! it without drift.

mod error;
mod spec;

pub use crate::error::{Result, SpecError};
pub use crate::spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, ResolvedResource, ResourceOutputs,
    VolumeBinding, VolumeSource, from_resource,
};
