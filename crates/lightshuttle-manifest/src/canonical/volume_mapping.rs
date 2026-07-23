//! The single volume mapping grammar of the project.
//!
//! A mapping is `source:target`, and the colon that separates them is not
//! always the first one. `C:\project\data:/data` carries a drive colon that
//! belongs to the source, so splitting on the first colon produced a named
//! volume called `C` and a target of `\project\data:/data`. The mapping then
//! survived every later check, because both halves are non empty strings.
//!
//! Splitting on the first colon has a second consequence. The grammar defines
//! exactly two fields, so `./data:/app:ro` is not a mapping this project
//! understands, yet it was accepted and its trailing field folded into the
//! target. A container then mounted at `/app:ro`, a path the manifest never
//! named. Mount options may be supported one day; guessing at them is a
//! different thing, and this module refuses instead.
//!
//! Drive letters are recognised on every platform rather than behind a
//! `cfg!(windows)`. A named volume cannot contain a backslash, so there is no
//! ambiguity to arbitrate, and an export run on a Linux runner for a Windows
//! host must read the manifest exactly as the Windows workstation does.

use std::fmt;

/// Where the host side of a mapping comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingSource {
    /// A path on the host, relative or absolute, including drive qualified.
    HostPath(String),
    /// A volume managed by the container runtime, named in the manifest.
    Named(String),
}

/// A parsed `source:target` volume mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeMapping {
    source: MappingSource,
    target: String,
}

/// Reasons a volume mapping cannot be parsed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeMappingError {
    /// The value is empty or blank.
    Empty,
    /// No separator was found, so there is no target.
    MissingTarget {
        /// The offending mapping, as supplied.
        mapping: String,
    },
    /// Either side of the separator is empty.
    EmptyComponent {
        /// The offending mapping, as supplied.
        mapping: String,
    },
    /// The mapping carries a field beyond the source and the target.
    UnsupportedField {
        /// The offending mapping, as supplied.
        mapping: String,
        /// The trailing field this grammar does not define.
        field: String,
    },
    /// The named volume carries a character that is unsafe downstream.
    InvalidName {
        /// The offending name, as supplied.
        name: String,
    },
}

impl fmt::Display for VolumeMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("volume mapping is empty"),
            Self::MissingTarget { mapping } => write!(
                formatter,
                "invalid volume mapping `{mapping}`: expected `source:target`, such as `./data:/app` or `cache:/var/cache`"
            ),
            Self::EmptyComponent { mapping } => write!(
                formatter,
                "invalid volume mapping `{mapping}`: the source and the target must both be present"
            ),
            Self::UnsupportedField { mapping, field } => write!(
                formatter,
                "invalid volume mapping `{mapping}`: `{field}` is a third field, and a mapping carries only a source and a target. Mount options are not supported, and folding `{field}` into the target would mount at a path you did not name."
            ),
            Self::InvalidName { name } => write!(
                formatter,
                "volume name `{name}` must not contain '{{' or '}}': unsafe in export templates"
            ),
        }
    }
}

impl std::error::Error for VolumeMappingError {}

impl VolumeMapping {
    /// Parses a manifest volume entry such as `./data:/app`, `cache:/var/cache`
    /// or `C:\project\data:/data`.
    ///
    /// # Errors
    ///
    /// Returns a [`VolumeMappingError`] when the value is empty, carries no
    /// target, carries a field this grammar does not define, or names a volume
    /// with an unsafe character.
    pub fn parse(mapping: &str) -> Result<Self, VolumeMappingError> {
        let trimmed = mapping.trim();
        if trimmed.is_empty() {
            return Err(VolumeMappingError::Empty);
        }

        let separator =
            separator_index(trimmed).ok_or_else(|| VolumeMappingError::MissingTarget {
                mapping: mapping.to_owned(),
            })?;
        let (source, target) = trimmed.split_at(separator);
        let target = &target[1..];

        // A colon left in the target is a field this grammar does not define.
        // Absorbing it silently is the defect this parser exists to remove.
        if let Some((_, field)) = target.split_once(':') {
            return Err(VolumeMappingError::UnsupportedField {
                mapping: mapping.to_owned(),
                field: field.to_owned(),
            });
        }

        if source.is_empty() || target.is_empty() {
            return Err(VolumeMappingError::EmptyComponent {
                mapping: mapping.to_owned(),
            });
        }

        Ok(Self {
            source: classify_source(source)?,
            target: target.to_owned(),
        })
    }

    /// The host side of the mapping.
    #[must_use]
    pub fn source(&self) -> &MappingSource {
        &self.source
    }

    /// The container path the source is mounted at.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }
}

/// Reports whether a value starts with a drive qualified path such as `C:\` or
/// `C:/`, whose colon belongs to the source rather than to the separator.
#[must_use]
pub fn is_drive_qualified(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// Reports whether a drive qualified value is unambiguously a path.
///
/// A backslash cannot appear in a container volume name, so `C:\data` has one
/// reading. `C:/data` has two, since `C` is itself a legal volume name.
fn is_certainly_a_drive_path(value: &str) -> bool {
    is_drive_qualified(value) && value.as_bytes()[2] == b'\\'
}

/// The index of the colon that separates the source from the target.
///
/// A drive colon is never that separator, so the search starts past it. When
/// the drive reading is ambiguous and leaves no separator at all, the value is
/// read as a plain `name:target` instead: a mapping carries two fields, so the
/// only complete reading is the intended one.
fn separator_index(mapping: &str) -> Option<usize> {
    const AFTER_DRIVE_COLON: usize = 2;

    if is_certainly_a_drive_path(mapping) {
        return mapping[AFTER_DRIVE_COLON..]
            .find(':')
            .map(|index| index + AFTER_DRIVE_COLON);
    }
    if is_drive_qualified(mapping)
        && let Some(index) = mapping[AFTER_DRIVE_COLON..].find(':')
    {
        return Some(index + AFTER_DRIVE_COLON);
    }
    mapping.find(':')
}

/// Decides whether a source names a host path or a managed volume.
fn classify_source(source: &str) -> Result<MappingSource, VolumeMappingError> {
    if source.starts_with('.') || source.starts_with('/') || is_drive_qualified(source) {
        return Ok(MappingSource::HostPath(source.to_owned()));
    }
    // Volume names are interpolated into the generated Helm and Compose
    // documents, where a brace would open a template expression.
    if source.contains(['{', '}']) {
        return Err(VolumeMappingError::InvalidName {
            name: source.to_owned(),
        });
    }
    Ok(MappingSource::Named(source.to_owned()))
}
