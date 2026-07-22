//! The Helm chart must carry image coordinates that survive a registry port
//! and a digest.
//!
//! These assertions read the emitted `values.yaml` as a parsed document rather
//! than as text. The golden file tests next door assert exact formatting, and
//! that is precisely why they never caught these defects: they check how the
//! chart is laid out, never what the values mean.

use lightshuttle_export::{Emitter, HelmEmitter, lower};
use lightshuttle_manifest::Manifest;
use serde_norway::Value;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn values_for(image: &str) -> Value {
    let yaml = format!(
        "project:\n  name: shop\nresources:\n  api:\n    container:\n      image: {image}\n"
    );
    let manifest = Manifest::parse(&yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    let artifacts = HelmEmitter.emit(&model).expect("emit succeeds");
    let values = artifacts
        .files
        .iter()
        .find(|file| file.path.to_str() == Some("values.yaml"))
        .expect("values.yaml is emitted");
    serde_norway::from_str(&values.contents).expect("values.yaml is valid YAML")
}

fn image_field<'a>(values: &'a Value, field: &str) -> Option<&'a str> {
    values["services"]["api"]["image"][field].as_str()
}

#[test]
fn a_plain_tagged_image_keeps_its_repository_and_tag() {
    let values = values_for("alpine:3.20");

    assert_eq!(image_field(&values, "repository"), Some("alpine"));
    assert_eq!(image_field(&values, "tag"), Some("3.20"));
    assert_eq!(image_field(&values, "digest"), None);
}

#[test]
fn an_untagged_image_falls_back_to_the_default_tag() {
    let values = values_for("alpine");

    assert_eq!(image_field(&values, "repository"), Some("alpine"));
    assert_eq!(image_field(&values, "tag"), Some("latest"));
}

/// Splitting on the last colon put the registry port in the tag, so the chart
/// deployed `registry.example.com` at version `5000/team/api`.
#[test]
fn a_registry_port_stays_in_the_repository() {
    let values = values_for("registry.example.com:5000/team/api:1.2");

    assert_eq!(
        image_field(&values, "repository"),
        Some("registry.example.com:5000/team/api")
    );
    assert_eq!(image_field(&values, "tag"), Some("1.2"));
    assert_eq!(image_field(&values, "digest"), None);
}

/// Without a tag, the last colon was the registry port, so the repository was
/// truncated to the host and the port became the version.
#[test]
fn an_untagged_image_on_a_ported_registry_keeps_its_full_repository() {
    let values = values_for("registry.example.com:5000/team/api");

    assert_eq!(
        image_field(&values, "repository"),
        Some("registry.example.com:5000/team/api")
    );
    assert_eq!(image_field(&values, "tag"), Some("latest"));
}

/// Splitting on the last colon welded the digest algorithm onto the
/// repository and made the hexadecimal payload the tag, producing a chart
/// that could not deploy at all.
#[test]
fn a_digest_pinned_image_is_carried_as_a_digest() {
    let values = values_for(&format!("alpine@{DIGEST}"));

    assert_eq!(image_field(&values, "repository"), Some("alpine"));
    assert_eq!(image_field(&values, "digest"), Some(DIGEST));
}

#[test]
fn a_malformed_image_is_reported_rather_than_emitted() {
    let yaml =
        "project:\n  name: shop\nresources:\n  api:\n    container:\n      image: 'Alpine:3.20'\n";
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    let error = HelmEmitter
        .emit(&model)
        .expect_err("an uppercase repository is not a valid image reference");

    assert!(
        error.to_string().contains("api"),
        "the error should name the offending resource, got: {error}"
    );
}
