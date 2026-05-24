//! CLI argument parser definitions.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    pub file: Option<PathBuf>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// All recognised subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Boot the stack and supervise it until interrupted.
    Up {
        /// SIGTERM-to-SIGKILL grace window per resource at shutdown.
        #[arg(long, default_value = "10s")]
        grace: humantime::Duration,
    },

    /// Stop every container managed by this project.
    Down {
        /// SIGTERM-to-SIGKILL grace window per container.
        #[arg(long, default_value = "10s")]
        grace: humantime::Duration,
    },

    /// List managed resources and their status.
    Ps,

    /// Stream logs of a single resource.
    Logs {
        /// Resource name as declared in the manifest.
        resource: String,
        /// Follow the log stream until interrupted.
        #[arg(long, short = 'f')]
        follow: bool,
    },

    /// Parse and validate the manifest without starting anything.
    Validate {
        /// Upgrade warnings to errors. Use in continuous integration.
        #[arg(long)]
        strict: bool,
    },

    /// Dump the resolved manifest to stdout as YAML.
    Manifest,
}
