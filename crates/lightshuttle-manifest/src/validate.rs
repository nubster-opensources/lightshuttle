//! Semantic validation: naming rules, dependency graph, references.
//!
//! This module provides the [`Manifest::validate`] method. All validation
//! passes are private functions called from that single entry point. The
//! passes run in dependency order:
//!
//! 1. Name pattern check for the project name and every resource name.
//! 2. Kind-specific checks (required fields, database name pattern,
//!    healthcheck duration syntax).
//! 3. Dependency graph check (unknown references, cycle detection via
//!    depth-first search with three-colour marking).
//! 4. Interpolation reference check (unknown `${resources.name.property}`
//!    targets).
//! 5. Dashboard port check (rejects port `0`).
//! 6. Export target check (resource keys in export sub-tables must exist
//!    in the manifest).

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::error::{ManifestError, Result};
use crate::interpolate::{InterpolationContext, Interpolator, Reference};
use crate::model::{Command, Healthcheck, Manifest, ResourceKind};

const NAME_PATTERN: &str = "^[a-z][a-z0-9_-]{0,31}$";
const DATABASE_PATTERN: &str = "^[a-z][a-z0-9_]{0,62}$";
const NAME_MAX_LEN: usize = 32;
/// PostgreSQL truncates identifiers at 63 bytes, so a longer database
/// name would be silently shortened at runtime.
const DATABASE_MAX_LEN: usize = 63;

impl Manifest {
    /// Run semantic validation on the manifest.
    ///
    /// Called automatically by [`Manifest::parse`] after structural
    /// decoding. Can also be called manually when a [`Manifest`] is built
    /// programmatically rather than parsed from YAML.
    ///
    /// The following checks are performed in order:
    ///
    /// - Project name and every resource name match `^[a-z][a-z0-9_-]{0,31}$`.
    /// - Kind-specific constraints: non-empty `image` for `container`,
    ///   non-empty `context` for `dockerfile`, valid `database` pattern for
    ///   `postgres`, syntactically valid healthcheck durations for all kinds.
    /// - No cycles in the `depends_on` graph.
    /// - All `${resources.name.*}` interpolations reference existing resources.
    /// - `dashboard.port` is not `0`.
    /// - All resource keys inside `export.*.resources` exist in the manifest.
    ///
    /// Returns the first [`ManifestError`] encountered, or `Ok(())`.
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.project.name)?;

        for (name, kind) in &self.resources {
            validate_name(name)?;
            validate_resource_kind(name, kind)?;
        }

        validate_dependency_graph(&self.resources)?;
        validate_references(self)?;
        validate_dashboard(self)?;
        validate_export_targets(self)?;
        Ok(())
    }
}

fn validate_export_targets(manifest: &Manifest) -> Result<()> {
    let Some(export) = &manifest.export else {
        return Ok(());
    };
    let known: HashSet<&str> = manifest.resources.keys().map(String::as_str).collect();

    let mut overrides: Vec<(&str, &String)> = Vec::new();
    if let Some(compose) = &export.compose {
        overrides.extend(compose.resources.keys().map(|n| ("export.compose", n)));
    }
    if let Some(kubernetes) = &export.kubernetes {
        overrides.extend(
            kubernetes
                .resources
                .keys()
                .map(|n| ("export.kubernetes", n)),
        );
    }
    if let Some(helm) = &export.helm {
        overrides.extend(helm.resources.keys().map(|n| ("export.helm", n)));
    }

    for (target, name) in overrides {
        if !known.contains(name.as_str()) {
            return Err(ManifestError::UnknownResource(format!(
                "`{name}` (referenced from `{target}.resources`)"
            )));
        }
    }

    Ok(())
}

fn validate_dashboard(manifest: &Manifest) -> Result<()> {
    if let Some(dashboard) = &manifest.dashboard
        && let Some(port) = dashboard.port
        && port == 0
    {
        return Err(ManifestError::InvalidDashboardPort { port });
    }
    Ok(())
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
    if name.is_empty() || name.len() > DATABASE_MAX_LEN {
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
            validate_entrypoint(name, c.entrypoint.as_ref())?;
            validate_secret_keys(name, &c.env, &c.secrets)?;
        }
        ResourceKind::Dockerfile(c) => {
            if c.context.trim().is_empty() {
                return Err(ManifestError::MissingField {
                    resource: name.to_owned(),
                    field: "context",
                });
            }
            validate_entrypoint(name, c.entrypoint.as_ref())?;
            validate_secret_keys(name, &c.env, &c.secrets)?;
        }
        ResourceKind::Redis(_) => {}
    }

    if let Some(hc) = kind.healthcheck() {
        validate_healthcheck(hc)?;
    }

    Ok(())
}

fn validate_secret_keys(
    resource: &str,
    env: &indexmap::IndexMap<String, String>,
    secrets: &indexmap::IndexMap<String, String>,
) -> Result<()> {
    if let Some(key) = secrets.keys().find(|key| env.contains_key(*key)) {
        return Err(ManifestError::DuplicateEnvironmentKey {
            resource: resource.to_owned(),
            key: key.clone(),
        });
    }
    Ok(())
}

/// Rejects `entrypoint: []` and an empty (or whitespace-only) `entrypoint: ""`.
///
/// An empty list form, spelled `entrypoint: []` in Compose, is refused at
/// the door: it resolves to a container with no command at all. A blank
/// string form is rejected under the same error: it resolves to
/// `["sh", "-c", ""]`, a container that exits instantly. The Docker Engine
/// API's own reset spelling, `entrypoint: [""]`, is a one-element list, not
/// an empty one, so it is not caught here and passes through unchanged.
/// Whether that spelling should also be refused is out of scope for v0
/// (locked design decision, left unarbitrated on purpose).
fn validate_entrypoint(name: &str, entrypoint: Option<&Command>) -> Result<()> {
    let is_empty = match entrypoint {
        Some(Command::Args(args)) => args.is_empty(),
        Some(Command::Single(s)) => s.trim().is_empty(),
        None => false,
    };
    if is_empty {
        return Err(ManifestError::EmptyEntrypoint {
            resource: name.to_owned(),
        });
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
    for (resource, kind) in resources {
        for dep in kind.depends_on() {
            if !resources.contains_key(dep) {
                return Err(ManifestError::UnknownResource(format!(
                    "`{dep}` (depended on by `{resource}`)"
                )));
            }
        }
    }

    let graph: HashMap<&str, Vec<&str>> = resources
        .iter()
        .map(|(name, kind)| {
            let deps: Vec<&str> = kind
                .merged_dependencies(name)
                .into_iter()
                .filter_map(|dep| {
                    resources
                        .get_key_value(dep.as_str())
                        .map(|(k, _)| k.as_str())
                })
                .collect();
            (name.as_str(), deps)
        })
        .collect();

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
        for value in kind.interpolatable_strings() {
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
