//! Shared helpers for the external-validation tests.

/// True when `bin` is installed and on the PATH. Used by the ignored
/// validation tests to skip gracefully instead of failing when the
/// external tool is absent.
///
/// Presence is detected by spawning `bin --help` (a flag every target
/// tool accepts) and checking only that the process ran: a spawn error
/// means the binary is absent, while any exit status means it is there.
pub(crate) fn tool_available(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}
