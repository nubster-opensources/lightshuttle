//! Optional dashboard section of the manifest.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Settings for the local control plane HTTP server.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DashboardConfig {
    /// Fixed TCP port for the dashboard. Absent or `null` lets
    /// `lightshuttle up` pick a random free port. Zero is rejected as
    /// it would be indistinguishable from "no preference" at the
    /// runtime layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}
