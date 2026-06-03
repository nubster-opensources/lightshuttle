//! `lightshuttle secrets`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lightshuttle_manifest::{InterpolationContext, Interpolator, Reference};
use lightshuttle_secrets::{EnvFileSource, SecretSource as _};
use owo_colors::OwoColorize;

use crate::cli::SecretsAction;

use super::{ExitOutcome, load_manifest};

/// Dispatch secrets subcommands.
pub(crate) fn run(file: &Path, action: &SecretsAction) -> Result<ExitOutcome> {
    match action {
        SecretsAction::Check { env_file } => check(file, env_file.clone()),
    }
}

/// Status of a single `${env.VAR}` reference.
#[derive(Debug)]
enum VarStatus {
    /// Found in the loaded env map.
    Set,
    /// Not set but every occurrence in the manifest supplies a default.
    Defaulted(String),
    /// Not set and at least one occurrence has no fallback.
    Missing,
}

struct VarEntry {
    /// `true` when at least one reference uses `${env.NAME}` with no default.
    required: bool,
    /// Default value from the first optional occurrence, for display.
    default_hint: Option<String>,
}

fn check(file: &Path, env_file: Option<PathBuf>) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let yaml = manifest
        .to_yaml()
        .context("failed to re-serialize manifest")?;

    // Scan the full YAML text for every ${env.*} reference without resolving.
    let ctx = InterpolationContext::new();
    let interpolator = Interpolator::new(&ctx);
    let refs = interpolator
        .scan(&yaml)
        .context("failed to scan manifest for env references")?;

    // Aggregate per variable name. BTreeMap keeps alphabetical display order.
    let mut vars: BTreeMap<String, VarEntry> = BTreeMap::new();
    for reference in refs {
        if let Reference::Env { name, default } = reference {
            let entry = vars.entry(name).or_insert(VarEntry {
                required: false,
                default_hint: default.clone(),
            });
            if default.is_none() {
                entry.required = true;
            }
        }
    }

    if vars.is_empty() {
        println!(
            "no `${{env.*}}` references found in project `{}`",
            manifest.project.name
        );
        return Ok(ExitOutcome::Success);
    }

    let env_map = load_env(env_file)?;

    println!("secrets for project `{}`:\n", manifest.project.name);

    let mut missing_count = 0usize;
    for (name, entry) in &vars {
        let status = resolve_status(name, entry, &env_map);
        match &status {
            VarStatus::Set => {
                println!("  {:<32} {}", name, "set".green());
            }
            VarStatus::Defaulted(hint) => {
                println!("  {:<32} {} ({})", name, "default".yellow(), hint.dimmed());
            }
            VarStatus::Missing => {
                missing_count += 1;
                println!("  {:<32} {}", name, "missing".red().bold());
            }
        }
    }

    if missing_count > 0 {
        Err(anyhow::anyhow!(
            "{missing_count} required variable(s) not set — add them to a .env file or pass --env-file <PATH>"
        ))
    } else {
        println!("\n{}", "all required secrets are set".green().bold());
        Ok(ExitOutcome::Success)
    }
}

fn resolve_status(name: &str, entry: &VarEntry, env_map: &HashMap<String, String>) -> VarStatus {
    if env_map.contains_key(name) {
        VarStatus::Set
    } else if !entry.required {
        VarStatus::Defaulted(entry.default_hint.clone().unwrap_or_default())
    } else {
        VarStatus::Missing
    }
}

fn load_env(path: Option<PathBuf>) -> Result<HashMap<String, String>> {
    if let Some(explicit) = path {
        let source = EnvFileSource::load(&explicit)
            .with_context(|| format!("failed to load --env-file {}", explicit.display()))?;
        source
            .load()
            .with_context(|| format!("failed to read env file {}", explicit.display()))
    } else {
        match EnvFileSource::load_optional(".env").context("failed to parse .env")? {
            Some(source) => source.load().context("failed to read .env"),
            None => Ok(HashMap::new()),
        }
    }
}
