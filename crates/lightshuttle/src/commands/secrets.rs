//! `lightshuttle secrets`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lightshuttle_runtime::{EnvSource, EnvVarStatus, LifecyclePlan};
use owo_colors::OwoColorize;

use crate::cli::SecretsAction;

use super::{ExitOutcome, load_env, load_manifest};

/// Dispatch secrets subcommands.
pub(crate) fn run(file: &Path, action: &SecretsAction) -> Result<ExitOutcome> {
    match action {
        SecretsAction::Check { env_file } => check(file, env_file.clone()),
    }
}

/// Report which `${env.*}` variables the manifest needs and whether each
/// one resolves, defaults, or is missing.
///
/// The classification is produced by [`LifecyclePlan::env_report`], the same
/// engine `up` uses for its fail-fast preflight, so this command predicts a
/// real run exactly. No container runtime is required: the plan is built and
/// inspected without contacting any daemon.
fn check(file: &Path, env_file: Option<PathBuf>) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let plan = LifecyclePlan::from_manifest(&manifest)
        .context("failed to build an execution plan from the manifest")?;

    let env_map = load_env(env_file)?;
    let report = plan.env_report(&env_map);

    if report.is_empty() {
        println!(
            "no `${{env.*}}` references found in project `{}`",
            manifest.project.name
        );
        return Ok(ExitOutcome::Success);
    }

    println!("secrets for project `{}`:\n", manifest.project.name);

    for var in &report.vars {
        match &var.status {
            EnvVarStatus::Resolved(EnvSource::EnvFile) => {
                println!("  {:<32} {}", var.name, "set (.env)".green());
            }
            EnvVarStatus::Resolved(EnvSource::Process) => {
                println!("  {:<32} {}", var.name, "set (env)".green());
            }
            EnvVarStatus::Defaulted { defaults } => {
                println!(
                    "  {:<32} {} ({})",
                    var.name,
                    "default".yellow(),
                    defaults.join(" | ").dimmed()
                );
            }
            EnvVarStatus::Missing => {
                println!("  {:<32} {}", var.name, "missing".red().bold());
            }
        }
    }

    let missing = report.missing();
    if missing.is_empty() {
        println!("\n{}", "all required secrets are set".green().bold());
        Ok(ExitOutcome::Success)
    } else {
        Err(anyhow::anyhow!(
            "{} required variable(s) not set: {} \u{2014} add them to a .env file or pass --env-file <PATH>",
            missing.len(),
            missing.join(", "),
        ))
    }
}
