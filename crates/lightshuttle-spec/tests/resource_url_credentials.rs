//! Resource connection URLs, read back by an external parser.
//!
//! The encoder has its own tests in `lightshuttle-manifest`. What is checked
//! here is the composition: that the URL a resource publishes still names the
//! host, the port and the database the manifest declared, once a credential
//! carries a reserved character.

use lightshuttle_manifest::Manifest;
use lightshuttle_spec::from_resource;
use url::Url;

/// Resolves a stack and returns the named output of one of its resources.
fn output(yaml: &str, resource: &str, key: &str) -> String {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let kind = manifest
        .resources
        .get(resource)
        .unwrap_or_else(|| panic!("resource `{resource}` is missing"));
    from_resource(&manifest.project.name, resource, kind)
        .expect("resolution succeeds")
        .outputs
        .get(key)
        .unwrap_or_else(|| panic!("output `{key}` is missing"))
        .clone()
}

/// The service host a resource is reachable at, as the resolver builds it.
fn host_of(project: &str, resource: &str) -> String {
    format!("{project}_{resource}")
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

#[test]
fn a_reserved_character_in_a_postgres_password_does_not_move_the_host() {
    let raw = output(&postgres_stack("p@ss:w/rd"), "db", "url");
    let parsed = Url::parse(&raw).unwrap_or_else(|error| panic!("`{raw}` must parse: {error}"));

    assert_eq!(parsed.scheme(), "postgres");
    assert_eq!(parsed.host_str(), Some(host_of("shop", "db").as_str()));
    assert_eq!(parsed.port(), Some(5432));
    assert_eq!(parsed.path(), "/appdb");
}

// The structured outputs feed the service container directly, so they must
// stay exactly as declared. Only the URL is encoded.
#[test]
fn the_structured_postgres_outputs_stay_raw() {
    let stack = postgres_stack("p@ss:w/rd");
    assert_eq!(output(&stack, "db", "password"), "p@ss:w/rd");
    assert_eq!(output(&stack, "db", "user"), "appuser");
    assert_eq!(output(&stack, "db", "database"), "appdb");
}

#[test]
fn an_ordinary_postgres_password_keeps_the_url_unchanged() {
    let raw = output(&postgres_stack("s3cret"), "db", "url");
    let host = host_of("shop", "db");
    assert_eq!(raw, format!("postgres://appuser:s3cret@{host}:5432/appdb"));
}

#[test]
fn a_reserved_character_in_a_redis_password_does_not_move_the_host() {
    let yaml = r"
project:
  name: shop
resources:
  cache:
    redis:
      version: '7'
      password: 'p@ss/word'
";
    let raw = output(yaml, "cache", "url");
    let parsed = Url::parse(&raw).unwrap_or_else(|error| panic!("`{raw}` must parse: {error}"));

    assert_eq!(parsed.scheme(), "redis");
    assert_eq!(parsed.host_str(), Some(host_of("shop", "cache").as_str()));
    assert_eq!(parsed.port(), Some(6379));
    assert!(parsed.username().is_empty(), "redis carries no user name");
}

#[test]
fn an_ordinary_redis_password_keeps_the_url_unchanged() {
    let yaml = r"
project:
  name: shop
resources:
  cache:
    redis:
      version: '7'
      password: 's3cret'
";
    let host = host_of("shop", "cache");
    assert_eq!(
        output(yaml, "cache", "url"),
        format!("redis://:s3cret@{host}:6379")
    );
}
