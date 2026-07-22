//! Two manifest names that used to converge must produce two artifacts.
//!
//! The defect was silent: the artifact list is written in order, so the second
//! file simply overwrote the first and the export reported success. Nothing
//! here asserts formatting; the assertions are about how many distinct outputs
//! exist and which namespace they carry.

use lightshuttle_export::{Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

/// Two resources whose names differ only by the separator. Both are valid
/// under the manifest name grammar, and both used to normalise to `foo-bar`.
const COLLIDING_STACK: &str = r"
project:
  name: shop
resources:
  foo_bar:
    container:
      image: alpine:3.20
  foo-bar:
    container:
      image: alpine:3.20
";

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

fn paths(artifacts: &ExportArtifacts) -> Vec<String> {
    artifacts
        .files
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn kubernetes_emits_one_file_per_colliding_resource() {
    let artifacts = kubernetes(COLLIDING_STACK);
    let paths = paths(&artifacts);
    let unique: std::collections::BTreeSet<&String> = paths.iter().collect();

    assert_eq!(
        paths.len(),
        unique.len(),
        "two resources overwrote each other: {paths:?}"
    );
    // namespace.yaml plus one file per resource.
    assert_eq!(paths.len(), 3, "expected both resources emitted: {paths:?}");
}

#[test]
fn helm_emits_one_template_per_colliding_resource() {
    let artifacts = helm(COLLIDING_STACK);
    let templates: Vec<String> = paths(&artifacts)
        .into_iter()
        .filter(|path| path.starts_with("templates/"))
        .collect();
    let unique: std::collections::BTreeSet<&String> = templates.iter().collect();

    assert_eq!(
        templates.len(),
        unique.len(),
        "two resources shared a template: {templates:?}"
    );
    assert_eq!(templates.len(), 2, "got {templates:?}");
}

/// The `values.yaml` map is keyed by normalised name, so a collision there
/// dropped one service before anything was even written.
#[test]
fn helm_values_keep_both_colliding_services() {
    let artifacts = helm(COLLIDING_STACK);
    let values = artifacts
        .files
        .iter()
        .find(|file| file.path.to_str() == Some("values.yaml"))
        .expect("values.yaml is emitted");
    let parsed: serde_norway::Value =
        serde_norway::from_str(&values.contents).expect("values.yaml is valid YAML");

    let services = parsed["services"]
        .as_mapping()
        .expect("services is a mapping");
    assert_eq!(services.len(), 2, "one service was dropped: {services:?}");
}

// --- Namespace (#284) --------------------------------------------------------

#[test]
fn a_project_name_that_is_not_a_label_yields_a_valid_namespace() {
    let yaml = "project:\n  name: my_project\nresources:\n  api:\n    container:\n      image: alpine:3.20\n";
    let artifacts = kubernetes(yaml);
    let namespace_doc = artifacts
        .files
        .iter()
        .find(|file| file.path.to_str() == Some("namespace.yaml"))
        .expect("namespace.yaml is emitted");
    let parsed: serde_norway::Value =
        serde_norway::from_str(&namespace_doc.contents).expect("valid YAML");
    let name = parsed["metadata"]["name"]
        .as_str()
        .expect("namespace has a name");

    assert!(
        lightshuttle_manifest::canonical::is_dns_label(name),
        "`my_project` produced the namespace `{name}`, which Kubernetes rejects"
    );
}

#[test]
fn a_valid_explicit_namespace_is_used_verbatim() {
    let yaml = "project:\n  name: shop\nexport:\n  kubernetes:\n    namespace: prod-eu\nresources:\n  api:\n    container:\n      image: alpine:3.20\n";
    let artifacts = kubernetes(yaml);
    let namespace_doc = artifacts
        .files
        .iter()
        .find(|file| file.path.to_str() == Some("namespace.yaml"))
        .expect("namespace.yaml is emitted");
    let parsed: serde_norway::Value =
        serde_norway::from_str(&namespace_doc.contents).expect("valid YAML");

    assert_eq!(parsed["metadata"]["name"].as_str(), Some("prod-eu"));
}

/// An override is a deliberate value. Rewriting it silently would deploy into
/// a namespace the user never wrote and cannot predict.
#[test]
fn an_invalid_explicit_namespace_is_rejected_rather_than_rewritten() {
    let yaml = "project:\n  name: shop\nexport:\n  kubernetes:\n    namespace: 'My Project'\nresources:\n  api:\n    container:\n      image: alpine:3.20\n";
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let error = KubernetesEmitter
        .emit(&model)
        .expect_err("`My Project` is not a valid Kubernetes namespace");

    assert!(
        error.to_string().contains("namespace"),
        "the diagnostic should point at the namespace, got: {error}"
    );
}
