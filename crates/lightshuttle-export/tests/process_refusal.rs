//! A manifest holding `process` resources must be refused by every export
//! target, naming every offending resource at once.
//!
//! Manifests here are built directly through the manifest model rather than
//! parsed from YAML, bypassing `Manifest::parse`. `Manifest::validate` is
//! not exercised by these tests as a result; that is deliberate, since the
//! subject under test is the export pipeline's refusal, not manifest
//! validation (see `lightshuttle-manifest/tests/process_validation.rs` for
//! that).

use indexmap::IndexMap;
use lightshuttle_export::{
    ComposeEmitter, Emitter, ExportError, HelmEmitter, KubernetesEmitter, lower,
};
use lightshuttle_manifest::{Manifest, ProcessConfig, Project, ResourceKind};

fn manifest_with_processes(names: &[&str]) -> Manifest {
    let mut resources = IndexMap::new();
    for name in names {
        resources.insert(
            (*name).to_owned(),
            ResourceKind::Process(ProcessConfig::new(vec!["true".to_owned()])),
        );
    }
    Manifest {
        lightshuttle: None,
        project: Project {
            name: "shop".to_owned(),
            version: None,
            description: None,
        },
        dashboard: None,
        observability: None,
        export: None,
        resources,
    }
}

#[test]
fn single_process_resource_is_refused_for_compose_export() {
    let manifest = manifest_with_processes(&["worker"]);
    let error = lower(&manifest)
        .and_then(|model| ComposeEmitter.emit(&model))
        .expect_err("a manifest holding a process resource must be refused for compose export");
    assert!(
        matches!(&error, ExportError::ProcessNotExportable { resources } if resources.iter().any(|r| r == "worker")),
        "got: {error}"
    );
}

#[test]
fn single_process_resource_is_refused_for_kubernetes_export() {
    let manifest = manifest_with_processes(&["worker"]);
    let error = lower(&manifest)
        .and_then(|model| KubernetesEmitter.emit(&model))
        .expect_err("a manifest holding a process resource must be refused for kubernetes export");
    assert!(
        matches!(&error, ExportError::ProcessNotExportable { resources } if resources.iter().any(|r| r == "worker")),
        "got: {error}"
    );
}

#[test]
fn single_process_resource_is_refused_for_helm_export() {
    let manifest = manifest_with_processes(&["worker"]);
    let error = lower(&manifest)
        .and_then(|model| HelmEmitter.emit(&model))
        .expect_err("a manifest holding a process resource must be refused for helm export");
    assert!(
        matches!(&error, ExportError::ProcessNotExportable { resources } if resources.iter().any(|r| r == "worker")),
        "got: {error}"
    );
}

/// The central regression guard: a refusal that names only the first process
/// resource it happens to reach, rather than every one of them sorted, must
/// fail this test.
#[test]
fn refusal_names_every_process_resource_at_once() {
    let manifest = manifest_with_processes(&["zeta", "alpha", "mid"]);

    let error = lower(&manifest).expect_err(
        "a manifest holding process resources must be refused before any artifact is emitted",
    );

    let ExportError::ProcessNotExportable { resources } = error else {
        panic!("expected ExportError::ProcessNotExportable, got a different variant");
    };

    assert_eq!(
        resources,
        vec!["alpha".to_owned(), "mid".to_owned(), "zeta".to_owned()],
        "every process resource must be named at once, sorted, in a single message"
    );
}

/// Control case: a manifest with no `process` resource must keep exporting
/// normally on every target.
#[test]
fn manifest_without_process_resources_exports_normally() {
    let manifest = Manifest::parse(
        r"
project:
  name: shop
resources:
  api:
    container:
      image: alpine
",
    )
    .expect("a process-free manifest must parse");

    let model = lower(&manifest).expect("lowering a process-free manifest must succeed");
    ComposeEmitter
        .emit(&model)
        .expect("compose export must succeed for a process-free manifest");
    KubernetesEmitter
        .emit(&model)
        .expect("kubernetes export must succeed for a process-free manifest");
    HelmEmitter
        .emit(&model)
        .expect("helm export must succeed for a process-free manifest");
}
