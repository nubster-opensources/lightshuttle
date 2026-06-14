//! Lowering: turn a parsed manifest into the neutral [`ExportModel`] (IR).
//!
//! This is the first stage of the export pipeline. The IR produced here is
//! consumed by every emitter without further resolution of manifest details.

use lightshuttle_manifest::Manifest;
use lightshuttle_spec::from_resource;

use crate::error::{ExportError, Result};
use crate::model::{ExportModel, ExportProject, ExportService};

/// Lowers a [`lightshuttle_manifest::Manifest`] into an [`ExportModel`].
///
/// Each manifest resource is resolved through `lightshuttle-spec`, so the
/// resulting model inherits the same image, port, environment, and healthcheck
/// defaults that the runtime applies. This keeps `lightshuttle up` and
/// `lightshuttle export` in sync with no manual duplication.
///
/// The raw `export:` section is carried through unchanged; emitters read it
/// via the [`crate::resolve`] helpers to apply per-target overrides.
///
/// # Errors
///
/// Returns [`ExportError::Spec`] when a resource cannot be resolved into a
/// container specification by `lightshuttle-spec`.
pub fn lower(manifest: &Manifest) -> Result<ExportModel> {
    let project = ExportProject {
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
    };

    let mut services = Vec::with_capacity(manifest.resources.len());
    for (name, kind) in &manifest.resources {
        let resolved = from_resource(&manifest.project.name, name, kind).map_err(|source| {
            ExportError::Spec {
                resource: name.clone(),
                source,
            }
        })?;
        services.push(ExportService {
            spec: resolved.spec,
            depends_on: kind.depends_on().to_vec(),
        });
    }

    Ok(ExportModel {
        project,
        services,
        export: manifest.export.clone(),
    })
}
