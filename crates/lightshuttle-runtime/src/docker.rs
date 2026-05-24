//! Docker container runtime backed by the `bollard` crate.

use std::collections::HashMap;
use std::pin::Pin;
use std::time::{Duration, Instant, SystemTime};

use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::{HealthConfig, HostConfig, PortBinding as BollardPortBinding};
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
        let options = CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        };
        let mut stream = self.client.create_image(Some(options), None, None);
        while let Some(event) = stream.next().await {
            event.map_err(|e| RuntimeError::ImagePull {
                image: image.to_owned(),
                source: e,
            })?;
        }
        Ok(())
    }
}

impl ContainerRuntime for DockerRuntime {
    async fn start(&self, spec: &ContainerSpec) -> Result<ContainerId> {
        let image_ref = match &spec.image {
            ImageSource::Pull(image) => {
                self.ensure_image(image).await?;
                image.clone()
            }
            ImageSource::Build { .. } => {
                return Err(RuntimeError::InvalidSpec(
                    "dockerfile builds are not yet implemented in this runtime; \
                     planned for a follow-up PR within v0.1.0"
                        .to_owned(),
                ));
            }
        };

        let host_config = build_host_config(&spec.ports, &spec.volumes);
        let exposed_ports = build_exposed_ports(&spec.ports);
        let env = build_env(&spec.env);
        let healthcheck = spec.healthcheck.as_ref().map(build_healthcheck);

        let config = Config {
            image: Some(image_ref),
            env: Some(env),
            cmd: spec.command.clone(),
            host_config: Some(host_config),
            exposed_ports: Some(exposed_ports),
            healthcheck,
            ..Default::default()
        };

        let create_options = CreateContainerOptions {
            name: spec.name.clone(),
            platform: None,
        };

        let created = self
            .client
            .create_container(Some(create_options), config)
            .await
            .map_err(RuntimeError::Start)?;

        self.client
            .start_container::<String>(&created.id, None)
            .await
            .map_err(RuntimeError::Start)?;

        Ok(ContainerId::new(created.id))
    }

    async fn stop(&self, id: &ContainerId, grace: Duration) -> Result<()> {
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let options = StopContainerOptions {
            t: grace.as_secs() as i64,
        };
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
        let options = LogsOptions::<String> {
            follow,
            stdout: true,
            stderr: true,
            timestamps: true,
            ..Default::default()
        };
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

#[allow(clippy::zero_sized_map_values)]
fn build_exposed_ports(ports: &[PortBinding]) -> HashMap<String, HashMap<(), ()>> {
    ports
        .iter()
        .map(|p| (format!("{}/tcp", p.container_port), HashMap::new()))
        .collect()
}

fn build_host_config(ports: &[PortBinding], volumes: &[VolumeBinding]) -> HostConfig {
    let port_bindings = ports
        .iter()
        .map(|p| {
            let bindings = vec![BollardPortBinding {
                host_ip: p.host_address.clone(),
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
