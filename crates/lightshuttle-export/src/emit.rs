//! The emitter contract: turn an [`ExportModel`] into target files.

use crate::error::Result;
use crate::model::{ExportArtifacts, ExportModel, Target};

/// Transpiles an [`ExportModel`] into the files of a single target.
///
/// Implementations live in their own modules (compose, kubernetes,
/// helm). Emitters must produce deterministic output: container
/// environment maps are unordered, so any map-derived output has to be
/// sorted by key before it is written, otherwise golden-file tests turn
/// flaky.
pub trait Emitter {
    /// The target this emitter produces.
    fn target(&self) -> Target;

    /// Emit the target files for `model`.
    fn emit(&self, model: &ExportModel) -> Result<ExportArtifacts>;
}
