//! Round-trip tests: parse a manifest, re-serialise it, parse again, and
//! confirm the two parsed values are equal.

use lightshuttle_manifest::{Manifest, ResourceKind};

const HELLO_WORLD: &str = include_str!("fixtures/hello-world.yml");
const REAL_WORLD: &str = include_str!("fixtures/real-world.yml");

#[test]
fn parses_hello_world() {
    let manifest = Manifest::parse(HELLO_WORLD).expect("hello-world fixture should parse");
    assert_eq!(manifest.project.name, "hello");
    assert_eq!(manifest.resources.len(), 2);
    assert!(matches!(
        manifest.resources.get("db"),
        Some(ResourceKind::Postgres(_))
    ));
    assert!(matches!(
        manifest.resources.get("app"),
        Some(ResourceKind::Container(_))
    ));
}

#[test]
fn parses_real_world() {
    let manifest = Manifest::parse(REAL_WORLD).expect("real-world fixture should parse");
    assert_eq!(manifest.project.name, "my-app");
    assert_eq!(manifest.project.version.as_deref(), Some("0.1.0"));
    assert_eq!(manifest.resources.len(), 4);
}

#[test]
fn round_trip_hello_world() {
    let original = Manifest::parse(HELLO_WORLD).unwrap();
    let yaml = original.to_yaml().expect("to_yaml should succeed");
    let reparsed = Manifest::parse(&yaml).expect("re-parse should succeed");
    assert_eq!(original, reparsed);
}

#[test]
fn round_trip_real_world() {
    let original = Manifest::parse(REAL_WORLD).unwrap();
    let yaml = original.to_yaml().expect("to_yaml should succeed");
    let reparsed = Manifest::parse(&yaml).expect("re-parse should succeed");
    assert_eq!(original, reparsed);
}

#[test]
fn preserves_resource_order() {
    let manifest = Manifest::parse(REAL_WORLD).unwrap();
    let names: Vec<&str> = manifest.resources.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["cache", "api_db", "api", "frontend"]);
}
