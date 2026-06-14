//! `lightshuttle` binary entry point.
//!
//! A thin shim over [`lightshuttle::run`]. All parsing and command logic live
//! in the `lightshuttle` library crate so that workspace tooling (doc
//! generation, integration tests) can share the same `clap` command tree
//! without spawning a subprocess.
//!
//! # Exit codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | Success |
//! | 1 | User or lifecycle error (invalid manifest, validation failure) |
//! | 2 | Runtime error (Docker unreachable, container failed to start) |
//! | 130 | Interrupted (SIGINT / Ctrl+C) |

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    lightshuttle::run().await
}
