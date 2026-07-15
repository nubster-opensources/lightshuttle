//! Strongly-typed model of a LightShuttle manifest.
//!
//! Each sub-module owns one concept from the `lightshuttle.yml` schema.
//! All public types are re-exported here and again at the crate root for
//! ergonomic access via `use lightshuttle_manifest::TypeName`.

/// Container `CMD` override types.
pub mod command;
/// Registry-backed container resource configuration.
pub mod container;
/// Local control-plane dashboard configuration.
pub mod dashboard;
/// Locally-built Dockerfile resource configuration.
pub mod dockerfile;
/// Export target overrides (compose, kubernetes, helm).
pub mod export;
/// Per-resource healthcheck configuration.
pub mod healthcheck;
/// Top-level manifest and project types.
pub mod manifest;
/// Observability section of the manifest.
pub mod observability;
/// Port mapping type.
pub mod port;
/// PostgreSQL resource configuration.
pub mod postgres;
/// Redis resource configuration.
pub mod redis;
/// Resource kind enumeration.
pub mod resource;
/// Volume persistence specification.
pub mod volume;

pub use command::Command;
pub use container::ContainerConfig;
pub use dashboard::DashboardConfig;
pub use dockerfile::DockerfileConfig;
pub use export::{
    ComposeExport, ComposeResourceExport, ExportConfig, HelmExport, HelmResourceExport,
    ImagePullPolicy, KubernetesExport, KubernetesResourceExport,
};
pub use healthcheck::Healthcheck;
pub use manifest::{Manifest, Project, Version};
pub use observability::{ObservabilityConfig, OtelConfig};
pub use port::PortMapping;
pub use postgres::PostgresConfig;
pub use redis::RedisConfig;
pub use resource::ResourceKind;
pub use volume::Volume;
