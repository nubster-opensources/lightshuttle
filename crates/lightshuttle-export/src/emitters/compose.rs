//! Docker Compose emitter: renders an [`ExportModel`] into a single
//! `docker-compose.yml`.
//!
//! The emitted file uses the Compose v3 schema. Port bindings default to the
//! loopback address so the stack keeps the same not-exposed-by-default posture
//! as `lightshuttle up`. Named volumes are collected into the top-level
//! `volumes:` block so Compose can manage their lifecycle.

use std::collections::BTreeMap;
use std::time::Duration;

use indexmap::IndexMap;
use lightshuttle_spec::{ContainerSpec, ImageSource, PortBinding, VolumeBinding, VolumeSource};
use serde::Serialize;

use crate::emit::Emitter;
use crate::error::{ExportError, Result};
use crate::model::{ExportModel, ExportService, Target};
use crate::resolve::enabled_for;

/// Loopback address used when a port declares no explicit host bind, so
/// the exported stack keeps the same not-exposed-by-default posture as
/// `lightshuttle up`.
const DEFAULT_HOST_BIND_ADDRESS: &str = "127.0.0.1";

/// Emits a single `docker-compose.yml` from the export model.
///
/// Each enabled service in the [`crate::ExportModel`] becomes one entry in the
/// Compose `services:` block. Ports default to `127.0.0.1` as the host bind
/// address. Named volumes are collected into the top-level `volumes:` block.
/// Dependencies with a healthcheck use the `service_healthy` condition;
/// dependencies without one use `service_started`.
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_export::{lower, ComposeEmitter, Emitter};
/// use lightshuttle_manifest::Manifest;
///
/// # fn main() -> lightshuttle_export::Result<()> {
/// let manifest: Manifest = todo!("parse from YAML");
/// let model = lower(&manifest)?;
/// let artifacts = ComposeEmitter.emit(&model)?;
/// // artifacts.files[0].path == "docker-compose.yml"
/// # Ok(())
/// # }
/// ```
pub struct ComposeEmitter;

impl Emitter for ComposeEmitter {
    fn target(&self) -> Target {
        Target::Compose
    }

    fn emit(&self, model: &ExportModel) -> Result<crate::ExportArtifacts> {
        let file = build_compose(model);
        let yaml = serde_norway::to_string(&file).map_err(|e| ExportError::Unsupported {
            resource: "<compose>".to_owned(),
            target: "compose",
            reason: format!("failed to serialise compose file: {e}"),
        })?;
        let mut artifacts = crate::ExportArtifacts::new();
        artifacts.push("docker-compose.yml", yaml);
        Ok(artifacts)
    }
}

/// Typed `docker-compose` document.
#[derive(Debug, Serialize)]
struct ComposeFile {
    services: IndexMap<String, ComposeService>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    volumes: BTreeMap<String, ComposeVolumeDef>,
}

#[derive(Debug, Serialize, Default)]
struct ComposeService {
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<ComposeBuild>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ports: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    volumes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entrypoint: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    healthcheck: Option<ComposeHealthcheck>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    depends_on: IndexMap<String, ComposeDependency>,
}

#[derive(Debug, Serialize)]
struct ComposeBuild {
    context: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dockerfile: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    args: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComposeDependency {
    condition: &'static str,
}

#[derive(Debug, Serialize)]
struct ComposeHealthcheck {
    test: Vec<String>,
    interval: String,
    timeout: String,
    retries: u32,
    start_period: String,
}

/// Named-volume definition. Rendered as `name: {}` while no options are
/// set; the optional `driver` keeps it open for future overrides.
#[derive(Debug, Serialize, Default)]
struct ComposeVolumeDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    driver: Option<String>,
}

fn build_compose(model: &ExportModel) -> ComposeFile {
    let mut services = IndexMap::new();
    let mut volumes: BTreeMap<String, ComposeVolumeDef> = BTreeMap::new();

    for service in &model.services {
        if !enabled_for(
            Target::Compose,
            &service.spec.resource,
            model.export.as_ref(),
        ) {
            continue;
        }
        collect_named_volumes(&service.spec.volumes, &mut volumes);
        services.insert(
            service.spec.resource.clone(),
            compose_service(service, model),
        );
    }

    ComposeFile { services, volumes }
}

fn compose_service(service: &ExportService, model: &ExportModel) -> ComposeService {
    let spec = &service.spec;
    let (image, build) = image_or_build(spec);

    ComposeService {
        image,
        build,
        ports: spec.ports.iter().map(port_string).collect(),
        environment: spec
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        volumes: spec.volumes.iter().map(volume_string).collect(),
        entrypoint: spec.entrypoint.clone(),
        command: spec.command.clone(),
        healthcheck: spec.healthcheck.as_ref().map(|hc| ComposeHealthcheck {
            test: hc.test.clone(),
            interval: duration_str(hc.interval),
            timeout: duration_str(hc.timeout),
            retries: hc.retries,
            start_period: duration_str(hc.start_period),
        }),
        depends_on: service
            .depends_on
            .iter()
            .map(|dep| {
                let has_healthcheck = model
                    .services
                    .iter()
                    .any(|s| s.spec.resource == *dep && s.spec.healthcheck.is_some());
                (
                    dep.clone(),
                    ComposeDependency {
                        condition: if has_healthcheck {
                            "service_healthy"
                        } else {
                            "service_started"
                        },
                    },
                )
            })
            .collect(),
    }
}

fn image_or_build(spec: &ContainerSpec) -> (Option<String>, Option<ComposeBuild>) {
    match &spec.image {
        ImageSource::Pull(image) => (Some(image.clone()), None),
        ImageSource::Build {
            context,
            dockerfile,
            build_args,
            target,
            tag,
        } => {
            let build = ComposeBuild {
                context: context.clone(),
                dockerfile: Some(dockerfile.clone()),
                args: build_args
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                target: target.clone(),
            };
            (Some(tag.clone()), Some(build))
        }
    }
}

fn port_string(port: &PortBinding) -> String {
    let host = port
        .host_address
        .as_deref()
        .unwrap_or(DEFAULT_HOST_BIND_ADDRESS);
    format!("{host}:{}:{}", port.host_port, port.container_port)
}

fn volume_string(volume: &VolumeBinding) -> String {
    match &volume.source {
        VolumeSource::HostPath(path) => format!("{path}:{}", volume.target),
        VolumeSource::Named(name) => format!("{name}:{}", volume.target),
        VolumeSource::Anonymous => volume.target.clone(),
    }
}

fn collect_named_volumes(volumes: &[VolumeBinding], out: &mut BTreeMap<String, ComposeVolumeDef>) {
    for volume in volumes {
        if let VolumeSource::Named(name) = &volume.source {
            out.entry(name.clone()).or_default();
        }
    }
}

/// Render a duration as a Go-style compose duration string.
fn duration_str(d: Duration) -> String {
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    match (secs, millis) {
        (s, 0) => format!("{s}s"),
        (0, ms) => format!("{ms}ms"),
        (s, ms) => format!("{s}s{ms}ms"),
    }
}
