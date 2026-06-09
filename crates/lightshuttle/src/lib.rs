//! LightShuttle library surface.
//!
//! Holds the CLI parser, the per-command implementations and the glue that the
//! `lightshuttle` binary drives. The binary itself is a thin shim over [`run`];
//! the workspace tooling that generates the CLI reference depends on this
//! library to read the `clap` command tree.

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::commands::ExitOutcome;
use crate::discovery::resolve_manifest;

#[doc(hidden)]
pub mod cli;
mod commands;
mod control_url;
mod discovery;
mod output;

/// Parse the command line and run the selected command to a POSIX exit code.
pub async fn run() -> ExitCode {
    let cli = Cli::parse();

    // `up` owns its telemetry init: with OTel enabled it installs a
    // tracing subscriber wired to the OTLP exporter, otherwise it falls
    // back to plain logging itself. Every other command uses plain
    // logging set up here. The global subscriber can only be installed
    // once, hence the split.
    if !matches!(cli.command, Command::Up { .. }) {
        init_plain_logging();
    }

    match dispatch(cli).await {
        Ok(outcome) => ExitCode::from(u8::try_from(outcome.code()).unwrap_or(1)),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

/// Resolve the manifest where the command needs it and delegate to the
/// matching per-command module.
async fn dispatch(cli: Cli) -> anyhow::Result<ExitOutcome> {
    match cli.command {
        Command::Up {
            grace,
            control_port,
            no_otel,
            env_file,
        } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::up::run(&manifest, grace.into(), control_port, no_otel, env_file).await
        }
        Command::Down { grace } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::down::run(&manifest, grace.into()).await
        }
        Command::Ps => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::ps::run(&manifest).await
        }
        Command::Logs { resource, follow } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::logs::run(&manifest, &resource, follow).await
        }
        Command::Validate { strict } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::validate::run(&manifest, strict)
        }
        Command::Manifest => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::manifest::run(&manifest)
        }
        Command::Restart { resource, detach } => commands::restart::run(&resource, detach).await,
        Command::Alias { action } => commands::alias::run(&action),
        Command::Secrets { action } => {
            let manifest_path = resolve_manifest(cli.file.as_deref())?;
            commands::secrets::run(&manifest_path, &action)
        }
        Command::Export {
            target,
            output,
            force,
        } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::export::run(&manifest, target, output, force)
        }
    }
}

/// Install a plain compact `fmt` tracing subscriber. Used by every command
/// except `up` with `OTel` enabled, which installs its own subscriber wired to
/// the OTLP exporter.
pub(crate) fn init_plain_logging() {
    let filter =
        EnvFilter::try_from_env("LIGHTSHUTTLE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();
}
