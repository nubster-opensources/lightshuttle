//! Substitution engine for `${...}` interpolations in manifest string values.
//!
//! Two reference schemes are supported:
//!
//! - `${env.NAME}`: substituted with the value of the environment variable
//!   `NAME`. The form `${env.NAME:-default}` uses `default` when `NAME` is
//!   unset or empty.
//! - `${resources.name.property}`: substituted with a runtime property of
//!   the named resource (e.g. `host`, `port`, `password`). Properties are
//!   injected by the runtime layer, not by this crate.
//!
//! The escape form `${{ ... }}` emits a literal `${ ... }` without
//! triggering substitution.
//!
//! # Usage
//!
//! Build an [`InterpolationContext`] with the available values, then create
//! an [`Interpolator`] to resolve or scan individual strings.
//!
//! ```rust
//! use lightshuttle_manifest::interpolate::{InterpolationContext, Interpolator};
//!
//! let ctx = InterpolationContext::new()
//!     .with_env([("PORT".to_string(), "8080".to_string())]);
//! let interpolator = Interpolator::new(&ctx);
//! let result = interpolator.resolve("http://localhost:${env.PORT}").unwrap();
//! assert_eq!(result, "http://localhost:8080");
//! ```

use std::collections::HashMap;
use std::fmt::Write as _;
use std::iter::Peekable;
use std::str::Chars;

use indexmap::IndexMap;

use crate::error::{ManifestError, Result};

/// Maximum nesting depth accepted for `${...}` interpolations. A top-level
/// reference is depth 1; a reference inside an `env` default is depth 2; and
/// so on. Opening a reference beyond this depth raises
/// [`ManifestError::InterpolationTooDeep`].
pub const MAX_INTERPOLATION_DEPTH: usize = 10;

/// Runtime context that backs an [`Interpolator`].
///
/// Holds the set of environment variables and the runtime-resolved properties
/// of each resource (host, port, password, etc.). The context is immutable
/// once built; the builder methods consume `self` and return a new value.
///
/// # Building a context
///
/// ```rust
/// use lightshuttle_manifest::interpolate::InterpolationContext;
/// use indexmap::IndexMap;
///
/// let mut props = IndexMap::new();
/// props.insert("host".to_string(), "127.0.0.1".to_string());
/// props.insert("port".to_string(), "5432".to_string());
///
/// let ctx = InterpolationContext::new()
///     .with_env([("DB_NAME".to_string(), "mydb".to_string())])
///     .with_resource("db", props);
/// ```
#[derive(Debug, Default, Clone)]
pub struct InterpolationContext {
    env: HashMap<String, String>,
    resources: HashMap<String, IndexMap<String, String>>,
}

impl InterpolationContext {
    /// Create an empty context with no environment variables and no resources.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context pre-populated with the current process environment.
    ///
    /// Equivalent to calling `new()` followed by
    /// `with_env(std::env::vars())`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            env: std::env::vars().collect(),
            resources: HashMap::new(),
        }
    }

    /// Add or override a batch of environment variables.
    ///
    /// Later calls to `with_env` for the same key win; the last value set
    /// is the one used during resolution.
    #[must_use]
    pub fn with_env<I>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        self.env.extend(vars);
        self
    }

    /// Register (or replace) the runtime-resolved properties for a named
    /// resource.
    ///
    /// `name` must match the resource key as declared in the manifest.
    /// The `properties` map is keyed by property name (e.g. `"host"`,
    /// `"port"`, `"password"`).
    #[must_use]
    pub fn with_resource(
        mut self,
        name: impl Into<String>,
        properties: IndexMap<String, String>,
    ) -> Self {
        self.resources.insert(name.into(), properties);
        self
    }
}

/// Parsed form of a `${...}` interpolation reference.
///
/// Produced by the internal parser and used by [`Interpolator::resolve`] and
/// [`Interpolator::scan`]. Consumers of the crate can inspect the scanned
/// references to build static dependency maps without performing actual
/// value resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A `${resources.<name>.<property>}` reference.
    ///
    /// Resolved against the properties registered for the named resource in
    /// the [`InterpolationContext`].
    Resource {
        /// Name of the target resource as declared in the manifest.
        name: String,
        /// Property key on that resource (e.g. `"host"`, `"port"`).
        property: String,
    },

    /// A `${env.<NAME>}` or `${env.<NAME>:-<default>}` reference.
    ///
    /// Resolved against the environment variables in the
    /// [`InterpolationContext`]. When `default` is `Some`, it is used as
    /// a fallback when the variable is unset or empty.
    Env {
        /// Environment variable name.
        name: String,
        /// Optional fallback value used when `name` is unset or empty.
        default: Option<String>,
    },
}

impl Reference {
    /// Returns the target resource name when this is a
    /// [`Reference::Resource`], or `None` for an environment reference.
    ///
    /// Used to derive implicit dependencies: a `${resources.<name>.*}`
    /// interpolation makes the enclosing resource depend on `<name>`.
    #[must_use]
    pub fn resource_name(self) -> Option<String> {
        match self {
            Self::Resource { name, .. } => Some(name),
            Self::Env { .. } => None,
        }
    }
}

/// Interpolation engine bound to an [`InterpolationContext`].
///
/// Create one with [`Interpolator::new`], then call [`Interpolator::resolve`]
/// to substitute references in a string, or [`Interpolator::scan`] to
/// enumerate references without substituting them.
pub struct Interpolator<'ctx> {
    ctx: &'ctx InterpolationContext,
}

impl<'ctx> Interpolator<'ctx> {
    /// Create an interpolator that resolves references against `ctx`.
    #[must_use]
    pub fn new(ctx: &'ctx InterpolationContext) -> Self {
        Self { ctx }
    }

    /// Resolve all `${...}` references in `input` and return the resulting
    /// string.
    ///
    /// Literal braces can be escaped with `${{ ... }}`, which emits
    /// `${ ... }` verbatim. Any unknown scheme or unresolvable reference
    /// returns a [`ManifestError`].
    ///
    /// ```rust
    /// use lightshuttle_manifest::interpolate::{InterpolationContext, Interpolator};
    ///
    /// let ctx = InterpolationContext::new()
    ///     .with_env([("HOST".to_string(), "localhost".to_string())]);
    /// let interpolator = Interpolator::new(&ctx);
    ///
    /// let out = interpolator.resolve("connect to ${env.HOST}").unwrap();
    /// assert_eq!(out, "connect to localhost");
    /// ```
    pub fn resolve(&self, input: &str) -> Result<String> {
        let parsed = segments(input)?;
        self.resolve_segments(&parsed)
    }

    /// Walks parsed segments, substituting each `env` and `resource`
    /// reference and concatenating literals, recursing into an `env`
    /// default's own segments only when the variable is unset or empty.
    fn resolve_segments(&self, segments: &[Segment]) -> Result<String> {
        let mut output = String::new();

        for segment in segments {
            match segment {
                Segment::Literal(text) => output.push_str(text),
                Segment::Resource { name, property } => {
                    let value = self.lookup(&Reference::Resource {
                        name: name.clone(),
                        property: property.clone(),
                    })?;
                    output.push_str(&value);
                }
                Segment::Env { name, default } => {
                    if let Some(value) = self.ctx.env.get(name).filter(|v| !v.is_empty()) {
                        output.push_str(value);
                    } else if let Some(default_segments) = default {
                        output.push_str(&self.resolve_segments(default_segments)?);
                    } else {
                        return Err(ManifestError::EnvUnset(name.clone()));
                    }
                }
            }
        }

        Ok(output)
    }

    /// Scan `input` and return every [`Reference`] it contains without
    /// resolving values.
    ///
    /// Useful for static analysis: the validation pass calls `scan` to
    /// verify that every `${resources.name.property}` expression refers
    /// to a resource that exists in the manifest, before any container
    /// is started.
    ///
    /// Returns a [`ManifestError`] if the interpolation syntax is invalid
    /// (e.g. unterminated `${`).
    pub fn scan(&self, input: &str) -> Result<Vec<Reference>> {
        let parsed = segments(input)?;
        let mut refs = Vec::new();
        collect_references(&parsed, &mut refs);
        Ok(refs)
    }

    fn lookup(&self, reference: &Reference) -> Result<String> {
        match reference {
            Reference::Resource { name, property } => {
                let resource = self
                    .ctx
                    .resources
                    .get(name)
                    .ok_or_else(|| ManifestError::UnknownResource(name.clone()))?;
                let value =
                    resource
                        .get(property)
                        .ok_or_else(|| ManifestError::UnknownProperty {
                            resource: name.clone(),
                            property: property.clone(),
                            kind: "<runtime>",
                        })?;
                Ok(value.clone())
            }
            Reference::Env { name, default } => {
                if let Some(value) = self.ctx.env.get(name).filter(|v| !v.is_empty()) {
                    Ok(value.clone())
                } else if let Some(fallback) = default {
                    Ok(fallback.clone())
                } else {
                    Err(ManifestError::EnvUnset(name.clone()))
                }
            }
        }
    }
}

/// Consume a brace-balanced `${...}` body, the opening `${` already consumed.
///
/// Nested `${` sequences are counted so the outer reference's body is returned
/// intact (e.g. `env.X:-${env.Y}` for `${env.X:-${env.Y}}`). `depth` is the
/// nesting level of the reference being consumed (1 at the top level); opening
/// a nested reference beyond [`MAX_INTERPOLATION_DEPTH`] raises
/// [`ManifestError::InterpolationTooDeep`].
fn consume_balanced_body(
    chars: &mut Peekable<Chars<'_>>,
    full: &str,
    depth: usize,
) -> Result<String> {
    let mut body = String::new();
    let mut nesting = 1usize;

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            nesting += 1;
            if depth + (nesting - 1) > MAX_INTERPOLATION_DEPTH {
                return Err(ManifestError::InterpolationTooDeep {
                    limit: MAX_INTERPOLATION_DEPTH,
                    context: full.to_owned(),
                });
            }
            body.push('$');
            body.push('{');
        } else if c == '}' {
            nesting -= 1;
            if nesting == 0 {
                return Ok(body);
            }
            body.push('}');
        } else {
            body.push(c);
        }
    }

    Err(ManifestError::InvalidInterpolation(format!(
        "unterminated `${{` in `{full}`"
    )))
}

fn consume_until_double_close(chars: &mut Peekable<Chars<'_>>, full: &str) -> Result<String> {
    let mut body = String::new();
    while let Some(c) = chars.next() {
        if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
            return Ok(body);
        }
        body.push(c);
    }
    Err(ManifestError::InvalidInterpolation(format!(
        "unterminated `${{{{` in `{full}`"
    )))
}

/// Flattens parsed segments back into the references they hold, in the
/// order [`segments`] produced them: an `env` reference surfaces before the
/// references nested inside its own default, mirroring how [`segments`]
/// nests a default's segments under the reference that carries it.
fn collect_references(segments: &[Segment], out: &mut Vec<Reference>) {
    for segment in segments {
        match segment {
            Segment::Literal(_) => {}
            Segment::Resource { name, property } => {
                out.push(Reference::Resource {
                    name: name.clone(),
                    property: property.clone(),
                });
            }
            Segment::Env { name, default } => {
                out.push(Reference::Env {
                    name: name.clone(),
                    default: default.as_ref().map(|segments| render_segments(segments)),
                });
                if let Some(default_segments) = default {
                    collect_references(default_segments, out);
                }
            }
        }
    }
}

/// Renders parsed segments back to the interpolation text they were parsed
/// from. Used only to fill [`Reference::Env::default`] for
/// [`Interpolator::scan`]; [`Interpolator::resolve`] never needs it, since
/// it substitutes values instead of reconstructing source text.
fn render_segments(segments: &[Segment]) -> String {
    let mut out = String::new();

    for segment in segments {
        match segment {
            Segment::Literal(text) => out.push_str(text),
            Segment::Resource { name, property } => {
                let _ = write!(out, "${{resources.{name}.{property}}}");
            }
            Segment::Env {
                name,
                default: None,
            } => {
                let _ = write!(out, "${{env.{name}}}");
            }
            Segment::Env {
                name,
                default: Some(default_segments),
            } => {
                let rendered = render_segments(default_segments);
                let _ = write!(out, "${{env.{name}:-{rendered}}}");
            }
        }
    }

    out
}

/// One piece of an interpolatable string: literal text or a reference.
///
/// Produced by [`segments`], which splits a raw manifest string into its
/// literal and reference parts without resolving any value. This is the
/// single decomposition that [`Interpolator::resolve`] and
/// [`Interpolator::scan`] are rebuilt on top of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Literal text, with `${{ ... }}` escapes already unfolded to `${ ... }`.
    Literal(String),
    /// A `${resources.<name>.<property>}` reference.
    Resource {
        /// Name of the target resource as declared in the manifest.
        name: String,
        /// Property key on that resource (e.g. `"host"`, `"port"`).
        property: String,
    },
    /// A `${env.<NAME>}` reference, with its default split into segments.
    Env {
        /// Environment variable name.
        name: String,
        /// Optional fallback, itself split into segments so that a
        /// reference nested in the default surfaces as its own
        /// [`Segment::Env`] or [`Segment::Resource`].
        default: Option<Vec<Segment>>,
    },
}

/// Splits `input` into literal text and references without resolving them.
///
/// Uses the exact grammar of [`Interpolator::resolve`] and
/// [`Interpolator::scan`]: the `${{ ... }}` escape, a lone `$` kept as a
/// literal character, and `env` defaults parsed recursively so that a
/// reference nested in a default surfaces as its own segment.
///
/// # Errors
///
/// Returns a [`ManifestError`] on the same malformed input that
/// [`Interpolator::resolve`] rejects: an unterminated `${`, an unknown
/// reference scheme, a `${` nested outside an `env` default, or an
/// interpolation nested deeper than [`MAX_INTERPOLATION_DEPTH`].
pub fn segments(input: &str) -> Result<Vec<Segment>> {
    segments_at(input, 1)
}

/// The single reader of the `${...}` grammar: walks `input` once, character
/// by character, emitting literal text and references as it goes. `depth`
/// is the nesting level of `input` itself (1 for a top-level manifest
/// string, 2 for the text of an `env` default one level in, and so on);
/// [`consume_balanced_body`] checks it against [`MAX_INTERPOLATION_DEPTH`].
///
/// An `env` default is not resolved here: its raw text is handed back to
/// this same function, one level deeper, so the whole default becomes its
/// own sequence of segments. `Interpolator::resolve` and
/// `Interpolator::scan` never re-read `${...}` themselves; they only walk
/// the [`Segment`] tree this function returns.
fn segments_at(input: &str, depth: usize) -> Result<Vec<Segment>> {
    let mut out = Vec::new();
    let mut literal = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '$' {
            literal.push(c);
            continue;
        }

        if chars.peek() != Some(&'{') {
            literal.push('$');
            continue;
        }
        chars.next();

        // Escape form `${{ ... }}`: unfolds to a literal `${ ... }`, merged
        // into the surrounding literal text.
        if chars.peek() == Some(&'{') {
            chars.next();
            let body = consume_until_double_close(&mut chars, input)?;
            literal.push('$');
            literal.push('{');
            literal.push_str(&body);
            literal.push('}');
            continue;
        }

        let body = consume_balanced_body(&mut chars, input, depth)?;
        let reference = parse_reference(&body)?;

        if !literal.is_empty() {
            out.push(Segment::Literal(std::mem::take(&mut literal)));
        }

        out.push(match reference {
            Reference::Resource { name, property } => Segment::Resource { name, property },
            Reference::Env {
                name,
                default: None,
            } => Segment::Env {
                name,
                default: None,
            },
            Reference::Env {
                name,
                default: Some(raw_default),
            } => {
                let default_segments = segments_at(&raw_default, depth + 1)?;
                Segment::Env {
                    name,
                    default: Some(default_segments),
                }
            }
        });
    }

    if !literal.is_empty() {
        out.push(Segment::Literal(literal));
    }

    Ok(out)
}

fn parse_reference(body: &str) -> Result<Reference> {
    if let Some(rest) = body.strip_prefix("resources.") {
        let (name, property) = rest.split_once('.').ok_or_else(|| {
            ManifestError::InvalidInterpolation(format!(
                "resource reference missing property in `${{{body}}}`"
            ))
        })?;
        if name.is_empty() || property.is_empty() {
            return Err(ManifestError::InvalidInterpolation(format!(
                "empty resource reference in `${{{body}}}`"
            )));
        }
        if name.contains("${") || property.contains("${") {
            return Err(ManifestError::InvalidInterpolation(format!(
                "nested interpolation is only allowed in an env default, not in `${{{body}}}`"
            )));
        }
        Ok(Reference::Resource {
            name: name.to_owned(),
            property: property.to_owned(),
        })
    } else if let Some(rest) = body.strip_prefix("env.") {
        if let Some((name, default)) = rest.split_once(":-") {
            if name.contains("${") {
                return Err(ManifestError::InvalidInterpolation(format!(
                    "nested interpolation is only allowed in an env default, not in the variable name of `${{{body}}}`"
                )));
            }
            Ok(Reference::Env {
                name: name.to_owned(),
                default: Some(default.to_owned()),
            })
        } else {
            if rest.contains("${") {
                return Err(ManifestError::InvalidInterpolation(format!(
                    "nested interpolation is only allowed in an env default, not in `${{{body}}}`"
                )));
            }
            Ok(Reference::Env {
                name: rest.to_owned(),
                default: None,
            })
        }
    } else {
        Err(ManifestError::InvalidInterpolation(format!(
            "unknown reference scheme in `${{{body}}}`"
        )))
    }
}
