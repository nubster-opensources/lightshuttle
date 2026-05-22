//! Resource kind enumeration tagged externally by the YAML key.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::{Deserializer, Error as DeError};
use serde::ser::{Serialize, SerializeMap, Serializer};

use super::{
    container::ContainerConfig, dockerfile::DockerfileConfig, healthcheck::Healthcheck,
    postgres::PostgresConfig, redis::RedisConfig,
};

/// Kind-specific configuration of a resource.
///
/// The variant is selected by the YAML key under the resource entry
/// (externally tagged shape):
///
/// ```yaml
/// api_db:
///   postgres:    # ← variant is ResourceKind::Postgres
///     version: "16"
/// ```
///
/// Serde's default external tagging produces a YAML tag (`!postgres
/// ...`) rather than a map entry, so this enum implements `Serialize`
/// and `Deserialize` manually to keep the manifest format identical to
/// the specification.
#[derive(Debug, Clone, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    /// PostgreSQL resource.
    Postgres(PostgresConfig),

    /// Redis resource.
    Redis(RedisConfig),

    /// Container pulled from a registry.
    Container(ContainerConfig),

    /// Container built locally from a Dockerfile.
    Dockerfile(DockerfileConfig),
}

impl ResourceKind {
    /// The list of resources this one explicitly depends on.
    #[must_use]
    pub fn depends_on(&self) -> &[String] {
        match self {
            Self::Postgres(c) => &c.depends_on,
            Self::Redis(c) => &c.depends_on,
            Self::Container(c) => &c.depends_on,
            Self::Dockerfile(c) => &c.depends_on,
        }
    }

    /// The optional healthcheck override for this resource.
    #[must_use]
    pub fn healthcheck(&self) -> Option<&Healthcheck> {
        match self {
            Self::Postgres(c) => c.healthcheck.as_ref(),
            Self::Redis(c) => c.healthcheck.as_ref(),
            Self::Container(c) => c.healthcheck.as_ref(),
            Self::Dockerfile(c) => c.healthcheck.as_ref(),
        }
    }

    /// The kind name as it appears in YAML, for diagnostic messages.
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
