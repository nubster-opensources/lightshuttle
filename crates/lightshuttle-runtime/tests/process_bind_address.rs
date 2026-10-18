//! Behaviour of `process_bind_address`: the address a native process of a
//! project must bind to so that the project's containers can reach it.
//!
//! Only the runtime can answer, and the answer depends on two things: the
//! kind of daemon running (a Docker Desktop daemon proxies loopback into
//! every container by itself; a bare Linux engine does not, so a process
//! bound to loopback would be unreachable from a container there), and
//! whether the project holds any container at all (with none, nothing is
//! asking, so binding wider than loopback would just expose a development
//! service to the local network for nobody).
//!
//! # How both kinds of daemon are exercised without two machines
//!
//! The first three tests distinguish a Docker Desktop daemon from a bare
//! Linux engine. The real [`lightshuttle_runtime::DockerRuntime`] only knows
//! which it is talking to by asking a live daemon, which would make these
//! tests depend on what happens to be installed on the machine running them
//! rather than on the logic under test.
//!
//! [`DaemonKind`] and [`MockRuntime::simulate_daemon`] exist for that: they
//! let the mock answer as either kind. They live in
//! [`testkit`](lightshuttle_runtime::testkit) because they serve tests and
//! nothing else, and they carry only the one distinction that is real, which
//! is where a native process must bind.
//!
//! The fourth test needs no such addition: it exercises the real
//! `ProcessRuntime` and checks that the bind address is not just computed
//! but actually lands in the spawned process's environment.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use futures::StreamExt;

use lightshuttle_runtime::testkit::{DaemonKind, MockRuntime};
use lightshuttle_runtime::{BIND_ADDRESS_VARIABLE, ProcessRuntime, ResourceRuntime};
use lightshuttle_spec::{ContainerSpec, ImageSource, ProcessSpec, ResourceSpec};

fn mock_container(project: &str, resource: &str) -> ContainerSpec {
    ContainerSpec::new(
        format!("{project}_{resource}"),
        project.to_owned(),
        resource.to_owned(),
        ImageSource::Pull("alpine:3.20".to_owned()),
    )
}

#[tokio::test]
async fn a_docker_desktop_daemon_binds_a_process_to_loopback_even_with_a_container_running() {
    let mock = MockRuntime::new();
    mock.simulate_daemon(DaemonKind::DockerDesktop);

    // A container is running for the project: under a Linux engine this
    // would matter, but a Docker Desktop daemon proxies loopback into every
    // container by itself, so the answer must stay loopback regardless.
    mock.start(&ResourceSpec::Container(mock_container("proj", "app")))
        .await
        .expect("mock container starts");

    let address = mock
        .process_bind_address("proj")
        .await
        .expect("bind address resolves");
    assert_eq!(address, IpAddr::V4(Ipv4Addr::LOCALHOST));
}

#[tokio::test]
async fn a_linux_engine_with_a_container_binds_a_process_to_the_project_gateway() {
    let gateway: IpAddr = "172.20.0.1".parse().expect("valid address literal");
    let mock = MockRuntime::new();
    mock.simulate_daemon(DaemonKind::Linux { gateway });

    mock.start(&ResourceSpec::Container(mock_container("proj", "app")))
        .await
        .expect("mock container starts");

    let address = mock
        .process_bind_address("proj")
        .await
        .expect("bind address resolves");
    assert_eq!(
        address, gateway,
        "a process bound to loopback would be unreachable from a container \
         under a bare Linux engine, so the answer must be the project gateway"
    );
}

#[tokio::test]
async fn a_linux_engine_with_no_container_binds_a_process_to_loopback() {
    let gateway: IpAddr = "172.20.0.1".parse().expect("valid address literal");
    let mock = MockRuntime::new();
    mock.simulate_daemon(DaemonKind::Linux { gateway });

    // No container was ever started for this project: nothing is asking to
    // reach the process, so binding wider than loopback would only expose a
    // development service to the local network for nobody.
    let address = mock
        .process_bind_address("proj")
        .await
        .expect("bind address resolves");
    assert_eq!(address, IpAddr::V4(Ipv4Addr::LOCALHOST));
}

#[cfg(unix)]
fn shell_echo_command(variable: &str) -> Vec<String> {
    vec![
        "sh".to_owned(),
        "-c".to_owned(),
        format!("echo ${variable}"),
    ]
}

#[cfg(windows)]
fn shell_echo_command(variable: &str) -> Vec<String> {
    vec![
        "cmd".to_owned(),
        "/C".to_owned(),
        format!("echo %{variable}%"),
    ]
}

/// The bind address is meaningless if it is only ever computed and never
/// actually handed to the process. This test spawns a real one-shot process
/// through the real `ProcessRuntime`, has it print its own environment
/// variable back, and reads that back from its captured logs: the value
/// must have made the round trip through the operating system, not just
/// through this crate's own bookkeeping.
#[tokio::test]
async fn the_bind_address_variable_is_actually_present_in_the_spawned_environment() {
    let tmp = tempfile::tempdir().expect("temp dir is created");
    let runtime = ProcessRuntime::new(tmp.path());

    let bind_address = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let command = shell_echo_command(BIND_ADDRESS_VARIABLE);
    let spec = ProcessSpec::new(
        "proj_probe".to_owned(),
        "proj".to_owned(),
        "probe".to_owned(),
        command,
    );

    let id = runtime
        .start(&spec, bind_address)
        .await
        .expect("process starts");

    // The probe is a one-shot echo, not a long-running service: give it a
    // moment to run to completion before reading its output back.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut stream = runtime
        .logs(&id, false)
        .await
        .expect("logs are available for a process this runtime started");
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        output.extend_from_slice(&chunk.expect("a log chunk reads without error").bytes);
    }
    let output = String::from_utf8_lossy(&output);

    assert!(
        output.contains(&bind_address.to_string()),
        "the spawned process must actually see {BIND_ADDRESS_VARIABLE} in its \
         own environment, not just have it computed and discarded; \
         captured output was: {output:?}"
    );
}
