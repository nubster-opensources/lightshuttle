//! Lowering: turn a parsed manifest into the neutral [`ExportModel`].

use lightshuttle_manifest::Manifest;
use lightshuttle_spec::from_resource;

use crate::error::{ExportError, Result};
use crate::model::{ExportModel, ExportProject, ExportService};

/// Lower `manifest` into an [`ExportModel`].
///
/// Each resource is resolved through `lightshuttle-spec`, so the model
/// inherits the same image, port, environment and healthcheck defaults
/// the runtime applies, with no drift between `up` and `export`. The
/// raw `export:` section is carried through for per-target resolution.
///
/// # Errors
///
/// Returns [`ExportError::Spec`] if a resource cannot be resolved into a
/// container specification.
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
