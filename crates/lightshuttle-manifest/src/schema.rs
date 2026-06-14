//! JSON Schema generation for the manifest model.
//!
//! Exposes a single public function [`schema`] that derives a JSON Schema
//! from the [`Manifest`] type tree via `schemars`. All model types carry
//! `#[derive(JsonSchema)]` for this purpose.

use schemars::{Schema, schema_for};

use crate::model::Manifest;

/// Build the JSON Schema describing a valid `lightshuttle.yml`.
///
/// The schema is generated from the Rust types annotated with
/// `#[derive(JsonSchema)]`. It is consumed by:
///
/// - Editors that recognise the
///   `# yaml-language-server: $schema=...` header
///   (VS Code, IntelliJ, neovim with yaml-language-server) to provide
///   inline validation and autocompletion.
/// - The `cargo xtask schema` subcommand which dumps it to
///   `docs/spec/manifest-v0.schema.json`.
/// - Test suites that check fixtures against the canonical schema.
#[must_use]
pub fn schema() -> Schema {
    schema_for!(Manifest)
}
