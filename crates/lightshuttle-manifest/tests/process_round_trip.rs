//! Round-trip tests for the `process` resource kind.
//!
//! Mirrors `round_trip.rs`, but scoped to the one behaviour that file does
//! not cover: a `process:` resource entry must deserialise, re-serialise,
//! and re-deserialise to the same value, exactly like every other resource
//! kind.

use lightshuttle_manifest::{Manifest, ProcessConfig, ResourceKind};

const PROCESS_ALL_FIELDS: &str = r#"
project:
  name: app
resources:
  db:
    postgres:
      version: "16"
  worker:
    process:
      command: ["node", "server.js"]
      working_dir: "./services/worker"
      env:
        NODE_ENV: production
      port: 4000
      depends_on: [db]
"#;

#[test]
fn process_resource_deserialises_from_yaml() {
    let manifest =
        Manifest::parse(PROCESS_ALL_FIELDS).expect("manifest with a process resource should parse");

    let mut expected = ProcessConfig::new(vec!["node".to_owned(), "server.js".to_owned()]);
    expected.working_dir = Some("./services/worker".to_owned());
    expected
        .env
        .insert("NODE_ENV".to_owned(), "production".to_owned());
    expected.port = Some(4000);
    expected.depends_on = vec!["db".to_owned()];

    assert_eq!(
        manifest.resources.get("worker"),
        Some(&ResourceKind::Process(expected)),
        "the process resource must deserialise into the expected ProcessConfig"
    );
}

#[test]
fn process_resource_round_trips_through_yaml_serialisation() {
    let original =
        Manifest::parse(PROCESS_ALL_FIELDS).expect("manifest with a process resource should parse");
    let yaml = original.to_yaml().expect("to_yaml should succeed");
    let reparsed = Manifest::parse(&yaml).expect("re-parse should succeed");
    assert_eq!(
        original, reparsed,
        "a process resource must survive a serialise/deserialise round trip like every other kind"
    );
}

/// `ResourceKind::deserialize` selects a variant by comparing the single
/// YAML key of a resource entry against a string literal
/// (`"postgres"`, `"redis"`, `"container"`, `"dockerfile"`, `"process"`).
/// Every other place that enumerates the kinds (`kind_name`, `depends_on`,
/// `healthcheck`, `interpolatable_fields_mut`, the manual `Serialize` impl,
/// and `lightshuttle-spec`'s `resolve`) is a `match` over the enum itself,
/// so the compiler refuses to build once a variant is added until every arm
/// names it. This string comparison has no such guard: nothing stops
/// `"process"` from being silently left out of the match and falling
/// through to the `other => Err("unknown resource kind")` catch-all. This
/// test is the only thing in the workspace that would notice.
#[test]
fn process_key_is_recognised_by_deserialisation() {
    let manifest = Manifest::parse(PROCESS_ALL_FIELDS)
        .expect("the `process` key must be recognised by deserialisation");

    assert!(
        manifest.resources.contains_key("worker"),
        "the parsed manifest must hold the declared process resource"
    );
}

#[test]
fn process_resource_kind_name_is_process() {
    let kind = ResourceKind::Process(ProcessConfig::new(vec!["node".to_owned()]));
    assert_eq!(kind.kind_name(), "process");
}
