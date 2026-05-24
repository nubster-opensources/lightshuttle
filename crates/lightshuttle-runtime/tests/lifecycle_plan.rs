//! Unit tests for `LifecyclePlan`: topological sort, cycle detection,
//! dependent lookup. No runtime required.

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{LifecycleError, LifecyclePlan};

const TWO_TIER: &str = r"
project:
  name: app
resources:
  db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      depends_on: [db]
";

#[test]
fn builds_in_topological_order() {
    let manifest = Manifest::parse(TWO_TIER).unwrap();
    let plan = LifecyclePlan::from_manifest(&manifest).expect("plan builds");

    let order: Vec<&str> = plan.nodes().iter().map(|n| n.name.as_str()).collect();
    let db_idx = order.iter().position(|n| *n == "db").expect("db present");
    let api_idx = order.iter().position(|n| *n == "api").expect("api present");
    assert!(db_idx < api_idx, "db must come before api, got {order:?}");
}

#[test]
fn lookup_dependents_of_a_resource() {
    let manifest = Manifest::parse(TWO_TIER).unwrap();
    let plan = LifecyclePlan::from_manifest(&manifest).unwrap();

    assert_eq!(plan.dependents_of("db"), vec!["api"]);
    assert!(plan.dependents_of("api").is_empty());
    assert!(plan.dependents_of("unknown").is_empty());
}

#[test]
fn detects_a_cycle() {
    let manifest = Manifest::parse(
        r"
project:
  name: app
resources:
  a:
    container:
      image: alpine
      depends_on: [b]
  b:
    container:
      image: alpine
      depends_on: [a]
",
    );
    // Manifest::parse already rejects cycles in v0; if it ever stops
    // doing so the plan builder must catch them.
    assert!(manifest.is_err(), "manifest parser should reject cycles");
}

#[test]
fn surfaces_spec_build_errors() {
    // Manifest with an invalid port string that survives manifest
    // validation but blows up at spec build time.
    let manifest = Manifest::parse(
        r#"
project:
  name: app
resources:
  api:
    container:
      image: alpine
      ports: ["not-a-port"]
"#,
    )
    .expect("manifest parses (port is a string)");

    let err =
        LifecyclePlan::from_manifest(&manifest).expect_err("plan should fail on invalid port spec");
    assert!(
        matches!(err, LifecycleError::SpecBuild { .. }),
        "got: {err:?}"
    );
}
