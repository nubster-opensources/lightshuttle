//! `xtask`: workspace tooling entry point.
//!
//! Invoke through the alias declared in `.cargo/config.toml`:
//!
//! ```sh
//! cargo xtask help
//! cargo xtask schema [--check]
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// Path to the canonical JSON Schema, relative to the workspace root.
const SCHEMA_PATH: &str = "docs/spec/manifest-v0.schema.json";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => schema_cmd(&args[1..]),
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => bail!("unknown task: {other}"),
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  schema [--check]   Regenerate or verify the JSON Schema");
    eprintln!("  help               Show this message");
}

fn schema_cmd(args: &[String]) -> Result<()> {
    let check = args.iter().any(|a| a == "--check");
    let target = PathBuf::from(SCHEMA_PATH);

    let schema = lightshuttle_manifest::schema();
    let mut generated = serde_json::to_string_pretty(&schema)
        .context("failed to serialise schema as JSON")?;
    generated.push('\n');

    if check {
        let actual = std::fs::read_to_string(&target)
            .with_context(|| format!("failed to read {}", target.display()))?;
        // Normalise CRLF to LF so the gate stays platform-independent
        // when git autocrlf has rewritten line endings on checkout.
        let actual_normalised = actual.replace("\r\n", "\n");
        if actual_normalised != generated {
            bail!(
                "schema at {} is out of date: run `cargo xtask schema` to regenerate",
                target.display()
            );
        }
        println!("schema at {} is up to date", target.display());
    } else {
        std::fs::write(&target, &generated)
            .with_context(|| format!("failed to write {}", target.display()))?;
        println!("wrote {} ({} bytes)", target.display(), generated.len());
    }
    Ok(())
}
