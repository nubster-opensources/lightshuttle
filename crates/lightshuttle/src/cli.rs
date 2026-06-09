//! CLI argument parser definitions.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::commands::alias::shell::Shell;

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(
    name = "lightshuttle",
    version,
    about = "Lightweight dev orchestrator for polyglot teams"
)]
pub struct Cli {
    /// Path to the manifest. Overrides the upward discovery.
    #[arg(long, short = 'f', global = true)]
    pub(crate) file: Option<PathBuf>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// All recognised subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Boot the stack and supervise it until interrupted.
    #[command(after_long_help = "Examples:
  # Boot the stack defined by the nearest manifest
  lightshuttle up

  # Use an explicit manifest and a custom control plane port
  lightshuttle -f stack.yaml up --control-port 8080

  # Run without the bundled OpenTelemetry collector
  lightshuttle up --no-otel")]
    Up {
        /// SIGTERM-to-SIGKILL grace window per resource at shutdown.
        #[arg(long, default_value = "10s")]
        grace: humantime::Duration,

        /// Override the local control plane port. Defaults to
        /// `dashboard.port` from the manifest, or a random free port
        /// picked by the OS.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        control_port: Option<u16>,

        /// Skip the bundled OpenTelemetry collector and the
        /// per-resource `OTEL_*` env injection, even if
        /// `observability.otel.enabled` is `true` (or absent) in the
        /// manifest.
        #[arg(long)]
        no_otel: bool,

        /// Path to a .env file supplying secret values referenced as
        /// `${env.VAR}` in the manifest. An explicit path that does not
        /// exist is an error. The default `.env` is silently skipped
        /// when absent.
        #[arg(long)]
        env_file: Option<PathBuf>,
    },

    /// Stop every container managed by this project.
    #[command(after_long_help = "Examples:
  # Stop every container, allowing 30s for graceful shutdown
  lightshuttle down --grace 30s")]
    Down {
        /// SIGTERM-to-SIGKILL grace window per container.
        #[arg(long, default_value = "10s")]
        grace: humantime::Duration,
    },

    /// List managed resources and their status.
    #[command(after_long_help = "Examples:
  # List managed resources and their status
  lightshuttle ps")]
    Ps,

    /// Stream logs of a single resource.
    #[command(after_long_help = "Examples:
  # Show the recent logs of the `api` resource
  lightshuttle logs api

  # Follow the log stream until interrupted
  lightshuttle logs api --follow")]
    Logs {
        /// Resource name as declared in the manifest.
        resource: String,
        /// Follow the log stream until interrupted.
        #[arg(long, short = 'f')]
        follow: bool,
    },

    /// Parse and validate the manifest without starting anything.
    #[command(after_long_help = "Examples:
  # Validate the manifest
  lightshuttle validate

  # Fail on warnings, for continuous integration
  lightshuttle validate --strict")]
    Validate {
        /// Upgrade warnings to errors. Use in continuous integration.
        #[arg(long)]
        strict: bool,
    },

    /// Dump the resolved manifest to stdout as YAML.
    #[command(after_long_help = "Examples:
  # Print the resolved manifest as YAML
  lightshuttle manifest")]
    Manifest,

    /// Restart a single managed resource through the running control
    /// plane. Requires `lightshuttle up` to be active in the same
    /// working directory so the discovery file
    /// `.lightshuttle/control.url` is present.
    #[command(after_long_help = "Examples:
  # Restart the `api` resource and wait for it to become healthy again
  lightshuttle restart api

  # Request the restart and return immediately
  lightshuttle restart api --detach")]
    Restart {
        /// Resource name as declared in the manifest.
        resource: String,

        /// Return immediately after the control plane accepted the
        /// restart request, without waiting for the resource to become
        /// healthy again.
        #[arg(long)]
        detach: bool,
    },

    /// Manage the optional `lsh` shell alias.
    #[command(after_long_help = "Examples:
  # Install the `lsh` alias into your shell's startup file
  lightshuttle alias install

  # Preview what install would do, without writing anything
  lightshuttle alias check

  # Remove the alias
  lightshuttle alias uninstall")]
    Alias {
        /// Action to perform.
        #[command(subcommand)]
        action: AliasAction,
    },

    /// Generate deployment artifacts from the manifest.
    #[command(after_long_help = "Examples:
  # Generate a docker-compose.yml under ./export/compose
  lightshuttle export compose

  # Generate Kubernetes manifests into a chosen directory, overwriting it
  lightshuttle export kubernetes --output ./k8s --force")]
    Export {
        /// Target format to generate.
        target: ExportTarget,

        /// Output directory. Defaults to `./export/<target>`.
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,

        /// Overwrite a non-empty output directory.
        #[arg(long)]
        force: bool,
    },

    /// Inspect `${env.*}` variable references in the manifest.
    #[command(after_long_help = "Examples:
  # Report which ${env.*} variables are set, defaulted, or missing
  lightshuttle secrets check

  # Check against a specific .env file
  lightshuttle secrets check --env-file .env.prod")]
    Secrets {
        /// Action to perform.
        #[command(subcommand)]
        action: SecretsAction,
    },
}

/// Deployment targets the `export` command can generate.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum ExportTarget {
    /// A `docker-compose.yml` file.
    Compose,
    /// Plain Kubernetes manifests, one file per resource.
    Kubernetes,
    /// A Helm chart.
    Helm,
}

/// Actions for the `secrets` subcommand.
#[derive(Debug, Subcommand)]
pub(crate) enum SecretsAction {
    /// Report which `${env.*}` variables are set, defaulted, or missing.
    Check {
        /// Path to a .env file to check against. Defaults to `.env` when
        /// present in the working directory; silently skipped when absent.
        #[arg(long)]
        env_file: Option<PathBuf>,
    },
}

/// Actions for the `alias` subcommand.
#[derive(Debug, Subcommand)]
pub(crate) enum AliasAction {
    /// Add the `lsh` alias to your shell's startup file.
    Install {
        /// Override shell auto-detection.
        #[arg(long)]
        shell: Option<Shell>,

        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },

    /// Report what `install` would do, without writing anything.
    Check {
        /// Override shell auto-detection.
        #[arg(long)]
        shell: Option<Shell>,
    },

    /// Remove the `lsh` alias from your shell's startup file.
    Uninstall {
        /// Override shell auto-detection.
        #[arg(long)]
        shell: Option<Shell>,

        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}
