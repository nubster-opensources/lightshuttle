//! Integration test: secret injection with a real PostgreSQL container.
//!
//! The test starts a Postgres container via the Docker CLI, attaches it to the
//! LightShuttle project network under the alias `db`, then starts a `client`
//! container whose healthcheck runs `psql` with a password drawn from the
//! `DB_SECRET` environment variable injected via `LifecycleManager::with_env`.
//! The client reaching `Healthy` proves that the runtime correctly expands
//! `${env.DB_SECRET}` in the manifest and that the secret authenticates against
//! the real database.
//!
//! No testcontainers crate is used: Postgres is started with `docker run` to
//! avoid a bollard version conflict between testcontainers and this workspace.
//!
//! Run locally with:
//! `cargo test -p lightshuttle-runtime --test secrets_scenarios -- --ignored --nocapture`

mod common;

use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{DockerRuntime, LifecycleManager, LifecyclePlan};

/// Postgres password injected as a secret through `with_env`.
const DB_SECRET: &str = "s3cr3t-pw";

/// Generous deadline: covers a cold image pull, Postgres startup, and the
/// first successful `psql` healthcheck probe.
const HEALTH_DEADLINE: Duration = Duration::from_secs(60);

/// Grace window for teardown.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Parse `yaml` into a `LifecyclePlan`, panicking with context on failure.
fn plan_from(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

/// Manifest describing a single `client` container that authenticates against
/// the Postgres instance running under the `db` alias on the project network.
///
/// The secret is referenced as `${env.DB_SECRET}` so the runtime expands it
/// from the environment map injected via `with_env`.
fn secrets_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  client:
    container:
      image: postgres:16
      command: ["sh", "-c", "sleep 120"]
      env:
        PGPASSWORD: "${{env.DB_SECRET}}"
      healthcheck:
        test: ["CMD-SHELL", "psql -h db -U app -d app -c 'SELECT 1' || exit 1"]
        interval: "1s"
        timeout: "3s"
        retries: 20
        start_period: "1s"
"#
    )
}

/// Run a Docker CLI command and return the trimmed stdout.
///
/// Panics on failure unless `tolerate_already_exists` is `true` and the
/// stderr output contains "already exists" (network create 409 case).
fn run_docker(args: &[&str], tolerate_already_exists: bool) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker CLI is available");

    if output.status.success() {
        return String::from_utf8_lossy(&output.stdout).trim().to_owned();
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if tolerate_already_exists && stderr.contains("already exists") {
        return String::new();
    }

    panic!(
        "docker {} failed (exit {:?}):\n{stderr}",
        args.join(" "),
        output.status.code(),
    );
}

/// RAII guard that force-removes a single container by id on drop.
struct ContainerCleanup {
    id: String,
}

impl ContainerCleanup {
    fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl Drop for ContainerCleanup {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.id]).output();
    }
}

/// A secret injected via `with_env` authenticates a `psql` client against a
/// real PostgreSQL container attached to the project network under alias `db`.
///
/// Drop order is intentional: `_pg_guard` is declared after `_guard` so Rust
/// drops `_pg_guard` first (force-removing the Postgres container and
/// detaching it from the network), then `_guard` removes the network and the
/// client containers. Reversing the order would cause `docker network rm` to
/// race with a still-attached Postgres container.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn injected_secret_authenticates_against_a_real_postgres() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("sec");

    // Outer RAII guard: removes client containers and the project network.
    // Declared first so it is dropped last.
    let _guard = common::ProjectCleanup::new(project.clone());

    // Create the project network before starting Postgres so it exists when
    // Postgres is connected. Tolerate "already exists" in case the runtime
    // creates it first.
    //
    // The ownership label is what the runtime itself writes, and it is not
    // decoration here: the runtime refuses to attach to a `lightshuttle-*`
    // network that carries no owner, because such a network is
    // indistinguishable from one that belongs to something else on the host.
    // Pre-creating it without the label would make this test simulate a
    // foreign network rather than the project's own.
    let network = format!("lightshuttle-{project}");
    run_docker(
        &[
            "network",
            "create",
            "--label",
            &format!("lightshuttle.project={project}"),
            &network,
        ],
        true,
    );

    // Start a Postgres container directly via the Docker CLI to avoid a
    // bollard version conflict that testcontainers would introduce.
    let pg_id = run_docker(
        &[
            "run",
            "--detach",
            "--env",
            "POSTGRES_USER=app",
            "--env",
            &format!("POSTGRES_PASSWORD={DB_SECRET}"),
            "--env",
            "POSTGRES_DB=app",
            "postgres:16",
        ],
        false,
    );
    // Inner RAII guard: force-removes Postgres before the outer guard tears
    // down the network.
    let _pg_guard = ContainerCleanup::new(pg_id.clone());

    // Attach Postgres to the LightShuttle project network under alias `db`.
    run_docker(
        &["network", "connect", "--alias", "db", &network, &pg_id],
        false,
    );

    // Build and start the LightShuttle stack.
    let plan = plan_from(&secrets_stack(&project));
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    let mut env = HashMap::new();
    env.insert("DB_SECRET".to_owned(), DB_SECRET.to_owned());
    let manager = manager.with_env(env);
    manager
        .check_required_env()
        .expect("required secret DB_SECRET is present");

    manager.start_all().await.expect("stack boots");

    common::wait_for_healthy(&mut events, "client", HEALTH_DEADLINE)
        .await
        .expect("client authenticates against postgres with the injected secret");

    manager.stop_all(STOP_GRACE).await.expect("stack stops");
}
