//! Integration tests that require a running Docker daemon.
//!
//! Run with `cargo test --test integration_docker -- --ignored` on a
//! machine where `docker info` succeeds. These tests are excluded from
//! the standard CI run.

use std::collections::HashMap;
use std::time::Duration;

use lightshuttle_runtime::{
    ContainerRuntime, ContainerSpec, ContainerStatus, DockerRuntime, ImageSource,
};

fn small_image_spec(name: &str) -> ContainerSpec {
    ContainerSpec {
        name: name.to_owned(),
        image: ImageSource::Pull("alpine:3.20".to_owned()),
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: Some(vec!["sh".to_owned(), "-c".to_owned(), "sleep 5".to_owned()]),
        healthcheck: None,
    }
}

#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn start_inspect_stop_alpine() {
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let spec = small_image_spec("lightshuttle_it_start_inspect_stop");

    let id = runtime.start(&spec).await.expect("container starts");

    let status = runtime.inspect(&id).await.expect("inspect succeeds");
    assert!(matches!(
        status,
        ContainerStatus::Running | ContainerStatus::Starting | ContainerStatus::Healthy
    ));

    runtime
        .stop(&id, Duration::from_secs(2))
        .await
        .expect("stop succeeds");
}

#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn build_variant_is_currently_unsupported() {
    use lightshuttle_runtime::RuntimeError;

    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let spec = ContainerSpec {
        name: "lightshuttle_it_build".to_owned(),
        image: ImageSource::Build {
            context: ".".to_owned(),
            dockerfile: "Dockerfile".to_owned(),
            build_args: HashMap::new(),
            target: None,
            tag: "lightshuttle/it_build:dev".to_owned(),
        },
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: None,
        healthcheck: None,
    };

    let err = runtime
        .start(&spec)
        .await
        .expect_err("build should error in v0.1");
    assert!(
        matches!(err, RuntimeError::InvalidSpec(_)),
        "expected InvalidSpec error, got {err:?}"
    );
}
