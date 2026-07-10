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
use std::iter::Peekable;
use std::str::Chars;

use indexmap::IndexMap;

use crate::error::{ManifestError, Result};

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
        let mut output = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '$' {
                output.push(c);
                continue;
            }

            if chars.peek() != Some(&'{') {
                output.push('$');
                continue;
            }
            chars.next();

            // Escape form `${{ ... }}`
            if chars.peek() == Some(&'{') {
                chars.next();
                let body = consume_until_double_close(&mut chars, input)?;
                output.push('$');
                output.push('{');
                output.push_str(&body);
                output.push('}');
                continue;
            }

            let body = consume_until_close(&mut chars, input)?;
            let reference = parse_reference(&body)?;
            let resolved = self.lookup(&reference)?;
            output.push_str(&resolved);
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
        let mut refs = Vec::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '$' || chars.peek() != Some(&'{') {
                continue;
            }
            chars.next();

            if chars.peek() == Some(&'{') {
                chars.next();
                consume_until_double_close(&mut chars, input)?;
                continue;
            }

            let body = consume_until_close(&mut chars, input)?;
            refs.push(parse_reference(&body)?);
        }

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

fn consume_until_close(chars: &mut Peekable<Chars<'_>>, full: &str) -> Result<String> {
    let mut body = String::new();
    for c in chars.by_ref() {
        if c == '}' {
            return Ok(body);
        }
        body.push(c);
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
        Ok(Reference::Resource {
            name: name.to_owned(),
            property: property.to_owned(),
        })
    } else if let Some(rest) = body.strip_prefix("env.") {
        if let Some((name, default)) = rest.split_once(":-") {
            Ok(Reference::Env {
                name: name.to_owned(),
                default: Some(default.to_owned()),
            })
        } else {
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
