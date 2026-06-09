//! LightShuttle CLI entry point.
//!
//! A thin shim over [`lightshuttle::run`]: all logic lives in the library so
//! the workspace tooling can share the same `clap` command tree when it
//! generates the CLI reference.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    lightshuttle::run().await
}
