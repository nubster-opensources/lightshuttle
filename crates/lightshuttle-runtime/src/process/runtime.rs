//! Supervision of the native processes of a project.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use lightshuttle_spec::ProcessSpec;

use crate::error::{Result, RuntimeError};
use crate::process::launcher::{self, ProcessExit, RunningProcess};
use crate::process::registry::{ProcessRecord, ProcessRegistry};
use crate::runtime::{ContainerStatus, LogChunkStream, ResourceId};

/// The native processes this supervisor started, and the registry that
/// outlives it.
///
/// Deliberately not an implementation of [`crate::ResourceRuntime`]. Half of
/// that trait is about a project network, which a process runtime has no
/// business creating, and the other half would then need a stub. Composition
/// belongs one level up, in [`crate::HostRuntime`], which routes by the
/// nature of the spec.
pub struct ProcessRuntime {
    state_root: PathBuf,
    /// Processes started by this supervisor, by the identity they were started
    /// under.
    ///
    /// The registry on disk outlives this map and is what a later `down` from
    /// another terminal reads. This map is what lets `logs` and `inspect`
    /// answer for a process *this* supervisor owns, which the registry cannot:
    /// it holds numbers, not the buffer a process printed into.
    supervised: Mutex<HashMap<String, Supervised>>,
}

/// One process this supervisor started, with the manifest coordinates needed
/// to find its registry entry again.
///
/// The project and resource are kept rather than split back out of the
/// identity. `<project>_<resource>` is not a decodable form: a resource name
/// may itself contain an underscore, so `my_app_web` is both `my_app` plus
/// `web` and `my` plus `app_web`, and picking either would be wrong half the
/// time.
struct Supervised {
    project: String,
    resource: String,
    process: Arc<RunningProcess>,
}

/// A process the registry of a project holds, and whether its number still
/// names the process that was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedProcess {
    /// Resource name as declared in the manifest.
    pub resource: String,
    /// Process number that was recorded.
    pub pid: u32,
    /// Whether the number still names the very process recorded under it.
    ///
    /// `false` means the process ended, or the number was recycled onto
    /// something else. Nothing distinguishes the two from outside, and nothing
    /// needs to: in both cases this record no longer designates anything of
    /// ours, so it is reported as gone and never acted on.
    pub is_live: bool,
}

/// Whether `record` still names the very process it was written for.
///
/// Both sides of the comparison come from the kernel's counter: the recorded
/// value was read back from the process just after it was spawned, and this
/// one is read now. Comparing a stored wall-clock instant against a kernel
/// counter would disagree at one-second resolution about half the time, and a
/// check that is wrong half the time guards nothing.
///
/// This is the single place the question is answered, so `down` and `ps`
/// cannot come to different conclusions about the same record.
fn still_the_recorded_process(record: &ProcessRecord) -> bool {
    launcher::started_at_epoch_seconds(record.pid)
        .is_some_and(|live| live == record.started_at_epoch_seconds)
}

impl ProcessRuntime {
    /// Builds a runtime keeping its registry under `state_root`.
    #[must_use]
    pub fn new(state_root: &Path) -> Self {
        Self {
            state_root: state_root.to_path_buf(),
            supervised: Mutex::new(HashMap::new()),
        }
    }

    /// Directory under which this runtime keeps its per-project registries.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// Starts `spec`, binding it to `bind_address`, and records it in the
    /// project registry.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be started or the registry
    /// cannot be written.
    #[allow(
        clippy::unused_async,
        reason = "mirrors the async shape of `ResourceRuntime`, which `HostRuntime` routes here; \n                  making it synchronous now would have to be undone by lot 2's readiness probe, \n                  and sync to async is a breaking change on a published crate"
    )]
    pub async fn start(&self, spec: &ProcessSpec, bind_address: IpAddr) -> Result<ResourceId> {
        let running = launcher::spawn(spec, bind_address)?;

        // Recorded only when the system reported a start instant. Without one
        // there is nothing a later `down` could safely match against, and a
        // record it cannot verify is a record it must not act on, so writing it
        // would only create something to be ignored.
        if let Some(started_at_epoch_seconds) = running.started_at_epoch_seconds() {
            self.registry(&spec.project).record(ProcessRecord {
                resource: spec.resource.clone(),
                pid: running.pid(),
                started_at_epoch_seconds,
            })?;
        }

        self.supervised
            .lock()
            .expect("supervised mutex poisoned")
            .insert(
                spec.name.clone(),
                Supervised {
                    project: spec.project.clone(),
                    resource: spec.resource.clone(),
                    process: Arc::new(running),
                },
            );

        // The identity is the resource name, never the process number: it
        // survives a restart, exactly as a container name does.
        Ok(ResourceId::new(spec.name.clone()))
    }

    /// Stops the process group of `id`, allowing `grace`, and drops its
    /// record.
    ///
    /// # Errors
    ///
    /// Returns an error when the group cannot be stopped.
    pub async fn stop(&self, id: &ResourceId, grace: Duration) -> Result<()> {
        let running = self
            .supervised
            .lock()
            .expect("supervised mutex poisoned")
            .remove(id.as_str());

        let Some(supervised) = running else {
            // Not ours to stop through this path. A group left behind by a
            // supervisor that has since died is reclaimed through
            // `reclaim_project`, from the registry, which is the only place
            // that knows it exists.
            return Ok(());
        };

        let outcome = match Arc::try_unwrap(supervised.process) {
            Ok(owned) => owned.stop(grace).await,
            // Another caller still holds a handle, which only a live `logs`
            // stream does. Signalling the group by number reaches the same
            // processes without needing sole ownership.
            Err(shared) => launcher::stop_group(shared.pid(), grace).await,
        };

        // The record is dropped whether or not the group went quietly: leaving
        // it would make the next `down` try again on a number that either no
        // longer exists or was never ours.
        self.registry(&supervised.project)
            .forget(&supervised.resource)?;
        outcome
    }

    /// Current status of the process behind `id`.
    ///
    /// # Errors
    ///
    /// Returns an error when the process is unknown to this runtime.
    #[allow(
        clippy::unused_async,
        reason = "mirrors the async shape of `ResourceRuntime`; see `start`"
    )]
    pub async fn inspect(&self, id: &ResourceId) -> Result<ContainerStatus> {
        let process = self.process_of(id)?;
        Ok(match process.exit() {
            // Every exit is a failure for a long-running service, including
            // status zero, so the status carries the code rather than judging
            // it here. A signal has no exit code to report.
            Some(ProcessExit::Code(code)) => ContainerStatus::Stopped {
                exit_code: Some(code),
            },
            Some(ProcessExit::Signal(_)) => ContainerStatus::Stopped { exit_code: None },
            // Running, never Healthy: a process has no health probe in this
            // lot, and reporting health nobody measured would be a claim, not
            // an observation.
            None => ContainerStatus::Running,
        })
    }

    /// Logs of the process behind `id`, replayed and then followed.
    ///
    /// # Errors
    ///
    /// Returns an error when the process is unknown to this runtime.
    #[allow(
        clippy::unused_async,
        reason = "mirrors the async shape of `ResourceRuntime`; see `start`"
    )]
    pub async fn logs(&self, id: &ResourceId, follow: bool) -> Result<LogChunkStream> {
        let process = self.process_of(id)?;

        // Both halves taken under one lock. Replaying and then subscribing as
        // two calls leaves a window in which a chunk is retained after the
        // snapshot and broadcast before the subscription exists, so it reaches
        // neither and disappears.
        let (history, follower) = process.logs().replay_and_subscribe();

        let replay = futures::stream::iter(history.into_iter().map(Ok));
        if !follow {
            return Ok(Box::pin(replay));
        }

        let live = futures::stream::unfold(follower, |mut follower| async move {
            loop {
                match follower.recv().await {
                    Ok(chunk) => return Some((Ok(chunk), follower)),
                    // The follower fell behind the channel and lost chunks. A
                    // supervisor never applies back pressure to the program it
                    // supervises, so the gap is accepted and the stream
                    // continues rather than ending on it.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        });

        Ok(Box::pin(replay.chain(live)))
    }

    /// Reclaims every process recorded for `project`, including those started
    /// by a supervisor that has since died.
    ///
    /// This is what makes `down` work from a terminal other than the one that
    /// ran `up`. A record whose process number no longer matches the start
    /// instant it was recorded with is stale: it is skipped, and the skip is
    /// reported rather than passed over in silence.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry cannot be read or a live group
    /// cannot be stopped.
    pub async fn reclaim_project(&self, project: &str, grace: Duration) -> Result<Vec<String>> {
        let registry = self.registry(project);
        let mut skipped = Vec::new();

        for record in registry.records()? {
            if still_the_recorded_process(&record) {
                launcher::stop_group(record.pid, grace).await?;
            } else {
                // Gone, recycled, or an instant the system would not report:
                // all three mean this record no longer designates the process
                // it was written for. Skipped, and said, never passed over in
                // silence.
                skipped.push(record.resource.clone());
            }
        }

        registry.clear()?;
        self.supervised
            .lock()
            .expect("supervised mutex poisoned")
            .retain(|_, supervised| supervised.project != project);

        Ok(skipped)
    }

    /// Every process recorded for `project`, with whether its number still
    /// names the process it was recorded for.
    ///
    /// Answers for processes this supervisor did not start, which is what a
    /// `ps` run from a second terminal needs: the registry outlives the `up`
    /// that wrote it, while the in-memory map does not.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry cannot be read.
    pub fn recorded_processes(&self, project: &str) -> Result<Vec<RecordedProcess>> {
        Ok(self
            .registry(project)
            .records()?
            .into_iter()
            .map(|record| RecordedProcess {
                is_live: still_the_recorded_process(&record),
                resource: record.resource,
                pid: record.pid,
            })
            .collect())
    }

    /// Whether `id` names a process this supervisor started.
    ///
    /// This is how the composite runtime tells the two worlds apart. It asks
    /// the process supervisor first and treats everything else as a container,
    /// rather than trying to recognise an identifier by its shape: a process
    /// identity is `<project>_<resource>`, and nothing stops a daemon from
    /// handing back a container name that looks the same.
    #[must_use]
    pub fn supervises(&self, id: &ResourceId) -> bool {
        self.supervised
            .lock()
            .expect("supervised mutex poisoned")
            .contains_key(id.as_str())
    }

    /// Registry of `project`, rooted under this runtime's state root.
    fn registry(&self, project: &str) -> ProcessRegistry {
        ProcessRegistry::for_project(&self.state_root, project)
    }

    /// The live process behind `id`, when this supervisor started it.
    fn process_of(&self, id: &ResourceId) -> Result<Arc<RunningProcess>> {
        self.supervised
            .lock()
            .expect("supervised mutex poisoned")
            .get(id.as_str())
            .map(|supervised| Arc::clone(&supervised.process))
            .ok_or_else(|| RuntimeError::ProcessNotSupervised {
                name: id.as_str().to_owned(),
            })
    }
}
