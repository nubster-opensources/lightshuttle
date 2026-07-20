//! Container resource configuration (image pulled from a registry).
//!
//! A `container` resource declares a pre-built image that LightShuttle
//! will pull and run. For locally-built images use [`crate::DockerfileConfig`]
//! instead.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{command::Command, healthcheck::Healthcheck, port::PortMapping};

/// Configuration of a `container` resource backed by a registry image.
///
/// Corresponds to the `container:` key in a resource entry. The runtime
/// pulls `image`, applies the declared port mappings, mounts volumes,
/// and injects environment variables before starting the container.
///
/// See [`crate::DockerfileConfig`] for the locally-built equivalent.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContainerConfig {
    /// Full image reference including the tag, e.g. `"nginx:1.25-alpine"`.
    pub image: String,

    /// Port mappings between the host and the container.
    ///
    /// Each element is a [`PortMapping`]: either a bare container port
    /// (mirrored on the host) or a full `"host:container"` string.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortMapping>,

    /// Environment variables injected into the container at startup.
    ///
    /// Values are interpolated: `${env.NAME}` and
    /// `${resources.name.property}` expressions are resolved at runtime.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,

    /// Sensitive environment variables injected at runtime.
    ///
    /// These values behave like `env` during local execution, but production
    /// exporters replace them with placeholders instead of writing their
    /// contents to Compose, Kubernetes or Helm artifacts.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub secrets: IndexMap<String, String>,

    /// Volume mappings in `"host:container"` or `"named:container"` form.
    ///
    /// Relative host paths (starting with `.`) are resolved against the
    /// manifest directory by [`crate::Manifest::resolve_host_volume_paths`] before
    /// they reach the runtime.
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
    /// `ENTRYPOINT` is preserved. See [`Command`] for the two accepted
    /// forms (string or argument list) and for what this means against an
    /// image whose entrypoint is a binary rather than a shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,

    /// Optional working directory override inside the container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Optional healthcheck. Overrides whatever is baked into the image.
    ///
    /// See [`Healthcheck`] for field semantics and defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of other resources this container must wait for before
    /// starting. Validated by [`crate::Manifest::validate`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

impl ContainerConfig {
    /// Builds a [`ContainerConfig`] pulling `image`, with no port mappings,
    /// environment variables, volumes, entrypoint, command, working
    /// directory, healthcheck or dependencies.
    ///
    /// Callers set the remaining fields as needed.
    #[must_use]
    pub fn new(image: String) -> Self {
        Self {
            image,
            ports: Vec::new(),
            env: IndexMap::new(),
            secrets: IndexMap::new(),
            volumes: Vec::new(),
            entrypoint: None,
            command: None,
            working_dir: None,
            healthcheck: None,
            depends_on: Vec::new(),
        }
    }
}
