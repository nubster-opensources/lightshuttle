//! A container started by this runtime must be able to reach a native
//! process running on the developer's machine, the same way it reaches a
//! sibling container by name. Docker provides that path through the
//! `host.docker.internal` alias, but only resolves it inside a container
//! when the container's configuration carries an explicit
//! `host.docker.internal:host-gateway` entry (Docker Desktop wires the
//! alias in by itself; a bare Linux engine does not).
//!
//! # What this file proves, and what it does not
//!
//! The guarantee splits in two, and so does its proof.
//!
//! That the alias is set on **every** container, unconditionally, is a
//! property of the one function that builds a container's host
//! configuration. It is proved without a daemon by the unit tests beside
//! that function, in `src/docker.rs`, and it is proved there rather than
//! here because that is where a condition on the project's composition
//! would have to be introduced to break it.
//!
//! What remains is the end of the chain: that the configuration really does
//! reach the daemon and that the daemon really does record it. Nothing but a
//! daemon can answer that, so the test below is marked `#[ignore]` and runs
//! in the Docker job of the continuous integration, like every other test in
//! this crate that needs one. A test that failed loudly instead would turn
//! every machine without Docker, and two of the three operating systems the
//! ordinary test job runs on, red for a reason that has nothing to do with
//! the change under test.

use std::process::Command;

use lightshuttle_runtime::{DockerRuntime, ResourceRuntime};
use lightshuttle_spec::{Argument, ContainerSpec, ImageSource, ResourceSpec};

fn unique_project(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("lsalias-{prefix}-{}-{seq}", std::process::id())
}

/// RAII guard that force-removes the container and the project network on
/// drop, so a failed assertion never leaves state behind for the next run.
struct Cleanup {
    container_name: String,
    project: String,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .output();
        let _ = Command::new("docker")
            .args(["network", "rm", &format!("lightshuttle-{}", self.project)])
            .output();
    }
}

/// The raw `HostConfig.ExtraHosts` list Docker recorded for `container_name`,
/// as reported by `docker inspect`.
fn extra_hosts_of(container_name: &str) -> String {
    let output = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{json .HostConfig.ExtraHosts}}",
            container_name,
        ])
        .output()
        .expect("docker inspect runs");
    assert!(
        output.status.success(),
        "docker inspect failed for `{container_name}`: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn sleeping_container(project: &str) -> ContainerSpec {
    let mut spec = ContainerSpec::new(
        format!("{project}_app"),
        project.to_owned(),
        "app".to_owned(),
        ImageSource::Pull("alpine:3.20".to_owned()),
    );
    spec.command = Some(vec![
        Argument::literal("sh"),
        Argument::literal("-c"),
        Argument::literal("sleep 30"),
    ]);
    spec
}

/// The daemon records the alias for a container this runtime started.
///
/// This is the end-to-end half of the guarantee: the unit tests in
/// `src/docker.rs` prove the configuration is built with the alias, and this
/// one proves it survives the trip to the daemon. The project used here holds
/// nothing but this container, so a version that set the alias only for
/// projects declaring a `process` resource would fail here too.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn a_started_container_carries_the_host_gateway_alias() {
    let project = unique_project("basic");
    let spec = sleeping_container(&project);
    let container_name = spec.name.clone();
    let _cleanup = Cleanup {
        container_name: container_name.clone(),
        project: project.clone(),
    };

    let runtime = DockerRuntime::connect().expect("docker runtime connects");
    runtime
        .start(&ResourceSpec::Container(spec))
        .await
        .expect("container starts");

    let extra_hosts = extra_hosts_of(&container_name);
    assert!(
        extra_hosts.contains("host.docker.internal:host-gateway"),
        "a started container must carry the host.docker.internal alias so \
         it can reach a native process; got ExtraHosts = {extra_hosts}"
    );
}
