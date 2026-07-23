//! Behaviour of the single duration grammar.
//!
//! Two defects meet here. One is drift: two parsers read the same string
//! differently, so `validate` accepted what `up` refused. The other is silent
//! truncation: a sub second duration became zero on the way to a Kubernetes
//! probe, and a probe with a period of zero does not fail loudly, it leaves
//! the container permanently unready.

use std::time::Duration;

use lightshuttle_manifest::canonical::{DurationError, parse_duration, to_whole_seconds};

fn parsed(value: &str) -> Duration {
    parse_duration(value).unwrap_or_else(|error| panic!("`{value}` should parse, got {error}"))
}

fn rejected(value: &str) -> DurationError {
    match parse_duration(value) {
        Ok(duration) => panic!("`{value}` should be rejected, parsed as {duration:?}"),
        Err(error) => error,
    }
}

// --- The grammar -------------------------------------------------------------

#[test]
fn every_unit_converts_to_the_expected_duration() {
    assert_eq!(parsed("5ns"), Duration::from_nanos(5));
    assert_eq!(parsed("5us"), Duration::from_micros(5));
    assert_eq!(parsed("200ms"), Duration::from_millis(200));
    assert_eq!(parsed("30s"), Duration::from_secs(30));
    assert_eq!(parsed("2m"), Duration::from_secs(120));
    assert_eq!(parsed("1h"), Duration::from_secs(3_600));
}

#[test]
fn a_fractional_value_keeps_its_precision() {
    assert_eq!(parsed("1.5s"), Duration::from_millis(1_500));
    assert_eq!(parsed("0.25m"), Duration::from_secs(15));
    assert_eq!(parsed("2.5ms"), Duration::from_micros(2_500));
}

/// Both abbreviated decimal forms are valid numbers and were already accepted
/// by lowering, so narrowing the grammar here would break working manifests.
#[test]
fn abbreviated_decimal_forms_are_accepted() {
    assert_eq!(parsed(".5s"), Duration::from_millis(500));
    assert_eq!(parsed("5.s"), Duration::from_secs(5));
}

#[test]
fn surrounding_whitespace_is_ignored() {
    assert_eq!(parsed("  30s  "), Duration::from_secs(30));
}

#[test]
fn zero_is_a_duration() {
    assert_eq!(parsed("0s"), Duration::ZERO);
}

// --- The drift between the two parsers (#279) --------------------------------

/// These three passed `lightshuttle validate`, whose character class accepted
/// any run of digits and dots, and were then rejected at `up` when lowering
/// handed the same prefix to a float parser. None of them is a number.
#[test]
fn the_forms_that_validate_used_to_accept_are_now_rejected_up_front() {
    for value in [".s", "1..2s", "..5s", "1.2.3s"] {
        assert!(
            matches!(rejected(value), DurationError::Malformed { .. }),
            "`{value}` should be reported as malformed"
        );
    }
}

#[test]
fn an_empty_duration_is_rejected() {
    assert_eq!(rejected(""), DurationError::Empty);
    assert_eq!(rejected("   "), DurationError::Empty);
}

#[test]
fn a_missing_or_unknown_unit_is_rejected() {
    for value in ["10", "10x", "10sec", "s"] {
        assert!(
            matches!(
                rejected(value),
                DurationError::UnknownUnit { .. } | DurationError::Malformed { .. }
            ),
            "`{value}` should be rejected"
        );
    }
}

#[test]
fn a_negative_duration_is_rejected() {
    assert!(matches!(
        rejected("-5s"),
        DurationError::UnknownUnit { .. } | DurationError::Malformed { .. }
    ));
}

// --- Boundaries (#279 acceptance criteria) -----------------------------------

/// A float based parser saturated here instead of reporting, so an absurd
/// value silently became the largest representable duration.
#[test]
fn a_value_too_large_to_represent_is_reported_rather_than_saturated() {
    assert!(matches!(
        rejected("99999999999999999999999h"),
        DurationError::Overflow { .. }
    ));
}

/// A positive value below the resolution of `Duration` would become zero.
/// Returning zero is exactly the silent collapse this module exists to stop.
#[test]
fn a_positive_value_below_one_nanosecond_is_reported_rather_than_zeroed() {
    assert!(matches!(
        rejected("0.5ns"),
        DurationError::BelowResolution { .. }
    ));
}

// --- Whole second conversion (#285) ------------------------------------------

/// `as_secs` floored this to zero, and Kubernetes requires `periodSeconds` and
/// `timeoutSeconds` to be at least one, so the emitted object was rejected by
/// the API.
#[test]
fn a_sub_second_duration_never_becomes_zero_seconds() {
    assert_eq!(to_whole_seconds(Duration::from_millis(200)), 1);
    assert_eq!(to_whole_seconds(Duration::from_nanos(1)), 1);
    assert_eq!(to_whole_seconds(Duration::ZERO), 1);
}

#[test]
fn a_whole_number_of_seconds_is_unchanged() {
    assert_eq!(to_whole_seconds(Duration::from_secs(30)), 30);
    assert_eq!(to_whole_seconds(Duration::from_secs(1)), 1);
}

/// Rounding up rather than down keeps the emitted probe at least as patient as
/// the manifest asked for. Rounding down would make it stricter than declared.
#[test]
fn a_fractional_second_rounds_up() {
    assert_eq!(to_whole_seconds(Duration::from_millis(1_001)), 2);
    assert_eq!(to_whole_seconds(Duration::from_millis(1_999)), 2);
}

#[test]
fn an_enormous_duration_saturates_rather_than_wrapping() {
    assert_eq!(to_whole_seconds(Duration::from_secs(u64::MAX)), u32::MAX);
}

// --- The property that ties the two issues together --------------------------

/// The contract `validate` exists to make: anything it accepts must survive
/// every later stage. One parser is what makes that true by construction.
#[test]
fn every_accepted_duration_yields_a_usable_probe_period() {
    for value in ["1ns", "200ms", "1s", "1.5s", ".5s", "30s", "2m", "1h"] {
        let period = to_whole_seconds(parsed(value));
        assert!(
            period >= 1,
            "`{value}` produced a probe period of {period}, which Kubernetes rejects"
        );
    }
}
