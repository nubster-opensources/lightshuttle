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

use lightshuttle_export::{
    ComposeEmitter, Emitter, ExportArtifacts, HelmEmitter, KubernetesEmitter, lower,
};
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

/// Two resources of the same managed kind, carrying deliberately
/// different passwords: the fixture for
/// [`two_resources_of_one_kind_do_not_share_one_compose_reference`].
const TWO_DATABASES: &str = r"
project:
  name: probe
resources:
  main:
    postgres:
      version: '16'
      password: PASSWORD_OF_MAIN_DATABASE
  reporting:
    postgres:
      version: '16'
      password: PASSWORD_OF_REPORTING_DATABASE
";

/// A resource whose name is legal in the manifest but not as an
/// environment variable name.
const DASHED_CACHE: &str = r"
project:
  name: probe
resources:
  web-cache:
    redis:
      version: '7'
      password: NEVER_EXPORT_THIS_REDIS_VALUE_7f3a
";

/// Two resource names that differ in the manifest but normalise to the
/// same environment variable name.
const COLLIDING_CACHES: &str = r"
project:
  name: probe
resources:
  web-cache:
    redis:
      version: '7'
      password: NEVER_EXPORT_THIS_REDIS_VALUE_7f3a
  web_cache:
    redis:
      version: '7'
      password: NEVER_EXPORT_THIS_POSTGRES_VALUE_9c21
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
        ResourceKind::Container(_) | ResourceKind::Dockerfile(_) => None,
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
    let secrets: Vec<&'static str> = manifest
        .resources
        .values()
        .filter_map(expected_secret)
        .collect();
    assert_eq!(
        secrets.len(),
        2,
        "the probe stack must carry exactly the postgres and redis secrets, got {secrets:?}"
    );

    let model = lower(&manifest).expect("lowering succeeds");

    let targets: [(&str, ExportArtifacts); 3] = [
        (
            "compose",
            ComposeEmitter.emit(&model).expect("compose emits"),
        ),
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
///
/// The two are not symmetrical, and that asymmetry is deliberate.
/// Kubernetes and Helm substitute from the pod's own environment, which
/// is already isolated per resource, so the plain key is unambiguous
/// there. Compose interpolates from a single project-wide `.env`, so the
/// reference it emits carries the resource name as a prefix: see
/// [`two_resources_of_one_kind_do_not_share_one_compose_reference`] for
/// what that prefix prevents.
#[test]
fn each_target_renders_the_redis_credential_as_a_named_reference_in_its_own_syntax() {
    let manifest = Manifest::parse(REDIS_WITH_PASSWORD).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");
    assert!(
        compose_yaml.contains("${CACHE_REDIS_PASSWORD}"),
        "compose's command should reference the secret by its resource-scoped name, got:\n{compose_yaml}"
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
        compose_yaml.contains("${CACHE_REDIS_PASSWORD}"),
        "the reference must be present in Compose's own syntax, got:\n{compose_yaml}"
    );
    assert!(
        !compose_yaml.contains("$${CACHE_REDIS_PASSWORD}"),
        "the reference must not come back escaped as if it were literal text, got:\n{compose_yaml}"
    );
}

/// Compose interpolates from one project-wide `.env`, so a reference
/// named after the kind alone collapses every instance of that kind onto
/// a single value.
///
/// Measured on `main` before this change: two postgres resources holding
/// deliberately different passwords both emitted
/// `POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}`, so Compose handed both
/// databases the same value and the distinction the manifest declared was
/// lost with no diagnostic. The key inside the container keeps the name
/// the image requires; only the reference is scoped to the resource.
#[test]
fn two_resources_of_one_kind_do_not_share_one_compose_reference() {
    let manifest = Manifest::parse(TWO_DATABASES).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");

    assert!(
        compose_yaml.contains("${MAIN_POSTGRES_PASSWORD}"),
        "the first database must own its reference, got:\n{compose_yaml}"
    );
    assert!(
        compose_yaml.contains("${REPORTING_POSTGRES_PASSWORD}"),
        "the second database must own its reference, got:\n{compose_yaml}"
    );
    assert!(
        !compose_yaml.contains("${POSTGRES_PASSWORD}"),
        "no reference may stay scoped to the kind alone, got:\n{compose_yaml}"
    );

    // The key the image reads is imposed by the image and must not move.
    assert_eq!(
        compose_yaml.matches("POSTGRES_PASSWORD:").count(),
        2,
        "each service keeps the environment key its image requires, got:\n{compose_yaml}"
    );
}

/// A resource name is free-form, an environment variable name is not: a
/// name carrying a character no shell would accept must be normalised
/// before it can be referenced.
///
/// Normalisation cannot be proven injective, so the guarantee has to come
/// from a uniqueness check at the point of emission rather than from the
/// function: `web-cache` and `web_cache` both normalise to
/// `WEB_CACHE_REDIS_PASSWORD`, and an export that silently handed both
/// resources the same reference would reintroduce, by another route,
/// exactly the collision this file is about.
#[test]
fn a_resource_name_that_is_not_a_valid_variable_name_is_normalised_or_refused() {
    let manifest = Manifest::parse(DASHED_CACHE).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let compose = ComposeEmitter.emit(&model).expect("compose emits");
    let compose_yaml = file(&compose, "docker-compose.yml");

    assert!(
        compose_yaml.contains("${WEB_CACHE_REDIS_PASSWORD}"),
        "a dashed resource name must be normalised into a usable variable name, got:\n{compose_yaml}"
    );
    assert!(
        !compose_yaml.contains("${web-cache"),
        "a reference must never carry a character the shell cannot read, got:\n{compose_yaml}"
    );
}

/// Two resource names that normalise to the same variable must be
/// refused rather than silently collapsed onto one reference.
#[test]
fn two_resource_names_that_normalise_alike_are_refused() {
    let manifest = Manifest::parse(COLLIDING_CACHES).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let failure = ComposeEmitter
        .emit(&model)
        .expect_err("two resources sharing one reference must be refused");

    let message = failure.to_string();
    assert!(
        message.contains("web-cache") && message.contains("web_cache"),
        "the refusal must name both resources that collide, got: {message}"
    );
}

/// Two secret keys of one resource that differ only by case.
const CASE_COLLIDING_KEYS: &str = r"
project:
  name: probe
resources:
  api:
    container:
      image: alpine:3.20
      env:
        db_password: FIRST_VALUE
        DB_PASSWORD: SECOND_VALUE
";

/// The collision guard must compare produced names across every
/// `(resource, key)` source, not across resources: two keys of a single
/// resource can normalise alike just as two resources can.
#[test]
fn two_secret_keys_of_one_resource_that_normalise_alike_are_refused() {
    let manifest = Manifest::parse(CASE_COLLIDING_KEYS).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let failure = ComposeEmitter
        .emit(&model)
        .expect_err("two keys sharing one reference must be refused");

    let message = failure.to_string();
    assert!(
        message.contains("db_password") && message.contains("DB_PASSWORD"),
        "the refusal must name both keys that collide, got: {message}"
    );
}
