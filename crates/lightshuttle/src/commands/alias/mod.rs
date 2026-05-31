//! `lightshuttle alias`: manage the optional `lsh` shell alias.
//!
//! The command is split into a pure planner ([`plan`]) and a side
//! effecting layer ([`apply`]); this module wires them to the CLI and
//! owns the interactive confirmation and user-facing output.

mod apply;
mod plan;
pub(crate) mod shell;

use anyhow::{Result, anyhow};

use self::plan::AliasPlan;
use self::shell::Shell;
use super::ExitOutcome;
use crate::cli::AliasAction;

/// Entry point dispatched from `main::run`.
pub(crate) fn run(action: &AliasAction) -> Result<ExitOutcome> {
    match *action {
        AliasAction::Install { shell, yes } => install(shell, yes),
        AliasAction::Check { shell } => check(shell),
        AliasAction::Uninstall { shell, yes } => uninstall(shell, yes),
    }
}

fn install(shell: Option<Shell>, yes: bool) -> Result<ExitOutcome> {
    let shell = resolve_shell(shell)?;
    let path = apply::rc_path(shell)?;
    let contents = apply::read_rc(&path)?;

    match plan::plan_install(&contents, apply::lsh_on_path()) {
        AliasPlan::Conflict { reason } => Err(anyhow!(reason)),
        AliasPlan::AlreadyPresent => {
            println!(
                "ok: `lsh` alias already present in {} ({})",
                path.display(),
                shell.label()
            );
            Ok(ExitOutcome::Success)
        }
        AliasPlan::WillAdd => {
            let line = shell.alias_line();
            println!("Detected shell: {}", shell.label());
            println!("Will add `{line}` to {}", path.display());
            if !yes && !apply::confirm("Proceed?")? {
                println!("aborted: nothing was written");
                return Ok(ExitOutcome::Success);
            }
            let updated = plan::with_block_added(&contents, &line);
            apply::write_rc(&path, &updated)?;
            println!(
                "ok: added `lsh` alias. Restart your shell or reload {}",
                path.display()
            );
            Ok(ExitOutcome::Success)
        }
        AliasPlan::WillRemove | AliasPlan::NotPresent => {
            unreachable!("install never plans removal")
        }
    }
}

fn check(shell: Option<Shell>) -> Result<ExitOutcome> {
    let shell = resolve_shell(shell)?;
    let path = apply::rc_path(shell)?;
    let contents = apply::read_rc(&path)?;

    match plan::plan_install(&contents, apply::lsh_on_path()) {
        AliasPlan::Conflict { reason } => {
            println!("conflict: {reason}");
            Ok(ExitOutcome::Success)
        }
        AliasPlan::AlreadyPresent => {
            println!(
                "present: `lsh` alias is installed in {} ({})",
                path.display(),
                shell.label()
            );
            Ok(ExitOutcome::Success)
        }
        AliasPlan::WillAdd => {
            println!(
                "absent: `install` would add `{}` to {} ({})",
                shell.alias_line(),
                path.display(),
                shell.label()
            );
            Ok(ExitOutcome::Success)
        }
        AliasPlan::WillRemove | AliasPlan::NotPresent => unreachable!("check uses plan_install"),
    }
}

fn uninstall(shell: Option<Shell>, yes: bool) -> Result<ExitOutcome> {
    let shell = resolve_shell(shell)?;
    let path = apply::rc_path(shell)?;
    let contents = apply::read_rc(&path)?;

    match plan::plan_uninstall(&contents) {
        AliasPlan::NotPresent => {
            println!(
                "ok: no `lsh` alias in {} ({})",
                path.display(),
                shell.label()
            );
            Ok(ExitOutcome::Success)
        }
        AliasPlan::WillRemove => {
            println!("Will remove the `lsh` alias from {}", path.display());
            if !yes && !apply::confirm("Proceed?")? {
                println!("aborted: nothing was written");
                return Ok(ExitOutcome::Success);
            }
            let updated = plan::with_block_removed(&contents);
            apply::write_rc(&path, &updated)?;
            println!("ok: removed `lsh` alias from {}", path.display());
            Ok(ExitOutcome::Success)
        }
        AliasPlan::AlreadyPresent | AliasPlan::WillAdd | AliasPlan::Conflict { .. } => {
            unreachable!("uninstall uses plan_uninstall")
        }
    }
}

/// Resolve the shell from an explicit `--shell` flag, falling back to
/// auto-detection. Errors with guidance when detection fails.
fn resolve_shell(flag: Option<Shell>) -> Result<Shell> {
    flag.or_else(Shell::detect).ok_or_else(|| {
        anyhow!("could not determine your shell; pass `--shell <bash|zsh|fish|powershell>`")
    })
}
