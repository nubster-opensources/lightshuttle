//! Native process resource configuration (a command run on the host).
//!
//! A `process` resource declares a command LightShuttle executes directly on
//! the developer's machine, as a first-class member of the dependency plan.
//! It exists for the parts of a stack that are not containerised: a compiler
//! in watch mode, a language server, a worker being debugged.
//!
//! The command is never handed to a shell. See [`ProcessConfig::command`].

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Configuration of a `process` resource executed natively on the host.
///
/// Corresponds to the `process:` key in a resource entry. The runtime
/// resolves the executable, starts it in its own process group, injects the
/// declared environment on top of what it provides itself, and supervises it
/// for as long as the stack is up.
///
/// A `process` is a long-running service. Any exit, whatever the status code,
/// is a failure: a dependent does not wait for its antecedent to finish, it
/// waits for it to stand up.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessConfig {
    /// Program to run, followed by its arguments.
    ///
    /// Always a list, never a single string, and never interpreted by a
    /// shell: a manifest must read the same way on every machine, and a
    /// shell would make quoting and word splitting depend on the host.
    ///
    /// The first element names the program. The runtime resolves it against
    /// `PATH`, honouring `PATHEXT` on Windows so that `npm` finds `npm.cmd`
    /// without the manifest having to say so.
    ///
    /// An empty list is rejected by [`crate::Manifest::validate`].
    pub command: Vec<String>,

    /// Working directory the process starts in.
    ///
    /// A relative path is resolved against the manifest directory, not
    /// against the directory the command was invoked from, so the same
    /// manifest behaves identically wherever it is run from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Environment variables injected into the process on top of those the
    /// runtime provides itself.
    ///
    /// Values are interpolated: `${env.NAME}` and
    /// `${resources.name.property}` expressions are resolved before the
    /// process starts. A value resolved from another resource is rendered
    /// from the point of view of a host process, which is not the value a
    /// container would receive for the same reference.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,

    /// Port the process is expected to listen on.
    ///
    /// Optional, because a companion that listens on nothing is a legitimate
    /// resource. Omitting it costs one thing only: another resource cannot
    /// reference this one through `${resources.<name>.host}`, `.url` or
    /// `.port`, and such a reference is refused by
    /// [`crate::Manifest::validate`] rather than guessed at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Names of other resources this process must wait for before starting.
    /// Validated by [`crate::Manifest::validate`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

impl ProcessConfig {
    /// Builds a [`ProcessConfig`] running `command`, with no working
    /// directory, environment, port or dependencies.
    ///
    /// Callers set the remaining fields as needed.
    #[must_use]
    pub fn new(command: Vec<String>) -> Self {
        Self {
            command,
            working_dir: None,
            env: IndexMap::new(),
            port: None,
            depends_on: Vec::new(),
        }
    }
}
