//! The emitter contract: turn an [`ExportModel`] into target files.

use crate::error::Result;
use crate::model::{ExportArtifacts, ExportModel, Target};

/// Transpiles an [`ExportModel`] into the files of a single deployment target.
///
/// Implementations live in their own modules: [`crate::ComposeEmitter`],
/// [`crate::KubernetesEmitter`], and [`crate::HelmEmitter`].
///
/// # Determinism requirement
///
/// Emitters must produce deterministic output. Container environment maps are
/// unordered, so any map-derived output must be sorted by key before it is
/// written - otherwise golden-file tests become flaky.
///
/// # Example
///
/// ```rust
/// use lightshuttle_export::{Emitter, ComposeEmitter, Target};
///
/// let emitter = ComposeEmitter;
/// assert_eq!(emitter.target(), Target::Compose);
/// ```
pub trait Emitter {
    /// Returns the [`Target`] that this emitter produces.
    fn target(&self) -> Target;

    /// Transpiles `model` into a set of named files for the emitter's target.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ExportError::Unsupported`] when a resource in `model`
    /// cannot be represented for this target (for example, a locally built
    /// image referenced by a Kubernetes manifest).
    fn emit(&self, model: &ExportModel) -> Result<ExportArtifacts>;
}
