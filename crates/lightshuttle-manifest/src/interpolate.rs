//! Substitution engine for `${...}` interpolations in manifest string
//! values.

use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

use indexmap::IndexMap;

use crate::error::{ManifestError, Result};

/// Runtime context for interpolation: environment variables and resolved
/// resource properties.
#[derive(Debug, Default, Clone)]
pub struct InterpolationContext {
    env: HashMap<String, String>,
    resources: HashMap<String, IndexMap<String, String>>,
}

impl InterpolationContext {
    /// Create an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context preloaded with the host process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            env: std::env::vars().collect(),
            resources: HashMap::new(),
        }
    }

    /// Add or override environment variables.
    #[must_use]
    pub fn with_env<I>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        self.env.extend(vars);
        self
    }

    /// Add or replace the properties exposed by a resource.
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

/// Parsed form of a `${...}` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// `${resources.<name>.<property>}`.
    Resource {
        /// Target resource name.
        name: String,
        /// Property of the target resource.
        property: String,
    },

    /// `${env.<NAME>}` or `${env.<NAME>:-<default>}`.
    Env {
        /// Environment variable name.
        name: String,
        /// Optional fallback when the variable is unset or empty.
        default: Option<String>,
    },
}

/// Interpolation engine over a [`InterpolationContext`].
pub struct Interpolator<'ctx> {
    ctx: &'ctx InterpolationContext,
}

impl<'ctx> Interpolator<'ctx> {
    /// Build a new interpolator bound to `ctx`.
    #[must_use]
    pub fn new(ctx: &'ctx InterpolationContext) -> Self {
        Self { ctx }
    }

    /// Resolve every `${...}` occurrence in `input`.
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

    /// Scan `input` and return the references it contains, without
    /// resolving them.
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
