//! Interpolation tests: validate the `${...}` substitution semantics.

use std::fmt::Write as _;

use indexmap::IndexMap;
use lightshuttle_manifest::{InterpolationContext, Interpolator, ManifestError, Reference};

#[test]
fn substitutes_environment_variable() {
    let ctx =
        InterpolationContext::new().with_env(vec![("LOG_LEVEL".to_owned(), "debug".to_owned())]);
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("level=${env.LOG_LEVEL}").unwrap();
    assert_eq!(out, "level=debug");
}

#[test]
fn applies_environment_default_when_unset() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("level=${env.LOG_LEVEL:-info}").unwrap();
    assert_eq!(out, "level=info");
}

#[test]
fn applies_environment_default_when_empty() {
    let ctx = InterpolationContext::new().with_env(vec![("LOG_LEVEL".to_owned(), String::new())]);
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("level=${env.LOG_LEVEL:-info}").unwrap();
    assert_eq!(out, "level=info");
}

#[test]
fn errors_on_unset_env_without_default() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let err = interp.resolve("${env.MISSING}").unwrap_err();
    assert!(matches!(err, ManifestError::EnvUnset(_)), "got: {err:?}");
}

#[test]
fn escape_form_emits_literal_braces() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("${{not.interpolated}}").unwrap();
    assert_eq!(out, "${not.interpolated}");
}

#[test]
fn substitutes_resource_property() {
    let mut props = IndexMap::new();
    props.insert("url".to_owned(), "postgres://localhost/api".to_owned());
    let ctx = InterpolationContext::new().with_resource("api_db", props);
    let interp = Interpolator::new(&ctx);
    let out = interp
        .resolve("DATABASE_URL=${resources.api_db.url}")
        .unwrap();
    assert_eq!(out, "DATABASE_URL=postgres://localhost/api");
}

#[test]
fn scan_returns_every_reference_without_resolving() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let input = "URL=${resources.db.url};LEVEL=${env.LOG:-info}";
    let refs = interp.scan(input).unwrap();
    assert_eq!(
        refs,
        vec![
            Reference::Resource {
                name: "db".to_owned(),
                property: "url".to_owned(),
            },
            Reference::Env {
                name: "LOG".to_owned(),
                default: Some("info".to_owned()),
            },
        ]
    );
}

#[test]
fn scan_ignores_escape_form() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let refs = interp.scan("literal ${{a.b}} and real ${env.X}").unwrap();
    assert_eq!(refs.len(), 1);
}

#[test]
fn rejects_unterminated_interpolation() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let err = interp.resolve("${env.X").unwrap_err();
    assert!(matches!(err, ManifestError::InvalidInterpolation(_)));
}

#[test]
fn rejects_unknown_reference_scheme() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let err = interp.resolve("${secret.X}").unwrap_err();
    assert!(matches!(err, ManifestError::InvalidInterpolation(_)));
}

#[test]
fn resolves_nested_env_default() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("${env.X:-${env.Y:-fallback}}").unwrap();
    assert_eq!(out, "fallback");

    let ctx2 = InterpolationContext::new().with_env(vec![("Y".to_owned(), "why".to_owned())]);
    let interp2 = Interpolator::new(&ctx2);
    let out2 = interp2.resolve("${env.X:-${env.Y:-fallback}}").unwrap();
    assert_eq!(out2, "why");
}

#[test]
fn rejects_interpolation_deeper_than_limit() {
    let mut input = String::new();
    for i in 0..11 {
        write!(input, "${{env.A{i}:-").unwrap();
    }
    input.push('x');
    for _ in 0..11 {
        input.push('}');
    }

    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let err = interp
        .scan(&input)
        .expect_err("11 levels should exceed the cap");
    assert!(
        matches!(err, ManifestError::InterpolationTooDeep { limit: 10, .. }),
        "got: {err:?}"
    );
}

#[test]
fn rejects_nested_interpolation_outside_env_default() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let err = interp
        .scan("${resources.${env.X}.host}")
        .expect_err("nested interpolation in a resource name is invalid");
    assert!(
        matches!(err, ManifestError::InvalidInterpolation(_)),
        "got: {err:?}"
    );
}

#[test]
fn scan_descends_into_env_default() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let refs = interp.scan("${env.MISSING:-${resources.db.host}}").unwrap();
    assert!(
        refs.iter()
            .any(|r| matches!(r, Reference::Resource { name, .. } if name == "db")),
        "db should surface as a nested resource reference: {refs:?}"
    );
}

#[test]
fn top_level_escape_still_literal() {
    let ctx = InterpolationContext::new();
    let interp = Interpolator::new(&ctx);
    let out = interp.resolve("literal ${{ env.X }} kept").unwrap();
    assert_eq!(out, "literal ${ env.X } kept");
}
