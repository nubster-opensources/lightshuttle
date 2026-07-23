//! Percent encoding of URL components, checked against an external parser.
//!
//! Asserting the encoded string against a literal only proves the encoder
//! agrees with itself. The property that matters is that a conforming parser
//! reads back exactly the value that went in, so every case here rebuilds a
//! URL and asks the `url` crate what it sees.

use lightshuttle_manifest::canonical::{encode_path_segment, encode_userinfo};
use url::Url;

/// Builds a Postgres style URL the way a resource output does, and asks an
/// external parser to hand the components back.
fn round_trip(user: &str, password: &str, database: &str) -> (String, String, String) {
    let raw = format!(
        "postgres://{}:{}@db:5432/{}",
        encode_userinfo(user),
        encode_userinfo(password),
        encode_path_segment(database)
    );
    let parsed = Url::parse(&raw).unwrap_or_else(|error| panic!("`{raw}` must parse: {error}"));

    assert_eq!(parsed.host_str(), Some("db"), "the host must not move");
    assert_eq!(parsed.port(), Some(5432), "the port must not move");

    let decode = |value: &str| {
        percent_decode(value).unwrap_or_else(|| panic!("`{value}` must decode as UTF-8"))
    };
    let path = parsed
        .path()
        .strip_prefix('/')
        .unwrap_or_default()
        .to_owned();
    (
        decode(parsed.username()),
        decode(parsed.password().unwrap_or_default()),
        decode(&path),
    )
}

/// Decodes a percent encoded value, independently of the encoder under test.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[test]
fn ordinary_credentials_keep_the_current_url_shape() {
    assert_eq!(encode_userinfo("appuser"), "appuser");
    assert_eq!(encode_userinfo("s3cret"), "s3cret");
    assert_eq!(encode_path_segment("appdb"), "appdb");
}

#[test]
fn the_unreserved_set_is_never_escaped() {
    let unreserved = "abcXYZ0189-._~";
    assert_eq!(encode_userinfo(unreserved), unreserved);
    assert_eq!(encode_path_segment(unreserved), unreserved);
}

// The defect of issue #286: the first `@` closed the userinfo, so the host
// became `ss@db` and the connection went nowhere the manifest described.
#[test]
fn an_at_sign_in_a_password_does_not_move_the_host() {
    let (user, password, database) = round_trip("appuser", "p@ss", "appdb");
    assert_eq!(user, "appuser");
    assert_eq!(password, "p@ss");
    assert_eq!(database, "appdb");
}

#[test]
fn a_colon_in_a_user_does_not_split_the_userinfo() {
    let (user, password, _) = round_trip("ad:min", "secret", "appdb");
    assert_eq!(user, "ad:min");
    assert_eq!(password, "secret");
}

#[test]
fn a_slash_in_a_password_does_not_open_the_path() {
    let (_, password, database) = round_trip("appuser", "a/b", "appdb");
    assert_eq!(password, "a/b");
    assert_eq!(database, "appdb");
}

#[test]
fn a_question_mark_in_a_password_does_not_open_a_query() {
    let (_, password, database) = round_trip("appuser", "why?", "appdb");
    assert_eq!(password, "why?");
    assert_eq!(database, "appdb");
}

#[test]
fn a_hash_in_a_password_does_not_truncate_the_url() {
    let (_, password, database) = round_trip("appuser", "a#b", "appdb");
    assert_eq!(password, "a#b");
    assert_eq!(database, "appdb");
}

#[test]
fn a_percent_sign_in_a_password_is_escaped_rather_than_read_as_an_escape() {
    let (_, password, _) = round_trip("appuser", "100%pure", "appdb");
    assert_eq!(password, "100%pure");
    assert_eq!(encode_userinfo("%"), "%25");
}

#[test]
fn a_slash_in_a_database_name_does_not_add_a_path_segment() {
    let (_, _, database) = round_trip("appuser", "secret", "main/db");
    assert_eq!(database, "main/db");
}

#[test]
fn every_reserved_delimiter_round_trips() {
    // The full RFC 3986 gen-delims and sub-delims sets.
    for delimiter in [
        ":", "/", "?", "#", "[", "]", "@", "!", "$", "&", "'", "(", ")", "*", "+", ",", ";", "=",
    ] {
        let password = format!("a{delimiter}b");
        let (_, decoded, _) = round_trip("appuser", &password, "appdb");
        assert_eq!(decoded, password, "delimiter `{delimiter}` did not survive");
    }
}

#[test]
fn a_space_round_trips() {
    let (_, password, _) = round_trip("appuser", "two words", "appdb");
    assert_eq!(password, "two words");
}

#[test]
fn unicode_round_trips() {
    let (user, password, database) = round_trip("élève", "mot de passe é", "basé");
    assert_eq!(user, "élève");
    assert_eq!(password, "mot de passe é");
    assert_eq!(database, "basé");
}

#[test]
fn an_empty_value_encodes_to_an_empty_string() {
    assert_eq!(encode_userinfo(""), "");
    assert_eq!(encode_path_segment(""), "");
}

#[test]
fn a_value_made_only_of_delimiters_carries_no_delimiter_through() {
    let encoded = encode_userinfo(":/?#[]@");
    assert!(
        !encoded.contains([':', '/', '?', '#', '[', ']', '@']),
        "`{encoded}` still carries a delimiter"
    );
}
