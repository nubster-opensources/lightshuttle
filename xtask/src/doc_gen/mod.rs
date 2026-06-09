//! Generation of the reference documentation pages.
//!
//! Each target (`manifest`, `cli`) renders a set of markdown pages from a
//! single source of truth, writes them under the book, and is verified fresh
//! by CI so the reference can never drift. The page model and the write/verify
//! gate are shared here; the per-target rendering lives in the submodules.

mod cli;
mod manifest;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// One generated page: its path under the book and its markdown body.
struct GeneratedPage {
    path: PathBuf,
    body: String,
}

/// `doc-gen <target> [--check]`: regenerate or verify generated documentation.
pub(crate) fn cmd(args: &[String]) -> Result<()> {
    let check = args.iter().any(|a| a == "--check");
    match args.first().map(String::as_str) {
        Some("manifest") => manifest::manifest(check),
        Some("cli") => cli::cli(check),
        Some(other) if !other.starts_with("--") => bail!("unknown doc-gen target: {other}"),
        _ => bail!("usage: cargo xtask doc-gen <manifest|cli> [--check]"),
    }
}

/// Build a page under `dir` with a trailing newline.
fn page(dir: &str, name: &str, mut body: String) -> GeneratedPage {
    if !body.ends_with('\n') {
        body.push('\n');
    }
    GeneratedPage {
        path: PathBuf::from(dir).join(name),
        body,
    }
}

/// Write every page (regenerate) or fail on any stale page (`--check`).
///
/// `target` is the `doc-gen` subcommand name, used both in the up-to-date
/// message and in the remediation hint printed when a page is stale.
fn write_or_check(target: &str, dir: &str, pages: &[GeneratedPage], check: bool) -> Result<()> {
    if check {
        let mut stale = Vec::new();
        for page in pages {
            // Normalise CRLF to LF so the gate stays platform-independent
            // when git autocrlf has rewritten line endings on checkout.
            let actual = std::fs::read_to_string(&page.path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            if actual != page.body {
                stale.push(page.path.display().to_string());
            }
        }
        if !stale.is_empty() {
            bail!(
                "stale generated page(s): {}; run `cargo xtask doc-gen {target}`",
                stale.join(", ")
            );
        }
        println!("doc-gen {target}: {} page(s) up to date", pages.len());
    } else {
        std::fs::create_dir_all(dir).with_context(|| format!("failed to create {dir}"))?;
        for page in pages {
            std::fs::write(&page.path, &page.body)
                .with_context(|| format!("failed to write {}", page.path.display()))?;
        }
        println!("doc-gen {target}: wrote {} page(s)", pages.len());
    }
    Ok(())
}

/// Collapse whitespace and escape table separators so prose fits one table cell.
fn cell(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}
