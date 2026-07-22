//! Parsing of OCI image references.
//!
//! The colon carries three different meanings in this grammar: it separates a
//! registry host from its port, a repository from its tag, and a digest
//! algorithm from its payload. No fixed split position disambiguates them,
//! which is why splitting on the first or on the last colon corrupts a
//! reference rather than parsing it. The separators are resolved by context
//! instead: the digest is taken first, then the tag is only a tag when no
//! path separator follows it, and only then is the registry distinguished
//! from a leading repository path component.

use std::fmt;

/// Longest tag the OCI distribution grammar accepts.
const MAX_TAG_LENGTH: usize = 128;

/// Shortest digest payload the OCI distribution grammar accepts. It rules out
/// a truncated digest, which would otherwise silently pin nothing.
const MIN_DIGEST_PAYLOAD_LENGTH: usize = 32;

/// A parsed OCI image reference.
///
/// Parsing is the only supported way to build this type, so a value of this
/// type is always a well formed reference.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    registry: Option<String>,
    repository: String,
    tag: Option<String>,
    digest: Option<String>,
}

/// Reasons an image reference cannot be parsed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReferenceError {
    /// The reference is empty or contains only whitespace.
    Empty,
    /// The repository part is missing or malformed.
    InvalidRepository {
        /// The offending reference, as supplied.
        reference: String,
    },
    /// The registry host is malformed.
    InvalidRegistry {
        /// The offending registry, as supplied.
        registry: String,
    },
    /// The tag is empty or uses characters a tag does not accept.
    InvalidTag {
        /// The offending tag, as supplied.
        tag: String,
    },
    /// The digest is not of the form `algorithm:hex`.
    InvalidDigest {
        /// The offending digest, as supplied.
        digest: String,
    },
}

impl fmt::Display for ImageReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("image reference is empty"),
            Self::InvalidRepository { reference } => write!(
                formatter,
                "invalid image reference `{reference}`: expected `[registry[:port]/]repository[:tag][@digest]` with a lowercase repository"
            ),
            Self::InvalidRegistry { registry } => write!(
                formatter,
                "invalid image registry `{registry}`: expected a host name, optionally followed by `:port`"
            ),
            Self::InvalidTag { tag } => write!(
                formatter,
                "invalid image tag `{tag}`: expected at most {MAX_TAG_LENGTH} characters of `[A-Za-z0-9_.-]`, starting with an alphanumeric or `_`"
            ),
            Self::InvalidDigest { digest } => write!(
                formatter,
                "invalid image digest `{digest}`: expected `algorithm:hex`, such as `sha256:` followed by 64 hexadecimal characters"
            ),
        }
    }
}

impl std::error::Error for ImageReferenceError {}

impl ImageReference {
    /// Parses a reference of the form `[registry[:port]/]repository[:tag][@digest]`.
    ///
    /// # Errors
    ///
    /// Returns an [`ImageReferenceError`] when the reference does not follow
    /// the grammar.
    pub fn parse(reference: &str) -> Result<Self, ImageReferenceError> {
        let trimmed = reference.trim();
        if trimmed.is_empty() {
            return Err(ImageReferenceError::Empty);
        }

        // The digest is taken first: it is introduced by the only unambiguous
        // separator in the grammar, and removing it leaves a name that no
        // longer contains a digest colon.
        let (name_and_tag, digest) = match trimmed.split_once('@') {
            Some((name_and_tag, digest)) => {
                validate_digest(digest)?;
                (name_and_tag, Some(digest.to_owned()))
            }
            None => (trimmed, None),
        };

        // A colon introduces a tag only when nothing after it is a path
        // separator. Otherwise it is a registry port, and the repository path
        // continues past it.
        let (name, tag) = match name_and_tag.rsplit_once(':') {
            Some((name, candidate)) if !candidate.contains('/') => {
                validate_tag(candidate)?;
                (name, Some(candidate.to_owned()))
            }
            _ => (name_and_tag, None),
        };

        // Only now can the leading component be classified: a registry is
        // told apart from a repository namespace by carrying a dot, a port,
        // or by being `localhost`.
        let (registry, repository) = match name.split_once('/') {
            Some((candidate, rest)) if is_registry_host(candidate) => {
                validate_registry(candidate)?;
                (Some(candidate.to_owned()), rest)
            }
            _ => (None, name),
        };

        validate_repository(repository).map_err(|()| ImageReferenceError::InvalidRepository {
            reference: reference.to_owned(),
        })?;

        Ok(Self {
            registry,
            repository: repository.to_owned(),
            tag,
            digest,
        })
    }

    /// Returns the registry host and optional port, when one is present.
    #[must_use]
    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }

    /// Returns the repository path, without registry, tag or digest.
    #[must_use]
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Returns the tag, when the reference carries one.
    #[must_use]
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    /// Returns the digest, when the reference is digest pinned.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    /// Returns the repository qualified with its registry, as expected by the
    /// Docker pull API and by the Helm `image.repository` value.
    #[must_use]
    pub fn qualified_repository(&self) -> String {
        match &self.registry {
            Some(registry) => format!("{registry}/{}", self.repository),
            None => self.repository.clone(),
        }
    }
}

/// Tells a registry host apart from the first component of a repository path.
///
/// A component qualifies as a registry when it carries a dot or a port, or
/// when it is `localhost`, which is the one host name conventionally allowed
/// to stand alone.
fn is_registry_host(candidate: &str) -> bool {
    candidate.contains('.') || candidate.contains(':') || candidate == "localhost"
}

fn validate_registry(registry: &str) -> Result<(), ImageReferenceError> {
    let invalid = || ImageReferenceError::InvalidRegistry {
        registry: registry.to_owned(),
    };

    let (host, port) = match registry.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (registry, None),
    };

    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
    }

    if host.is_empty() {
        return Err(invalid());
    }
    for label in host.split('.') {
        let is_valid_label = !label.is_empty()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-');
        if !is_valid_label {
            return Err(invalid());
        }
    }

    Ok(())
}

/// Validates a repository path against the OCI distribution grammar.
///
/// Returns the unit error, so the caller can report the whole reference rather
/// than the extracted fragment: a repository is only meaningful in context.
fn validate_repository(repository: &str) -> Result<(), ()> {
    if repository.is_empty() {
        return Err(());
    }
    for component in repository.split('/') {
        if !is_valid_path_component(component) {
            return Err(());
        }
    }
    Ok(())
}

/// A path component is `[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*`.
///
/// Uppercase is rejected rather than folded: a container runtime refuses it,
/// and folding it here would make two distinct manifest entries resolve to the
/// same image.
fn is_valid_path_component(component: &str) -> bool {
    let is_alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();

    let bytes = component.as_bytes();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    if !is_alphanumeric(first) || !is_alphanumeric(last) {
        return false;
    }

    // A separator run is a single dot, one or two underscores, or a run of
    // hyphens. Any other mixture, including a mix of separator kinds, is
    // outside the grammar.
    let mut run_separator = None;
    let mut run_length = 0_usize;
    for &byte in bytes {
        if is_alphanumeric(byte) {
            run_separator = None;
            run_length = 0;
            continue;
        }
        if !matches!(byte, b'.' | b'_' | b'-') {
            return false;
        }
        if run_separator.is_some_and(|separator| separator != byte) {
            return false;
        }
        run_separator = Some(byte);
        run_length += 1;
        let is_run_allowed = match byte {
            b'.' => run_length == 1,
            b'_' => run_length <= 2,
            _ => true,
        };
        if !is_run_allowed {
            return false;
        }
    }

    true
}

fn validate_tag(tag: &str) -> Result<(), ImageReferenceError> {
    let invalid = || ImageReferenceError::InvalidTag {
        tag: tag.to_owned(),
    };

    if tag.is_empty() || tag.len() > MAX_TAG_LENGTH {
        return Err(invalid());
    }
    let Some(first) = tag.bytes().next() else {
        return Err(invalid());
    };
    if !first.is_ascii_alphanumeric() && first != b'_' {
        return Err(invalid());
    }
    if !tag
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
    {
        return Err(invalid());
    }

    Ok(())
}

/// Validates a digest as `algorithm:payload`.
///
/// The payload length floor matters: a truncated digest looks like a pin and
/// is not one, so accepting it would defeat the reason to pin at all.
fn validate_digest(digest: &str) -> Result<(), ImageReferenceError> {
    let invalid = || ImageReferenceError::InvalidDigest {
        digest: digest.to_owned(),
    };

    let Some((algorithm, payload)) = digest.split_once(':') else {
        return Err(invalid());
    };

    if algorithm.is_empty() {
        return Err(invalid());
    }
    for component in algorithm.split(['.', '+', '_', '-']) {
        let is_valid_component = !component.is_empty()
            && component
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        if !is_valid_component {
            return Err(invalid());
        }
    }

    if payload.len() < MIN_DIGEST_PAYLOAD_LENGTH
        || !payload.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid());
    }

    Ok(())
}
