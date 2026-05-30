//! Cross-check: the generated JSON Schema accepts every fixture that
//! the parser accepts. Guards against drift between Rust types,
//! schema generation and the documented examples.

use lightshuttle_manifest::{Manifest, schema};

const HELLO_WORLD: &str = include_str!("fixtures/hello-world.yml");
const REAL_WORLD: &str = include_str!("fixtures/real-world.yml");

fn validate(fixture: &str, label: &str) {
    let schema_value = serde_json::to_value(schema()).expect("schema serialisable to JSON value");
    let validator = jsonschema::validator_for(&schema_value).expect("schema compiles");

    let manifest = Manifest::parse(fixture).expect("fixture parses");
    let manifest_value =
        serde_json::to_value(&manifest).expect("manifest serialisable to JSON value");

    if !validator.is_valid(&manifest_value) {
        for err in validator.iter_errors(&manifest_value) {
            eprintln!("validation error in {label}: {err}");
        }
        panic!("{label} should validate against the generated schema");
    }
}

#[test]
fn hello_world_matches_schema() {
    validate(HELLO_WORLD, "hello-world");
}

#[test]
fn real_world_matches_schema() {
    validate(REAL_WORLD, "real-world");
}
