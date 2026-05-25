//! Formatting helpers for tabular CLI output and streaming logs.

use std::fmt::Write as _;
use std::io::Write;

use lightshuttle_runtime::{ContainerStatus, LogChunk, LogStream, ManagedContainer};

/// Render a `ps`-style tabular view of the managed containers.
#[must_use]
pub(crate) fn format_ps(containers: &[ManagedContainer]) -> String {
    if containers.is_empty() {
        return "(no managed containers found for this project)\n".to_owned();
    }

    let resource_width = containers
        .iter()
        .map(|c| c.resource.len())
        .max()
        .unwrap_or(8)
        .max("RESOURCE".len());
    let status_width = containers
        .iter()
        .map(|c| status_label(&c.status).len())
        .max()
        .unwrap_or(8)
        .max("STATUS".len());

    let mut out = String::new();
    let _ = writeln!(
        &mut out,
        "{:<width$}  {:<sw$}  ID",
        "RESOURCE",
        "STATUS",
        width = resource_width,
        sw = status_width
    );
    for c in containers {
        let short_id: String = c.id.as_str().chars().take(12).collect();
        let _ = writeln!(
            &mut out,
            "{:<width$}  {:<sw$}  {}",
            c.resource,
            status_label(&c.status),
            short_id,
            width = resource_width,
            sw = status_width
        );
    }
    out
}

fn status_label(status: &ContainerStatus) -> String {
    match status {
        ContainerStatus::Starting => "starting".to_owned(),
        ContainerStatus::Running => "running".to_owned(),
        ContainerStatus::Healthy => "healthy".to_owned(),
        ContainerStatus::Unhealthy => "unhealthy".to_owned(),
        ContainerStatus::Stopped {
            exit_code: Some(code),
        } => format!("stopped({code})"),
        ContainerStatus::Stopped { exit_code: None } => "stopped".to_owned(),
    }
}

/// Write a log chunk to stdout or stderr depending on its origin
/// stream. Falls back to stdout on any error.
pub(crate) fn write_log_chunk(chunk: &LogChunk) {
    let payload: &[u8] = &chunk.bytes;
    let _ = match chunk.stream {
        LogStream::Stdout => std::io::stdout().write_all(payload),
        LogStream::Stderr => std::io::stderr().write_all(payload),
    };
}
