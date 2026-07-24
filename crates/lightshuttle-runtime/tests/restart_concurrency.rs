//! Concurrency tests for per-resource restart serialization.
//!
//! Two restart requests for the same resource must never run at the same
//! time: the loser is rejected with [`LifecycleError::RestartInProgress`]
//! so no freshly created container is torn down by a competing restart.
//! Restarts of distinct resources stay independent.

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{LifecycleError, LifecycleManager, LifecyclePlan};

fn build_plan(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

const STACK: &str = r"
project:
  name: app
resources:
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine
      depends_on: [cache]
";

#[tokio::test]
async fn try_begin_restart_is_exclusive_per_resource() {
    let plan = build_plan(STACK);
    let (manager, _events) = LifecycleManager::new(plan, MockRuntime::new());
    manager.start_all().await.expect("initial start");

    let permit = manager
        .try_begin_restart("cache")
        .expect("first admission succeeds");

    let contended = manager.try_begin_restart("cache");
    assert!(
        matches!(
            &contended,
            Err(LifecycleError::RestartInProgress { resource }) if resource == "cache"
        ),
        "a second admission while one is held must be rejected, got {contended:?}"
    );

    drop(permit);
    manager
        .try_begin_restart("cache")
        .expect("admission succeeds again once the permit is released");
}

#[tokio::test]
async fn different_resources_are_admitted_concurrently() {
    let plan = build_plan(STACK);
    let (manager, _events) = LifecycleManager::new(plan, MockRuntime::new());
    manager.start_all().await.expect("initial start");

    let _cache = manager
        .try_begin_restart("cache")
        .expect("cache admission succeeds");
    manager
        .try_begin_restart("api")
        .expect("api admission is independent of cache");
}

#[tokio::test]
async fn concurrent_restart_one_rejects_the_loser_and_preserves_the_container() {
    let plan = build_plan(STACK);
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);
    manager.start_all().await.expect("initial start");

    let (first, second) = tokio::join!(manager.restart_one("cache"), manager.restart_one("cache"));

    let outcomes = [first, second];
    let winners = outcomes.iter().filter(|r| r.is_ok()).count();
    let losers: Vec<&LifecycleError> = outcomes.iter().filter_map(|r| r.as_ref().err()).collect();

    assert_eq!(winners, 1, "exactly one restart may win, got {outcomes:?}");
    assert_eq!(losers.len(), 1, "exactly one restart must be rejected");
    assert!(
        matches!(losers[0], LifecycleError::RestartInProgress { resource } if resource == "cache"),
        "the loser must be rejected with RestartInProgress, got {:?}",
        losers[0]
    );

    // The winner started `app_cache` exactly once on top of the initial
    // start: a second concurrent restart never created (or removed) a
    // competing container.
    let starts = observer
        .started_resources()
        .into_iter()
        .filter(|name| name == "app_cache")
        .count();
    assert_eq!(
        starts, 2,
        "app_cache must be started once at boot and once by the sole restart winner"
    );
}
