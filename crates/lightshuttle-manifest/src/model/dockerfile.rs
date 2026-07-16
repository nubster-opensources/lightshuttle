//! Dockerfile resource configuration (image built locally).
//!
//! A `dockerfile` resource builds an image from a local `Dockerfile` and
//! runs the resulting container. For pre-built registry images use
//! [`crate::ContainerConfig`] instead.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{command::Command, healthcheck::Healthcheck, port::PortMapping};

/// Configuration of a `dockerfile` resource built locally before being run.
///
/// The runtime performs a `docker build` in `context`, then starts the
/// resulting image as it would for a [`crate::ContainerConfig`].
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DockerfileConfig {
    /// Build context path, relative to the manifest file.
    ///
    /// Resolved to an absolute path by
    /// [`crate::Manifest::resolve_host_volume_paths`] before it is handed to the
    /// runtime.
    pub context: String,

    /// Path to the Dockerfile within `context`. Defaults to `"Dockerfile"`.
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,

    /// Build-time `ARG` values passed to `docker build --build-arg`.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub build_args: IndexMap<String, String>,

    /// Multi-stage build target passed to `docker build --target`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Port mappings between the host and the container. See [`PortMapping`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortMapping>,

    /// Environment variables injected into the container at runtime.
    ///
    /// Values support `${env.NAME}` and `${resources.name.property}`
    /// interpolation.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,

    /// Volume mappings in `"host:container"` or `"named:container"` form.
    ///
    /// Relative host paths are resolved by
    /// [`crate::Manifest::resolve_host_volume_paths`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,

    /// Optional override for the image `ENTRYPOINT`, the executable the
    /// container runs. See [`Command`] for the accepted forms.
    ///
    /// Setting this discards the image `CMD`: every target (the Engine
    /// API, Compose and Kubernetes) ignores the image default command
    /// once an entrypoint is overridden. Set `command` as well to supply
    /// arguments. An empty list or a blank string is rejected; omit the
    /// field to keep the image entrypoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Command>,

    /// Optional override for the image default `CMD`. The image
    /// `ENTRYPOINT` is preserved. See [`Command`] for the accepted forms
    /// and for what this means against an image whose entrypoint is a
    /// binary rather than a shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,

    /// Optional working directory override inside the container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Optional healthcheck override. See [`Healthcheck`] for field semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of other resources this build must wait for before starting.
    /// Validated by [`crate::Manifest::validate`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

fn default_dockerfile() -> String {
    "Dockerfile".to_owned()
}

impl DockerfileConfig {
    /// Builds a [`DockerfileConfig`] for the given build `context`, with
    /// the Dockerfile path defaulted to `"Dockerfile"` and every other
    /// field (build args, target, ports, env, volumes, entrypoint,
    /// command, working directory, healthcheck, dependencies) defaulted
    /// to empty or `None`.
    ///
    /// Callers set the remaining fields as needed.
    #[must_use]
    pub fn new(context: String) -> Self {
        Self {
            context,
            dockerfile: default_dockerfile(),
            build_args: IndexMap::new(),
            target: None,
            ports: Vec::new(),
            env: IndexMap::new(),
            volumes: Vec::new(),
            entrypoint: None,
            command: None,
            working_dir: None,
            healthcheck: None,
            depends_on: Vec::new(),
        }
    }
}
