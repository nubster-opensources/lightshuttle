//! PostgreSQL resource configuration.
//!
//! A `postgres` resource provisions a managed PostgreSQL container. The
//! runtime selects a suitable image automatically from `version` unless
//! `image` is supplied explicitly.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{healthcheck::Healthcheck, volume::Volume};

/// Configuration of a managed PostgreSQL instance.
///
/// The runtime resolves the effective image using the following priority
/// order: `image` (if set) takes precedence; otherwise `version` is
/// expanded to `postgres:<version>-alpine`; if neither is set the
/// runtime picks its own default.
///
/// The `database` field must match `^[a-z][a-z0-9_]{0,62}$` and is
/// validated by [`crate::Manifest::validate`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    /// PostgreSQL major version, e.g. `"16"`.
    ///
    /// Expanded into `postgres:<version>-alpine` when `image` is absent.
    /// Ignored when `image` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Explicit image reference. Takes precedence over `version`.
    ///
    /// Use this to pin a specific digest or to point to a private registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Initial database name created at first startup.
    ///
    /// Must match `^[a-z][a-z0-9_]{0,62}$`. Defaults to the resource
    /// name when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Superuser account name. Defaults to `"postgres"` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Superuser password. The runtime generates a random password when
    /// this is unset and exposes it via `${resources.name.password}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Host port the container port `5432` is mapped to.
    ///
    /// The runtime chooses a random free port when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Persistent volume configuration. See [`Volume`] for the accepted forms.
    ///
    /// Defaults to an auto-named volume when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,

    /// Healthcheck override. Replaces the built-in `pg_isready` check.
    ///
    /// See [`Healthcheck`] for field semantics and defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Healthcheck>,

    /// Names of other resources this instance must wait for before starting.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}
