//! Unit tests for the manifest-to-spec conversion. No Docker daemon
//! required.

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{ImageSource, VolumeSource, from_resource};

const MANIFEST: &str = r#"
project:
  name: app
resources:
  api_db:
    postgres:
      version: "16"
  cache:
    redis:
      version: "7"
      password: "s3cret"
  api:
    container:
      image: my-org/api:1.0
      ports:
        - 8080
        - "9090:9090"
      env:
        LEVEL: info
"#;

#[test]
fn postgres_resolves_defaults() {
    let manifest = Manifest::parse(MANIFEST).expect("parses");
    let kind = manifest.resources.get("api_db").expect("api_db exists");
    let spec = from_resource("app", "api_db", kind).expect("spec built");

    assert_eq!(spec.name, "app_api_db");
    assert!(matches!(spec.image, ImageSource::Pull(ref s) if s == "postgres:16-alpine"));
    // Database name auto-derived from resource name.
    assert_eq!(
        spec.env.get("POSTGRES_DB").map(String::as_str),
        Some("api_db")
    );
    assert_eq!(
        spec.env.get("POSTGRES_USER").map(String::as_str),
        Some("postgres")
    );
    assert!(spec.env.contains_key("POSTGRES_PASSWORD"));
    assert_eq!(spec.ports.len(), 1);
    assert_eq!(spec.ports[0].container_port, 5432);
    assert_eq!(spec.ports[0].host_port, 5432);
    // Anonymous volume by default.
    assert_eq!(spec.volumes.len(), 1);
    assert!(matches!(spec.volumes[0].source, VolumeSource::Anonymous));
    assert_eq!(spec.volumes[0].target, "/var/lib/postgresql/data");
    // Default healthcheck materialised.
    assert!(spec.healthcheck.is_some());
}

#[test]
fn redis_passes_password_through_command() {
    let manifest = Manifest::parse(MANIFEST).unwrap();
    let kind = manifest.resources.get("cache").unwrap();
    let spec = from_resource("app", "cache", kind).expect("spec built");

    assert_eq!(spec.name, "app_cache");
    assert!(matches!(spec.image, ImageSource::Pull(ref s) if s == "redis:7-alpine"));
    let command = spec.command.as_ref().expect("redis carries a command");
    assert_eq!(command[0], "redis-server");
    assert!(command.iter().any(|s| s == "--requirepass"));
    assert!(command.iter().any(|s| s == "s3cret"));
    assert_eq!(spec.ports[0].container_port, 6379);
}

#[test]
fn container_keeps_explicit_image_and_ports() {
    let manifest = Manifest::parse(MANIFEST).unwrap();
    let kind = manifest.resources.get("api").unwrap();
    let spec = from_resource("app", "api", kind).expect("spec built");

    assert_eq!(spec.name, "app_api");
    assert!(matches!(spec.image, ImageSource::Pull(ref s) if s == "my-org/api:1.0"));
    assert_eq!(spec.env.get("LEVEL").map(String::as_str), Some("info"));
    // Short form 8080 maps host = container.
    assert!(
        spec.ports
            .iter()
            .any(|p| p.container_port == 8080 && p.host_port == 8080)
    );
    // Full form "9090:9090".
    assert!(spec.ports.iter().any(|p| p.container_port == 9090));
}

#[test]
fn dockerfile_produces_build_image_source() {
    let manifest = Manifest::parse(
        r"
project:
  name: app
resources:
  frontend:
    dockerfile:
      context: ./apps/frontend
      target: dev
",
    )
    .unwrap();
    let kind = manifest.resources.get("frontend").unwrap();
    let spec = from_resource("app", "frontend", kind).expect("spec built");

    assert_eq!(spec.name, "app_frontend");
    match spec.image {
        ImageSource::Build {
            context,
            dockerfile,
            target,
            tag,
            ..
        } => {
            assert_eq!(context, "./apps/frontend");
            assert_eq!(dockerfile, "Dockerfile");
            assert_eq!(target.as_deref(), Some("dev"));
            assert_eq!(tag, "lightshuttle/app_frontend:dev");
        }
        ImageSource::Pull(_) => panic!("expected ImageSource::Build, got Pull"),
    }
}
