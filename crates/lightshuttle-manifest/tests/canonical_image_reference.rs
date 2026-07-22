//! Behaviour of the canonical OCI image reference parser.
//!
//! These tests assert parsed values and structure, never a formatted output
//! string. The defects this parser closes were invisible to the existing
//! emitter tests precisely because those assert rendered YAML rather than the
//! grammar underneath it.

use lightshuttle_manifest::canonical::{ImageReference, ImageReferenceError};

/// A syntactically valid digest, used wherever the digest itself is not what
/// the test is about.
const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn parse(reference: &str) -> ImageReference {
    ImageReference::parse(reference).unwrap_or_else(|error| {
        panic!("`{reference}` should parse, got {error}");
    })
}

fn error(reference: &str) -> ImageReferenceError {
    match ImageReference::parse(reference) {
        Ok(parsed) => panic!("`{reference}` should be rejected, parsed as {parsed:?}"),
        Err(error) => error,
    }
}

// --- The shapes that already worked -----------------------------------------

#[test]
fn a_bare_repository_carries_neither_registry_nor_tag() {
    let image = parse("postgres");

    assert_eq!(image.registry(), None);
    assert_eq!(image.repository(), "postgres");
    assert_eq!(image.tag(), None);
    assert_eq!(image.digest(), None);
    assert_eq!(image.qualified_repository(), "postgres");
}

#[test]
fn a_tagged_repository_separates_repository_from_tag() {
    let image = parse("postgres:16");

    assert_eq!(image.registry(), None);
    assert_eq!(image.repository(), "postgres");
    assert_eq!(image.tag(), Some("16"));
    assert_eq!(image.qualified_repository(), "postgres");
}

#[test]
fn a_namespaced_repository_is_not_mistaken_for_a_registry() {
    // `myteam` has no dot and no port, so it is a path component on the
    // default registry, not a registry host.
    let image = parse("myteam/api:1.2");

    assert_eq!(image.registry(), None);
    assert_eq!(image.repository(), "myteam/api");
    assert_eq!(image.tag(), Some("1.2"));
}

// --- The shapes the hand rolled splits corrupted -----------------------------

/// `docker.rs` split on the first colon, so the registry port became the tag
/// and the pull targeted a repository that does not exist.
#[test]
fn a_registry_port_is_part_of_the_registry_not_the_tag() {
    let image = parse("registry.example.com:5000/team/api:1.2");

    assert_eq!(image.registry(), Some("registry.example.com:5000"));
    assert_eq!(image.repository(), "team/api");
    assert_eq!(image.tag(), Some("1.2"));
    assert_eq!(image.digest(), None);
    assert_eq!(
        image.qualified_repository(),
        "registry.example.com:5000/team/api"
    );
}

/// `helm.rs` split on the last colon, so an untagged reference served by a
/// registry on a custom port lost its repository path into the tag.
#[test]
fn an_untagged_reference_on_a_ported_registry_keeps_its_full_repository() {
    let image = parse("registry.example.com:5000/team/api");

    assert_eq!(image.registry(), Some("registry.example.com:5000"));
    assert_eq!(image.repository(), "team/api");
    assert_eq!(image.tag(), None);
    assert_eq!(
        image.qualified_repository(),
        "registry.example.com:5000/team/api"
    );
}

/// `helm.rs` split on the last colon, so the digest algorithm was welded onto
/// the repository and the hexadecimal digest became the tag.
#[test]
fn a_digest_pinned_reference_keeps_the_digest_out_of_the_repository() {
    let image = parse(&format!("alpine@{DIGEST}"));

    assert_eq!(image.registry(), None);
    assert_eq!(image.repository(), "alpine");
    assert_eq!(image.tag(), None);
    assert_eq!(image.digest(), Some(DIGEST));
    assert_eq!(image.qualified_repository(), "alpine");
}

#[test]
fn a_ported_registry_and_a_digest_are_disambiguated_together() {
    let image = parse(&format!("registry.example.com:5000/team/api@{DIGEST}"));

    assert_eq!(image.registry(), Some("registry.example.com:5000"));
    assert_eq!(image.repository(), "team/api");
    assert_eq!(image.tag(), None);
    assert_eq!(image.digest(), Some(DIGEST));
}

/// The OCI grammar allows a tag and a digest on the same reference. The tag is
/// then informational and the digest is what is resolved.
#[test]
fn a_reference_may_carry_both_a_tag_and_a_digest() {
    let image = parse(&format!("postgres:16@{DIGEST}"));

    assert_eq!(image.repository(), "postgres");
    assert_eq!(image.tag(), Some("16"));
    assert_eq!(image.digest(), Some(DIGEST));
}

// --- Registry detection ------------------------------------------------------

#[test]
fn localhost_is_a_registry_even_without_a_dot_or_a_port() {
    let image = parse("localhost/api:1.0");

    assert_eq!(image.registry(), Some("localhost"));
    assert_eq!(image.repository(), "api");
}

#[test]
fn a_host_with_a_port_is_a_registry_even_without_a_dot() {
    let image = parse("registry:5000/api");

    assert_eq!(image.registry(), Some("registry:5000"));
    assert_eq!(image.repository(), "api");
}

#[test]
fn a_registry_may_serve_a_deeply_nested_repository() {
    let image = parse("ghcr.example.org/org/team/service:sha-abc123");

    assert_eq!(image.registry(), Some("ghcr.example.org"));
    assert_eq!(image.repository(), "org/team/service");
    assert_eq!(image.tag(), Some("sha-abc123"));
}

// --- Rejections --------------------------------------------------------------

#[test]
fn an_empty_reference_is_rejected() {
    assert_eq!(error(""), ImageReferenceError::Empty);
    assert_eq!(error("   "), ImageReferenceError::Empty);
}

#[test]
fn a_reference_without_a_repository_is_rejected() {
    assert!(matches!(
        error(":16"),
        ImageReferenceError::InvalidRepository { .. }
    ));
    assert!(matches!(
        error("registry.example.com:5000/"),
        ImageReferenceError::InvalidRepository { .. }
    ));
    assert!(matches!(
        error("team//api"),
        ImageReferenceError::InvalidRepository { .. }
    ));
}

/// A container runtime rejects an uppercase repository, so accepting one here
/// would only postpone the failure to a point where the message is opaque.
#[test]
fn an_uppercase_repository_is_rejected() {
    assert!(matches!(
        error("Postgres:16"),
        ImageReferenceError::InvalidRepository { .. }
    ));
}

#[test]
fn an_empty_tag_is_rejected() {
    assert!(matches!(
        error("postgres:"),
        ImageReferenceError::InvalidTag { .. }
    ));
}

#[test]
fn a_tag_starting_with_a_separator_is_rejected() {
    assert!(matches!(
        error("postgres:.16"),
        ImageReferenceError::InvalidTag { .. }
    ));
    assert!(matches!(
        error("postgres:-16"),
        ImageReferenceError::InvalidTag { .. }
    ));
}

#[test]
fn a_tag_longer_than_the_grammar_allows_is_rejected() {
    let long_tag = "a".repeat(129);

    assert!(matches!(
        error(&format!("postgres:{long_tag}")),
        ImageReferenceError::InvalidTag { .. }
    ));
}

#[test]
fn a_digest_without_an_algorithm_is_rejected() {
    assert!(matches!(
        error("alpine@0123456789abcdef"),
        ImageReferenceError::InvalidDigest { .. }
    ));
}

#[test]
fn a_digest_whose_payload_is_not_hexadecimal_is_rejected() {
    let not_hex = "z".repeat(64);

    assert!(matches!(
        error(&format!("alpine@sha256:{not_hex}")),
        ImageReferenceError::InvalidDigest { .. }
    ));
}

#[test]
fn a_truncated_digest_is_rejected() {
    assert!(matches!(
        error("alpine@sha256:abc123"),
        ImageReferenceError::InvalidDigest { .. }
    ));
}

#[test]
fn an_empty_digest_is_rejected() {
    assert!(matches!(
        error("alpine@"),
        ImageReferenceError::InvalidDigest { .. }
    ));
}

// --- Properties --------------------------------------------------------------

/// The qualified repository is the input a container runtime is handed, so it
/// has to parse back to the same registry and repository. A parser that loses
/// the distinction here would corrupt exactly one hop later.
#[test]
fn the_qualified_repository_parses_back_to_the_same_registry_and_repository() {
    for reference in [
        "postgres",
        "postgres:16",
        "myteam/api:1.2",
        "localhost:5000/api",
        "registry.example.com:5000/team/api:1.2",
        "ghcr.example.org/org/team/service",
    ] {
        let image = parse(reference);
        let round_tripped = parse(&image.qualified_repository());

        assert_eq!(
            round_tripped.registry(),
            image.registry(),
            "registry drifted for `{reference}`"
        );
        assert_eq!(
            round_tripped.repository(),
            image.repository(),
            "repository drifted for `{reference}`"
        );
        assert_eq!(
            round_tripped.tag(),
            None,
            "a qualified repository must not carry a tag, for `{reference}`"
        );
    }
}

/// Two references that differ must not collapse onto the same qualified
/// repository and tag. This is the property the hand rolled splits broke:
/// `registry.example.com:5000/team/api` and `registry.example.com` both
/// reduced to the same repository under the old code.
#[test]
fn distinct_references_do_not_collapse_onto_the_same_pull_target() {
    let references = [
        "postgres:16",
        "postgres:17",
        "registry.example.com:5000/team/api:1.2",
        "registry.example.com:5000/team/api:1.3",
        "registry.example.com/team/api:1.2",
        "team/api:1.2",
    ];

    let mut targets = Vec::with_capacity(references.len());
    for reference in references {
        let image = parse(reference);
        targets.push((image.qualified_repository(), image.tag().map(str::to_owned)));
    }

    let unique: std::collections::BTreeSet<_> = targets.iter().collect();
    assert_eq!(
        unique.len(),
        references.len(),
        "two distinct references resolved to the same pull target: {targets:?}"
    );
}
