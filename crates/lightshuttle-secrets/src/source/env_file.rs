//! `.env` file source implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::SecretError;
use crate::source::SecretSource;

/// Loads secrets from a `.env` file.
///
/// The file is parsed once at construction time. Supported syntax:
///
/// - `KEY=VALUE`: plain value
/// - `KEY="quoted value"`: double-quoted (quotes stripped)
/// - `KEY='quoted value'`: single-quoted (quotes stripped)
/// - `export KEY=VALUE`: optional `export` prefix, followed by spaces or tabs (ignored)
/// - `# comment`: ignored
/// - Blank lines: ignored
/// - Inline comments: `KEY=VALUE # comment` (unquoted values only)
#[derive(Debug)]
pub struct EnvFileSource {
    path: PathBuf,
    entries: HashMap<String, String>,
}

impl EnvFileSource {
    /// Load from `path`.
    ///
    /// Returns [`SecretError::FileNotFound`] if the file does not exist.
    /// Use this when the path was explicitly provided by the user (e.g. via
    /// `--env-file`).
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, SecretError> {
        let path = path.into();
        if !path.exists() {
            return Err(SecretError::FileNotFound(path));
        }
        let entries = parse_env_file(&path)?;
        Ok(Self { path, entries })
    }

    /// Load from `path` if it exists, or return `None` if the file is absent.
    ///
    /// Use this for the default `.env` path so that projects without a `.env`
    /// file are not forced to create one.
    pub fn load_optional(path: impl Into<PathBuf>) -> Result<Option<Self>, SecretError> {
        let path = path.into();
        if !path.exists() {
            return Ok(None);
        }
        let entries = parse_env_file(&path)?;
        Ok(Some(Self { path, entries }))
    }

    /// Number of entries loaded from the file.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the file contained no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl SecretSource for EnvFileSource {
    fn load(&self) -> Result<HashMap<String, String>, SecretError> {
        Ok(self.entries.clone())
    }

    fn source_name(&self) -> &str {
        self.path.to_str().unwrap_or(".env")
    }
}

fn parse_env_file(path: &Path) -> Result<HashMap<String, String>, SecretError> {
    let content = std::fs::read_to_string(path).map_err(|source| SecretError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    // Editors on Windows frequently prepend a UTF-8 byte-order mark; strip it
    // so the first key is not silently misnamed with a leading `\u{feff}`.
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);

    let mut map = HashMap::new();

    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = strip_export_prefix(line);

        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(SecretError::InvalidSyntax {
                path: path.to_path_buf(),
                line: idx + 1,
                message: format!("expected KEY=VALUE, got `{line}`"),
            });
        };

        let key = key.trim();
        if key.is_empty() {
            return Err(SecretError::InvalidSyntax {
                path: path.to_path_buf(),
                line: idx + 1,
                message: "empty key".to_owned(),
            });
        }

        let value = unescape_value(raw_value.trim());
        map.insert(key.to_owned(), value);
    }

    Ok(map)
}

/// Strip an optional `export` keyword followed by horizontal whitespace.
///
/// `export KEY=VALUE` and `export<TAB>KEY=VALUE` both yield `KEY=VALUE`, while
/// `exportKEY=VALUE` is left untouched because `export` is part of the key.
fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export")
        .filter(|rest| rest.starts_with([' ', '\t']))
        .map_or(line, |rest| rest.trim_start_matches([' ', '\t']))
}

fn unescape_value(s: &str) -> String {
    let s = s.trim();

    // Quoted value: return the span between the opening quote and its first
    // matching closing quote, discarding any trailing inline comment. This
    // lets a quoted value contain ` #` without being truncated.
    if let Some(quote) = s.chars().next().filter(|c| *c == '"' || *c == '\'') {
        if let Some(end) = s[1..].find(quote) {
            return s[1..=end].to_owned();
        }
        // No closing quote: fall through and treat the value literally.
    }

    // Unquoted value: strip a trailing ` #` inline comment.
    if let Some((value, _comment)) = s.split_once(" #") {
        value.trim_end().to_owned()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn write_env(content: &str) -> (tempfile::NamedTempFile, PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let path = f.path().to_path_buf();
        (f, path)
    }

    #[test]
    fn parse_plain_key_value() {
        let (_f, path) = write_env("DB_URL=postgres://localhost/db\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["DB_URL"], "postgres://localhost/db");
    }

    #[test]
    fn parse_double_quoted_value() {
        let (_f, path) = write_env("SECRET=\"hello world\"\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["SECRET"], "hello world");
    }

    #[test]
    fn parse_single_quoted_value() {
        let (_f, path) = write_env("TOKEN='abc123'\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["TOKEN"], "abc123");
    }

    #[test]
    fn skip_comments_and_blank_lines() {
        let (_f, path) = write_env("# comment\n\nKEY=val\n");
        let src = EnvFileSource::load(&path).unwrap();
        assert_eq!(src.len(), 1);
    }

    #[test]
    fn strip_export_prefix() {
        let (_f, path) = write_env("export API_KEY=secret\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["API_KEY"], "secret");
    }

    #[test]
    fn strip_inline_comment() {
        let (_f, path) = write_env("PORT=8080 # default port\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["PORT"], "8080");
    }

    #[test]
    fn load_optional_absent_returns_none() {
        let result = EnvFileSource::load_optional("/nonexistent/.env").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_explicit_absent_returns_error() {
        let err = EnvFileSource::load("/nonexistent/.env").unwrap_err();
        assert!(matches!(err, SecretError::FileNotFound(_)));
    }

    #[test]
    fn invalid_line_returns_error() {
        let (_f, path) = write_env("NOT_A_VALID_LINE\n");
        let err = EnvFileSource::load(&path).unwrap_err();
        assert!(matches!(err, SecretError::InvalidSyntax { line: 1, .. }));
    }

    #[test]
    fn strips_utf8_bom_from_first_key() {
        let (_f, path) = write_env("\u{feff}FIRST=value\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["FIRST"], "value");
        assert!(!map.contains_key("\u{feff}FIRST"));
    }

    #[test]
    fn quoted_value_with_inline_comment_drops_the_comment_and_quotes() {
        let (_f, path) = write_env("KEY=\"val\" # trailing comment\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["KEY"], "val");
    }

    #[test]
    fn hash_inside_quotes_is_preserved() {
        let (_f, path) = write_env("PASSWORD=\"a b#c #d\"\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["PASSWORD"], "a b#c #d");
    }

    #[test]
    fn unquoted_value_without_space_hash_keeps_fragment() {
        let (_f, path) = write_env("URL=https://example.com/p#frag\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["URL"], "https://example.com/p#frag");
    }

    #[test]
    fn strip_export_prefix_with_tab() {
        let (_f, path) = write_env("export\tAPI_KEY=secret\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["API_KEY"], "secret");
    }

    #[test]
    fn export_glued_to_key_is_not_stripped() {
        let (_f, path) = write_env("exportAPI_KEY=secret\n");
        let src = EnvFileSource::load(&path).unwrap();
        let map = SecretSource::load(&src).unwrap();
        assert_eq!(map["exportAPI_KEY"], "secret");
    }
}
