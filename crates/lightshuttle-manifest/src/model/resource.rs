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
    Command, container::ContainerConfig, dockerfile::DockerfileConfig, healthcheck::Healthcheck,
    postgres::PostgresConfig, redis::RedisConfig,
};
use crate::interpolate::{InterpolationContext, Interpolator};

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

    /// Returns every string field of this resource that may carry a `${...}`
    /// interpolation.
    ///
    /// The list covers image and build inputs, environment values, volume
    /// mounts, working directory, command arguments and healthcheck test
    /// commands. It is the shared input to reference validation and to
    /// implicit dependency derivation.
    #[must_use]
    pub fn interpolatable_strings(&self) -> Vec<String> {
        // Derive the read-only scan from the one canonical field walk, so the
        // set of scanned fields and the set of substituted fields can never
        // drift apart (#276).
        let mut probe = self.clone();
        probe
            .interpolatable_fields_mut()
            .into_iter()
            .map(std::mem::take)
            .collect()
    }

    /// The single canonical enumeration of every interpolatable string field,
    /// as mutable references.
    ///
    /// Both [`Self::interpolatable_strings`] (read-only scanning, which drives
    /// reference validation and implicit dependency derivation) and
    /// [`Self::interpolate_in_place`] (runtime and export substitution) are
    /// defined in terms of this walk, so a field can never be scanned without
    /// being substituted, or substituted without being scanned.
    ///
    /// The walk covers image and build inputs, environment and secret values,
    /// volume mounts, entrypoint, command, working directory and healthcheck
    /// test commands.
    fn interpolatable_fields_mut(&mut self) -> Vec<&mut String> {
        let mut out: Vec<&mut String> = Vec::new();
        match self {
            Self::Container(c) => {
                out.push(&mut c.image);
                out.extend(c.env.values_mut());
                out.extend(c.secrets.values_mut());
                out.extend(c.volumes.iter_mut());
                if let Some(entrypoint) = c.entrypoint.as_mut() {
                    out.extend(command_fields_mut(entrypoint));
                }
                if let Some(command) = c.command.as_mut() {
                    out.extend(command_fields_mut(command));
                }
                if let Some(working_dir) = c.working_dir.as_mut() {
                    out.push(working_dir);
                }
                if let Some(healthcheck) = c.healthcheck.as_mut() {
                    out.extend(healthcheck.test.iter_mut());
                }
            }
            Self::Dockerfile(c) => {
                out.push(&mut c.context);
                out.push(&mut c.dockerfile);
                out.extend(c.env.values_mut());
                out.extend(c.secrets.values_mut());
                out.extend(c.volumes.iter_mut());
                out.extend(c.build_args.values_mut());
                if let Some(target) = c.target.as_mut() {
                    out.push(target);
                }
                if let Some(entrypoint) = c.entrypoint.as_mut() {
                    out.extend(command_fields_mut(entrypoint));
                }
                if let Some(command) = c.command.as_mut() {
                    out.extend(command_fields_mut(command));
                }
                if let Some(working_dir) = c.working_dir.as_mut() {
                    out.push(working_dir);
                }
                if let Some(healthcheck) = c.healthcheck.as_mut() {
                    out.extend(healthcheck.test.iter_mut());
                }
            }
            Self::Postgres(c) => {
                if let Some(password) = c.password.as_mut() {
                    out.push(password);
                }
                if let Some(database) = c.database.as_mut() {
                    out.push(database);
                }
                if let Some(user) = c.user.as_mut() {
                    out.push(user);
                }
                if let Some(healthcheck) = c.healthcheck.as_mut() {
                    out.extend(healthcheck.test.iter_mut());
                }
            }
            Self::Redis(c) => {
                if let Some(password) = c.password.as_mut() {
                    out.push(password);
                }
                if let Some(healthcheck) = c.healthcheck.as_mut() {
                    out.extend(healthcheck.test.iter_mut());
                }
            }
        }
        out
    }

    /// Interpolatable strings that become container environment variables or
    /// command arguments at runtime: environment and secret values, the
    /// command, and the connection parameters of a managed resource.
    ///
    /// This is the narrower scope that `secrets check` and the `up` preflight
    /// report against. Image references, volume mappings, the working directory,
    /// build inputs and healthcheck commands are deliberately excluded: an
    /// `${env.*}` reference there is resolved at start time but is not surfaced
    /// as a required secret (regression guard F3).
    #[must_use]
    pub fn environment_reference_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self {
            Self::Container(c) => {
                out.extend(c.env.values().cloned());
                out.extend(c.secrets.values().cloned());
                if let Some(command) = &c.command {
                    out.extend(command_strings(command));
                }
            }
            Self::Dockerfile(c) => {
                out.extend(c.env.values().cloned());
                out.extend(c.secrets.values().cloned());
                if let Some(command) = &c.command {
                    out.extend(command_strings(command));
                }
            }
            Self::Postgres(c) => {
                out.extend(c.password.clone());
                out.extend(c.user.clone());
                out.extend(c.database.clone());
            }
            Self::Redis(c) => {
                out.extend(c.password.clone());
            }
        }
        out
    }

    /// Resolve every interpolatable field in place, using `interpolator` to
    /// substitute `${env.*}` and `${resources.*.*}` expressions.
    ///
    /// This walks the exact same fields as [`Self::interpolatable_strings`],
    /// so substitution can never fall behind scanning. Interpolation runs
    /// before the resource is lowered to a [`lightshuttle_spec::ContainerSpec`],
    /// so canonical parsers (image reference, volume mapping, healthcheck)
    /// only ever see fully resolved values (#276).
    ///
    /// # Errors
    ///
    /// Returns a [`crate::ManifestError`] when any field references an unknown
    /// resource, property or environment variable without a default.
    pub fn interpolate_in_place(&mut self, interpolator: &Interpolator) -> crate::Result<()> {
        for field in self.interpolatable_fields_mut() {
            *field = interpolator.resolve(field)?;
        }
        Ok(())
    }

    /// Returns the names of resources this one implicitly depends on through
    /// `${resources.<name>.*}` interpolations in its string fields.
    ///
    /// Interpolating a property of another resource requires that resource to
    /// be started first, so it is documented as equivalent to an explicit
    /// `depends_on` entry. The returned names are de-duplicated while
    /// preserving first-occurrence order.
    ///
    /// The resource's own name is not filtered here because a [`ResourceKind`]
    /// does not carry its manifest key; the plan builder excludes self-loops.
    /// Interpolation syntax is assumed valid (the manifest is validated before
    /// a plan is built), so any string that fails to scan is skipped.
    #[must_use]
    pub fn implicit_dependencies(&self) -> Vec<String> {
        let ctx = InterpolationContext::new();
        let interpolator = Interpolator::new(&ctx);
        let mut out: Vec<String> = Vec::new();
        for value in self.interpolatable_strings() {
            let Ok(references) = interpolator.scan(&value) else {
                continue;
            };
            for reference in references {
                if let Some(name) = reference.resource_name()
                    && !out.contains(&name)
                {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Explicit `depends_on` unioned with the implicit dependencies derived
    /// from `${resources.<name>.*}` interpolations.
    ///
    /// Explicit entries keep their declared position, implicit ones are
    /// appended in first-occurrence order, and the whole list is de-duplicated.
    /// `own_name` is excluded so a self-referencing interpolation does not turn
    /// into a spurious cycle.
    #[must_use]
    pub fn merged_dependencies(&self, own_name: &str) -> Vec<String> {
        let mut dependencies = self.depends_on().to_vec();
        for implicit in self.implicit_dependencies() {
            if implicit != own_name && !dependencies.contains(&implicit) {
                dependencies.push(implicit);
            }
        }
        dependencies
    }
}

fn command_fields_mut(command: &mut Command) -> Vec<&mut String> {
    match command {
        Command::Single(s) => vec![s],
        Command::Args(args) => args.iter_mut().collect(),
    }
}

fn command_strings(command: &Command) -> Vec<String> {
    match command {
        Command::Single(s) => vec![s.clone()],
        Command::Args(args) => args.clone(),
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

#[cfg(test)]
mod interpolation_walk_tests {
    use crate::Manifest;

    const FULL_CONTAINER: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: "example/app:${env.TAG}"
      env:
        A: "${env.A_VALUE}"
      volumes:
        - "${env.DATA_DIR}:/data"
      entrypoint: "${env.ENTRY}"
      command: ["serve", "${env.PORT}"]
      working_dir: "${env.WORKDIR}"
      healthcheck:
        test: ["CMD", "curl", "${env.HEALTH_URL}"]
"#;

    #[test]
    fn interpolatable_strings_scans_every_field_including_entrypoint() {
        let manifest = Manifest::parse(FULL_CONTAINER).expect("manifest parses");
        let strings = manifest.resources["app"].interpolatable_strings();

        for needle in [
            "${env.TAG}",
            "${env.A_VALUE}",
            "${env.DATA_DIR}",
            "${env.ENTRY}",
            "${env.PORT}",
            "${env.WORKDIR}",
            "${env.HEALTH_URL}",
        ] {
            assert!(
                strings.iter().any(|s| s.contains(needle)),
                "{needle} must be scanned as interpolatable, got {strings:?}"
            );
        }
    }

    const FULL_DOCKERFILE: &str = r#"
project:
  name: app
resources:
  builder:
    dockerfile:
      context: "${env.CONTEXT}"
      dockerfile: "${env.DOCKERFILE}"
      build_args:
        VERSION: "${env.VERSION}"
      target: "${env.TARGET}"
      env:
        MODE: "${env.MODE}"
      volumes:
        - "${env.CACHE}:/cache"
      entrypoint: "${env.ENTRY}"
      command: ["build", "${env.FLAG}"]
      working_dir: "${env.WORKDIR}"
      healthcheck:
        test: ["CMD", "${env.PROBE}"]
"#;

    #[test]
    fn scan_and_substitution_do_not_drift_on_a_dockerfile_resource() {
        use crate::interpolate::{InterpolationContext, Interpolator};

        let manifest = Manifest::parse(FULL_DOCKERFILE).expect("manifest parses");

        // Every interpolatable field, including all build inputs, must be
        // scanned. A field added to the config but forgotten in the canonical
        // walk would slip through this list.
        let scanned = manifest.resources["builder"].interpolatable_strings();
        for needle in [
            "${env.CONTEXT}",
            "${env.DOCKERFILE}",
            "${env.VERSION}",
            "${env.TARGET}",
            "${env.MODE}",
            "${env.CACHE}",
            "${env.ENTRY}",
            "${env.FLAG}",
            "${env.WORKDIR}",
            "${env.PROBE}",
        ] {
            assert!(
                scanned.iter().any(|s| s.contains(needle)),
                "{needle} must be scanned, got {scanned:?}"
            );
        }

        // Substitution walks the exact same fields: after resolving, nothing
        // scanned may still hold a reference, which is what guarantees scan and
        // substitution cannot drift (#276).
        let ctx = InterpolationContext::new().with_env([
            ("CONTEXT".to_owned(), ".".to_owned()),
            ("DOCKERFILE".to_owned(), "Dockerfile".to_owned()),
            ("VERSION".to_owned(), "1".to_owned()),
            ("TARGET".to_owned(), "release".to_owned()),
            ("MODE".to_owned(), "prod".to_owned()),
            ("CACHE".to_owned(), "/tmp/cache".to_owned()),
            ("ENTRY".to_owned(), "/bin/build".to_owned()),
            ("FLAG".to_owned(), "--fast".to_owned()),
            ("WORKDIR".to_owned(), "/work".to_owned()),
            ("PROBE".to_owned(), "true".to_owned()),
        ]);
        let interpolator = Interpolator::new(&ctx);

        let mut kind = manifest.resources["builder"].clone();
        kind.interpolate_in_place(&interpolator)
            .expect("every reference resolves");

        let resolved = kind.interpolatable_strings();
        assert_eq!(
            resolved.len(),
            scanned.len(),
            "substitution must walk exactly the scanned fields"
        );
        assert!(
            resolved.iter().all(|s| !s.contains("${")),
            "no scanned field may retain a reference after substitution, got {resolved:?}"
        );
    }

    #[test]
    fn interpolate_in_place_resolves_every_scanned_field() {
        use crate::interpolate::{InterpolationContext, Interpolator};

        let manifest = Manifest::parse(FULL_CONTAINER).expect("manifest parses");
        let ctx = InterpolationContext::new().with_env([
            ("TAG".to_owned(), "v1".to_owned()),
            ("A_VALUE".to_owned(), "x".to_owned()),
            ("DATA_DIR".to_owned(), "/srv".to_owned()),
            ("ENTRY".to_owned(), "run".to_owned()),
            ("PORT".to_owned(), "8080".to_owned()),
            ("WORKDIR".to_owned(), "/app".to_owned()),
            ("HEALTH_URL".to_owned(), "http://h".to_owned()),
        ]);
        let interpolator = Interpolator::new(&ctx);

        let mut kind = manifest.resources["app"].clone();
        kind.interpolate_in_place(&interpolator)
            .expect("every reference resolves");

        let resolved = kind.interpolatable_strings();
        assert!(
            resolved.iter().all(|s| !s.contains("${")),
            "no field may retain an unresolved reference, got {resolved:?}"
        );
        assert!(
            resolved.iter().any(|s| s == "example/app:v1"),
            "image must be resolved, got {resolved:?}"
        );
        assert!(
            resolved.iter().any(|s| s == "run"),
            "entrypoint must be resolved, got {resolved:?}"
        );
        assert!(
            resolved.iter().any(|s| s == "/srv:/data"),
            "volume must be resolved, got {resolved:?}"
        );
    }
}
