//! Tests for the Helm emitter.

use lightshuttle_export::{Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower};
use lightshuttle_manifest::Manifest;

mod common;

const STACK: &str = r"
project:
  name: shop
  version: 1.4.0
export:
  helm:
    chart_name: shop-chart
resources:
  db:
    postgres:
      version: '16'
      password: devsecret
      volume: dbdata
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine:3.20
      ports:
        - 8080:80
      env:
        LOG_LEVEL: info
        API_TOKEN: t0ken
      depends_on: [db]
";

fn artifacts(yaml: &str) -> ExportArtifacts {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    HelmEmitter.emit(&model).expect("emit succeeds")
}

fn file<'a>(artifacts: &'a ExportArtifacts, name: &str) -> &'a str {
    let found = artifacts
        .files
        .iter()
        .find(|f| f.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing file {name}"));
    found.contents.as_str()
}

#[test]
fn matches_golden_files() {
    let a = artifacts(STACK);
    assert_eq!(
        file(&a, "Chart.yaml"),
        include_str!("golden/helm/Chart.yaml"),
        "Chart.yaml drifted"
    );
    assert_eq!(
        file(&a, "values.yaml"),
        include_str!("golden/helm/values.yaml"),
        "values.yaml drifted"
    );
    assert_eq!(
        file(&a, "templates/db.yaml"),
        include_str!("golden/helm/db.yaml"),
        "templates/db.yaml drifted"
    );
    assert_eq!(
        file(&a, "templates/cache.yaml"),
        include_str!("golden/helm/cache.yaml"),
        "cache.yaml drifted from the golden file"
    );
}

#[test]
fn values_carry_resource_knobs() {
    let values = {
        let a = artifacts(STACK);
        file(&a, "values.yaml").to_owned()
    };
    assert!(values.contains("replicas: 1"));
    assert!(values.contains("repository: postgres"));
    assert!(values.contains("LOG_LEVEL: info"), "env in values");
    assert!(
        values.contains("API_TOKEN: '***'"),
        "secret placeholder in values"
    );
}

#[test]
fn templates_reference_values() {
    let a = artifacts(STACK);
    let db = file(&a, "templates/db.yaml");
    assert!(db.contains(r#"index .Values.services "db""#), "got:\n{db}");
    assert!(db.contains("replicas: {{ $svc.replicas }}"), "got:\n{db}");
    assert!(db.contains("range $k, $v := $svc.env"), "got:\n{db}");
}

// --- portless + split_env characterisation tests ---

const PORTLESS_STACK: &str = r"
project:
  name: shop
resources:
  worker:
    container:
      image: alpine:3.20
      env:
        DB_URL: postgres://db:5432/app
        DB_PASSWORD: s3cret
";

#[test]
fn portless_service_emits_no_helm_service() {
    let a = artifacts(PORTLESS_STACK);
    let worker_template = file(&a, "templates/worker.yaml");
    assert!(
        !worker_template.contains("kind: Service"),
        "worker has no ports so no Service block should be emitted, got:\n{worker_template}"
    );
}

#[test]
fn mixed_env_routes_to_values() {
    let a = artifacts(PORTLESS_STACK);
    let values = file(&a, "values.yaml");
    let worker_template = file(&a, "templates/worker.yaml");
    // DB_URL is plain config: appears in values.yaml env section.
    assert!(
        values.contains("DB_URL"),
        "DB_URL missing from values.yaml, got:\n{values}"
    );
    // DB_PASSWORD matches SECRET_MARKERS: appears as placeholder in secrets section.
    assert!(
        values.contains("DB_PASSWORD: '***'"),
        "DB_PASSWORD placeholder missing from values.yaml, got:\n{values}"
    );
    // Real credential must never appear anywhere.
    assert!(
        !values.contains("s3cret"),
        "real secret value leaked into values.yaml, got:\n{values}"
    );
    // Template wires env and secrets from values.
    assert!(
        worker_template.contains("range $k, $v := $svc.env"),
        "env range missing from worker template, got:\n{worker_template}"
    );
    assert!(
        worker_template.contains("range $k, $v := $svc.secrets"),
        "secrets range missing from worker template, got:\n{worker_template}"
    );
}

#[test]
fn portless_worker_matches_golden() {
    let a = artifacts(PORTLESS_STACK);
    assert_eq!(
        file(&a, "templates/worker.yaml"),
        include_str!("golden/helm/worker.yaml"),
        "templates/worker.yaml drifted from the golden file"
    );
}

/// The resolved `command` is Docker's `Cmd`, which Kubernetes calls
/// `args`. The redis resource resolves to `["redis-server"]`; before this
/// test existed, the Helm emitter dropped it entirely.
#[test]
fn emits_resolved_command_as_args() {
    let a = artifacts(STACK);
    let cache = file(&a, "templates/cache.yaml");
    assert!(
        cache.contains("        args:\n        - redis-server\n"),
        "cache args missing, got:\n{cache}"
    );
    assert!(
        !cache.contains("        command:\n        - redis-server\n"),
        "redis-server must be args, not command: command is the entrypoint in Kubernetes, got:\n{cache}"
    );
}

/// Extracts the `- item` lines directly following the `header` line
/// (e.g. `"        command:\n"`), stopping at the first line that is not
/// a list item at the same indentation. Used to compare the argv block
/// written by the Helm emitter (hand-written text) against the same
/// block written by the Kubernetes emitter (typed struct through
/// serde), independently of the rest of the surrounding document.
fn argv_block<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let after = text.split(header).nth(1).unwrap_or_else(|| {
        panic!("header {header:?} not found in:\n{text}");
    });
    after
        .lines()
        .take_while(|line| line.starts_with("        - "))
        .collect()
}

/// The motivating case of #261: a redis `--requirepass s3cr:t` argument
/// contains a colon followed by a space, which YAML would otherwise
/// parse as a mapping key rather than part of the scalar. Both emitters
/// must agree on how this argument is quoted, since `up` and
/// `export kubernetes`/`export helm` describe the same container.
///
/// An argument containing `{{` is deliberately NOT exercised here: as
/// of fix wave 2, the two emitters diverge on that input on purpose
/// (see `helm_escapes_template_braces_to_close_the_injection` below and
/// `kubernetes_does_not_escape_template_braces` in
/// `kubernetes_emitter.rs`), so it would make this cross-emitter
/// equality check fail for the wrong reason.
#[test]
fn helm_quotes_scalars_like_the_kubernetes_emitter() {
    let yaml = r"
project:
  name: shop
resources:
  svc:
    container:
      image: alpine:3.20
      entrypoint: ['sh', '-c']
      command: ['echo a: b']
";
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let helm = HelmEmitter.emit(&model).expect("helm emit succeeds");
    let kubernetes = KubernetesEmitter
        .emit(&model)
        .expect("kubernetes emit succeeds");

    let helm_svc = helm
        .files
        .iter()
        .find(|f| f.path.to_str() == Some("templates/svc.yaml"))
        .expect("helm template emitted")
        .contents
        .as_str();
    let kubernetes_svc = kubernetes
        .files
        .iter()
        .find(|f| f.path.to_str() == Some("svc.yaml"))
        .expect("kubernetes manifest emitted")
        .contents
        .as_str();

    assert_eq!(
        argv_block(helm_svc, "        command:\n"),
        argv_block(kubernetes_svc, "        command:\n"),
        "Helm command: block quoted differently from Kubernetes, got helm:\n{helm_svc}\nkubernetes:\n{kubernetes_svc}"
    );
    assert_eq!(
        argv_block(helm_svc, "        args:\n"),
        argv_block(kubernetes_svc, "        args:\n"),
        "Helm args: block quoted differently from Kubernetes, got helm:\n{helm_svc}\nkubernetes:\n{kubernetes_svc}"
    );

    // The colon-space argument is the ambiguous one: unquoted, YAML
    // would parse it as a mapping key rather than as a scalar.
    let args = argv_block(helm_svc, "        args:\n");
    assert!(
        args.contains(&"        - 'echo a: b'"),
        "the colon-space argument must be quoted, got:\n{helm_svc}"
    );
}

/// Validates the generated chart with the real `helm` CLI.
/// Ignored by default: it needs Helm on the host.
#[test]
#[ignore = "requires helm on the host"]
fn output_passes_helm_lint() {
    use std::io::Write;

    if !common::tool_available("helm") {
        eprintln!("skipping: helm not found on PATH");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let chart = dir.path().join("chart");
    for f in &artifacts(STACK).files {
        let path = chart.join(&f.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::File::create(&path)
            .and_then(|mut file| file.write_all(f.contents.as_bytes()))
            .expect("write chart file");
    }

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

/// Same translation as the Kubernetes emitter: a chart is Kubernetes.
#[test]
fn entrypoint_becomes_helm_command_and_command_becomes_args() {
    let yaml = r"
project:
  name: shop
  version: 1.4.0
export:
  helm:
    chart_name: shop-chart
resources:
  svc:
    container:
      image: alpine:3.20
      entrypoint: ['sh', '-c']
      command: ['echo hi']
";
    let a = artifacts(yaml);
    let svc = file(&a, "templates/svc.yaml");
    assert!(
        svc.contains("        command:\n        - sh\n        - -c\n"),
        "the manifest entrypoint must become the chart command, got:\n{svc}"
    );
    assert!(
        svc.contains("        args:\n        - echo hi\n"),
        "the manifest command must become args, got:\n{svc}"
    );
}

/// Fix wave 2, Critical A. Helm renders every file under `templates/`
/// through Go `text/template` BEFORE any YAML parser sees it, so a
/// resolved argv element containing `{{` is a template injection path,
/// not a formatting nit: quoting it as a YAML string does nothing to
/// Go's templater, which reads past the quotes. The oracle here is
/// independent of the Kubernetes emitter (that cross-check is what let
/// this hole through last wave): it applies the inverse of Helm's
/// literal escape, the same substitution Go's templater performs on
/// `{{ "{{" }}` at `helm install` time, and reparses the result as
/// YAML to check the argument comes back exactly as it went in.
#[test]
fn helm_escapes_template_braces_to_close_the_injection() {
    let original = "redis {{ .Values.x }} arg";
    let yaml = format!(
        r"
project:
  name: shop
resources:
  svc:
    container:
      image: alpine:3.20
      command: [{original:?}]
"
    );
    let a = artifacts(&yaml);
    let svc = file(&a, "templates/svc.yaml");

    let args = argv_block(svc, "        args:\n");
    assert_eq!(args.len(), 1, "expected exactly one arg, got:\n{svc}");
    let scalar = args[0]
        .strip_prefix("        - ")
        .expect("list item has the expected prefix");

    assert!(
        scalar.contains(r#"{{ "{{" }}"#),
        "the `{{{{` opener must be escaped to the Helm literal, got:\n{scalar}"
    );
    // Every escaped occurrence in the arg accounted for, nothing bare
    // should remain (the rest of the file legitimately has other,
    // unrelated `{{ ... }}` Go template directives, so this check is
    // scoped to the arg's own scalar, not the whole file).
    let without_escapes = scalar.replace(r#"{{ "{{" }}"#, "");
    assert!(
        !without_escapes.contains("{{"),
        "a raw, unescaped `{{{{` opener survived in the arg, got:\n{scalar}"
    );

    // Simulate Go's templater: it renders `{{ "{{" }}` back to a
    // literal `{{` before the YAML parser ever runs.
    let rendered = scalar.replace(r#"{{ "{{" }}"#, "{{");
    let round_tripped: String =
        serde_norway::from_str(&rendered).expect("the rendered scalar reparses as YAML");
    assert_eq!(
        round_tripped, original,
        "the arg must round-trip through the escape/render/parse chain to the exact original value"
    );
}

/// Fix wave 2, Critical B. `serde_norway` indents a block scalar's body
/// two columns from the document root, but this emitter splices the
/// result after a `        - ` list marker (column 10): an unindented
/// body dedents out of the list item and the chart fails to parse.
/// Reparsing the emitted template as YAML is the oracle: it must
/// reproduce the exact multi-line value, not merely look plausible.
#[test]
fn multiline_arg_round_trips_through_the_emitted_template() {
    let original = "set -e\necho one\nexec app";
    let yaml = format!(
        r"
project:
  name: shop
resources:
  svc:
    container:
      image: alpine:3.20
      entrypoint: ['sh', '-c']
      command: [{original:?}]
"
    );
    let a = artifacts(&yaml);
    let svc = file(&a, "templates/svc.yaml");

    let first_doc = svc.split("---").next().expect("at least one document");
    // Go's templater consumes the leading `{{- $svc := ... -}}`
    // directive before the YAML parser ever runs; drop that one line
    // here so the reparse below is not derailed by an unrelated
    // concern (this file emits raw, unrendered Go template text).
    let without_directive: String = first_doc
        .lines()
        .filter(|line| !line.trim_start().starts_with("{{-"))
        .collect::<Vec<_>>()
        .join("\n");

    let parsed: serde_norway::Value = serde_norway::from_str(&without_directive)
        .expect("the emitted chart must reparse as YAML after Fix B");
    let args = parsed["spec"]["template"]["spec"]["containers"][0]["args"]
        .as_sequence()
        .expect("args is a sequence");
    assert_eq!(args.len(), 1, "expected exactly one arg, got: {args:?}");
    assert_eq!(
        args[0].as_str(),
        Some(original),
        "the multi-line arg must round-trip exactly, got: {:?}",
        args[0]
    );
}
