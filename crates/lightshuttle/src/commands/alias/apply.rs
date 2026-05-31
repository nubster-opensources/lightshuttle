//! Side-effecting helpers: filesystem, PATH lookup and stdin prompt.
//!
//! Every function that reads the environment or touches a file lives
//! here, so the planner in [`super::plan`] can stay pure.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use super::shell::Shell;

/// True when an `lsh` executable is resolvable on the PATH.
///
/// A shell alias never appears as a PATH entry, so any `lsh` binary
/// found is necessarily a different program (the GNU lsh package) that
/// `install` must not shadow.
pub(crate) fn lsh_on_path() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let names: &[&str] = if cfg!(windows) {
        &["lsh.exe", "lsh.bat", "lsh.cmd"]
    } else {
        &["lsh"]
    };
    std::env::split_paths(&path).any(|dir| names.iter().any(|name| dir.join(name).is_file()))
}

/// Resolve the startup file path for `shell`.
///
/// `PowerShell`'s profile path is not derivable from the home directory
/// alone, so it is queried from the interpreter; the other shells use
/// their conventional location.
pub(crate) fn rc_path(shell: Shell) -> Result<PathBuf> {
    match shell {
        Shell::Bash => {
            let name = if cfg!(target_os = "macos") {
                ".bash_profile"
            } else {
                ".bashrc"
            };
            Ok(home_dir()?.join(name))
        }
        Shell::Zsh => Ok(home_dir()?.join(".zshrc")),
        Shell::Fish => Ok(home_dir()?.join(".config").join("fish").join("config.fish")),
        Shell::PowerShell => powershell_profile_path(),
    }
}

/// Read the rc file, returning an empty string when it does not exist.
pub(crate) fn read_rc(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("failed to read `{}`", path.display())),
    }
}

/// Write `contents` to the rc file, creating parent directories.
pub(crate) fn write_rc(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("failed to write `{}`", path.display()))
}

/// Ask the user to confirm a mutating action. Returns `true` only on an
/// explicit `y`/`yes` answer.
pub(crate) fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N]: ");
    std::io::stdout()
        .flush()
        .context("failed to flush stdout")?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;
    let answer = answer.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Best-effort home directory from the platform environment variables.
fn home_dir() -> Result<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("could not determine the home directory from `{var}`"))
}

/// Query `PowerShell` for `$PROFILE.CurrentUserAllHosts`, falling back
/// to the conventional `PowerShell` 7 location when no interpreter
/// answers.
fn powershell_profile_path() -> Result<PathBuf> {
    for exe in ["pwsh", "powershell"] {
        let output = Command::new(exe)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$PROFILE.CurrentUserAllHosts",
            ])
            .output();
        if let Ok(output) = output
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }
    Ok(home_dir()?
        .join("Documents")
        .join("PowerShell")
        .join("Microsoft.PowerShell_profile.ps1"))
}
