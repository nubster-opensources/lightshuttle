//! End-to-end gate for #308 (export deployment placeholders).
//!
//! These tests drive the crate exclusively through its current public API
//! (`lower` plus the three emitters): none of them reach into the
//! `placeholder` module under construction. Every scenario is the target
//! behaviour described by the design (`2026-09-15-export-deployment-placeholders-design.md`,
//! sections 1, 3 and 6): most fail today because `lower` still passes
//! `${...}` references through verbatim, and turn green once the deployment
//! text renderers land.
//!
//! Tests marked `#[ignore]` drive the real external validators
//! (`docker compose config`, `helm lint`, `helm template`) exactly as the
//! existing ignored tests in `compose_emitter.rs`, `kubernetes_emitter.rs`
//! and `helm_emitter.rs` do: same `common::tool_available` gate, same
//! temp-directory write-then-invoke shape, same graceful skip when the tool
//! is absent from the host.

use std::io::Write as _;

use lightshuttle_export::{
    ComposeEmitter, Emitter, ExportArtifacts, ExportFile, HelmEmitter, KubernetesEmitter, lower,
};
use lightshuttle_manifest::{DnsName, Manifest};

mod common;

// --- Manifests -------------------------------------------------------------

/// The probe manifest from design section 1: an image tag, a working
/// directory and a command argument referencing `${env...}` and
/// `${resources...}`, an environment variable carrying a sensitive resource
/// output, a `${env.NAME:-default}` greeting, a literal `$` and a `${{ }}`
/// escape. `APP_DIR` has no default on purpose: Compose tolerates that,
/// Kubernetes must refuse it (see `K8S_MISSING_DEFAULTS_STACK` below for the
/// dedicated Kubernetes refusal test).
const PROBE_STACK: &str = r#"
project:
  name: probe
resources:
  main_db:
    postgres:
      version: '16'
      password: devsecret
  api:
    container:
      image: "example/api:${env.TAG:-1.0}"
      working_dir: "/srv/${env.APP_DIR}"
      command: [serve, --db, "${resources.main_db.host}"]
      env:
        DATABASE_URL: "${resources.main_db.url}"
        GREETING: "hello ${env.WHO:-world}"
        PRICE: "costs $5"
        LITERAL: "${{ not.a.reference }}"
      depends_on: [main_db]
"#;

/// Every `${env...}` reference carries a default, and the resource
/// reference targets a non-sensitive property, so this manifest must export
/// cleanly on every target, including Kubernetes.
const K8S_ALL_DEFAULTS_STACK: &str = r#"
project:
  name: probe
resources:
  main_db:
    postgres:
      version: '16'
      password: devsecret
  api:
    container:
      image: alpine:3.20
      env:
        GREETING: "hello ${env.WHO:-world}"
      command: [serve, --db, "${resources.main_db.host}"]
      depends_on: [main_db]
"#;

/// Two `${env...}` references with no default: `APP_DIR` and `REGION`.
/// Kubernetes must refuse the export and name both, sorted.
const K8S_MISSING_DEFAULTS_STACK: &str = r#"
project:
  name: probe
resources:
  api:
    container:
      image: alpine:3.20
      working_dir: "/srv/${env.APP_DIR}"
      env:
        REGION: "${env.REGION}"
"#;

/// The motivating Helm failure from design section 1: `ImageReference::parse`
/// splits `example/api:${env.TAG:-1.0}` at the last `:` and chokes on the
/// `-1.0}` tail.
const HELM_IMAGE_STACK: &str = r#"
project:
  name: probe
  version: 1.0.0
export:
  helm:
    chart_name: probe-chart
resources:
  api:
    container:
      image: "example/api:${env.TAG:-1.0}"
"#;

/// A literal `{{ ... }}` in a resolved argument, unrelated to the `${{ }}`
/// escape: the dangerous case where Helm's Go templater must not interpret
/// it as a template action.
const HELM_INJECTION_STACK: &str = r#"
project:
  name: probe
  version: 1.0.0
export:
  helm:
    chart_name: probe-chart
resources:
  api:
    container:
      image: alpine:3.20
      command: ["echo", "{{ dangerous }}"]
"#;

/// `DATABASE_URL` is not a name any `SECRET_MARKERS` heuristic would catch;
/// only D4 (propagation from a sensitive resource output) can redact it.
/// Every `${env...}` reference carries a default, so all three targets
/// accept the export and there is a full set of artifacts to scan.
const SECURITY_STACK: &str = r#"
project:
  name: probe
resources:
  main_db:
    postgres:
      version: '16'
      password: devsecret
  api:
    container:
      image: alpine:3.20
      env:
        DATABASE_URL: "${resources.main_db.url}"
"#;

/// `command` (not `env`) references a sensitive property: D4 must refuse
/// the export outright rather than let the password land in clear text in
/// an argv array.
const COMMAND_SENSITIVE_STACK: &str = r#"
project:
  name: probe
resources:
  main_db:
    postgres:
      version: '16'
      password: devsecret
  api:
    container:
      image: alpine:3.20
      command: ["connect", "${resources.main_db.password}"]
"#;

// --- Helpers -----------------------------------------------------------

fn manifest(yaml: &str) -> Manifest {
    Manifest::parse(yaml).expect("manifest parses")
}

fn compose_artifacts(yaml: &str) -> lightshuttle_export::Result<ExportArtifacts> {
    ComposeEmitter.emit(&lower(&manifest(yaml))?)
}

fn kubernetes_artifacts(yaml: &str) -> lightshuttle_export::Result<ExportArtifacts> {
    KubernetesEmitter.emit(&lower(&manifest(yaml))?)
}

fn helm_artifacts(yaml: &str) -> lightshuttle_export::Result<ExportArtifacts> {
    HelmEmitter.emit(&lower(&manifest(yaml))?)
}

fn file<'a>(files: &'a [ExportFile], name: &str) -> &'a str {
    files
        .iter()
        .find(|f| f.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing file {name}, got: {files:#?}"))
        .contents
        .as_str()
}

/// The DNS label an export target uses for `resource`, computed the same
/// way `lightshuttle_export::resolve::dns_name` does, so file names and
/// resolved hosts are asserted against a real value instead of a guessed
/// hash suffix.
fn dns_label(resource: &str) -> String {
    DnsName::from_manifest_name(resource)
        .unwrap_or_else(|err| panic!("{resource} is a valid manifest name: {err}"))
        .as_str()
        .to_owned()
}

/// Writes `contents` as `docker-compose.yml` in a fresh temp directory and
/// returns the directory (keep it alive for the duration of the test) and
/// the file path.
fn write_compose_file(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("docker-compose.yml");
    let mut compose_file = std::fs::File::create(&path).expect("write compose");
    compose_file
        .write_all(contents.as_bytes())
        .expect("write bytes");
    (dir, path)
}

/// Writes every artifact under `<tempdir>/chart/` and returns the temp
/// directory (keep it alive for the duration of the test).
fn write_chart(files: &[ExportFile]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let chart = dir.path().join("chart");
    for exported in files {
        let path = chart.join(&exported.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::File::create(&path)
            .and_then(|mut chart_file| chart_file.write_all(exported.contents.as_bytes()))
            .expect("write chart file");
    }
    dir
}

// --- Compose -------------------------------------------------------------

/// Rendering table row 1: `${env.TAG:-1.0}` becomes the bare `${TAG:-1.0}`
/// Compose already understands, never the `${env.TAG:-1.0}` form Compose
/// rejects.
#[test]
fn compose_image_variable_drops_env_prefix() {
    let artifacts = compose_artifacts(PROBE_STACK).expect("compose lowering and emission succeed");
    let compose = file(&artifacts.files, "docker-compose.yml");
    assert!(
        compose.contains("${TAG:-1.0}"),
        "image should carry the bare Compose variable ${{TAG:-1.0}}, got:\n{compose}"
    );
    assert!(
        !compose.contains("${env.TAG:-1.0}"),
        "image must not carry the manifest-grammar ${{env.TAG:-1.0}} form, got:\n{compose}"
    );
}

/// Rendering table row 3: `${resources.main_db.host}` is resolved at
/// export time to the raw Compose service name, not left as a reference for
/// Compose to interpolate (Compose has no notion of `resources.*`).
#[test]
fn compose_resource_host_reference_resolves_to_raw_service_name() {
    let artifacts = compose_artifacts(PROBE_STACK).expect("compose lowering and emission succeed");
    let compose = file(&artifacts.files, "docker-compose.yml");
    assert!(
        !compose.contains("${resources.main_db.host}"),
        "the resource reference must be resolved, not left verbatim, got:\n{compose}"
    );
    // Checked as a distinct command list item, not a bare substring: the
    // service key `main_db:` also contains "main_db" and would otherwise
    // make this assertion pass regardless of what the command resolves to.
    assert!(
        compose.contains("- main_db\n"),
        "command should carry the raw Compose service name main_db as its own argv element, got:\n{compose}"
    );
}

/// The real gate: `docker compose config` against the emitted file.
/// Today this fails with `invalid interpolation format for
/// services.api.image.` (measured on main 017fe90, design section 1)
/// because the image still carries the manifest-grammar `${env.TAG:-1.0}`
/// form Compose does not understand. It also exercises the `$` and
/// `${{ }}` escapes and the resolved resource host end to end, via the
/// actual interpolation engine rather than a hand-written oracle.
///
/// Ignored by default: it needs Docker Compose on the host.
#[test]
#[ignore = "requires docker compose on the host"]
fn compose_output_passes_docker_compose_config() {
    if !common::tool_available("docker") {
        eprintln!("skipping: docker not found on PATH");
        return;
    }

    let artifacts = compose_artifacts(PROBE_STACK).expect("compose lowering and emission succeed");
    let compose = file(&artifacts.files, "docker-compose.yml");
    let (_dir, path) = write_compose_file(compose);

    let output = std::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(&path)
        .arg("config")
        .env("APP_DIR", "app")
        .env("WHO", "tester")
        .env("DATABASE_URL", "resolved-db-url")
        .output()
        .expect("docker compose runs");

    assert!(
        output.status.success(),
        "docker compose config rejected the output:\n{}\n---\n{}",
        String::from_utf8_lossy(&output.stderr),
        compose
    );

    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(
        rendered.contains("example/api:1.0"),
        "image should resolve to the default tag 1.0, got:\n{rendered}"
    );
    assert!(
        rendered.contains("/srv/app"),
        "working_dir should resolve APP_DIR from the environment, got:\n{rendered}"
    );
    // Checked as a distinct command list item for the same reason as
    // `compose_resource_host_reference_resolves_to_raw_service_name`: the
    // service key `main_db:` also contains "main_db" as a bare substring.
    assert!(
        rendered.contains("- main_db\n") || rendered.contains("- main_db\r\n"),
        "command should carry the raw service name main_db as its own argv element, got:\n{rendered}"
    );
    assert!(
        rendered.contains("resolved-db-url"),
        "DATABASE_URL should resolve to the value supplied at config time, got:\n{rendered}"
    );
    assert!(
        rendered.contains("hello tester"),
        "GREETING should resolve WHO from the environment, got:\n{rendered}"
    );
    assert!(
        rendered.contains("$5"),
        "the literal $ in \"costs $5\" must survive interpolation, got:\n{rendered}"
    );
    assert!(
        rendered.contains("{ not.a.reference }"),
        "the ${{{{ }}}} escape must survive interpolation as a literal, got:\n{rendered}"
    );
}

// --- Kubernetes ------------------------------------------------------------

/// Rendering table: every `${env...}` reference here carries a default, so
/// Kubernetes must accept the export (unlike `PROBE_STACK`, whose `APP_DIR`
/// has none).
#[test]
fn kubernetes_all_defaults_manifest_exports_successfully() {
    let artifacts = kubernetes_artifacts(K8S_ALL_DEFAULTS_STACK).unwrap_or_else(|err| {
        panic!(
            "a manifest where every ${{env...}} reference has a default must export to Kubernetes, got: {err:?}"
        )
    });

    // Accepting the export is not enough on its own: an exporter that
    // validates nothing also accepts everything. What must be observed is
    // that the default was frozen into the artifact, since plain Kubernetes
    // substitutes nothing at deploy time.
    let rendered = artifacts
        .files
        .iter()
        .map(|exported| exported.contents.as_str())
        .collect::<String>();
    assert!(
        rendered.contains("hello world"),
        "the `${{env.WHO:-world}}` default must be frozen in the manifest, got:
{rendered}"
    );
    assert!(
        !rendered.contains("${env."),
        "no `${{env...}}` reference may survive into a plain Kubernetes manifest, got:
{rendered}"
    );
}

/// D1: an `${env.NAME}` reference with no default must refuse the whole
/// export, naming every missing variable (sorted), not just the first one
/// found.
#[test]
fn kubernetes_missing_defaults_are_all_reported_sorted() {
    let result = kubernetes_artifacts(K8S_MISSING_DEFAULTS_STACK);
    let Err(err) = result else {
        panic!(
            "a manifest with APP_DIR and REGION missing their default must refuse the Kubernetes export"
        );
    };
    let message = err.to_string();
    assert!(
        message.contains("APP_DIR"),
        "error should name APP_DIR, got: {message}"
    );
    assert!(
        message.contains("REGION"),
        "error should name REGION, got: {message}"
    );
    let app_dir_at = message.find("APP_DIR").expect("checked above");
    let region_at = message.find("REGION").expect("checked above");
    assert!(
        app_dir_at < region_at,
        "variables should be listed sorted (APP_DIR before REGION), got: {message}"
    );
}

/// D2: `${resources.main_db.host}` renders as the DNS name Kubernetes gives
/// the `main_db` resource, not the raw manifest name and not the reference
/// left verbatim.
#[test]
fn kubernetes_resource_host_reference_resolves_to_dns_name() {
    let artifacts =
        kubernetes_artifacts(K8S_ALL_DEFAULTS_STACK).expect("Kubernetes export succeeds");
    let api = file(&artifacts.files, "api.yaml");
    assert!(
        !api.contains("${resources.main_db.host}"),
        "the resource reference must be resolved, not left verbatim, got:\n{api}"
    );
    let expected_host = dns_label("main_db");
    assert!(
        api.contains(&expected_host),
        "command should carry the DNS name {expected_host} of main_db, got:\n{api}"
    );
}

/// The real gate: `kubeconform` against every emitted manifest, mirroring
/// `kubernetes_emitter.rs::output_passes_kubeconform` exactly (same stdin
/// piping, same `-strict -summary -` invocation). Ignored by default: it
/// needs `kubeconform` on the host.
#[test]
#[ignore = "requires kubeconform on the host"]
fn kubernetes_all_defaults_output_passes_kubeconform() {
    if !common::tool_available("kubeconform") {
        eprintln!("skipping: kubeconform not found on PATH");
        return;
    }

    let artifacts =
        kubernetes_artifacts(K8S_ALL_DEFAULTS_STACK).expect("Kubernetes export succeeds");
    for exported in &artifacts.files {
        let output = std::process::Command::new("kubeconform")
            .args(["-strict", "-summary", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child
                    .stdin
                    .take()
                    .expect("stdin")
                    .write_all(exported.contents.as_bytes())?;
                child.wait_with_output()
            })
            .expect("kubeconform runs");
        assert!(
            output.status.success(),
            "kubeconform rejected {}:\n{}\n{}",
            exported.path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// --- Helm --------------------------------------------------------------

/// The motivating Helm failure from design section 1: today `HelmEmitter`
/// fails with `invalid image tag '-1.0}'` because `ImageReference::parse`
/// splits the image at the last `:` before the placeholder is resolved.
#[test]
fn helm_image_with_env_placeholder_and_default_exports_successfully() {
    let result = helm_artifacts(HELM_IMAGE_STACK);
    assert!(
        result.is_ok(),
        "an image tag of the form ${{env.TAG:-1.0}} must export to Helm, got: {:?}",
        result.err()
    );
}

/// The real gate: `helm lint` against the emitted chart, mirroring
/// `helm_emitter.rs::output_passes_helm_lint` exactly (same chart layout,
/// same invocation). Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn helm_output_passes_helm_lint() {
    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let artifacts = helm_artifacts(HELM_IMAGE_STACK)
        .unwrap_or_else(|err| panic!("helm emit should succeed before lint can run, got: {err}"));
    let dir = write_chart(&artifacts.files);
    let chart = dir.path().join("chart");

    let output = std::process::Command::new("helm")
        .arg("lint")
        .arg(&chart)
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "helm lint rejected the chart:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `helm template` with no override renders the chart's default tag, `1.0`.
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn helm_template_default_renders_tag_1_0() {
    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let artifacts = helm_artifacts(HELM_IMAGE_STACK).unwrap_or_else(|err| {
        panic!("helm emit should succeed before template can run, got: {err}")
    });
    let dir = write_chart(&artifacts.files);
    let chart = dir.path().join("chart");

    let output = std::process::Command::new("helm")
        .arg("template")
        .arg(&chart)
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "helm template rejected the chart:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(
        rendered.contains("example/api:1.0"),
        "helm template with no override should render tag 1.0, got:\n{rendered}"
    );
}

/// `helm template --set variables.TAG=2.0` overrides the tag to `2.0`.
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn helm_template_set_override_renders_tag_2_0() {
    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let artifacts = helm_artifacts(HELM_IMAGE_STACK).unwrap_or_else(|err| {
        panic!("helm emit should succeed before template can run, got: {err}")
    });
    let dir = write_chart(&artifacts.files);
    let chart = dir.path().join("chart");

    let output = std::process::Command::new("helm")
        .arg("template")
        .arg(&chart)
        .args(["--set", "variables.TAG=2.0"])
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "helm template rejected the chart:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(
        rendered.contains("example/api:2.0"),
        "helm template --set variables.TAG=2.0 should render tag 2.0, got:\n{rendered}"
    );
}

/// The most dangerous point of the lot (design section 8): a resolved
/// argument that happens to contain a literal `{{ ... }}` must not be
/// interpreted as a Go template action by `helm template`, and must come
/// back unchanged in the rendered manifest. `helm_emitter.rs` already
/// carries a unit-level oracle for this
/// (`helm_escapes_template_braces_to_close_the_injection`); this is the
/// same property proven against the real templater instead of a
/// hand-written inverse-escape oracle.
///
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn helm_template_literal_double_braces_survive_end_to_end() {
    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let artifacts = helm_artifacts(HELM_INJECTION_STACK).unwrap_or_else(|err| {
        panic!("helm emit should succeed before template can run, got: {err}")
    });
    let dir = write_chart(&artifacts.files);
    let chart = dir.path().join("chart");

    let output = std::process::Command::new("helm")
        .arg("template")
        .arg(&chart)
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "a literal {{{{ in an argument must not break helm template:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(
        rendered.contains("{{ dangerous }}"),
        "the literal {{{{ dangerous }}}} must survive Go templating unchanged, got:\n{rendered}"
    );
}

// --- Security (D4), all three targets ---------------------------------

/// D4: `DATABASE_URL: ${resources.main_db.url}` carries a sensitive
/// property (`url`, which embeds the password) through a key name no
/// `SECRET_MARKERS` heuristic would catch. The real password must never
/// appear in clear text in any file emitted for any of the three targets:
/// a negative assertion over every artifact, not just the one place a leak
/// would be expected.
#[test]
fn security_password_reference_never_leaks_in_clear_across_all_targets() {
    const PASSWORD: &str = "devsecret";
    const REFERENCE: &str = "${resources.main_db.url}";

    let compose = compose_artifacts(SECURITY_STACK).expect("compose export succeeds");
    let kubernetes = kubernetes_artifacts(SECURITY_STACK).expect("kubernetes export succeeds");
    let helm = helm_artifacts(SECURITY_STACK).expect("helm export succeeds");

    for (target, artifacts) in [
        ("compose", &compose),
        ("kubernetes", &kubernetes),
        ("helm", &helm),
    ] {
        // Positive half, and the reason this test is not vacuous: an exporter
        // that resolves nothing leaks nothing either, so the absence of the
        // password below only means something once the reference is gone and
        // the key it fed actually reached the artifacts.
        assert!(
            !artifacts.files.is_empty(),
            "{target} exported no file, so the absence of the password proves nothing"
        );
        assert!(
            artifacts
                .files
                .iter()
                .any(|exported| exported.contents.contains("DATABASE_URL")),
            "{target} never emitted DATABASE_URL, so this test cannot observe the leak it guards"
        );
        for exported in &artifacts.files {
            assert!(
                !exported.contents.contains(REFERENCE),
                "{target} artifact {} left the reference unresolved, which makes the password assertion vacuous:
{}",
                exported.path.display(),
                exported.contents
            );
        }

        // Negative half: the credential appears in no artifact at all, not
        // merely in the one file where it was expected.
        for exported in &artifacts.files {
            assert!(
                !exported.contents.contains(PASSWORD),
                "{target} artifact {} leaks the password in clear text:
{}",
                exported.path.display(),
                exported.contents
            );
        }
    }
}

/// D4's other half: a sensitive reference outside of an environment
/// variable (here, an argv element) has nowhere safe to hide the value, so
/// the export must be refused outright rather than emit the password in an
/// argv array.
#[test]
fn security_command_referencing_password_outside_env_is_refused() {
    let result = compose_artifacts(COMMAND_SENSITIVE_STACK);
    let Err(err) = result else {
        panic!(
            "a command referencing ${{resources.main_db.password}} must be refused, not exported"
        );
    };
    let message = err.to_string();
    assert!(
        message.to_ascii_lowercase().contains("password"),
        "refusal should name the offending sensitive property, got: {message}"
    );
}
