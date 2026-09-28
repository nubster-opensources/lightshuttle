//! `HostRuntime` connects to the container daemon lazily, on first real
//! need, never at construction. A project made only of `process` resources
//! must be able to start without a container daemon running at all, and
//! must never create the per-project bridge network that only containers
//! need.
//!
//! # Which half of that needs a daemon, and which does not
//!
//! That nothing in the process path reaches for the daemon is proved
//! **without** one, by `connect_does_not_contact_the_daemon`: it points
//! `DOCKER_HOST` at a socket that cannot exist, so any code path that did
//! reach for the daemon fails there instead of quietly succeeding against
//! whatever daemon the machine happens to be running. That test also settles
//! the network question by implication, since creating a network is something
//! only the daemon can do.
//!
//! The two network-shape tests confirm the same thing from the other side, by
//! looking at what the daemon actually holds. They need one, so they are
//! marked `#[ignore]` and run in the Docker job of the continuous
//! integration, like every other test in this crate that needs a daemon. They
//! used to fail loudly instead, which turned macOS and Windows red in the
//! ordinary test job for want of a daemon those runners never had.

use std::process::Command;

use lightshuttle_runtime::{HostRuntime, ResourceRuntime};
use lightshuttle_spec::{Argument, ContainerSpec, ImageSource, ProcessSpec, ResourceSpec};

fn unique_project(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("lsonly-{prefix}-{}-{seq}", std::process::id())
}

fn network_exists(project: &str) -> bool {
    Command::new("docker")
        .args(["network", "inspect", &format!("lightshuttle-{project}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// RAII guard removing every container and the project network on drop.
struct Cleanup {
    project: String,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let label = format!("label=lightshuttle.project={}", self.project);
        if let Ok(listed) = Command::new("docker")
            .args(["ps", "-aq", "--filter", &label])
            .output()
        {
            for id in String::from_utf8_lossy(&listed.stdout).split_whitespace() {
                let _ = Command::new("docker").args(["rm", "-f", id]).output();
            }
        }
        let _ = Command::new("docker")
            .args(["network", "rm", &format!("lightshuttle-{}", self.project)])
            .output();
    }
}

/// A command that stays up long enough for the assertions below, on either
/// platform.
///
/// `sh` is not on a Windows machine's search path, so a single hard-coded
/// command would have made this whole file fail there for a reason that has
/// nothing to do with what it tests.
#[cfg(unix)]
fn long_running_command() -> Vec<String> {
    vec!["sh".to_owned(), "-c".to_owned(), "sleep 30".to_owned()]
}

#[cfg(windows)]
fn long_running_command() -> Vec<String> {
    // `ping` with a count is the portable Windows wait: `timeout` refuses to
    // run when standard input is redirected, which is exactly how a test
    // harness runs it.
    vec![
        "cmd".to_owned(),
        "/C".to_owned(),
        "ping -n 31 127.0.0.1 >nul".to_owned(),
    ]
}

fn a_process(project: &str) -> ProcessSpec {
    ProcessSpec::new(
        format!("{project}_worker"),
        project.to_owned(),
        "worker".to_owned(),
        long_running_command(),
    )
}

fn a_container(project: &str) -> ContainerSpec {
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

/// Confirms from the daemon's side what `connect_does_not_contact_the_daemon`
/// establishes by implication: no bridge network exists for a project made
/// only of `process` resources.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn a_process_only_project_creates_no_project_network() {
    let project = unique_project("bare");
    let _cleanup = Cleanup {
        project: project.clone(),
    };

    let tmp = tempfile::tempdir().expect("temp state root is created");
    let runtime = HostRuntime::connect(tmp.path()).expect("host runtime connects");

    runtime
        .start(&ResourceSpec::Process(a_process(&project)))
        .await
        .expect("a process-only resource starts without a network");

    assert!(
        !network_exists(&project),
        "a project made only of process resources must never create \
         `lightshuttle-{project}`"
    );
}

/// `HostRuntime::connect` must never contact the container daemon: it is
/// how a project made only of `process` resources gets to start without a
/// Docker daemon running at all.
///
/// `DOCKER_HOST` is the lever that proves it, pointed at a socket path that
/// cannot exist so any code path that does reach for the daemon fails
/// loudly instead of silently succeeding against whatever real daemon
/// happens to be running on the machine executing this test. Setting an
/// environment variable is unsafe as of this workspace's edition, and this
/// workspace denies unsafe code outright, so the poisoned value is set on a
/// freshly spawned child process's environment (a safe builder method)
/// instead of mutated on this one: this test re-executes the very binary it
/// runs in, filtered to just the assertion below.
#[test]
fn connect_does_not_contact_the_daemon() {
    let exe = std::env::current_exe().expect("this test binary's own path is known");
    let output = Command::new(exe)
        .args([
            "connect_does_not_contact_the_daemon_when_poisoned",
            "--exact",
        ])
        .env(
            "DOCKER_HOST",
            "unix:///nonexistent/lightshuttle-test-unreachable.sock",
        )
        .output()
        .expect("the child test process runs");

    assert!(
        output.status.success(),
        "starting a process-only resource must succeed even though the \
         configured Docker daemon is unreachable: nothing in that path may \
         lazily reach for it.\nchild stdout: {}\nchild stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The actual assertion behind `connect_does_not_contact_the_daemon`, run
/// only in the child process it spawns with `DOCKER_HOST` poisoned. Running
/// here too as part of the ordinary suite (with whatever daemon state the
/// host machine happens to have) is harmless: it exercises the same code
/// path, just without proving laziness on its own.
#[tokio::test]
async fn connect_does_not_contact_the_daemon_when_poisoned() {
    let project = unique_project("lazy");
    let tmp = tempfile::tempdir().expect("temp state root is created");

    let runtime =
        HostRuntime::connect(tmp.path()).expect("connect must not itself contact the daemon");
    runtime
        .start(&ResourceSpec::Process(a_process(&project)))
        .await
        .expect(
            "starting a process-only resource must succeed even though the \
             configured Docker daemon is unreachable: nothing in that path \
             may lazily reach for it",
        );
}

/// The contrast to `a_process_only_project_creates_no_project_network`: a
/// project that does hold a container must still get its bridge network.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn a_project_with_a_container_creates_the_project_network() {
    let project = unique_project("with-container");
    let _cleanup = Cleanup {
        project: project.clone(),
    };

    let tmp = tempfile::tempdir().expect("temp state root is created");
    let runtime = HostRuntime::connect(tmp.path()).expect("host runtime connects");

    runtime
        .start(&ResourceSpec::Container(a_container(&project)))
        .await
        .expect("a container resource starts");

    assert!(
        network_exists(&project),
        "a project holding a container must create `lightshuttle-{project}`"
    );
}
