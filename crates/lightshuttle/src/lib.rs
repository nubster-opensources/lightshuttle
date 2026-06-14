//! LightShuttle - lightweight developer-time orchestrator for polyglot teams.
//!
//! This crate is the **umbrella facade** of the LightShuttle ecosystem. It
//! wires together all member crates and exposes the [`run`] entry point that
//! the `lightshuttle` binary calls.
//!
//! # What is LightShuttle?
//!
//! LightShuttle reads a single YAML manifest (think `lightshuttle.yaml`) and
//! launches, supervises, and connects every service your project needs: API
//! servers, databases, workers, sidecars. It is the developer-time equivalent
//! of a production orchestrator, designed for teams that work across multiple
//! languages and runtimes.
//!
//! Key properties:
//!
//! - **Polyglot**: any process or Docker container is a first-class resource.
//! - **Single binary**: `lightshuttle up` is the only command developers need
//!   to memorize.
//! - **Export**: the same manifest drives `docker-compose`, Kubernetes, and
//!   Helm output via `lightshuttle export`.
//! - **Observable**: an optional bundled OpenTelemetry collector wires
//!   `OTEL_*` environment variables into every resource automatically.
//!
//! # Member crates
//!
//! | Crate | Purpose |
//! |-------|---------|
//! | [`lightshuttle_manifest`] | Manifest parser and domain model |
//! | [`lightshuttle_runtime`] | Process supervisor and Docker adapter |
//! | [`lightshuttle_control`] | Local HTTP control plane (`restart`, `ps`, `logs`) |
//! | [`lightshuttle_export`] | Emit Compose, Kubernetes, or Helm artifacts |
//! | [`lightshuttle_secrets`] | `.env` file loader and `${env.*}` resolver |
//! | [`lightshuttle_otel`] | Bundled OpenTelemetry collector integration |
//!
//! # CLI binary
//!
//! The `lightshuttle` binary is a thin shim over [`run`]. Install it with:
//!
//! ```text
//! cargo install lightshuttle
//! ```
//!
//! Then start your stack:
//!
//! ```text
//! lightshuttle up
//! ```
//!
//! **Exit codes**
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | Success |
//! | 1 | User error (invalid manifest, validation failure, lifecycle failure) |
//! | 2 | Runtime error (Docker unreachable, container failed to start) |
//! | 130 | Interrupted (SIGINT / Ctrl+C) |
//!
//! # Source repository
//!
//! <https://github.com/nubster-opensources/lightshuttle>
//!
//! # Using this crate as a library
//!
//! This crate is primarily a binary distribution. The only stable public
//! surface is [`run`], intended for embedders and workspace tooling that needs
//! access to the `clap` command tree:
//!
//! ```rust,no_run
//! #[tokio::main]
//! async fn main() -> std::process::ExitCode {
//!     lightshuttle::run().await
//! }
//! ```

#![deny(missing_docs)]

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

/// Parse the command line and dispatch to the matching subcommand implementation.
///
/// This is the single entry point shared by the `lightshuttle` binary and any
/// embedder. It reads `argv` via [`clap`], initialises logging (unless the
/// `up` subcommand handles its own tracing subscriber), runs the selected
/// command, and maps the result to a POSIX exit code.
///
/// # Exit codes
///
/// | Code | Meaning |
/// |------|---------|
/// | 0 | Success |
/// | 1 | User or lifecycle error (invalid manifest, validation failure, lifecycle failure) |
/// | 2 | Runtime error (Docker unreachable, container failed to start) |
/// | 130 | Interrupted (SIGINT / Ctrl+C, set by the tokio signal handler) |
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
