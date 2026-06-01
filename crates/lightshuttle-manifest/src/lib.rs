//! Manifest types, parser, interpolation and JSON Schema generation for
//! LightShuttle.
//!
//! The crate is a structural layer above `serde_yml`. It exposes a
//! strongly-typed model of the `lightshuttle.yml` manifest, an
//! interpolation engine that resolves `${...}` expressions against an
//! environment plus the runtime properties of dependent resources, and a
//! validation pass that catches naming, dependency and reference issues
//! before any runtime work is attempted.
//!
//! See [`docs/spec/manifest-v0.md`][spec] in the main repository for the
//! specification this crate implements.
//!
//! [spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md

pub use crate::error::{ManifestError, Result};
pub use crate::interpolate::{InterpolationContext, Interpolator, Reference};
pub use crate::model::{
    Command, ComposeExport, ComposeResourceExport, ContainerConfig, DashboardConfig,
    DockerfileConfig, ExportConfig, Healthcheck, HelmExport, HelmResourceExport, ImagePullPolicy,
    KubernetesExport, KubernetesResourceExport, Manifest, ObservabilityConfig, OtelConfig,
    PortMapping, PostgresConfig, Project, RedisConfig, ResourceKind, Version, Volume,
};
pub use crate::schema::schema;

mod error;
pub mod interpolate;
pub mod model;
mod parse;
mod schema;
mod validate;
