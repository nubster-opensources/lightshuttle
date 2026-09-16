//! Behavioural oracle: no export target may ever print a managed
//! credential in clear text.
//!
//! `spec_redis` (`lightshuttle-spec`) currently bakes a redis password
//! straight into the container's `command` (`["redis-server",
//! "--requirepass", "<value>"]`), unlike postgres, whose password travels
//! through `env` and is therefore caught by
//! `lightshuttle_export::resolve::is_secret_key`. The redis password comes
//! back out in clear text in all three export targets: Compose's
//! `docker-compose.yml`, the Kubernetes manifest's `args:`, and the Helm
//! chart template.
//!
//! This file does not assert on any list of environment-variable names: it
//! asserts on the *value*. A probe secret is planted in the manifest and
//! every emitted file, across every target, is searched for it verbatim.

use lightshuttle_export::{ComposeEmitter, Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower};
use lightshuttle_manifest::{Manifest, ResourceKind};

/// Probe value planted as the postgres password. Long and unique so a
/// false positive (matching something else in the output) is not
/// plausible, and distinct from the redis probe so the two channels
/// cannot be confused with one another.
const POSTGRES_SECRET: &str = "NEVER_EXPORT_THIS_POSTGRES_VALUE_9c21";

/// Probe value planted as the redis password. See [`POSTGRES_SECRET`] for
/// why it is long, unique, and distinct from the postgres probe.
const REDIS_SECRET: &str = "NEVER_EXPORT_THIS_REDIS_VALUE_7f3a";

/// One resource of every kind `lightshuttle-manifest` currently knows,
/// with the credential-bearing kinds carrying their probe value.
const PROBE_STACK: &str = r"
project:
  name: probe
  version: 1.0.0
export:
  helm:
    chart_name: probe-chart
resources:
  db:
    postgres:
      version: '16'
      password: NEVER_EXPORT_THIS_POSTGRES_VALUE_9c21
      volume: dbdata
  cache:
    redis:
      version: '7'
      password: NEVER_EXPORT_THIS_REDIS_VALUE_7f3a
  api:
    container:
      image: alpine:3.20
  builder:
    dockerfile:
      context: ./app
      dockerfile: Dockerfile
";

/// A redis resource with no password declared at all: the fixture for
/// [`redis_without_a_password_carries_no_requirepass_and_no_secret`].
const REDIS_NO_PASSWORD: &str = r"
project:
  name: probe
resources:
  cache:
    redis:
      version: '7'
";

/// A single redis resource carrying the probe password, isolated from the
/// other kinds so the reference-syntax and escaping tests read one
/// service's command line without noise from the rest of the stack.
const REDIS_WITH_PASSWORD: &str = r"
project:
  name: probe
export:
  helm:
    chart_name: probe-chart
resources:
  cache:
    redis:
      version: '7'
      password: NEVER_EXPORT_THIS_REDIS_VALUE_7f3a
";

fn file<'a>(artifacts: &'a ExportArtifacts, name: &str) -> &'a str {
    artifacts
        .files
        .iter()
        .find(|candidate| candidate.path.to_str() == Some(name))
        .unwrap_or_else(|| panic!("missing file {name}"))
        .contents
        .as_str()
}

/// The value this test expects never to appear in clear text for one
/// `ResourceKind` variant, or `None` for a kind that carries no
/// credential of its own.
///
/// Matched exhaustively, with no `_` arm on purpose: the day
/// `lightshuttle-manifest` grows a new `ResourceKind` variant that can
/// carry a credential (a new managed datastore, for instance), this
/// function stops compiling until a probe value is chosen for it here.
/// That is the enforcement the issue asked for: an added variant breaks
/// this test's compilation rather than silently expanding the leak
/// surface this oracle does not know to check.
fn expected_secret(kind: &ResourceKind) -> Option<&'static str> {
    match kind {
        ResourceKind::Postgres(_) => Some(POSTGRES_SECRET),
        ResourceKind::Redis(_) => Some(REDIS_SECRET),
        ResourceKind::Container(_) => None,
        ResourceKind::Dockerfile(_) => None,
    }
}

/// The central oracle: exports [`PROBE_STACK`] to all three targets and
/// requires that neither probe value ever appears verbatim in any
/// produced file.
///
/// This proves the defect directly, by value, rather than by asserting on
/// which environment-variable names get redacted: today, the redis probe
/// leaks into `docker-compose.yml`, the Kubernetes manifest and the Helm
/// template, because `spec_redis` writes the password straight into the
/// container's `command` instead of routing it through `env`.
#[test]
fn no_export_target_ever_prints_a_managed_credential_in_clear_text() {
    let manifest = Manifest::parse(PROBE_STACK).expect("manifest parses");

    // If this manifest stops carrying exactly the postgres and redis
    // probes (for example because a future edit renames or removes one of
    // those resources), the oracle below would silently check nothing:
    // catch that here instead of passing for the wrong reason.
    let secrets: Vec<&'static str> = manifest.resources.values().filter_map(expected_secret).collect();
    assert_eq!(
        secrets.len(),
        2,
        "the probe stack must carry exactly the postgres and redis secrets, got {secrets:?}"
    );

    let model = lower(&manifest).expect("lowering succeeds");

    let targets: [(&str, ExportArtifacts); 3] = [
        ("compose", ComposeEmitter.emit(&model).expect("compose emits")),
        (
            "kubernetes",
            KubernetesEmitter.emit(&model).expect("kubernetes emits"),
        ),
        ("helm", HelmEmitter.emit(&model).expect("helm emits")),
    ];

    for (target_name, artifacts) in &targets {
        for exported in &artifacts.files {
            for secret in &secrets {
                assert!(
                    !exported.contents.contains(secret),
                    "{target_name} artifact {} leaks a credential in clear text:\n{}",
                    exported.path.display(),
                    exported.contents
                );
            }
        }
    }
}

/// Fixes today's correct behaviour so a future redaction pass cannot
/// regress it: a redis resource with no password declared must keep
/// carrying no `--requirepass` flag, no `REDIS_PASSWORD`-shaped
/// environment key, and no empty secret manufactured by the redaction
/// machinery alone.
#[test]
fn redis_without_a_password_carries_no_requirepass_and_no_secret() {
    let manifest = Manifest::parse(REDIS_NO_PASSWORD).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");
    assert!(
        !compose_yaml.contains("requirepass"),
        "a passwordless redis must not gain --requirepass, got:\n{compose_yaml}"
    );
    assert!(
        !compose_yaml.contains("REDIS_PASSWORD"),
        "a passwordless redis must not gain a password environment key, got:\n{compose_yaml}"
    );

    let kubernetes = KubernetesEmitter.emit(&model).expect("kubernetes emits");
    let kubernetes_yaml = file(&kubernetes, "cache.yaml");
    assert!(
        !kubernetes_yaml.contains("requirepass"),
        "a passwordless redis must not gain --requirepass, got:\n{kubernetes_yaml}"
    );
    assert!(
        !kubernetes_yaml.contains("REDIS_PASSWORD"),
        "a passwordless redis must not gain a password environment key, got:\n{kubernetes_yaml}"
    );
}

/// Fixes the reference syntax each target must use once the redis
/// password is redacted instead of copied verbatim: Compose's own
/// interpolation syntax, and the `$(NAME)` syntax Kubernetes and Helm
/// both use to substitute a container's own declared environment into
/// its command line.
#[test]
fn each_target_renders_the_redis_credential_as_a_named_reference_in_its_own_syntax() {
    let manifest = Manifest::parse(REDIS_WITH_PASSWORD).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");
    assert!(
        compose_yaml.contains("${REDIS_PASSWORD}"),
        "compose's command should reference the secret by name, got:\n{compose_yaml}"
    );

    let kubernetes = KubernetesEmitter.emit(&model).expect("kubernetes emits");
    let kubernetes_yaml = file(&kubernetes, "cache.yaml");
    assert!(
        kubernetes_yaml.contains("$(REDIS_PASSWORD)"),
        "kubernetes's args should reference the secret by name, got:\n{kubernetes_yaml}"
    );

    let helm = HelmEmitter.emit(&model).expect("helm emits");
    let helm_yaml = file(&helm, "templates/cache.yaml");
    assert!(
        helm_yaml.contains("$(REDIS_PASSWORD)"),
        "helm's args should reference the secret by name, got:\n{helm_yaml}"
    );
}

/// Fixes the coupling with the Compose `$` escaping introduced by #308:
/// literal text is escaped so a literal `$` survives Compose's own
/// interpolation pass (`costs $5` must not silently become `costs `), but
/// that escape must stop at the redis credential reference this file
/// expects Compose to produce, or the reference renders as a literal `$`
/// followed by inert text instead of a placeholder `docker compose`
/// substitutes at deploy time.
#[test]
fn compose_dollar_escaping_does_not_swallow_the_redis_credential_reference() {
    let manifest = Manifest::parse(REDIS_WITH_PASSWORD).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");

    assert!(
        compose_yaml.contains("${REDIS_PASSWORD}"),
        "the reference must be present in Compose's own syntax, got:\n{compose_yaml}"
    );
    assert!(
        !compose_yaml.contains("$${REDIS_PASSWORD}"),
        "the reference must not come back escaped as if it were literal text, got:\n{compose_yaml}"
    );
}
