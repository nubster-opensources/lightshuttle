//! Redis resource configuration.
//!
//! A `redis` resource provisions a managed Redis container. Image
//! selection follows the same `version`/`image` priority as
//! [`crate::PostgresConfig`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{healthcheck::Healthcheck, volume::Volume};

/// Configuration of a managed Redis instance.
///
/// The runtime resolves the effective image using the same priority as
/// [`crate::PostgresConfig`]: `image` takes precedence; otherwise `version` is
/// expanded to `redis:<version>-alpine`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    /// Redis major version, e.g. `"7"`.
    ///
    /// Expanded into `redis:<version>-alpine` when `image` is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Explicit image reference. Takes precedence over `version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Authentication password for the `requirepass` directive.
    ///
    /// An empty string or `None` runs Redis without authentication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Host port the container port `6379` is mapped to.
    ///
    /// The runtime chooses a random free port when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Persistent volume configuration. See [`Volume`] for accepted forms.
    ///
    /// Defaults to an auto-named volume when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,

    /// Healthcheck override. Replaces the built-in `redis-cli PING` check.
    ///
    /// See [`Healthcheck`] for field semantics and defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of other resources this instance must wait for before starting.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}
