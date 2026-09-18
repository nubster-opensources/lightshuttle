//! Docker Compose emitter: renders an [`ExportModel`] into a single
//! `docker-compose.yml`.
//!
//! The emitted file uses the Compose v3 schema. Port bindings default to the
//! loopback address so the stack keeps the same not-exposed-by-default posture
//! as `lightshuttle up`. Named volumes are collected into the top-level
//! `volumes:` block so Compose can manage their lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use indexmap::IndexMap;
use lightshuttle_spec::{ContainerSpec, ImageSource, PortBinding, VolumeBinding, VolumeSource};
use serde::Serialize;

use crate::deployment::{RenderedModel, RenderedService, render_for_target};
use crate::emit::Emitter;
use crate::error::{DisabledDependency, ExportError, Result};
use crate::model::{ExportModel, Target};
use crate::resolve::{
    compose_arguments, compose_env, compose_variable_name, enabled_for, is_secret_key,
};

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
        let rendered = render_for_target(model, Target::Compose)?;
        ensure_no_disabled_dependencies(model, &rendered)?;
        ensure_no_variable_collisions(&rendered)?;
        let file = build_compose(model, &rendered.services);
        let yaml = serde_norway::to_string(&file).map_err(|e| ExportError::Unsupported {
            resource: "<compose>".to_owned(),
            target: "compose",
            reason: format!("failed to serialise compose file: {e}"),
        })?;
        let mut artifacts = crate::ExportArtifacts::new();
        artifacts.push("docker-compose.yml", yaml);
        artifacts.ensure_unique_paths()?;
        Ok(artifacts)
    }
}

/// Refuses an export where a service Compose emits depends on a resource
/// this export excludes.
///
/// Compose is the only target that emits `depends_on`, so it is the only
/// one where excluding a resource can leave a reference to a service the
/// file never defines, which `docker compose config` rejects. Whether a
/// dependency is excluded is asked of [`crate::resolve::enabled_for`] rather than deduced
/// from its absence among the rendered services: a dependency that names no
/// manifest resource at all is not disabled, it is unknown, and
/// `lightshuttle_manifest::Manifest::validate` is what reports that.
///
/// # Errors
///
/// Returns [`ExportError::DisabledDependencies`] carrying every dangling
/// edge, sorted by service then dependency, so one export reports them all.
fn ensure_no_disabled_dependencies(model: &ExportModel, rendered: &RenderedModel) -> Result<()> {
    let mut dependencies: BTreeSet<DisabledDependency> = BTreeSet::new();

    for service in &rendered.services {
        for dependency in &service.depends_on {
            if !enabled_for(Target::Compose, dependency, model.export.as_ref()) {
                dependencies.insert(DisabledDependency::new(
                    service.spec.resource.as_str(),
                    dependency.as_str(),
                ));
            }
        }
    }

    if dependencies.is_empty() {
        return Ok(());
    }
    Err(ExportError::DisabledDependencies {
        target: "compose",
        dependencies: dependencies.into_iter().collect(),
    })
}

/// Refuses an export where two distinct sources would produce the same
/// Compose interpolation variable name.
///
/// [`compose_variable_name`] is not injective (a dashed and an underscored
/// resource name can normalise alike, and so can two different `(resource,
/// key)` splits), so the guarantee has to come from checking the names this
/// export actually produces rather than from the normalisation itself. Every
/// secret environment key of every service is one source; the deployment
/// placeholder variables collected by `crate::deployment` (see #308) are
/// another, since they interpolate through the same unqualified `${NAME}`
/// syntax and can collide with a qualified name just as easily.
///
/// # Errors
///
/// Returns [`ExportError::Unsupported`] naming every source that produced
/// the colliding variable. A source is a `(resource, key)` pair rather than a
/// resource, because two secret keys of one resource (`db_password` and
/// `DB_PASSWORD`) normalise alike just as two resources can.
fn ensure_no_variable_collisions(rendered: &RenderedModel) -> Result<()> {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for service in &rendered.services {
        let spec = &service.spec;
        for key in spec.env.keys() {
            if is_secret_key(spec, key) {
                let name = compose_variable_name(&spec.resource, key);
                owners
                    .entry(name)
                    .or_default()
                    .insert(format!("{} (key `{key}`)", spec.resource));
            }
        }
    }
    for variable in &rendered.variables {
        owners
            .entry(variable.clone())
            .or_default()
            .insert(format!("deployment variable `{variable}`"));
    }

    for (name, resources) in owners {
        if resources.len() > 1 {
            let resources: Vec<String> = resources.into_iter().collect();
            return Err(ExportError::Unsupported {
                resource: resources.join(", "),
                target: "compose",
                reason: format!(
                    "would all reference the Compose variable `{name}`, so one would silently override another's value"
                ),
            });
        }
    }
    Ok(())
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
    working_dir: Option<String>,
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

fn build_compose(model: &ExportModel, rendered: &[RenderedService]) -> ComposeFile {
    let mut services = IndexMap::new();
    let mut volumes: BTreeMap<String, ComposeVolumeDef> = BTreeMap::new();

    for service in rendered {
        collect_named_volumes(&service.spec.volumes, &mut volumes);
        services.insert(
            service.spec.resource.clone(),
            compose_service(service, model),
        );
    }

    ComposeFile { services, volumes }
}

fn compose_service(service: &RenderedService, model: &ExportModel) -> ComposeService {
    let spec = &service.spec;
    let (image, build) = image_or_build(spec);

    ComposeService {
        image,
        build,
        ports: spec.ports.iter().map(port_string).collect(),
        environment: compose_env(spec),
        volumes: spec.volumes.iter().map(volume_string).collect(),
        working_dir: spec.working_dir.clone(),
        entrypoint: spec
            .entrypoint
            .as_ref()
            .map(|arguments| compose_arguments(&spec.resource, arguments)),
        command: spec
            .command
            .as_ref()
            .map(|arguments| compose_arguments(&spec.resource, arguments)),
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
