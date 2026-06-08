//! `xtask`: workspace tooling entry point.
//!
//! Invoke through the alias declared in `.cargo/config.toml`:
//!
//! ```sh
//! cargo xtask help
//! cargo xtask schema [--check]
//! cargo xtask doc-validate
//! cargo xtask doc-gen manifest [--check]
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

mod doc_gen;

/// Path to the canonical JSON Schema, relative to the workspace root.
const SCHEMA_PATH: &str = "docs/spec/manifest-v0.schema.json";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => schema_cmd(&args[1..]),
        Some("doc-validate") => doc_validate_cmd(),
        Some("doc-gen") => doc_gen::cmd(&args[1..]),
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
    eprintln!("  doc-validate       Validate every manifest example in the book");
    eprintln!("  doc-gen <target>   Generate reference pages (target: manifest)");
    eprintln!("  help               Show this message");
}

fn schema_cmd(args: &[String]) -> Result<()> {
    let check = args.iter().any(|a| a == "--check");
    let target = PathBuf::from(SCHEMA_PATH);

    let schema = lightshuttle_manifest::schema();
    let mut generated =
        serde_json::to_string_pretty(&schema).context("failed to serialise schema as JSON")?;
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

/// Directory holding the mdBook sources whose manifest blocks are validated.
const BOOK_SRC: &str = "docs/book/src";

/// A manifest snippet located in the documentation, kept for diagnostics.
struct ManifestBlock {
    file: PathBuf,
    /// 1-based index of the `yaml` block within its file.
    index: usize,
    yaml: String,
}

/// Validate every full-manifest `yaml` block found in the book sources.
///
/// Documentation snippets are code: a block that no longer parses against the
/// current manifest types fails the build, so the examples cannot rot. Indented
/// fragments that do not declare a root-level `project:` key are skipped.
fn doc_validate_cmd() -> Result<()> {
    let root = PathBuf::from(BOOK_SRC);
    let files =
        markdown_files(&root).with_context(|| format!("failed to scan {}", root.display()))?;

    let mut validated = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for file in &files {
        let contents = std::fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        for block in manifest_blocks(file, &contents) {
            match lightshuttle_manifest::Manifest::parse(&block.yaml) {
                Ok(_) => validated += 1,
                Err(err) => failures.push(format!(
                    "{}: yaml block #{}: {err:#}",
                    block.file.display(),
                    block.index
                )),
            }
        }
    }

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("error: {failure}");
        }
        bail!(
            "{} documentation manifest example(s) failed to validate",
            failures.len()
        );
    }

    if validated == 0 {
        bail!(
            "no manifest example found under {BOOK_SRC}: the extractor matched nothing, \
             which usually means the heuristic or the source layout changed"
        );
    }

    println!(
        "doc-validate: {validated} manifest example(s) validated across {} file(s)",
        files.len()
    );
    Ok(())
}

/// Recursively collect every Markdown file under `root`, sorted for stable output.
fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_markdown(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// Pull every fenced `yaml` block out of one Markdown source, keeping only the
/// blocks whose content declares a root-level `project:` key.
fn manifest_blocks(file: &Path, contents: &str) -> Vec<ManifestBlock> {
    let mut blocks = Vec::new();
    let mut lines = contents.lines();
    let mut yaml_index = 0usize;

    while let Some(line) = lines.next() {
        if line.trim_end() != "```yaml" {
            continue;
        }
        yaml_index += 1;
        let mut body = String::new();
        for inner in lines.by_ref() {
            if inner.trim_end() == "```" {
                break;
            }
            body.push_str(inner);
            body.push('\n');
        }
        if is_full_manifest(&body) {
            blocks.push(ManifestBlock {
                file: file.to_path_buf(),
                index: yaml_index,
                yaml: body,
            });
        }
    }

    blocks
}

/// A block is a complete manifest when it declares a `project:` key at column
/// zero, as opposed to an indented fragment of a larger manifest.
fn is_full_manifest(yaml: &str) -> bool {
    yaml.lines().any(|line| line.starts_with("project:"))
}
