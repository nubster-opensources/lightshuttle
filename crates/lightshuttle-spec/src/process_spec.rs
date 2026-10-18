//! Fully resolved description of a native host process to start.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

/// Self-contained description of a native process to start, derived from a
/// manifest `process` resource.
///
/// All fields are fully resolved: interpolation has run, the working
/// directory has been resolved against the manifest directory, and the
/// environment holds final values.
///
/// The executable named by the first element of `command` is **not** resolved
/// here. Resolution against `PATH` belongs to the runtime, which is the only
/// component that knows the platform it runs on, and therefore the only one
/// that can honour `PATHEXT` on Windows.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    /// Stable identity of the process, of the form `<project>_<resource>`.
    ///
    /// Deliberately not the process number: a number changes at every
    /// restart and is reused by the operating system, so it identifies a
    /// running instance, never the resource. This mirrors how a container
    /// keeps its name across a recreate.
    pub name: String,
    /// Project name as declared in the manifest.
    pub project: String,
    /// Resource name as declared in the manifest.
    pub resource: String,
    /// Program to run followed by its arguments, never passed to a shell.
    pub command: Vec<String>,
    /// Directory the process starts in, already resolved to an absolute path.
    pub working_dir: Option<PathBuf>,
    /// Environment injected on top of what the runtime provides.
    pub env: HashMap<String, String>,
    /// Environment keys the manifest marked sensitive.
    ///
    /// Values stay in `env` for injection. The set exists so that anything
    /// rendering this spec, a diagnostic or an export refusal, can name a key
    /// without printing what it holds.
    pub secret_env_keys: BTreeSet<String>,
    /// Port the process is expected to listen on, when it declares one.
    pub port: Option<u16>,
}

impl ProcessSpec {
    /// Builds a [`ProcessSpec`] for `command`, with no working directory,
    /// environment, sensitive keys or port.
    #[must_use]
    pub fn new(name: String, project: String, resource: String, command: Vec<String>) -> Self {
        Self {
            name,
            project,
            resource,
            command,
            working_dir: None,
            env: HashMap::new(),
            secret_env_keys: BTreeSet::new(),
            port: None,
        }
    }
}
