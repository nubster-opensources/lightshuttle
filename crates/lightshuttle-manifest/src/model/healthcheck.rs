//! Healthcheck configuration, compatible with the Docker Compose conventions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Per-resource healthcheck configuration.
///
/// The field semantics mirror those used by Docker Compose so that
/// developers already familiar with `docker-compose.yml` translate their
/// knowledge directly to LightShuttle manifests.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Healthcheck {
    /// Command to run. The first element should be `"CMD"` or
    /// `"CMD-SHELL"`.
    pub test: Vec<String>,

    /// Interval between consecutive checks. Default `"5s"`.
    #[serde(default = "default_interval")]
    pub interval: String,

    /// Maximum time a single check is allowed to run before being
    /// considered failed. Default `"3s"`.
    #[serde(default = "default_timeout")]
    pub timeout: String,

    /// Number of consecutive failed checks required to mark the resource
    /// as unhealthy. Default `5`.
    #[serde(default = "default_retries")]
    pub retries: u32,

    /// Grace period after the resource starts during which check
    /// failures are not counted. Default `"5s"`.
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
