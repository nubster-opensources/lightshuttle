//! Validation and lowering must read a duration the same way.
//!
//! `lightshuttle validate` exists to make one promise: what it accepts will
//! not be refused later. Two parsers broke that promise silently, because the
//! looser one ran first. This suite asserts the promise directly rather than
//! asserting that a particular parser exists.

use lightshuttle_export::lower;
use lightshuttle_manifest::Manifest;

fn manifest_with(interval: &str) -> String {
    format!(
        r#"
project:
  name: shop
resources:
  api:
    container:
      image: alpine:3.20
      healthcheck:
        test: ["CMD", "true"]
        interval: "{interval}"
"#
    )
}

/// The forms that used to slip through: the validation pass accepted any run
/// of digits and dots, and none of these is a number.
const MALFORMED: [&str; 5] = [".s", "1..2s", "..5s", "1.2.3s", "10x"];

const WELL_FORMED: [&str; 8] = ["1ns", "200ms", "1s", "1.5s", ".5s", "5.s", "2m", "1h"];

#[test]
fn a_malformed_duration_is_refused_by_validation() {
    for interval in MALFORMED {
        assert!(
            Manifest::parse(&manifest_with(interval)).is_err(),
            "`{interval}` should be refused by validation"
        );
    }
}

/// The property that closes the drift. If validation ever accepts a value that
/// lowering refuses, this fails, whatever the two implementations happen to be.
#[test]
fn anything_validation_accepts_survives_lowering() {
    for interval in WELL_FORMED.iter().chain(MALFORMED.iter()) {
        let Ok(manifest) = Manifest::parse(&manifest_with(interval)) else {
            // Refused up front, which is the other half of the contract.
            continue;
        };
        assert!(
            lower(&manifest).is_ok(),
            "`{interval}` passed validation and was then refused by lowering"
        );
    }
}

#[test]
fn a_well_formed_duration_is_accepted_by_both_stages() {
    for interval in WELL_FORMED {
        let manifest = Manifest::parse(&manifest_with(interval))
            .unwrap_or_else(|error| panic!("`{interval}` should validate, got {error}"));
        lower(&manifest).unwrap_or_else(|error| panic!("`{interval}` should lower, got {error}"));
    }
}
