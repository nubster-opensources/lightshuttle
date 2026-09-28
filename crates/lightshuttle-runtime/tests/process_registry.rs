//! Behaviour of `ProcessRegistry`: the on-disk record of native processes a
//! supervisor started for a project, so that a later `down` from another
//! terminal can reclaim them.
//!
//! Every test uses a throwaway directory under the system temp directory as
//! the registry root, so nothing here touches the real developer machine
//! state and nothing needs a Docker daemon.

use lightshuttle_runtime::{ProcessRecord, ProcessRegistry};

/// A fresh, empty directory for one test's registry, plus its removal on
/// drop.
struct TempRoot {
    path: std::path::PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "lightshuttle-process-registry-test-{label}-{}-{}",
            std::process::id(),
            SystemTimeNanos::now()
        );
        path.push(unique);
        std::fs::create_dir_all(&path).expect("temp root is created");
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Nanosecond-resolution counter used only to keep concurrently running
/// tests from colliding on the same temp directory name.
struct SystemTimeNanos;

impl SystemTimeNanos {
    fn now() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos()
    }
}

/// A process number the operating system could hand out twice, once to each
/// of two processes started at different moments.
const RECYCLED_PID: u32 = 4242;

fn record(resource: &str, pid: u32, started_at_epoch_seconds: u64) -> ProcessRecord {
    ProcessRecord {
        resource: resource.to_owned(),
        pid,
        started_at_epoch_seconds,
    }
}

#[test]
fn a_recorded_process_reads_back_identically() {
    let root = TempRoot::new("roundtrip");
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    let written = record("app", 4242, 1_726_000_000);
    registry.record(written.clone()).expect("record is written");

    let records = registry.records().expect("records are read back");
    assert_eq!(
        records,
        vec![written],
        "a written record must read back byte-for-byte identical"
    );
}

#[test]
fn forget_removes_one_record_and_keeps_the_rest() {
    let root = TempRoot::new("forget");
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    registry
        .record(record("app", 100, 1_000))
        .expect("app is recorded");
    registry
        .record(record("worker", 200, 2_000))
        .expect("worker is recorded");

    registry.forget("app").expect("app is forgotten");

    let remaining = registry.records().expect("records are read back");
    assert_eq!(
        remaining,
        vec![record("worker", 200, 2_000)],
        "forgetting one resource must leave every other record untouched"
    );
}

#[test]
fn records_on_a_missing_file_returns_an_empty_list() {
    let root = TempRoot::new("missing-file");
    // Nothing was ever recorded: the backing file was never created.
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    let records = registry
        .records()
        .expect("a missing registry file is the ordinary state of a container-only project");
    assert!(
        records.is_empty(),
        "an absent file must read as an empty list, not an error"
    );
}

/// The field this test is about, `started_at_epoch_seconds`, is what stops
/// the registry from being a weapon. Process numbers handed out by the
/// operating system are recycled: the `pid` that named this developer's
/// `dev-server` yesterday can be the very number the kernel hands to a
/// stranger's process this morning. A registry that identified a process by
/// `pid` alone would, on that morning, tell `down` to kill the stranger's
/// process instead of reporting that the recorded one is gone.
///
/// This test pins two records that collide on `pid` (as two processes
/// started at different times legitimately can) but disagree on
/// `started_at_epoch_seconds`, and checks that the registry keeps both as
/// what they are: records of two distinct processes, never folded into one
/// because their process numbers happen to match. An implementation that
/// silently keyed its storage on `pid` alone, ignoring
/// `started_at_epoch_seconds`, would collapse the second `record()` call
/// onto the first and lose `app` entirely, which is exactly what this test
/// must catch.
#[test]
fn a_recycled_process_number_is_not_reclaimed() {
    let root = TempRoot::new("recycled-pid");
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    let app = record("app", RECYCLED_PID, 1_000);
    let worker = record("worker", RECYCLED_PID, 2_000);

    registry.record(app.clone()).expect("app is recorded");
    registry.record(worker.clone()).expect("worker is recorded");

    let mut records = registry.records().expect("records are read back");
    records.sort_by(|a, b| a.resource.cmp(&b.resource));

    assert_eq!(
        records.len(),
        2,
        "two records sharing a recycled pid must both survive, keyed by \
         resource, not collapsed into one because the pid matches"
    );
    assert_eq!(
        records,
        vec![app, worker],
        "each record must keep its own started_at_epoch_seconds: a record \
         that lost track of it would no longer tell these two processes apart"
    );
}

#[test]
fn clear_removes_every_record() {
    let root = TempRoot::new("clear");
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    registry
        .record(record("app", 1, 10))
        .expect("app is recorded");
    registry
        .record(record("worker", 2, 20))
        .expect("worker is recorded");

    registry.clear().expect("clear succeeds");

    let records = registry.records().expect("records are read back");
    assert!(records.is_empty(), "clear must drop every record");
}

#[test]
fn the_registry_lives_under_the_dot_lightshuttle_project_directory() {
    let root = TempRoot::new("path-shape");
    let registry = ProcessRegistry::for_project(&root.path, "myproject");

    let expected = root
        .path
        .join(".lightshuttle")
        .join("myproject")
        .join("processes.json");
    assert_eq!(
        registry.path(),
        expected.as_path(),
        "the registry must sit under `.lightshuttle/<project>/processes.json`, \
         beside the manifest it describes, not somewhere tied to the machine"
    );
}
