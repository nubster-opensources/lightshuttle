//! A Windows host path must reach every target as a bind, not as a volume.
//!
//! The defect was invisible in the Compose output, because reassembling
//! `source:target` rebuilds the original string whichever way it was split.
//! It only surfaced in the semantic types: a spurious top level volume named
//! `C`, and a `PersistentVolumeClaim` where a `hostPath` belonged.

use lightshuttle_export::{
    ComposeEmitter, Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower,
};
use lightshuttle_manifest::Manifest;

/// A manifest whose host path is drive qualified, as it is on Windows once
/// relative paths have been resolved against the manifest directory.
const DRIVE_QUALIFIED_STACK: &str = r"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      volumes:
        - 'C:\project\data:/data'
";

/// The same stack with a genuine named volume, as a control.
const NAMED_VOLUME_STACK: &str = r"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      volumes:
        - 'cache:/data'
";

fn emit(emitter: &impl Emitter, yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    emitter.emit(&model).expect("emit succeeds")
}

fn joined(artifacts: &ExportArtifacts) -> String {
    artifacts
        .files
        .iter()
        .map(|file| file.contents.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn compose_does_not_declare_a_volume_named_after_the_drive_letter() {
    let rendered = joined(&emit(&ComposeEmitter, DRIVE_QUALIFIED_STACK));
    let control = joined(&emit(&ComposeEmitter, NAMED_VOLUME_STACK));

    assert!(
        control.contains("volumes:"),
        "the control stack must declare its named volume"
    );
    assert!(
        !rendered.contains("\n  C:"),
        "a top level volume `C` was declared:\n{rendered}"
    );
}

#[test]
fn compose_keeps_the_drive_qualified_source_intact() {
    let rendered = joined(&emit(&ComposeEmitter, DRIVE_QUALIFIED_STACK));
    assert!(
        rendered.contains(r"C:\project\data:/data"),
        "the bind lost its drive qualified source:\n{rendered}"
    );
}

#[test]
fn kubernetes_emits_a_host_path_rather_than_a_claim() {
    let rendered = joined(&emit(&KubernetesEmitter, DRIVE_QUALIFIED_STACK));
    assert!(
        rendered.contains("hostPath"),
        "expected a hostPath volume:\n{rendered}"
    );
    assert!(
        !rendered.contains("PersistentVolumeClaim"),
        "a claim was emitted for a host path:\n{rendered}"
    );
}

#[test]
fn kubernetes_emits_a_claim_for_a_genuine_named_volume() {
    let rendered = joined(&emit(&KubernetesEmitter, NAMED_VOLUME_STACK));
    assert!(
        rendered.contains("PersistentVolumeClaim"),
        "the control stack must still produce a claim:\n{rendered}"
    );
}

// Helm cannot be asserted the same way: it drops non named volumes entirely,
// which is issue #301. What it must not do, and what this patch is about, is
// mistake the drive letter for a volume name and emit a claim called `c`.
#[test]
fn helm_does_not_emit_a_claim_named_after_the_drive_letter() {
    let rendered = joined(&emit(&HelmEmitter, DRIVE_QUALIFIED_STACK));
    assert!(
        !rendered.contains("PersistentVolumeClaim"),
        "a claim was emitted for a host path:\n{rendered}"
    );
}

#[test]
fn a_mount_option_is_refused_before_any_artifact_is_produced() {
    let yaml = r"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      volumes:
        - './data:/app:ro'
";
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let error = lower(&manifest).expect_err("a third field is refused");

    // The CLI renders errors with `{err:#}`, so the diagnostic the user reads
    // is the whole chain, not just the outermost message.
    let mut chain = error.to_string();
    let mut cause: Option<&dyn std::error::Error> = std::error::Error::source(&error);
    while let Some(current) = cause {
        chain.push_str(": ");
        chain.push_str(&current.to_string());
        cause = current.source();
    }

    assert!(
        chain.contains("ro"),
        "the diagnostic must name the unsupported field, got `{chain}`"
    );
}
