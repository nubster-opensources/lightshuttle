//! Top-level parse and serialise entry points on [`Manifest`].

use crate::error::{ManifestError, Result};
use crate::model::Manifest;

impl Manifest {
    /// Parse a manifest from a YAML string.
    ///
    /// Runs structural decoding through `serde_yml` followed by semantic
    /// validation (naming rules, dependency graph, references).
    ///
    /// Returns the parsed manifest or the first error encountered.
    pub fn parse(yaml: &str) -> Result<Self> {
        let manifest: Self = serde_yml::from_str(yaml).map_err(ManifestError::from)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Re-emit the manifest as YAML.
    ///
    /// Optional fields whose value is `None` and empty collections are
    /// omitted. The output is lossless for the v0 model: a round-trip
    /// through `parse` then `to_yaml` then `parse` yields an equal
    /// [`Manifest`].
    pub fn to_yaml(&self) -> Result<String> {
        serde_yml::to_string(self).map_err(ManifestError::from)
    }
}
