//! `from_resource_on_host` tests: resolving a resource with a deployment
//! host instead of the runtime container name.
//!
//! `from_resource` keeps its own tests in `resource_url_credentials.rs`.
//! What is checked here is the delta: `host` (and everything built from
//! it, namely `url`) tracks the given service host, while every other
//! output and the percent-encoding applied to credentials stay identical
//! to `from_resource`.

use lightshuttle_manifest::Manifest;
use lightshuttle_spec::{SENSITIVE_OUTPUTS, from_resource, from_resource_on_host};
use url::Url;

fn resolve(yaml: &str, resource: &str) -> lightshuttle_spec::ResolvedResource {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let kind = manifest
        .resources
        .get(resource)
        .unwrap_or_else(|| panic!("resource `{resource}` is missing"));
    from_resource(&manifest.project.name, resource, kind).expect("resolution succeeds")
}

fn resolve_on_host(
    yaml: &str,
    resource: &str,
    service_host: &str,
) -> lightshuttle_spec::ResolvedResource {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let kind = manifest
        .resources
        .get(resource)
        .unwrap_or_else(|| panic!("resource `{resource}` is missing"));
    from_resource_on_host(&manifest.project.name, resource, kind, service_host)
        .expect("resolution succeeds")
}

fn postgres_stack(password: &str) -> String {
    format!(
        r"
project:
  name: shop
resources:
  db:
    postgres:
      version: '16'
      user: appuser
      password: '{password}'
      database: appdb
"
    )
}

fn redis_stack(password: &str) -> String {
    format!(
        r"
project:
  name: shop
resources:
  cache:
    redis:
      version: '7'
      password: '{password}'
"
    )
}

#[test]
fn postgres_host_and_url_carry_the_given_service_host() {
    let yaml = postgres_stack("p@ss:w/rd");
    let resolved = resolve_on_host(&yaml, "db", "db-77e71df3");

    assert_eq!(resolved.outputs["host"], "db-77e71df3");
    let url = Url::parse(&resolved.outputs["url"])
        .unwrap_or_else(|error| panic!("`{}` must parse: {error}", resolved.outputs["url"]));
    assert_eq!(url.host_str(), Some("db-77e71df3"));
}

#[test]
fn redis_host_and_url_carry_the_given_service_host() {
    let yaml = redis_stack("p@ss/word");
    let resolved = resolve_on_host(&yaml, "cache", "cache-77e71df3");

    assert_eq!(resolved.outputs["host"], "cache-77e71df3");
    let url = Url::parse(&resolved.outputs["url"])
        .unwrap_or_else(|error| panic!("`{}` must parse: {error}", resolved.outputs["url"]));
    assert_eq!(url.host_str(), Some("cache-77e71df3"));
}

#[test]
fn percent_encoded_credentials_match_from_resource() {
    let yaml = postgres_stack("p@ss:w/rd");
    let runtime = resolve(&yaml, "db");
    let on_host = resolve_on_host(&yaml, "db", "db-77e71df3");

    let runtime_url = Url::parse(&runtime.outputs["url"]).expect("runtime url parses");
    let on_host_url = Url::parse(&on_host.outputs["url"]).expect("on-host url parses");

    assert_eq!(runtime_url.username(), on_host_url.username());
    assert_eq!(runtime_url.password(), on_host_url.password());
    assert_ne!(
        runtime_url.host_str(),
        on_host_url.host_str(),
        "the two hosts must differ, otherwise this test cannot tell the two functions apart"
    );
}

#[test]
fn postgres_outputs_other_than_host_and_url_are_unchanged() {
    let yaml = postgres_stack("p@ss:w/rd");
    let runtime = resolve(&yaml, "db");
    let on_host = resolve_on_host(&yaml, "db", "db-77e71df3");

    for key in ["port", "user", "password", "database"] {
        assert_eq!(
            runtime.outputs[key], on_host.outputs[key],
            "output `{key}` must stay identical to from_resource"
        );
    }
}

#[test]
fn redis_outputs_other_than_host_and_url_are_unchanged() {
    let yaml = redis_stack("p@ss/word");
    let runtime = resolve(&yaml, "cache");
    let on_host = resolve_on_host(&yaml, "cache", "cache-77e71df3");

    for key in ["port", "password"] {
        assert_eq!(
            runtime.outputs[key], on_host.outputs[key],
            "output `{key}` must stay identical to from_resource"
        );
    }
}

#[test]
fn from_resource_is_unchanged_host_stays_the_runtime_container_name() {
    let yaml = postgres_stack("s3cret");
    let runtime = resolve(&yaml, "db");
    assert_eq!(runtime.outputs["host"], "shop_db");
}

#[test]
fn sensitive_outputs_lists_exactly_password_and_url() {
    assert_eq!(SENSITIVE_OUTPUTS, &["password", "url"]);
}
