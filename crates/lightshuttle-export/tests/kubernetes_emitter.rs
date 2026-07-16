//! Tests for the Kubernetes emitter.

use lightshuttle_export::{Emitter, ExportArtifacts, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

mod common;

const STACK: &str = r"
project:
  name: shop
export:
  kubernetes:
    namespace: prod
    replicas: 2
    resources:
      api:
        replicas: 3
resources:
  db:
    postgres:
      version: '16'
      password: devsecret
      volume: dbdata
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine:3.20
      ports:
        - 8080:80
      env:
        LOG_LEVEL: info
        API_TOKEN: t0ken
      depends_on: [db]
";

fn artifacts(yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    KubernetesEmitter.emit(&model).expect("emit succeeds")
}

fn file<'a>(artifacts: &'a ExportArtifacts, name: &str) -> &'a str {
    let found = artifacts
        .files
        .iter()
        .find(|f| f.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing file {name}"));
    found.contents.as_str()
}

#[test]
fn matches_golden_files() {
    let a = artifacts(STACK);
    assert_eq!(
        file(&a, "namespace.yaml"),
        include_str!("golden/k8s/namespace.yaml"),
        "namespace.yaml drifted from the golden file"
    );
    assert_eq!(
        file(&a, "db.yaml"),
        include_str!("golden/k8s/db.yaml"),
        "db.yaml drifted from the golden file"
    );
    assert_eq!(
        file(&a, "cache.yaml"),
        include_str!("golden/k8s/cache.yaml"),
        "cache.yaml drifted from the golden file"
    );
}

/// Validates the emitted manifests with `kubeconform`, an offline
/// schema validator. Ignored by default: it needs kubeconform on the
/// host. `kubectl --dry-run=client` is avoided because it contacts the
/// cluster API server to fetch the `OpenAPI` schema, which makes it
/// non-deterministic in CI.
#[test]
#[ignore = "requires kubeconform on the host"]
fn output_passes_kubeconform() {
    if !common::tool_available("kubeconform") {
        eprintln!("skipping: kubeconform not found on PATH");
        return;
    }

    let a = artifacts(STACK);
    for f in &a.files {
        let output = std::process::Command::new("kubeconform")
            .args(["-strict", "-summary", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child
                    .stdin
                    .take()
                    .expect("stdin")
                    .write_all(f.contents.as_bytes())?;
                child.wait_with_output()
            })
            .expect("kubeconform runs");
        assert!(
            output.status.success(),
            "kubeconform rejected {}:\n{}\n{}",
            f.path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn emits_namespace_and_per_resource_files() {
    let a = artifacts(STACK);
    let names: Vec<&str> = a.files.iter().filter_map(|f| f.path.to_str()).collect();
    assert!(names.contains(&"namespace.yaml"), "got {names:?}");
    assert!(names.contains(&"db.yaml"), "got {names:?}");
    assert!(names.contains(&"api.yaml"), "got {names:?}");
}

#[test]
fn namespace_override_is_applied() {
    let a = artifacts(STACK);
    assert!(file(&a, "namespace.yaml").contains("name: prod"));
    assert!(file(&a, "db.yaml").contains("namespace: prod"));
}

#[test]
fn per_resource_replica_override_wins() {
    let a = artifacts(STACK);
    assert!(file(&a, "api.yaml").contains("replicas: 3"), "api override");
    assert!(
        file(&a, "db.yaml").contains("replicas: 2"),
        "target default"
    );
}

#[test]
fn secret_keys_route_to_secret_others_to_configmap() {
    let a = artifacts(STACK);
    let api = file(&a, "api.yaml");
    // API_TOKEN matches a secret marker, LOG_LEVEL does not.
    assert!(api.contains("kind: Secret"), "got:\n{api}");
    assert!(api.contains("API_TOKEN"), "got:\n{api}");
    assert!(api.contains("kind: ConfigMap"), "got:\n{api}");
    assert!(api.contains("LOG_LEVEL"), "got:\n{api}");
}

#[test]
fn postgres_gets_probe_and_pvc() {
    let a = artifacts(STACK);
    let db = file(&a, "db.yaml");
    assert!(db.contains("readinessProbe"), "got:\n{db}");
    assert!(db.contains("pg_isready"), "got:\n{db}");
    assert!(db.contains("kind: PersistentVolumeClaim"), "got:\n{db}");
}

// --- portless + split_env characterisation tests ---

const PORTLESS_STACK: &str = r"
project:
  name: shop
resources:
  worker:
    container:
      image: alpine:3.20
      env:
        DB_URL: postgres://db:5432/app
        DB_PASSWORD: s3cret
";

#[test]
fn portless_service_emits_no_k8s_service() {
    let a = artifacts(PORTLESS_STACK);
    let worker = file(&a, "worker.yaml");
    assert!(
        !worker.contains("kind: Service"),
        "worker has no ports so no Service should be emitted, got:\n{worker}"
    );
}

#[test]
fn mixed_env_routes_to_secret_and_configmap() {
    let a = artifacts(PORTLESS_STACK);
    let worker = file(&a, "worker.yaml");
    // DB_PASSWORD matches SECRET_MARKERS -> Secret, value replaced.
    assert!(
        worker.contains("kind: Secret"),
        "missing Secret, got:\n{worker}"
    );
    assert!(
        worker.contains("DB_PASSWORD"),
        "missing DB_PASSWORD key, got:\n{worker}"
    );
    // DB_URL is plain config -> ConfigMap.
    assert!(
        worker.contains("kind: ConfigMap"),
        "missing ConfigMap, got:\n{worker}"
    );
    assert!(
        worker.contains("DB_URL"),
        "missing DB_URL key, got:\n{worker}"
    );
    // Real credential must never appear in the exported manifest.
    assert!(
        !worker.contains("s3cret"),
        "real secret value leaked into manifest, got:\n{worker}"
    );
}

#[test]
fn portless_worker_matches_golden() {
    let a = artifacts(PORTLESS_STACK);
    assert_eq!(
        file(&a, "worker.yaml"),
        include_str!("golden/k8s/worker.yaml"),
        "worker.yaml drifted from the golden file"
    );
}

/// The resolved `command` is Docker's `Cmd`. Kubernetes calls that `args`;
/// its `command` field is Docker's `ENTRYPOINT`. Emitting the resolved
/// command as `command` replaces the image entrypoint, which makes
/// `lightshuttle up` and `lightshuttle export kubernetes` disagree.
#[test]
fn emits_resolved_command_as_args_not_command() {
    let a = artifacts(STACK);
    let cache = file(&a, "cache.yaml");
    assert!(
        cache.contains("        args:\n        - redis-server\n"),
        "cache args missing, got:\n{cache}"
    );
    assert!(
        !cache.contains("        command:\n        - redis-server\n"),
        "redis-server must be args, not command, got:\n{cache}"
    );
}
