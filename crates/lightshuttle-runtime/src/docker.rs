//! Docker container runtime backed by the `bollard` crate.

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant, SystemTime};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerCreateBody, ContainerSummaryStateEnum, HealthConfig, HostConfig,
    PortBinding as BollardPortBinding,
};
use bollard::query_parameters::{
    BuildImageOptionsBuilder, CreateContainerOptionsBuilder, CreateImageOptionsBuilder,
    ListContainersOptionsBuilder, LogsOptionsBuilder, StartContainerOptions,
    StopContainerOptionsBuilder,
};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};

use crate::error::{Result, RuntimeError};
use crate::runtime::{
    ContainerId, ContainerRuntime, ContainerStatus, LogChunk, LogChunkStream, LogStream,
};
use crate::spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, VolumeBinding, VolumeSource,
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Docker container runtime backed by the `bollard` crate.
///
/// Connects to the local Docker daemon using the platform default
/// transport (Unix socket on Linux and macOS, named pipe on Windows).
pub struct DockerRuntime {
    client: Docker,
}

impl DockerRuntime {
    /// Connect to the local Docker daemon.
    pub fn connect() -> Result<Self> {
        let client = Docker::connect_with_local_defaults().map_err(RuntimeError::Connect)?;
        Ok(Self { client })
    }

    /// Wrap an existing `bollard::Docker` client. Useful for tests that
    /// supply a pre-configured client (custom transport, mock, etc.).
    #[must_use]
    pub fn from_client(client: Docker) -> Self {
        Self { client }
    }

    async fn ensure_image(&self, image: &str) -> Result<()> {
        let (from_image, tag) = split_image_ref(image);
        let options = CreateImageOptionsBuilder::default()
            .from_image(from_image)
            .tag(tag)
            .build();
        let mut stream = self.client.create_image(Some(options), None, None);
        while let Some(event) = stream.next().await {
            event.map_err(|e| RuntimeError::ImagePull {
                image: image.to_owned(),
                source: e,
            })?;
        }
        Ok(())
    }

    /// List every container labelled with `lightshuttle.project=<project>`,
    /// including stopped ones. Used by the CLI to implement `ps` and
    /// `down` without relying on in-memory state.
    pub async fn list_managed(&self, project: &str) -> Result<Vec<ManagedContainer>> {
        let label_filter = format!("{LABEL_PROJECT}={project}");
        let mut filters: HashMap<String, Vec<String>> = HashMap::new();
        filters.insert("label".to_owned(), vec![label_filter]);
        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();
        let summaries = self
            .client
            .list_containers(Some(options))
            .await
            .map_err(|source| RuntimeError::Inspect {
                id: format!("project={project}"),
                source,
            })?;

        let mut out = Vec::with_capacity(summaries.len());
        for summary in summaries {
            let Some(id) = summary.id else { continue };
            let resource = summary
                .labels
                .as_ref()
                .and_then(|labels| labels.get(LABEL_RESOURCE))
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned());
            let status = parse_summary_state(summary.state.as_ref());
            out.push(ManagedContainer {
                id: ContainerId::new(id),
                resource,
                status,
            });
        }
        out.sort_by(|a, b| a.resource.cmp(&b.resource));
        Ok(out)
    }

    async fn build_image(
        &self,
        context: &str,
        dockerfile: &str,
        build_args: &HashMap<String, String>,
        target: Option<&str>,
        tag: &str,
    ) -> Result<()> {
        let context_owned = context.to_owned();
        let tar_bytes =
            tokio::task::spawn_blocking(move || build_tar_archive(Path::new(&context_owned)))
                .await
                .map_err(|join_err| {
                    RuntimeError::InvalidSpec(format!("tar build task panicked: {join_err}"))
                })?
                .map_err(|io_err| {
                    RuntimeError::InvalidSpec(format!("failed to build tar archive: {io_err}"))
                })?;

        let options = BuildImageOptionsBuilder::default()
            .dockerfile(dockerfile)
            .t(tag)
            .rm(true)
            .buildargs(build_args)
            .target(target.unwrap_or(""))
            .build();

        let mut stream = self.client.build_image(
            options,
            None,
            Some(bollard::body_full(Bytes::from(tar_bytes))),
        );
        while let Some(event) = stream.next().await {
            event.map_err(RuntimeError::Build)?;
        }
        Ok(())
    }
}

/// Build a tar archive from `context`, respecting `.dockerignore`
/// patterns found within. Returns the raw tar bytes (uncompressed).
fn build_tar_archive(context: &Path) -> std::io::Result<Vec<u8>> {
    use ignore::WalkBuilder;

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut buf);
        builder.follow_symlinks(false);

        let walker = WalkBuilder::new(context)
            .add_custom_ignore_filename(".dockerignore")
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .hidden(false)
            .build();

        for entry in walker {
            let entry = entry.map_err(|e| std::io::Error::other(format!("walk error: {e}")))?;
            let path = entry.path();
            let relative = match path.strip_prefix(context) {
                Ok(p) if !p.as_os_str().is_empty() => p,
                _ => continue,
            };
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                builder.append_dir(relative, path)?;
            } else if file_type.is_file() {
                let mut file = std::fs::File::open(path)?;
                builder.append_file(relative, &mut file)?;
            }
        }
        builder.finish()?;
    }
    Ok(buf)
}

impl ContainerRuntime for DockerRuntime {
    async fn start(&self, spec: &ContainerSpec) -> Result<ContainerId> {
        let image_ref = match &spec.image {
            ImageSource::Pull(image) => {
                self.ensure_image(image).await?;
                image.clone()
            }
            ImageSource::Build {
                context,
                dockerfile,
                build_args,
                target,
                tag,
            } => {
                self.build_image(context, dockerfile, build_args, target.as_deref(), tag)
                    .await?;
                tag.clone()
            }
        };

        let host_config = build_host_config(&spec.ports, &spec.volumes);
        let exposed_ports = build_exposed_ports(&spec.ports);
        let env = build_env(&spec.env);
        let healthcheck = spec.healthcheck.as_ref().map(build_healthcheck);
        let labels = build_labels(&spec.project, &spec.resource);

        let config = ContainerCreateBody {
            image: Some(image_ref),
            env: Some(env),
            cmd: spec.command.clone(),
            host_config: Some(host_config),
            exposed_ports: Some(exposed_ports),
            healthcheck,
            labels: Some(labels),
            ..Default::default()
        };

        let create_options = CreateContainerOptionsBuilder::default()
            .name(&spec.name)
            .build();

        let created = self
            .client
            .create_container(Some(create_options), config)
            .await
            .map_err(RuntimeError::Start)?;

        self.client
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
            .map_err(RuntimeError::Start)?;

        Ok(ContainerId::new(created.id))
    }

    async fn stop(&self, id: &ContainerId, grace: Duration) -> Result<()> {
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let options = StopContainerOptionsBuilder::default()
            .t(grace.as_secs() as i32)
            .build();
        match self.client.stop_container(id.as_str(), Some(options)).await {
            Ok(())
            | Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304 | 404,
                ..
            }) => Ok(()),
            Err(e) => Err(RuntimeError::Stop {
                id: id.to_string(),
                source: e,
            }),
        }
    }

    async fn inspect(&self, id: &ContainerId) -> Result<ContainerStatus> {
        let info = self
            .client
            .inspect_container(id.as_str(), None)
            .await
            .map_err(|e| match e {
                bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                } => RuntimeError::NotFound(id.to_string()),
                other => RuntimeError::Inspect {
                    id: id.to_string(),
                    source: other,
                },
            })?;

        let state = info.state.as_ref();
        let Some(state) = state else {
            return Ok(ContainerStatus::Starting);
        };

        if matches!(state.running, Some(true)) {
            if let Some(health) = &state.health {
                return Ok(match health.status {
                    Some(bollard::models::HealthStatusEnum::HEALTHY) => ContainerStatus::Healthy,
                    Some(bollard::models::HealthStatusEnum::UNHEALTHY) => {
                        ContainerStatus::Unhealthy
                    }
                    _ => ContainerStatus::Running,
                });
            }
            return Ok(ContainerStatus::Running);
        }

        if matches!(state.dead, Some(true))
            || state.status == Some(bollard::models::ContainerStateStatusEnum::EXITED)
        {
            #[allow(clippy::cast_possible_truncation)]
            let exit_code = state.exit_code.map(|c| c as i32);
            return Ok(ContainerStatus::Stopped { exit_code });
        }

        Ok(ContainerStatus::Starting)
    }

    async fn wait_healthy(&self, id: &ContainerId, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.inspect(id).await? {
                ContainerStatus::Healthy | ContainerStatus::Running => return Ok(()),
                ContainerStatus::Unhealthy => {
                    if Instant::now() >= deadline {
                        return Err(RuntimeError::Timeout {
                            operation: "wait_healthy",
                            after: timeout,
                        });
                    }
                }
                ContainerStatus::Starting => {}
                ContainerStatus::Stopped { exit_code } => {
                    return Err(RuntimeError::InvalidSpec(format!(
                        "container `{id}` exited with code {exit_code:?} before becoming healthy"
                    )));
                }
            }
            if Instant::now() >= deadline {
                return Err(RuntimeError::Timeout {
                    operation: "wait_healthy",
                    after: timeout,
                });
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn logs(&self, id: &ContainerId, follow: bool) -> Result<LogChunkStream> {
        let options = LogsOptionsBuilder::default()
            .follow(follow)
            .stdout(true)
            .stderr(true)
            .timestamps(true)
            .build();
        let stream = self.client.logs(id.as_str(), Some(options));
        let mapped: Pin<Box<dyn Stream<Item = Result<LogChunk>> + Send>> =
            Box::pin(stream.map(map_log_item));
        Ok(mapped)
    }
}

fn split_image_ref(image: &str) -> (&str, &str) {
    image.split_once(':').unwrap_or((image, "latest"))
}

fn build_env(env: &HashMap<String, String>) -> Vec<String> {
    env.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

fn build_labels(project: &str, resource: &str) -> HashMap<String, String> {
    let mut labels = HashMap::with_capacity(2);
    labels.insert(LABEL_PROJECT.to_owned(), project.to_owned());
    labels.insert(LABEL_RESOURCE.to_owned(), resource.to_owned());
    labels
}

/// Docker label key set on every container managed by LightShuttle to
/// carry the manifest project name.
pub const LABEL_PROJECT: &str = "lightshuttle.project";

/// Docker label key set on every container to carry the manifest
/// resource name.
pub const LABEL_RESOURCE: &str = "lightshuttle.resource";

/// One entry returned by [`DockerRuntime::list_managed`].
#[derive(Debug, Clone)]
pub struct ManagedContainer {
    /// Container identifier.
    pub id: ContainerId,
    /// Resource name as declared in the manifest.
    pub resource: String,
    /// Current lifecycle status.
    pub status: ContainerStatus,
}

fn parse_summary_state(state: Option<&ContainerSummaryStateEnum>) -> ContainerStatus {
    match state {
        Some(ContainerSummaryStateEnum::RUNNING) => ContainerStatus::Running,
        Some(ContainerSummaryStateEnum::EXITED | ContainerSummaryStateEnum::DEAD) => {
            ContainerStatus::Stopped { exit_code: None }
        }
        _ => ContainerStatus::Starting,
    }
}

fn build_exposed_ports(ports: &[PortBinding]) -> Vec<String> {
    ports
        .iter()
        .map(|p| format!("{}/tcp", p.container_port))
        .collect()
}

/// Default host bind address for published ports.
///
/// Loopback by default so a dev machine never exposes managed services
/// (PostgreSQL, Redis, application ports) to the wider network. A
/// manifest that needs a broader bind must request it explicitly via
/// the `address:host:container` port mapping form.
const DEFAULT_HOST_BIND_ADDRESS: &str = "127.0.0.1";

fn build_host_config(ports: &[PortBinding], volumes: &[VolumeBinding]) -> HostConfig {
    let port_bindings = ports
        .iter()
        .map(|p| {
            let host_ip = p
                .host_address
                .clone()
                .unwrap_or_else(|| DEFAULT_HOST_BIND_ADDRESS.to_owned());
            let bindings = vec![BollardPortBinding {
                host_ip: Some(host_ip),
                host_port: Some(p.host_port.to_string()),
            }];
            (format!("{}/tcp", p.container_port), Some(bindings))
        })
        .collect::<HashMap<_, _>>();

    let binds: Vec<String> = volumes
        .iter()
        .filter_map(|v| match &v.source {
            VolumeSource::HostPath(path) => Some(format!("{path}:{}", v.target)),
            VolumeSource::Named(name) => Some(format!("{name}:{}", v.target)),
            VolumeSource::Anonymous => None,
        })
        .collect();

    HostConfig {
        port_bindings: Some(port_bindings),
        binds: if binds.is_empty() { None } else { Some(binds) },
        ..Default::default()
    }
}

fn build_healthcheck(hc: &HealthcheckSpec) -> HealthConfig {
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    HealthConfig {
        test: Some(hc.test.clone()),
        interval: Some(hc.interval.as_nanos() as i64),
        timeout: Some(hc.timeout.as_nanos() as i64),
        retries: Some(i64::from(hc.retries)),
        start_period: Some(hc.start_period.as_nanos() as i64),
        ..Default::default()
    }
}

fn map_log_item(item: std::result::Result<LogOutput, bollard::errors::Error>) -> Result<LogChunk> {
    match item {
        Ok(LogOutput::StdErr { message }) => Ok(LogChunk {
            stream: LogStream::Stderr,
            timestamp: SystemTime::now(),
            bytes: message.to_vec(),
        }),
        Ok(
            LogOutput::StdOut { message }
            | LogOutput::Console { message }
            | LogOutput::StdIn { message },
        ) => Ok(LogChunk {
            stream: LogStream::Stdout,
            timestamp: SystemTime::now(),
            bytes: message.to_vec(),
        }),
        Err(e) => Err(RuntimeError::LogStream(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::{PortBinding, build_host_config};

    fn host_ip_for(ports: &[PortBinding], key: &str) -> Option<String> {
        let config = build_host_config(ports, &[]);
        config
            .port_bindings
            .and_then(|map| map.get(key).cloned())
            .flatten()
            .and_then(|bindings| bindings.into_iter().next())
            .and_then(|binding| binding.host_ip)
    }

    #[test]
    fn unspecified_address_binds_to_loopback() {
        let ports = vec![PortBinding {
            container_port: 5432,
            host_address: None,
            host_port: 5432,
        }];
        assert_eq!(
            host_ip_for(&ports, "5432/tcp").as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn explicit_address_is_preserved() {
        let ports = vec![PortBinding {
            container_port: 80,
            host_address: Some("0.0.0.0".to_owned()),
            host_port: 8080,
        }];
        assert_eq!(host_ip_for(&ports, "80/tcp").as_deref(), Some("0.0.0.0"));
    }
}
