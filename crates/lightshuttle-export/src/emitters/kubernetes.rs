//! Kubernetes emitter: renders an [`ExportModel`] into plain manifests,
//! one multi-document file per resource plus a namespace file.

use std::collections::BTreeMap;
use std::time::Duration;

use lightshuttle_manifest::ImagePullPolicy;
use lightshuttle_spec::{
    ContainerSpec, HealthcheckSpec, ImageSource, PortBinding, VolumeBinding, VolumeSource,
};
use serde::Serialize;

use crate::emit::Emitter;
use crate::error::Result;
use crate::model::{ExportModel, ExportService, Target};
use crate::resolve::{
    SECRET_MARKERS, dns_name, enabled_for, image_pull_policy_for, namespace_for, replicas_for,
};

/// Emits plain Kubernetes manifests from the export model.
pub struct KubernetesEmitter;

impl Emitter for KubernetesEmitter {
    fn target(&self) -> Target {
        Target::Kubernetes
    }

    fn emit(&self, model: &ExportModel) -> Result<crate::ExportArtifacts> {
        let namespace = namespace_for(&model.project.name, model.export.as_ref());
        let mut artifacts = crate::ExportArtifacts::new();
        artifacts.push("namespace.yaml", namespace_doc(&namespace)?);

        for service in &model.services {
            if !enabled_for(
                Target::Kubernetes,
                &service.spec.resource,
                model.export.as_ref(),
            ) {
                continue;
            }
            let docs = resource_docs(service, model, &namespace)?;
            artifacts.push(format!("{}.yaml", dns_name(&service.spec.resource)), docs);
        }

        Ok(artifacts)
    }
}

fn namespace_doc(namespace: &str) -> Result<String> {
    let ns = Namespace {
        api_version: "v1",
        kind: "Namespace",
        metadata: NameOnly {
            name: namespace.to_owned(),
        },
    };
    to_yaml(&ns)
}

fn resource_docs(service: &ExportService, model: &ExportModel, namespace: &str) -> Result<String> {
    let spec = &service.spec;
    let name = dns_name(&spec.resource);
    let labels = labels(&name);
    let (config_env, secret_env) = split_env(&spec.env);

    let mut docs: Vec<String> = Vec::new();

    docs.push(to_yaml(&deployment(
        spec, model, namespace, &name, &labels,
    ))?);
    if !spec.ports.is_empty() {
        docs.push(to_yaml(&service_object(spec, namespace, &name, &labels))?);
    }

    if !config_env.is_empty() {
        docs.push(to_yaml(&ConfigMap {
            api_version: "v1",
            kind: "ConfigMap",
            metadata: meta(&format!("{name}-config"), namespace, &labels),
            data: config_env,
        })?);
    }
    if !secret_env.is_empty() {
        docs.push(to_yaml(&Secret {
            api_version: "v1",
            kind: "Secret",
            metadata: meta(&format!("{name}-secret"), namespace, &labels),
            string_data: secret_env,
        })?);
    }
    for volume in &spec.volumes {
        if let VolumeSource::Named(vol) = &volume.source {
            docs.push(to_yaml(&pvc(&name, &dns_name(vol), namespace, &labels))?);
        }
    }

    Ok(docs.join("---\n"))
}

fn deployment(
    spec: &ContainerSpec,
    model: &ExportModel,
    namespace: &str,
    name: &str,
    labels: &BTreeMap<String, String>,
) -> Deployment {
    let replicas = replicas_for(Target::Kubernetes, &spec.resource, model.export.as_ref());
    let pull_policy = image_pull_policy_for(&spec.resource, model.export.as_ref());

    let mut env_from: Vec<EnvFromSource> = Vec::new();
    let (config_env, secret_env) = split_env(&spec.env);
    if !config_env.is_empty() {
        env_from.push(EnvFromSource::config(format!("{name}-config")));
    }
    if !secret_env.is_empty() {
        env_from.push(EnvFromSource::secret(format!("{name}-secret")));
    }

    let mut mounts: Vec<VolumeMount> = Vec::new();
    let mut volumes: Vec<PodVolume> = Vec::new();
    for (idx, volume) in spec.volumes.iter().enumerate() {
        let (vol_name, source) = pod_volume(name, idx, volume);
        mounts.push(VolumeMount {
            name: vol_name.clone(),
            mount_path: volume.target.clone(),
        });
        volumes.push(PodVolume {
            name: vol_name,
            source,
        });
    }

    let probe = spec.healthcheck.as_ref().map(probe);

    Deployment {
        api_version: "apps/v1",
        kind: "Deployment",
        metadata: meta(name, namespace, labels),
        spec: DeploymentSpec {
            replicas,
            selector: Selector {
                match_labels: labels.clone(),
            },
            template: PodTemplate {
                metadata: TemplateMeta {
                    labels: labels.clone(),
                },
                spec: PodSpec {
                    containers: vec![Container {
                        name: name.to_owned(),
                        image: image_ref(&spec.image),
                        image_pull_policy: pull_policy_str(pull_policy).to_owned(),
                        ports: spec.ports.iter().map(container_port).collect(),
                        env_from,
                        volume_mounts: mounts,
                        command: spec.command.clone(),
                        readiness_probe: probe.clone(),
                        liveness_probe: probe,
                    }],
                    volumes,
                },
            },
        },
    }
}

fn service_object(
    spec: &ContainerSpec,
    namespace: &str,
    name: &str,
    labels: &BTreeMap<String, String>,
) -> Service {
    Service {
        api_version: "v1",
        kind: "Service",
        metadata: meta(name, namespace, labels),
        spec: ServiceSpec {
            selector: labels.clone(),
            ports: spec
                .ports
                .iter()
                .map(|p| ServicePort {
                    port: p.container_port,
                    target_port: p.container_port,
                })
                .collect(),
        },
    }
}

fn pvc(name: &str, volume: &str, namespace: &str, labels: &BTreeMap<String, String>) -> Pvc {
    Pvc {
        api_version: "v1",
        kind: "PersistentVolumeClaim",
        metadata: meta(&format!("{name}-{volume}"), namespace, labels),
        spec: PvcSpec {
            access_modes: vec!["ReadWriteOnce".to_owned()],
            resources: PvcResources {
                requests: BTreeMap::from([("storage".to_owned(), "1Gi".to_owned())]),
            },
        },
    }
}

/// Build the pod volume name and source for `volume`.
fn pod_volume(resource: &str, idx: usize, volume: &VolumeBinding) -> (String, PodVolumeSource) {
    match &volume.source {
        VolumeSource::Named(vol) => {
            let vol = dns_name(vol);
            let claim = format!("{resource}-{vol}");
            (vol, PodVolumeSource::Pvc(PvcRef::new(claim)))
        }
        VolumeSource::HostPath(path) => (
            format!("{resource}-host-{idx}"),
            PodVolumeSource::HostPath(HostPathSource {
                host_path: HostPathInner { path: path.clone() },
            }),
        ),
        VolumeSource::Anonymous => (
            format!("{resource}-data-{idx}"),
            PodVolumeSource::EmptyDir(EmptyDir {
                empty_dir: EmptyDirInner {},
            }),
        ),
    }
}

/// Split env into (config, secret) by case-insensitive key marker.
fn split_env(
    env: &std::collections::HashMap<String, String>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut config = BTreeMap::new();
    let mut secret = BTreeMap::new();
    for (key, value) in env {
        let upper = key.to_ascii_uppercase();
        if SECRET_MARKERS.iter().any(|m| upper.contains(m)) {
            secret.insert(key.clone(), value.clone());
        } else {
            config.insert(key.clone(), value.clone());
        }
    }
    (config, secret)
}

fn probe(hc: &HealthcheckSpec) -> Probe {
    let command = match hc.test.first().map(String::as_str) {
        Some("CMD") => hc.test[1..].to_vec(),
        Some("CMD-SHELL") => vec!["sh".to_owned(), "-c".to_owned(), hc.test[1..].join(" ")],
        _ => hc.test.clone(),
    };
    Probe {
        exec: ExecAction { command },
        period_seconds: secs(hc.interval),
        timeout_seconds: secs(hc.timeout),
        failure_threshold: hc.retries,
        initial_delay_seconds: secs(hc.start_period),
    }
}

fn container_port(port: &PortBinding) -> ContainerPort {
    ContainerPort {
        container_port: port.container_port,
    }
}

fn image_ref(image: &ImageSource) -> String {
    match image {
        ImageSource::Pull(img) => img.clone(),
        ImageSource::Build { tag, .. } => tag.clone(),
    }
}

fn pull_policy_str(policy: ImagePullPolicy) -> &'static str {
    match policy {
        ImagePullPolicy::Always => "Always",
        ImagePullPolicy::IfNotPresent => "IfNotPresent",
        ImagePullPolicy::Never => "Never",
    }
}

fn labels(name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("app".to_owned(), name.to_owned())])
}

fn meta(name: &str, namespace: &str, labels: &BTreeMap<String, String>) -> Meta {
    Meta {
        name: name.to_owned(),
        namespace: namespace.to_owned(),
        labels: labels.clone(),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn secs(d: Duration) -> u32 {
    d.as_secs().min(u64::from(u32::MAX)) as u32
}

fn to_yaml<T: Serialize>(value: &T) -> Result<String> {
    serde_norway::to_string(value).map_err(|e| crate::ExportError::Unsupported {
        resource: "<kubernetes>".to_owned(),
        target: "kubernetes",
        reason: format!("failed to serialise manifest: {e}"),
    })
}

// --- Typed Kubernetes objects -------------------------------------------

#[derive(Serialize)]
struct NameOnly {
    name: String,
}

#[derive(Serialize)]
struct Meta {
    name: String,
    namespace: String,
    labels: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Namespace {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: NameOnly,
}

#[derive(Serialize)]
struct Deployment {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: Meta,
    spec: DeploymentSpec,
}

#[derive(Serialize)]
struct DeploymentSpec {
    replicas: u32,
    selector: Selector,
    template: PodTemplate,
}

#[derive(Serialize)]
struct Selector {
    #[serde(rename = "matchLabels")]
    match_labels: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct PodTemplate {
    metadata: TemplateMeta,
    spec: PodSpec,
}

#[derive(Serialize)]
struct TemplateMeta {
    labels: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct PodSpec {
    containers: Vec<Container>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    volumes: Vec<PodVolume>,
}

#[derive(Serialize)]
struct Container {
    name: String,
    image: String,
    #[serde(rename = "imagePullPolicy")]
    image_pull_policy: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ports: Vec<ContainerPort>,
    #[serde(rename = "envFrom", skip_serializing_if = "Vec::is_empty")]
    env_from: Vec<EnvFromSource>,
    #[serde(rename = "volumeMounts", skip_serializing_if = "Vec::is_empty")]
    volume_mounts: Vec<VolumeMount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<Vec<String>>,
    #[serde(rename = "readinessProbe", skip_serializing_if = "Option::is_none")]
    readiness_probe: Option<Probe>,
    #[serde(rename = "livenessProbe", skip_serializing_if = "Option::is_none")]
    liveness_probe: Option<Probe>,
}

#[derive(Serialize)]
struct ContainerPort {
    #[serde(rename = "containerPort")]
    container_port: u16,
}

#[derive(Serialize)]
struct EnvFromSource {
    #[serde(rename = "configMapRef", skip_serializing_if = "Option::is_none")]
    config_map_ref: Option<RefName>,
    #[serde(rename = "secretRef", skip_serializing_if = "Option::is_none")]
    secret_ref: Option<RefName>,
}

impl EnvFromSource {
    fn config(name: String) -> Self {
        Self {
            config_map_ref: Some(RefName { name }),
            secret_ref: None,
        }
    }
    fn secret(name: String) -> Self {
        Self {
            config_map_ref: None,
            secret_ref: Some(RefName { name }),
        }
    }
}

#[derive(Serialize)]
struct RefName {
    name: String,
}

#[derive(Serialize)]
struct VolumeMount {
    name: String,
    #[serde(rename = "mountPath")]
    mount_path: String,
}

#[derive(Clone, Serialize)]
struct Probe {
    exec: ExecAction,
    #[serde(rename = "periodSeconds")]
    period_seconds: u32,
    #[serde(rename = "timeoutSeconds")]
    timeout_seconds: u32,
    #[serde(rename = "failureThreshold")]
    failure_threshold: u32,
    #[serde(rename = "initialDelaySeconds")]
    initial_delay_seconds: u32,
}

#[derive(Clone, Serialize)]
struct ExecAction {
    command: Vec<String>,
}

#[derive(Serialize)]
struct PodVolume {
    name: String,
    #[serde(flatten)]
    source: PodVolumeSource,
}

#[derive(Serialize)]
#[serde(untagged)]
enum PodVolumeSource {
    Pvc(PvcRef),
    HostPath(HostPathSource),
    EmptyDir(EmptyDir),
}

#[derive(Serialize)]
struct PvcRef {
    #[serde(rename = "persistentVolumeClaim")]
    persistent_volume_claim: ClaimName,
}

impl PvcRef {
    fn new(claim_name: String) -> Self {
        Self {
            persistent_volume_claim: ClaimName { claim_name },
        }
    }
}

#[derive(Serialize)]
struct ClaimName {
    #[serde(rename = "claimName")]
    claim_name: String,
}

#[derive(Serialize)]
struct HostPathSource {
    #[serde(rename = "hostPath")]
    host_path: HostPathInner,
}

#[derive(Serialize)]
struct HostPathInner {
    path: String,
}

#[derive(Serialize)]
struct EmptyDir {
    #[serde(rename = "emptyDir")]
    empty_dir: EmptyDirInner,
}

#[derive(Serialize)]
struct EmptyDirInner {}

#[derive(Serialize)]
struct Service {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: Meta,
    spec: ServiceSpec,
}

#[derive(Serialize)]
struct ServiceSpec {
    selector: BTreeMap<String, String>,
    ports: Vec<ServicePort>,
}

#[derive(Serialize)]
struct ServicePort {
    port: u16,
    #[serde(rename = "targetPort")]
    target_port: u16,
}

#[derive(Serialize)]
struct ConfigMap {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: Meta,
    data: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Secret {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: Meta,
    #[serde(rename = "stringData")]
    string_data: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Pvc {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: Meta,
    spec: PvcSpec,
}

#[derive(Serialize)]
struct PvcSpec {
    #[serde(rename = "accessModes")]
    access_modes: Vec<String>,
    resources: PvcResources,
}

#[derive(Serialize)]
struct PvcResources {
    requests: BTreeMap<String, String>,
}
