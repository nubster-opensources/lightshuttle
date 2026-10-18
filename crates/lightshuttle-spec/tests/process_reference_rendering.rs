//! The four cases of the reference-rendering table from the `process`
//! networking design (#171), made executable.
//!
//! A `${resources.<name>.<property>}` reference no longer has one value. The
//! address a service is reached at depends on who is asking: a container sits
//! on the project network, a native process does not. These tests pin the
//! four combinations so that neither side can be changed without the other
//! being considered.
//!
//! They deliberately build `ResourceKind` values through the Rust API rather
//! than parsing YAML. Parsing would make every test here fail at the same
//! place, in deserialisation, and tell us nothing about rendering.

use indexmap::IndexMap;
use lightshuttle_manifest::{ContainerConfig, PortMapping, ProcessConfig, ResourceKind};
use lightshuttle_spec::{ConsumerKind, SpecError, outputs_for_consumer};

const PROJECT: &str = "acme";

/// A container publishing host port 8080 onto container port 80.
fn published_container() -> ResourceKind {
    let mut config = ContainerConfig::new("example/api:1.0".to_owned());
    config.ports = vec![PortMapping::Mapping("8080:80".to_owned())];
    ResourceKind::Container(config)
}

/// A process listening on port 5173.
fn listening_process() -> ResourceKind {
    let mut config = ProcessConfig::new(vec!["npm".to_owned(), "run".to_owned(), "dev".to_owned()]);
    config.port = Some(5173);
    ResourceKind::Process(config)
}

fn host_of(outputs: &IndexMap<String, String>) -> String {
    outputs
        .get("host")
        .unwrap_or_else(|| panic!("outputs carry no `host` property: {outputs:?}"))
        .clone()
}

fn port_of(outputs: &IndexMap<String, String>) -> String {
    outputs
        .get("port")
        .unwrap_or_else(|| panic!("outputs carry no `port` property: {outputs:?}"))
        .clone()
}

#[test]
fn container_consumer_targeting_container_uses_the_resource_name() {
    let outputs = outputs_for_consumer(
        PROJECT,
        "api",
        &published_container(),
        ConsumerKind::Container,
    )
    .expect("a published container resolves for a container consumer");

    assert_eq!(
        host_of(&outputs),
        format!("{PROJECT}_api"),
        "containers reach each other by container name through the project bridge DNS"
    );
}

#[test]
fn container_consumer_targeting_process_uses_host_docker_internal() {
    let outputs = outputs_for_consumer(
        PROJECT,
        "web",
        &listening_process(),
        ConsumerKind::Container,
    )
    .expect("a listening process resolves for a container consumer");

    assert_eq!(
        host_of(&outputs),
        "host.docker.internal",
        "a container leaves the project network to reach a host process"
    );
    assert_eq!(port_of(&outputs), "5173");
}

#[test]
fn process_consumer_targeting_container_uses_loopback_and_the_published_host_port() {
    let outputs = outputs_for_consumer(
        PROJECT,
        "api",
        &published_container(),
        ConsumerKind::Process,
    )
    .expect("a published container resolves for a process consumer");

    assert_eq!(
        host_of(&outputs),
        "127.0.0.1",
        "a host process does not resolve the project bridge DNS"
    );
    assert_eq!(
        port_of(&outputs),
        "8080",
        "the port a process reaches is the one published on the host, not the container port"
    );
}

#[test]
fn process_consumer_targeting_process_uses_loopback_and_the_process_port() {
    let outputs = outputs_for_consumer(PROJECT, "web", &listening_process(), ConsumerKind::Process)
        .expect("a listening process resolves for a process consumer");

    assert_eq!(host_of(&outputs), "127.0.0.1");
    assert_eq!(port_of(&outputs), "5173");
}

/// The same target rendered for both consumers must not yield the same host.
///
/// Stated on its own because it is the whole point of the table, and because
/// an implementation that ignored `ConsumerKind` entirely would still satisfy
/// two of the four cases above by coincidence.
#[test]
fn the_same_target_renders_differently_for_each_consumer() {
    let target = published_container();

    let for_container = outputs_for_consumer(PROJECT, "api", &target, ConsumerKind::Container)
        .expect("resolves for a container consumer");
    let for_process = outputs_for_consumer(PROJECT, "api", &target, ConsumerKind::Process)
        .expect("resolves for a process consumer");

    assert_ne!(
        host_of(&for_container),
        host_of(&for_process),
        "a reference that rendered identically for both consumers would mean one of them cannot reach the service"
    );
}

/// A container publishing nothing is reachable by its container siblings and
/// by nobody else: a native process is not on the project network. Rendering
/// an address for it would hand the process a host with no port, or a port
/// belonging to the inside of the container.
///
/// The refusal is stated here because the four rendering tests above all use a
/// container that does publish, so none of them would notice its absence.
#[test]
fn a_container_publishing_no_port_has_no_address_for_a_process() {
    let unpublished = ResourceKind::Container(ContainerConfig::new("example/api:1.0".to_owned()));

    let error = outputs_for_consumer(PROJECT, "api", &unpublished, ConsumerKind::Process)
        .expect_err("a container publishing no port must not render an address for a process");

    assert!(
        matches!(error, SpecError::NoPublishedPort { ref resource } if resource == "api"),
        "the refusal must name the resource that publishes nothing, got: {error}"
    );
}

/// The contrast: the very same container is perfectly reachable by another
/// container, through the project bridge DNS. Without this case, the refusal
/// above could be satisfied by refusing every unpublished container to
/// everyone.
#[test]
fn a_container_publishing_no_port_is_still_reachable_by_a_container() {
    let unpublished = ResourceKind::Container(ContainerConfig::new("example/api:1.0".to_owned()));

    let outputs = outputs_for_consumer(PROJECT, "api", &unpublished, ConsumerKind::Container)
        .expect("a container reaches an unpublished sibling through the bridge DNS");

    assert_eq!(host_of(&outputs), format!("{PROJECT}_api"));
}

/// A process that declares no `port` is a legitimate resource, and it starts
/// normally. What it does not have is an address, so a reference asking for
/// one is refused rather than rendered with a port guessed at runtime.
#[test]
fn a_process_declaring_no_port_has_no_address() {
    let silent = ResourceKind::Process(ProcessConfig::new(vec!["npm".to_owned()]));

    for consumer in [ConsumerKind::Container, ConsumerKind::Process] {
        let error = outputs_for_consumer(PROJECT, "web", &silent, consumer)
            .expect_err("a process declaring no port must not render an address");

        assert!(
            matches!(error, SpecError::ProcessWithoutPort { ref resource } if resource == "web"),
            "the refusal must name the portless process, got: {error} for {consumer:?}"
        );
    }
}

/// A process exposes no `url`, even when it declares a port.
///
/// It declares a port, never a protocol. Rendering `http://` would be a guess,
/// and a guessed URL lands in an environment variable, so it is discovered by
/// the service that reads it rather than by the developer who wrote the
/// manifest.
#[test]
fn a_process_exposes_no_url_output() {
    let outputs = outputs_for_consumer(PROJECT, "web", &listening_process(), ConsumerKind::Process)
        .expect("a listening process resolves");

    assert!(
        !outputs.contains_key("url"),
        "a process must expose no `url`, since it declares no protocol; got: {outputs:?}"
    );
}
