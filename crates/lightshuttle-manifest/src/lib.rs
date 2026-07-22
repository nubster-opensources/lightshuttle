//! Manifest types, parser, interpolation, and JSON Schema generation for
//! LightShuttle.
//!
//! This crate is the **leaf layer** of the LightShuttle workspace: it carries
//! zero internal dependencies and is consumed by every other crate in the
//! graph (`lightshuttle-spec`, `lightshuttle-runtime`, `lightshuttle-export`,
//! and the facade `lightshuttle`). Any type that appears in a
//! `lightshuttle.yml` manifest is defined here.
//!
//! # What this crate provides
//!
//! - A strongly-typed model of `lightshuttle.yml` rooted at [`Manifest`].
//!   The model is `serde`-annotated for both serialisation and JSON Schema
//!   generation via `schemars`.
//! - [`Manifest::parse`]: one-shot YAML parsing combined with semantic
//!   validation (naming rules, dependency cycle detection, reference
//!   checking). Returns a [`ManifestError`] on the first problem found.
//! - [`Manifest::to_yaml`]: lossless round-trip serialisation back to YAML.
//! - [`Manifest::validate`]: the semantic validation pass in isolation,
//!   callable after building a manifest programmatically.
//! - [`Manifest::resolve_host_volume_paths`]: rewrites relative `src` paths
//!   in volume mappings to absolute paths anchored on the manifest directory.
//! - [`Interpolator`] + [`InterpolationContext`]: resolution of `${...}`
//!   expressions in string fields, supporting `${env.NAME}` and
//!   `${resources.name.property}` references.
//! - [`schema`]: generates the JSON Schema consumed by editor tooling and
//!   the `cargo xtask schema` subcommand.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use lightshuttle_manifest::Manifest;
//!
//! let yaml = r#"
//! project:
//!   name: my-app
//! resources:
//!   db:
//!     postgres:
//!       version: "16"
//! "#;
//!
//! let manifest = Manifest::parse(yaml).expect("valid manifest");
//! assert_eq!(manifest.project.name, "my-app");
//! ```
//!
//! See the [`Manifest`], [`ResourceKind`], and [`interpolate`] module for
//! detailed documentation.
//!
//! # Specification
//!
//! The YAML format this crate implements is described in
//! `docs/spec/manifest-v0.md` in the main repository.
#![deny(missing_docs)]

pub use crate::canonical::{
    DnsName, DnsNameError, DurationError, ImageReference, ImageReferenceError,
};
pub use crate::error::{ManifestError, Result};
pub use crate::interpolate::{InterpolationContext, Interpolator, Reference};
pub use crate::model::{
    Command, ComposeExport, ComposeResourceExport, ContainerConfig, DashboardConfig,
    DockerfileConfig, ExportConfig, Healthcheck, HelmExport, HelmResourceExport, ImagePullPolicy,
    KubernetesExport, KubernetesResourceExport, Manifest, ObservabilityConfig, OtelConfig,
    PortMapping, PostgresConfig, Project, RedisConfig, ResourceKind, Version, Volume,
};
pub use crate::schema::schema;

/// Canonical parsers for the normalised grammars shared across the workspace.
pub mod canonical;
mod error;
mod host_paths;
/// Substitution engine for `${...}` interpolations in manifest string values.
pub mod interpolate;
/// Strongly-typed model of a `lightshuttle.yml` manifest.
pub mod model;
mod parse;
mod schema;
mod validate;
