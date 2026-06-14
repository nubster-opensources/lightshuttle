//! Optional dashboard section of the manifest.
//!
//! [`DashboardConfig`] maps to the `dashboard:` top-level key in
//! `lightshuttle.yml` and is stored in [`crate::Manifest::dashboard`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Settings for the local control-plane HTTP server (the dashboard).
///
/// Stored in [`crate::Manifest::dashboard`]. When the whole `dashboard:` section
/// is absent from the manifest, the runtime uses its built-in defaults
/// (random free port, all-loopback binding).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DashboardConfig {
    /// Fixed TCP port for the dashboard HTTP server.
    ///
    /// - Absent or `null`: the runtime picks a random free port at startup.
    /// - `0`: rejected by [`crate::Manifest::validate`] with
    ///   [`crate::ManifestError::InvalidDashboardPort`] (indistinguishable from
    ///   "no preference" at the OS level).
    /// - `1..=65535`: the dashboard binds to this port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}
