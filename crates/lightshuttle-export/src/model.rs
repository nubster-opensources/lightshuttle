//! Neutral intermediate representation (IR) produced by the lowering step
//! and consumed by every emitter.
//!
//! The IR sits between the parsed manifest and the target-specific emission.
//! [`ExportModel`] is the root: it holds [`ExportProject`] metadata and a
//! flat list of [`ExportService`] entries already resolved through
//! `lightshuttle-spec`. Each emitter reads the model read-only and writes its
//! output into [`ExportArtifacts`], a list of [`ExportFile`] values that the
//! CLI layer writes to disk.

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
///
/// Each variant maps to one [`crate::Emitter`] implementation and one
/// CLI argument value (see [`Target::label`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A `docker-compose.yml` file, emitted by [`crate::ComposeEmitter`].
    Compose,
    /// Plain Kubernetes manifests, emitted by [`crate::KubernetesEmitter`].
    Kubernetes,
    /// A Helm chart (`Chart.yaml`, `values.yaml`, templates), emitted by
    /// [`crate::HelmEmitter`].
    Helm,
}

impl Target {
    /// Returns the stable lower-case label for this target.
    ///
    /// The label doubles as the CLI argument value and as the default output
    /// sub-directory name.
    ///
    /// ```rust
    /// use lightshuttle_export::Target;
    ///
    /// assert_eq!(Target::Compose.label(), "compose");
    /// assert_eq!(Target::Kubernetes.label(), "kubernetes");
    /// assert_eq!(Target::Helm.label(), "helm");
    /// ```
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

/// A set of named files produced by an emitter, ready to be written to disk
/// by the CLI layer.
///
/// Files are stored in deterministic emission order. The CLI writes them under
/// a per-target output directory; the relative paths inside [`ExportFile`]
/// determine the final names.
///
/// ```rust
/// use lightshuttle_export::ExportArtifacts;
///
/// let mut artifacts = ExportArtifacts::new();
/// artifacts.push("docker-compose.yml", "services: {}\n");
/// assert_eq!(artifacts.files.len(), 1);
/// assert_eq!(artifacts.files[0].path.to_str().unwrap(), "docker-compose.yml");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ExportArtifacts {
    /// Files in deterministic emission order.
    pub files: Vec<ExportFile>,
}

impl ExportArtifacts {
    /// Creates an empty artifact set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a file at `path` with the given `contents`.
    ///
    /// `path` is relative to the export output directory. `contents` is the
    /// complete textual content of the file.
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
