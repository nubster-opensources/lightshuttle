//! Dockerfile resource configuration (image built locally).

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{command::Command, healthcheck::Healthcheck, port::PortMapping};

/// Configuration of a resource built locally from a Dockerfile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DockerfileConfig {
    /// Build context, relative to the manifest file.
    pub context: String,

    /// Dockerfile path within the context. Defaults to `"Dockerfile"`.
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,

    /// Build-time arguments.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub build_args: IndexMap<String, String>,

    /// Multi-stage build target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Port mappings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortMapping>,

    /// Environment variables injected at runtime. Values support
    /// interpolation.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,

    /// Volume mappings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,

    /// Override of the image entrypoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,

    /// Override of the image working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Optional healthcheck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of resources this build explicitly depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

fn default_dockerfile() -> String {
    "Dockerfile".to_owned()
}
