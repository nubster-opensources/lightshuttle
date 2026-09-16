//! Deployment-time placeholder rendering.
//!
//! `lower` resolves every manifest resource through `lightshuttle-spec`, but
//! it never interprets the `${...}` text carried by an image reference, a
//! command, a working directory, or an environment value: those strings stay
//! verbatim in the [`crate::ExportModel`]. This module is where that text
//! finally gets interpreted, once per export target.
//!
//! A `${resources.<name>.<property>}` reference is resolved eagerly against a
//! [`ResourceDirectory`], because its value depends on nothing but the
//! target's host naming (see the design's rendering table). A
//! `${env.<NAME>}` reference is not: the value is only known when the
//! deployment starts, so it survives as a variable that
//! [`PlaceholderRenderer::render`] renders in the syntax of one target
//! (Compose `${NAME}`, a Kubernetes refusal, or a Helm `.Values.variables.NAME`
//! lookup).
//!
//! Every emitter builds one [`ResourceDirectory`], then calls
//! [`DeploymentText::parse`] for every interpolatable field of every
//! resource, then renders the result through the [`PlaceholderRenderer`] that
//! matches its target.

use lightshuttle_manifest::Manifest;

use crate::Target;
use crate::error::Result;

/// A deployment-time string: literal text and variables left for the target.
///
/// Built by [`DeploymentText::parse`], which splits `raw` into the same
/// segments `lightshuttle-manifest` uses for runtime interpolation, resolves
/// every `${resources.<name>.<property>}` reference against a
/// [`ResourceDirectory`], and keeps every `${env.<NAME>}` reference as a
/// variable for a [`PlaceholderRenderer`] to render.
#[derive(Debug, Clone)]
pub struct DeploymentText {
    // Segments after resource resolution: literal text and unresolved
    // `${env.<NAME>}` references, in first-seen order, plus the resource and
    // field this text was parsed from for diagnostics. Populated once
    // `parse` is implemented.
}

/// Where a text lands, used for diagnostics and the D4 refusal (a sensitive
/// resource property may only be referenced from an environment value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField<'a> {
    /// The `image` reference of a resource.
    Image,
    /// The `entrypoint` override of a resource.
    Entrypoint,
    /// The `command` override of a resource.
    Command,
    /// The `working_dir` override of a resource.
    WorkingDir,
    /// One entry of a resource's environment map.
    Env {
        /// The environment key this text is assigned to.
        key: &'a str,
    },
    /// The `healthcheck` command of a resource.
    Healthcheck,
    /// A Dockerfile build input (build arg or build context) of a resource.
    BuildInput,
}

/// Resource outputs addressed for one target, keyed by resource name.
///
/// Built once per export target by [`ResourceDirectory::for_target`], then
/// passed to every [`DeploymentText::parse`] call so a
/// `${resources.<name>.<property>}` reference resolves to the value the
/// target actually reaches a service through: the raw manifest name for
/// Compose, the generated DNS name for Kubernetes and Helm.
#[derive(Debug, Clone)]
pub struct ResourceDirectory {
    // Resolved output properties, keyed by resource name. Populated once
    // `for_target` is implemented.
}

impl ResourceDirectory {
    /// Builds the directory for `target` (host naming per the design's
    /// rendering table).
    ///
    /// Resolves every resource declared in `manifest` through
    /// [`lightshuttle_spec::from_resource_on_host`] with the hostname
    /// `target` reaches that resource through.
    pub fn for_target(manifest: &Manifest, target: Target) -> Result<Self> {
        todo!()
    }
}

impl DeploymentText {
    /// Parses `raw`, resolves every resource reference against `directory`,
    /// and keeps every environment reference as a variable.
    ///
    /// `field` and `resource` are carried for diagnostics only: they name
    /// the offending field and resource in
    /// [`crate::ExportError::SensitiveReferenceOutsideEnv`] and
    /// [`crate::ExportError::InvalidVariableName`].
    pub fn parse(
        raw: &str,
        field: TextField<'_>,
        resource: &str,
        directory: &ResourceDirectory,
    ) -> Result<Self> {
        todo!()
    }

    /// Variables referenced by this text, defaults included, in first-seen
    /// order and without duplicates.
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        todo!()
    }

    /// Returns `true` when this text carries no variable: it renders
    /// identically on every target.
    #[must_use]
    pub fn is_literal(&self) -> bool {
        todo!()
    }
}

/// Renders a [`DeploymentText`] in the syntax of one export target.
///
/// One implementation per [`Target`]: [`ComposeRenderer`],
/// [`KubernetesRenderer`], and [`HelmRenderer`].
pub trait PlaceholderRenderer {
    /// Renders `text` in this renderer's target syntax.
    ///
    /// Returns [`crate::ExportError::UnresolvedVariables`] when `text`
    /// carries a variable with no default and this target refuses to export
    /// without one (Kubernetes).
    fn render(&self, text: &DeploymentText) -> Result<String>;
}

/// Renders a [`DeploymentText`] in Docker Compose interpolation syntax
/// (`${NAME}`, `${NAME:-default}`).
#[derive(Debug, Clone, Copy)]
pub struct ComposeRenderer;

/// Renders a [`DeploymentText`] for plain Kubernetes manifests: a variable
/// with a default is figured in place, a variable with no default is
/// refused.
#[derive(Debug, Clone, Copy)]
pub struct KubernetesRenderer;

/// Renders a [`DeploymentText`] for a Helm chart: a variable becomes a
/// `.Values.variables.NAME` lookup, wrapped in `required` or `default`
/// depending on whether the source carried a default.
#[derive(Debug, Clone, Copy)]
pub struct HelmRenderer;

impl PlaceholderRenderer for ComposeRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        todo!()
    }
}

impl PlaceholderRenderer for KubernetesRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        todo!()
    }
}

impl PlaceholderRenderer for HelmRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        todo!()
    }
}
