//! Redis resource configuration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{healthcheck::Healthcheck, volume::Volume};

/// Configuration of a Redis resource.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    /// Major version. Expanded into `redis:<version>-alpine` when
    /// `image` is unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Explicit image reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Authentication password. An empty string disables authentication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Container port. Defaults to `6379` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Persistent volume configuration. Defaults to an auto-named
    /// volume when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,

    /// Override of the default `redis-cli PING` healthcheck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of resources this Redis instance explicitly depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}
