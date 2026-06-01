//! Neutral intermediate representation produced by the lowering step
//! and consumed by every emitter.

use std::path::PathBuf;

use lightshuttle_manifest::ExportConfig;
use lightshuttle_spec::ContainerSpec;

/// Target-agnostic model of a stack ready to be emitted.
#[derive(Debug, Clone)]
pub struct ExportModel {
    /// Project metadata carried from the manifest.
    pub project: ExportProject,
    /// Services in manifest declaration order.
    pub services: Vec<ExportService>,
    /// Raw `export:` section, resolved per target by each emitter.
    pub export: Option<ExportConfig>,
}

/// Project metadata relevant to an export.
#[derive(Debug, Clone)]
pub struct ExportProject {
    /// Project name, used as the default namespace and chart name.
    pub name: String,
    /// Free-form project version, used as the default chart version.
    pub version: Option<String>,
}

/// One service in the export model: a resolved container specification
/// plus the resources it depends on.
#[derive(Debug, Clone)]
pub struct ExportService {
    /// Resolved container specification (image, env, ports, volumes,
    /// healthcheck) as produced by `lightshuttle-spec`.
    pub spec: ContainerSpec,
    /// Names of the resources this service depends on.
    pub depends_on: Vec<String>,
}

/// Supported export targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A `docker-compose.yml` file.
    Compose,
    /// Plain Kubernetes manifests.
    Kubernetes,
    /// A Helm chart.
    Helm,
}

impl Target {
    /// Stable lower-case label, also used as the CLI argument value and
    /// the default output sub-directory.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Compose => "compose",
            Self::Kubernetes => "kubernetes",
            Self::Helm => "helm",
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A set of named files produced by an emitter, written to disk by the
/// CLI.
#[derive(Debug, Clone, Default)]
pub struct ExportArtifacts {
    /// Files in deterministic emission order.
    pub files: Vec<ExportFile>,
}

impl ExportArtifacts {
    /// Build an empty artifact set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a file at `path` with `contents`.
    pub fn push(&mut self, path: impl Into<PathBuf>, contents: impl Into<String>) {
        self.files.push(ExportFile {
            path: path.into(),
            contents: contents.into(),
        });
    }
}

/// A single emitted file: a relative path and its textual contents.
#[derive(Debug, Clone)]
pub struct ExportFile {
    /// Path relative to the export output directory.
    pub path: PathBuf,
    /// Full textual contents of the file.
    pub contents: String,
}
