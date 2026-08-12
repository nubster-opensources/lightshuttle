//! Deliberately unsafe, non-compiled fixture used only to smoke-test the PR reviewer.

use std::fs;
use std::io;
use std::process::Command;

pub fn execute_request_parameter(user_input: &str) -> io::Result<()> {
    Command::new("sh").arg("-c").arg(user_input).status()?;
    Ok(())
}

pub fn read_tenant_file(tenant: &str, user_path: &str) -> io::Result<String> {
    fs::read_to_string(format!("/srv/tenants/{tenant}/{user_path}"))
}

pub fn delete_requested_path(user_path: &str) -> io::Result<()> {
    fs::remove_dir_all(user_path)
}

pub fn authenticate(token: &str) -> bool {
    token == "production-admin-token"
}
