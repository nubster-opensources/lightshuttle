//! The single duration grammar of the project.
//!
//! Two parsers used to read the same strings differently. Validation checked
//! for a run of digits and dots followed by a unit, so `.s`, `1..2s` and
//! `..5s` passed `lightshuttle validate`; lowering then handed the same prefix
//! to `f64::from_str`, which rejected all three. A manifest could therefore be
//! declared valid and fail at `up`, which is precisely the promise `validate`
//! exists to make.
//!
//! Parsing here is integer arithmetic on `u128` rather than `f64`. A float
//! turns two failure modes into silence: a large value saturates on the cast
//! to `u64`, and a fractional value loses precision without saying so.
//!
//! Conversion to whole seconds is a separate, explicit operation, because the
//! two consumers differ. A container runtime accepts sub second probes; the
//! Kubernetes API requires `periodSeconds` and `timeoutSeconds` to be at least
//! one. Lowering with `Duration::as_secs` silently floored `200ms` to zero and
//! produced an object the API rejects.

use std::fmt;
use std::time::Duration;

/// Nanoseconds in one second, the scale every unit converts through.
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Reasons a duration string cannot be parsed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurationError {
    /// The value is empty or blank.
    Empty,
    /// The numeric part is not a plain decimal.
    Malformed {
        /// The offending value, as supplied.
        value: String,
    },
    /// The unit is missing or not one this grammar knows.
    UnknownUnit {
        /// The offending unit, as supplied.
        unit: String,
    },
    /// The value does not fit in a [`Duration`].
    Overflow {
        /// The offending value, as supplied.
        value: String,
    },
    /// The value is positive but shorter than the one nanosecond resolution of
    /// [`Duration`], so it would silently become zero.
    BelowResolution {
        /// The offending value, as supplied.
        value: String,
    },
}

impl fmt::Display for DurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("duration is empty"),
            Self::Malformed { value } => write!(
                formatter,
                "invalid duration `{value}`: expected a decimal number followed by a unit, such as `30s`, `200ms` or `1.5m`"
            ),
            Self::UnknownUnit { unit } => write!(
                formatter,
                "invalid duration unit `{unit}`: expected one of `ns`, `us`, `ms`, `s`, `m`, `h`"
            ),
            Self::Overflow { value } => {
                write!(formatter, "duration `{value}` is too large to represent")
            }
            Self::BelowResolution { value } => write!(
                formatter,
                "duration `{value}` is positive but shorter than one nanosecond, so it would be indistinguishable from zero"
            ),
        }
    }
}

impl std::error::Error for DurationError {}

/// Parses a manifest duration string such as `30s`, `200ms` or `1.5m`.
///
/// This is the single duration grammar of the project. Validation, lowering
/// and export all go through it, so a duration accepted by `validate` is
/// always accepted by `up` and by `export`.
///
/// # Errors
///
/// Returns a [`DurationError`] when the value is empty, is not a decimal
/// followed by a known unit, does not fit in a [`Duration`], or is positive
/// yet shorter than one nanosecond.
pub fn parse_duration(value: &str) -> Result<Duration, DurationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DurationError::Empty);
    }

    let split_at = trimmed
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split_at);

    let unit_nanos = unit_in_nanos(unit)?;
    let (whole, fraction, fraction_digits) =
        split_decimal(number).ok_or_else(|| DurationError::Malformed {
            value: value.to_owned(),
        })?;

    let scale = pow10(fraction_digits).ok_or_else(|| DurationError::Overflow {
        value: value.to_owned(),
    })?;
    let scaled = whole
        .checked_mul(scale)
        .and_then(|scaled| scaled.checked_add(fraction))
        .ok_or_else(|| DurationError::Overflow {
            value: value.to_owned(),
        })?;
    let nanos = scaled
        .checked_mul(unit_nanos)
        .map(|nanos| nanos / scale)
        .ok_or_else(|| DurationError::Overflow {
            value: value.to_owned(),
        })?;

    // A positive value that rounds to zero nanoseconds is reported rather
    // than returned. Silently collapsing a duration to zero is the very class
    // of defect this module exists to remove.
    if nanos == 0 && scaled > 0 {
        return Err(DurationError::BelowResolution {
            value: value.to_owned(),
        });
    }

    let nanos = u64::try_from(nanos).map_err(|_| DurationError::Overflow {
        value: value.to_owned(),
    })?;
    Ok(Duration::from_nanos(nanos))
}

/// Converts a duration into whole seconds for consumers that only accept
/// integer seconds, such as Kubernetes probes.
///
/// Rounds up, with a floor of one second, so a sub second duration never
/// collapses to zero. `200ms` becomes one second rather than none, which is
/// the closest value the target can express.
#[must_use]
pub fn to_whole_seconds(duration: Duration) -> u32 {
    let nanos = duration.as_nanos();
    let seconds = nanos.div_ceil(NANOS_PER_SECOND).max(1);
    u32::try_from(seconds).unwrap_or(u32::MAX)
}

/// Nanoseconds in one of the units this grammar accepts.
fn unit_in_nanos(unit: &str) -> Result<u128, DurationError> {
    match unit {
        "ns" => Ok(1),
        "us" => Ok(1_000),
        "ms" => Ok(1_000_000),
        "s" => Ok(NANOS_PER_SECOND),
        "m" => Ok(60 * NANOS_PER_SECOND),
        "h" => Ok(3_600 * NANOS_PER_SECOND),
        _ => Err(DurationError::UnknownUnit {
            unit: unit.to_owned(),
        }),
    }
}

/// Splits a plain decimal into its whole part, its fractional digits, and how
/// many fractional digits there were.
///
/// Returns `None` for anything that is not a plain decimal, which is what the
/// old validation pass let through: `.`, `1..2` and `..5` all matched its
/// character class and none of them is a number.
fn split_decimal(number: &str) -> Option<(u128, u128, u32)> {
    if number.is_empty() {
        return None;
    }

    let (whole_text, fraction_text) = match number.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.contains('.') {
                return None;
            }
            (whole, fraction)
        }
        None => (number, ""),
    };

    // At least one side of the dot must carry a digit, so `.5` and `5.` are
    // accepted while a bare `.` is not.
    if whole_text.is_empty() && fraction_text.is_empty() {
        return None;
    }

    let whole = if whole_text.is_empty() {
        0
    } else {
        whole_text.parse().ok()?
    };
    let fraction = if fraction_text.is_empty() {
        0
    } else {
        fraction_text.parse().ok()?
    };
    let fraction_digits = u32::try_from(fraction_text.len()).ok()?;

    Some((whole, fraction, fraction_digits))
}

/// Ten to the power of `exponent`, or `None` when it does not fit.
fn pow10(exponent: u32) -> Option<u128> {
    10_u128.checked_pow(exponent)
}
