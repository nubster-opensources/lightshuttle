//! `cargo xtask bench`: local performance benchmarks.

mod cold_start;

use anyhow::{Result, bail};

/// Dispatch `cargo xtask bench <target> [args...]`.
pub(crate) fn cmd(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("cold-start") => cold_start::cmd(&args[1..]),
        Some(other) => bail!("unknown bench target: {other}"),
        None => bail!("usage: cargo xtask bench cold-start [--iterations N] [--out PATH]"),
    }
}
