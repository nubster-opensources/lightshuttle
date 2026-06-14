//! Tests for the Helm emitter.

use lightshuttle_export::{Emitter, ExportArtifacts, HelmEmitter, lower};
use lightshuttle_manifest::Manifest;

mod common;

const STACK: &str = r"
project:
  name: shop
  version: 1.4.0
export:
  helm:
    chart_name: shop-chart
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
    HelmEmitter.emit(&model).expect("emit succeeds")
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
        file(&a, "Chart.yaml"),
        include_str!("golden/helm/Chart.yaml"),
        "Chart.yaml drifted"
    );
    assert_eq!(
        file(&a, "values.yaml"),
        include_str!("golden/helm/values.yaml"),
        "values.yaml drifted"
    );
    assert_eq!(
        file(&a, "templates/db.yaml"),
        include_str!("golden/helm/db.yaml"),
        "templates/db.yaml drifted"
    );
}

#[test]
fn values_carry_resource_knobs() {
    let values = {
        let a = artifacts(STACK);
        file(&a, "values.yaml").to_owned()
    };
    assert!(values.contains("replicas: 1"));
    assert!(values.contains("repository: postgres"));
    assert!(values.contains("LOG_LEVEL: info"), "env in values");
    assert!(
        values.contains("API_TOKEN: '***'"),
        "secret placeholder in values"
    );
}

#[test]
fn templates_reference_values() {
    let a = artifacts(STACK);
    let db = file(&a, "templates/db.yaml");
    assert!(db.contains(r#"index .Values.services "db""#), "got:\n{db}");
    assert!(db.contains("replicas: {{ $svc.replicas }}"), "got:\n{db}");
    assert!(db.contains("range $k, $v := $svc.env"), "got:\n{db}");
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
fn portless_service_emits_no_helm_service() {
    let a = artifacts(PORTLESS_STACK);
    let worker_template = file(&a, "templates/worker.yaml");
    assert!(
        !worker_template.contains("kind: Service"),
        "worker has no ports so no Service block should be emitted, got:\n{worker_template}"
    );
}

#[test]
fn mixed_env_routes_to_values() {
    let a = artifacts(PORTLESS_STACK);
    let values = file(&a, "values.yaml");
    let worker_template = file(&a, "templates/worker.yaml");
    // DB_URL is plain config: appears in values.yaml env section.
    assert!(
        values.contains("DB_URL"),
        "DB_URL missing from values.yaml, got:\n{values}"
    );
    // DB_PASSWORD matches SECRET_MARKERS: appears as placeholder in secrets section.
    assert!(
        values.contains("DB_PASSWORD: '***'"),
        "DB_PASSWORD placeholder missing from values.yaml, got:\n{values}"
    );
    // Real credential must never appear anywhere.
    assert!(
        !values.contains("s3cret"),
        "real secret value leaked into values.yaml, got:\n{values}"
    );
    // Template wires env and secrets from values.
    assert!(
        worker_template.contains("range $k, $v := $svc.env"),
        "env range missing from worker template, got:\n{worker_template}"
    );
    assert!(
        worker_template.contains("range $k, $v := $svc.secrets"),
        "secrets range missing from worker template, got:\n{worker_template}"
    );
}

#[test]
fn portless_worker_matches_golden() {
    let a = artifacts(PORTLESS_STACK);
    assert_eq!(
        file(&a, "templates/worker.yaml"),
        include_str!("golden/helm/worker.yaml"),
        "templates/worker.yaml drifted from the golden file"
    );
}

/// Validates the generated chart with the real `helm` CLI.
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn output_passes_helm_lint() {
    use std::io::Write;

    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let chart = dir.path().join("chart");
    for f in &artifacts(STACK).files {
        let path = chart.join(&f.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::File::create(&path)
            .and_then(|mut file| file.write_all(f.contents.as_bytes()))
            .expect("write chart file");
    }

    let output = std::process::Command::new("helm")
        .arg("lint")
        .arg(&chart)
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "helm lint rejected the chart:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
