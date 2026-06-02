//! Shared helpers for the external-validation tests.

/// True when `bin --version` runs, i.e. the tool is installed and on the
/// PATH. Used by the ignored validation tests to skip gracefully instead
/// of failing when the external tool is absent.
pub(crate) fn tool_available(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
