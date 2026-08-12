//! Deliberately unsafe, non-compiled fixture for the AI reviewer cache smoke test.

use std::process::Command;

pub fn execute_untrusted(input: &str) {
    let _ = Command::new("sh").arg("-c").arg(input).status();
}
