//! Semantic validation: naming rules, dependency graph, references.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::error::{ManifestError, Result};
use crate::interpolate::{InterpolationContext, Interpolator, Reference};
use crate::model::{Healthcheck, Manifest, ResourceKind};

const NAME_PATTERN: &str = "^[a-z][a-z0-9_-]{0,31}$";
const DATABASE_PATTERN: &str = "^[a-z][a-z0-9_]*$";
const NAME_MAX_LEN: usize = 32;

impl Manifest {
    /// Run structural and semantic validation on the parsed manifest.
    ///
    /// This is invoked automatically by [`Manifest::parse`] but can be
    /// called manually after the model has been built programmatically.
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.project.name)?;

        for (name, kind) in &self.resources {
            validate_name(name)?;
            validate_resource_kind(name, kind)?;
        }

        validate_dependency_graph(&self.resources)?;
        validate_references(self)?;
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if matches_name_pattern(name) {
        Ok(())
    } else {
        Err(ManifestError::InvalidName {
            name: name.to_owned(),
            pattern: NAME_PATTERN,
        })
    }
}

fn matches_name_pattern(name: &str) -> bool {
    if name.is_empty() || name.len() > NAME_MAX_LEN {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or(' ');
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn matches_database_pattern(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or(' ');
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn validate_resource_kind(name: &str, kind: &ResourceKind) -> Result<()> {
    match kind {
        ResourceKind::Postgres(c) => {
            if let Some(db) = c.database.as_deref()
                && !matches_database_pattern(db)
            {
                return Err(ManifestError::InvalidName {
                    name: db.to_owned(),
                    pattern: DATABASE_PATTERN,
                });
            }
        }
        ResourceKind::Container(c) => {
            if c.image.trim().is_empty() {
                return Err(ManifestError::MissingField {
                    resource: name.to_owned(),
                    field: "image",
                });
            }
        }
        ResourceKind::Dockerfile(c) => {
            if c.context.trim().is_empty() {
                return Err(ManifestError::MissingField {
                    resource: name.to_owned(),
                    field: "context",
                });
            }
        }
        ResourceKind::Redis(_) => {}
    }

    if let Some(hc) = kind.healthcheck() {
        validate_healthcheck(hc)?;
    }

    Ok(())
}

fn validate_healthcheck(hc: &Healthcheck) -> Result<()> {
    if hc.test.is_empty() {
        return Err(ManifestError::InvalidInterpolation(
            "healthcheck.test cannot be empty".to_owned(),
        ));
    }
    parse_duration(&hc.interval)?;
    parse_duration(&hc.timeout)?;
    parse_duration(&hc.start_period)?;
    Ok(())
}

fn parse_duration(input: &str) -> Result<()> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ManifestError::InvalidDuration(input.to_owned()));
    }
    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'.') {
        idx += 1;
    }
    if idx == 0 {
        return Err(ManifestError::InvalidDuration(input.to_owned()));
    }
    let unit = &trimmed[idx..];
    if !matches!(unit, "ns" | "us" | "ms" | "s" | "m" | "h") {
        return Err(ManifestError::InvalidDuration(input.to_owned()));
    }
    Ok(())
}

fn validate_dependency_graph(resources: &IndexMap<String, ResourceKind>) -> Result<()> {
    let graph: HashMap<&str, Vec<&str>> = resources
        .iter()
        .map(|(name, kind)| {
            let deps: Vec<&str> = kind.depends_on().iter().map(String::as_str).collect();
            (name.as_str(), deps)
        })
        .collect();

    for (resource, deps) in &graph {
        for dep in deps {
            if !graph.contains_key(*dep) {
                return Err(ManifestError::UnknownResource(format!(
                    "`{dep}` (depended on by `{resource}`)"
                )));
            }
        }
    }

    let mut colors: HashMap<&str, Color> = graph.keys().map(|n| (*n, Color::White)).collect();
    let nodes: Vec<&str> = graph.keys().copied().collect();
    for node in nodes {
        let mut stack: Vec<&str> = Vec::new();
        visit(node, &graph, &mut colors, &mut stack)?;
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum Color {
    White,
    Gray,
    Black,
}

fn visit<'a>(
    node: &'a str,
    graph: &HashMap<&'a str, Vec<&'a str>>,
    colors: &mut HashMap<&'a str, Color>,
    stack: &mut Vec<&'a str>,
) -> Result<()> {
    match colors.get(node) {
        Some(Color::Black) => return Ok(()),
        Some(Color::Gray) => {
            let start = stack.iter().position(|n| *n == node).unwrap_or(0);
            let mut cycle: Vec<&str> = stack[start..].to_vec();
            cycle.push(node);
            return Err(ManifestError::Cycle(cycle.join(" -> ")));
        }
        _ => {}
    }
    colors.insert(node, Color::Gray);
    stack.push(node);

    let deps: Vec<&str> = graph.get(node).cloned().unwrap_or_default();
    for dep in deps {
        visit(dep, graph, colors, stack)?;
    }

    stack.pop();
    colors.insert(node, Color::Black);
    Ok(())
}

fn validate_references(manifest: &Manifest) -> Result<()> {
    let ctx = InterpolationContext::new();
    let interpolator = Interpolator::new(&ctx);
    let known_resources: HashSet<&str> = manifest.resources.keys().map(String::as_str).collect();

    for (name, kind) in &manifest.resources {
        for value in collect_strings(kind) {
            for reference in interpolator.scan(&value)? {
                if let Reference::Resource { name: target, .. } = reference
                    && !known_resources.contains(target.as_str())
                {
                    return Err(ManifestError::UnknownResource(format!(
                        "`{target}` (referenced from `{name}`)"
                    )));
                }
            }
        }
    }

    Ok(())
}

fn collect_strings(kind: &ResourceKind) -> Vec<String> {
    let mut out = Vec::new();
    match kind {
        ResourceKind::Container(c) => {
            out.push(c.image.clone());
            out.extend(c.env.values().cloned());
            out.extend(c.volumes.iter().cloned());
            if let Some(w) = &c.working_dir {
                out.push(w.clone());
            }
        }
        ResourceKind::Dockerfile(c) => {
            out.push(c.context.clone());
            out.push(c.dockerfile.clone());
            out.extend(c.env.values().cloned());
            out.extend(c.volumes.iter().cloned());
            out.extend(c.build_args.values().cloned());
            if let Some(t) = &c.target {
                out.push(t.clone());
            }
            if let Some(w) = &c.working_dir {
                out.push(w.clone());
            }
        }
        ResourceKind::Postgres(c) => {
            if let Some(s) = &c.password {
                out.push(s.clone());
            }
            if let Some(s) = &c.database {
                out.push(s.clone());
            }
            if let Some(s) = &c.user {
                out.push(s.clone());
            }
        }
        ResourceKind::Redis(c) => {
            if let Some(s) = &c.password {
                out.push(s.clone());
            }
        }
    }
    out
}
