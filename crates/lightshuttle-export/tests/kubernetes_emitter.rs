//! Tests for the Kubernetes emitter.

use lightshuttle_export::{Emitter, ExportArtifacts, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

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
}

/// Validates the emitted manifests with the real `kubectl` CLI.
/// Ignored by default: it needs kubectl on the host.
#[test]
#[ignore = "requires kubectl on the host"]
fn output_passes_kubectl_dry_run() {
    let a = artifacts(STACK);
    for f in &a.files {
        let output = std::process::Command::new("kubectl")
            .args(["apply", "--dry-run=client", "-f", "-"])
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
            .expect("kubectl runs");
        assert!(
            output.status.success(),
            "kubectl rejected {}:\n{}",
            f.path.display(),
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
