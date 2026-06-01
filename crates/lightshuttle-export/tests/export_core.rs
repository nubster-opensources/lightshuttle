//! Tests for the export core: lowering, per-target resolution and the
//! emitter contract.

use lightshuttle_export::resolve::{
    chart_name_for, chart_version_for, enabled_for, image_pull_policy_for, namespace_for,
    replicas_for,
};
use lightshuttle_export::{Emitter, ExportArtifacts, ExportModel, Target, lower};
use lightshuttle_manifest::{ImagePullPolicy, Manifest};
use lightshuttle_spec::ImageSource;

const STACK: &str = r"
project:
  name: shop
  version: 2.0.0
resources:
  db:
    postgres:
      version: '16'
      volume: dbdata
  api:
    container:
      image: alpine
      depends_on: [db]
";

const STACK_WITH_OVERRIDES: &str = r"
project:
  name: shop
export:
  kubernetes:
    namespace: prod
    replicas: 2
    image_pull_policy: Always
    resources:
      api:
        replicas: 5
  helm:
    chart_name: shop-chart
    chart_version: 9.9.9
  compose:
    resources:
      worker:
        enabled: false
resources:
  api:
    container:
      image: alpine
  worker:
    container:
      image: alpine
";

fn parse(yaml: &str) -> Manifest {
    Manifest::parse(yaml).expect("manifest parses")
}

#[test]
fn lower_resolves_services_with_defaults_and_dependencies() {
    let model = lower(&parse(STACK)).expect("lowering succeeds");

    assert_eq!(model.project.name, "shop");
    assert_eq!(model.project.version.as_deref(), Some("2.0.0"));
    assert_eq!(model.services.len(), 2);

    // The postgres service inherits the spec defaults: image, port, env
    // and a named volume are all carried into the model.
    let db = &model.services[0];
    assert_eq!(db.spec.resource, "db");
    assert!(
        matches!(&db.spec.image, ImageSource::Pull(img) if img == "postgres:16-alpine"),
        "postgres image should resolve to the spec default, got {:?}",
        db.spec.image
    );
    assert!(
        db.spec.ports.iter().any(|p| p.container_port == 5432),
        "postgres port should be lowered, got {:?}",
        db.spec.ports
    );
    assert!(
        db.spec.env.contains_key("POSTGRES_DB"),
        "postgres env should be lowered, got {:?}",
        db.spec.env
    );
    assert!(
        !db.spec.volumes.is_empty(),
        "named volume should be lowered, got {:?}",
        db.spec.volumes
    );
    assert!(db.depends_on.is_empty());

    let api = &model.services[1];
    assert_eq!(api.spec.resource, "api");
    assert_eq!(api.depends_on, vec!["db".to_owned()]);
}

#[test]
fn resolve_applies_defaults_without_export_section() {
    let model = lower(&parse(STACK)).expect("lowering succeeds");
    let export = model.export.as_ref();

    assert!(enabled_for(Target::Kubernetes, "api", export));
    assert_eq!(replicas_for(Target::Kubernetes, "api", export), 1);
    assert_eq!(namespace_for(&model.project.name, export), "shop");
    assert_eq!(
        image_pull_policy_for("api", export),
        ImagePullPolicy::IfNotPresent
    );
    assert_eq!(chart_name_for(&model.project.name, export), "shop");
    // Project version becomes the chart version default.
    assert_eq!(
        chart_version_for(model.project.version.as_deref(), export),
        "2.0.0"
    );
}

#[test]
fn resolve_honours_export_overrides() {
    let model = lower(&parse(STACK_WITH_OVERRIDES)).expect("lowering succeeds");
    let export = model.export.as_ref();

    // Per-resource override wins over the per-target default.
    assert_eq!(replicas_for(Target::Kubernetes, "api", export), 5);
    // A resource without its own override inherits the target default.
    assert_eq!(replicas_for(Target::Kubernetes, "worker", export), 2);
    assert_eq!(namespace_for(&model.project.name, export), "prod");
    assert_eq!(
        image_pull_policy_for("api", export),
        ImagePullPolicy::Always
    );
    assert_eq!(chart_name_for(&model.project.name, export), "shop-chart");
    assert_eq!(
        chart_version_for(model.project.version.as_deref(), export),
        "9.9.9"
    );
    // Compose disables worker; api stays enabled.
    assert!(!enabled_for(Target::Compose, "worker", export));
    assert!(enabled_for(Target::Compose, "api", export));
}

/// Minimal emitter that lists enabled service names, used to exercise
/// the trait and the artifact container.
struct ListEmitter;

impl Emitter for ListEmitter {
    fn target(&self) -> Target {
        Target::Compose
    }

    fn emit(&self, model: &ExportModel) -> lightshuttle_export::Result<ExportArtifacts> {
        let mut artifacts = ExportArtifacts::new();
        let names: Vec<&str> = model
            .services
            .iter()
            .filter(|s| enabled_for(self.target(), &s.spec.resource, model.export.as_ref()))
            .map(|s| s.spec.resource.as_str())
            .collect();
        artifacts.push("services.txt", names.join("\n"));
        Ok(artifacts)
    }
}

#[test]
fn emitter_contract_produces_named_files() {
    let model = lower(&parse(STACK_WITH_OVERRIDES)).expect("lowering succeeds");
    let artifacts = ListEmitter.emit(&model).expect("emit succeeds");

    assert_eq!(artifacts.files.len(), 1);
    let file = &artifacts.files[0];
    assert_eq!(file.path.to_str(), Some("services.txt"));
    // worker is disabled for compose, so only api is listed.
    assert_eq!(file.contents, "api");
}
