//! A literal `{{` in an environment value must reach the deployed
//! `ConfigMap` unchanged, with or without a deployment placeholder in the
//! same service.
//!
//! Helm escapes a literal `{{` to `{{ "{{" }}`, and that escape is itself a
//! Go template action: it comes back as a literal `{{` only because
//! something renders it. Values under `values.yaml` are data, so a chart
//! renders them only where it wires `tpl` in, and it wires `tpl` in only for
//! a service that has a placeholder to substitute. Escaping and templating
//! are therefore one decision taken twice, in the placeholder pass and in
//! the emitter, and the two failure modes are opposite: escape without
//! `tpl` and the escape itself lands in the deployed value; `tpl` without
//! escape and a manifest `{{` becomes a template action at install time.
//!
//! `helm_emitter.rs::helm_escapes_template_braces_to_close_the_injection`
//! covers the same hazard on argv, which the chart splices into its
//! templates rather than routing through `values.yaml`: that path is
//! rendered by Go unconditionally, so it never had this second case.

use std::io::Write as _;

use lightshuttle_export::{Emitter, ExportArtifacts, ExportFile, HelmEmitter, lower};
use lightshuttle_manifest::Manifest;

mod common;

/// A literal `{{` in an environment value, in a service with no
/// `${env...}` reference anywhere: nothing will render this chart's
/// environment.
const PLAIN_STACK: &str = r#"
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
      env:
        LITERAL: "{{ dangerous }}"
"#;

/// The same literal, in a service that also carries a placeholder: this
/// chart's environment goes through `tpl`.
const TEMPLATED_STACK: &str = r#"
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
      env:
        LITERAL: "{{ dangerous }}"
        GREETING: "hello ${env.WHO:-world}"
"#;

fn artifacts(yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    HelmEmitter.emit(&model).expect("emit succeeds")
}

fn file<'a>(artifacts: &'a ExportArtifacts, name: &str) -> &'a str {
    artifacts
        .files
        .iter()
        .find(|candidate| candidate.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing file {name}"))
        .contents
        .as_str()
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

/// Renders the chart with `helm template` and returns its output.
fn helm_template(yaml: &str) -> String {
    let artifacts = artifacts(yaml);
    let dir = write_chart(&artifacts.files);
    let output = std::process::Command::new("helm")
        .arg("template")
        .arg(dir.path().join("chart"))
        .output()
        .expect("helm runs");
    assert!(
        output.status.success(),
        "helm template rejected the chart:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Escape and `tpl` are one decision: a chart that does not template its
/// environment must not carry an escape either, since nothing would turn it
/// back into the `{{` the manifest wrote.
#[test]
fn a_service_without_placeholders_carries_neither_the_escape_nor_tpl() {
    let emitted = artifacts(PLAIN_STACK);
    let values = file(&emitted, "values.yaml");
    let template = file(&emitted, "templates/api.yaml");

    assert!(
        !values.contains(r#"{{ "{{" }}"#),
        "an un-templated value must not be escaped, got:\n{values}"
    );
    assert!(
        !template.contains("tpl $v"),
        "a service with no placeholder must not route its environment through tpl, got:\n{template}"
    );
}

/// The other half of the same decision: a chart that does template its
/// environment must escape, or the manifest's `{{` becomes a template
/// action at install time.
#[test]
fn a_service_with_placeholders_carries_both_the_escape_and_tpl() {
    let emitted = artifacts(TEMPLATED_STACK);
    let values = file(&emitted, "values.yaml");
    let template = file(&emitted, "templates/api.yaml");

    assert!(
        values.contains(r#"{{ "{{" }}"#),
        "a templated value must escape the manifest's literal braces, got:\n{values}"
    );
    assert!(
        template.contains("tpl $v"),
        "a service with a placeholder must route its environment through tpl, got:\n{template}"
    );
}

/// The proof the two assertions above only approximate: what the deployed
/// `ConfigMap` actually holds, according to Helm itself.
///
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn a_literal_brace_survives_helm_template_in_both_shapes() {
    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    for (shape, yaml) in [("plain", PLAIN_STACK), ("templated", TEMPLATED_STACK)] {
        let rendered = helm_template(yaml);
        assert!(
            rendered.contains("{{ dangerous }}"),
            "the {shape} chart must deliver the manifest's literal braces unchanged, got:\n{rendered}"
        );
        assert!(
            !rendered.contains(r#"{{ "{{" }}"#),
            "the {shape} chart leaked its own escape into the deployed value, got:\n{rendered}"
        );
    }

    let templated = helm_template(TEMPLATED_STACK);
    assert!(
        templated.contains("hello world"),
        "the placeholder default must be substituted in the same ConfigMap, got:\n{templated}"
    );
}
