//! Percent encoding for the credentials that resource URLs are built from.
//!
//! Postgres and Redis outputs used to build their connection URL by plain
//! string formatting. A password of `p@ss` produced
//! `postgres://user:p@ss@host:5432/db`, where the first `@` is read as the end
//! of the userinfo, so the host becomes `ss@host`. The same password reached
//! the container correctly, so the service worked and only the URL lied.
//!
//! Every reserved character has this shape: it does not corrupt the value, it
//! relocates a boundary. A `/` in a password moves the start of the path, a `?`
//! opens a query string, a `#` truncates the URL at a fragment. None of these
//! fail loudly.
//!
//! The encoding here keeps only the RFC 3986 unreserved set and escapes
//! everything else, including characters a userinfo or a path segment would
//! technically tolerate. A wider allowance would be correct and harder to
//! prove; this one round trips through any conforming parser, which is the
//! property the tests assert against an external URL parser.

/// Percent encodes a userinfo component, such as a user name or a password.
///
/// The `:` that separates the user from the password and the `@` that closes
/// the userinfo are escaped, so a value carrying either cannot move the
/// boundary it sits next to.
#[must_use]
pub fn encode_userinfo(value: &str) -> String {
    encode_unreserved_only(value)
}

/// Percent encodes a path segment, such as a database name.
///
/// The `/` that separates segments is escaped, so a value carrying one cannot
/// introduce a segment the manifest never declared.
#[must_use]
pub fn encode_path_segment(value: &str) -> String {
    encode_unreserved_only(value)
}

/// Escapes every byte outside the RFC 3986 unreserved set.
///
/// Userinfo and path segments each tolerate a slightly wider set, and the two
/// sets differ. Encoding both down to the unreserved set costs a few escapes
/// on values nobody types and removes the need to track which component a
/// value is heading for, which is the mistake that produced the defect.
fn encode_unreserved_only(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if is_unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
    }
    encoded
}

/// Reports whether a byte is in the RFC 3986 unreserved set.
///
/// Non ASCII bytes are never unreserved: a UTF-8 sequence is escaped byte by
/// byte, which is how a conforming parser expects to read it back.
const fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

/// Renders a nibble as an uppercase hexadecimal digit.
const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + nibble - 10) as char,
    }
}
