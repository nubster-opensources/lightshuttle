//! Native host processes as first-class resources.
//!
//! One concept per module: [`launcher`] starts and stops a process group,
//! [`log_buffer`] keeps what a process printed, [`registry`] remembers across
//! supervisor lifetimes, and [`runtime`] ties the three together for a
//! project.

/// Starting a process group and stopping it whole.
pub(crate) mod launcher;
/// Bounded in-memory retention of a process's output.
pub(crate) mod log_buffer;
/// On-disk record of the processes started for a project.
pub(crate) mod registry;
/// Supervision of the native processes of a project.
pub(crate) mod runtime;

pub use launcher::{BIND_ADDRESS_VARIABLE, ProcessExit, RunningProcess};
pub use log_buffer::ProcessLogBuffer;
pub use registry::{ProcessRecord, ProcessRegistry};
pub use runtime::{ProcessRuntime, RecordedProcess};
