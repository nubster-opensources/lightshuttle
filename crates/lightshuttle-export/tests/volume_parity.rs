//! The Kubernetes and Helm targets must represent volumes the same way.
//!
//! The export specification states that the Helm templates reach parity with
//! the Kubernetes target. They did not: Helm kept only named volumes and
//! discarded host paths and anonymous volumes, producing a chart that passes
//! `helm lint` and starts a container with nothing mounted.
//!
//! No corpus caught it because no test and no golden file covered a volume
//! that is not named, so nothing ever compared the two targets on the case
//! they disagreed about.

use lightshuttle_export::{Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

/// One stack carrying all three volume sources at once: a named volume, a
/// host path, and the anonymous volume a `postgres` resource resolves to when
/// it persists without naming its volume. `volume: false` is not that case:
/// it declares no volume at all, so the data stays in the container layer.
const EVERY_VOLUME_SOURCE: &str = r"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      volumes:
        - 'cache:/var/cache'
        - './src:/app'
  db:
    postgres:
      version: '16'
      volume: true
";

fn rendered(emitter: &impl Emitter, yaml: &str) -> String {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    let artifacts: ExportArtifacts = emitter.emit(&model).expect("emit succeeds");
    artifacts
        .files
        .iter()
        .map(|file| file.contents.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn kubernetes() -> String {
    rendered(&KubernetesEmitter, EVERY_VOLUME_SOURCE)
}

fn helm() -> String {
    rendered(&HelmEmitter, EVERY_VOLUME_SOURCE)
}

/// Counts how many times a marker occurs, so a target that emits a volume
/// once cannot be confused with one that emits it not at all.
fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn kubernetes_represents_all_three_sources() {
    let rendered = kubernetes();
    assert!(
        rendered.contains("persistentVolumeClaim"),
        "the named volume is missing:\n{rendered}"
    );
    assert!(
        rendered.contains("hostPath"),
        "the host path is missing:\n{rendered}"
    );
    assert!(
        rendered.contains("emptyDir"),
        "the anonymous volume is missing:\n{rendered}"
    );
}

// The defect of issue #301.
#[test]
fn helm_represents_the_host_path() {
    let rendered = helm();
    assert!(
        rendered.contains("hostPath"),
        "the host path was dropped:\n{rendered}"
    );
    assert!(
        rendered.contains("./src"),
        "the host path lost its source:\n{rendered}"
    );
}

#[test]
fn helm_represents_the_anonymous_volume() {
    let rendered = helm();
    assert!(
        rendered.contains("emptyDir"),
        "the anonymous volume was dropped:\n{rendered}"
    );
}

#[test]
fn helm_still_represents_the_named_volume() {
    let rendered = helm();
    assert!(
        rendered.contains("persistentVolumeClaim"),
        "the named volume regressed:\n{rendered}"
    );
}

#[test]
fn both_targets_mount_the_same_number_of_volumes() {
    let kubernetes = occurrences(&kubernetes(), "mountPath:");
    let helm = occurrences(&helm(), "mountPath:");
    assert_eq!(
        kubernetes, helm,
        "the two targets mount a different number of volumes"
    );
    // Two on `api`, one injected on `db`.
    assert_eq!(kubernetes, 3, "expected three mounts across the stack");
}

#[test]
fn both_targets_agree_on_every_volume_name() {
    let names = |rendered: &str| -> Vec<String> {
        rendered
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- name: "))
            .map(str::to_owned)
            .filter(|name| name.starts_with("api-") || name.starts_with("db-"))
            .collect()
    };
    let mut kubernetes = names(&kubernetes());
    let mut helm = names(&helm());
    kubernetes.sort();
    kubernetes.dedup();
    helm.sort();
    helm.dedup();

    assert_eq!(
        kubernetes, helm,
        "the two targets name their volumes differently"
    );
    assert!(
        !kubernetes.is_empty(),
        "the name extraction found nothing, so this test proves nothing"
    );
}

#[test]
fn a_host_path_mount_target_survives_in_helm() {
    let rendered = helm();
    assert!(
        rendered.contains("mountPath: /app"),
        "the host path was not mounted where the manifest asked:\n{rendered}"
    );
}
