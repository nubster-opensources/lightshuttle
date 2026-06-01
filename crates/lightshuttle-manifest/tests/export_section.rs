//! Tests for the optional `export` manifest section: parsing,
//! round-trip and resource-reference validation.

use lightshuttle_manifest::{ImagePullPolicy, Manifest, ManifestError};

const WITH_EXPORT: &str = r"
project:
  name: app
export:
  compose:
    resources:
      worker:
        enabled: false
  kubernetes:
    namespace: my-ns
    image_pull_policy: Always
    replicas: 2
    resources:
      api:
        replicas: 3
        image_pull_policy: IfNotPresent
  helm:
    chart_name: my-chart
    chart_version: 1.2.3
    resources:
      api:
        replicas: 4
resources:
  api:
    container:
      image: alpine
  worker:
    container:
      image: alpine
";

#[test]
fn parses_export_section() {
    let manifest = Manifest::parse(WITH_EXPORT).expect("manifest with export parses");
    let export = manifest.export.expect("export section present");

    let kubernetes = export.kubernetes.expect("kubernetes target present");
    assert_eq!(kubernetes.namespace.as_deref(), Some("my-ns"));
    assert_eq!(kubernetes.image_pull_policy, Some(ImagePullPolicy::Always));
    assert_eq!(kubernetes.replicas, Some(2));
    assert_eq!(kubernetes.resources["api"].replicas, Some(3));

    let helm = export.helm.expect("helm target present");
    assert_eq!(helm.chart_name.as_deref(), Some("my-chart"));
    assert_eq!(helm.chart_version.as_deref(), Some("1.2.3"));

    let compose = export.compose.expect("compose target present");
    assert_eq!(compose.resources["worker"].enabled, Some(false));
}

#[test]
fn round_trips_export_section() {
    let original = Manifest::parse(WITH_EXPORT).expect("parse");
    let yaml = original.to_yaml().expect("to_yaml");
    let reparsed = Manifest::parse(&yaml).expect("re-parse");
    assert_eq!(original, reparsed);
}

#[test]
fn absent_export_is_none() {
    let yaml = r"
project:
  name: app
resources:
  api:
    container:
      image: alpine
";
    let manifest = Manifest::parse(yaml).expect("parse");
    assert!(manifest.export.is_none(), "absent export must be None");
}

#[test]
fn rejects_override_for_unknown_resource() {
    let yaml = r"
project:
  name: app
export:
  kubernetes:
    resources:
      ghost:
        replicas: 2
resources:
  api:
    container:
      image: alpine
";
    let err = Manifest::parse(yaml).expect_err("unknown override resource is rejected");
    assert!(
        matches!(err, ManifestError::UnknownResource(ref m) if m.contains("ghost")),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_field_in_export() {
    let yaml = r"
project:
  name: app
export:
  kubernetes:
    bogus: true
resources:
  api:
    container:
      image: alpine
";
    let err = Manifest::parse(yaml).expect_err("unknown field is rejected");
    assert!(matches!(err, ManifestError::Yaml(_)), "got: {err:?}");
}
