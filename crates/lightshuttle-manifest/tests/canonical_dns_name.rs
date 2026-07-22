//! Behaviour of the canonical DNS label normalisation.
//!
//! The defect these tests close is a silent one: two distinct manifest names
//! produced one identifier, so an exported artifact overwrote another or two
//! projects shared a Docker network. Nothing failed, so nothing was visible.
//! The assertions therefore centre on names *not* converging, which is a
//! property, rather than on any particular output string.

use lightshuttle_manifest::canonical::{DnsName, DnsNameError, is_dns_label};

fn normalise(name: &str) -> String {
    DnsName::from_manifest_name(name)
        .unwrap_or_else(|error| panic!("`{name}` should normalise, got {error}"))
        .as_str()
        .to_owned()
}

// --- Names that must survive untouched --------------------------------------

/// Existing resources must keep the identifiers they already have. A patch
/// that renamed every deployed object would be a migration, not a fix.
#[test]
fn a_name_that_is_already_a_label_passes_through_unchanged() {
    for name in ["api", "my-service", "db2", "a", "postgres-primary"] {
        assert_eq!(normalise(name), name, "`{name}` must not be rewritten");
    }
}

#[test]
fn normalisation_is_idempotent() {
    for name in ["api", "my_service", "trailing-", "Upper_Case"] {
        let once = normalise(name);
        let twice = normalise(&once);
        assert_eq!(once, twice, "normalising `{name}` twice must be stable");
    }
}

#[test]
fn normalisation_is_deterministic() {
    assert_eq!(normalise("my_service"), normalise("my_service"));
}

// --- The collisions that caused the defect ----------------------------------

/// The cause of #281, #284 and #288: both names mapped `_` and `-` onto `-`,
/// so one manifest entry silently replaced the other.
#[test]
fn an_underscore_name_no_longer_converges_with_its_hyphen_twin() {
    assert_ne!(normalise("foo_bar"), normalise("foo-bar"));
}

/// Stripping a trailing hyphen made a third name join the same collision.
#[test]
fn a_trailing_separator_no_longer_converges_with_the_bare_name() {
    let names = ["foo-bar", "foo_bar", "foo-bar-", "foo_bar_"];
    let normalised: Vec<String> = names.iter().map(|name| normalise(name)).collect();
    let unique: std::collections::BTreeSet<&String> = normalised.iter().collect();

    assert_eq!(
        unique.len(),
        names.len(),
        "these four names must not converge, got {normalised:?}"
    );
}

#[test]
fn case_differences_no_longer_converge() {
    let names = ["myservice", "MyService", "MYSERVICE"];
    let normalised: Vec<String> = names.iter().map(|name| normalise(name)).collect();
    let unique: std::collections::BTreeSet<&String> = normalised.iter().collect();

    assert_eq!(unique.len(), names.len(), "got {normalised:?}");
}

/// Truncation was its own collision source: two long names sharing a prefix
/// collapsed onto the same 63 character label.
#[test]
fn long_names_sharing_a_prefix_no_longer_converge() {
    let prefix = "a".repeat(70);
    let first = format!("{prefix}-one");
    let second = format!("{prefix}-two");

    assert_ne!(normalise(&first), normalise(&second));
}

// --- Properties of every produced label -------------------------------------

#[test]
fn every_produced_label_is_a_valid_dns_label() {
    let long = "z".repeat(200);
    for name in [
        "api",
        "my_service",
        "Upper_Case",
        "trailing-",
        "-leading",
        "___",
        "1numeric",
        "a.b.c",
        "with space",
        long.as_str(),
    ] {
        let label = normalise(name);
        assert!(
            is_dns_label(&label),
            "`{name}` produced `{label}`, which is not a valid label"
        );
        assert!(
            label.len() <= 63,
            "`{name}` produced a label of {} characters",
            label.len()
        );
    }
}

/// The property that actually matters: across a set of names chosen to stress
/// every normalisation rule at once, no two converge.
#[test]
fn a_stress_set_of_names_produces_no_collision() {
    let names = [
        "foo-bar", "foo_bar", "foo-bar-", "foo_bar_", "foobar", "FooBar", "foo.bar", "foo bar",
        "foo--bar", "foo__bar",
    ];
    let normalised: Vec<String> = names.iter().map(|name| normalise(name)).collect();
    let unique: std::collections::BTreeSet<&String> = normalised.iter().collect();

    assert_eq!(
        unique.len(),
        names.len(),
        "collision inside the stress set: {normalised:?}"
    );
}

#[test]
fn an_empty_name_is_rejected() {
    assert_eq!(
        DnsName::from_manifest_name(""),
        Err(DnsNameError::Empty),
        "an empty name has nothing to normalise"
    );
}

// --- Strict parsing, for values the user wrote deliberately ------------------

#[test]
fn parse_accepts_a_valid_label() {
    for value in ["api", "my-service", "db2", "a", &"a".repeat(63)] {
        assert!(
            DnsName::parse(value).is_ok(),
            "`{value}` is a valid label and should be accepted"
        );
    }
}

/// An explicit override is an intention. Rewriting it silently would hand the
/// user an identifier they never wrote and cannot predict, so it is refused.
#[test]
fn parse_rejects_anything_that_is_not_a_label() {
    for value in [
        "My Project",
        "my_project",
        "-leading",
        "trailing-",
        "UPPER",
        "a.b",
        &"a".repeat(64),
    ] {
        assert!(
            matches!(
                DnsName::parse(value),
                Err(DnsNameError::NotALabel { .. } | DnsNameError::Empty)
            ),
            "`{value}` is not a label and should be rejected"
        );
    }
}

#[test]
fn parse_rejects_an_empty_value() {
    assert_eq!(DnsName::parse(""), Err(DnsNameError::Empty));
}

#[test]
fn parse_is_the_inverse_of_recognition() {
    for value in ["api", "my-service", "My Project", "my_project", ""] {
        assert_eq!(
            DnsName::parse(value).is_ok(),
            is_dns_label(value),
            "`{value}`: parse and is_dns_label must agree"
        );
    }
}
