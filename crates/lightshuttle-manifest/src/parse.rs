//! Top-level parse and serialise entry points on [`Manifest`].
//!
//! This module contains the `impl Manifest` blocks for [`Manifest::parse`]
//! and [`Manifest::to_yaml`], kept separate from the struct definition so
//! that the model module stays free of `serde_norway` coupling.

use crate::error::{ManifestError, Result};
use crate::model::Manifest;

impl Manifest {
    /// Parse a `lightshuttle.yml` manifest from a YAML string.
    ///
    /// Two phases run in sequence:
    ///
    /// 1. Structural decoding via `serde_norway`. Unknown fields cause an
    ///    error (`deny_unknown_fields` is set on every model type).
    /// 2. Semantic validation via [`Manifest::validate`]: naming rules,
    ///    dependency cycle detection, and reference resolution.
    ///
    /// Returns the first [`ManifestError`] encountered, or the fully
    /// validated manifest.
    ///
    /// ```rust,no_run
    /// use lightshuttle_manifest::Manifest;
    ///
    /// let yaml = r#"
    /// project:
    ///   name: demo
    /// resources:
    ///   db:
    ///     postgres:
    ///       version: "16"
    /// "#;
    ///
    /// let manifest = Manifest::parse(yaml).expect("valid manifest");
    /// assert_eq!(manifest.project.name, "demo");
    /// ```
    pub fn parse(yaml: &str) -> Result<Self> {
        let manifest: Self = serde_norway::from_str(yaml).map_err(ManifestError::from)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serialise the manifest back to a YAML string.
    ///
    /// Optional fields with a `None` value and empty collections are omitted
    /// from the output. The serialisation is lossless for the v0 model: a
    /// round-trip through `parse` then `to_yaml` then `parse` produces an
    /// equal [`Manifest`].
    ///
    /// ```rust,no_run
    /// use lightshuttle_manifest::Manifest;
    ///
    /// let yaml = "project:\n  name: demo\nresources: {}\n";
    /// let manifest = Manifest::parse(yaml).unwrap();
    /// let output = manifest.to_yaml().unwrap();
    /// assert!(output.contains("name: demo"));
    /// ```
    pub fn to_yaml(&self) -> Result<String> {
        serde_norway::to_string(self).map_err(ManifestError::from)
    }
}
