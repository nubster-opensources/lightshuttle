//! `lightshuttle up`.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use lightshuttle_control::{ControlServer, ControlState, Metrics, bind, observe_event_duration};
use lightshuttle_otel::{CollectorConfig, TracerGuard, augment_manifest, is_enabled};
use lightshuttle_runtime::{
    DockerRuntime, LifecycleEvent, LifecycleManager, LifecyclePlan, ManagerHandle,
};
use owo_colors::OwoColorize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::oneshot;
use tracing::{info, warn};

use super::{ExitOutcome, load_manifest};
use crate::control_url;

/// Telemetry handle held for the lifetime of `up`. The `Otel` variant
/// owns a [`TracerGuard`] whose drop flushes pending spans.
enum Telemetry {
    Otel(#[allow(dead_code)] TracerGuard),
    Plain,
}

/// Boot the stack, supervise it, and stop it cleanly on signal.
pub(crate) async fn run(
    file: &Path,
    grace: Duration,
    control_port_override: Option<u16>,
    no_otel: bool,
) -> Result<ExitOutcome> {
    let mut manifest = load_manifest(file)?;
    let collector = CollectorConfig::defaults();
    let otel_on = !no_otel && is_enabled(&manifest);

    // Initialise telemetry first so every subsequent log and span is
    // captured. With OTel on, the tracer guard owns the subscriber and
    // flushes on drop; otherwise a plain fmt subscriber is installed.
    let _telemetry = if otel_on {
        let endpoint = format!("http://127.0.0.1:{}", collector.otlp_grpc_port);
        match lightshuttle_otel::init_orchestrator_tracer(&endpoint, "lightshuttle") {
            Ok(guard) => Telemetry::Otel(guard),
            Err(err) => {
                crate::init_plain_logging();
                warn!(error = %err, "OTel tracer init failed; continuing without self-tracing");
                Telemetry::Plain
            }
        }
    } else {
        crate::init_plain_logging();
        Telemetry::Plain
    };

    if otel_on {
        augment_manifest(&mut manifest, &collector);
        info!("OTel collector enabled; env injected into container resources");
    } else {
        info!(no_otel, "OTel collector disabled");
    }

    let plan = LifecyclePlan::from_manifest(&manifest)?;
    let runtime = DockerRuntime::connect()?;
    let (manager, _events) = LifecycleManager::new(plan, runtime);
    let manager = Arc::new(manager);

    // Drive the lifecycle event broadcast into the metrics histogram:
    // measure how long each resource takes from started to healthy.
    spawn_metrics_pump(&manager);

    let port = control_port_override
        .or_else(|| manifest.dashboard.as_ref().and_then(|d| d.port))
        .unwrap_or(0);
    let listener = bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .context("failed to bind control plane socket")?;
    let local_addr = listener
        .local_addr()
        .context("failed to read control plane local addr")?;
    let url = format!("http://{local_addr}/");

    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    let url_path =
        control_url::write(&cwd, &url).context("failed to write .lightshuttle/control.url")?;

    println!(
        "{} {}",
        "LightShuttle dashboard ready at".green().bold(),
        url.cyan().bold()
    );

    let project = manifest.project.name.clone();
    let handle = ManagerHandle::new(Arc::clone(&manager));
    let metrics = Arc::new(Metrics::install());
    let server = ControlServer::new(ControlState::with_metrics(project.clone(), handle, metrics));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let _ = server
            .serve(listener, async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    info!(project = %project, "stack starting");
    let stack_result = manager.run_until_signal(grace).await;
    info!(project = %project, "stack stopped");

    let _ = shutdown_tx.send(());
    if let Err(join_err) = server_task.await {
        warn!(error = %join_err, "control plane task did not join cleanly");
    }
    if let Err(remove_err) = control_url::remove(&cwd) {
        warn!(
            error = %remove_err,
            path = %url_path.display(),
            "failed to remove discovery file",
        );
    }

    stack_result?;
    Ok(ExitOutcome::Success)
}

/// Spawn a task that consumes the lifecycle event broadcast and
/// observes the started-to-healthy duration of every resource into the
/// Prometheus histogram.
fn spawn_metrics_pump<R>(manager: &Arc<LifecycleManager<R>>)
where
    R: lightshuttle_runtime::ContainerRuntime + 'static,
{
    let mut events = manager.subscribe_events();
    tokio::spawn(async move {
        let mut pending: HashMap<String, Instant> = HashMap::new();
        loop {
            match events.recv().await {
                Ok(LifecycleEvent::ResourceStarted { name, .. }) => {
                    pending.insert(name, Instant::now());
                }
                Ok(LifecycleEvent::ResourceHealthy { name }) => {
                    if let Some(started) = pending.remove(&name) {
                        observe_event_duration(started.elapsed().as_secs_f64());
                    }
                }
                Err(RecvError::Closed) => break,
                Ok(_) | Err(RecvError::Lagged(_)) => {}
            }
        }
    });
}
