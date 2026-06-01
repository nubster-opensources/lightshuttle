//! Self-contained container specification, derived from a manifest
//! resource declaration.

use std::collections::HashMap;
use std::time::Duration;

use indexmap::IndexMap;
use lightshuttle_manifest::{
    Command, ContainerConfig, DockerfileConfig, Healthcheck, PortMapping, PostgresConfig,
    RedisConfig, ResourceKind, Volume,
};

use crate::error::{Result, SpecError};

/// Properties a managed resource exposes to its dependents.
///
/// Keys follow the conventions documented in
/// `docs/spec/manifest-v0.md`:
///
/// - `host`, `port`, `database`, `user`, `password`, `url` for
///   `postgres`.
/// - `host`, `port`, `password`, `url` for `redis`.
/// - `host`, `ports` (comma-separated) for `container` and
///   `dockerfile`.
pub type ResourceOutputs = IndexMap<String, String>;

/// A [`ContainerSpec`] together with the outputs the resource exposes
/// to its dependents at runtime.
#[derive(Debug, Clone)]
pub struct ResolvedResource {
    /// Container specification consumed by the runtime.
    pub spec: ContainerSpec,
    /// Properties exposed to dependents (resolved into LSH_* env vars
    /// and substituted into `${resources.<name>.<property>}`
    /// expressions).
    pub outputs: ResourceOutputs,
}

const DEFAULT_PG_VERSION: &str = "16";
const DEFAULT_PG_USER: &str = "postgres";
const DEFAULT_PG_PORT: u16 = 5432;
const DEFAULT_REDIS_VERSION: &str = "7";
const DEFAULT_REDIS_PORT: u16 = 6379;
const HEALTHCHECK_DEFAULT_INTERVAL: Duration = Duration::from_secs(5);
const HEALTHCHECK_DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTHCHECK_DEFAULT_RETRIES: u32 = 5;
const HEALTHCHECK_DEFAULT_START_PERIOD: Duration = Duration::from_secs(5);

/// Self-contained description of a container to start, derived from a
/// manifest resource.
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    /// Container name, of the form `<project>_<resource>`.
    pub name: String,
    /// Project name as declared in the manifest. Used as a Docker
    /// label for discovery by `ps` and `down`.
    pub project: String,
    /// Resource name as declared in the manifest. Used as a Docker
    /// label so the CLI can find a single resource by name.
    pub resource: String,
    /// How the container image is obtained.
    pub image: ImageSource,
    /// Environment variables to inject into the container.
    pub env: HashMap<String, String>,
    /// Ports to publish.
    pub ports: Vec<PortBinding>,
    /// Volumes to mount.
    pub volumes: Vec<VolumeBinding>,
    /// Optional command override.
    pub command: Option<Vec<String>>,
    /// Optional healthcheck.
    pub healthcheck: Option<HealthcheckSpec>,
}

/// How the container image is obtained.
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Pull the image from a registry.
    Pull(String),
    /// Build the image locally from a Dockerfile.
    Build {
        /// Build context path, relative to the manifest file.
        context: String,
        /// Dockerfile path within the context.
        dockerfile: String,
        /// Build-time arguments.
        build_args: HashMap<String, String>,
        /// Optional multi-stage target.
        target: Option<String>,
        /// Tag applied to the resulting image.
        tag: String,
    },
}

/// Port mapping resolved from the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortBinding {
    /// Container-side port.
    pub container_port: u16,
    /// Optional host bind address.
    pub host_address: Option<String>,
    /// Host-side port. Mirrors the container port when the short form is used.
    pub host_port: u16,
}

/// Volume mapping resolved from the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeBinding {
    /// Source on the host or the named volume registry.
    pub source: VolumeSource,
    /// Mount point inside the container.
    pub target: String,
}

/// Where the volume content lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeSource {
    /// Bind mount from a host path.
    HostPath(String),
    /// Named volume managed by the runtime.
    Named(String),
    /// Anonymous volume (lifetime tied to the container).
    Anonymous,
}

/// Healthcheck resolved from the manifest, with manifest-side durations
/// already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthcheckSpec {
    /// Command to run for the check.
    pub test: Vec<String>,
    /// Interval between consecutive checks.
    pub interval: Duration,
    /// Maximum time a single check is allowed.
    pub timeout: Duration,
    /// Number of consecutive failures before marking unhealthy.
    pub retries: u32,
    /// Grace period after start.
    pub start_period: Duration,
}

/// Build a [`ContainerSpec`] from a manifest resource declaration.
///
/// Applies the v0 defaults documented in `docs/spec/manifest-v0.md`:
/// version expansion to official images, database name derived from
/// the resource name, default ports, healthcheck materialisation.
pub fn from_resource(
    project: &str,
    resource_name: &str,
    kind: &ResourceKind,
) -> Result<ResolvedResource> {
    let name = format!("{project}_{resource_name}");
    match kind {
        ResourceKind::Postgres(c) => spec_postgres(name, project, resource_name, c),
        ResourceKind::Redis(c) => spec_redis(name, project, resource_name, c),
        ResourceKind::Container(c) => spec_container(name, project, resource_name, c),
        ResourceKind::Dockerfile(c) => spec_dockerfile(name, project, resource_name, c),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn spec_postgres(
    name: String,
    project: &str,
    resource_name: &str,
    c: &PostgresConfig,
) -> Result<ResolvedResource> {
    let version = c.version.as_deref().unwrap_or(DEFAULT_PG_VERSION);
    let image = c
        .image
        .clone()
        .unwrap_or_else(|| format!("postgres:{version}-alpine"));
    let database = c
        .database
        .clone()
        .unwrap_or_else(|| resource_name.to_owned());
    let user = c.user.clone().unwrap_or_else(|| DEFAULT_PG_USER.to_owned());
    let password = c.password.clone().unwrap_or_else(generate_random_password);
    let port = c.port.unwrap_or(DEFAULT_PG_PORT);

    let mut env = HashMap::new();
    env.insert("POSTGRES_DB".to_owned(), database);
    env.insert("POSTGRES_USER".to_owned(), user.clone());
    env.insert("POSTGRES_PASSWORD".to_owned(), password);

    let ports = vec![PortBinding {
        container_port: port,
        host_address: None,
        host_port: port,
    }];

    let volumes = volume_to_binding(c.volume.as_ref(), "/var/lib/postgresql/data");

    let healthcheck = c
        .healthcheck
        .as_ref()
        .map(parse_healthcheck)
        .transpose()?
        .or_else(|| {
            Some(HealthcheckSpec {
                test: vec![
                    "CMD".to_owned(),
                    "pg_isready".to_owned(),
                    "-U".to_owned(),
                    user,
                ],
                interval: HEALTHCHECK_DEFAULT_INTERVAL,
                timeout: HEALTHCHECK_DEFAULT_TIMEOUT,
                retries: HEALTHCHECK_DEFAULT_RETRIES,
                start_period: HEALTHCHECK_DEFAULT_START_PERIOD,
            })
        });

    let spec = ContainerSpec {
        name: name.clone(),
        project: project.to_owned(),
        resource: resource_name.to_owned(),
        image: ImageSource::Pull(image),
        env: env.clone(),
        ports,
        volumes,
        command: None,
        healthcheck,
    };

    let mut outputs = ResourceOutputs::new();
    outputs.insert("host".to_owned(), name.clone());
    outputs.insert("port".to_owned(), port.to_string());
    let user_out = env.get("POSTGRES_USER").cloned().unwrap_or_default();
    let pwd_out = env.get("POSTGRES_PASSWORD").cloned().unwrap_or_default();
    let db_out = env.get("POSTGRES_DB").cloned().unwrap_or_default();
    outputs.insert("user".to_owned(), user_out.clone());
    outputs.insert("password".to_owned(), pwd_out.clone());
    outputs.insert("database".to_owned(), db_out.clone());
    outputs.insert(
        "url".to_owned(),
        format!("postgres://{user_out}:{pwd_out}@{name}:{port}/{db_out}"),
    );

    Ok(ResolvedResource { spec, outputs })
}

#[allow(clippy::needless_pass_by_value)]
fn spec_redis(
    name: String,
    project: &str,
    resource_name: &str,
    c: &RedisConfig,
) -> Result<ResolvedResource> {
    let version = c.version.as_deref().unwrap_or(DEFAULT_REDIS_VERSION);
    let image = c
        .image
        .clone()
        .unwrap_or_else(|| format!("redis:{version}-alpine"));
    let port = c.port.unwrap_or(DEFAULT_REDIS_PORT);

    let mut command = vec!["redis-server".to_owned()];
    if let Some(password) = c.password.as_deref()
        && !password.is_empty()
    {
        command.push("--requirepass".to_owned());
        command.push(password.to_owned());
    }

    let ports = vec![PortBinding {
        container_port: port,
        host_address: None,
        host_port: port,
    }];

    let volumes = volume_to_binding(c.volume.as_ref(), "/data");

    let healthcheck = c
        .healthcheck
        .as_ref()
        .map(parse_healthcheck)
        .transpose()?
        .or_else(|| {
            Some(HealthcheckSpec {
                test: vec!["CMD".to_owned(), "redis-cli".to_owned(), "ping".to_owned()],
                interval: HEALTHCHECK_DEFAULT_INTERVAL,
                timeout: HEALTHCHECK_DEFAULT_TIMEOUT,
                retries: HEALTHCHECK_DEFAULT_RETRIES,
                start_period: HEALTHCHECK_DEFAULT_START_PERIOD,
            })
        });

    let password_out = c.password.clone().unwrap_or_default();
    let spec = ContainerSpec {
        name: name.clone(),
        project: project.to_owned(),
        resource: resource_name.to_owned(),
        image: ImageSource::Pull(image),
        env: HashMap::new(),
        ports,
        volumes,
        command: Some(command),
        healthcheck,
    };

    let mut outputs = ResourceOutputs::new();
    outputs.insert("host".to_owned(), name.clone());
    outputs.insert("port".to_owned(), port.to_string());
    outputs.insert("password".to_owned(), password_out.clone());
    let url = if password_out.is_empty() {
        format!("redis://{name}:{port}")
    } else {
        format!("redis://:{password_out}@{name}:{port}")
    };
    outputs.insert("url".to_owned(), url);

    Ok(ResolvedResource { spec, outputs })
}

#[allow(clippy::needless_pass_by_value)]
fn spec_container(
    name: String,
    project: &str,
    resource_name: &str,
    c: &ContainerConfig,
) -> Result<ResolvedResource> {
    let env: HashMap<String, String> = c.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let ports = c
        .ports
        .iter()
        .map(parse_port_mapping)
        .collect::<Result<Vec<_>>>()?;
    let volumes = c
        .volumes
        .iter()
        .map(|s| parse_volume_string(s))
        .collect::<Result<Vec<_>>>()?;
    let command = c.command.as_ref().map(parse_command);
    let healthcheck = c.healthcheck.as_ref().map(parse_healthcheck).transpose()?;

    let ports_csv: String = ports
        .iter()
        .map(|p| p.container_port.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let spec = ContainerSpec {
        name: name.clone(),
        project: project.to_owned(),
        resource: resource_name.to_owned(),
        image: ImageSource::Pull(c.image.clone()),
        env,
        ports,
        volumes,
        command,
        healthcheck,
    };

    let mut outputs = ResourceOutputs::new();
    outputs.insert("host".to_owned(), name);
    outputs.insert("ports".to_owned(), ports_csv);

    Ok(ResolvedResource { spec, outputs })
}

#[allow(clippy::needless_pass_by_value)]
fn spec_dockerfile(
    name: String,
    project: &str,
    resource_name: &str,
    c: &DockerfileConfig,
) -> Result<ResolvedResource> {
    let tag = format!("lightshuttle/{name}:dev");

    let env: HashMap<String, String> = c.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let build_args: HashMap<String, String> = c
        .build_args
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let ports = c
        .ports
        .iter()
        .map(parse_port_mapping)
        .collect::<Result<Vec<_>>>()?;
    let volumes = c
        .volumes
        .iter()
        .map(|s| parse_volume_string(s))
        .collect::<Result<Vec<_>>>()?;
    let command = c.command.as_ref().map(parse_command);
    let healthcheck = c.healthcheck.as_ref().map(parse_healthcheck).transpose()?;

    let ports_csv: String = ports
        .iter()
        .map(|p| p.container_port.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let spec = ContainerSpec {
        name: name.clone(),
        project: project.to_owned(),
        resource: resource_name.to_owned(),
        image: ImageSource::Build {
            context: c.context.clone(),
            dockerfile: c.dockerfile.clone(),
            build_args,
            target: c.target.clone(),
            tag,
        },
        env,
        ports,
        volumes,
        command,
        healthcheck,
    };

    let mut outputs = ResourceOutputs::new();
    outputs.insert("host".to_owned(), name);
    outputs.insert("ports".to_owned(), ports_csv);

    Ok(ResolvedResource { spec, outputs })
}

fn volume_to_binding(volume: Option<&Volume>, target: &str) -> Vec<VolumeBinding> {
    match volume {
        None | Some(Volume::Boolean(true)) => vec![VolumeBinding {
            source: VolumeSource::Anonymous,
            target: target.to_owned(),
        }],
        Some(Volume::Boolean(false)) => Vec::new(),
        Some(Volume::Named(name)) => vec![VolumeBinding {
            source: VolumeSource::Named(name.clone()),
            target: target.to_owned(),
        }],
    }
}

fn parse_port_mapping(mapping: &PortMapping) -> Result<PortBinding> {
    match mapping {
        PortMapping::Container(port) => Ok(PortBinding {
            container_port: *port,
            host_address: None,
            host_port: *port,
        }),
        PortMapping::Mapping(s) => parse_port_string(s),
    }
}

fn parse_port_string(input: &str) -> Result<PortBinding> {
    let parts: Vec<&str> = input.split(':').collect();
    match parts.as_slice() {
        [host_port, container_port] => {
            let host_port: u16 = host_port
                .parse()
                .map_err(|_| SpecError::InvalidSpec(format!("invalid host port `{host_port}`")))?;
            let container_port: u16 = container_port.parse().map_err(|_| {
                SpecError::InvalidSpec(format!("invalid container port `{container_port}`"))
            })?;
            Ok(PortBinding {
                container_port,
                host_address: None,
                host_port,
            })
        }
        [host_address, host_port, container_port] => {
            let host_port: u16 = host_port
                .parse()
                .map_err(|_| SpecError::InvalidSpec(format!("invalid host port `{host_port}`")))?;
            let container_port: u16 = container_port.parse().map_err(|_| {
                SpecError::InvalidSpec(format!("invalid container port `{container_port}`"))
            })?;
            Ok(PortBinding {
                container_port,
                host_address: Some((*host_address).to_owned()),
                host_port,
            })
        }
        _ => Err(SpecError::InvalidSpec(format!(
            "invalid port mapping `{input}`"
        ))),
    }
}

fn parse_volume_string(input: &str) -> Result<VolumeBinding> {
    let (source, target) = input.split_once(':').ok_or_else(|| {
        SpecError::InvalidSpec(format!(
            "invalid volume mapping `{input}`: expected `src:target`"
        ))
    })?;
    let source = if source.starts_with('.') || source.starts_with('/') {
        VolumeSource::HostPath(source.to_owned())
    } else {
        VolumeSource::Named(source.to_owned())
    };
    Ok(VolumeBinding {
        source,
        target: target.to_owned(),
    })
}

fn parse_command(command: &Command) -> Vec<String> {
    match command {
        Command::Single(s) => vec!["sh".to_owned(), "-c".to_owned(), s.clone()],
        Command::Args(args) => args.clone(),
    }
}

fn parse_healthcheck(hc: &Healthcheck) -> Result<HealthcheckSpec> {
    Ok(HealthcheckSpec {
        test: hc.test.clone(),
        interval: parse_duration(&hc.interval)?,
        timeout: parse_duration(&hc.timeout)?,
        retries: hc.retries,
        start_period: parse_duration(&hc.start_period)?,
    })
}

fn parse_duration(input: &str) -> Result<Duration> {
    let trimmed = input.trim();
    let (digits, unit) = split_duration(trimmed)
        .ok_or_else(|| SpecError::InvalidSpec(format!("invalid duration `{input}`")))?;
    let value: f64 = digits
        .parse()
        .map_err(|_| SpecError::InvalidSpec(format!("invalid duration `{input}`")))?;
    let nanos = match unit {
        "ns" => value,
        "us" => value * 1_000.0,
        "ms" => value * 1_000_000.0,
        "s" => value * 1_000_000_000.0,
        "m" => value * 60.0 * 1_000_000_000.0,
        "h" => value * 3_600.0 * 1_000_000_000.0,
        _ => {
            return Err(SpecError::InvalidSpec(format!(
                "invalid duration unit `{unit}`"
            )));
        }
    };
    if nanos.is_sign_negative() || !nanos.is_finite() {
        return Err(SpecError::InvalidSpec(format!(
            "invalid duration `{input}`"
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Duration::from_nanos(nanos as u64))
}

fn split_duration(input: &str) -> Option<(&str, &str)> {
    let bytes = input.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'.') {
        idx += 1;
    }
    if idx == 0 || idx == bytes.len() {
        return None;
    }
    Some((&input[..idx], &input[idx..]))
}

/// Generate a 24-character alphanumeric password from a cryptographically
/// secure random source.
///
/// The alphabet excludes visually ambiguous characters (`0`, `O`, `1`,
/// `I`, `l`). The password is for local development and is surfaced
/// through `lightshuttle ps`; production export still requires an
/// explicit password.
fn generate_random_password() -> String {
    use rand::Rng;

    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    const LEN: usize = 24;

    let mut rng = rand::rng();
    (0..LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::generate_random_password;

    #[test]
    fn generated_password_has_expected_shape() {
        let password = generate_random_password();
        assert_eq!(password.len(), 24);
        assert!(
            password
                .chars()
                .all(|c| c.is_ascii_alphanumeric() && !"0O1Il".contains(c)),
            "password must be unambiguous alphanumeric, got `{password}`"
        );
    }

    #[test]
    fn generated_passwords_are_distinct() {
        // A clock-seeded generator would collide for calls within the
        // same instant; a CSPRNG must not.
        let first = generate_random_password();
        let second = generate_random_password();
        assert_ne!(first, second);
    }
}
