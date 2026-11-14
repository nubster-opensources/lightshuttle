//! `OTel` augmentation of a `process` resource.
//!
//! A `process` runs on the developer's own machine, not inside the Docker
//! network the bundled collector's container joins. It must therefore be
//! pointed at the collector through the loopback address and the published
//! OTLP gRPC port, never through the collector's Docker DNS name, which a
//! native process cannot resolve.
//!
//! Manifests here are built directly through the manifest model rather than
//! parsed from YAML, bypassing `Manifest::parse`, so these tests exercise
//! `augment_manifest` in isolation from `ResourceKind` deserialisation.

use indexmap::IndexMap;
use lightshuttle_manifest::{ContainerConfig, Manifest, ProcessConfig, Project, ResourceKind};
use lightshuttle_otel::{CollectorConfig, SYNTHETIC_RESOURCE_NAME, augment_manifest};

const OTEL_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const OTEL_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";

fn manifest_with_one_resource(name: &str, kind: ResourceKind) -> Manifest {
    let mut resources = IndexMap::new();
    resources.insert(name.to_owned(), kind);
    Manifest {
        lightshuttle: None,
        project: Project {
            name: "demo".to_owned(),
            version: None,
            description: None,
        },
        dashboard: None,
        observability: None,
        export: None,
        resources,
    }
}

fn process_kind() -> ResourceKind {
    let mut config = ProcessConfig::new(vec!["node".to_owned(), "server.js".to_owned()]);
    config.port = Some(4000);
    ResourceKind::Process(config)
}

#[test]
fn process_resource_receives_the_loopback_otlp_endpoint() {
    let mut manifest = manifest_with_one_resource("worker", process_kind());
    let config = CollectorConfig::defaults();

    augment_manifest(&mut manifest, &config);

    let ResourceKind::Process(worker) = manifest
        .resources
        .get("worker")
        .expect("worker resource must still exist")
    else {
        panic!("expected the worker resource to stay a process");
    };

    assert_eq!(
        worker.env.get(OTEL_ENDPOINT).map(String::as_str),
        Some(format!("http://127.0.0.1:{}", config.otlp_grpc_port).as_str()),
        "a process must reach the collector on the loopback address and the published OTLP gRPC \
         port, got env: {:?}",
        worker.env
    );
}

#[test]
fn process_resource_never_receives_the_collector_dns_name() {
    let mut manifest = manifest_with_one_resource("worker", process_kind());
    let config = CollectorConfig::defaults();

    augment_manifest(&mut manifest, &config);

    let ResourceKind::Process(worker) = manifest
        .resources
        .get("worker")
        .expect("worker resource must still exist")
    else {
        panic!("expected the worker resource to stay a process");
    };

    let endpoint = worker
        .env
        .get(OTEL_ENDPOINT)
        .expect("the process must receive an OTLP endpoint");
    let dns_name = config.hostname(&manifest.project.name);
    assert!(
        !endpoint.contains(dns_name.as_str()),
        "a process cannot resolve the collector's Docker-network DNS name `{dns_name}`, but the \
         endpoint `{endpoint}` contains it"
    );
}

#[test]
fn process_resource_receives_service_name_and_collector_dependency() {
    let mut manifest = manifest_with_one_resource("worker", process_kind());
    let config = CollectorConfig::defaults();

    augment_manifest(&mut manifest, &config);

    let ResourceKind::Process(worker) = manifest
        .resources
        .get("worker")
        .expect("worker resource must still exist")
    else {
        panic!("expected the worker resource to stay a process");
    };

    assert_eq!(
        worker.env.get(OTEL_SERVICE_NAME).map(String::as_str),
        Some("worker"),
        "a process must receive OTEL_SERVICE_NAME like a container, got env: {:?}",
        worker.env
    );
    assert!(
        worker
            .depends_on
            .iter()
            .any(|dependency| dependency == SYNTHETIC_RESOURCE_NAME),
        "a process must depend on the collector like a container, got depends_on: {:?}",
        worker.depends_on
    );
}

/// Control case: a container must keep receiving the collector's Docker
/// DNS name, exactly as before this manifest could also hold a process.
#[test]
fn container_resource_still_receives_the_collector_dns_name() {
    let kind = ResourceKind::Container(ContainerConfig::new("alpine".to_owned()));
    let mut manifest = manifest_with_one_resource("api", kind);
    let config = CollectorConfig::defaults();

    augment_manifest(&mut manifest, &config);

    let ResourceKind::Container(api) = manifest
        .resources
        .get("api")
        .expect("api resource must still exist")
    else {
        panic!("expected the api resource to stay a container");
    };

    assert_eq!(
        api.env.get(OTEL_ENDPOINT).map(String::as_str),
        Some(
            format!(
                "http://demo_{SYNTHETIC_RESOURCE_NAME}:{}",
                config.otlp_grpc_port
            )
            .as_str()
        ),
        "a container must keep resolving the collector by its Docker-network DNS name, got env: {:?}",
        api.env
    );
}
