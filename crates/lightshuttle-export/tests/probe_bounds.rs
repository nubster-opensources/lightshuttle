//! A healthcheck the manifest accepts must produce a probe the Kubernetes API
//! accepts.
//!
//! The golden corpus next door only ever exercises whole second durations, so
//! it could never have caught this: the flooring it hides only shows up below
//! one second. These tests assert the emitted numbers, not the layout.

use lightshuttle_export::{Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

/// Every probe value Kubernetes constrains to be at least one, driven to the
/// value that used to floor to zero.
const SUB_SECOND_STACK: &str = r#"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      healthcheck:
        test: ["CMD", "true"]
        interval: 200ms
        timeout: 100ms
        start_period: 50ms
        retries: 0
"#;

fn kubernetes(yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    KubernetesEmitter.emit(&model).expect("emit succeeds")
}

fn helm(yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    HelmEmitter.emit(&model).expect("emit succeeds")
}

fn contents(artifacts: &ExportArtifacts, name: &str) -> String {
    artifacts
        .files
        .iter()
        .find(|file| file.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing {name}"))
        .contents
        .clone()
}

/// Reads every occurrence of a probe field out of a rendered document.
///
/// The Helm templates are Go templates rather than data, so they cannot be
/// parsed as YAML. Scanning for the field keeps the assertion on the values
/// rather than on the surrounding layout.
fn probe_values(document: &str, field: &str) -> Vec<i64> {
    document
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix(field)?.strip_prefix(':')?;
            rest.trim().parse().ok()
        })
        .collect()
}

#[test]
fn kubernetes_never_emits_a_zero_probe_period() {
    let artifacts = kubernetes(SUB_SECOND_STACK);
    let document = contents(&artifacts, "api.yaml");

    for field in ["periodSeconds", "timeoutSeconds", "failureThreshold"] {
        let values = probe_values(&document, field);
        assert!(!values.is_empty(), "`{field}` is not emitted at all");
        assert!(
            values.iter().all(|value| *value >= 1),
            "`{field}` emitted {values:?}, and the Kubernetes API requires at least 1"
        );
    }
}

#[test]
fn helm_never_emits_a_zero_probe_period() {
    let artifacts = helm(SUB_SECOND_STACK);
    let document = contents(&artifacts, "templates/api.yaml");

    for field in ["periodSeconds", "timeoutSeconds", "failureThreshold"] {
        let values = probe_values(&document, field);
        assert!(!values.is_empty(), "`{field}` is not emitted at all");
        assert!(
            values.iter().all(|value| *value >= 1),
            "`{field}` emitted {values:?}, and the Kubernetes API requires at least 1"
        );
    }
}

/// Rounding up keeps the emitted probe at least as patient as the manifest
/// asked. A whole second value must still pass through untouched.
#[test]
fn a_whole_second_healthcheck_is_emitted_verbatim() {
    let yaml = r#"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      healthcheck:
        test: ["CMD", "true"]
        interval: 7s
        timeout: 4s
        retries: 2
"#;
    let artifacts = kubernetes(yaml);
    let document = contents(&artifacts, "api.yaml");

    assert!(
        probe_values(&document, "periodSeconds")
            .iter()
            .all(|v| *v == 7)
    );
    assert!(
        probe_values(&document, "timeoutSeconds")
            .iter()
            .all(|v| *v == 4)
    );
    assert!(
        probe_values(&document, "failureThreshold")
            .iter()
            .all(|v| *v == 2)
    );
}
