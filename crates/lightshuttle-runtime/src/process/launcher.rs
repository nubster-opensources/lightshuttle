//! Starting a native process in its own group, and stopping that whole group.

use std::ffi::{OsStr, OsString};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use process_wrap::tokio::{ChildWrapper, CommandWrap};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use lightshuttle_spec::ProcessSpec;

use crate::error::{Result, RuntimeError};
use crate::process::log_buffer::ProcessLogBuffer;
use crate::runtime::{LogChunk, LogStream};

/// Environment variable through which a process learns the address it must
/// bind to for the containers of its project to reach it.
///
/// The manifest never carries this value. It depends on the daemon and on
/// what the project holds, which the runtime knows and the manifest does not,
/// and a manifest that hard-coded it would be wrong on the next machine.
pub const BIND_ADDRESS_VARIABLE: &str = "LIGHTSHUTTLE_BIND_ADDRESS";

/// How often a stopping process group is checked for having ended, while the
/// grace window runs.
///
/// Unix only: Windows offers a console program no graceful signal to wait on,
/// so there is no window to poll through there.
#[cfg(unix)]
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How a supervised process ended.
///
/// Every variant is a failure. A `process` resource is a long-running
/// service: a dependent does not wait for its antecedent to finish, it waits
/// for it to stand up. An exit with status zero is therefore reported, not
/// swallowed, because for a service it means the thing stopped serving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessExit {
    /// The process returned `code`, whatever its value.
    Code(i32),
    /// The process was ended by a signal, named as the platform reports it.
    Signal(String),
}

impl ProcessExit {
    /// Reads an exit status as this supervisor reports it.
    fn from_status(status: ExitStatus) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(number) = status.signal() {
                return Self::Signal(signal_name(number));
            }
        }
        // A status with neither a code nor a signal cannot happen on the
        // platforms this supports, but the standard library types it as
        // optional, so the case is named rather than unwrapped.
        Self::Code(status.code().unwrap_or(-1))
    }
}

/// Name a Unix signal number is known by, `SIGTERM` rather than `15`.
#[cfg(unix)]
fn signal_name(number: i32) -> String {
    nix::sys::signal::Signal::try_from(number).map_or_else(
        |_| format!("signal {number}"),
        |signal| signal.as_str().to_owned(),
    )
}

/// A process this supervisor started and still owns.
pub struct RunningProcess {
    logs: Arc<ProcessLogBuffer>,
    child: Mutex<Box<dyn ChildWrapper>>,
    pid: u32,
    started_at_epoch_seconds: Option<u64>,
}

impl RunningProcess {
    /// Process number assigned by the operating system.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Start instant as the operating system reports it, in seconds since
    /// the epoch. Recorded in the registry, never the supervisor's clock.
    ///
    /// `None` when the system did not report one, which in practice means the
    /// process had already ended by the time it was asked. Optional rather
    /// than defaulted on purpose: the registry compares this value against
    /// the live process before killing anything, so a made-up number would be
    /// a number that matches something. Nothing is recorded when nothing was
    /// read.
    #[must_use]
    pub fn started_at_epoch_seconds(&self) -> Option<u64> {
        self.started_at_epoch_seconds
    }

    /// Log buffer fed by this process's standard output and error.
    #[must_use]
    pub fn logs(&self) -> &Arc<ProcessLogBuffer> {
        &self.logs
    }

    /// How the process ended, once it has.
    #[must_use]
    pub fn exit(&self) -> Option<ProcessExit> {
        let mut child = self.child.lock().expect("child mutex poisoned");
        match child.try_wait() {
            Ok(Some(status)) => Some(ProcessExit::from_status(status)),
            Ok(None) | Err(_) => None,
        }
    }

    /// Stops the whole process group, allowing `grace` before forcing it.
    ///
    /// # Errors
    ///
    /// Returns an error when the group cannot be signalled or does not end.
    pub async fn stop(self, grace: Duration) -> Result<()> {
        let pid = self.pid;
        let mut child = self
            .child
            .into_inner()
            .expect("child mutex poisoned before stop");

        #[cfg(unix)]
        {
            // Directed at the group, not at the leader: `ProcessGroup` makes
            // this a `killpg`, which is the whole point of wrapping the spawn.
            if let Err(error) = child.signal(nix::sys::signal::Signal::SIGTERM as i32) {
                return Err(RuntimeError::ProcessStop {
                    pid,
                    reason: error.to_string(),
                });
            }
        }

        #[cfg(windows)]
        {
            // Windows offers a console program no equivalent of `SIGTERM`: the
            // graceful half of this sequence has nothing to send. Rather than
            // pretend, the grace window is skipped and the job object is
            // terminated, which ends the whole tree at once. Sending a window
            // close message, which is what `taskkill` without `/F` does, is
            // ignored by exactly the console programs this supervises.
            let _ = grace;
        }

        #[cfg(unix)]
        if tokio::time::timeout(grace, child.wait()).await.is_ok() {
            return Ok(());
        }

        child
            .start_kill()
            .map_err(|error| RuntimeError::ProcessStop {
                pid,
                reason: error.to_string(),
            })?;
        child
            .wait()
            .await
            .map(|_| ())
            .map_err(|error| RuntimeError::ProcessStop {
                pid,
                reason: error.to_string(),
            })
    }
}

/// Starts `spec` in its own process group, binding it to `bind_address`.
///
/// # Errors
///
/// Returns an error when the executable cannot be resolved or the process
/// cannot be spawned.
pub(crate) fn spawn(spec: &ProcessSpec, bind_address: IpAddr) -> Result<RunningProcess> {
    let program = spec
        .command
        .first()
        .ok_or_else(|| RuntimeError::ExecutableNotFound {
            program: String::new(),
        })?;
    let executable = resolve_executable(program, std::env::var_os("PATH").as_deref())?;

    let mut wrap = CommandWrap::with_new(&executable, |command| {
        command.args(&spec.command[1..]);
        // Set before the declared environment, so that a manifest stays able
        // to override it. `ProcessConfig::env` is documented as injected on
        // top of what the runtime provides, and that contract is kept here
        // rather than quietly reversed for one variable.
        command.env(BIND_ADDRESS_VARIABLE, bind_address.to_string());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        if let Some(directory) = spec.working_dir.as_ref() {
            command.current_dir(directory);
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
    });

    #[cfg(unix)]
    wrap.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    wrap.wrap(process_wrap::tokio::JobObject);

    let mut child = wrap.spawn().map_err(|source| RuntimeError::ProcessStart {
        resource: spec.resource.clone(),
        program: program.clone(),
        source,
    })?;

    let pid = child.id().ok_or_else(|| RuntimeError::ProcessStart {
        resource: spec.resource.clone(),
        program: program.clone(),
        source: std::io::Error::other("the operating system reported no process number"),
    })?;

    // Read back from the system, never `SystemTime::now()`. The check made
    // before killing reads this same counter, and two clocks that disagree by
    // milliseconds make an equality at one-second resolution wrong about half
    // the time.
    let started_at_epoch_seconds = started_at_epoch_seconds(pid);

    let logs = Arc::new(ProcessLogBuffer::default());
    if let Some(stdout) = child.stdout().take() {
        tokio::spawn(pump(stdout, LogStream::Stdout, Arc::clone(&logs)));
    }
    if let Some(stderr) = child.stderr().take() {
        tokio::spawn(pump(stderr, LogStream::Stderr, Arc::clone(&logs)));
    }

    Ok(RunningProcess {
        logs,
        child: Mutex::new(child),
        pid,
        started_at_epoch_seconds,
    })
}

/// Forwards every line `reader` produces into `logs`, tagged as `stream`.
///
/// Reads by line rather than by fixed block so the dashboard receives whole
/// lines, matching what the daemon hands back for a container.
async fn pump<R>(reader: R, stream: LogStream, logs: Arc<ProcessLogBuffer>)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        logs.push(LogChunk {
            stream,
            timestamp: std::time::SystemTime::now(),
            bytes: line.into_bytes(),
        });
    }
}

/// Resolves `program` to an executable path, searching `path_env`.
///
/// On Windows the search honours `PATHEXT`, so a manifest saying `npm` finds
/// `npm.cmd` without having to name the extension. That is what lets one
/// manifest read the same way on every machine, which is the same reason
/// `command` is a list and never a shell string.
///
/// # Errors
///
/// Returns an error naming the program when no executable matches.
pub(crate) fn resolve_executable(program: &str, path_env: Option<&OsStr>) -> Result<PathBuf> {
    let not_found = || RuntimeError::ExecutableNotFound {
        program: program.to_owned(),
    };

    // A program naming a path is taken as one, and never searched for: a
    // manifest saying `./scripts/dev` must not silently find a `dev` that
    // happens to sit on the search path.
    let names_a_path = program.contains('/') || (cfg!(windows) && program.contains('\\'));
    if names_a_path {
        return first_existing(Path::new(program)).ok_or_else(not_found);
    }

    let path_env = path_env.ok_or_else(not_found)?;
    for directory in std::env::split_paths(path_env) {
        if let Some(found) = first_existing(&directory.join(program)) {
            return Ok(found);
        }
    }
    Err(not_found())
}

/// The candidate itself when it names an existing file, or the first
/// executable extension of it that does.
///
/// On Unix the extension list is empty, so this is a plain existence check.
fn first_existing(candidate: &Path) -> Option<PathBuf> {
    if candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    for extension in executable_extensions() {
        let mut with_extension = OsString::from(candidate.as_os_str());
        with_extension.push(&extension);
        let candidate = PathBuf::from(with_extension);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Extensions that make a file executable on this platform, `PATHEXT` on
/// Windows and nothing anywhere else.
#[cfg(windows)]
fn executable_extensions() -> Vec<OsString> {
    std::env::var_os("PATHEXT").map_or_else(
        || {
            [".COM", ".EXE", ".BAT", ".CMD"]
                .iter()
                .map(OsString::from)
                .collect()
        },
        |value| {
            std::env::split_paths(&value)
                .map(std::path::PathBuf::into_os_string)
                .collect()
        },
    )
}

/// Extensions that make a file executable on this platform.
#[cfg(not(windows))]
fn executable_extensions() -> Vec<OsString> {
    Vec::new()
}

/// Stops the process group led by `pid`, allowing `grace` before forcing it.
///
/// Always the group, never the single process that was launched. `cargo run`
/// compiles and then executes a child binary; `npm run dev` starts a server
/// as a grandchild. Killing only the launched process leaves the thing that
/// actually holds the port running, which is the failure this resource kind
/// exists to avoid.
///
/// This is the path used to reclaim a group started by a supervisor that has
/// since died, so it works from a process number alone. `process-wrap` cannot
/// help here: it knows the children it spawned, not a number read out of a
/// file written by another terminal.
///
/// # Errors
///
/// Returns an error when the group cannot be signalled or does not end.
pub(crate) async fn stop_group(pid: u32, grace: Duration) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        let group = Pid::from_raw(i32::try_from(pid).map_err(|_| RuntimeError::ProcessStop {
            pid,
            reason: "process number does not fit in a process identifier".to_owned(),
        })?);

        // A process group that does not exist *yet* is indistinguishable from
        // one that does not exist any more, and both are reported as `ESRCH`
        // or, on BSD-derived systems, as `EPERM`.
        //
        // The window is real: `spawn` returns once the fork has happened, but
        // the child joins its own group from inside the child, just before it
        // execs. Signalling in that instant names a group nobody has created,
        // and the difference between the two readings is whether the process
        // number is still live. So the signal is retried while it is, and only
        // then is the group taken to be gone.
        let signal_deadline = tokio::time::Instant::now() + grace;
        loop {
            match killpg(group, Signal::SIGTERM) {
                Ok(()) => break,
                Err(nix::errno::Errno::ESRCH | nix::errno::Errno::EPERM)
                    if started_at_epoch_seconds(pid).is_none() =>
                {
                    // The process itself is gone, so the outcome asked for is
                    // the outcome already observed.
                    return Ok(());
                }
                Err(_) if tokio::time::Instant::now() < signal_deadline => {
                    tokio::time::sleep(STOP_POLL_INTERVAL).await;
                }
                Err(error) => {
                    return Err(RuntimeError::ProcessStop {
                        pid,
                        reason: format!(
                            "{error} while the process number is still live, so its group \
                             could not be reached"
                        ),
                    });
                }
            }
        }

        let deadline = tokio::time::Instant::now() + grace;
        while tokio::time::Instant::now() < deadline {
            if matches!(killpg(group, None), Err(nix::errno::Errno::ESRCH)) {
                return Ok(());
            }
            tokio::time::sleep(STOP_POLL_INTERVAL).await;
        }

        match killpg(group, Signal::SIGKILL) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
            Err(error) => Err(RuntimeError::ProcessStop {
                pid,
                reason: error.to_string(),
            }),
        }
    }

    #[cfg(windows)]
    {
        // One phase, not two. `taskkill` without `/F` asks windows to close,
        // which a console program does not receive, so a graceful attempt here
        // would only spend the grace window achieving nothing. `/T` is what
        // makes this the tree rather than the one process.
        let _ = grace;
        let status = tokio::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|error| RuntimeError::ProcessStop {
                pid,
                reason: error.to_string(),
            })?;

        // A non-zero status is what `taskkill` reports for a number that no
        // longer exists, which is the outcome asked for. Distinguishing that
        // from a refusal would mean parsing localised text, so the process
        // number is checked instead.
        if status.success() || started_at_epoch_seconds(pid).is_none() {
            return Ok(());
        }
        Err(RuntimeError::ProcessStop {
            pid,
            reason: "taskkill refused and the process number is still live".to_owned(),
        })
    }
}

/// Start instant the operating system reports for `pid`, in seconds since the
/// epoch, or `None` when no such process exists.
///
/// Compared against the value stored in the registry before anything is
/// killed. Process numbers are reused, so a record that matched on the number
/// alone would eventually name a stranger.
#[must_use]
pub(crate) fn started_at_epoch_seconds(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    // One process number, and no optional probe: the cost of this call stays
    // proportional to what is being asked, not to how many processes the
    // machine happens to be running.
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system.process(pid).map(sysinfo::Process::start_time)
}
