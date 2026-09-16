//! Red tests for the deployment-time placeholder rendering module.
//!
//! Covers the rendering table of the export-deployment-placeholders design,
//! line by line, for the three export targets (Compose, Kubernetes, Helm).
//! Every assertion below documents the behaviour the Building phase must
//! deliver; until [`DeploymentText::parse`], [`ResourceDirectory::for_target`]
//! and the three renderers are implemented, every test panics with
//! `not yet implemented`.

use lightshuttle_export::{
    ComposeRenderer, DeploymentText, ExportError, HelmRenderer, KubernetesRenderer,
    PlaceholderRenderer, ResourceDirectory, Target, TextField,
};
use lightshuttle_manifest::Manifest;

/// Probe stack: a postgres resource (`main_db`), a redis resource (`cache`),
/// and a container (`api`) that references both, mirroring the design's
/// section 1 probe manifest plus a redis reference for port coverage.
const STACK: &str = r#"
project:
  name: shop
resources:
  main_db:
    postgres:
      version: '16'
      password: devsecret
  cache:
    redis:
      version: '7'
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
        CACHE_PORT: "${resources.cache.port}"
      depends_on: [main_db, cache]
"#;

fn manifest() -> Manifest {
    Manifest::parse(STACK).expect("probe manifest parses")
}

fn directory(target: Target) -> ResourceDirectory {
    ResourceDirectory::for_target(&manifest(), target).expect("directory resolves")
}

fn parse(raw: &str, field: TextField<'_>, resource: &str, target: Target) -> DeploymentText {
    DeploymentText::parse(raw, field, resource, &directory(target)).expect("text parses")
}

// --- `${env.N}` without a default -----------------------------------------

#[test]
fn compose_renders_env_without_default_as_dollar_brace_name() {
    let text = parse("${env.TAG}", TextField::Image, "api", Target::Compose);
    assert_eq!(ComposeRenderer.render(&text).unwrap(), "${TAG}");
}

#[test]
fn kubernetes_refuses_env_without_default() {
    let text = parse("${env.TAG}", TextField::Image, "api", Target::Kubernetes);
    let err = KubernetesRenderer.render(&text).unwrap_err();
    assert!(
        matches!(
            err,
            ExportError::UnresolvedVariables { ref resource, target: "kubernetes", ref variables }
                if resource == "api" && variables == &["TAG".to_owned()]
        ),
        "expected UnresolvedVariables for `api`/TAG, got {err:?}"
    );
}

#[test]
fn helm_renders_env_without_default_as_required() {
    let text = parse("${env.TAG}", TextField::Image, "api", Target::Helm);
    assert_eq!(
        HelmRenderer.render(&text).unwrap(),
        r#"{{ required "variables.TAG is required" .Values.variables.TAG }}"#
    );
}

// --- `${env.N:-d}` with a default ------------------------------------------

#[test]
fn compose_renders_env_with_default_as_dollar_brace_name_dash_default() {
    let text = parse(
        "hello ${env.WHO:-world}",
        TextField::Env { key: "GREETING" },
        "api",
        Target::Compose,
    );
    assert_eq!(
        ComposeRenderer.render(&text).unwrap(),
        "hello ${WHO:-world}"
    );
}

#[test]
fn kubernetes_freezes_env_default_in_place() {
    let text = parse(
        "hello ${env.WHO:-world}",
        TextField::Env { key: "GREETING" },
        "api",
        Target::Kubernetes,
    );
    assert_eq!(KubernetesRenderer.render(&text).unwrap(), "hello world");
}

#[test]
fn helm_renders_env_with_default_as_default_pipe() {
    let text = parse(
        "hello ${env.WHO:-world}",
        TextField::Env { key: "GREETING" },
        "api",
        Target::Helm,
    );
    assert_eq!(
        HelmRenderer.render(&text).unwrap(),
        r#"hello {{ .Values.variables.WHO | default "world" }}"#
    );
}

#[test]
fn compose_renders_nested_default_recursively() {
    let text = parse(
        "prefix-${env.A:-fallback-${env.B}}",
        TextField::WorkingDir,
        "api",
        Target::Compose,
    );
    assert_eq!(
        ComposeRenderer.render(&text).unwrap(),
        "prefix-${A:-fallback-${B}}"
    );
}

// --- `${resources.X.host}` --------------------------------------------------

#[test]
fn compose_resolves_resource_host_to_raw_service_name() {
    let text = parse(
        "${resources.main_db.host}",
        TextField::Command,
        "api",
        Target::Compose,
    );
    assert_eq!(ComposeRenderer.render(&text).unwrap(), "main_db");
}

#[test]
fn kubernetes_resolves_resource_host_to_dns_name() {
    let text = parse(
        "${resources.main_db.host}",
        TextField::Command,
        "api",
        Target::Kubernetes,
    );
    let rendered = KubernetesRenderer.render(&text).unwrap();
    assert_ne!(
        rendered, "main_db",
        "kubernetes must address the generated dns_name, not the raw resource name"
    );
}

#[test]
fn helm_resolves_resource_host_to_dns_name() {
    let text = parse(
        "${resources.main_db.host}",
        TextField::Command,
        "api",
        Target::Helm,
    );
    let rendered = HelmRenderer.render(&text).unwrap();
    assert_ne!(
        rendered, "main_db",
        "helm must address the generated dns_name, not the raw resource name"
    );
}

// --- `${resources.X.port}` and other non-sensitive outputs -----------------

#[test]
fn compose_renders_resource_port_in_clear() {
    let text = parse(
        "${resources.cache.port}",
        TextField::Env { key: "CACHE_PORT" },
        "api",
        Target::Compose,
    );
    assert_eq!(ComposeRenderer.render(&text).unwrap(), "6379");
}

#[test]
fn kubernetes_renders_resource_port_in_clear() {
    let text = parse(
        "${resources.cache.port}",
        TextField::Env { key: "CACHE_PORT" },
        "api",
        Target::Kubernetes,
    );
    assert_eq!(KubernetesRenderer.render(&text).unwrap(), "6379");
}

#[test]
fn helm_renders_resource_port_in_clear() {
    let text = parse(
        "${resources.cache.port}",
        TextField::Env { key: "CACHE_PORT" },
        "api",
        Target::Helm,
    );
    assert_eq!(HelmRenderer.render(&text).unwrap(), "6379");
}

// --- sensitive property outside `env` ---------------------------------------

#[test]
fn sensitive_reference_outside_env_is_refused_naming_resource_and_field() {
    for target in [Target::Compose, Target::Kubernetes, Target::Helm] {
        let dir = directory(target);
        let err = DeploymentText::parse(
            "${resources.main_db.password}",
            TextField::Command,
            "api",
            &dir,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ExportError::SensitiveReferenceOutsideEnv { ref resource, ref reference, .. }
                    if resource == "api" && reference.contains("main_db") && reference.contains("password")
            ),
            "expected SensitiveReferenceOutsideEnv naming `api` and the password reference, got {err:?}"
        );
    }
}

// --- literal `$` -------------------------------------------------------------

#[test]
fn compose_escapes_literal_dollar_as_double_dollar() {
    let text = parse(
        "costs $5",
        TextField::Env { key: "PRICE" },
        "api",
        Target::Compose,
    );
    assert_eq!(ComposeRenderer.render(&text).unwrap(), "costs $$5");
}

#[test]
fn kubernetes_leaves_literal_dollar_unchanged() {
    let text = parse(
        "costs $5",
        TextField::Env { key: "PRICE" },
        "api",
        Target::Kubernetes,
    );
    assert_eq!(KubernetesRenderer.render(&text).unwrap(), "costs $5");
}

#[test]
fn helm_leaves_literal_dollar_unchanged() {
    let text = parse(
        "costs $5",
        TextField::Env { key: "PRICE" },
        "api",
        Target::Helm,
    );
    assert_eq!(HelmRenderer.render(&text).unwrap(), "costs $5");
}

// --- escape `${{ x }}` -------------------------------------------------------

#[test]
fn compose_renders_escape_form_as_double_dollar_brace() {
    let text = parse(
        "${{ not.a.reference }}",
        TextField::Env { key: "LITERAL" },
        "api",
        Target::Compose,
    );
    assert_eq!(
        ComposeRenderer.render(&text).unwrap(),
        "$${ not.a.reference }"
    );
}

#[test]
fn kubernetes_renders_escape_form_as_single_dollar_brace() {
    let text = parse(
        "${{ not.a.reference }}",
        TextField::Env { key: "LITERAL" },
        "api",
        Target::Kubernetes,
    );
    assert_eq!(
        KubernetesRenderer.render(&text).unwrap(),
        "${ not.a.reference }"
    );
}

#[test]
fn helm_renders_escape_form_as_single_dollar_brace() {
    let text = parse(
        "${{ not.a.reference }}",
        TextField::Env { key: "LITERAL" },
        "api",
        Target::Helm,
    );
    assert_eq!(HelmRenderer.render(&text).unwrap(), "${ not.a.reference }");
}

// --- literal `{{` -------------------------------------------------------------

#[test]
fn compose_leaves_literal_double_brace_unchanged() {
    let text = parse(
        "value {{ not.a.template }} tail",
        TextField::WorkingDir,
        "api",
        Target::Compose,
    );
    assert_eq!(
        ComposeRenderer.render(&text).unwrap(),
        "value {{ not.a.template }} tail"
    );
}

#[test]
fn kubernetes_leaves_literal_double_brace_unchanged() {
    let text = parse(
        "value {{ not.a.template }} tail",
        TextField::WorkingDir,
        "api",
        Target::Kubernetes,
    );
    assert_eq!(
        KubernetesRenderer.render(&text).unwrap(),
        "value {{ not.a.template }} tail"
    );
}

#[test]
fn helm_escapes_literal_double_brace_only_for_helm() {
    let text = parse(
        "value {{ not.a.template }} tail",
        TextField::WorkingDir,
        "api",
        Target::Helm,
    );
    assert_eq!(
        HelmRenderer.render(&text).unwrap(),
        r#"value {{ "{{" }} not.a.template }} tail"#
    );
}

// --- variable name grammar ---------------------------------------------------

#[test]
fn invalid_variable_name_leading_digit_is_refused() {
    let dir = directory(Target::Compose);
    let err = DeploymentText::parse("${env.1BAD}", TextField::Image, "api", &dir).unwrap_err();
    assert!(
        matches!(
            err,
            ExportError::InvalidVariableName { ref resource, ref name }
                if resource == "api" && name == "1BAD"
        ),
        "expected InvalidVariableName for `1BAD`, got {err:?}"
    );
}

#[test]
fn invalid_variable_name_with_hyphen_is_refused() {
    let dir = directory(Target::Compose);
    let err = DeploymentText::parse("${env.A-B}", TextField::Image, "api", &dir).unwrap_err();
    assert!(
        matches!(
            err,
            ExportError::InvalidVariableName { ref resource, ref name }
                if resource == "api" && name == "A-B"
        ),
        "expected InvalidVariableName for `A-B`, got {err:?}"
    );
}

// --- `is_literal` -------------------------------------------------------------

#[test]
fn is_literal_is_true_for_text_without_any_reference() {
    let text = parse("alpine:3.20", TextField::Image, "api", Target::Compose);
    assert!(text.is_literal());
}

#[test]
fn is_literal_is_false_as_soon_as_an_env_reference_is_present() {
    let text = parse(
        "example/api:${env.TAG:-1.0}",
        TextField::Image,
        "api",
        Target::Compose,
    );
    assert!(!text.is_literal());
}

#[test]
fn is_literal_is_false_as_soon_as_a_resolved_resource_reference_is_present() {
    // The `${resources...}` reference is resolved away by `parse`, but the
    // text it produced is not a target-independent literal: it changes with
    // the target's host naming, so `is_literal` must still say `false`.
    let text = parse(
        "${resources.main_db.host}",
        TextField::Command,
        "api",
        Target::Compose,
    );
    assert!(!text.is_literal());
}

// --- `variables()` ordering ----------------------------------------------------

#[test]
fn variables_lists_names_in_first_seen_order_defaults_included_without_duplicates() {
    let text = parse(
        "${env.C}-${env.A:-${env.B}}-${env.A}-${env.C}",
        TextField::WorkingDir,
        "api",
        Target::Compose,
    );
    assert_eq!(text.variables(), vec!["C", "A", "B"]);
}

// --- CRUCIAL: kubernetes lists every unresolved variable, sorted -------------

#[test]
fn kubernetes_unresolved_variables_error_lists_every_variable_sorted_not_just_the_first() {
    let text = parse(
        "${env.CHARLIE}-${env.ALPHA}-${env.BRAVO}",
        TextField::WorkingDir,
        "api",
        Target::Kubernetes,
    );
    let err = KubernetesRenderer.render(&text).unwrap_err();
    assert!(
        matches!(
            err,
            ExportError::UnresolvedVariables { ref resource, target: "kubernetes", ref variables }
                if resource == "api"
                    && variables == &["ALPHA".to_owned(), "BRAVO".to_owned(), "CHARLIE".to_owned()]
        ),
        "expected all three variables listed and sorted, got {err:?}"
    );
}
