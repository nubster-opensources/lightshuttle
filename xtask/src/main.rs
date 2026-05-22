//! `xtask`: workspace tooling entry point.
//!
//! Invoke through the alias declared in `.cargo/config.toml`:
//!
//! ```sh
//! cargo xtask help
//! ```
//!
//! This binary is intentionally minimal at bootstrap. Concrete tasks
//! (`release`, `schema`, `lint`) land in follow-up pull requests.

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("help") | None => {
            eprintln!("Usage: cargo xtask <command>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  help    Show this message");
            Ok(())
        }
        Some(other) => bail!("unknown task: {other}"),
    }
}
