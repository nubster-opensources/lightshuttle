//! On-disk record of the native processes started for a project.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, RuntimeError};

/// Directory, relative to the manifest, holding per-project supervisor state.
pub(crate) const STATE_DIR_NAME: &str = ".lightshuttle";

/// File holding the process records of one project.
pub(crate) const REGISTRY_FILE_NAME: &str = "processes.json";

/// One process this supervisor started, as recorded on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRecord {
    /// Resource name as declared in the manifest.
    pub resource: String,
    /// Process number assigned by the operating system.
    pub pid: u32,
    /// Start instant **as the operating system reports it**, in seconds since
    /// the epoch, read back from the process itself right after the spawn.
    ///
    /// Never the supervisor's own clock. The check made before killing reads
    /// this same value from the live process, so both sides must come from
    /// the kernel's counter or they would never agree: two clocks disagree by
    /// milliseconds, which is enough to make an equality at one-second
    /// resolution wrong about half the time.
    ///
    /// This field is what stops the registry from being a weapon. Process
    /// numbers are reused; without it, a stale record would name a stranger's
    /// process and `down` would kill it.
    pub started_at_epoch_seconds: u64,
}

/// On-disk record of processes started for a project, so that a later `down`
/// from another terminal can reclaim them.
///
/// A container daemon remembers the containers of a project and hands them
/// back on request, which is how `sweep_project` finds them by label. Nothing
/// plays that role for a native process: this supervisor is mortal, a process
/// carries no label, and under Unix a process group outlives the death of its
/// leader. Persisting nothing would therefore leave a server holding its port
/// after an aborted `up`, and the next `up` would fail against it.
pub struct ProcessRegistry {
    path: PathBuf,
}

impl ProcessRegistry {
    /// Registry of `project`, stored under `root`.
    ///
    /// `root` is the manifest directory, so the state sits beside the
    /// manifest it describes and travels with the project rather than with
    /// the machine. The directory is listed in the `.gitignore` shipped with
    /// this change: it names process numbers of one developer's machine, and
    /// a committed record would designate processes on someone else's.
    #[must_use]
    pub fn for_project(root: &Path, project: &str) -> Self {
        Self {
            path: root
                .join(STATE_DIR_NAME)
                .join(project)
                .join(REGISTRY_FILE_NAME),
        }
    }

    /// Path of the file backing this registry.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Adds or replaces the record of `record.resource`.
    ///
    /// # Errors
    ///
    /// Returns an error when the state directory cannot be created or the
    /// file cannot be written.
    pub fn record(&self, record: ProcessRecord) -> Result<()> {
        let mut records = self.records()?;
        // Keyed by resource, never by process number. Two records can share a
        // number the operating system recycled between two runs; folding them
        // together on that basis would lose one of the two resources.
        records.retain(|held| held.resource != record.resource);
        records.push(record);
        self.write(&records)
    }

    /// Drops the record of `resource`, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be rewritten.
    pub fn forget(&self, resource: &str) -> Result<()> {
        let mut records = self.records()?;
        let before = records.len();
        records.retain(|held| held.resource != resource);
        if records.len() == before {
            return Ok(());
        }
        self.write(&records)
    }

    /// Every record currently held.
    ///
    /// A missing file is not an error: it means no process of this project
    /// was ever recorded, which is the ordinary state of a project made only
    /// of containers.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be read or parsed.
    pub fn records(&self) -> Result<Vec<ProcessRecord>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| self.failure(&source)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(source) => Err(self.failure(&source)),
        }
    }

    /// Drops every record.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be removed.
    pub fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            // Nothing to drop is the same outcome as having dropped
            // everything, and a `down` on a project that never started a
            // process must not fail on it.
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(self.failure(&source)),
        }
    }

    /// Writes `records` as the whole content of the registry, creating the
    /// state directory on the way.
    fn write(&self, records: &[ProcessRecord]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| self.failure(&source))?;
        }
        let serialised =
            serde_json::to_vec_pretty(records).map_err(|source| self.failure(&source))?;
        std::fs::write(&self.path, serialised).map_err(|source| self.failure(&source))
    }

    /// Builds the error reported for any failure touching this registry.
    ///
    /// The path is named because a registry lives beside the manifest, so
    /// which project's state failed is not otherwise obvious.
    fn failure(&self, source: &dyn std::fmt::Display) -> RuntimeError {
        RuntimeError::ProcessRegistry {
            path: self.path.display().to_string(),
            reason: source.to_string(),
        }
    }
}
