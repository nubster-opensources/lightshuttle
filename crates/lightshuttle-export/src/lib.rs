//! Manifest to deployment artifact transpilation for LightShuttle.
//!
//! The export pipeline follows a compiler shape: [`lower`] turns a
//! parsed `lightshuttle-manifest` into a neutral [`ExportModel`] by
//! resolving every resource through `lightshuttle-spec`, then a target
//! [`Emitter`] renders that model into [`ExportArtifacts`]. Per-target
//! defaults and overrides are resolved by the pure helpers in
//! [`resolve`], shared by every emitter.
//!
//! This crate carries no container daemon dependency: it only reads the
//! manifest and the resolved specification, so it transpiles the same
//! way on a developer machine or in CI without Docker.

mod emit;
mod emitters;
mod error;
mod lower;
mod model;
pub mod resolve;

pub use crate::emit::Emitter;
pub use crate::emitters::ComposeEmitter;
pub use crate::error::{ExportError, Result};
pub use crate::lower::lower;
pub use crate::model::{
    ExportArtifacts, ExportFile, ExportModel, ExportProject, ExportService, Target,
};
