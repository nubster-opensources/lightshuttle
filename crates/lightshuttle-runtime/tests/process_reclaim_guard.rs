//! The check that keeps the process registry from being a weapon.
//!
//! `down` can run from a terminal other than the one that ran `up`, so it
//! finds the processes to stop by reading a file rather than by asking a
//! daemon. A file holds process numbers, and operating systems recycle those:
//! the number that named this developer's dev server yesterday can be the one
//! the kernel hands to a stranger's process this morning.
//!
//! What separates the two is the start instant, recorded from the system at
//! spawn and read from the system again before anything is stopped. A record
//! whose number is live but whose instant disagrees does not designate the
//! process it was written for, and must be left alone.
//!
//! # Why these tests point the registry at this very test process
//!
//! A test that recorded some other process and checked it survived would only
//! prove that nothing happened, which is also what a runtime that did nothing
//! at all would show. Recording *this* process, under an instant that cannot
//! be right, makes the failure mode unmistakable: if the check is removed, the
//! reclaim kills the process running the assertion, and the suite reports it.
//!
//! Nothing here can harm a bystander. The only number these tests name is
//! their own, and one that no process holds.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use lightshuttle_runtime::{ProcessRecord, ProcessRegistry, ProcessRuntime};
use lightshuttle_spec::ProcessSpec;

const PROJECT: &str = "reclaim-guard";
const GRACE: Duration = Duration::from_millis(50);

/// An instant far enough in the past that no live process can claim it: the
/// second after the epoch.
const IMPOSSIBLE_START_INSTANT: u64 = 1;

#[tokio::test]
async fn a_record_whose_start_instant_disagrees_is_skipped_and_not_stopped() {
    let root = tempfile::tempdir().expect("temp state root is created");
    let registry = ProcessRegistry::for_project(root.path(), PROJECT);

    // This very process, under an instant it cannot have started at.
    registry
        .record(ProcessRecord {
            resource: "impostor".to_owned(),
            pid: std::process::id(),
            started_at_epoch_seconds: IMPOSSIBLE_START_INSTANT,
        })
        .expect("the record is written");

    let runtime = ProcessRuntime::new(root.path());
    let skipped = runtime
        .reclaim_project(PROJECT, GRACE)
        .await
        .expect("reclaiming reads the registry without error");

    // Reaching this line at all is half the proof: without the check, the
    // reclaim above would have stopped the process executing it.
    assert_eq!(
        skipped,
        vec!["impostor".to_owned()],
        "a record whose start instant disagrees with the live process must be \
         reported as skipped, never acted on"
    );
}

#[tokio::test]
async fn a_record_naming_a_number_no_process_holds_is_skipped() {
    let root = tempfile::tempdir().expect("temp state root is created");
    let registry = ProcessRegistry::for_project(root.path(), PROJECT);

    // Above every process number either platform hands out.
    registry
        .record(ProcessRecord {
            resource: "departed".to_owned(),
            pid: u32::MAX - 1,
            started_at_epoch_seconds: IMPOSSIBLE_START_INSTANT,
        })
        .expect("the record is written");

    let runtime = ProcessRuntime::new(root.path());
    let skipped = runtime
        .reclaim_project(PROJECT, GRACE)
        .await
        .expect("reclaiming reads the registry without error");

    assert_eq!(skipped, vec!["departed".to_owned()]);
}

/// Whatever it decided about each record, the reclaim leaves the registry
/// empty. A record kept after a `down` would be tried again by the next one,
/// against a number that is by then even more likely to belong to someone
/// else.
#[tokio::test]
async fn the_registry_is_emptied_even_when_every_record_was_skipped() {
    let root = tempfile::tempdir().expect("temp state root is created");
    let registry = ProcessRegistry::for_project(root.path(), PROJECT);
    registry
        .record(ProcessRecord {
            resource: "impostor".to_owned(),
            pid: std::process::id(),
            started_at_epoch_seconds: IMPOSSIBLE_START_INSTANT,
        })
        .expect("the record is written");

    ProcessRuntime::new(root.path())
        .reclaim_project(PROJECT, GRACE)
        .await
        .expect("reclaiming succeeds");

    assert!(
        registry
            .records()
            .expect("the registry reads back")
            .is_empty(),
        "every record must be dropped once the reclaim has decided about it"
    );
}

/// The positive counterpart of the two tests above, and the one that gives
/// them their meaning.
///
/// A guard that refused everything would pass every assertion so far while
/// making `down` useless: it would report each process as stale and leave them
/// all running. This test starts a real process through the runtime and
/// requires that a later reclaim actually acts on it.
///
/// It is also the only thing that notices if the instant written at spawn stops
/// coming from the operating system. Recording the supervisor's own clock
/// instead compiles, passes every other test, and quietly guarantees that no
/// record ever matches again: the two values come from different clocks that
/// disagree by milliseconds, which is enough to be wrong at one-second
/// resolution about half the time, and on the wrong side of it every time the
/// supervisor's clock rounds up.
#[tokio::test]
async fn a_process_started_here_is_reclaimed_rather_than_skipped() {
    let root = tempfile::tempdir().expect("temp state root is created");
    let runtime = ProcessRuntime::new(root.path());

    let spec = ProcessSpec::new(
        "reclaimed_worker".to_owned(),
        "reclaimed".to_owned(),
        "worker".to_owned(),
        long_running_command(),
    );
    let started = runtime
        .start(&spec, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("the worker starts");

    let pid = ProcessRegistry::for_project(root.path(), "reclaimed")
        .records()
        .expect("the registry reads back")
        .first()
        .map(|record| record.pid)
        .expect("starting a process must record it");

    let skipped = runtime
        .reclaim_project("reclaimed", GRACE)
        .await
        .expect("reclaiming succeeds");

    assert!(
        skipped.is_empty(),
        "a process this runtime started moments ago must be reclaimed, not \
         reported as stale; skipped: {skipped:?}"
    );
    assert!(
        !process_is_alive(pid),
        "the reclaimed process must actually be gone"
    );
    drop(started);
}

/// A command that stays up long enough to be reclaimed, on either platform.
#[cfg(unix)]
fn long_running_command() -> Vec<String> {
    vec!["sh".to_owned(), "-c".to_owned(), "sleep 30".to_owned()]
}

#[cfg(windows)]
fn long_running_command() -> Vec<String> {
    vec![
        "cmd".to_owned(),
        "/C".to_owned(),
        "ping -n 31 127.0.0.1 >nul".to_owned(),
    ]
}

/// Whether the operating system still knows `pid`.
fn process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        output
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}
