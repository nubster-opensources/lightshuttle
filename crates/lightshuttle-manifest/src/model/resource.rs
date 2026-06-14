//! Resource kind enumeration tagged externally by the YAML key.
//!
//! Each entry in the `resources:` map of a manifest is a [`ResourceKind`]
//! value. The variant is determined by the single YAML key nested under
//! the resource name (`postgres`, `redis`, `container`, or `dockerfile`).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::{Deserializer, Error as DeError};
use serde::ser::{Serialize, SerializeMap, Serializer};

use super::{
    container::ContainerConfig, dockerfile::DockerfileConfig, healthcheck::Healthcheck,
    postgres::PostgresConfig, redis::RedisConfig,
};

/// Kind-specific configuration of a resource declared in `resources:`.
///
/// The variant is selected by the single YAML key nested under a resource
/// name:
///
/// ```yaml
/// api_db:
///   postgres:    # selects ResourceKind::Postgres
///     version: "16"
/// cache:
///   redis: {}   # selects ResourceKind::Redis
/// ```
///
/// `serde`'s default external tagging would emit a YAML tag (`!postgres`)
/// rather than a plain map key, so `Serialize` and `Deserialize` are
/// implemented manually to preserve the format defined by the specification.
///
/// Use [`ResourceKind::depends_on`], [`ResourceKind::healthcheck`], and
/// [`ResourceKind::kind_name`] to query cross-cutting properties without
/// pattern-matching on the variant.
#[derive(Debug, Clone, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    /// Managed PostgreSQL instance. Configuration carried by [`PostgresConfig`].
    Postgres(PostgresConfig),

    /// Managed Redis instance. Configuration carried by [`RedisConfig`].
    Redis(RedisConfig),

    /// Container pulled from a registry. Configuration carried by [`ContainerConfig`].
    Container(ContainerConfig),

    /// Container built locally from a Dockerfile. Configuration carried by [`DockerfileConfig`].
    Dockerfile(DockerfileConfig),
}

impl ResourceKind {
    /// Returns the `depends_on` list declared for this resource, regardless of
    /// variant.
    ///
    /// The returned slice is empty when no explicit dependencies are declared.
    /// The validation pass verifies that every name in this list refers to a
    /// resource that exists in the manifest.
    #[must_use]
    pub fn depends_on(&self) -> &[String] {
        match self {
            Self::Postgres(c) => &c.depends_on,
            Self::Redis(c) => &c.depends_on,
            Self::Container(c) => &c.depends_on,
            Self::Dockerfile(c) => &c.depends_on,
        }
    }

    /// Returns the healthcheck override for this resource, if any.
    ///
    /// A `None` result means the runtime falls back to its built-in default
    /// for the resource kind. See [`Healthcheck`] for field semantics.
    #[must_use]
    pub fn healthcheck(&self) -> Option<&Healthcheck> {
        match self {
            Self::Postgres(c) => c.healthcheck.as_ref(),
            Self::Redis(c) => c.healthcheck.as_ref(),
            Self::Container(c) => c.healthcheck.as_ref(),
            Self::Dockerfile(c) => c.healthcheck.as_ref(),
        }
    }

    /// Returns the YAML key that identifies this variant (`"postgres"`,
    /// `"redis"`, `"container"`, or `"dockerfile"`).
    ///
    /// Used in diagnostic messages and export target logic.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Postgres(_) => "postgres",
            Self::Redis(_) => "redis",
            Self::Container(_) => "container",
            Self::Dockerfile(_) => "dockerfile",
        }
    }
}

impl Serialize for ResourceKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Postgres(c) => map.serialize_entry("postgres", c)?,
            Self::Redis(c) => map.serialize_entry("redis", c)?,
            Self::Container(c) => map.serialize_entry("container", c)?,
            Self::Dockerfile(c) => map.serialize_entry("dockerfile", c)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ResourceKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Each resource entry is a YAML map with exactly one key whose
        // name selects the variant.
        let entries: BTreeMap<String, serde_norway::Value> = BTreeMap::deserialize(deserializer)?;

        let mut iter = entries.into_iter();
        let (kind, value) = iter
            .next()
            .ok_or_else(|| DeError::custom("resource entry must contain exactly one kind"))?;
        if iter.next().is_some() {
            return Err(DeError::custom(
                "resource entry must contain exactly one kind",
            ));
        }

        match kind.as_str() {
            "postgres" => serde_norway::from_value(value)
                .map(Self::Postgres)
                .map_err(|e| DeError::custom(e.to_string())),
            "redis" => serde_norway::from_value(value)
                .map(Self::Redis)
                .map_err(|e| DeError::custom(e.to_string())),
            "container" => serde_norway::from_value(value)
                .map(Self::Container)
                .map_err(|e| DeError::custom(e.to_string())),
            "dockerfile" => serde_norway::from_value(value)
                .map(Self::Dockerfile)
                .map_err(|e| DeError::custom(e.to_string())),
            other => Err(DeError::custom(format!("unknown resource kind `{other}`"))),
        }
    }
}
