//! Self-contained container specification, derived from a manifest
//! resource declaration.
//!
//! This module contains the resolved types consumed by
//! `lightshuttle-runtime` and `lightshuttle-export`, together with the
//! private resolution helpers that apply v0 defaults. The public entry
//! point is [`from_resource`].

use std::collections::HashMap;
use std::time::Duration;

use indexmap::IndexMap;
use lightshuttle_manifest::{
    Command, ContainerConfig, DockerfileConfig, Healthcheck, PortMapping, PostgresConfig,
    RedisConfig, ResourceKind, Volume,
};

use crate::error::{Result, SpecError};

/// Key/value properties that a managed resource exposes to its
/// dependents.
///
/// The map is ordered by insertion order (backed by [`indexmap::IndexMap`])
/// so that export serializers produce deterministic output.
///
/// # Key conventions (manifest-v0)
///
/// | Resource kind | Available keys |
/// |---|---|
/// | `postgres` | `host`, `port`, `database`, `user`, `password`, `url` |
/// | `redis` | `host`, `port`, `password`, `url` |
/// | `container` / `dockerfile` | `host`, `ports` (comma-separated list) |
///
/// These keys are surfaced at runtime as `LSH_<RESOURCE>_<KEY>` env
/// vars and substituted into `${resources.<name>.<key>}` expressions
/// in sibling resource declarations.
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::ResourceOutputs;
///
/// let mut outputs = ResourceOutputs::new();
/// outputs.insert("host".into(), "myproject_db".into());
/// outputs.insert("port".into(), "5432".into());
///
/// assert_eq!(outputs["host"], "myproject_db");
/// ```
pub type ResourceOutputs = IndexMap<String, String>;

/// A [`ContainerSpec`] bundled with the [`ResourceOutputs`] the
/// resource exposes to its dependents at runtime.
///
/// Produced by [`from_resource`] and consumed by both
/// `lightshuttle-runtime` (to launch the container) and
/// `lightshuttle-export` (to emit a Compose/Helm artifact).
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_manifest::{PostgresConfig, ResourceKind};
/// use lightshuttle_spec::from_resource;
///
/// // Resolve a postgres resource declared in a manifest.
/// let kind = ResourceKind::Postgres(PostgresConfig::default());
/// let resolved = from_resource("myproject", "db", &kind).unwrap();
///
/// // The spec carries the container description.
/// assert_eq!(resolved.spec.resource, "db");
/// // The outputs expose the connection URL to dependents.
/// assert!(resolved.outputs.contains_key("url"));
/// ```
#[derive(Debug, Clone)]
pub struct ResolvedResource {
    /// Container specification consumed by the runtime and the export
    /// pipeline to describe the container to launch.
    pub spec: ContainerSpec,
    /// Key/value properties exposed to dependents, resolved into
    /// `LSH_*` env vars and substituted into
    /// `${resources.<name>.<property>}` expressions.
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
/// manifest resource declaration.
///
/// All fields are fully resolved: defaults have been applied, optional
/// values materialised, and duration strings parsed. Consumers (the
/// runtime and the export pipeline) can use this struct directly
/// without any further resolution.
///
/// Created exclusively by [`from_resource`].
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    /// Container name, of the form `<project>_<resource>`.
    ///
    /// Used as the actual container name when starting the container
    /// and as the DNS hostname reachable by other containers in the
    /// same network.
    pub name: String,
    /// Project name as declared in the manifest.
    ///
    /// Attached as a container label so that `lightshuttle ps` and
    /// `lightshuttle down` can filter by project.
    pub project: String,
    /// Resource name as declared in the manifest.
    ///
    /// Attached as a container label so that the CLI can address a
    /// single resource by name within a project.
    pub resource: String,
    /// How the container image is obtained: pulled from a registry or
    /// built locally from a Dockerfile.
    pub image: ImageSource,
    /// Environment variables to inject into the container at startup.
    pub env: HashMap<String, String>,
    /// Host-to-container port bindings to publish.
    pub ports: Vec<PortBinding>,
    /// Volume and bind-mount mappings to attach.
    pub volumes: Vec<VolumeBinding>,
    /// Optional override for the image `ENTRYPOINT`, the executable the
    /// container runs.
    ///
    /// A `Command::Single` string is wrapped as `["sh", "-c", ...]`;
    /// a `Command::Args` list is passed through as-is. `None` leaves the
    /// image entrypoint in place.
    pub entrypoint: Option<Vec<String>>,
    /// Optional command that overrides the image default `CMD`.
    ///
    /// A `Command::Single` string is wrapped as `["sh", "-c", ...]`;
    /// a `Command::Args` list is passed through as-is.
    pub command: Option<Vec<String>>,
    /// Optional healthcheck. For `postgres` and `redis`, a sensible
    /// default is injected when the manifest omits one.
    pub healthcheck: Option<HealthcheckSpec>,
    /// Optional working directory override inside the container.
    pub working_dir: Option<String>,
}

/// How the container image is obtained.
///
/// Produced during resolution and consumed by the runtime (to decide
/// whether to call `docker pull` or `docker build`) and by the export
/// pipeline (to emit the correct Compose `image` or `build` stanza).
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::ImageSource;
/// use std::collections::HashMap;
///
/// // A pre-built image pulled from a registry.
/// let pulled = ImageSource::Pull("postgres:16-alpine".into());
///
/// // An image built locally from a Dockerfile.
/// let built = ImageSource::Build {
///     context: ".".into(),
///     dockerfile: "Dockerfile".into(),
///     build_args: HashMap::new(),
///     target: None,
///     tag: "lightshuttle/myproject_app:dev".into(),
/// };
/// ```
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Pull the named image reference from a registry.
    ///
    /// The inner `String` is a fully qualified image reference such as
    /// `postgres:16-alpine` or `ghcr.io/org/image:tag`.
    Pull(String),
    /// Build the image locally from a Dockerfile.
    ///
    /// The runtime calls `docker build` (or equivalent) with these
    /// parameters before starting the container.
    Build {
        /// Build context directory, relative to the manifest file.
        context: String,
        /// Dockerfile path within `context`.
        dockerfile: String,
        /// Build-time `--build-arg` key/value pairs.
        build_args: HashMap<String, String>,
        /// Optional multi-stage `--target` stage name.
        target: Option<String>,
        /// Tag applied to the resulting image (e.g.
        /// `lightshuttle/<project>_<resource>:dev`).
        tag: String,
    },
}

/// Host-to-container port binding resolved from the manifest.
///
/// Corresponds to the `-p` / `--publish` Docker flag. The manifest
/// supports three forms:
///
/// | Manifest form | Result |
/// |---|---|
/// | `8080` (short) | `container_port = 8080`, `host_port = 8080`, no address |
/// | `"8080:80"` | `host_port = 8080`, `container_port = 80` |
/// | `"127.0.0.1:8080:80"` | as above plus `host_address = Some("127.0.0.1")` |
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::PortBinding;
///
/// let binding = PortBinding {
///     container_port: 80,
///     host_address: Some("127.0.0.1".into()),
///     host_port: 8080,
/// };
/// assert_eq!(binding.host_port, 8080);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortBinding {
    /// Port exposed by the container.
    pub container_port: u16,
    /// Optional host interface to bind to. When `None` the runtime
    /// binds to all interfaces (`0.0.0.0`).
    pub host_address: Option<String>,
    /// Port published on the host. Mirrors `container_port` when the
    /// short integer form is used in the manifest.
    pub host_port: u16,
}

/// Volume or bind-mount mapping resolved from the manifest.
///
/// Covers the three forms supported by the manifest `volumes` list:
/// named volumes (`data:/var/lib/data`), host bind-mounts
/// (`./src:/app` or `/abs/path:/app`), and the implicit anonymous
/// volume injected for `postgres` and `redis` when no explicit volume
/// is declared.
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::{VolumeBinding, VolumeSource};
///
/// let named = VolumeBinding {
///     source: VolumeSource::Named("pgdata".into()),
///     target: "/var/lib/postgresql/data".into(),
/// };
/// let bind = VolumeBinding {
///     source: VolumeSource::HostPath("./src".into()),
///     target: "/app".into(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeBinding {
    /// Origin of the volume content.
    pub source: VolumeSource,
    /// Absolute path of the mount point inside the container.
    pub target: String,
}

/// Origin of the content mounted into the container.
///
/// Determines how the runtime creates the volume and whether it
/// survives container removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeSource {
    /// Bind-mount from a host path (starts with `.` or `/` in the
    /// manifest).
    ///
    /// The inner `String` is the path as written in the manifest,
    /// relative or absolute.
    HostPath(String),
    /// Named volume managed by the container runtime.
    ///
    /// The inner `String` is the volume name (no `.` or `/` prefix).
    /// Template-unsafe characters (`{`, `}`) are rejected during
    /// resolution.
    Named(String),
    /// Anonymous volume whose lifetime is tied to the container.
    ///
    /// Injected automatically for `postgres` and `redis` when the
    /// manifest sets `volume: true` or omits the field entirely.
    Anonymous,
}

/// Healthcheck resolved from the manifest, with duration strings
/// already parsed into [`std::time::Duration`] values.
///
/// For `postgres` resources the default test is
/// `["CMD", "pg_isready", "-U", <user>]`. For `redis` it is
/// `["CMD", "redis-cli", "ping"]`. Generic `container` and
/// `dockerfile` resources have no default: the manifest must provide
/// one explicitly if a healthcheck is needed.
///
/// Default timing values when the manifest omits them:
/// `interval = 5s`, `timeout = 3s`, `retries = 5`,
/// `start_period = 5s`.
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::HealthcheckSpec;
/// use std::time::Duration;
///
/// let hc = HealthcheckSpec {
///     test: vec!["CMD".into(), "pg_isready".into(), "-U".into(), "postgres".into()],
///     interval: Duration::from_secs(5),
///     timeout: Duration::from_secs(3),
///     retries: 5,
///     start_period: Duration::from_secs(5),
/// };
/// assert_eq!(hc.retries, 5);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthcheckSpec {
    /// Command the runtime runs to probe container health.
    ///
    /// The first element is typically `"CMD"` or `"CMD-SHELL"` as per
    /// the Docker healthcheck specification.
    pub test: Vec<String>,
    /// Time between consecutive health checks.
    pub interval: Duration,
    /// Maximum wall-clock time a single check invocation may take.
    pub timeout: Duration,
    /// Number of consecutive failures before the container is
    /// considered unhealthy.
    pub retries: u32,
    /// Grace period after container start before health checks begin.
    pub start_period: Duration,
}

/// Resolve a manifest resource declaration into a [`ResolvedResource`].
///
/// This is the single entry point into `lightshuttle-spec`. It
/// dispatches on `kind` and applies all v0 defaults:
///
/// - **Image**: falls back to the official Alpine image for the
///   declared version (e.g. `postgres:16-alpine`, `redis:7-alpine`).
/// - **Database name** (`postgres`): defaults to `resource_name`.
/// - **User** (`postgres`): defaults to `"postgres"`.
/// - **Password**: generated with a 24-character CSPRNG-backed
///   alphabet when absent from the manifest.
/// - **Ports**: uses the canonical default port for `postgres`
///   (5432) and `redis` (6379).
/// - **Healthcheck**: injects `pg_isready` / `redis-cli ping` when
///   the manifest omits a healthcheck for managed services.
///
/// The container name is always `<project>_<resource_name>` and also
/// serves as the DNS hostname inside the project network.
///
/// # Errors
///
/// Returns [`crate::SpecError::InvalidSpec`] when a port mapping,
/// volume string, or duration in the manifest is syntactically invalid.
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_manifest::{PostgresConfig, ResourceKind};
/// use lightshuttle_spec::from_resource;
///
/// let kind = ResourceKind::Postgres(PostgresConfig::default());
/// let resolved = from_resource("acme", "db", &kind).unwrap();
///
/// // Container name follows the `<project>_<resource>` convention.
/// assert_eq!(resolved.spec.name, "acme_db");
/// // A connection URL is always present for postgres.
/// assert!(resolved.outputs["url"].starts_with("postgres://"));
/// ```
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
        entrypoint: None,
        command: None,
        healthcheck,
        working_dir: None,
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
        entrypoint: None,
        command: Some(command),
        healthcheck,
        working_dir: None,
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
    let entrypoint = c.entrypoint.as_ref().map(parse_command);
    let command = c
        .command
        .as_ref()
        .map(parse_command)
        .filter(|cmd| !cmd.is_empty());
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
        entrypoint,
        command,
        healthcheck,
        working_dir: c.working_dir.clone(),
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
    let entrypoint = c.entrypoint.as_ref().map(parse_command);
    let command = c
        .command
        .as_ref()
        .map(parse_command)
        .filter(|cmd| !cmd.is_empty());
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
        entrypoint,
        command,
        healthcheck,
        working_dir: c.working_dir.clone(),
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
        if source.contains(['{', '}']) {
            return Err(SpecError::InvalidSpec(format!(
                "volume name `{source}` must not contain '{{' or '}}': unsafe in export templates"
            )));
        }
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
    use super::{
        VolumeSource, from_resource, generate_random_password, parse_command, parse_duration,
        parse_port_string, parse_volume_string,
    };
    use lightshuttle_manifest::Command;
    use std::time::Duration;

    #[test]
    fn parse_port_string_two_part() {
        let b = parse_port_string("8080:80").unwrap();
        assert_eq!(b.host_port, 8080);
        assert_eq!(b.container_port, 80);
        assert_eq!(b.host_address, None);
    }

    #[test]
    fn parse_port_string_three_part() {
        let b = parse_port_string("127.0.0.1:8080:80").unwrap();
        assert_eq!(b.host_port, 8080);
        assert_eq!(b.container_port, 80);
        assert_eq!(b.host_address.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn parse_port_string_single_part_is_error() {
        assert!(parse_port_string("80").is_err());
    }

    #[test]
    fn parse_port_string_non_numeric_is_error() {
        assert!(parse_port_string("abc:80").is_err());
    }

    #[test]
    fn parse_volume_string_named() {
        let b = parse_volume_string("data:/var/lib/data").unwrap();
        assert!(matches!(b.source, VolumeSource::Named(_)));
        assert_eq!(b.target, "/var/lib/data");
    }

    #[test]
    fn parse_volume_string_relative_host() {
        let b = parse_volume_string("./src:/app").unwrap();
        assert!(matches!(b.source, VolumeSource::HostPath(_)));
        assert_eq!(b.target, "/app");
    }

    #[test]
    fn parse_volume_string_absolute_host() {
        let b = parse_volume_string("/abs/path:/app").unwrap();
        assert!(matches!(b.source, VolumeSource::HostPath(_)));
        assert_eq!(b.target, "/app");
    }

    #[test]
    fn parse_volume_string_no_colon_is_error() {
        assert!(parse_volume_string("nodatahere").is_err());
    }

    #[test]
    fn parse_volume_string_braces_in_name_is_error() {
        assert!(parse_volume_string("my{vol}:/data").is_err());
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_milliseconds() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn parse_duration_unknown_unit_is_error() {
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn parse_duration_no_unit_is_error() {
        assert!(parse_duration("10").is_err());
    }

    #[test]
    fn parse_duration_no_digits_is_error() {
        assert!(parse_duration("s").is_err());
    }

    #[test]
    fn parse_command_empty_args_produces_empty_vec() {
        assert!(parse_command(&Command::Args(vec![])).is_empty());
    }

    #[test]
    fn parse_command_single_becomes_sh_c() {
        let v = parse_command(&Command::Single("echo hi".to_owned()));
        assert_eq!(v, vec!["sh", "-c", "echo hi"]);
    }

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

    #[test]
    fn entrypoint_resolves_to_argv_and_leaves_command_alone() {
        let yaml = r#"
project:
  name: app
resources:
  svc:
    dockerfile:
      context: .
      entrypoint: ["sh", "-c"]
      command: ["echo hi"]
"#;
        let manifest = lightshuttle_manifest::Manifest::parse(yaml).expect("manifest parses");
        let resolved =
            from_resource("app", "svc", &manifest.resources["svc"]).expect("resolution succeeds");
        assert_eq!(
            resolved.spec.entrypoint,
            Some(vec!["sh".to_owned(), "-c".to_owned()])
        );
        assert_eq!(
            resolved.spec.command,
            Some(vec!["echo hi".to_owned()]),
            "resolving an entrypoint must not disturb the command"
        );
    }

    #[test]
    fn absent_entrypoint_resolves_to_none() {
        let yaml = r"
project:
  name: app
resources:
  svc:
    dockerfile:
      context: .
";
        let manifest = lightshuttle_manifest::Manifest::parse(yaml).expect("manifest parses");
        let resolved =
            from_resource("app", "svc", &manifest.resources["svc"]).expect("resolution succeeds");
        assert_eq!(
            resolved.spec.entrypoint, None,
            "existing manifests must be unaffected"
        );
    }

    #[test]
    fn generated_resources_declare_no_entrypoint() {
        let yaml = r"
project:
  name: app
resources:
  cache:
    redis:
      version: '7'
  db:
    postgres:
      version: '16'
";
        let manifest = lightshuttle_manifest::Manifest::parse(yaml).expect("manifest parses");
        for name in ["cache", "db"] {
            let resolved =
                from_resource("app", name, &manifest.resources[name]).expect("resolution succeeds");
            assert_eq!(
                resolved.spec.entrypoint, None,
                "{name} must keep the image entrypoint"
            );
        }
        let cache = from_resource("app", "cache", &manifest.resources["cache"])
            .expect("resolution succeeds");
        assert_eq!(
            cache.spec.command,
            Some(vec!["redis-server".to_owned()]),
            "the redis command must be untouched"
        );
    }
}
