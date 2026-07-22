//! Names that are valid as RFC 1123 DNS labels.
//!
//! Manifest names accept both `_` and `-`, and every downstream normalisation
//! mapped the two onto `-`. The mapping is therefore not injective: `foo_bar`
//! and `foo-bar` are two distinct manifest entries that produced one single
//! identifier, so one silently overwrote the other in an exported artifact or
//! shared a Docker network with it.
//!
//! Two entry points serve two different intents. [`DnsName::parse`] validates
//! a value the user wrote as a label and rejects it when it is not one, so an
//! explicit override is never silently rewritten into something else.
//! [`DnsName::from_manifest_name`] converts a manifest name into a label,
//! appending a deterministic suffix whenever the name is not already one, so
//! two distinct names do not converge.
//!
//! Normalisation alone cannot be *proven* injective: a suffixed name could in
//! principle coincide with a name that was already a valid label, and any
//! digest can collide. It makes convergence vanishingly unlikely; the airtight
//! guarantee has to come from a uniqueness check where the names are emitted.

use std::fmt;

/// Longest RFC 1123 DNS label.
const MAX_LABEL_LENGTH: usize = 63;

/// Length of the hexadecimal suffix appended to a normalised name.
const SUFFIX_LENGTH: usize = 8;

/// A name that is valid as an RFC 1123 DNS label.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DnsName {
    label: String,
}

/// Reasons a name cannot be converted into a DNS label.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsNameError {
    /// The value is empty.
    Empty,
    /// The value was required to be a label already and is not one.
    NotALabel {
        /// The offending value, as supplied.
        value: String,
    },
}

impl fmt::Display for DnsNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("name is empty"),
            Self::NotALabel { value } => write!(
                formatter,
                "`{value}` is not a valid DNS label: expected at most {MAX_LABEL_LENGTH} characters of lowercase alphanumerics and `-`, starting and ending with an alphanumeric"
            ),
        }
    }
}

impl std::error::Error for DnsNameError {}

impl DnsName {
    /// Accepts a value that is already a DNS label, and rejects anything else.
    ///
    /// Used for values the user wrote deliberately, such as an explicit
    /// namespace override. Normalising those would hand the user an identifier
    /// they did not ask for and cannot predict.
    ///
    /// # Errors
    ///
    /// Returns [`DnsNameError::NotALabel`] when the value is not a label.
    pub fn parse(value: &str) -> Result<Self, DnsNameError> {
        if value.is_empty() {
            return Err(DnsNameError::Empty);
        }
        if !is_dns_label(value) {
            return Err(DnsNameError::NotALabel {
                value: value.to_owned(),
            });
        }
        Ok(Self {
            label: value.to_owned(),
        })
    }

    /// Converts an arbitrary manifest name into a DNS label.
    ///
    /// A name that already is a valid label is returned unchanged, so existing
    /// resources keep the identifiers they have. Any other name receives a
    /// deterministic suffix derived from the original, so two names that used
    /// to converge no longer do.
    ///
    /// # Errors
    ///
    /// Returns [`DnsNameError::Empty`] when the name is empty.
    pub fn from_manifest_name(name: &str) -> Result<Self, DnsNameError> {
        if name.is_empty() {
            return Err(DnsNameError::Empty);
        }
        if is_dns_label(name) {
            return Ok(Self {
                label: name.to_owned(),
            });
        }

        // The suffix is derived from the original name, not from the
        // sanitised one, so two names that sanitise identically still differ.
        let suffix = format!("{:08x}", fnv1a_32(name.as_bytes()));
        let budget = MAX_LABEL_LENGTH - SUFFIX_LENGTH - 1;
        let mut base: String = sanitise(name).chars().take(budget).collect();
        while base.ends_with('-') {
            base.pop();
        }

        let label = if base.is_empty() {
            suffix
        } else {
            format!("{base}-{suffix}")
        };
        Ok(Self { label })
    }

    /// Returns the label as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.label
    }
}

impl fmt::Display for DnsName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Tells whether a value is already an RFC 1123 DNS label.
#[must_use]
pub fn is_dns_label(value: &str) -> bool {
    let is_alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();

    if value.is_empty() || value.len() > MAX_LABEL_LENGTH {
        return false;
    }
    let bytes = value.as_bytes();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    if !is_alphanumeric(first) || !is_alphanumeric(last) {
        return false;
    }
    bytes
        .iter()
        .all(|&byte| is_alphanumeric(byte) || byte == b'-')
}

/// Maps a name onto the label alphabet, without any guarantee of injectivity.
///
/// This is the lossy step, which is exactly why its result is never used on
/// its own: the caller pairs it with a digest of the original name.
fn sanitise(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|character| {
            let lowered = character.to_ascii_lowercase();
            if lowered.is_ascii_lowercase() || lowered.is_ascii_digit() {
                lowered
            } else {
                '-'
            }
        })
        .collect();
    mapped.trim_matches('-').to_owned()
}

/// FNV-1a, 32 bit.
///
/// Written out rather than taken from `DefaultHasher`, whose output is not
/// stable across Rust releases. A normalised name has to be identical on every
/// machine and every toolchain, otherwise the same manifest yields different
/// identifiers depending on who ran the export.
fn fnv1a_32(bytes: &[u8]) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;

    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
