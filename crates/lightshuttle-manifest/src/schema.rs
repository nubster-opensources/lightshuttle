//! JSON Schema generation for the manifest model.

use schemars::schema::RootSchema;
use schemars::schema_for;

use crate::model::Manifest;

/// Build the JSON Schema describing a valid `lightshuttle.yml`.
///
/// The schema is generated from the Rust types annotated with
/// `#[derive(JsonSchema)]`. It targets JSON Schema draft 7 and is
/// consumed by:
///
/// - Editors that recognise the
///   `# yaml-language-server: $schema=...` header
///   (VS Code, IntelliJ, neovim with yaml-language-server) to provide
///   inline validation and autocompletion.
/// - The `cargo xtask schema` subcommand which dumps it to
///   `docs/spec/manifest-v0.schema.json`.
/// - Test suites that check fixtures against the canonical schema.
#[must_use]
pub fn schema() -> RootSchema {
    schema_for!(Manifest)
}
