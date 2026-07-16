//! Validation tests: confirm that invalid manifests are rejected with
//! the expected error variant.

use lightshuttle_manifest::{Manifest, ManifestError};

const CYCLE: &str = include_str!("fixtures/invalid/cycle.yml");

#[test]
fn rejects_cycle_in_dependency_graph() {
    let err = Manifest::parse(CYCLE).expect_err("cycle should be rejected");
    assert!(matches!(err, ManifestError::Cycle(_)), "got: {err:?}");
}

#[test]
fn rejects_resource_name_starting_with_digit() {
    let yaml = r"
project:
  name: app
resources:
  1bad:
    container:
      image: alpine
";
    let err = Manifest::parse(yaml).expect_err("bad name should be rejected");
    assert!(
        matches!(err, ManifestError::InvalidName { .. }),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_resource_kind() {
    let yaml = r"
project:
  name: app
resources:
  foo:
    unknown_kind:
      ignored: true
";
    let err = Manifest::parse(yaml).expect_err("unknown kind should be rejected");
    assert!(matches!(err, ManifestError::Yaml(_)), "got: {err:?}");
}

#[test]
fn rejects_unknown_dependency() {
    let yaml = r"
project:
  name: app
resources:
  foo:
    container:
      image: alpine
      depends_on: [bar]
";
    let err = Manifest::parse(yaml).expect_err("unknown dep should be rejected");
    assert!(
        matches!(err, ManifestError::UnknownResource(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_resource_reference_in_interpolation() {
    let yaml = r"
project:
  name: app
resources:
  app:
    container:
      image: alpine
      env:
        URL: ${resources.missing.url}
";
    let err = Manifest::parse(yaml).expect_err("missing ref should be rejected");
    assert!(
        matches!(err, ManifestError::UnknownResource(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_invalid_database_name() {
    let yaml = r"
project:
  name: app
resources:
  db:
    postgres:
      database: '123bad'
";
    let err = Manifest::parse(yaml).expect_err("bad database name should be rejected");
    assert!(
        matches!(err, ManifestError::InvalidName { .. }),
        "got: {err:?}"
    );
}

#[test]
fn rejects_invalid_duration_in_healthcheck() {
    let yaml = r#"
project:
  name: app
resources:
  app:
    container:
      image: alpine
      healthcheck:
        test: ["CMD", "true"]
        interval: "not-a-duration"
"#;
    let err = Manifest::parse(yaml).expect_err("bad duration should be rejected");
    assert!(
        matches!(err, ManifestError::InvalidDuration(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_overly_long_database_name() {
    let long = "a".repeat(64);
    let yaml = format!(
        "
project:
  name: app
resources:
  db:
    postgres:
      database: '{long}'
"
    );
    let err = Manifest::parse(&yaml).expect_err("64 byte database name should be rejected");
    assert!(
        matches!(err, ManifestError::InvalidName { .. }),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_reference_in_command() {
    let yaml = r"
project:
  name: app
resources:
  app:
    container:
      image: alpine
      command: ${resources.missing.url}
";
    let err = Manifest::parse(yaml).expect_err("missing ref in command should be rejected");
    assert!(
        matches!(err, ManifestError::UnknownResource(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_reference_in_healthcheck() {
    let yaml = r#"
project:
  name: app
resources:
  app:
    container:
      image: alpine
      healthcheck:
        test: ["CMD-SHELL", "check ${resources.missing.host}"]
        interval: "5s"
"#;
    let err = Manifest::parse(yaml).expect_err("missing ref in healthcheck should be rejected");
    assert!(
        matches!(err, ManifestError::UnknownResource(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_interpolation_cycle_between_resources() {
    let yaml = r#"
project:
  name: app
resources:
  a:
    container:
      image: alpine
      env:
        X: "${resources.b.host}"
  b:
    container:
      image: alpine
      env:
        Y: "${resources.a.host}"
"#;
    let err = Manifest::parse(yaml).expect_err("interpolation cycle should be rejected");
    assert!(matches!(err, ManifestError::Cycle(_)), "got: {err:?}");
}

#[test]
fn empty_entrypoint_is_rejected() {
    let yaml = r"
project:
  name: app
resources:
  svc:
    dockerfile:
      context: .
      entrypoint: []
";
    let err = Manifest::parse(yaml).expect_err("empty entrypoint must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("svc") && message.contains("entrypoint"),
        "the message must name the resource and the field, got: {message}"
    );
}

#[test]
fn accepts_omitted_lightshuttle_discriminator() {
    let yaml = r"
project:
  name: app
resources:
  app:
    container:
      image: alpine
";
    let manifest = Manifest::parse(yaml).expect("missing discriminator should default to v0");
    assert!(manifest.lightshuttle.is_none());
}
