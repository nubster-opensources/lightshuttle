//! Strongly-typed model of a LightShuttle manifest.

pub mod command;
pub mod container;
pub mod dashboard;
pub mod dockerfile;
pub mod healthcheck;
pub mod manifest;
pub mod port;
pub mod postgres;
pub mod redis;
pub mod resource;
pub mod volume;

pub use command::Command;
pub use container::ContainerConfig;
pub use dashboard::DashboardConfig;
pub use dockerfile::DockerfileConfig;
pub use healthcheck::Healthcheck;
pub use manifest::{Manifest, Project, Version};
pub use port::PortMapping;
pub use postgres::PostgresConfig;
pub use redis::RedisConfig;
pub use resource::ResourceKind;
pub use volume::Volume;
