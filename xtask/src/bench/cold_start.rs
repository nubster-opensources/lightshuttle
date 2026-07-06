//! `cargo xtask bench cold-start`: measure the wall-clock time from a fresh
//! `LifecycleManager::start_all()` call to every resource in a Postgres plus
//! one dependent container manifest reporting healthy.
//!
//! ## Methodology
//!
//! Each iteration:
//!
//! 1. Parses the fixture manifest below and rebuilds the [`LifecyclePlan`]
//!    from scratch, so no state leaks between iterations.
//! 2. Reconnects to the Docker daemon (cheap: constructs a local client, no
//!    network round trip) to mirror a fresh `lightshuttle up` invocation.
//! 3. Starts a timer, calls [`LifecycleManager::start_all`], stops the timer
//!    once every resource is healthy.
//! 4. Tears the stack down with [`LifecycleManager::stop_all`] before the
//!    next iteration begins.
//!
//! The reported statistics (min, median, mean, max) are computed over the
//! per-iteration durations. The target is five seconds on a developer-grade
//! laptop; exceeding it prints a warning but does not fail the run, since
//! Docker-backed timings vary with the host and are not a merge gate.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{ContainerRuntime, DockerRuntime, LifecycleManager, LifecyclePlan};
use serde::Serialize;

/// Fixture manifest: a managed Postgres instance plus one dependent
/// container reading its connection URL. Mirrors
/// `examples/02-postgres-and-api/lightshuttle.yml`.
const FIXTURE_MANIFEST: &str = r#"
project:
  name: cold_start_bench

resources:
  db:
    postgres:
      version: "16"

  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo connected to $DATABASE_URL && sleep 3600"]
      depends_on: [db]
      env:
        DATABASE_URL: ${resources.db.url}
"#;

/// Documented cold-start target, in seconds, from issue #167.
const TARGET_SECONDS: f64 = 5.0;

/// Default number of iterations when `--iterations` is not given.
const DEFAULT_ITERATIONS: usize = 5;

/// Default report output path, relative to the workspace root.
const DEFAULT_OUT: &str = "target/bench/cold-start.json";

/// Aggregate statistics computed over a set of per-iteration durations.
///
/// Every field carries a `_seconds` suffix on purpose: it is the report's
/// unit of measure, not incidental repetition.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct ColdStartStats {
    pub(crate) min_seconds: f64,
    pub(crate) median_seconds: f64,
    pub(crate) mean_seconds: f64,
    pub(crate) max_seconds: f64,
}

/// Compute [`ColdStartStats`] over `samples`. Returns all-zero stats when
/// `samples` is empty; callers are expected to guarantee at least one
/// sample before reporting.
pub(crate) fn summarize(samples: &[Duration]) -> ColdStartStats {
    let mut seconds: Vec<f64> = samples.iter().map(Duration::as_secs_f64).collect();
    seconds.sort_by(f64::total_cmp);

    let min_seconds = seconds.first().copied().unwrap_or(0.0);
    let max_seconds = seconds.last().copied().unwrap_or(0.0);
    // `--iterations` is a small, human-chosen sample count: nowhere near the
    // 2^52 threshold where the f64 conversion would lose precision.
    #[allow(clippy::cast_precision_loss)]
    let sample_count = seconds.len() as f64;
    let mean_seconds = if seconds.is_empty() {
        0.0
    } else {
        seconds.iter().sum::<f64>() / sample_count
    };
    let median_seconds = if seconds.is_empty() {
        0.0
    } else if seconds.len() % 2 == 0 {
        let mid = seconds.len() / 2;
        f64::midpoint(seconds[mid - 1], seconds[mid])
    } else {
        seconds[seconds.len() / 2]
    };

    ColdStartStats {
        min_seconds,
        median_seconds,
        mean_seconds,
        max_seconds,
    }
}

/// Run `iterations` cold-start cycles of `manifest_yaml` against runtimes
/// built by `make_runtime`, returning the wall-clock duration of each
/// `start_all` call.
///
/// Generic over [`ContainerRuntime`] so the mechanics (parse, plan, start,
/// stop) can be exercised in tests against
/// [`lightshuttle_runtime::testkit::MockRuntime`] without a Docker daemon.
pub(crate) async fn run_iterations<R, F>(
    manifest_yaml: &str,
    iterations: usize,
    grace: Duration,
    mut make_runtime: F,
) -> Result<Vec<Duration>>
where
    R: ContainerRuntime + 'static,
    F: FnMut() -> Result<R>,
{
    let manifest = Manifest::parse(manifest_yaml).context("fixture manifest failed to parse")?;
    let mut samples = Vec::with_capacity(iterations);

    for iteration in 0..iterations {
        let plan = LifecyclePlan::from_manifest(&manifest)
            .with_context(|| format!("iteration {iteration}: plan build failed"))?;
        let runtime = make_runtime()
            .with_context(|| format!("iteration {iteration}: runtime connect failed"))?;
        let (manager, _events) = LifecycleManager::new(plan, runtime);

        let started = Instant::now();
        manager
            .start_all()
            .await
            .with_context(|| format!("iteration {iteration}: stack failed to start"))?;
        samples.push(started.elapsed());

        manager
            .stop_all(grace)
            .await
            .with_context(|| format!("iteration {iteration}: stack failed to stop cleanly"))?;
    }

    Ok(samples)
}

/// How the benchmark was run, embedded in the report so it is self
/// describing without cross-referencing this source file.
#[derive(Debug, Serialize)]
pub(crate) struct Methodology {
    pub(crate) description: String,
    pub(crate) manifest: String,
    pub(crate) iterations: usize,
    pub(crate) grace_seconds: f64,
}

/// Host details recorded alongside the measurement, since Docker-backed
/// timings are not portable across machines.
#[derive(Debug, Serialize)]
pub(crate) struct Environment {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) lightshuttle_version: String,
}

/// Full JSON report written to `--out`.
#[derive(Debug, Serialize)]
pub(crate) struct ColdStartReport {
    pub(crate) methodology: Methodology,
    pub(crate) environment: Environment,
    pub(crate) samples_seconds: Vec<f64>,
    pub(crate) stats: ColdStartStats,
    pub(crate) target_seconds: f64,
    pub(crate) meets_target: bool,
    pub(crate) recorded_at: String,
}

/// Parsed `--iterations` / `--out` arguments.
struct Args {
    iterations: usize,
    out: PathBuf,
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut iterations = DEFAULT_ITERATIONS;
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut idx = 0;

    while idx < args.len() {
        match args[idx].as_str() {
            "--iterations" => {
                idx += 1;
                let value = args.get(idx).context("--iterations requires a value")?;
                iterations = value.parse().with_context(|| {
                    format!("--iterations value `{value}` is not a positive integer")
                })?;
            }
            "--out" => {
                idx += 1;
                let value = args.get(idx).context("--out requires a path")?;
                out = PathBuf::from(value);
            }
            other => bail!("unknown `bench cold-start` argument: {other}"),
        }
        idx += 1;
    }

    if iterations == 0 {
        bail!("--iterations must be at least 1");
    }

    Ok(Args { iterations, out })
}

/// Entry point for `cargo xtask bench cold-start`.
pub(crate) fn cmd(args: &[String]) -> Result<()> {
    let Args { iterations, out } = parse_args(args)?;
    let grace = Duration::from_secs(5);

    let tokio_runtime =
        tokio::runtime::Runtime::new().context("failed to start the async runtime")?;
    let samples =
        tokio_runtime.block_on(run_iterations(FIXTURE_MANIFEST, iterations, grace, || {
            DockerRuntime::connect().map_err(anyhow::Error::from)
        }))?;

    let stats = summarize(&samples);
    let meets_target = stats.median_seconds <= TARGET_SECONDS;
    let report = ColdStartReport {
        methodology: Methodology {
            description: "Wall-clock time from LifecycleManager::start_all() to every \
                resource reporting healthy, for a Postgres instance plus one dependent \
                container. Each iteration reconnects to the Docker daemon and tears the \
                stack down before the next one starts."
                .to_owned(),
            manifest: FIXTURE_MANIFEST.to_owned(),
            iterations,
            grace_seconds: grace.as_secs_f64(),
        },
        environment: Environment {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            lightshuttle_version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        samples_seconds: samples.iter().map(Duration::as_secs_f64).collect(),
        stats,
        target_seconds: TARGET_SECONDS,
        meets_target,
        recorded_at: jiff::Timestamp::now().to_string(),
    };

    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&report).context("failed to serialise report")?;
    std::fs::write(&out, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", out.display()))?;

    println!("cold-start benchmark: {iterations} iteration(s)");
    for (index, sample) in samples.iter().enumerate() {
        println!("  iteration {index}: {:.3}s", sample.as_secs_f64());
    }
    println!(
        "  min {:.3}s / median {:.3}s / mean {:.3}s / max {:.3}s (target {TARGET_SECONDS:.1}s)",
        stats.min_seconds, stats.median_seconds, stats.mean_seconds, stats.max_seconds,
    );
    if !meets_target {
        println!("  warning: median cold-start exceeds the {TARGET_SECONDS:.1}s target");
    }
    println!("wrote {}", out.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lightshuttle_runtime::testkit::MockRuntime;

    use super::{ColdStartStats, FIXTURE_MANIFEST, run_iterations, summarize};

    #[test]
    fn summarize_computes_min_median_mean_max_for_odd_sample_count() {
        let samples = vec![
            Duration::from_millis(500),
            Duration::from_millis(100),
            Duration::from_millis(300),
        ];

        let stats = summarize(&samples);

        assert!((stats.min_seconds - 0.1).abs() < 1e-9);
        assert!((stats.median_seconds - 0.3).abs() < 1e-9);
        assert!((stats.mean_seconds - 0.3).abs() < 1e-9);
        assert!((stats.max_seconds - 0.5).abs() < 1e-9);
    }

    #[test]
    fn summarize_averages_the_two_middle_samples_for_even_sample_count() {
        let samples = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(300),
            Duration::from_millis(400),
        ];

        let stats = summarize(&samples);

        assert!((stats.median_seconds - 0.25).abs() < 1e-9);
    }

    #[test]
    fn summarize_of_empty_samples_is_all_zero() {
        let stats = summarize(&[]);

        assert_eq!(
            stats,
            ColdStartStats {
                min_seconds: 0.0,
                median_seconds: 0.0,
                mean_seconds: 0.0,
                max_seconds: 0.0,
            }
        );
    }

    #[tokio::test]
    async fn run_iterations_records_one_sample_per_iteration_against_a_mock_runtime() {
        let samples = run_iterations(FIXTURE_MANIFEST, 3, Duration::from_secs(1), || {
            Ok(MockRuntime::new())
        })
        .await
        .expect("mock runtime never fails to start or stop");

        assert_eq!(samples.len(), 3);
    }

    #[tokio::test]
    async fn run_iterations_rejects_an_unparseable_manifest() {
        let result = run_iterations("not: [valid, manifest", 1, Duration::from_secs(1), || {
            Ok(MockRuntime::new())
        })
        .await;

        assert!(result.is_err());
    }
}
