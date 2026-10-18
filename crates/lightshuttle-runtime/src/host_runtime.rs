//! The developer's machine as a single runtime: a container daemon and the
//! native processes this supervisor started.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use std::sync::OnceLock;

use lightshuttle_spec::ResourceSpec;

use crate::docker::{DockerRuntime, ManagedContainer};
use crate::error::{Result, RuntimeError};
use crate::process::ProcessRuntime;
use crate::project_sweep::ProjectInventory;
use crate::runtime::{ContainerStatus, LogChunkStream, ResourceId, ResourceRuntime};

/// A whole developer machine: the container daemon plus the native processes
/// started here.
///
/// This is the one type through which containers and processes coexist.
/// [`ResourceRuntime`] returns `impl Future` and is therefore not object
/// safe, so they cannot coexist behind a `dyn`, and the lifecycle manager is
/// monomorphised over a single runtime type. Routing by [`ResourceSpec`]
/// inside one type is what lets a process depend on a container and the
/// reverse, with one topological plan, one dependency-wait and one event
/// channel rather than two of each kept in agreement by hand.
///
/// The daemon connection is established **on first real need**, never at
/// construction. A project made only of `process` resources must start
/// without a container daemon running at all, and a constructor that
/// connected eagerly would make that impossible while still passing every
/// test written on a machine where the daemon happens to be up.
pub struct HostRuntime {
    docker: OnceLock<DockerRuntime>,
    processes: ProcessRuntime,
    state_root: PathBuf,
}

impl HostRuntime {
    /// Builds a runtime for the machine, keeping process state under
    /// `state_root`.
    ///
    /// Does not contact the container daemon. See the type documentation.
    ///
    /// # Errors
    ///
    /// Returns an error when the state root cannot be used.
    pub fn connect(state_root: &Path) -> Result<Self> {
        Ok(Self {
            docker: OnceLock::new(),
            processes: ProcessRuntime::new(state_root),
            state_root: state_root.to_path_buf(),
        })
    }

    /// Directory under which per-project process state is kept.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// The container daemon, connecting on first call.
    ///
    /// # Errors
    ///
    /// Returns an error when no daemon can be reached. That error surfaces
    /// here, at the point where a container is actually needed, and not
    /// before: it is the difference between "this stack needs a daemon" and
    /// "this tool needs a daemon".
    pub fn docker(&self) -> Result<&DockerRuntime> {
        if let Some(existing) = self.docker.get() {
            return Ok(existing);
        }
        let connected = DockerRuntime::connect()?;
        Ok(self.docker.get_or_init(|| connected))
    }

    /// The native process supervisor of this machine.
    #[must_use]
    pub fn processes(&self) -> &ProcessRuntime {
        &self.processes
    }
}

impl ProjectInventory for HostRuntime {
    /// Every container labelled as belonging to `project`.
    ///
    /// Containers only, deliberately, and the name of the trait says why: this
    /// is what [`crate::sweep_project`] repeatedly lists, stops and removes
    /// until nothing is left. Native processes are not swept that way. They
    /// carry no label, the daemon does not know them, and reclaiming them is
    /// one pass over a registry rather than a converging loop, so folding them
    /// in here would make the sweep iterate over something that cannot change
    /// between passes.
    async fn list_managed(&self, project: &str) -> Result<Vec<ManagedContainer>> {
        self.docker()?.list_managed(project).await
    }
}

impl ResourceRuntime for HostRuntime {
    async fn start(&self, spec: &ResourceSpec) -> Result<ResourceId> {
        match spec {
            ResourceSpec::Container(_) => self.docker()?.start(spec).await,
            ResourceSpec::Process(process) => {
                let bind_address = self.process_bind_address(&process.project).await?;
                self.processes.start(process, bind_address).await
            }
        }
    }

    async fn stop(&self, id: &ResourceId, grace: Duration) -> Result<()> {
        if self.processes.supervises(id) {
            return self.processes.stop(id, grace).await;
        }
        self.docker()?.stop(id, grace).await
    }

    async fn remove(&self, name: &str) -> Result<()> {
        // A process leaves nothing behind to remove: no image, no writable
        // layer, no daemon record. Stopping it already dropped its registry
        // entry, so removal is the identity here rather than an error.
        if self.processes.supervises(&ResourceId::new(name)) {
            return Ok(());
        }
        self.docker()?.remove(name).await
    }

    async fn inspect(&self, id: &ResourceId) -> Result<ContainerStatus> {
        if self.processes.supervises(id) {
            return self.processes.inspect(id).await;
        }
        self.docker()?.inspect(id).await
    }

    async fn wait_healthy(&self, id: &ResourceId, timeout: Duration) -> Result<()> {
        if self.processes.supervises(id) {
            // A process carries no health probe in this lot, so there is
            // nothing to wait for. What can still be checked is that it has not
            // already ended: for a long-running service every exit is a
            // failure, status zero included, and declaring a dead process
            // healthy would let its dependents start against nothing.
            return match self.processes.inspect(id).await? {
                ContainerStatus::Stopped { exit_code } => Err(RuntimeError::ProcessStop {
                    pid: 0,
                    reason: match exit_code {
                        Some(code) => {
                            format!("`{id}` exited with status {code} instead of staying up")
                        }
                        None => format!("`{id}` was ended by a signal instead of staying up"),
                    },
                }),
                _ => Ok(()),
            };
        }
        self.docker()?.wait_healthy(id, timeout).await
    }

    async fn logs(&self, id: &ResourceId, follow: bool) -> Result<LogChunkStream> {
        if self.processes.supervises(id) {
            return self.processes.logs(id, follow).await;
        }
        self.docker()?.logs(id, follow).await
    }

    async fn ensure_project_network(&self, project: &str) -> Result<()> {
        // Reaching this means a caller decided the project holds a container,
        // which is the only reason to want a network. The daemon is therefore a
        // real need here, and its absence is a real error.
        self.docker()?.ensure_project_network(project).await
    }

    async fn teardown_project_network(&self, project: &str) -> Result<()> {
        // Only a network this supervisor could have created is torn down. If
        // the daemon was never reached in this process's lifetime, no container
        // was ever started from here and no network was ever created, so there
        // is nothing to remove and failing to connect would turn a `down` on a
        // process-only project into an error about Docker.
        if self.docker.get().is_none() {
            return Ok(());
        }
        self.docker()?.teardown_project_network(project).await
    }

    async fn process_bind_address(&self, project: &str) -> Result<IpAddr> {
        // Asked for every process start, including in a project that holds no
        // container and therefore may run on a machine with no daemon at all.
        // An unreachable daemon means no container is asking, so the loopback
        // is the address that meets the need; it is also the most closed one,
        // which is why `0.0.0.0` was refused in the first place.
        //
        // This cannot silently give the wrong answer for a project that does
        // hold containers: those containers could not have started either, so
        // the run fails on them rather than on a quietly unreachable process.
        match self.docker() {
            Ok(docker) => docker.process_bind_address(project).await,
            Err(_) => Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        }
    }
}
