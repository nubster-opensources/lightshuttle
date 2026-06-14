#![deny(missing_docs)]
//! Manifest to deployment artifact transpilation for LightShuttle.
//!
//! # Position in the crate graph
//!
//! `lightshuttle-export` sits above `lightshuttle-manifest` (the parsed
//! YAML representation) and `lightshuttle-spec` (the canonical resource
//! specification). It depends on both; the CLI crate depends on this one
//! to write the produced files to disk.
//!
//! # Pipeline: manifest -> IR -> artifacts
//!
//! The export pipeline follows a compiler shape:
//!
//! 1. **Lowering** ([`lower`]): a [`lightshuttle_manifest::Manifest`] is
//!    lowered into a target-agnostic [`ExportModel`] (the IR). Every
//!    resource is resolved through `lightshuttle-spec` so the model
//!    inherits the same image, port, environment, and healthcheck defaults
//!    that the runtime applies - no drift between `lightshuttle up` and
//!    `lightshuttle export`.
//! 2. **Emission** ([`Emitter`]): a target-specific emitter consumes the
//!    [`ExportModel`] and produces [`ExportArtifacts`], a list of named
//!    files whose textual contents are ready to write. Three emitters ship
//!    out of the box: [`ComposeEmitter`], [`KubernetesEmitter`], and
//!    [`HelmEmitter`].
//! 3. **Resolution** ([`resolve`]): pure helpers that turn the optional
//!    `export:` manifest section into concrete per-target values (namespace,
//!    replica count, image pull policy, chart name). All emitters share
//!    this module so defaults are defined and tested in one place.
//!
//! # No daemon dependency
//!
//! This crate carries no container daemon dependency. It only reads the
//! manifest and the resolved specification, so it transpiles identically
//! on a developer machine or in CI without Docker.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use lightshuttle_export::{lower, ComposeEmitter, Emitter};
//! use lightshuttle_manifest::Manifest;
//!
//! # fn main() -> lightshuttle_export::Result<()> {
//! // Load a manifest from disk (I/O, so no_run).
//! let manifest: Manifest = todo!("parse from YAML");
//!
//! // Lower the manifest into the neutral IR.
//! let model = lower(&manifest)?;
//!
//! // Emit Docker Compose artifacts.
//! let emitter = ComposeEmitter;
//! let artifacts = emitter.emit(&model)?;
//!
//! for file in &artifacts.files {
//!     println!("{}: {} bytes", file.path.display(), file.contents.len());
//! }
//! # Ok(())
//! # }
//! ```

mod emit;
mod emitters;
mod error;
mod lower;
mod model;
pub mod resolve;

pub use crate::emit::Emitter;
pub use crate::emitters::{ComposeEmitter, HelmEmitter, KubernetesEmitter};
pub use crate::error::{ExportError, Result};
pub use crate::lower::lower;
pub use crate::model::{
    ExportArtifacts, ExportFile, ExportModel, ExportProject, ExportService, Target,
};
