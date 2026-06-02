//! Integration tests that require a running Docker daemon.
//!
//! Run with `cargo test --test integration_docker -- --ignored` on a
//! machine where `docker info` succeeds. These tests are excluded from
//! the standard CI run.

use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;

use lightshuttle_runtime::{
    ContainerRuntime, ContainerSpec, ContainerStatus, DockerRuntime, ImageSource,
};

fn small_image_spec(name: &str) -> ContainerSpec {
    ContainerSpec {
        name: name.to_owned(),
        project: "lightshuttle_it".to_owned(),
        resource: name.to_owned(),
        image: ImageSource::Pull("alpine:3.20".to_owned()),
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: Some(vec!["sh".to_owned(), "-c".to_owned(), "sleep 5".to_owned()]),
        healthcheck: None,
        working_dir: None,
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
async fn builds_and_runs_a_dockerfile_resource() {
    let context = tempfile::tempdir().expect("temp dir created");

    // Minimal Dockerfile: small image, instant exit.
    let dockerfile_path = context.path().join("Dockerfile");
    let mut dockerfile = std::fs::File::create(&dockerfile_path).expect("Dockerfile created");
    writeln!(dockerfile, "FROM alpine:3.20").expect("write line");
    writeln!(dockerfile, "CMD [\"sh\", \"-c\", \"sleep 5\"]").expect("write line");
    drop(dockerfile);

    // Verify that .dockerignore filtering does not break the build:
    // include one ignored file plus an entry in .dockerignore.
    std::fs::write(context.path().join("noisy.tmp"), b"junk").expect("noisy file written");
    std::fs::write(context.path().join(".dockerignore"), b"*.tmp\n").expect("ignore written");

    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let spec = ContainerSpec {
        name: "lightshuttle_it_build_run".to_owned(),
        project: "lightshuttle_it".to_owned(),
        resource: "build_run".to_owned(),
        image: ImageSource::Build {
            context: context.path().to_string_lossy().into_owned(),
            dockerfile: "Dockerfile".to_owned(),
            build_args: HashMap::new(),
            target: None,
            tag: "lightshuttle/it_build_run:dev".to_owned(),
        },
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: None,
        healthcheck: None,
        working_dir: None,
    };

    let id = runtime
        .start(&spec)
        .await
        .expect("dockerfile resource builds and starts");

    runtime
        .stop(&id, Duration::from_secs(2))
        .await
        .expect("container stops cleanly");
}

#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn builds_buildkit_only_dockerfile() {
    let context = tempfile::tempdir().expect("temp dir created");

    let dockerfile_path = context.path().join("Dockerfile");
    let mut dockerfile = std::fs::File::create(&dockerfile_path).expect("Dockerfile created");
    writeln!(dockerfile, "# syntax=docker/dockerfile:1").expect("write line");
    writeln!(dockerfile, "FROM alpine:3.20").expect("write line");
    writeln!(
        dockerfile,
        "RUN --mount=type=cache,target=/var/cache/apk echo cached"
    )
    .expect("write line");
    writeln!(dockerfile, "CMD [\"sh\", \"-c\", \"sleep 5\"]").expect("write line");
    drop(dockerfile);

    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let spec = ContainerSpec {
        name: "lightshuttle_it_buildkit".to_owned(),
        project: "lightshuttle_it".to_owned(),
        resource: "buildkit".to_owned(),
        image: ImageSource::Build {
            context: context.path().to_string_lossy().into_owned(),
            dockerfile: "Dockerfile".to_owned(),
            build_args: HashMap::new(),
            target: None,
            tag: "lightshuttle/it_buildkit:dev".to_owned(),
        },
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: None,
        healthcheck: None,
        working_dir: None,
    };

    let id = runtime
        .start(&spec)
        .await
        .expect("BuildKit-only Dockerfile builds and starts");

    runtime
        .stop(&id, Duration::from_secs(2))
        .await
        .expect("container stops cleanly");
}
