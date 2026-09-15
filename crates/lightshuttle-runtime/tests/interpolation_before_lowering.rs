//! Every field advertised as interpolatable must be resolved before the
//! container is lowered and started (#276).
//!
//! The manifest below carries a `${env.*}` reference in each interpolatable
//! field. The values are provided through the manager's env, so the spec the
//! runtime actually starts must contain the concrete values, never a literal
//! `${...}`.

use std::collections::HashMap;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{LifecycleManager, LifecyclePlan};
use lightshuttle_spec::ImageSource;

const MANIFEST: &str = r#"
project:
  name: t
resources:
  app:
    container:
      image: "example/app:${env.TAG}"
      env:
        GREETING: "hello ${env.WHO}"
      volumes:
        - "${env.DATA}:/data"
      entrypoint: "${env.ENTRY}"
      command: ["serve", "--port", "${env.PORT}"]
      working_dir: "${env.WORKDIR}"
      healthcheck:
        test: ["CMD", "curl", "${env.HEALTH}"]
"#;

fn env() -> HashMap<String, String> {
    HashMap::from([
        ("TAG".to_owned(), "v9".to_owned()),
        ("WHO".to_owned(), "world".to_owned()),
        ("DATA".to_owned(), "/srv/data".to_owned()),
        ("ENTRY".to_owned(), "/bin/run".to_owned()),
        ("PORT".to_owned(), "8080".to_owned()),
        ("WORKDIR".to_owned(), "/app".to_owned()),
        ("HEALTH".to_owned(), "http://localhost".to_owned()),
    ])
}

#[tokio::test]
async fn every_interpolatable_field_is_resolved_before_the_container_starts() {
    let manifest = Manifest::parse(MANIFEST).expect("manifest parses");
    let plan = LifecyclePlan::from_manifest(&manifest).expect("plan builds");
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);
    let manager = manager.with_env(env());

    manager.start_all().await.expect("stack starts");

    let specs = observer.started_specs();
    let spec = specs
        .iter()
        .find(|s| s.resource == "app")
        .expect("app must have been started");

    // Image reference resolved.
    match &spec.image {
        ImageSource::Pull(reference) => assert_eq!(reference, "example/app:v9"),
        ImageSource::Build { .. } => panic!("expected a pulled image, got a build"),
    }

    // Environment value resolved.
    assert_eq!(
        spec.env.get("GREETING").map(String::as_str),
        Some("hello world")
    );

    // Working directory resolved.
    assert_eq!(spec.working_dir.as_deref(), Some("/app"));

    // Command arguments resolved (a `Command::Args` list passes through as-is).
    assert_eq!(
        spec.command.as_deref(),
        Some(&["serve".to_owned(), "--port".to_owned(), "8080".to_owned()][..])
    );

    // Entrypoint resolved (previously omitted from substitution entirely).
    let entrypoint = spec.entrypoint.as_ref().expect("entrypoint set");
    assert!(
        entrypoint.iter().any(|part| part == "/bin/run"),
        "entrypoint must be resolved, got {entrypoint:?}"
    );

    // Catch-all: no field of the started spec may retain a literal `${...}`,
    // which also covers the volume mapping and the healthcheck test command.
    let rendered = format!("{spec:?}");
    assert!(
        !rendered.contains("${"),
        "the started spec must not retain any unresolved interpolation: {rendered}"
    );
    assert!(
        rendered.contains("/srv/data"),
        "the volume host path must be resolved: {rendered}"
    );
    assert!(
        rendered.contains("http://localhost"),
        "the healthcheck test command must be resolved: {rendered}"
    );
}
