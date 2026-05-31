//! End-to-end tests for `lightshuttle alias`, driven through the built
//! binary with a temporary home directory.
//!
//! The deterministic cases target `zsh` so the rc file is always
//! `~/.zshrc`, sidestepping the bash `.bashrc` vs `.bash_profile` split.
//! A live-shell case that proves the alias actually resolves is kept
//! behind `#[ignore]` because it needs a real interactive shell.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

const ALIAS_LINE: &str = "alias lsh='lightshuttle'";
const MARKER_BEGIN: &str = "# >>> lightshuttle alias >>>";

/// Build a command pointed at an isolated home and an empty PATH, so the
/// run can never pick up a real `lsh` on the developer's machine.
fn cmd(home: &Path, empty_bin: &Path) -> Command {
    let mut c = Command::cargo_bin("lightshuttle").expect("binary builds");
    c.env("HOME", home)
        .env("USERPROFILE", home)
        .env("PATH", empty_bin);
    c
}

fn zshrc(home: &Path) -> PathBuf {
    home.join(".zshrc")
}

#[test]
fn install_check_uninstall_round_trip() {
    let home = TempDir::new().expect("temp home");
    let empty_bin = TempDir::new().expect("temp bin");
    let home_path = home.path();
    let bin_path = empty_bin.path();

    // Seed a non-empty rc so we exercise appending, not file creation.
    std::fs::write(zshrc(home_path), "export EXISTING=1\n").expect("seed rc");

    cmd(home_path, bin_path)
        .args(["alias", "check", "--shell", "zsh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("absent"));

    cmd(home_path, bin_path)
        .args(["alias", "install", "--shell", "zsh", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("added"));

    let after_install = std::fs::read_to_string(zshrc(home_path)).expect("rc readable");
    assert!(
        after_install.contains("export EXISTING=1"),
        "body preserved"
    );
    assert!(after_install.contains(MARKER_BEGIN), "block marker present");
    assert!(after_install.contains(ALIAS_LINE), "alias line present");

    // Second install is a no-op.
    cmd(home_path, bin_path)
        .args(["alias", "install", "--shell", "zsh", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already present"));
    assert_eq!(
        std::fs::read_to_string(zshrc(home_path))
            .expect("rc readable")
            .matches(ALIAS_LINE)
            .count(),
        1,
        "re-install must not duplicate the alias",
    );

    cmd(home_path, bin_path)
        .args(["alias", "check", "--shell", "zsh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("present"));

    cmd(home_path, bin_path)
        .args(["alias", "uninstall", "--shell", "zsh", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("removed"));

    let after_uninstall = std::fs::read_to_string(zshrc(home_path)).expect("rc readable");
    assert!(after_uninstall.contains("export EXISTING=1"), "body intact");
    assert!(!after_uninstall.contains(MARKER_BEGIN), "block removed");
    assert!(!after_uninstall.contains(ALIAS_LINE), "alias removed");

    // Second uninstall is a no-op.
    cmd(home_path, bin_path)
        .args(["alias", "uninstall", "--shell", "zsh", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no `lsh` alias"));
}

#[test]
fn check_never_writes() {
    let home = TempDir::new().expect("temp home");
    let empty_bin = TempDir::new().expect("temp bin");

    cmd(home.path(), empty_bin.path())
        .args(["alias", "check", "--shell", "zsh"])
        .assert()
        .success();

    assert!(
        !zshrc(home.path()).exists(),
        "check must not create the rc file",
    );
}

#[test]
fn unknown_shell_flag_is_rejected() {
    let home = TempDir::new().expect("temp home");
    let empty_bin = TempDir::new().expect("temp bin");

    cmd(home.path(), empty_bin.path())
        .args(["alias", "install", "--shell", "tcsh"])
        .assert()
        .failure();
}

/// Proves the installed alias actually resolves in a live interactive
/// bash. Ignored by default: it needs a real bash and writes to
/// `~/.bashrc` under a temporary home.
#[test]
#[ignore = "requires an interactive bash shell"]
#[cfg(unix)]
fn installed_alias_resolves_in_bash() {
    let home = TempDir::new().expect("temp home");
    let empty_bin = TempDir::new().expect("temp bin");

    cmd(home.path(), empty_bin.path())
        .args(["alias", "install", "--shell", "bash", "--yes"])
        .assert()
        .success();

    let output = std::process::Command::new("bash")
        .args(["-i", "-c", "type lsh"])
        .env("HOME", home.path())
        .output()
        .expect("bash runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("lightshuttle"),
        "expected `lsh` to alias lightshuttle, got: {stdout}",
    );
}
