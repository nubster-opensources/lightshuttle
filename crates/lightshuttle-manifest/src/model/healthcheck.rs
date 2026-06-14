//! Healthcheck configuration, compatible with the Docker Compose conventions.
//!
//! A [`Healthcheck`] can be attached to any resource kind via its
//! `healthcheck` field. Duration fields accept strings like `"5s"`,
//! `"200ms"`, or `"2m"` and are validated by [`crate::Manifest::validate`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Per-resource healthcheck configuration.
///
/// Field semantics mirror Docker Compose so that existing knowledge
/// transfers directly. Duration fields (`interval`, `timeout`,
/// `start_period`) accept strings like `"5s"`, `"200ms"`, or `"2m"` and
/// are validated by [`crate::Manifest::validate`].
///
/// A `Healthcheck` is embedded in [`crate::PostgresConfig`], [`crate::RedisConfig`],
/// [`crate::ContainerConfig`], and [`crate::DockerfileConfig`] via their `healthcheck`
/// field, and is also accessible through [`crate::ResourceKind::healthcheck`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Healthcheck {
    /// Command to run. The first element must be `"CMD"` or `"CMD-SHELL"`.
    ///
    /// Cannot be empty (enforced by [`crate::Manifest::validate`]).
    pub test: Vec<String>,

    /// Time between consecutive check executions. Default `"5s"`.
    ///
    /// Accepted suffixes: `ns`, `us`, `ms`, `s`, `m`, `h`.
    #[serde(default = "default_interval")]
    pub interval: String,

    /// Maximum duration a single check execution may take before the
    /// runtime treats it as failed. Default `"3s"`.
    #[serde(default = "default_timeout")]
    pub timeout: String,

    /// Number of consecutive failures needed to declare the resource
    /// unhealthy. Default `5`.
    #[serde(default = "default_retries")]
    pub retries: u32,

    /// Grace period at startup during which check failures are not
    /// counted toward `retries`. Default `"5s"`.
    #[serde(default = "default_start_period")]
    pub start_period: String,
}

fn default_interval() -> String {
    "5s".to_owned()
}

fn default_timeout() -> String {
    "3s".to_owned()
}

fn default_retries() -> u32 {
    5
}

fn default_start_period() -> String {
    "5s".to_owned()
}
