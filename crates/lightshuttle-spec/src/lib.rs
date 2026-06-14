//! Manifest-to-container specification resolution for LightShuttle.
//!
//! # Position in the layer graph
//!
//! `lightshuttle-spec` sits between the manifest parsing layer and the
//! execution/export layer:
//!
//! ```text
//! lightshuttle-manifest  (parsing, typed resource declarations)
//!         |
//!         v
//! lightshuttle-spec      (THIS CRATE - resolution, defaults, outputs)
//!         |
//!   +-----+------+
//!   v            v
//! runtime      export
//! ```
//!
//! It depends on `lightshuttle-manifest` and is consumed by
//! `lightshuttle-runtime` and `lightshuttle-export`. It does not depend
//! on any container daemon, so both consumers share a single, consistent
//! resolution without drift.
//!
//! # What resolution does
//!
//! The entry point is [`from_resource`]. Given a project name, a
//! resource name, and a [`lightshuttle_manifest::ResourceKind`], it
//! returns a [`ResolvedResource`] that bundles:
//!
//! - A [`ContainerSpec`]: the complete, self-contained description of
//!   the container to launch (image, env vars, ports, volumes,
//!   healthcheck).
//! - A [`ResourceOutputs`] map: the key/value pairs exposed to
//!   dependent resources via `${resources.<name>.<key>}` interpolation
//!   and `LSH_*` environment variables.
//!
//! # Resolved outputs by resource kind
//!
//! | Kind | Keys |
//! |---|---|
//! | `postgres` | `host`, `port`, `database`, `user`, `password`, `url` |
//! | `redis` | `host`, `port`, `password`, `url` |
//! | `container` / `dockerfile` | `host`, `ports` (comma-separated) |
//!
//! # Errors
//!
//! Resolution fails with a [`SpecError`] when a port mapping, volume
//! string, or healthcheck duration in the manifest is structurally
//! invalid.

#![deny(missing_docs)]

mod error;
mod spec;

pub use crate::error::{Result, SpecError};
pub use crate::spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, ResolvedResource, ResourceOutputs,
    VolumeBinding, VolumeSource, from_resource,
};
