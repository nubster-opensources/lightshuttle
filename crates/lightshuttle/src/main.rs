//! LightShuttle CLI entry point.
//!
//! Wires the `clap` parser to the per-command modules and translates
//! their outcome into a POSIX exit code.

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::commands::ExitOutcome;
use crate::discovery::resolve_manifest;

mod cli;
mod commands;
mod control_url;
mod discovery;
mod output;

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();

    match run(cli).await {
        Ok(outcome) => ExitCode::from(u8::try_from(outcome.code()).unwrap_or(1)),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<ExitOutcome> {
    match cli.command {
        Command::Up {
            grace,
            control_port,
        } => {
            let manifest = resolve_manifest(cli.file.as_deref())?;
            commands::up::run(&manifest, grace.into(), control_port).await
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
    }
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_env("LIGHTSHUTTLE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();
}
