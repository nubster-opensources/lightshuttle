//! Unit tests for [`lightshuttle_spec::Argument`].
//!
//! `Argument` is the type a resolver will use to describe one element of a
//! container's `command` or `entrypoint`: either a value safe to copy
//! verbatim, or a reference to an environment key whose value must be
//! looked up instead of written into the argument itself. Every test here
//! is expected to fail until `Argument::literal`, `Argument::secret` and
//! `Argument::resolve` carry a real implementation instead of `todo!()`.

use std::collections::HashMap;

use lightshuttle_spec::Argument;

/// Proves that `Argument::literal` builds a value equal to another literal
/// built from the same text: construction is deterministic and `Argument`
/// is comparable.
#[test]
fn literal_built_from_the_same_text_are_equal() {
    assert_eq!(Argument::literal("redis-server"), Argument::literal("redis-server"));
}

/// Proves that `Argument::secret` builds a value equal to another secret
/// built from the same environment key.
#[test]
fn secrets_built_from_the_same_key_are_equal() {
    assert_eq!(
        Argument::secret("REDIS_PASSWORD"),
        Argument::secret("REDIS_PASSWORD")
    );
}

/// Proves that a literal and a secret carrying the same text are distinct:
/// the variant tag is part of the value's identity, not just its payload.
#[test]
fn a_literal_and_a_secret_with_the_same_text_are_not_equal() {
    assert_ne!(Argument::literal("REDIS_PASSWORD"), Argument::secret("REDIS_PASSWORD"));
}

/// Proves that a `Literal` resolves to its own value, unconditionally: the
/// environment it is handed plays no part in the outcome, including an
/// environment that happens to define a key with the same name.
#[test]
fn a_literal_resolves_to_its_own_value_regardless_of_the_environment() {
    let mut env = HashMap::new();
    env.insert("redis-server".to_owned(), "something-else-entirely".to_owned());

    let argument = Argument::literal("redis-server");
    assert_eq!(argument.resolve(&env), Some("redis-server"));

    let empty_env = HashMap::new();
    assert_eq!(argument.resolve(&empty_env), Some("redis-server"));
}

/// Proves that a `Secret` resolves to the value stored under its
/// environment key, not to the key name itself.
#[test]
fn a_secret_resolves_to_the_value_of_its_environment_key() {
    let mut env = HashMap::new();
    env.insert(
        "REDIS_PASSWORD".to_owned(),
        "NEVER_EXPORT_THIS_REDIS_VALUE_7f3a".to_owned(),
    );

    let argument = Argument::secret("REDIS_PASSWORD");
    assert_eq!(
        argument.resolve(&env),
        Some("NEVER_EXPORT_THIS_REDIS_VALUE_7f3a")
    );
}

/// Proves the case the design exists for: a `Secret` whose environment key
/// is absent resolves to `None`, not to an empty string. A caller that
/// treats `None` as "no environment key declared, refuse the export"
/// cannot be handed a silent empty value to accidentally launch a
/// container with no password instead.
#[test]
fn a_secret_with_no_matching_environment_key_resolves_to_none_not_an_empty_string() {
    let env: HashMap<String, String> = HashMap::new();

    let argument = Argument::secret("REDIS_PASSWORD");
    assert_eq!(argument.resolve(&env), None);
}
